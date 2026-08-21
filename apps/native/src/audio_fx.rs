use std::path::{Path, PathBuf};

use audio::silence::{detect_silent_ranges, SilenceOptions, SilentRange};
use audio::{
    denoise, detect_onsets, ducking_envelope, estimate_tempo, DenoiseOptions, DuckingOptions,
    GainPoint, OnsetOptions,
};
use cutix_playback::audio_decode::decode_audio;
use cutix_playback::retime::source_offset_seconds;
use cutix_playback::AudioBuffer;
use cutix_project::model::RetimeConfig;
use cutix_project::TimelineElement;
use dsp::{
    apply_reverb, apply_voice_preset, equalize, graphic_bands, shift_pitch, EqualizerOptions,
    PitchOptions, ReverbOptions, ReverbPreset, VoicePreset,
};
use time::MediaTime;

use crate::ai::Job;

pub const TICKS_PER_SECOND: i64 = 120_000;
pub const MAX_BEAT_MARKERS: usize = 400;
pub const MAX_DUCKING_KEYFRAMES: usize = 400;
pub const MIN_CUT_SECONDS: f64 = 0.05;

pub const REVERB_PRESET_IDS: [&str; 3] = ["room", "hall", "plate"];

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Equalizer { gains_db: Vec<f32> },
    Pitch { semitones: f32, formant: f32 },
    Reverb { preset: String, wet: f32 },
    Voice { preset: String },
    Denoise { strength: f32 },
}

impl Effect {
    /// Only the tests in this file ask this; compiled for them alone so the shipping
    /// binary does not carry a method nothing calls.
    #[cfg(test)]
    pub fn tail_seconds(&self, sample_rate: u32) -> f64 {
        match self {
            Effect::Reverb { preset, wet } => {
                let options = ReverbOptions::from_preset(
                    reverb_preset(preset),
                    sample_rate as f32,
                    wet.clamp(0.0, 1.0),
                );
                dsp::reverb_tail_seconds(&options) as f64
            }
            _ => 0.0,
        }
    }

    pub fn is_neutral(&self) -> bool {
        match self {
            Effect::Equalizer { gains_db } => gains_db.iter().all(|gain| gain.abs() < 1e-3),
            Effect::Pitch { semitones, formant } => semitones.abs() < 1e-3 && formant.abs() < 1e-3,
            Effect::Reverb { wet, .. } => *wet <= 0.0,
            Effect::Voice { .. } => false,
            Effect::Denoise { strength } => *strength <= 0.0,
        }
    }
}

pub fn reverb_preset(id: &str) -> ReverbPreset {
    match id {
        "hall" => ReverbPreset::Hall,
        "plate" => ReverbPreset::Plate,
        _ => ReverbPreset::Room,
    }
}

pub fn apply_effect(channel: &[f32], sample_rate: u32, effect: &Effect) -> Vec<f32> {
    let rate = sample_rate.max(1) as f32;
    match effect {
        Effect::Equalizer { gains_db } => equalize(
            channel,
            &EqualizerOptions {
                sample_rate: rate,
                bands: graphic_bands(gains_db),
            },
        ),
        Effect::Pitch { semitones, formant } => shift_pitch(
            channel,
            &PitchOptions {
                sample_rate: rate,
                semitones: *semitones,
                formant_semitones: *formant,
                robotize: false,
            },
        ),
        Effect::Reverb { preset, wet } => apply_reverb(
            channel,
            &ReverbOptions::from_preset(reverb_preset(preset), rate, wet.clamp(0.0, 1.0)),
        ),
        Effect::Voice { preset } => apply_voice_preset(
            channel,
            VoicePreset::from_id(preset).unwrap_or(VoicePreset::Robot),
            rate,
        ),
        Effect::Denoise { strength } => denoise(
            channel,
            &DenoiseOptions {
                strength: strength.clamp(0.0, 1.0),
                ..DenoiseOptions::default()
            },
        ),
    }
}

pub struct Decoded {
    pub sample_rate: u32,
    pub channels: Vec<Vec<f32>>,
}

