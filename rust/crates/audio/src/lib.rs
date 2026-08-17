pub mod denoise;
pub mod silence;

pub use dsp::fft;

pub use denoise::{denoise, estimate_noise_profile, rms, DenoiseOptions};
pub use silence::{
    detect_silent_ranges, SilenceOptions, SilentRange, DEFAULT_MIN_SILENCE_SECONDS,
    DEFAULT_SILENCE_PADDING_SECONDS, DEFAULT_SILENCE_THRESHOLD_DB,
};

pub struct DuckingOptions {
    pub sample_rate: u32,
    pub frame_size: usize,
    pub hop_size: usize,
    pub threshold: f32,
    pub duck_gain: f32,
    pub attack_seconds: f32,
    pub hold_seconds: f32,
    pub release_seconds: f32,
}

impl Default for DuckingOptions {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            frame_size: 1024,
            hop_size: 512,
            threshold: 0.02,
            duck_gain: 0.25,
            attack_seconds: 0.08,
            hold_seconds: 0.25,
            release_seconds: 0.4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GainPoint {
    pub time: f32,
    pub gain: f32,
}

pub fn voice_activity(samples: &[f32], options: &DuckingOptions) -> Vec<bool> {
    frame_energies(samples, options.frame_size, options.hop_size)
        .into_iter()
        .map(|energy| energy >= options.threshold)
        .collect()
}

pub fn ducking_envelope(voice: &[f32], options: &DuckingOptions) -> Vec<GainPoint> {
    let active = voice_activity(voice, options);
    if active.is_empty() {
        return Vec::new();
    }

    let seconds_per_frame = options.hop_size as f32 / options.sample_rate as f32;
    let hold_frames = (options.hold_seconds / seconds_per_frame).ceil() as usize;
    let duck_gain = options.duck_gain.clamp(0.0, 1.0);

    let mut held = vec![false; active.len()];
    let mut remaining = 0usize;
    for index in 0..active.len() {
        if active[index] {
            remaining = hold_frames;
            held[index] = true;
            continue;
        }
        if remaining > 0 {
            remaining -= 1;
            held[index] = true;
        }
    }

    let mut points: Vec<GainPoint> = Vec::new();
    let mut push = |time: f32, gain: f32, points: &mut Vec<GainPoint>| {
        if let Some(last) = points.last() {
            if (last.time - time).abs() < 1e-6 && (last.gain - gain).abs() < 1e-6 {
                return;
            }
        }
        points.push(GainPoint {
            time: time.max(0.0),
            gain,
        });
    };

    push(0.0, if held[0] { duck_gain } else { 1.0 }, &mut points);

    for index in 1..held.len() {
        if held[index] == held[index - 1] {
            continue;
        }
        let boundary = index as f32 * seconds_per_frame;
        if held[index] {
            push(
                (boundary - options.attack_seconds).max(0.0),
                1.0,
                &mut points,
            );
            push(boundary, duck_gain, &mut points);
        } else {
            push(boundary, duck_gain, &mut points);
            push(boundary + options.release_seconds, 1.0, &mut points);
        }
    }

    points
}

pub fn gain_at(points: &[GainPoint], time: f32) -> f32 {
    if points.is_empty() {
        return 1.0;
    }
    if time <= points[0].time {
        return points[0].gain;
    }
    for pair in points.windows(2) {
        let (left, right) = (pair[0], pair[1]);
        if time >= left.time && time <= right.time {
            let span = right.time - left.time;
            if span <= 0.0 {
                return right.gain;
            }
            let ratio = (time - left.time) / span;
            return left.gain + (right.gain - left.gain) * ratio;
        }
    }
    points[points.len() - 1].gain
}

pub struct OnsetOptions {
    pub sample_rate: u32,
    pub frame_size: usize,
    pub hop_size: usize,
    pub sensitivity: f32,
    pub min_interval_seconds: f32,
}

impl Default for OnsetOptions {
    fn default() -> Self {
        Self {
            sample_rate: 48_000,
            frame_size: 1024,
            hop_size: 512,
            sensitivity: 1.4,
            min_interval_seconds: 0.12,
        }
    }
}

pub fn frame_energies(samples: &[f32], frame_size: usize, hop_size: usize) -> Vec<f32> {
    if samples.is_empty() || frame_size == 0 || hop_size == 0 {
        return Vec::new();
    }

    let mut energies = Vec::new();
    let mut start = 0;
    while start < samples.len() {
        let end = (start + frame_size).min(samples.len());
        let frame = &samples[start..end];
        let sum: f32 = frame.iter().map(|value| value * value).sum();
        energies.push((sum / frame.len() as f32).sqrt());
        start += hop_size;
    }
    energies
}

pub fn positive_flux(energies: &[f32]) -> Vec<f32> {
    energies
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).max(0.0))
        .collect()
}

pub fn detect_onsets(samples: &[f32], options: &OnsetOptions) -> Vec<f32> {
    let energies = frame_energies(samples, options.frame_size, options.hop_size);
    if energies.len() < 3 {
        return Vec::new();
    }

    let flux = positive_flux(&energies);
    let window = 16.min(flux.len());
    let seconds_per_frame = options.hop_size as f32 / options.sample_rate as f32;
    let min_frames = (options.min_interval_seconds / seconds_per_frame).ceil() as usize;

    let mut onsets = Vec::new();
    let mut last_index: Option<usize> = None;

    for index in 1..flux.len().saturating_sub(1) {
        let start = index.saturating_sub(window);
        let end = (index + window).min(flux.len());
        let neighbourhood = &flux[start..end];
        let mean = neighbourhood.iter().sum::<f32>() / neighbourhood.len() as f32;
        let threshold = mean * options.sensitivity;

        if flux[index] <= threshold || flux[index] <= f32::EPSILON {
            continue;
        }
        if flux[index] < flux[index - 1] || flux[index] < flux[index + 1] {
            continue;
        }
        if let Some(previous) = last_index {
            if index - previous < min_frames {
                continue;
            }
        }

        onsets.push((index + 1) as f32 * seconds_per_frame);
        last_index = Some(index);
    }

    onsets
}

