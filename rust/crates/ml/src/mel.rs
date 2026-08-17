pub const SAMPLE_RATE: u32 = 16_000;
pub const N_FFT: usize = 400;
pub const HOP_LENGTH: usize = 160;
pub const N_MELS: usize = 80;
pub const N_FREQS: usize = N_FFT / 2 + 1;
pub const CHUNK_SAMPLES: usize = SAMPLE_RATE as usize * 30;
pub const CHUNK_FRAMES: usize = CHUNK_SAMPLES / HOP_LENGTH;

pub fn hz_to_mel(hz: f64) -> f64 {
    let f_sp = 200.0 / 3.0;
    let min_log_hz = 1000.0;
    let min_log_mel = min_log_hz / f_sp;
    let logstep = (6.4_f64).ln() / 27.0;
    if hz >= min_log_hz {
        min_log_mel + (hz / min_log_hz).ln() / logstep
    } else {
        hz / f_sp
    }
}

pub fn mel_to_hz(mel: f64) -> f64 {
    let f_sp = 200.0 / 3.0;
    let min_log_hz = 1000.0;
    let min_log_mel = min_log_hz / f_sp;
    let logstep = (6.4_f64).ln() / 27.0;
    if mel >= min_log_mel {
        min_log_hz * (logstep * (mel - min_log_mel)).exp()
    } else {
        mel * f_sp
    }
}

pub fn mel_center_frequencies(n_mels: usize, sample_rate: u32) -> Vec<f64> {
    let max_mel = hz_to_mel(sample_rate as f64 / 2.0);
    (0..n_mels)
        .map(|index| mel_to_hz(max_mel * (index + 1) as f64 / (n_mels + 1) as f64))
        .collect()
}

pub fn mel_filterbank(n_mels: usize, n_fft: usize, sample_rate: u32) -> Vec<Vec<f32>> {
    let n_freqs = n_fft / 2 + 1;
    let freqs: Vec<f64> = (0..n_freqs)
        .map(|index| index as f64 * sample_rate as f64 / n_fft as f64)
        .collect();

    let max_mel = hz_to_mel(sample_rate as f64 / 2.0);
    let mel_points: Vec<f64> = (0..n_mels + 2)
        .map(|index| mel_to_hz(max_mel * index as f64 / (n_mels + 1) as f64))
        .collect();

    let mut filters = vec![vec![0.0f32; n_freqs]; n_mels];
    for index in 0..n_mels {
        let left = mel_points[index];
        let center = mel_points[index + 1];
        let right = mel_points[index + 2];
        let enorm = 2.0 / (right - left);
        for (bin, freq) in freqs.iter().enumerate() {
            let lower = (freq - left) / (center - left);
            let upper = (right - freq) / (right - center);
            let weight = lower.min(upper).max(0.0) * enorm;
            filters[index][bin] = weight as f32;
        }
    }
    filters
}

pub struct MelExtractor {
    filters: Vec<Vec<f32>>,
    window: Vec<f32>,
    cos_table: Vec<f32>,
    sin_table: Vec<f32>,
}

impl Default for MelExtractor {
    fn default() -> Self {
        Self::new()
    }
}

impl MelExtractor {
    pub fn new() -> Self {
        let mut cos_table = vec![0.0f32; N_FREQS * N_FFT];
        let mut sin_table = vec![0.0f32; N_FREQS * N_FFT];
        for k in 0..N_FREQS {
            for n in 0..N_FFT {
                let angle = -2.0 * std::f64::consts::PI * k as f64 * n as f64 / N_FFT as f64;
                cos_table[k * N_FFT + n] = angle.cos() as f32;
                sin_table[k * N_FFT + n] = angle.sin() as f32;
            }
        }

        let window: Vec<f32> = (0..N_FFT)
            .map(|n| {
                let value =
                    0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / N_FFT as f64).cos();
                value as f32
            })
            .collect();

        Self {
            filters: mel_filterbank(N_MELS, N_FFT, SAMPLE_RATE),
            window,
            cos_table,
            sin_table,
        }
    }

    pub fn log_mel(&self, samples: &[f32]) -> Vec<f32> {
        let padded = reflect_pad(samples, N_FFT / 2);
        let frames = CHUNK_FRAMES;
        let mut mel = vec![0.0f32; N_MELS * frames];
        let mut power = vec![0.0f32; N_FREQS];
        let mut frame = vec![0.0f32; N_FFT];

        for index in 0..frames {
            let start = index * HOP_LENGTH;
            for n in 0..N_FFT {
                let sample = padded.get(start + n).copied().unwrap_or(0.0);
                frame[n] = sample * self.window[n];
            }
            for k in 0..N_FREQS {
                let cos_row = &self.cos_table[k * N_FFT..(k + 1) * N_FFT];
                let sin_row = &self.sin_table[k * N_FFT..(k + 1) * N_FFT];
                let mut re = 0.0f32;
                let mut im = 0.0f32;
                for n in 0..N_FFT {
                    re += frame[n] * cos_row[n];
                    im += frame[n] * sin_row[n];
                }
                power[k] = re * re + im * im;
            }
            for (bin, filter) in self.filters.iter().enumerate() {
                let mut sum = 0.0f32;
                for k in 0..N_FREQS {
                    sum += filter[k] * power[k];
                }
                mel[bin * frames + index] = sum;
            }
        }

        let mut maximum = f32::MIN;
        for value in mel.iter_mut() {
            *value = value.max(1e-10).log10();
            if *value > maximum {
                maximum = *value;
            }
        }
        let floor = maximum - 8.0;
        for value in mel.iter_mut() {
            *value = (value.max(floor) + 4.0) / 4.0;
        }

        mel
    }
}