impl Decoded {
    pub fn mono(&self) -> Vec<f32> {
        downmix(&self.channels)
    }
}

pub fn downmix(channels: &[Vec<f32>]) -> Vec<f32> {
    if channels.is_empty() {
        return Vec::new();
    }
    if channels.len() == 1 {
        return channels[0].clone();
    }
    let frames = channels.iter().map(Vec::len).min().unwrap_or(0);
    (0..frames)
        .map(|index| {
            channels.iter().map(|channel| channel[index]).sum::<f32>() / channels.len() as f32
        })
        .collect()
}

pub fn decode(path: &Path) -> Result<Decoded, String> {
    let buffer = decode_audio(path).map_err(|error| error.to_string())?;
    if buffer.samples.is_empty() {
        return Err(cutix_i18n::t("audioEnhance.noSource"));
    }
    Ok(Decoded {
        sample_rate: buffer.sample_rate,
        channels: buffer.samples,
    })
}

fn interleave(channels: &[Vec<f32>]) -> Vec<f32> {
    let frames = channels.iter().map(Vec::len).max().unwrap_or(0);
    let mut out = Vec::with_capacity(frames * channels.len());
    for index in 0..frames {
        for channel in channels {
            out.push(channel.get(index).copied().unwrap_or(0.0));
        }
    }
    out
}

pub fn write_wav(path: &Path, channels: &[Vec<f32>], sample_rate: u32) -> Result<(), String> {
    let buffer = AudioBuffer {
        sample_rate,
        channels: channels.len().max(1),
        interleaved: interleave(channels),
    };
    cutix_export::wav::write_wav(path, &buffer)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub fn bake(
    source: &Path,
    effect: &Effect,
    name: &str,
    job: &Job,
) -> Result<(PathBuf, f64), String> {
    let decoded = decode(source)?;
    job.publish(cutix_i18n::t("audioEnhance.processing"), 0.2);

    let mut rendered: Vec<Vec<f32>> = Vec::with_capacity(decoded.channels.len());
    let total = decoded.channels.len().max(1);
    for (index, channel) in decoded.channels.iter().enumerate() {
        if job.is_cancelled() {
            return Err(cutix_i18n::t("cutout.cancel"));
        }
        rendered.push(apply_effect(channel, decoded.sample_rate, effect));
        job.publish(
            cutix_i18n::t("audioEnhance.processing"),
            0.2 + 0.7 * ((index + 1) as f32 / total as f32),
        );
    }

    let before = decoded.channels.first().map(Vec::len).unwrap_or(0);
    let after = rendered.first().map(Vec::len).unwrap_or(0);
    let extra_seconds = (after.saturating_sub(before)) as f64 / decoded.sample_rate.max(1) as f64;

    let directory = std::env::temp_dir().join("cutix-audio");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = directory.join(format!("{}.wav", sanitize(name)));
    write_wav(&path, &rendered, decoded.sample_rate)?;
    job.publish(cutix_i18n::t("audioEnhance.processing"), 1.0);

    Ok((path, extra_seconds))
}

fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '-' || character == ' ' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        format!("audio-{}", uuid::Uuid::new_v4())
    } else {
        format!("{trimmed}-{}", &uuid::Uuid::new_v4().to_string()[..8])
    }
}

#[derive(Clone, Debug)]
pub struct ClipWindow {
    pub start_time: i64,
    pub duration: i64,
    pub trim_start: i64,
    pub retime: Option<RetimeConfig>,
}

