use cutix_project::Project;
use cutix_project::model::{RetimeConfig, TimelineElement, Track};
use time::MediaTime;

use crate::animation::{has_channel, scalar_at};
use crate::audio_decode::{AudioCache, PcmBuffer};
use crate::error::{PlaybackError, Result};
use crate::media::MediaResolver;
use crate::retime::{effective_rate_at, source_offset_seconds};
use crate::transitions::{
    ElementEdges, TransitionRole, build_track_transition_edges, resolve_active_transition,
};

pub const VOLUME_DB_MIN: f64 = -60.0;
pub const VOLUME_DB_MAX: f64 = 20.0;
pub fn db_to_linear(db: f64) -> f64 {
    let db = if db.is_finite() {
        db.clamp(VOLUME_DB_MIN, VOLUME_DB_MAX)
    } else {
        0.0
    };
    10f64.powf(db / 20.0)
}

#[derive(Clone, Debug)]
pub struct AudioBuffer {
    pub sample_rate: u32,
    pub channels: usize,
    pub interleaved: Vec<f32>,
}

impl AudioBuffer {
    /// How many frames the buffer holds. A buffer with no channels holds none.
    pub fn frame_count(&self) -> usize {
        self.interleaved
            .len()
            .checked_div(self.channels)
            .unwrap_or(0)
    }

    pub fn sample(&self, frame: usize, channel: usize) -> f32 {
        self.interleaved[frame * self.channels + channel]
    }

    pub fn peak(&self) -> f32 {
        self.interleaved
            .iter()
            .fold(0.0f32, |peak, value| peak.max(value.abs()))
    }
}

pub struct MixRequest<'a> {
    pub project: &'a Project,
    pub scene_id: Option<&'a str>,
    pub start: MediaTime,
    pub duration: MediaTime,
    pub sample_rate: u32,
    pub channels: usize,
}

struct AudibleElement<'a> {
    element: &'a TimelineElement,
    media_id: &'a str,
    volume_db: f64,
    retime: Option<&'a RetimeConfig>,
}

fn audible<'a>(track: &'a Track, element: &'a TimelineElement) -> Option<AudibleElement<'a>> {
    let track_muted = match track {
        Track::Audio { muted, .. } | Track::Video { muted, .. } => *muted,
        _ => return None,
    };
    if track_muted {
        return None;
    }
    match element {
        TimelineElement::Audio(audio) => {
            if audio.muted.unwrap_or(false) {
                return None;
            }
            Some(AudibleElement {
                element,
                media_id: audio.media_id.as_deref()?,
                volume_db: audio.volume,
                retime: audio.retime.as_ref(),
            })
        }
        TimelineElement::Video(video) => {
            if video.muted.unwrap_or(false) || !video.is_source_audio_enabled.unwrap_or(true) {
                return None;
            }
            Some(AudibleElement {
                element,
                media_id: &video.media_id,
                volume_db: video.volume.unwrap_or(0.0),
                retime: video.retime.as_ref(),
            })
        }
        _ => None,
    }
}

/// The timeline span in which an element is heard, in seconds.
///
/// A transition is centred on the cut, so the outgoing clip keeps sounding for half the
/// transition past its own end and the incoming one starts half the transition before
/// its own start, exactly as the picture keeps showing both.
fn audible_span(element: &TimelineElement, edges: Option<&ElementEdges>) -> (f64, f64) {
    let base = element.base();
    let start = base.start_time.to_seconds_f64();
    let end = start + base.duration.to_seconds_f64();
    let head = edges
        .and_then(|edges| edges.incoming.as_ref())
        .map_or(start, |incoming| {
            start.min(incoming.start_time.to_seconds_f64())
        });
    let tail = edges
        .and_then(|edges| edges.outgoing.as_ref())
        .map_or(end, |outgoing| end.max(outgoing.end_time.to_seconds_f64()));
    (head, tail)
}

