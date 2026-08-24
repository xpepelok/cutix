#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BiquadKind {
    LowShelf,
    Peaking,
    HighShelf,
    LowPass,
    HighPass,
}

#[derive(Clone, Copy, Debug)]
pub struct BiquadCoefficients {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl BiquadCoefficients {
    pub fn identity() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }

    pub fn design(
        kind: BiquadKind,
        sample_rate: f32,
        frequency: f32,
        q: f32,
        gain_db: f32,
    ) -> Self {
        if sample_rate <= 0.0 {
            return Self::identity();
        }

        let nyquist = sample_rate * 0.5;
        let frequency = frequency.clamp(10.0, nyquist * 0.98);
        let q = q.clamp(0.05, 20.0);
        let omega = std::f32::consts::TAU * frequency / sample_rate;
        let (sin_omega, cos_omega) = omega.sin_cos();
        let amplitude = 10f32.powf(gain_db / 40.0);

        let (b0, b1, b2, a0, a1, a2) = match kind {
            BiquadKind::Peaking => {
                let alpha = sin_omega / (2.0 * q);
                (
                    1.0 + alpha * amplitude,
                    -2.0 * cos_omega,
                    1.0 - alpha * amplitude,
                    1.0 + alpha / amplitude,
                    -2.0 * cos_omega,
                    1.0 - alpha / amplitude,
                )
            }
            BiquadKind::LowShelf => {
                let alpha = sin_omega / 2.0
                    * ((amplitude + 1.0 / amplitude) * (1.0 / q - 1.0) + 2.0)
                        .max(0.0)
                        .sqrt();
                let two_sqrt_a_alpha = 2.0 * amplitude.sqrt() * alpha;
                (
                    amplitude
                        * ((amplitude + 1.0) - (amplitude - 1.0) * cos_omega + two_sqrt_a_alpha),
                    2.0 * amplitude * ((amplitude - 1.0) - (amplitude + 1.0) * cos_omega),
                    amplitude
                        * ((amplitude + 1.0) - (amplitude - 1.0) * cos_omega - two_sqrt_a_alpha),
                    (amplitude + 1.0) + (amplitude - 1.0) * cos_omega + two_sqrt_a_alpha,
                    -2.0 * ((amplitude - 1.0) + (amplitude + 1.0) * cos_omega),
                    (amplitude + 1.0) + (amplitude - 1.0) * cos_omega - two_sqrt_a_alpha,
                )
            }
            BiquadKind::HighShelf => {
                let alpha = sin_omega / 2.0
                    * ((amplitude + 1.0 / amplitude) * (1.0 / q - 1.0) + 2.0)
                        .max(0.0)
                        .sqrt();
                let two_sqrt_a_alpha = 2.0 * amplitude.sqrt() * alpha;
                (
                    amplitude
                        * ((amplitude + 1.0) + (amplitude - 1.0) * cos_omega + two_sqrt_a_alpha),
                    -2.0 * amplitude * ((amplitude - 1.0) + (amplitude + 1.0) * cos_omega),
                    amplitude
                        * ((amplitude + 1.0) + (amplitude - 1.0) * cos_omega - two_sqrt_a_alpha),
                    (amplitude + 1.0) - (amplitude - 1.0) * cos_omega + two_sqrt_a_alpha,
                    2.0 * ((amplitude - 1.0) - (amplitude + 1.0) * cos_omega),
                    (amplitude + 1.0) - (amplitude - 1.0) * cos_omega - two_sqrt_a_alpha,
                )
            }
            BiquadKind::LowPass => {
                let alpha = sin_omega / (2.0 * q);
                let base = (1.0 - cos_omega) / 2.0;
                (
                    base,
                    1.0 - cos_omega,
                    base,
                    1.0 + alpha,
                    -2.0 * cos_omega,
                    1.0 - alpha,
                )
            }
            BiquadKind::HighPass => {
                let alpha = sin_omega / (2.0 * q);
                let base = (1.0 + cos_omega) / 2.0;
                (
                    base,
                    -(1.0 + cos_omega),
                    base,
                    1.0 + alpha,
                    -2.0 * cos_omega,
                    1.0 - alpha,
                )
            }
        };

        if a0.abs() < f32::EPSILON {
            return Self::identity();
        }

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    pub fn magnitude_at(&self, sample_rate: f32, frequency: f32) -> f32 {
        let omega = std::f32::consts::TAU * frequency / sample_rate;
        let (sin1, cos1) = omega.sin_cos();
        let (sin2, cos2) = (2.0 * omega).sin_cos();

        let num_re = self.b0 + self.b1 * cos1 + self.b2 * cos2;
        let num_im = -(self.b1 * sin1 + self.b2 * sin2);
        let den_re = 1.0 + self.a1 * cos1 + self.a2 * cos2;
        let den_im = -(self.a1 * sin1 + self.a2 * sin2);

        let numerator = (num_re * num_re + num_im * num_im).sqrt();
        let denominator = (den_re * den_re + den_im * den_im).sqrt();
        if denominator < f32::EPSILON {
            return 0.0;
        }
        numerator / denominator
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    coefficients: BiquadCoefficients,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    pub fn new(coefficients: BiquadCoefficients) -> Self {
        Self {
            coefficients,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    pub fn process_sample(&mut self, input: f32) -> f32 {
        let c = self.coefficients;
        let output =
            c.b0 * input + c.b1 * self.x1 + c.b2 * self.x2 - c.a1 * self.y1 - c.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;
        output
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            *sample = self.process_sample(*sample);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peaking_boost_raises_only_its_band() {
        let sample_rate = 48_000.0;
        let coefficients =
            BiquadCoefficients::design(BiquadKind::Peaking, sample_rate, 1_000.0, 1.0, 12.0);

        let at_center = coefficients.magnitude_at(sample_rate, 1_000.0);
        let far_below = coefficients.magnitude_at(sample_rate, 50.0);
        let far_above = coefficients.magnitude_at(sample_rate, 15_000.0);

        assert!((20.0 * at_center.log10() - 12.0).abs() < 0.2);
        assert!(20.0 * far_below.log10() < 1.0);
        assert!(20.0 * far_above.log10() < 1.0);
    }

    #[test]
    fn low_shelf_lifts_the_bottom_end() {
        let sample_rate = 48_000.0;
        let coefficients =
            BiquadCoefficients::design(BiquadKind::LowShelf, sample_rate, 200.0, 0.707, 9.0);

        let dc = 20.0 * coefficients.magnitude_at(sample_rate, 1.0).log10();
        let top = 20.0 * coefficients.magnitude_at(sample_rate, 15_000.0).log10();

        assert!((dc - 9.0).abs() < 0.3, "dc gain was {dc}");
        assert!(top.abs() < 0.3, "top gain was {top}");
    }

    #[test]
    fn filter_is_stable_over_a_long_signal() {
        let coefficients =
            BiquadCoefficients::design(BiquadKind::Peaking, 48_000.0, 1_000.0, 1.0, 12.0);
        let mut filter = Biquad::new(coefficients);
        let mut samples: Vec<f32> = (0..48_000)
            .map(|index| (index as f32 * 0.05).sin() * 0.5)
            .collect();
        filter.process(&mut samples);

        assert!(samples.iter().all(|value| value.is_finite()));
        assert!(samples.iter().all(|value| value.abs() < 8.0));
    }
}
