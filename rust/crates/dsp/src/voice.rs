use crate::biquad::{Biquad, BiquadCoefficients, BiquadKind};
use crate::equalizer::{equalize, EqualizerBand, EqualizerOptions};
use crate::pitch::{shift_pitch, PitchOptions};
use crate::reverb::{apply_reverb, ReverbOptions, ReverbPreset};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoicePreset {
    Deep,
    Chipmunk,
    Robot,
    Telephone,
    Cave,
}

pub const VOICE_PRESETS: [VoicePreset; 5] = [
    VoicePreset::Deep,
    VoicePreset::Chipmunk,
    VoicePreset::Robot,
    VoicePreset::Telephone,
    VoicePreset::Cave,
];

impl VoicePreset {
    pub fn id(self) -> &'static str {
        match self {
            VoicePreset::Deep => "deep",
            VoicePreset::Chipmunk => "chipmunk",
            VoicePreset::Robot => "robot",
            VoicePreset::Telephone => "telephone",
            VoicePreset::Cave => "cave",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        VOICE_PRESETS.into_iter().find(|preset| preset.id() == id)
    }
}

fn filter(
    samples: &mut [f32],
    kind: BiquadKind,
    sample_rate: f32,
    frequency: f32,
    q: f32,
    gain_db: f32,
) {
    let mut biquad = Biquad::new(BiquadCoefficients::design(
        kind,
        sample_rate,
        frequency,
        q,
        gain_db,
    ));
    biquad.process(samples);
}

fn normalize(samples: &mut [f32], ceiling: f32) {
    let peak = samples
        .iter()
        .fold(0.0f32, |max, value| max.max(value.abs()));
    if peak > ceiling && peak > 0.0 {
        let scale = ceiling / peak;
        for value in samples.iter_mut() {
            *value *= scale;
        }
    }
}