/// The transition edges as the sound sees them.
///
/// The picture shows an incoming clip from the start of the centred window and simply
/// freezes on its first frame when the clip has no head handle to reach back into.
/// Sound cannot freeze: until the handle begins there is nothing to play, so the
/// fade-in is rebased to start where the first audible sample is. Otherwise a clip with
/// a short handle would jump in part-way up the ramp, which is heard as a click.
fn audio_edges(
    edges: &ElementEdges,
    element: &TimelineElement,
    retime: Option<&RetimeConfig>,
) -> ElementEdges {
    let mut edges = edges.clone();
    if let Some(incoming) = edges.incoming.as_mut() {
        let base = element.base();
        // The handle is measured in source seconds; played at the clip's starting speed
        // it covers a different stretch of the timeline.
        let rate = effective_rate_at(retime, 0.0);
        let head_ticks = if rate > 0.0 {
            (base.trim_start.as_ticks() as f64 / rate).round() as i64
        } else {
            0
        };
        let first_audible = MediaTime::from_ticks(base.start_time.as_ticks() - head_ticks);
        if first_audible > incoming.start_time {
            incoming.start_time = first_audible;
        }
    }
    edges
}

/// The fade a transition applies to this element's sound at `time`.
///
/// Uses the same edges as the picture, so the fade is clamped to the neighbours'
/// lengths, only happens where there is a neighbour to cross into, and runs over the
/// same centred window as the visual transition.
fn transition_gain(edges: Option<&ElementEdges>, time: MediaTime) -> f64 {
    match resolve_active_transition(edges, time) {
        Some(active) => match active.role {
            TransitionRole::Incoming => active.progress,
            TransitionRole::Outgoing => 1.0 - active.progress,
        }
        .clamp(0.0, 1.0),
        None => 1.0,
    }
}

fn sample_at(buffer: &PcmBuffer, channel: usize, source_seconds: f64) -> f32 {
    let data = buffer.channel(channel);
    if data.is_empty() {
        return 0.0;
    }
    let index = source_seconds * buffer.sample_rate as f64;
    if index < 0.0 {
        return 0.0;
    }
    let lower = index.floor() as usize;
    if lower >= data.len() {
        return 0.0;
    }
    let upper = (lower + 1).min(data.len() - 1);
    let fraction = (index - lower as f64) as f32;
    data[lower] * (1.0 - fraction) + data[upper] * fraction
}

/// The value output channel `output` of `outputs` takes from the source at
/// `source_seconds`.
///
/// Clamping the channel index is not a mapping: stereo into 5.1 would copy R into
/// C/LFE/SL/SR, and stereo into mono would drop R entirely.
fn mapped_sample(buffer: &PcmBuffer, output: usize, outputs: usize, source_seconds: f64) -> f32 {
    let sources = buffer.channels.min(buffer.samples.len());
    if sources == 0 {
        return 0.0;
    }
    if sources == outputs {
        return sample_at(buffer, output, source_seconds);
    }
    if sources == 1 {
        return sample_at(buffer, 0, source_seconds);
    }
    if outputs == 1 {
        let sum: f32 = (0..sources)
            .map(|channel| sample_at(buffer, channel, source_seconds))
            .sum();
        return sum / sources as f32;
    }
    if outputs == 2 && sources >= 3 {
        return fold_surround_into_stereo(buffer, output, sources, source_seconds);
    }
    // Both layouts follow the order WAVE and ffmpeg default to (FL FR FC LFE BL BR SL
    // SR), so the channels they have in common sit at the same index. A 5.1 source on
    // a 7.1 device keeps its dialogue and rears; the channels only one side has stay
    // silent rather than guessing at an upmix.
    if output < sources.min(outputs) {
        return sample_at(buffer, output, source_seconds);
    }
    0.0
}

/// -3 dB: the weight a channel gets when it is shared between two outputs or moved
/// from the rear to the front, so it neither dominates nor disappears.
const FOLD_WEIGHT: f32 = std::f32::consts::FRAC_1_SQRT_2;

/// Folds a surround source into one stereo output channel.
///
/// FL/FR keep their own side, the centre (dialogue) goes to both sides, and each rear
/// or side channel goes to its own side, all at -3 dB. The LFE is left out, as the
/// usual decoder downmix does. Dividing by the total weight keeps a full-scale source
/// from clipping. Assumes the 5.1 / 7.1 order: FL FR FC LFE BL BR SL SR.
fn fold_surround_into_stereo(
    buffer: &PcmBuffer,
    output: usize,
    sources: usize,
    source_seconds: f64,
) -> f32 {
    let mut value = sample_at(buffer, output, source_seconds);
    let mut weight = 1.0f32;
    if sources > 2 {
        value += FOLD_WEIGHT * sample_at(buffer, 2, source_seconds);
        weight += FOLD_WEIGHT;
    }
    for pair in [4usize, 6] {
        let channel = pair + output;
        if channel < sources {
            value += FOLD_WEIGHT * sample_at(buffer, channel, source_seconds);
            weight += FOLD_WEIGHT;
        }
    }
    value / weight
}

