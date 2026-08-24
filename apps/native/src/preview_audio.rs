use std::path::{Path, PathBuf};

use cutix_playback::audio_decode::PcmBuffer;
use cutix_playback::output::AudioOutput;

pub const CHUNK_SECONDS: f64 = 0.35;

const MAX_CHUNKS_PER_FEED: usize = 8;

const PITCH_TOLERANCE: f64 = 0.01;

pub const QUEUE_CEILING_SECONDS: f64 = 1.5;

pub fn take_interleaved(
    pcm: &PcmBuffer,
    start_frame: f64,
    out_frames: usize,
    out_channels: usize,
    out_rate: u32,
) -> Vec<f32> {
    let out_channels = out_channels.max(1);
    let source_frames = pcm.frame_count();
    if source_frames == 0 || out_frames == 0 || pcm.channels == 0 {
        return Vec::new();
    }

    let start_frame = if start_frame.is_finite() {
        start_frame.max(0.0)
    } else {
        0.0
    };
    let ratio = pcm.sample_rate.max(1) as f64 / out_rate.max(1) as f64;
    let mut out = Vec::with_capacity(out_frames * out_channels);

    for frame in 0..out_frames {
        let position = start_frame + frame as f64 * ratio;
        let index = position.floor() as usize;
        if index >= source_frames {
            break;
        }
        let blend = (position - index as f64) as f32;
        for channel in 0..out_channels {
            let lane = pcm.channel(channel.min(pcm.channels - 1));
            let here = lane[index];
            let next = if index + 1 < source_frames {
                lane[index + 1]
            } else {
                here
            };
            out.push(here + (next - here) * blend);
        }
    }
    out
}

const STRETCH_WINDOW: usize = 1_024;

const STRETCH_HOP: usize = STRETCH_WINDOW / 4;

const STRETCH_SEARCH: usize = 128;

const STRETCH_CORRELATION: usize = 192;

/// The lookahead the stretcher needs beyond the chunk itself, in seconds.
///
/// Zero at a speed the stretcher does not act on, because there it hands the chunk back
/// untouched and has no use for the extra frames.
fn stretch_lookahead(speed: f64, rate: u32) -> f64 {
    if (speed - 1.0).abs() < PITCH_TOLERANCE {
        0.0
    } else {
        (STRETCH_WINDOW + STRETCH_SEARCH) as f64 / rate.max(1) as f64
    }
}

/// How many source frames one feed chunk covers, lookahead included.
fn chunk_frames(speed: f64, rate: u32) -> usize {
    (CHUNK_SECONDS * rate.max(1) as f64 * speed) as usize
        + (stretch_lookahead(speed, rate) * rate.max(1) as f64) as usize
}

/// How far ahead of the playhead `queued_through` is allowed to sit before the queue is
/// treated as belonging to some other position.
///
/// This has to cover a queue filled to [`QUEUE_CEILING_SECONDS`] plus the whole chunk that
/// crossed the ceiling, lookahead and all. A bound that does not reach that far reads a
/// merely full queue as a discontinuity, and `feed` answers a discontinuity by flushing the
/// output and refilling it — every call, which is heard as a buzz rather than as playback.
fn queue_reach(speed: f64, rate: u32) -> f64 {
    (QUEUE_CEILING_SECONDS + CHUNK_SECONDS) * speed + stretch_lookahead(speed, rate)
}

fn hann(size: usize) -> Vec<f32> {
    (0..size)
        .map(|index| {
            let phase = std::f32::consts::TAU * index as f32 / size as f32;
            0.5 - 0.5 * phase.cos()
        })
        .collect()
}

fn best_offset(lane: &[f32], base: usize, against: &[f32]) -> usize {
    let reach = STRETCH_CORRELATION.min(against.len());
    if reach == 0 || base + STRETCH_SEARCH + reach >= lane.len() {
        return 0;
    }
    let mut best = 0usize;
    let mut score = f32::MIN;
    for offset in 0..=STRETCH_SEARCH {
        let mut sum = 0.0f32;
        for index in 0..reach {
            sum += lane[base + offset + index] * against[index];
        }
        if sum > score {
            score = sum;
            best = offset;
        }
    }
    best
}

pub struct Stretcher {
    window: Vec<f32>,
    tails: Vec<Vec<f32>>,
}

