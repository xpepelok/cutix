use std::path::{Path, PathBuf};

use cutix_playback::audio_decode::PcmBuffer;
use cutix_playback::output::AudioOutput;

pub const CHUNK_SECONDS: f64 = 0.35;

pub const QUEUE_CEILING_SECONDS: f64 = 0.5;

pub fn frame_index(seconds: f64, sample_rate: u32) -> usize {
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    (seconds * sample_rate.max(1) as f64) as usize
}

pub fn take_interleaved(
    pcm: &PcmBuffer,
    start_frame: usize,
    out_frames: usize,
    out_channels: usize,
    out_rate: u32,
) -> Vec<f32> {
    let out_channels = out_channels.max(1);
    let source_frames = pcm.frame_count();
    if source_frames == 0 || out_frames == 0 || pcm.channels == 0 {
        return Vec::new();
    }

    let ratio = pcm.sample_rate.max(1) as f64 / out_rate.max(1) as f64;
    let mut out = Vec::with_capacity(out_frames * out_channels);

    for frame in 0..out_frames {
        let source = start_frame + (frame as f64 * ratio) as usize;
        if source >= source_frames {
            break;
        }
        for channel in 0..out_channels {
            let sample = pcm.channel(channel.min(pcm.channels - 1))[source];
            out.push(sample);
        }
    }
    out
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

pub const WINDOW_SECONDS: f64 = 8.0;
pub const WINDOW_REFILL_SECONDS: f64 = 3.0;

pub struct Speaker {
    output: Option<AudioOutput>,

    volume: f32,

    path: Option<PathBuf>,
    pcm: Option<PcmBuffer>,

    window_start: f64,

    loading: bool,

    queued_through: Option<f64>,

    silent: Vec<(PathBuf, Silent)>,
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

    pub fn volume(&self) -> f32 {
        self.volume
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

    pub fn feed(&mut self, seconds: f64) {
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
        let queued =
            output.queued_samples() as f64 / (rate.max(1) as usize * channels.max(1)) as f64;
        if queued >= QUEUE_CEILING_SECONDS {
            return;
        }

        let follows_on = self
            .queued_through
            .is_some_and(|through| through >= seconds - CHUNK_SECONDS && through <= seconds + 1.0);
        let from = if follows_on {
            self.queued_through.unwrap_or(seconds)
        } else {
            output.seek();
            seconds
        };

        let offset = from - self.window_start;
        if offset < 0.0 || offset > pcm.duration_seconds() {
            return;
        }

        let frames = (CHUNK_SECONDS * rate as f64) as usize;
        let samples = take_interleaved(
            pcm,
            frame_index(offset, pcm.sample_rate),
            frames,
            channels,
            rate,
        );
        if samples.is_empty() {
            return;
        }
        let taken = samples.len() as f64 / (rate.max(1) as usize * channels.max(1)) as f64;
        self.queued_through = Some(from + taken);
        output.queue_samples(&samples);
        output.start();
    }

    pub fn silence(&mut self) {
        self.queued_through = None;
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

#[cfg(test)]
mod tests {
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
        let out = take_interleaved(&source, 0, 3, 2, 48_000);

        assert_eq!(out, vec![0.0, 1000.0, 1.0, 1001.0, 2.0, 1002.0]);
    }

    #[test]
    fn a_mono_file_is_heard_from_both_speakers_rather_than_only_the_left() {
        let source = pcm(1, 48_000, 4);
        let out = take_interleaved(&source, 0, 2, 2, 48_000);
        assert_eq!(out, vec![0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn a_source_slower_than_the_device_is_stretched_rather_than_played_fast() {
        let source = pcm(1, 24_000, 8);
        let out = take_interleaved(&source, 0, 4, 1, 48_000);
        assert_eq!(out, vec![0.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn a_source_faster_than_the_device_is_stepped_through_rather_than_played_slow() {
        let source = pcm(1, 96_000, 8);
        let out = take_interleaved(&source, 0, 4, 1, 48_000);
        assert_eq!(out, vec![0.0, 2.0, 4.0, 6.0]);
    }

    #[test]
    fn asking_past_the_end_yields_what_is_left_rather_than_reading_off_it() {
        let source = pcm(1, 48_000, 4);
        assert_eq!(take_interleaved(&source, 2, 10, 1, 48_000), vec![2.0, 3.0]);
        assert!(take_interleaved(&source, 99, 10, 1, 48_000).is_empty());
    }

    #[test]
    fn a_file_with_no_audio_at_all_yields_nothing_rather_than_a_panic() {
        let empty = PcmBuffer {
            sample_rate: 48_000,
            channels: 0,
            samples: Vec::new(),
        };
        assert!(take_interleaved(&empty, 0, 10, 2, 48_000).is_empty());
        assert!(take_interleaved(&pcm(1, 48_000, 4), 0, 0, 2, 48_000).is_empty());
    }

    #[test]
    fn more_speakers_than_the_file_has_are_fed_rather_than_left_silent() {
        let source = pcm(2, 48_000, 2);
        let out = take_interleaved(&source, 0, 1, 4, 48_000);

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
}