pub fn mix(
    request: &MixRequest<'_>,
    media: &dyn MediaResolver,
    cache: &mut AudioCache,
) -> Result<(AudioBuffer, Vec<String>)> {
    let scene = match request.scene_id {
        Some(id) => request
            .project
            .scenes
            .iter()
            .find(|scene| scene.id == id)
            .ok_or_else(|| PlaybackError::SceneNotFound(id.to_owned()))?,
        None => request
            .project
            .scenes
            .iter()
            .find(|scene| scene.id == request.project.current_scene_id)
            .or_else(|| request.project.scenes.first())
            .ok_or_else(|| PlaybackError::SceneNotFound("<current>".to_owned()))?,
    };

    let channels = request.channels.max(1);
    let sample_rate = request.sample_rate.max(1);
    let frames = (request.duration.to_seconds_f64() * sample_rate as f64)
        .ceil()
        .max(0.0) as usize;
    let mut interleaved = vec![0.0f32; frames * channels];
    let mut skipped = Vec::new();
    let range_start = request.start.to_seconds_f64();
    let range_end = range_start + request.duration.to_seconds_f64();

    for track in scene.tracks.all() {
        let transition_edges = build_track_transition_edges(track);
        for element in track.elements() {
            let Some(entry) = audible(track, element) else {
                continue;
            };
            let base = entry.element.base();
            let edges = transition_edges
                .get(base.id.as_str())
                .map(|edges| audio_edges(edges, entry.element, entry.retime));
            let edges = edges.as_ref();
            let (element_start, element_end) = audible_span(entry.element, edges);
            if element_end <= range_start || element_start >= range_end {
                continue;
            }
            let Some(path) = media.resolve(entry.media_id) else {
                skipped.push(format!("media:{}", entry.media_id));
                continue;
            };
            let buffer = match cache.load(entry.media_id, &path) {
                Ok(buffer) => buffer.clone(),
                Err(error) => {
                    skipped.push(format!("{}: {error}", entry.media_id));
                    continue;
                }
            };

            let animations = base.animations.as_ref();
            let animated_volume = has_channel(animations, "volume");
            let trim_start = base.trim_start.to_seconds_f64();
            let clip_start = base.start_time.to_seconds_f64();
            let head_rate = effective_rate_at(entry.retime, 0.0);

            for frame in 0..frames {
                let timeline_seconds = range_start + frame as f64 / sample_rate as f64;
                if timeline_seconds < element_start || timeline_seconds >= element_end {
                    continue;
                }
                let clip_seconds = timeline_seconds - clip_start;
                // Inside a transition the incoming clip plays its head handle before its
                // own start, at the speed it starts with, and the outgoing one runs on
                // into its trimmed tail, like the picture. A clip with no handle reads a
                // negative source time there and `sample_at` keeps it silent.
                let source_seconds = if clip_seconds >= 0.0 {
                    trim_start + source_offset_seconds(entry.retime, clip_seconds)
                } else {
                    trim_start + clip_seconds * head_rate
                };
                // Volume keys are authored from the clip's own start, not the handle.
                let local_ticks = clip_seconds.max(0.0) * time::TICKS_PER_SECOND as f64;
                let base_db = if animated_volume {
                    scalar_at(
                        animations,
                        "volume",
                        entry.volume_db,
                        MediaTime::from_ticks(local_ticks.round() as i64),
                    )
                } else {
                    entry.volume_db
                };
                let timeline_time = MediaTime::from_ticks(
                    (timeline_seconds * time::TICKS_PER_SECOND as f64).round() as i64,
                );
                let gain = db_to_linear(base_db) * transition_gain(edges, timeline_time);
                if gain == 0.0 {
                    continue;
                }
                for channel in 0..channels {
                    let value =
                        mapped_sample(&buffer, channel, channels, source_seconds) * gain as f32;
                    interleaved[frame * channels + channel] += value;
                }
            }
        }
    }

    Ok((
        AudioBuffer {
            sample_rate,
            channels,
            interleaved,
        },
        skipped,
    ))
}

#[cfg(test)]
mod tests {
    use super::{PcmBuffer, audible_span, mapped_sample, transition_gain};
    use crate::transitions::build_track_transition_edges;
    use cutix_project::model::Track;
    use serde_json::json;
    use time::{MediaTime, TICKS_PER_SECOND};