impl Default for Stretcher {
    fn default() -> Self {
        Self {
            window: hann(STRETCH_WINDOW),
            tails: Vec::new(),
        }
    }
}

impl Stretcher {
    pub fn reset(&mut self) {
        self.tails.clear();
    }

    pub fn compress(
        &mut self,
        interleaved: &[f32],
        channels: usize,
        speed: f64,
    ) -> (Vec<f32>, usize) {
        let channels = channels.max(1);
        let frames = interleaved.len() / channels;
        if frames == 0 {
            return (Vec::new(), 0);
        }
        if (speed - 1.0).abs() < PITCH_TOLERANCE {
            self.reset();
            return (interleaved.to_vec(), frames);
        }

        let overlap = STRETCH_WINDOW - STRETCH_HOP;
        let analysis_hop = ((STRETCH_HOP as f64) * speed).round().max(1.0) as usize;
        let usable = frames.saturating_sub(STRETCH_WINDOW + STRETCH_SEARCH);
        let windows = usable / analysis_hop;
        if windows == 0 {
            return (Vec::new(), 0);
        }
        if self.tails.len() != channels {
            self.tails = vec![vec![0.0; overlap]; channels];
        }

        let produced = windows * STRETCH_HOP;
        let mut lanes = vec![0.0f32; frames];
        let mut mixed = vec![0.0f32; produced * channels];

        for channel in 0..channels {
            for (frame, slot) in lanes.iter_mut().enumerate() {
                *slot = interleaved[frame * channels + channel];
            }

            let mut canvas = vec![0.0f32; produced + overlap];
            canvas[..overlap].copy_from_slice(&self.tails[channel]);

            let mut analysis = 0usize;
            let mut synthesis = 0usize;
            for _ in 0..windows {
                let offset = best_offset(&lanes, analysis, &canvas[synthesis..]);
                let from = analysis + offset;
                for index in 0..STRETCH_WINDOW {
                    let sample = lanes.get(from + index).copied().unwrap_or(0.0);
                    canvas[synthesis + index] += sample * self.window[index];
                }
                analysis += analysis_hop;
                synthesis += STRETCH_HOP;
            }

            self.tails[channel].copy_from_slice(&canvas[produced..produced + overlap]);
            for (frame, sample) in canvas[..produced].iter().enumerate() {
                mixed[frame * channels + channel] = sample * 0.5;
            }
        }

        (mixed, windows * analysis_hop)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Silent {
    NoDecoder,

    NoTrack,
}

impl Silent {
    pub fn message_key(self) -> &'static str {
        match self {
            Self::NoDecoder => "library.sound.noDecoder",
            Self::NoTrack => "library.sound.noTrack",
        }
    }
}

pub struct Sound {
    pub volume: f32,
    pub muted: bool,
    pub restore: f32,
}

impl Default for Sound {
    fn default() -> Self {
        Self {
            volume: 1.0,
            muted: true,
            restore: 1.0,
        }
    }
}

impl Sound {
    pub fn effective(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.volume
        }
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        self.muted = self.volume <= 0.001;
        if !self.muted {
            self.restore = self.volume;
        }
    }

    pub fn nudge(&mut self, step: f32) {
        let from = if self.muted { 0.0 } else { self.volume };
        self.set_volume(from + step);
    }

    pub fn toggle(&mut self) {
        if self.muted {
            let restored = if self.restore <= 0.001 {
                1.0
            } else {
                self.restore
            };
            self.volume = restored;
            self.muted = false;
        } else {
            self.restore = self.volume;
            self.muted = true;
        }
    }
}

pub const WINDOW_SECONDS: f64 = 20.0;
pub const WINDOW_REFILL_SECONDS: f64 = 5.0;

pub struct Speaker {
    output: Option<AudioOutput>,

    volume: f32,

    path: Option<PathBuf>,
    pcm: Option<PcmBuffer>,

    window_start: f64,

    loading: bool,

    queued_through: Option<f64>,

    silent: Vec<(PathBuf, Silent)>,

    stretcher: Stretcher,
}

impl Default for Speaker {
    fn default() -> Self {
        Self {
            output: None,
            volume: 1.0,
            path: None,
            pcm: None,
            window_start: 0.0,
            loading: false,
            queued_through: None,
            silent: Vec::new(),
            stretcher: Stretcher::default(),
        }
    }
}

