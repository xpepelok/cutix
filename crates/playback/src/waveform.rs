use std::path::Path;

use crate::audio_decode::{PcmBuffer, decode_audio};
use crate::error::Result;

pub const DEFAULT_BUCKETS: usize = 2048;

#[derive(Clone, Debug, PartialEq)]
pub struct WaveformPeaks {
    pub buckets: Vec<f32>,
    pub duration_seconds: f64,
}

impl WaveformPeaks {
    pub fn is_empty(&self) -> bool {
        self.buckets.is_empty()
    }

    pub fn resample(&self, width: usize) -> Vec<f32> {
        if width == 0 || self.buckets.is_empty() {
            return Vec::new();
        }
        let source = self.buckets.len();
        (0..width)
            .map(|index| {
                let start = index * source / width;
                let end = ((index + 1) * source).div_ceil(width).max(start + 1);
                self.buckets[start..end.min(source)]
                    .iter()
                    .copied()
                    .fold(0.0f32, f32::max)
            })
            .collect()
    }

    pub fn slice(&self, start_seconds: f64, end_seconds: f64, width: usize) -> Vec<f32> {
        if width == 0 || self.buckets.is_empty() || self.duration_seconds <= 0.0 {
            return Vec::new();
        }
        let total = self.buckets.len() as f64;
        let to_bucket =
            |seconds: f64| ((seconds / self.duration_seconds) * total).clamp(0.0, total) as usize;
        let from = to_bucket(start_seconds.max(0.0));
        let to = to_bucket(end_seconds.max(start_seconds)).max(from + 1);
        let window = &self.buckets[from.min(self.buckets.len() - 1)..to.min(self.buckets.len())];
        if window.is_empty() {
            return vec![0.0; width];
        }
        let source = window.len();
        (0..width)
            .map(|index| {
                let start = index * source / width;
                let end = ((index + 1) * source).div_ceil(width).max(start + 1);
                window[start..end.min(source)]
                    .iter()
                    .copied()
                    .fold(0.0f32, f32::max)
            })
            .collect()
    }
}

pub fn peaks_from_pcm(pcm: &PcmBuffer, buckets: usize) -> WaveformPeaks {
    let frames = pcm.frame_count();
    let buckets = buckets.max(1).min(frames.max(1));
    if frames == 0 {
        return WaveformPeaks {
            buckets: Vec::new(),
            duration_seconds: 0.0,
        };
    }
    let mut out = Vec::with_capacity(buckets);
    for index in 0..buckets {
        let start = index * frames / buckets;
        let end = ((index + 1) * frames)
            .div_ceil(buckets)
            .max(start + 1)
            .min(frames);
        let mut peak = 0.0f32;
        for channel in 0..pcm.channels.max(1) {
            let plane = pcm.channel(channel);
            for value in &plane[start.min(plane.len())..end.min(plane.len())] {
                let magnitude = value.abs();
                if magnitude > peak {
                    peak = magnitude;
                }
            }
        }
        out.push(peak.min(1.0));
    }
    WaveformPeaks {
        buckets: out,
        duration_seconds: pcm.duration_seconds(),
    }
}