pub fn estimate_tempo(onsets: &[f32]) -> Option<f32> {
    if onsets.len() < 3 {
        return None;
    }

    let mut intervals: Vec<f32> = onsets
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .filter(|interval| *interval > 0.2 && *interval < 2.0)
        .collect();

    if intervals.is_empty() {
        return None;
    }

    intervals.sort_by(|left, right| left.partial_cmp(right).unwrap());
    let median = intervals[intervals.len() / 2];
    if median <= 0.0 {
        return None;
    }

    let mut bpm = 60.0 / median;
    while bpm < 70.0 {
        bpm *= 2.0;
    }
    while bpm > 180.0 {
        bpm /= 2.0;
    }
    Some(bpm)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click_track(sample_rate: u32, beats_per_minute: f32, seconds: f32) -> Vec<f32> {
        let total = (sample_rate as f32 * seconds) as usize;
        let interval = (sample_rate as f32 * 60.0 / beats_per_minute) as usize;
        let mut samples = vec![0.0; total];
        let mut position = interval / 2;
        while position < total {
            for offset in 0..600 {
                if position + offset >= total {
                    break;
                }
                let decay = 1.0 - offset as f32 / 600.0;
                samples[position + offset] = decay * 0.9;
            }
            position += interval;
        }
        samples
    }

    fn speech_burst(sample_rate: u32, seconds: f32, start: f32, end: f32) -> Vec<f32> {
        let total = (sample_rate as f32 * seconds) as usize;
        let mut samples = vec![0.0; total];
        let from = (sample_rate as f32 * start) as usize;
        let to = ((sample_rate as f32 * end) as usize).min(total);
        for index in from..to {
            let phase = index as f32 / sample_rate as f32 * 220.0 * std::f32::consts::TAU;
            samples[index] = phase.sin() * 0.5;
        }
        samples
    }

    #[test]
    fn silence_keeps_full_gain() {
        let voice = vec![0.0; 48_000];
        let points = ducking_envelope(&voice, &DuckingOptions::default());
        for point in &points {
            assert_eq!(point.gain, 1.0);
        }
        assert_eq!(gain_at(&points, 0.5), 1.0);
    }

    #[test]
    fn ducks_while_voice_is_present() {
        let voice = speech_burst(48_000, 6.0, 2.0, 4.0);
        let points = ducking_envelope(&voice, &DuckingOptions::default());
        assert!(gain_at(&points, 0.5) > 0.9, "should be open before speech");
        assert!(gain_at(&points, 3.0) < 0.3, "should duck during speech");
        assert!(gain_at(&points, 5.5) > 0.9, "should recover after speech");
    }

    #[test]
    fn gain_never_leaves_valid_range() {
        let voice = speech_burst(48_000, 6.0, 1.0, 3.0);
        let points = ducking_envelope(&voice, &DuckingOptions::default());
        for point in &points {
            assert!(
                (0.0..=1.0).contains(&point.gain),
                "bad gain: {}",
                point.gain
            );
        }
    }

    #[test]
    fn hold_prevents_pumping_between_words() {
        let mut voice = speech_burst(48_000, 6.0, 1.0, 1.4);
        let second = speech_burst(48_000, 6.0, 1.5, 2.0);
        for (index, value) in second.iter().enumerate() {
            voice[index] += value;
        }
        let points = ducking_envelope(&voice, &DuckingOptions::default());
        assert!(
            gain_at(&points, 1.45) < 0.3,
            "gap shorter than hold must stay ducked"
        );
    }

    #[test]
    fn envelope_times_are_sorted() {
        let voice = speech_burst(48_000, 8.0, 2.0, 3.0);
        let points = ducking_envelope(&voice, &DuckingOptions::default());
        for pair in points.windows(2) {
            assert!(pair[1].time >= pair[0].time, "unsorted: {pair:?}");
        }
    }

    #[test]
    fn silence_has_no_onsets() {
        let samples = vec![0.0; 48_000];
        assert!(detect_onsets(&samples, &OnsetOptions::default()).is_empty());
    }

    #[test]
    fn finds_onsets_in_click_track() {
        let samples = click_track(48_000, 120.0, 8.0);
        let onsets = detect_onsets(&samples, &OnsetOptions::default());
        assert!(
            onsets.len() >= 12 && onsets.len() <= 20,
            "unexpected onset count: {}",
            onsets.len()
        );
    }

    #[test]
    fn estimates_tempo_of_click_track() {
        let samples = click_track(48_000, 120.0, 12.0);
        let onsets = detect_onsets(&samples, &OnsetOptions::default());
        let bpm = estimate_tempo(&onsets).expect("tempo");
        assert!((bpm - 120.0).abs() < 6.0, "unexpected bpm: {bpm}");
    }

    #[test]
    fn respects_minimum_interval() {
        let samples = click_track(48_000, 120.0, 8.0);
        let options = OnsetOptions {
            min_interval_seconds: 1.0,
            ..OnsetOptions::default()
        };
        let onsets = detect_onsets(&samples, &options);
        for pair in onsets.windows(2) {
            assert!(pair[1] - pair[0] >= 0.99, "onsets too close: {pair:?}");
        }
    }

    #[test]
    fn tempo_needs_enough_onsets() {
        assert!(estimate_tempo(&[0.5, 1.0]).is_none());
    }
}
