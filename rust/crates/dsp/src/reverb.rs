const COMB_DELAYS_44K: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_DELAYS_44K: [usize; 4] = [556, 441, 341, 225];
const REFERENCE_RATE: f32 = 44_100.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReverbPreset {
    Room,
    Hall,
    Plate,
}

#[derive(Clone, Copy, Debug)]
pub struct ReverbOptions {
    pub sample_rate: f32,
    pub room_size: f32,
    pub damping: f32,
    pub wet: f32,
    pub width_scale: f32,
    pub pre_delay_seconds: f32,
}

impl Default for ReverbOptions {
    fn default() -> Self {
        Self {
            sample_rate: 48_000.0,
            room_size: 0.7,
            damping: 0.4,
            wet: 0.3,
            width_scale: 1.0,
            pre_delay_seconds: 0.0,
        }
    }
}

impl ReverbOptions {
    pub fn from_preset(preset: ReverbPreset, sample_rate: f32, wet: f32) -> Self {
        let base = Self {
            sample_rate,
            wet: wet.clamp(0.0, 1.0),
            ..Default::default()
        };
        match preset {
            ReverbPreset::Room => Self {
                room_size: 0.55,
                damping: 0.55,
                width_scale: 0.7,
                pre_delay_seconds: 0.008,
                ..base
            },
            ReverbPreset::Hall => Self {
                room_size: 0.88,
                damping: 0.25,
                width_scale: 1.5,
                pre_delay_seconds: 0.035,
                ..base
            },
            ReverbPreset::Plate => Self {
                room_size: 0.75,
                damping: 0.12,
                width_scale: 0.9,
                pre_delay_seconds: 0.002,
                ..base
            },
        }
    }
}

struct Comb {
    buffer: Vec<f32>,
    index: usize,
    feedback: f32,
    damping: f32,
    store: f32,
}

impl Comb {
    fn new(length: usize, feedback: f32, damping: f32) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            index: 0,
            feedback,
            damping,
            store: 0.0,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.buffer[self.index];
        self.store = output * (1.0 - self.damping) + self.store * self.damping;
        self.buffer[self.index] = input + self.store * self.feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

struct Allpass {
    buffer: Vec<f32>,
    index: usize,
    feedback: f32,
}

impl Allpass {
    fn new(length: usize, feedback: f32) -> Self {
        Self {
            buffer: vec![0.0; length.max(1)],
            index: 0,
            feedback,
        }
    }

