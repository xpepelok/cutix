use crate::fft::{Complex, forward, hann_window, inverse};

const FRAME_SIZE: usize = 2048;
const OVERSAMPLING: usize = 4;

#[derive(Clone, Copy, Debug)]
pub struct PitchOptions {
    pub sample_rate: f32,
    pub semitones: f32,
    pub formant_semitones: f32,
    pub robotize: bool,
}

impl Default for PitchOptions {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            semitones: 0.0,
            formant_semitones: 0.0,
            robotize: false,
        }
    }
}

pub fn semitones_to_ratio(semitones: f32) -> f32 {
    2f32.powf(semitones / 12.0)
}

fn wrap_phase(value: f32) -> f32 {
    let tau = std::f32::consts::TAU;
    let mut wrapped = value;
    while wrapped > std::f32::consts::PI {
        wrapped -= tau;
    }
    while wrapped < -std::f32::consts::PI {
        wrapped += tau;
    }
    wrapped
}

fn spectral_envelope(magnitudes: &[f32], radius: usize) -> Vec<f32> {
    let bins = magnitudes.len();
    let mut prefix = vec![0.0f32; bins + 1];
    for index in 0..bins {
        prefix[index + 1] = prefix[index] + magnitudes[index];
    }
    (0..bins)
        .map(|index| {
            let low = index.saturating_sub(radius);
            let high = (index + radius + 1).min(bins);
            (prefix[high] - prefix[low]) / (high - low) as f32
        })
        .collect()
}

fn warp_envelope(envelope: &[f32], ratio: f32) -> Vec<f32> {
    let bins = envelope.len();
    (0..bins)
        .map(|index| {
            let source = index as f32 / ratio;
            if source >= (bins - 1) as f32 {
                return envelope[bins - 1];
            }
            let low = source.floor() as usize;
            let fraction = source - low as f32;
            envelope[low] * (1.0 - fraction) + envelope[low + 1] * fraction
        })
        .collect()
}

fn time_stretch(
    samples: &[f32],
    stretch: f32,
    envelope_ratio: f32,
    robotize: bool,
) -> (Vec<f32>, f32) {
    let analysis_hop = FRAME_SIZE / OVERSAMPLING;
    let synthesis_hop = ((analysis_hop as f32) * stretch).round().max(1.0) as usize;
    let bins = FRAME_SIZE / 2 + 1;
    let window = hann_window(FRAME_SIZE);
    let expected_phase_step = std::f32::consts::TAU * analysis_hop as f32 / FRAME_SIZE as f32;
    let warps_envelope = (envelope_ratio - 1.0).abs() > 1e-3;

    let actual_stretch = synthesis_hop as f32 / analysis_hop as f32;
    let frame_count = samples.len().div_ceil(analysis_hop) + 1;
    let output_length = (frame_count - 1) * synthesis_hop + FRAME_SIZE;

    let mut output = vec![0.0f32; output_length];
    let mut window_sum = vec![0.0f32; output_length];
    let mut previous_phase = vec![0.0f32; bins];
    let mut accumulated_phase = vec![0.0f32; bins];
    let mut spectrum = vec![Complex::default(); FRAME_SIZE];
    let mut magnitudes = vec![0.0f32; bins];

    for frame in 0..frame_count {
        let start = frame * analysis_hop;
        for index in 0..FRAME_SIZE {
            let value = samples.get(start + index).copied().unwrap_or(0.0);
            spectrum[index] = Complex::new(value * window[index], 0.0);
        }
        forward(&mut spectrum);

        for bin in 0..bins {
            magnitudes[bin] = spectrum[bin].magnitude();
        }

        if warps_envelope {
            let envelope = spectral_envelope(&magnitudes, 24);
            let warped = warp_envelope(&envelope, envelope_ratio);
            for bin in 0..bins {
                if envelope[bin] > 1e-9 {
                    magnitudes[bin] *= warped[bin] / envelope[bin];
                }
            }
        }

        for bin in 0..bins {
            let phase = spectrum[bin].im.atan2(spectrum[bin].re);
            let deviation =
                wrap_phase(phase - previous_phase[bin] - expected_phase_step * bin as f32);
            previous_phase[bin] = phase;

            let true_frequency = expected_phase_step * bin as f32 + deviation;
            accumulated_phase[bin] = if robotize {
                0.0
            } else {
                accumulated_phase[bin] + true_frequency * actual_stretch
            };

            let magnitude = magnitudes[bin];
            let value = Complex::new(
                magnitude * accumulated_phase[bin].cos(),
                magnitude * accumulated_phase[bin].sin(),
            );
            spectrum[bin] = value;
            if bin > 0 && bin < FRAME_SIZE - bin {
                spectrum[FRAME_SIZE - bin] = Complex::new(value.re, -value.im);
            }
        }

        inverse(&mut spectrum);

        let offset = frame * synthesis_hop;
        for index in 0..FRAME_SIZE {
            let target = offset + index;
            if target >= output_length {
                break;
            }
            output[target] += spectrum[index].re * window[index];
            window_sum[target] += window[index] * window[index];
        }
    }

    for index in 0..output_length {
        output[index] /= window_sum[index].max(0.15);
    }

    (output, actual_stretch)
}