impl Speaker {
    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume.clamp(0.0, 1.0);
        if let Some(output) = self.output.as_ref() {
            output.set_volume(self.volume);
        }
    }

    pub fn silent_reason(&self, path: &Path) -> Option<Silent> {
        self.silent
            .iter()
            .find(|(known, _)| known == path)
            .map(|(_, why)| *why)
    }

    pub fn is_silent(&self, path: &Path) -> bool {
        self.silent_reason(path).is_some()
    }

    pub fn window_needed(&self, path: &Path, seconds: f64) -> Option<f64> {
        if self.loading || self.is_silent(path) {
            return None;
        }
        if self.path.as_deref() != Some(path) {
            return Some((seconds - 0.2).max(0.0));
        }
        let Some(pcm) = self.pcm.as_ref() else {
            return Some((seconds - 0.2).max(0.0));
        };
        let end = self.window_start + pcm.duration_seconds();
        if seconds < self.window_start || seconds + WINDOW_REFILL_SECONDS > end {
            return Some((seconds - 0.2).max(0.0));
        }
        None
    }

    pub fn begin_loading(&mut self) {
        self.loading = true;
    }

    pub fn accept_window(&mut self, path: &Path, pcm: PcmBuffer, start: f64) {
        self.loading = false;
        if pcm.frame_count() == 0 {
            self.note_silent(path, Silent::NoTrack);
            return;
        }
        if self.path.as_deref() != Some(path) {
            self.silence();
            self.path = Some(path.to_path_buf());
        }
        self.pcm = Some(pcm);
        self.window_start = start;
    }

    pub fn note_silent(&mut self, path: &Path, why: Silent) {
        self.loading = false;
        self.pcm = None;
        if !self.is_silent(path) {
            self.silent.push((path.to_path_buf(), why));
        }
    }

    pub fn feed(&mut self, seconds: f64, speed: f32) {
        let Some(pcm) = self.pcm.as_ref() else {
            return;
        };
        if self.output.is_none() {
            self.output = AudioOutput::open().ok();
            if let Some(output) = self.output.as_ref() {
                output.set_volume(self.volume);
            }
        }
        let Some(output) = self.output.as_ref() else {
            return;
        };

        let rate = output.sample_rate();
        let channels = output.channels();
        let per_second = (rate.max(1) as usize * channels.max(1)) as f64;
        let speed = if speed.is_finite() && speed > 0.05 {
            f64::from(speed)
        } else {
            1.0
        };
        let reach = queue_reach(speed, rate);

        let follows_on = self.queued_through.is_some_and(|through| {
            through >= seconds - CHUNK_SECONDS * speed && through <= seconds + reach
        });
        if !follows_on {
            output.seek();
            self.queued_through = Some(seconds);
        }

        for _ in 0..MAX_CHUNKS_PER_FEED {
            let queued = output.queued_samples() as f64 / per_second;
            if queued >= QUEUE_CEILING_SECONDS {
                break;
            }
            let from = self.queued_through.unwrap_or(seconds);
            let offset = from - self.window_start;
            if offset < 0.0 || offset > pcm.duration_seconds() {
                break;
            }

            let wanted = chunk_frames(speed, rate);
            let raw = take_interleaved(
                pcm,
                offset * pcm.sample_rate.max(1) as f64,
                wanted,
                channels,
                rate,
            );
            if raw.is_empty() {
                break;
            }
            let (samples, consumed) = self.stretcher.compress(&raw, channels, speed);
            if samples.is_empty() || consumed == 0 {
                break;
            }
            self.queued_through = Some(from + consumed as f64 / rate.max(1) as f64);
            output.queue_samples(&samples);
        }

        output.start();
    }

    pub fn silence(&mut self) {
        self.queued_through = None;
        self.stretcher.reset();
        if let Some(output) = self.output.as_ref() {
            output.pause();
            output.seek();
        }
    }

    pub fn forget(&mut self) {
        self.silence();
        self.path = None;
        self.pcm = None;
        self.window_start = 0.0;
        self.loading = false;
    }
}

/// Only the tests in this file ask for this; compiled for them alone so the shipping
/// binary does not carry something nothing calls.
#[cfg(test)]
pub fn frame_index(seconds: f64, sample_rate: u32) -> usize {
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    (seconds * sample_rate.max(1) as f64) as usize
}