fn reflect_pad(samples: &[f32], pad: usize) -> Vec<f32> {
    if samples.is_empty() {
        return vec![0.0; pad * 2];
    }
    let mut out = Vec::with_capacity(samples.len() + pad * 2);
    for index in (1..=pad).rev() {
        out.push(samples[index.min(samples.len() - 1)]);
    }
    out.extend_from_slice(samples);
    for index in 1..=pad {
        let position = samples.len().saturating_sub(1 + index);
        out.push(samples[position]);
    }
    out
}

pub fn resample_to_16k(samples: &[f32], sample_rate: u32) -> Vec<f32> {
    if sample_rate == SAMPLE_RATE || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = sample_rate as f64 / SAMPLE_RATE as f64;
    let length = ((samples.len() as f64) / ratio).floor() as usize;
    let mut out = Vec::with_capacity(length);
    for index in 0..length {
        let position = index as f64 * ratio;
        let left = position.floor() as usize;
        let fraction = (position - left as f64) as f32;
        let a = samples[left.min(samples.len() - 1)];
        let b = samples[(left + 1).min(samples.len() - 1)];
        out.push(a + (b - a) * fraction);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mel_scale_round_trips() {
        for hz in [0.0, 100.0, 999.0, 1000.0, 4000.0, 8000.0] {
            let back = mel_to_hz(hz_to_mel(hz));
            assert!((back - hz).abs() < 1e-6, "{hz} -> {back}");
        }
    }

    #[test]
    fn mel_scale_is_linear_below_one_khz() {
        assert!((hz_to_mel(200.0) - 3.0).abs() < 1e-9);
        assert!((hz_to_mel(1000.0) - 15.0).abs() < 1e-9);
    }

    #[test]
    fn filterbank_has_expected_shape() {
        let filters = mel_filterbank(N_MELS, N_FFT, SAMPLE_RATE);
        assert_eq!(filters.len(), N_MELS);
        assert!(filters.iter().all(|row| row.len() == N_FREQS));
        assert!(filters
            .iter()
            .all(|row| row.iter().any(|value| *value > 0.0)));
        assert!(filters
            .iter()
            .all(|row| row.iter().all(|value| *value >= 0.0)));
    }

    #[test]
    fn filter_centres_increase_monotonically() {
        let centres = mel_center_frequencies(N_MELS, SAMPLE_RATE);
        assert_eq!(centres.len(), N_MELS);
        for pair in centres.windows(2) {
            assert!(pair[1] > pair[0]);
        }
        assert!(centres[0] < 100.0);
        assert!(*centres.last().unwrap() < 8000.0);
        assert!(*centres.last().unwrap() > 7000.0);
    }

    #[test]
    fn filterbank_matches_known_slaney_values() {
        let filters = mel_filterbank(N_MELS, N_FFT, SAMPLE_RATE);
        let peak = filters[0].iter().cloned().fold(0.0f32, f32::max);
        assert!((peak - 0.024861).abs() < 1e-5, "peak {peak}");
        assert_eq!(filters[0][0], 0.0);
        assert!(filters[0][1] > 0.0);
        assert_eq!(filters[79][0], 0.0);
    }

    #[test]
    fn log_mel_of_a_tone_peaks_in_the_expected_bin() {
        let samples: Vec<f32> = (0..SAMPLE_RATE as usize)
            .map(|n| (2.0 * std::f32::consts::PI * 440.0 * n as f32 / SAMPLE_RATE as f32).sin())
            .collect();
        let extractor = MelExtractor::new();
        let mel = extractor.log_mel(&samples);
        assert_eq!(mel.len(), N_MELS * CHUNK_FRAMES);

        let frame = 50;
        let mut best = 0usize;
        let mut best_value = f32::MIN;
        for bin in 0..N_MELS {
            let value = mel[bin * CHUNK_FRAMES + frame];
            if value > best_value {
                best_value = value;
                best = bin;
            }
        }
        assert!((9..=13).contains(&best), "peak bin {best}");
    }

    #[test]
    fn log_mel_values_stay_in_range() {
        let samples = vec![0.0f32; 16_000];
        let mel = MelExtractor::new().log_mel(&samples);
        assert!(mel.iter().all(|value| value.is_finite()));
        let maximum = mel.iter().cloned().fold(f32::MIN, f32::max);
        let minimum = mel.iter().cloned().fold(f32::MAX, f32::min);
        assert!((maximum - minimum) <= 2.0001);
    }

    #[test]
    fn resampling_halves_the_length_for_double_rate() {
        let samples = vec![0.5f32; 32_000];
        let out = resample_to_16k(&samples, 32_000);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|value| (*value - 0.5).abs() < 1e-6));
    }

    #[test]
    fn resampling_is_a_no_op_at_16k() {
        let samples = vec![1.0f32, 2.0, 3.0];
        assert_eq!(resample_to_16k(&samples, 16_000), samples);
    }
}