pub fn clip_ticks_at_source_ticks(source_ticks: i64, retime: Option<&RetimeConfig>) -> Option<f64> {
    if source_ticks <= 0 {
        return Some(0.0);
    }
    let Some(config) = retime else {
        return Some(source_ticks as f64);
    };
    if config.curve.as_ref().is_none_or(|points| points.is_empty()) {
        let rate = cutix_playback::retime::clamp_rate(config.rate);
        if rate <= 0.0 {
            return None;
        }
        return Some(source_ticks as f64 / rate);
    }

    let target = source_ticks as f64 / TICKS_PER_SECOND as f64;
    let mut low = 0.0f64;
    let mut high = 1.0f64;
    let mut guard = 0;
    while source_offset_seconds(Some(config), high) < target {
        high *= 2.0;
        guard += 1;
        if guard > 64 {
            return None;
        }
    }
    for _ in 0..80 {
        let middle = (low + high) / 2.0;
        if source_offset_seconds(Some(config), middle) < target {
            low = middle;
        } else {
            high = middle;
        }
    }
    let seconds = (low + high) / 2.0;
    seconds
        .is_finite()
        .then_some(seconds * TICKS_PER_SECOND as f64)
}

fn to_clip_ticks(source_seconds: f64, window: &ClipWindow) -> Option<f64> {
    let source_ticks =
        (source_seconds * TICKS_PER_SECOND as f64).round() as i64 - window.trim_start;
    if source_ticks <= 0 {
        return Some(0.0);
    }
    let clip = clip_ticks_at_source_ticks(source_ticks, window.retime.as_ref())?;
    Some(clip.clamp(0.0, window.duration.max(0) as f64))
}

pub fn map_ranges_to_timeline(ranges: &[SilentRange], window: &ClipWindow) -> Vec<(i64, i64)> {
    let mut mapped = Vec::new();
    for range in ranges {
        let (Some(start), Some(end)) = (
            to_clip_ticks(range.start_seconds, window),
            to_clip_ticks(range.end_seconds, window),
        ) else {
            continue;
        };
        if end <= start {
            continue;
        }
        mapped.push((
            window.start_time + start.round() as i64,
            window.start_time + end.round() as i64,
        ));
    }
    mapped
}