    fn process(&mut self, input: f32) -> f32 {
        let buffered = self.buffer[self.index];
        let output = -input + buffered;
        self.buffer[self.index] = input + buffered * self.feedback;
        self.index = (self.index + 1) % self.buffer.len();
        output
    }
}

pub fn reverb_tail_seconds(options: &ReverbOptions) -> f32 {
    let feedback = 0.28 + options.room_size * 0.7;
    let longest =
        COMB_DELAYS_44K[COMB_DELAYS_44K.len() - 1] as f32 * options.width_scale / REFERENCE_RATE;
    if feedback >= 0.999 {
        return 8.0;
    }
    let decays = (0.001f32).ln() / feedback.ln();
    (longest * decays + options.pre_delay_seconds).clamp(0.05, 8.0)
}

pub fn apply_reverb(samples: &[f32], options: &ReverbOptions) -> Vec<f32> {
    if samples.is_empty() || options.sample_rate <= 0.0 {
        return samples.to_vec();
    }

    let wet = options.wet.clamp(0.0, 1.0);
    if wet <= 0.0 {
        return samples.to_vec();
    }

    let rate_scale = options.sample_rate / REFERENCE_RATE * options.width_scale.max(0.1);
    let feedback = (0.28 + options.room_size * 0.7).clamp(0.0, 0.98);
    let damping = options.damping.clamp(0.0, 0.95);

    let mut combs: Vec<Comb> = COMB_DELAYS_44K
        .iter()
        .map(|delay| {
            Comb::new(
                (*delay as f32 * rate_scale).round() as usize,
                feedback,
                damping,
            )
        })
        .collect();
    let mut allpasses: Vec<Allpass> = ALLPASS_DELAYS_44K
        .iter()
        .map(|delay| Allpass::new((*delay as f32 * rate_scale).round() as usize, 0.5))
        .collect();

    let pre_delay = (options.pre_delay_seconds.max(0.0) * options.sample_rate).round() as usize;
    let tail = (reverb_tail_seconds(options) * options.sample_rate).round() as usize;
    let total = samples.len() + tail;

    let mut output = Vec::with_capacity(total);
    for index in 0..total {
        let dry = samples.get(index).copied().unwrap_or(0.0);
        let excitation = if index >= pre_delay {
            samples.get(index - pre_delay).copied().unwrap_or(0.0)
        } else {
            0.0
        };

        let mut wet_sample = 0.0;
        for comb in combs.iter_mut() {
            wet_sample += comb.process(excitation * 0.12);
        }
        for allpass in allpasses.iter_mut() {
            wet_sample = allpass.process(wet_sample);
        }

        output.push(dry * (1.0 - wet * 0.5) + wet_sample * wet);
    }

    let peak = output
        .iter()
        .fold(0.0f32, |max, value| max.max(value.abs()));
    if peak > 1.0 {
        let scale = 1.0 / peak;
        for value in output.iter_mut() {
            *value *= scale;
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn impulse(length: usize) -> Vec<f32> {
        let mut samples = vec![0.0f32; length];
        samples[0] = 1.0;
        samples
    }

    fn tail_length(samples: &[f32], threshold: f32) -> usize {
        samples
            .iter()
            .rposition(|value| value.abs() > threshold)
            .map(|index| index + 1)
            .unwrap_or(0)
    }

    #[test]
    fn reverb_extends_the_decay_tail() {
        let sample_rate = 48_000.0;
        let dry = impulse(4_800);
        let wet = apply_reverb(
            &dry,
            &ReverbOptions::from_preset(ReverbPreset::Hall, sample_rate, 0.6),
        );

        let dry_tail = tail_length(&dry, 0.001);
        let wet_tail = tail_length(&wet, 0.001);

        assert!(wet.len() > dry.len(), "output was not extended");
        assert!(
            wet_tail > dry_tail * 100,
            "tail only grew from {dry_tail} to {wet_tail} samples"
        );
        assert!(
            wet_tail as f32 / sample_rate > 0.5,
            "hall tail was only {} s",
            wet_tail as f32 / sample_rate
        );
    }

    #[test]
    fn a_hall_rings_longer_than_a_room() {
        let sample_rate = 48_000.0;
        let dry = impulse(2_400);
        let room = apply_reverb(
            &dry,
            &ReverbOptions::from_preset(ReverbPreset::Room, sample_rate, 0.6),
        );
        let hall = apply_reverb(
            &dry,
            &ReverbOptions::from_preset(ReverbPreset::Hall, sample_rate, 0.6),
        );

        let room_tail = tail_length(&room, 0.0005);
        let hall_tail = tail_length(&hall, 0.0005);
        assert!(
            hall_tail > room_tail,
            "hall {hall_tail} was not longer than room {room_tail}"
        );
    }

    #[test]
    fn output_stays_within_range_and_finite() {
        let sample_rate = 48_000.0;
        let input: Vec<f32> = (0..48_000)
            .map(|index| (index as f32 * 0.02).sin() * 0.8)
            .collect();
        let output = apply_reverb(
            &input,
            &ReverbOptions::from_preset(ReverbPreset::Plate, sample_rate, 1.0),
        );

        assert!(output.iter().all(|value| value.is_finite()));
        assert!(output.iter().all(|value| value.abs() <= 1.0001));
    }

    #[test]
    fn output_length_grows_by_the_preset_tail() {
        let sample_rate = 48_000.0;
        let input = impulse(48_000);
        for (preset, expected_seconds) in [
            (ReverbPreset::Room, 0.4426),
            (ReverbPreset::Plate, 1.0529),
            (ReverbPreset::Hall, 3.4947),
        ] {
            let options = ReverbOptions::from_preset(preset, sample_rate, 0.6);
            let output = apply_reverb(&input, &options);
            let grew = (output.len() - input.len()) as f32 / sample_rate;
            assert!(
                (grew - expected_seconds).abs() < 0.005,
                "{preset:?} grew by {grew} s, expected {expected_seconds} s"
            );
        }
    }

    #[test]
    fn zero_wet_returns_the_input() {
        let input = impulse(1_024);
        let output = apply_reverb(
            &input,
            &ReverbOptions {
                wet: 0.0,
                ..Default::default()
            },
        );
        assert_eq!(input, output);
    }
}
