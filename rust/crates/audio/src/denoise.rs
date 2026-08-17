use dsp::fft::{forward, hann_window, inverse, Complex};

pub struct DenoiseOptions {
    pub frame_size: usize,
    pub strength: f32,
    pub noise_floor: f32,
    pub noise_percentile: f32,
}

impl Default for DenoiseOptions {
    fn default() -> Self {
        Self {
            frame_size: 1024,
            strength: 1.5,
            noise_floor: 0.08,
            noise_percentile: 0.15,
        }
    }
}

pub fn estimate_noise_profile(samples: &[f32], options: &DenoiseOptions) -> Vec<f32> {
    let size = options.frame_size;
    let hop = size / 2;
    let bins = size / 2 + 1;
    if samples.len() < size {
        return vec![0.0; bins];
    }

    let window = hann_window(size);
    let mut per_bin: Vec<Vec<f32>> = vec![Vec::new(); bins];

    let mut start = 0;
    while start + size <= samples.len() {
        let mut spectrum: Vec<Complex> = (0..size)
            .map(|index| Complex::new(samples[start + index] * window[index], 0.0))
            .collect();
        forward(&mut spectrum);

        for bin in 0..bins {
            per_bin[bin].push(spectrum[bin].magnitude());
        }
        start += hop;
    }

    if per_bin[0].is_empty() {
        return vec![0.0; bins];
    }

    let percentile = options.noise_percentile.clamp(0.01, 0.9);
    per_bin
        .into_iter()
        .map(|mut magnitudes| {
            magnitudes.sort_by(|left, right| left.partial_cmp(right).unwrap());
            let index = ((magnitudes.len() as f32 - 1.0) * percentile).round() as usize;
            magnitudes[index.min(magnitudes.len() - 1)]
        })
        .collect()
}

pub fn denoise(samples: &[f32], options: &DenoiseOptions) -> Vec<f32> {
    let size = options.frame_size;
    let hop = size / 2;
    if samples.len() < size || !dsp::fft::is_power_of_two(size) {
        return samples.to_vec();
    }

    let profile = estimate_noise_profile(samples, options);
    let window = hann_window(size);
    let mut output = vec![0.0; samples.len()];
    let mut weights = vec![0.0; samples.len()];
    let floor = options.noise_floor.clamp(0.0, 1.0);

    let mut start = 0;
    while start + size <= samples.len() {
        let mut spectrum: Vec<Complex> = (0..size)
            .map(|index| Complex::new(samples[start + index] * window[index], 0.0))
            .collect();
        forward(&mut spectrum);

        for bin in 0..=size / 2 {
            let magnitude = spectrum[bin].magnitude();
            if magnitude <= f32::EPSILON {
                continue;
            }
            let subtracted = magnitude - options.strength * profile[bin];
            let target = subtracted.max(floor * magnitude);
            let gain = target / magnitude;

            spectrum[bin].re *= gain;
            spectrum[bin].im *= gain;
            if bin > 0 && bin < size / 2 {
                let mirror = size - bin;
                spectrum[mirror].re *= gain;
                spectrum[mirror].im *= gain;
            }
        }

        inverse(&mut spectrum);

        for index in 0..size {
            output[start + index] += spectrum[index].re * window[index];
            weights[start + index] += window[index] * window[index];
        }
        start += hop;
    }

    let steady = weights
        .iter()
        .copied()
        .fold(0.0_f32, f32::max)
        .max(f32::EPSILON);
    let minimum = steady * 0.5;

    for index in 0..output.len() {
        if weights[index] >= minimum {
            output[index] /= weights[index];
        } else {
            output[index] = samples[index];
        }
    }
    output
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|value| value * value).sum::<f32>() / samples.len() as f32).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pseudo_noise(length: usize, amplitude: f32) -> Vec<f32> {
        let mut state = 0x2545_f491_u32;
        (0..length)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                let unit = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
                unit * amplitude
            })
            .collect()
    }

    fn tone(length: usize, sample_rate: f32, frequency: f32, amplitude: f32) -> Vec<f32> {
        (0..length)
            .map(|index| {
                let phase = std::f32::consts::TAU * frequency * index as f32 / sample_rate;
                phase.sin() * amplitude
            })
            .collect()
    }

    #[test]
    fn reduces_noise_in_silence() {
        let noise = pseudo_noise(48_000, 0.1);
        let cleaned = denoise(&noise, &DenoiseOptions::default());
        assert!(
            rms(&cleaned) < rms(&noise) * 0.8,
            "noise not reduced: {} vs {}",
            rms(&cleaned),
            rms(&noise)
        );
    }

    fn bursts(length: usize, sample_rate: f32, frequency: f32, amplitude: f32) -> Vec<f32> {
        let full = tone(length, sample_rate, frequency, amplitude);
        let period = (sample_rate * 0.5) as usize;
        full.into_iter()
            .enumerate()
            .map(|(index, value)| {
                if (index / period) % 2 == 0 {
                    value
                } else {
                    0.0
                }
            })
            .collect()
    }

    #[test]
    fn keeps_the_signal_present() {
        let clean = bursts(48_000, 48_000.0, 440.0, 0.5);
        let noise = pseudo_noise(48_000, 0.05);
        let noisy: Vec<f32> = clean
            .iter()
            .zip(noise.iter())
            .map(|(signal, noise)| signal + noise)
            .collect();

        let cleaned = denoise(&noisy, &DenoiseOptions::default());
        assert!(
            rms(&cleaned) > rms(&clean) * 0.5,
            "signal was destroyed: {} vs {}",
            rms(&cleaned),
            rms(&clean)
        );
    }

    #[test]
    fn improves_signal_to_noise_ratio() {
        let clean = bursts(48_000, 48_000.0, 440.0, 0.5);
        let noise = pseudo_noise(48_000, 0.15);
        let noisy: Vec<f32> = clean
            .iter()
            .zip(noise.iter())
            .map(|(signal, noise)| signal + noise)
            .collect();

        let cleaned = denoise(&noisy, &DenoiseOptions::default());

        let error_before = rms(&noisy
            .iter()
            .zip(clean.iter())
            .map(|(value, reference)| value - reference)
            .collect::<Vec<_>>());
        let error_after = rms(&cleaned
            .iter()
            .zip(clean.iter())
            .map(|(value, reference)| value - reference)
            .collect::<Vec<_>>());

        assert!(
            error_after < error_before,
            "residual grew: {error_after} vs {error_before}"
        );
    }

    #[test]
    fn returns_input_when_too_short() {
        let samples = vec![0.2; 16];
        assert_eq!(denoise(&samples, &DenoiseOptions::default()), samples);
    }

    #[test]
    fn output_length_matches_input() {
        let samples = pseudo_noise(20_000, 0.1);
        assert_eq!(denoise(&samples, &DenoiseOptions::default()).len(), 20_000);
    }
}