pub fn normalize_ranges(ranges: &[(i64, i64)], min_ticks: i64) -> Vec<(i64, i64)> {
    let mut sorted: Vec<(i64, i64)> = ranges
        .iter()
        .copied()
        .filter(|(start, end)| end > start)
        .collect();
    sorted.sort_by_key(|(start, _)| *start);

    let mut merged: Vec<(i64, i64)> = Vec::new();
    for (start, end) in sorted {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
        .into_iter()
        .filter(|(start, end)| end - start >= min_ticks)
        .collect()
}

pub fn remove_silent_ranges(
    tracks: &mut cutix_project::model::SceneTracks,
    track_id: &str,
    element_id: &str,
    ranges: &[(i64, i64)],
) -> bool {
    if ranges.is_empty() {
        return false;
    }
    let Some(track) = std::iter::once(&mut tracks.main)
        .chain(tracks.overlay.iter_mut())
        .chain(tracks.audio.iter_mut())
        .find(|track| track.id() == track_id)
    else {
        return false;
    };
    let elements = track.elements_mut();
    let Some(index) = elements
        .iter()
        .position(|element| element.base().id == element_id)
    else {
        return false;
    };

    let original = elements[index].clone();
    let base = original.base();
    let start = base.start_time.as_ticks();
    let end = start + base.duration.as_ticks();
    let trim_start = base.trim_start.as_ticks();
    let retime = crate::edit::retime_of(&original).cloned();

    let cuts = normalize_ranges(ranges, 1)
        .into_iter()
        .filter_map(|(from, to)| {
            let from = from.max(start);
            let to = to.min(end);
            (to > from).then_some((from, to))
        })
        .collect::<Vec<_>>();
    if cuts.is_empty() {
        return false;
    }

    let mut segments: Vec<(i64, i64)> = Vec::new();
    let mut cursor = start;
    for (from, to) in &cuts {
        if *from > cursor {
            segments.push((cursor, *from));
        }
        cursor = cursor.max(*to);
    }
    if cursor < end {
        segments.push((cursor, end));
    }

    let removed: i64 = cuts.iter().map(|(from, to)| to - from).sum();

    let mut pieces: Vec<TimelineElement> = Vec::new();
    let mut shift = 0i64;
    let mut previous_cut_end = start;
    for (segment_start, segment_end) in &segments {
        shift += segment_start - previous_cut_end;
        previous_cut_end = *segment_end;

        let mut piece = original.clone();
        let source_offset = cutix_playback::retime::source_offset(
            retime.as_ref(),
            MediaTime::from_ticks(segment_start - start),
        );
        let fields = crate::edit::element_base_mut(&mut piece);
        if !pieces.is_empty() {
            fields.id = uuid::Uuid::new_v4().to_string();
        }
        fields.start_time = MediaTime::from_ticks(segment_start - shift);
        fields.duration = MediaTime::from_ticks(segment_end - segment_start);
        fields.trim_start = MediaTime::from_ticks(trim_start + source_offset.as_ticks());
        pieces.push(piece);
    }

    elements.remove(index);
    for element in elements.iter_mut() {
        let fields = crate::edit::element_base_mut(element);
        if fields.start_time.as_ticks() >= end {
            fields.start_time = MediaTime::from_ticks(fields.start_time.as_ticks() - removed);
        }
    }
    for (offset, piece) in pieces.into_iter().enumerate() {
        elements.insert(index + offset, piece);
    }
    elements.sort_by_key(|element| element.base().start_time.as_ticks());
    true
}

pub struct Beats {
    pub times: Vec<f32>,
    pub bpm: Option<f32>,
}

pub fn detect_beats(samples: &[f32], sample_rate: u32, sensitivity: f32) -> Beats {
    let options = OnsetOptions {
        sample_rate,
        sensitivity,
        ..OnsetOptions::default()
    };
    let times = detect_onsets(samples, &options);
    let bpm = estimate_tempo(&times);
    Beats { times, bpm }
}

pub fn beat_marker_ticks(beats: &[f32], window: &ClipWindow) -> Vec<i64> {
    beats
        .iter()
        .map(|seconds| {
            window.start_time + (*seconds as f64 * TICKS_PER_SECOND as f64).round() as i64
                - window.trim_start
        })
        .filter(|time| *time >= window.start_time && *time <= window.start_time + window.duration)
        .take(MAX_BEAT_MARKERS)
        .collect()
}

pub fn ducking_points(voice: &[f32], sample_rate: u32) -> Vec<GainPoint> {
    ducking_envelope(
        voice,
        &DuckingOptions {
            sample_rate,
            ..DuckingOptions::default()
        },
    )
}

pub fn gain_to_decibels(gain: f32) -> f64 {
    if gain <= 0.0001 {
        return -60.0;
    }
    20.0 * (gain as f64).log10()
}

pub fn ducking_keyframes(
    points: &[GainPoint],
    voice: &ClipWindow,
    music: &ClipWindow,
    base_volume: f64,
) -> Vec<(MediaTime, f64)> {
    points
        .iter()
        .map(|point| {
            let timeline = voice.start_time
                + (point.time as f64 * TICKS_PER_SECOND as f64).round() as i64
                - voice.trim_start;
            (timeline - music.start_time, point.gain)
        })
        .filter(|(local, _)| *local >= 0 && *local <= music.duration)
        .take(MAX_DUCKING_KEYFRAMES)
        .map(|(local, gain)| {
            (
                MediaTime::from_ticks(local),
                base_volume + gain_to_decibels(gain),
            )
        })
        .collect()
}

pub fn silence_options(sample_rate: u32, threshold_db: f32, min_seconds: f32) -> SilenceOptions {
    SilenceOptions {
        sample_rate,
        threshold_db,
        min_silence_seconds: min_seconds,
        padding_seconds: audio::DEFAULT_SILENCE_PADDING_SECONDS,
    }
}

pub fn silent_ranges(
    samples: &[f32],
    sample_rate: u32,
    threshold_db: f32,
    min_seconds: f32,
    window: &ClipWindow,
) -> Vec<(i64, i64)> {
    let ranges = detect_silent_ranges(
        samples,
        &silence_options(sample_rate, threshold_db, min_seconds),
    );
    let mapped = map_ranges_to_timeline(&ranges, window);
    normalize_ranges(
        &mapped,
        (MIN_CUT_SECONDS * TICKS_PER_SECOND as f64).round() as i64,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutix_project::model::RetimeSpeedPoint;

    fn ticks(seconds: f64) -> i64 {
        (seconds * TICKS_PER_SECOND as f64).round() as i64
    }

    fn window(start: f64, duration: f64, trim: f64, retime: Option<RetimeConfig>) -> ClipWindow {
        ClipWindow {
            start_time: ticks(start),
            duration: ticks(duration),
            trim_start: ticks(trim),
            retime,
        }
    }

    fn rate(value: f64) -> Option<RetimeConfig> {
        Some(RetimeConfig {
            rate: value,
            maintain_pitch: None,
            curve: None,
            blend_frames: None,
        })
    }

    const RANGE: [SilentRange; 1] = [SilentRange {
        start_seconds: 1.05,
        end_seconds: 1.95,
    }];

    #[test]
    fn maps_source_seconds_straight_through_without_retime() {
        let mapped = map_ranges_to_timeline(&RANGE, &window(10.0, 6.5, 0.0, None));
        assert_eq!(mapped, vec![(ticks(11.05), ticks(11.95))]);
    }

    #[test]
    fn accounts_for_trim_start() {
        let mapped = map_ranges_to_timeline(&RANGE, &window(0.0, 5.0, 1.0, None));
        assert_eq!(mapped, vec![(ticks(0.05), ticks(0.95))]);
    }

    #[test]
    fn halves_the_timeline_positions_at_2x_speed() {
        let mapped = map_ranges_to_timeline(&RANGE, &window(0.0, 3.25, 0.0, rate(2.0)));
        assert_eq!(mapped, vec![(ticks(0.525), ticks(0.975))]);
    }

    #[test]
    fn doubles_the_timeline_positions_at_half_speed() {
        let mapped = map_ranges_to_timeline(&RANGE, &window(0.0, 13.0, 0.0, rate(0.5)));
        assert_eq!(mapped, vec![(ticks(2.1), ticks(3.9))]);
    }

    #[test]
    fn combines_trim_start_with_a_retime_rate() {
        let mapped = map_ranges_to_timeline(&RANGE, &window(4.0, 2.0, 1.0, rate(2.0)));
        assert_eq!(mapped, vec![(ticks(4.025), ticks(4.475))]);
    }

    #[test]
    fn round_trips_through_a_speed_ramp() {
        let retime = Some(RetimeConfig {
            rate: 1.0,
            maintain_pitch: None,
            curve: Some(vec![
                RetimeSpeedPoint {
                    time: 0.0,
                    speed: 1.0,
                },
                RetimeSpeedPoint {
                    time: 8.0,
                    speed: 3.0,
                },
            ]),
            blend_frames: None,
        });
        let clip = window(2.0, 8.0, 0.0, retime.clone());
        let mapped = map_ranges_to_timeline(&RANGE, &clip);
        assert_eq!(mapped.len(), 1);
        let (start, end) = mapped[0];

        let start_source = source_offset_seconds(
            retime.as_ref(),
            (start - ticks(2.0)) as f64 / TICKS_PER_SECOND as f64,
        );
        let end_source = source_offset_seconds(
            retime.as_ref(),
            (end - ticks(2.0)) as f64 / TICKS_PER_SECOND as f64,
        );
        assert!((start_source - 1.05).abs() < 1e-3, "start: {start_source}");
        assert!((end_source - 1.95).abs() < 1e-3, "end: {end_source}");
        assert!(start < end);
        assert!(end <= ticks(10.0));
    }

    #[test]
    fn clamps_to_the_visible_clip_window_and_drops_empty_ranges() {
        let ranges = [
            SilentRange {
                start_seconds: 0.0,
                end_seconds: 0.4,
            },
            SilentRange {
                start_seconds: 0.5,
                end_seconds: 1.6,
            },
            SilentRange {
                start_seconds: 5.0,
                end_seconds: 6.0,
            },
        ];
        let mapped = map_ranges_to_timeline(&ranges, &window(0.0, 1.0, 1.0, rate(2.0)));
        assert_eq!(mapped, vec![(0, ticks(0.3))]);
    }

    #[test]
    fn normalizing_merges_overlaps_and_drops_slivers() {
        let ranges = [
            (ticks(4.0), ticks(5.0)),
            (ticks(0.0), ticks(1.0)),
            (ticks(0.9), ticks(1.5)),
            (ticks(6.0), ticks(6.01)),
        ];
        assert_eq!(
            normalize_ranges(&ranges, ticks(0.05)),
            vec![(ticks(0.0), ticks(1.5)), (ticks(4.0), ticks(5.0))]
        );
    }

    #[test]
    fn beat_markers_land_inside_the_clip_window() {
        let clip = window(10.0, 2.0, 1.0, None);
        let beats = [0.5f32, 1.5, 2.5, 9.0];
        assert_eq!(
            beat_marker_ticks(&beats, &clip),
            vec![ticks(10.5), ticks(11.5)]
        );
    }

    #[test]
    fn gain_to_decibels_matches_the_web_formula() {
        assert!((gain_to_decibels(1.0) - 0.0).abs() < 1e-9);
        assert!((gain_to_decibels(0.5) + 6.0206).abs() < 1e-3);
        assert!((gain_to_decibels(0.0) + 60.0).abs() < 1e-9);
    }

    #[test]
    fn ducking_keyframes_are_local_and_clamped() {
        let voice = window(4.0, 4.0, 0.0, None);
        let music = window(2.0, 6.0, 0.0, None);
        let points = [
            GainPoint {
                time: 0.0,
                gain: 1.0,
            },
            GainPoint {
                time: 1.0,
                gain: 0.25,
            },
            GainPoint {
                time: 20.0,
                gain: 1.0,
            },
        ];
        let keys = ducking_keyframes(&points, &voice, &music, 0.0);
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].0.as_ticks(), ticks(2.0));
        assert!((keys[1].1 + 12.0412).abs() < 1e-3, "{}", keys[1].1);
    }

    #[test]
    fn an_equalizer_band_moves_only_its_own_band() {
        let sample_rate = 48_000u32;
        let tone = |frequency: f32| -> Vec<f32> {
            (0..sample_rate as usize)
                .map(|index| {
                    (std::f32::consts::TAU * frequency * index as f32 / sample_rate as f32).sin()
                        * 0.5
                })
                .collect()
        };
        let mut gains = vec![0.0f32; 5];
        gains[0] = -18.0;
        let effect = Effect::Equalizer { gains_db: gains };

        let low = tone(60.0);
        let high = tone(4_000.0);
        let low_out = apply_effect(&low, sample_rate, &effect);
        let high_out = apply_effect(&high, sample_rate, &effect);

        let energy = |samples: &[f32]| -> f32 {
            samples.iter().map(|value| value * value).sum::<f32>() / samples.len() as f32
        };
        assert!(energy(&low_out) < energy(&low) * 0.2, "low band not cut");
        assert!(
            (energy(&high_out) - energy(&high)).abs() < energy(&high) * 0.1,
            "high band moved"
        );
    }

    #[test]
    fn a_neutral_effect_is_recognised() {
        assert!(Effect::Equalizer {
            gains_db: vec![0.0; 5]
        }
        .is_neutral());
        assert!(!Effect::Equalizer {
            gains_db: vec![3.0, 0.0, 0.0, 0.0, 0.0]
        }
        .is_neutral());
        assert!(Effect::Pitch {
            semitones: 0.0,
            formant: 0.0
        }
        .is_neutral());
        assert!(Effect::Reverb {
            preset: String::from("hall"),
            wet: 0.0
        }
        .is_neutral());
    }

    #[test]
    fn a_hall_reports_a_longer_tail_than_a_room() {
        let hall = Effect::Reverb {
            preset: String::from("hall"),
            wet: 0.4,
        };
        let room = Effect::Reverb {
            preset: String::from("room"),
            wet: 0.4,
        };
        assert!(hall.tail_seconds(48_000) > room.tail_seconds(48_000));
        assert!(
            Effect::Denoise { strength: 0.5 }.tail_seconds(48_000) == 0.0,
            "denoise has no tail"
        );
    }

    #[test]
    fn silence_removal_finds_a_constructed_gap() {
        let sample_rate = 48_000u32;
        let mut samples = vec![0.0f32; sample_rate as usize * 4];
        for (index, sample) in samples.iter_mut().enumerate() {
            let in_gap = index >= sample_rate as usize && index < sample_rate as usize * 2;
            if !in_gap {
                let phase = std::f32::consts::TAU * 440.0 * index as f32 / sample_rate as f32;
                *sample = phase.sin() * 0.5;
            }
        }
        let clip = window(0.0, 4.0, 0.0, None);
        let ranges = silent_ranges(&samples, sample_rate, -40.0, 0.5, &clip);
        assert_eq!(ranges, vec![(ticks(1.05), ticks(1.95))]);
    }

    #[test]
    fn a_click_track_reports_its_tempo() {
        let sample_rate = 48_000u32;
        let interval = (sample_rate as f32 * 60.0 / 120.0) as usize;
        let mut samples = vec![0.0f32; sample_rate as usize * 12];
        let mut position = interval / 2;
        while position < samples.len() {
            for offset in 0..600 {
                if position + offset >= samples.len() {
                    break;
                }
                samples[position + offset] = (1.0 - offset as f32 / 600.0) * 0.9;
            }
            position += interval;
        }
        let beats = detect_beats(&samples, sample_rate, 1.3);
        let bpm = beats.bpm.expect("tempo");
        assert!((bpm - 120.0).abs() < 6.0, "unexpected bpm: {bpm}");
        assert!(beats.times.len() >= 18, "beats: {}", beats.times.len());
    }

    fn audio_clip(id: &str, start: f64, duration: f64) -> TimelineElement {
        let json = serde_json::json!({
            "type": "audio",
            "id": id,
            "name": "clip",
            "sourceType": "upload",
            "mediaId": "media-1",
            "startTime": ticks(start),
            "duration": ticks(duration),
            "trimStart": 0,
            "trimEnd": 0,
            "volume": 0,
        });
        serde_json::from_value(json).expect("audio element")
    }

    fn layout(track: &cutix_project::model::Track) -> Vec<(i64, i64, i64)> {
        track
            .elements()
            .iter()
            .map(|element| {
                let base = element.base();
                (
                    base.start_time.as_ticks(),
                    base.duration.as_ticks(),
                    base.trim_start.as_ticks(),
                )
            })
            .collect()
    }

    fn scene_with_clip() -> cutix_project::model::SceneTracks {
        cutix_project::model::SceneTracks {
            overlay: Vec::new(),
            main: cutix_project::model::Track::Video {
                id: String::from("main"),
                name: String::from("main"),
                elements: Vec::new(),
                muted: false,
                hidden: false,
            },
            audio: vec![cutix_project::model::Track::Audio {
                id: String::from("audio-1"),
                name: String::from("audio"),
                elements: vec![audio_clip("clip-1", 0.0, 6.5)],
                muted: false,
            }],
        }
    }

    #[test]
    fn cuts_the_silent_ranges_out_and_closes_the_gaps() {
        let mut tracks = scene_with_clip();
        assert_eq!(layout(&tracks.audio[0]), vec![(0, ticks(6.5), 0)]);

        let ranges = normalize_ranges(
            &[(ticks(1.05), ticks(1.95)), (ticks(4.05), ticks(5.45))],
            ticks(0.05),
        );
        assert!(remove_silent_ranges(
            &mut tracks,
            "audio-1",
            "clip-1",
            &ranges
        ));

        assert_eq!(
            layout(&tracks.audio[0]),
            vec![
                (0, ticks(1.05), 0),
                (ticks(1.05), ticks(2.1), ticks(1.95)),
                (ticks(3.15), ticks(1.05), ticks(5.45)),
            ]
        );
    }

    #[test]
    fn removal_ripples_later_clips_on_the_same_track() {
        let mut tracks = scene_with_clip();
        tracks.audio[0]
            .elements_mut()
            .push(audio_clip("clip-2", 8.0, 2.0));

        assert!(remove_silent_ranges(
            &mut tracks,
            "audio-1",
            "clip-1",
            &[(ticks(1.0), ticks(2.0))]
        ));

        let rows = layout(&tracks.audio[0]);
        assert_eq!(rows.last().copied(), Some((ticks(7.0), ticks(2.0), 0)));
    }

    #[test]
    fn removal_is_a_no_op_for_an_unknown_element() {
        let mut tracks = scene_with_clip();
        assert!(!remove_silent_ranges(
            &mut tracks,
            "audio-1",
            "missing",
            &[(0, ticks(1.0))]
        ));
    }

    fn fixture() -> Option<PathBuf> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.fixtures/clip-audio.mp3");
        path.is_file().then_some(path)
    }

    #[test]
    fn real_dialogue_decodes_and_survives_every_effect() {
        let Some(path) = fixture() else {
            eprintln!("fixture missing, skipping");
            return;
        };
        let decoded = decode(&path).expect("decode");
        assert!(decoded.sample_rate > 0);
        let mono = decoded.mono();
        assert!(mono.len() > decoded.sample_rate as usize * 40, "too short");

        let head: Vec<f32> = mono[..decoded.sample_rate as usize * 5].to_vec();
        let effects = [
            Effect::Equalizer {
                gains_db: vec![6.0, 0.0, -6.0, 0.0, 3.0],
            },
            Effect::Pitch {
                semitones: 4.0,
                formant: 0.0,
            },
            Effect::Reverb {
                preset: String::from("hall"),
                wet: 0.35,
            },
            Effect::Voice {
                preset: String::from("telephone"),
            },
            Effect::Denoise { strength: 0.7 },
        ];
        for effect in &effects {
            let out = apply_effect(&head, decoded.sample_rate, effect);
            assert!(!out.is_empty(), "{effect:?} produced nothing");
            assert!(
                out.iter().all(|value| value.is_finite()),
                "{effect:?} produced non-finite audio"
            );
            let peak = out.iter().fold(0.0f32, |peak, value| peak.max(value.abs()));
            assert!(peak > 1e-4, "{effect:?} produced silence");
        }
    }

    #[test]
    fn real_dialogue_yields_silence_ranges_and_beats() {
        let Some(path) = fixture() else {
            return;
        };
        let decoded = decode(&path).expect("decode");
        let mono = decoded.mono();
        let seconds = mono.len() as f64 / decoded.sample_rate as f64;
        let clip = window(0.0, seconds, 0.0, None);

        let ranges = silent_ranges(&mono, decoded.sample_rate, -40.0, 0.5, &clip);
        assert!(!ranges.is_empty(), "dialogue should contain pauses");
        for (start, end) in &ranges {
            assert!(end > start);
            assert!(*end <= ticks(seconds) + 1);
        }

        let beats = detect_beats(&mono, decoded.sample_rate, 1.3);
        assert!(!beats.times.is_empty(), "no onsets in real dialogue");

        let points = ducking_points(&mono, decoded.sample_rate);
        assert!(!points.is_empty());
        assert!(points.iter().all(|point| (0.0..=1.0).contains(&point.gain)));
    }

    #[test]
    fn a_baked_wav_round_trips_through_the_decoder() {
        let Some(path) = fixture() else {
            return;
        };
        let job = Job::new(String::new());
        let (baked, extra) = bake(
            &path,
            &Effect::Reverb {
                preset: String::from("hall"),
                wet: 0.3,
            },
            "test-bake",
            &job,
        )
        .expect("bake");
        assert!(extra > 1.0, "hall tail should be seconds long: {extra}");
        let round_trip = decode(&baked).expect("decode baked wav");
        assert_eq!(round_trip.sample_rate, decode(&path).unwrap().sample_rate);
        assert!(round_trip.mono().iter().all(|value| value.is_finite()));
        let _ = std::fs::remove_file(&baked);
    }
}
