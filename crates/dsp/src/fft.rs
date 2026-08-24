#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Complex {
    pub re: f32,
    pub im: f32,
}

impl Complex {
    pub fn new(re: f32, im: f32) -> Self {
        Self { re, im }
    }

    pub fn magnitude(self) -> f32 {
        (self.re * self.re + self.im * self.im).sqrt()
    }

    fn mul(self, other: Self) -> Self {
        Self {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }

    fn add(self, other: Self) -> Self {
        Self {
            re: self.re + other.re,
            im: self.im + other.im,
        }
    }

    fn sub(self, other: Self) -> Self {
        Self {
            re: self.re - other.re,
            im: self.im - other.im,
        }
    }
}

pub fn is_power_of_two(value: usize) -> bool {
    value != 0 && value & (value - 1) == 0
}

fn transform(buffer: &mut [Complex], inverse: bool) {
    let length = buffer.len();
    if length <= 1 {
        return;
    }
    debug_assert!(is_power_of_two(length), "fft length must be a power of two");

    let mut target = 0;
    for source in 1..length {
        let mut bit = length >> 1;
        while target & bit != 0 {
            target ^= bit;
            bit >>= 1;
        }
        target |= bit;
        if source < target {
            buffer.swap(source, target);
        }
    }

    let mut size = 2;
    while size <= length {
        let sign = if inverse { 1.0 } else { -1.0 };
        let angle = sign * std::f32::consts::TAU / size as f32;
        let step = Complex::new(angle.cos(), angle.sin());

        for start in (0..length).step_by(size) {
            let mut factor = Complex::new(1.0, 0.0);
            for offset in 0..size / 2 {
                let even = buffer[start + offset];
                let odd = buffer[start + offset + size / 2].mul(factor);
                buffer[start + offset] = even.add(odd);
                buffer[start + offset + size / 2] = even.sub(odd);
                factor = factor.mul(step);
            }
        }
        size <<= 1;
    }

    if inverse {
        let scale = 1.0 / length as f32;
        for value in buffer.iter_mut() {
            value.re *= scale;
            value.im *= scale;
        }
    }
}

pub fn forward(buffer: &mut [Complex]) {
    transform(buffer, false);
}

pub fn inverse(buffer: &mut [Complex]) {
    transform(buffer, true);
}

pub fn hann_window(size: usize) -> Vec<f32> {
    (0..size)
        .map(|index| {
            let ratio = index as f32 / size as f32;
            0.5 - 0.5 * (std::f32::consts::TAU * ratio).cos()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_signal() {
        let original: Vec<f32> = (0..64)
            .map(|index| (index as f32 * 0.3).sin() * 0.7)
            .collect();
        let mut spectrum: Vec<Complex> = original
            .iter()
            .map(|value| Complex::new(*value, 0.0))
            .collect();

        forward(&mut spectrum);
        inverse(&mut spectrum);

        for (index, value) in original.iter().enumerate() {
            assert!(
                (spectrum[index].re - value).abs() < 1e-3,
                "sample {index} drifted"
            );
        }
    }

    #[test]
    fn finds_the_dominant_bin_of_a_sine() {
        let size = 128;
        let bin = 8;
        let mut spectrum: Vec<Complex> = (0..size)
            .map(|index| {
                let phase = std::f32::consts::TAU * bin as f32 * index as f32 / size as f32;
                Complex::new(phase.sin(), 0.0)
            })
            .collect();

        forward(&mut spectrum);

        let loudest = (1..size / 2)
            .max_by(|left, right| {
                spectrum[*left]
                    .magnitude()
                    .partial_cmp(&spectrum[*right].magnitude())
                    .unwrap()
            })
            .unwrap();
        assert_eq!(loudest, bin);
    }

    #[test]
    fn hann_window_starts_and_ends_at_zero() {
        let window = hann_window(32);
        assert!(window[0].abs() < 1e-6);
        assert!(window[16] > 0.99);
    }
}