pub fn apply_voice_preset(samples: &[f32], preset: VoicePreset, sample_rate: f32) -> Vec<f32> {
    if samples.is_empty() {
        return Vec::new();
    }

    let mut output = match preset {
        VoicePreset::Deep => shift_pitch(
            samples,
            &PitchOptions {
                sample_rate,
                semitones: -5.0,
                formant_semitones: -3.0,
                robotize: false,
            },
        ),
        VoicePreset::Chipmunk => shift_pitch(
            samples,
            &PitchOptions {
                sample_rate,
                semitones: 7.0,
                formant_semitones: 7.0,
                robotize: false,
            },
        ),
        VoicePreset::Robot => shift_pitch(
            samples,
            &PitchOptions {
                sample_rate,
                semitones: 0.0,
                formant_semitones: 0.0,
                robotize: true,
            },
        ),
        VoicePreset::Telephone => samples.to_vec(),
        VoicePreset::Cave => shift_pitch(
            samples,
            &PitchOptions {
                sample_rate,
                semitones: -2.0,
                formant_semitones: -1.0,
                robotize: false,
            },
        ),
    };

    match preset {
        VoicePreset::Deep => {
            output = equalize(
                &output,
                &EqualizerOptions {
                    sample_rate,
                    bands: vec![
                        EqualizerBand {
                            kind: BiquadKind::LowShelf,
                            frequency: 150.0,
                            q: 0.707,
                            gain_db: 5.0,
                        },
                        EqualizerBand {
                            kind: BiquadKind::HighShelf,
                            frequency: 6_000.0,
                            q: 0.707,
                            gain_db: -4.0,
                        },
                    ],
                },
            );
        }
        VoicePreset::Chipmunk => {
            output = equalize(
                &output,
                &EqualizerOptions {
                    sample_rate,
                    bands: vec![EqualizerBand {
                        kind: BiquadKind::LowShelf,
                        frequency: 200.0,
                        q: 0.707,
                        gain_db: -5.0,
                    }],
                },
            );
        }
        VoicePreset::Robot => {
            filter(
                &mut output,
                BiquadKind::HighPass,
                sample_rate,
                120.0,
                0.707,
                0.0,
            );
            filter(
                &mut output,
                BiquadKind::Peaking,
                sample_rate,
                1_800.0,
                1.2,
                5.0,
            );
        }
        VoicePreset::Telephone => {
            filter(
                &mut output,
                BiquadKind::HighPass,
                sample_rate,
                300.0,
                0.707,
                0.0,
            );
            filter(
                &mut output,
                BiquadKind::HighPass,
                sample_rate,
                300.0,
                0.707,
                0.0,
            );
            filter(
                &mut output,
                BiquadKind::LowPass,
                sample_rate,
                3_400.0,
                0.707,
                0.0,
            );
            filter(
                &mut output,
                BiquadKind::LowPass,
                sample_rate,
                3_400.0,
                0.707,
                0.0,
            );
            filter(
                &mut output,
                BiquadKind::Peaking,
                sample_rate,
                2_000.0,
                1.0,
                6.0,
            );
            for value in output.iter_mut() {
                *value = (*value * 1.8).tanh() * 0.6;
            }
        }
        VoicePreset::Cave => {
            output = apply_reverb(
                &output,
                &ReverbOptions::from_preset(ReverbPreset::Hall, sample_rate, 0.55),
            );
        }
    }

    normalize(&mut output, 0.98);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fft::{forward, hann_window, Complex};

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
        peak as f32 * sample_rate / size as f32
    }

    fn band_energy(samples: &[f32], sample_rate: f32, low: f32, high: f32) -> f32 {
        let size = 16_384;
        let start = samples.len() / 2 - size / 2;
        let window = hann_window(size);
        let mut spectrum: Vec<Complex> = (0..size)
            .map(|index| Complex::new(samples[start + index] * window[index], 0.0))
            .collect();
        forward(&mut spectrum);
        (1..size / 2)
            .filter(|bin| {
                let frequency = *bin as f32 * sample_rate / size as f32;
                frequency >= low && frequency <= high
            })
            .map(|bin| spectrum[bin].magnitude().powi(2))
            .sum()
    }

    #[test]
    fn deep_lowers_the_fundamental() {
        let sample_rate = 48_000.0;
        let input = sine(400.0, sample_rate, 96_000);
        let output = apply_voice_preset(&input, VoicePreset::Deep, sample_rate);
        let measured = dominant_frequency(&output, sample_rate);
        let expected = 400.0 * 2f32.powf(-5.0 / 12.0);
        assert!(
            (measured - expected).abs() < 12.0,
            "expected {expected} Hz, measured {measured}"
        );
    }

    #[test]
    fn chipmunk_raises_the_fundamental() {
        let sample_rate = 48_000.0;
        let input = sine(400.0, sample_rate, 96_000);
        let output = apply_voice_preset(&input, VoicePreset::Chipmunk, sample_rate);
        let measured = dominant_frequency(&output, sample_rate);
        let expected = 400.0 * 2f32.powf(7.0 / 12.0);
        assert!(
            (measured - expected).abs() < 15.0,
            "expected {expected} Hz, measured {measured}"
        );
    }

    #[test]
    fn telephone_strips_energy_outside_the_speech_band() {
        let sample_rate = 48_000.0;
        let low = sine(100.0, sample_rate, 96_000);
        let mid = sine(1_500.0, sample_rate, 96_000);
        let high = sine(9_000.0, sample_rate, 96_000);
        let input: Vec<f32> = (0..96_000)
            .map(|index| (low[index] + mid[index] + high[index]) / 3.0)
            .collect();

        let output = apply_voice_preset(&input, VoicePreset::Telephone, sample_rate);

        let low_ratio = band_energy(&output, sample_rate, 80.0, 130.0)
            / band_energy(&input, sample_rate, 80.0, 130.0);
        let high_ratio = band_energy(&output, sample_rate, 8_500.0, 9_500.0)
            / band_energy(&input, sample_rate, 8_500.0, 9_500.0);
        let mid_ratio = band_energy(&output, sample_rate, 1_400.0, 1_600.0)
            / band_energy(&input, sample_rate, 1_400.0, 1_600.0);

        assert!(
            10.0 * low_ratio.log10() < -20.0,
            "100 Hz only dropped {} dB",
            10.0 * low_ratio.log10()
        );
        assert!(
            10.0 * high_ratio.log10() < -20.0,
            "9 kHz only dropped {} dB",
            10.0 * high_ratio.log10()
        );
        assert!(
            10.0 * mid_ratio.log10() > -6.0,
            "1.5 kHz dropped {} dB",
            10.0 * mid_ratio.log10()
        );
    }

    #[test]
    fn every_preset_produces_finite_bounded_audio() {
        let sample_rate = 48_000.0;
        let input = sine(220.0, sample_rate, 24_000);
        for preset in VOICE_PRESETS {
            let output = apply_voice_preset(&input, preset, sample_rate);
            assert!(!output.is_empty(), "{} produced nothing", preset.id());
            assert!(
                output.iter().all(|value| value.is_finite()),
                "{} produced non-finite samples",
                preset.id()
            );
            assert!(
                output.iter().all(|value| value.abs() <= 1.0001),
                "{} clipped",
                preset.id()
            );
        }
    }

    #[test]
    fn preset_ids_round_trip() {
        for preset in VOICE_PRESETS {
            assert_eq!(VoicePreset::from_id(preset.id()), Some(preset));
        }
        assert_eq!(VoicePreset::from_id("nope"), None);
    }
}