fn resample(samples: &[f32], ratio: f32, target_length: usize) -> Vec<f32> {
    if samples.is_empty() {
        return vec![0.0; target_length];
    }
    (0..target_length)
        .map(|index| {
            let position = index as f32 * ratio;
            if position >= (samples.len() - 1) as f32 {
                return 0.0;
            }
            let low = position.floor() as usize;
            let fraction = position - low as f32;
            samples[low] * (1.0 - fraction) + samples[low + 1] * fraction
        })
        .collect()
}

pub fn shift_pitch(samples: &[f32], options: &PitchOptions) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }

    let pitch_ratio = semitones_to_ratio(options.semitones.clamp(-24.0, 24.0));
    let formant_ratio = semitones_to_ratio(options.formant_semitones.clamp(-24.0, 24.0));

    if (pitch_ratio - 1.0).abs() < 1e-4 && (formant_ratio - 1.0).abs() < 1e-4 && !options.robotize {
        return samples.to_vec();
    }

    let envelope_ratio = formant_ratio / pitch_ratio;
    let (stretched, actual_ratio) =
        time_stretch(samples, pitch_ratio, envelope_ratio, options.robotize);
    resample(&stretched, actual_ratio, samples.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fft::{Complex, forward, hann_window};

    fn sine(frequency: f32, sample_rate: f32, length: usize) -> Vec<f32> {
        (0..length)
            .map(|index| {
                (std::f32::consts::TAU * frequency * index as f32 / sample_rate).sin() * 0.5
            })
            .collect()
    }

    fn dominant_frequency(samples: &[f32], sample_rate: f32) -> f32 {
        let size = 16_384;
        let start = samples.len() / 2 - size / 2;
        let window = hann_window(size);
        let mut spectrum: Vec<Complex> = (0..size)
            .map(|index| Complex::new(samples[start + index] * window[index], 0.0))
            .collect();
        forward(&mut spectrum);

        let peak = (1..size / 2)
            .max_by(|left, right| {
                spectrum[*left]
                    .magnitude()
                    .partial_cmp(&spectrum[*right].magnitude())
                    .unwrap()
            })
            .unwrap();

        let left = spectrum[peak - 1].magnitude();
        let center = spectrum[peak].magnitude();
        let right = spectrum[peak + 1].magnitude();
        let correction = 0.5 * (left - right) / (left - 2.0 * center + right);
        (peak as f32 + correction) * sample_rate / size as f32
    }

    #[test]
    fn an_octave_up_doubles_the_fundamental() {
        let sample_rate = 48_000.0;
        let input = sine(440.0, sample_rate, 96_000);
        let output = shift_pitch(
            &input,
            &PitchOptions {
                sample_rate,
                semitones: 12.0,
                ..Default::default()
            },
        );

        assert_eq!(output.len(), input.len());
        let measured = dominant_frequency(&output, sample_rate);
        assert!(
            (measured - 880.0).abs() < 10.0,
            "expected 880 Hz, measured {measured}"
        );
    }

    #[test]
    fn an_octave_down_halves_the_fundamental() {
        let sample_rate = 48_000.0;
        let input = sine(440.0, sample_rate, 96_000);
        let output = shift_pitch(
            &input,
            &PitchOptions {
                sample_rate,
                semitones: -12.0,
                ..Default::default()
            },
        );

        let measured = dominant_frequency(&output, sample_rate);
        assert!(
            (measured - 220.0).abs() < 6.0,
            "expected 220 Hz, measured {measured}"
        );
    }

    #[test]
    fn a_fifth_up_matches_the_equal_tempered_ratio() {
        let sample_rate = 48_000.0;
        let input = sine(220.0, sample_rate, 96_000);
        let output = shift_pitch(
            &input,
            &PitchOptions {
                sample_rate,
                semitones: 7.0,
                ..Default::default()
            },
        );

        let expected = 220.0 * semitones_to_ratio(7.0);
        let measured = dominant_frequency(&output, sample_rate);
        assert!(
            (measured - expected).abs() < 8.0,
            "expected {expected} Hz, measured {measured}"
        );
    }

    #[test]
    fn duration_is_preserved_and_output_is_finite() {
        let sample_rate = 48_000.0;
        let input = sine(300.0, sample_rate, 50_000);
        for semitones in [-12.0, -5.0, 0.5, 7.0, 12.0] {
            let output = shift_pitch(
                &input,
                &PitchOptions {
                    sample_rate,
                    semitones,
                    ..Default::default()
                },
            );
            assert_eq!(output.len(), input.len(), "length changed at {semitones}");
            assert!(output.iter().all(|value| value.is_finite()));
            assert!(output.iter().all(|value| value.abs() < 4.0));
        }
    }

    #[test]
    fn a_zero_shift_returns_the_input_untouched() {
        let input = sine(440.0, 48_000.0, 8_192);
        let output = shift_pitch(&input, &PitchOptions::default());
        assert_eq!(input, output);
    }

    #[test]
    fn formant_shift_alone_keeps_the_fundamental() {
        let sample_rate = 48_000.0;
        let input = sine(220.0, sample_rate, 96_000);
        let output = shift_pitch(
            &input,
            &PitchOptions {
                sample_rate,
                semitones: 0.0,
                formant_semitones: 5.0,
                ..Default::default()
            },
        );

        let measured = dominant_frequency(&output, sample_rate);
        assert!(
            (measured - 220.0).abs() < 6.0,
            "fundamental drifted to {measured}"
        );
    }
}
