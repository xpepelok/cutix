use crate::biquad::{Biquad, BiquadCoefficients, BiquadKind};

pub const EQUALIZER_BAND_FREQUENCIES: [f32; 5] = [60.0, 250.0, 1_000.0, 4_000.0, 12_000.0];
pub const EQUALIZER_GAIN_LIMIT_DB: f32 = 18.0;

#[derive(Clone, Copy, Debug)]
pub struct EqualizerBand {
    pub kind: BiquadKind,
    pub frequency: f32,
    pub q: f32,
    pub gain_db: f32,
}

#[derive(Clone, Debug)]
pub struct EqualizerOptions {
    pub sample_rate: f32,
    pub bands: Vec<EqualizerBand>,
}

pub fn graphic_bands(gains_db: &[f32]) -> Vec<EqualizerBand> {
    EQUALIZER_BAND_FREQUENCIES
        .iter()
        .enumerate()
        .map(|(index, frequency)| {
            let gain_db = gains_db
                .get(index)
                .copied()
                .unwrap_or(0.0)
                .clamp(-EQUALIZER_GAIN_LIMIT_DB, EQUALIZER_GAIN_LIMIT_DB);
            let kind = match index {
                0 => BiquadKind::LowShelf,
                index if index == EQUALIZER_BAND_FREQUENCIES.len() - 1 => BiquadKind::HighShelf,
                _ => BiquadKind::Peaking,
            };
            EqualizerBand {
                kind,
                frequency: *frequency,
                q: if matches!(kind, BiquadKind::Peaking) {
                    1.0
                } else {
                    0.707
                },
                gain_db,
            }
        })
        .collect()
}

pub fn equalize(samples: &[f32], options: &EqualizerOptions) -> Vec<f32> {
    let mut output = samples.to_vec();
    if samples.is_empty() || options.sample_rate <= 0.0 {
        return output;
    }

    for band in &options.bands {
        if band.gain_db.abs() < 0.01
            && matches!(
                band.kind,
                BiquadKind::Peaking | BiquadKind::LowShelf | BiquadKind::HighShelf
            )
        {
            continue;
        }
        let coefficients = BiquadCoefficients::design(
            band.kind,
            options.sample_rate,
            band.frequency,
            band.q,
            band.gain_db,
        );
        let mut filter = Biquad::new(coefficients);
        filter.process(&mut output);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fft::{forward, hann_window, Complex};

    fn band_energy(samples: &[f32], sample_rate: f32, low: f32, high: f32) -> f32 {
        let size = 8192.min(samples.len().next_power_of_two() / 2).max(1024);
        let window = hann_window(size);
        let mut total = 0.0f32;
        let mut frames = 0;
        let mut start = 0;
        while start + size <= samples.len() {
            let mut spectrum: Vec<Complex> = (0..size)
                .map(|index| Complex::new(samples[start + index] * window[index], 0.0))
                .collect();
            forward(&mut spectrum);
            for (bin, value) in spectrum.iter().enumerate().take(size / 2).skip(1) {
                let frequency = bin as f32 * sample_rate / size as f32;
                if frequency >= low && frequency <= high {
                    let magnitude = value.magnitude();
                    total += magnitude * magnitude;
                }
            }
            frames += 1;
            start += size;
        }
        if frames == 0 {
            0.0
        } else {
            total / frames as f32
        }
    }

    fn sine(frequency: f32, sample_rate: f32, length: usize) -> Vec<f32> {
        (0..length)
            .map(|index| {
                (std::f32::consts::TAU * frequency * index as f32 / sample_rate).sin() * 0.5
            })
            .collect()
    }

    #[test]
    fn boosting_a_band_raises_that_band_by_the_requested_gain() {
        let sample_rate = 48_000.0;
        let input = sine(1_000.0, sample_rate, 48_000);
        let options = EqualizerOptions {
            sample_rate,
            bands: graphic_bands(&[0.0, 0.0, 12.0, 0.0, 0.0]),
        };
        let output = equalize(&input, &options);

        let before = band_energy(&input, sample_rate, 900.0, 1_100.0);
        let after = band_energy(&output, sample_rate, 900.0, 1_100.0);
        let gain_db = 10.0 * (after / before).log10();

        assert!(
            (gain_db - 12.0).abs() < 0.6,
            "expected +12 dB at 1 kHz, measured {gain_db}"
        );
    }

    #[test]
    fn cutting_a_band_leaves_other_bands_alone() {
        let sample_rate = 48_000.0;
        let low = sine(25.0, sample_rate, 48_000);
        let high = sine(8_000.0, sample_rate, 48_000);
        let input: Vec<f32> = low
            .iter()
            .zip(high.iter())
            .map(|(a, b)| (a + b) * 0.5)
            .collect();

        let options = EqualizerOptions {
            sample_rate,
            bands: graphic_bands(&[-12.0, 0.0, 0.0, 0.0, 0.0]),
        };
        let output = equalize(&input, &options);

        let low_before = band_energy(&input, sample_rate, 15.0, 40.0);
        let low_after = band_energy(&output, sample_rate, 15.0, 40.0);
        let high_before = band_energy(&input, sample_rate, 7_500.0, 8_500.0);
        let high_after = band_energy(&output, sample_rate, 7_500.0, 8_500.0);

        let low_db = 10.0 * (low_after / low_before).log10();
        let high_db = 10.0 * (high_after / high_before).log10();

        assert!(low_db < -9.0, "low band only moved {low_db} dB");
        assert!(high_db.abs() < 1.0, "high band moved {high_db} dB");
    }

    #[test]
    fn a_flat_equalizer_is_a_no_op() {
        let sample_rate = 48_000.0;
        let input = sine(440.0, sample_rate, 4_096);
        let output = equalize(
            &input,
            &EqualizerOptions {
                sample_rate,
                bands: graphic_bands(&[0.0; 5]),
            },
        );
        assert_eq!(input, output);
    }
}