    fn at(seconds: f64) -> MediaTime {
        MediaTime::from_ticks((seconds * TICKS_PER_SECOND as f64).round() as i64)
    }

    fn clip(
        id: &str,
        start: f64,
        duration: f64,
        transition_seconds: Option<f64>,
    ) -> serde_json::Value {
        let mut value = json!({
            "type": "video",
            "id": id,
            "name": id,
            "duration": at(duration).as_ticks(),
            "startTime": at(start).as_ticks(),
            "trimStart": 0,
            "trimEnd": 0,
            "mediaId": id,
            "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
            "opacity": 1.0
        });
        if let Some(seconds) = transition_seconds {
            value["transition"] = json!({
                "type": "crossfade",
                "duration": at(seconds).as_ticks(),
                "easing": "linear"
            });
        }
        value
    }

    fn video_track(elements: Vec<serde_json::Value>) -> Track {
        serde_json::from_value(json!({
            "type": "video",
            "id": "main",
            "name": "Main",
            "elements": elements,
            "muted": false,
            "hidden": false
        }))
        .unwrap()
    }

    #[test]
    fn the_audio_crossfade_follows_the_centred_picture_transition() {
        let track = video_track(vec![
            clip("a", 0.0, 2.0, None),
            clip("b", 2.0, 2.0, Some(1.0)),
        ]);
        let edges = build_track_transition_edges(&track);
        let outgoing = edges.get("a");
        let incoming = edges.get("b");

        assert_eq!(transition_gain(outgoing, at(1.4)), 1.0);
        assert!((transition_gain(outgoing, at(2.25)) - 0.25).abs() < 1e-6);
        assert!((transition_gain(incoming, at(2.25)) - 0.75).abs() < 1e-6);
        assert_eq!(transition_gain(incoming, at(2.6)), 1.0);

        let (_, outgoing_end) = audible_span(&track.elements()[0], outgoing);
        assert!((outgoing_end - 2.5).abs() < 1e-9, "{outgoing_end}");
    }

    #[test]
    fn a_transition_without_a_neighbour_does_not_fade_the_sound_in() {
        let track = video_track(vec![clip("alone", 1.0, 2.0, Some(1.0))]);
        let edges = build_track_transition_edges(&track);
        assert_eq!(transition_gain(edges.get("alone"), at(1.1)), 1.0);
    }

    #[test]
    fn an_oversized_transition_is_clamped_to_the_clips_like_the_picture() {
        let track = video_track(vec![
            clip("a", 0.0, 2.0, None),
            clip("b", 2.0, 2.0, Some(10.0)),
        ]);
        let edges = build_track_transition_edges(&track);
        // Clamped to the shorter clip: the window is 1..3 s, not 10 s wide.
        assert!((transition_gain(edges.get("b"), at(2.5)) - 0.75).abs() < 1e-6);
        assert_eq!(transition_gain(edges.get("b"), at(3.2)), 1.0);
    }

    /// One frame per channel, each channel holding its own index + 1 as a constant.
    fn constant_channels(channels: usize) -> PcmBuffer {
        PcmBuffer {
            sample_rate: 1,
            channels,
            samples: (0..channels)
                .map(|channel| vec![(channel + 1) as f32; 2])
                .collect(),
        }
    }

    fn mapped(sources: usize, outputs: usize) -> Vec<f32> {
        let buffer = constant_channels(sources);
        (0..outputs)
            .map(|output| mapped_sample(&buffer, output, outputs, 0.0))
            .collect()
    }