#[cfg(test)]
mod tests {
    fn tone(frames: usize, channels: usize, frequency: f32) -> Vec<f32> {
        (0..frames * channels)
            .map(|index| {
                let frame = (index / channels) as f32;
                (std::f32::consts::TAU * frequency * frame / 48_000.0).sin() * 0.4
            })
            .collect()
    }

    fn zero_crossings(samples: &[f32], channels: usize) -> usize {
        samples
            .iter()
            .step_by(channels)
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|pair| (pair[0].is_sign_negative()) != (pair[1].is_sign_negative()))
            .count()
    }

    #[test]
    fn one_times_speed_hands_the_samples_straight_back() {
        let samples = tone(960, 2, 220.0);
        let mut stretcher = Stretcher::default();
        let (same, consumed) = stretcher.compress(&samples, 2, 1.0);
        assert_eq!(same, samples);
        assert_eq!(consumed, 960);
    }

    #[test]
    fn compressing_shortens_the_chunk_by_the_speed() {
        let frames = 24_000;
        let samples = tone(frames, 2, 220.0);
        let mut stretcher = Stretcher::default();
        let (out, consumed) = stretcher.compress(&samples, 2, 2.0);
        let produced = out.len() / 2;
        assert!(produced > 0, "the stretcher produced nothing");
        let ratio = consumed as f64 / produced as f64;
        assert!(
            (ratio - 2.0).abs() < 0.05,
            "expected roughly two source frames per output frame, got {ratio}"
        );
    }

    #[test]
    fn compressing_keeps_the_pitch_rather_than_raising_it() {
        let frames = 48_000;
        let samples = tone(frames, 1, 220.0);
        let mut stretcher = Stretcher::default();
        let (out, consumed) = stretcher.compress(&samples, 1, 2.0);
        let produced = out.len();
        assert!(produced > 4_000);

        let source_rate = zero_crossings(&samples[..consumed], 1) as f64 / consumed as f64;
        let out_rate = zero_crossings(&out, 1) as f64 / produced as f64;
        assert!(
            (out_rate / source_rate - 1.0).abs() < 0.12,
            "pitch moved: {source_rate} crossings per frame in, {out_rate} out"
        );
    }

    #[test]
    fn an_empty_chunk_survives_the_stretcher() {
        let mut stretcher = Stretcher::default();
        assert_eq!(stretcher.compress(&[], 2, 2.0), (Vec::new(), 0));
    }

    #[test]
    fn a_chunk_too_short_to_hold_a_window_is_refused_rather_than_padded() {
        let samples = tone(64, 2, 220.0);
        let mut stretcher = Stretcher::default();
        let (out, consumed) = stretcher.compress(&samples, 2, 2.0);
        assert!(out.is_empty());
        assert_eq!(consumed, 0);
    }

    use super::*;

    fn pcm(channels: usize, rate: u32, frames: usize) -> PcmBuffer {
        let samples = (0..channels)
            .map(|channel| {
                (0..frames)
                    .map(|frame| (channel * 1000 + frame) as f32)
                    .collect()
            })
            .collect();
        PcmBuffer {
            sample_rate: rate,
            channels,
            samples,
        }
    }

    #[test]
    fn a_moment_in_seconds_lands_on_the_right_sample() {
        assert_eq!(frame_index(0.0, 48_000), 0);
        assert_eq!(frame_index(1.0, 48_000), 48_000);
        assert_eq!(frame_index(0.5, 44_100), 22_050);
    }

    #[test]
    fn a_nonsense_moment_starts_from_the_beginning_rather_than_panicking() {
        assert_eq!(frame_index(-3.0, 48_000), 0);
        assert_eq!(frame_index(f64::NAN, 48_000), 0);

        assert_eq!(frame_index(1.0, 0), 1);
    }

    #[test]
    fn stereo_at_the_device_rate_comes_out_interleaved_in_order() {
        let source = pcm(2, 48_000, 10);
        let out = take_interleaved(&source, 0.0, 3, 2, 48_000);

        assert_eq!(out, vec![0.0, 1000.0, 1.0, 1001.0, 2.0, 1002.0]);
    }

    #[test]
    fn a_mono_file_is_heard_from_both_speakers_rather_than_only_the_left() {
        let source = pcm(1, 48_000, 4);
        let out = take_interleaved(&source, 0.0, 2, 2, 48_000);
        assert_eq!(out, vec![0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn a_source_slower_than_the_device_is_stretched_rather_than_played_fast() {
        let source = pcm(1, 24_000, 8);
        let out = take_interleaved(&source, 0.0, 4, 1, 48_000);
        assert_eq!(
            out,
            vec![0.0, 0.5, 1.0, 1.5],
            "the gaps between source samples are filled in rather than held flat"
        );
    }

    #[test]
    fn a_source_faster_than_the_device_is_stepped_through_rather_than_played_slow() {
        let source = pcm(1, 96_000, 8);
        let out = take_interleaved(&source, 0.0, 4, 1, 48_000);
        assert_eq!(out, vec![0.0, 2.0, 4.0, 6.0]);
    }

    #[test]
    fn asking_past_the_end_yields_what_is_left_rather_than_reading_off_it() {
        let source = pcm(1, 48_000, 4);
        assert_eq!(
            take_interleaved(&source, 2.0, 10, 1, 48_000),
            vec![2.0, 3.0]
        );
        assert!(take_interleaved(&source, 99.0, 10, 1, 48_000).is_empty());
    }

    #[test]
    fn a_rate_the_device_does_not_share_is_interpolated_rather_than_stepped() {
        let source = pcm(1, 44_100, 16);
        let out = take_interleaved(&source, 0.0, 8, 1, 48_000);
        let mut steps: Vec<f32> = Vec::new();
        for pair in out.windows(2) {
            steps.push(pair[1] - pair[0]);
        }
        assert!(
            steps.iter().all(|step| *step > 0.0),
            "a held sample would show up as a flat step: {out:?}"
        );
    }

    #[test]
    fn a_file_with_no_audio_at_all_yields_nothing_rather_than_a_panic() {
        let empty = PcmBuffer {
            sample_rate: 48_000,
            channels: 0,
            samples: Vec::new(),
        };
        assert!(take_interleaved(&empty, 0.0, 10, 2, 48_000).is_empty());
        assert!(take_interleaved(&pcm(1, 48_000, 4), 0.0, 0, 2, 48_000).is_empty());
    }

    #[test]
    fn more_speakers_than_the_file_has_are_fed_rather_than_left_silent() {
        let source = pcm(2, 48_000, 2);
        let out = take_interleaved(&source, 0.0, 1, 4, 48_000);

        assert_eq!(out, vec![0.0, 1000.0, 1000.0, 1000.0]);
    }

    #[test]
    fn a_file_that_cannot_be_decoded_is_remembered_so_it_is_not_asked_for_again() {
        let mut speaker = Speaker::default();
        let missing = Path::new("/nowhere/at/all.mp4");
        speaker.note_silent(missing, Silent::NoDecoder);
        assert_eq!(speaker.silent_reason(missing), Some(Silent::NoDecoder));
        assert_eq!(speaker.window_needed(missing, 0.0), None);
    }

    #[test]
    fn a_decoded_stretch_is_kept_until_the_playhead_nears_its_end() {
        let mut speaker = Speaker::default();
        let path = Path::new("/videos/clip.mp4");
        assert_eq!(speaker.window_needed(path, 12.0), Some(11.8));

        speaker.begin_loading();
        speaker.accept_window(path, pcm(2, 48_000, 48_000 * 8), 11.8);

        assert_eq!(speaker.window_needed(path, 13.0), None);

        assert!(speaker.window_needed(path, 17.0).is_some());

        assert!(speaker.window_needed(path, 5.0).is_some());
    }

    /// The span `feed` may queue ahead of the playhead has to stay inside the span
    /// `follows_on` accepts. When it does not, a queue filled to the ceiling reads as a
    /// discontinuity, `feed` flushes the output and refills it from the playhead, and it
    /// does that on every call — which is heard as a buzz rather than as playback.
    #[test]
    fn a_full_queue_still_reads_as_continuous() {
        for rate in [44_100u32, 48_000, 96_000] {
            for speed in [1.0f64, 0.5, 2.0] {
                let reach = queue_reach(speed, rate);

                // The worst case: the queue sat one sample under the ceiling and one more
                // whole chunk went in on top of it.
                let chunk = chunk_frames(speed, rate) as f64 / rate as f64;
                let queued = QUEUE_CEILING_SECONDS * speed + chunk;

                assert!(
                    queued <= reach,
                    "at {rate} Hz and speed {speed} a full queue runs {queued} ahead,                      past the {reach} `follows_on` allows",
                );
            }
        }
    }
}