pub fn peaks_for_file(path: &Path, buckets: usize) -> Result<WaveformPeaks> {
    let pcm = decode_audio(path)?;
    Ok(peaks_from_pcm(&pcm, buckets))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frames: usize, amplitude: f32) -> PcmBuffer {
        PcmBuffer {
            sample_rate: 1000,
            channels: 1,
            samples: vec![
                (0..frames)
                    .map(|index| amplitude * ((index as f32 / 10.0) * std::f32::consts::TAU).sin())
                    .collect(),
            ],
        }
    }

    #[test]
    fn peaks_track_amplitude() {
        let peaks = peaks_from_pcm(&tone(1000, 0.5), 10);
        assert_eq!(peaks.buckets.len(), 10);
        assert_eq!(peaks.duration_seconds, 1.0);
        for value in &peaks.buckets {
            assert!((*value - 0.5).abs() < 0.03, "bucket {value} not near 0.5");
        }
    }

    #[test]
    fn peaks_follow_a_ramp() {
        let pcm = PcmBuffer {
            sample_rate: 100,
            channels: 1,
            samples: vec![(0..100).map(|index| index as f32 / 100.0).collect()],
        };
        let peaks = peaks_from_pcm(&pcm, 4);
        assert_eq!(peaks.buckets.len(), 4);
        assert!((peaks.buckets[0] - 0.24).abs() < 1e-5);
        assert!((peaks.buckets[1] - 0.49).abs() < 1e-5);
        assert!((peaks.buckets[2] - 0.74).abs() < 1e-5);
        assert!((peaks.buckets[3] - 0.99).abs() < 1e-5);
    }

    #[test]
    fn silence_yields_zero_peaks() {
        let pcm = PcmBuffer {
            sample_rate: 48_000,
            channels: 2,
            samples: vec![vec![0.0; 4800], vec![0.0; 4800]],
        };
        let peaks = peaks_from_pcm(&pcm, 32);
        assert_eq!(peaks.buckets.len(), 32);
        assert!(peaks.buckets.iter().all(|value| *value == 0.0));
        assert!((peaks.duration_seconds - 0.1).abs() < 1e-9);
    }

    #[test]
    fn loudest_channel_wins() {
        let pcm = PcmBuffer {
            sample_rate: 10,
            channels: 2,
            samples: vec![vec![0.1; 10], vec![0.8; 10]],
        };
        let peaks = peaks_from_pcm(&pcm, 2);
        assert_eq!(peaks.buckets, vec![0.8, 0.8]);
    }

    #[test]
    fn empty_audio_is_empty() {
        let pcm = PcmBuffer {
            sample_rate: 44_100,
            channels: 1,
            samples: vec![Vec::new()],
        };
        assert!(peaks_from_pcm(&pcm, 64).is_empty());
    }

    #[test]
    fn buckets_never_exceed_frames() {
        let pcm = PcmBuffer {
            sample_rate: 8,
            channels: 1,
            samples: vec![vec![0.5; 8]],
        };
        assert_eq!(peaks_from_pcm(&pcm, 1024).buckets.len(), 8);
    }

    #[test]
    fn resample_preserves_extremes() {
        let peaks = WaveformPeaks {
            buckets: vec![0.0, 1.0, 0.0, 0.25, 0.5, 0.75, 0.0, 0.1],
            duration_seconds: 8.0,
        };
        let out = peaks.resample(4);
        assert_eq!(out, vec![1.0, 0.25, 0.75, 0.1]);
        assert_eq!(peaks.resample(0).len(), 0);
    }

    #[test]
    fn resample_upscales() {
        let peaks = WaveformPeaks {
            buckets: vec![0.2, 0.9],
            duration_seconds: 2.0,
        };
        assert_eq!(peaks.resample(4), vec![0.2, 0.2, 0.9, 0.9]);
    }

    #[test]
    fn slice_selects_the_window() {
        let peaks = WaveformPeaks {
            buckets: vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8],
            duration_seconds: 8.0,
        };
        assert_eq!(peaks.slice(4.0, 8.0, 4), vec![0.5, 0.6, 0.7, 0.8]);
        assert_eq!(peaks.slice(0.0, 2.0, 2), vec![0.1, 0.2]);
    }

    #[test]
    fn slice_clamps_out_of_range() {
        let peaks = WaveformPeaks {
            buckets: vec![0.1, 0.2, 0.3, 0.4],
            duration_seconds: 4.0,
        };
        assert_eq!(peaks.slice(-5.0, 100.0, 4), vec![0.1, 0.2, 0.3, 0.4]);
        assert_eq!(peaks.slice(2.0, 2.0, 2).len(), 2);
    }

    #[test]
    fn empty_peaks_slice_to_nothing() {
        let peaks = WaveformPeaks {
            buckets: Vec::new(),
            duration_seconds: 0.0,
        };
        assert!(peaks.slice(0.0, 1.0, 10).is_empty());
        assert!(peaks.resample(10).is_empty());
    }
}