    #[test]
    fn matching_layouts_map_channel_for_channel() {
        assert_eq!(mapped(2, 2), [1.0, 2.0]);
        assert_eq!(mapped(6, 6), [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn a_mono_source_feeds_every_output_channel() {
        assert_eq!(mapped(1, 2), [1.0, 1.0]);
        assert_eq!(mapped(1, 6), [1.0; 6]);
    }

    #[test]
    fn a_mono_output_averages_the_source_instead_of_dropping_the_right_channel() {
        assert_eq!(mapped(2, 1), [1.5]);
    }

    #[test]
    fn stereo_into_surround_keeps_right_out_of_the_centre_and_rears() {
        assert_eq!(mapped(2, 6), [1.0, 2.0, 0.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn surround_into_stereo_folds_the_centre_and_rears_and_drops_the_lfe() {
        let side = std::f32::consts::FRAC_1_SQRT_2;
        let weight = 1.0 + side + side;
        // FL FR FC LFE BL BR = 1 2 3 4 5 6: the centre lands on both sides, each rear on
        // its own side, and the LFE (4) appears nowhere.
        let expected = [
            (1.0 + side * 3.0 + side * 5.0) / weight,
            (2.0 + side * 3.0 + side * 6.0) / weight,
        ];
        let folded = mapped(6, 2);
        for (channel, (measured, wanted)) in folded.iter().zip(expected).enumerate() {
            assert!(
                (measured - wanted).abs() < 1e-5,
                "channel {channel}: {measured} != {wanted}"
            );
        }

        // 7.1 adds SL/SR (7, 8) into their own sides with the same weight.
        let weight = 1.0 + side * 3.0;
        let wide = mapped(8, 2);
        assert!((wide[0] - (1.0 + side * (3.0 + 5.0 + 7.0)) / weight).abs() < 1e-5);
        assert!((wide[1] - (2.0 + side * (3.0 + 6.0 + 8.0)) / weight).abs() < 1e-5);
    }

    #[test]
    fn a_full_scale_surround_source_cannot_clip_when_folded_to_stereo() {
        let buffer = PcmBuffer {
            sample_rate: 1,
            channels: 6,
            samples: vec![vec![1.0f32; 2]; 6],
        };
        for output in 0..2 {
            let value = mapped_sample(&buffer, output, 2, 0.0);
            assert!(value <= 1.0 + 1e-6 && value > 0.9, "{value}");
        }
    }

    #[test]
    fn different_multichannel_layouts_keep_their_common_channels_one_to_one() {
        // 5.1 on a 7.1 device keeps dialogue and rears; the side pair stays silent.
        assert_eq!(mapped(6, 8), [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 0.0, 0.0]);
        // 7.1 into 5.1 drops only the side pair.
        assert_eq!(mapped(8, 6), [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn an_incoming_clip_is_audible_from_the_start_of_the_centred_window() {
        let track = video_track(vec![
            clip("a", 0.0, 2.0, None),
            clip("b", 2.0, 2.0, Some(1.0)),
        ]);
        let edges = build_track_transition_edges(&track);
        let (incoming_start, incoming_end) = audible_span(&track.elements()[1], edges.get("b"));
        assert!((incoming_start - 1.5).abs() < 1e-9, "{incoming_start}");
        assert!((incoming_end - 4.0).abs() < 1e-9, "{incoming_end}");
    }

    #[test]
    fn a_short_head_handle_rebases_the_fade_in_to_the_first_audible_sample() {
        let mut incoming = clip("b", 2.0, 2.0, Some(1.0));
        incoming["trimStart"] = json!(at(0.2).as_ticks());
        let track = video_track(vec![clip("a", 0.0, 2.0, None), incoming]);
        let edges = build_track_transition_edges(&track);
        let element = &track.elements()[1];
        let sound = super::audio_edges(edges.get("b").unwrap(), element, None);
        let rebased = sound.incoming.as_ref().unwrap();
        // 0.2 s of handle: the fade starts at 1.8 s, not at the window's 1.5 s, and
        // still ends with the picture at 2.5 s.
        assert_eq!(rebased.start_time, at(1.8));
        assert_eq!(rebased.end_time, at(2.5));
        assert_eq!(transition_gain(Some(&sound), at(1.8)), 0.0);
        assert_eq!(transition_gain(Some(&sound), at(2.5)), 1.0);

        // A handle longer than half the window changes nothing.
        let mut long = clip("b", 2.0, 2.0, Some(1.0));
        long["trimStart"] = json!(at(3.0).as_ticks());
        let track = video_track(vec![clip("a", 0.0, 2.0, None), long]);
        let edges = build_track_transition_edges(&track);
        let sound = super::audio_edges(edges.get("b").unwrap(), &track.elements()[1], None);
        assert_eq!(sound.incoming.as_ref().unwrap().start_time, at(1.5));
    }

    #[test]
    fn a_buffer_without_channels_is_silent() {
        let buffer = PcmBuffer {
            sample_rate: 1,
            channels: 0,
            samples: Vec::new(),
        };
        assert_eq!(mapped_sample(&buffer, 0, 2, 0.0), 0.0);
    }
}
