use dsp::fft::{Complex, forward, inverse, is_power_of_two};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Shift {
    pub dx: f32,
    pub dy: f32,
}

pub struct StabilizeOptions {
    pub smoothing_radius: usize,
    pub max_correction: f32,
}

impl Default for StabilizeOptions {
    fn default() -> Self {
        Self {
            smoothing_radius: 12,
            max_correction: 0.1,
        }
    }
}

pub fn column_projection(luma: &[f32], width: usize, height: usize) -> Vec<f32> {
    let mut projection = vec![0.0; width];
    for row in 0..height {
        for column in 0..width {
            projection[column] += luma[row * width + column];
        }
    }
    normalize(&mut projection);
    projection
}

pub fn row_projection(luma: &[f32], width: usize, height: usize) -> Vec<f32> {
    let mut projection = vec![0.0; height];
    for row in 0..height {
        let mut sum = 0.0;
        for column in 0..width {
            sum += luma[row * width + column];
        }
        projection[row] = sum;
    }
    normalize(&mut projection);
    projection
}

fn normalize(values: &mut [f32]) {
    if values.is_empty() {
        return;
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    for value in values.iter_mut() {
        *value -= mean;
    }
}

fn next_power_of_two(value: usize) -> usize {
    let mut size = 1;
    while size < value {
        size <<= 1;
    }
    size
}

pub fn phase_correlate(reference: &[f32], target: &[f32]) -> f32 {
    let length = reference.len().min(target.len());
    if length < 2 {
        return 0.0;
    }

    let size = next_power_of_two(length * 2);
    debug_assert!(is_power_of_two(size));

    let mut left: Vec<Complex> = (0..size)
        .map(|index| Complex::new(*reference.get(index).unwrap_or(&0.0), 0.0))
        .collect();
    let mut right: Vec<Complex> = (0..size)
        .map(|index| Complex::new(*target.get(index).unwrap_or(&0.0), 0.0))
        .collect();

    forward(&mut left);
    forward(&mut right);

    let mut cross: Vec<Complex> = (0..size)
        .map(|index| {
            let a = left[index];
            let b = right[index];
            let product = Complex::new(a.re * b.re + a.im * b.im, a.im * b.re - a.re * b.im);
            let magnitude = product.magnitude();
            if magnitude <= f32::EPSILON {
                Complex::new(0.0, 0.0)
            } else {
                Complex::new(product.re / magnitude, product.im / magnitude)
            }
        })
        .collect();

    inverse(&mut cross);

    let mut best_index = 0;
    let mut best_value = f32::MIN;
    for (index, sample) in cross[..size].iter().enumerate() {
        if sample.re > best_value {
            best_value = sample.re;
            best_index = index;
        }
    }

    let raw = if best_index > size / 2 {
        best_index as f32 - size as f32
    } else {
        best_index as f32
    };
    -raw
}

pub fn estimate_shift(reference: &[f32], target: &[f32], width: usize, height: usize) -> Shift {
    if width == 0 || height == 0 {
        return Shift::default();
    }
    Shift {
        dx: phase_correlate(
            &column_projection(reference, width, height),
            &column_projection(target, width, height),
        ),
        dy: phase_correlate(
            &row_projection(reference, width, height),
            &row_projection(target, width, height),
        ),
    }
}

pub fn cumulative_trajectory(shifts: &[Shift]) -> Vec<Shift> {
    let mut trajectory = Vec::with_capacity(shifts.len());
    let mut current = Shift::default();
    for shift in shifts {
        current = Shift {
            dx: current.dx + shift.dx,
            dy: current.dy + shift.dy,
        };
        trajectory.push(current);
    }
    trajectory
}

pub fn smooth_trajectory(trajectory: &[Shift], radius: usize) -> Vec<Shift> {
    if trajectory.is_empty() || radius == 0 {
        return trajectory.to_vec();
    }

    (0..trajectory.len())
        .map(|index| {
            let start = index.saturating_sub(radius);
            let end = (index + radius + 1).min(trajectory.len());
            let window = &trajectory[start..end];
            let count = window.len() as f32;
            Shift {
                dx: window.iter().map(|shift| shift.dx).sum::<f32>() / count,
                dy: window.iter().map(|shift| shift.dy).sum::<f32>() / count,
            }
        })
        .collect()
}

pub fn stabilization_offsets(
    shifts: &[Shift],
    width: usize,
    height: usize,
    options: &StabilizeOptions,
) -> Vec<Shift> {
    let trajectory = cumulative_trajectory(shifts);
    let smoothed = smooth_trajectory(&trajectory, options.smoothing_radius);
    let limit_x = width as f32 * options.max_correction;
    let limit_y = height as f32 * options.max_correction;

    trajectory
        .iter()
        .zip(smoothed.iter())
        .map(|(actual, target)| Shift {
            dx: (target.dx - actual.dx).clamp(-limit_x, limit_x),
            dy: (target.dy - actual.dy).clamp(-limit_y, limit_y),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texture_value(x: i32, y: i32) -> f32 {
        let mut state = (x as u32).wrapping_mul(0x9e37_79b9) ^ (y as u32).wrapping_mul(0x85eb_ca6b);
        state ^= state >> 13;
        state = state.wrapping_mul(0xc2b2_ae35);
        state ^= state >> 16;
        state as f32 / u32::MAX as f32
    }

    fn textured_frame(width: usize, height: usize, offset_x: i32, offset_y: i32) -> Vec<f32> {
        let mut frame = vec![0.0; width * height];
        for row in 0..height {
            for column in 0..width {
                let x = column as i32 - offset_x;
                let y = row as i32 - offset_y;
                frame[row * width + column] = if x >= 0 && y >= 0 {
                    texture_value(x, y)
                } else {
                    0.0
                };
            }
        }
        frame
    }

    #[test]
    fn detects_no_shift_between_identical_frames() {
        let frame = textured_frame(64, 64, 0, 0);
        let shift = estimate_shift(&frame, &frame, 64, 64);
        assert_eq!(shift.dx, 0.0);
        assert_eq!(shift.dy, 0.0);
    }

    #[test]
    fn detects_horizontal_shift() {
        let reference = textured_frame(64, 64, 0, 0);
        let target = textured_frame(64, 64, 5, 0);
        let shift = estimate_shift(&reference, &target, 64, 64);
        assert!((shift.dx - 5.0).abs() <= 1.0, "dx was {}", shift.dx);
    }

    #[test]
    fn detects_vertical_shift() {
        let reference = textured_frame(64, 64, 0, 0);
        let target = textured_frame(64, 64, 0, 4);
        let shift = estimate_shift(&reference, &target, 64, 64);
        assert!((shift.dy - 4.0).abs() <= 1.0, "dy was {}", shift.dy);
    }

    #[test]
    fn accumulates_trajectory() {
        let shifts = vec![
            Shift { dx: 1.0, dy: 0.0 },
            Shift { dx: 1.0, dy: 2.0 },
            Shift { dx: -1.0, dy: 1.0 },
        ];
        let trajectory = cumulative_trajectory(&shifts);
        assert_eq!(trajectory[2].dx, 1.0);
        assert_eq!(trajectory[2].dy, 3.0);
    }

    #[test]
    fn smoothing_reduces_variation() {
        let shaky: Vec<Shift> = (0..40)
            .map(|index| Shift {
                dx: if index % 2 == 0 { 8.0 } else { -8.0 },
                dy: 0.0,
            })
            .collect();
        let trajectory = cumulative_trajectory(&shaky);
        let smoothed = smooth_trajectory(&trajectory, 6);

        let spread = |values: &[Shift]| {
            let max = values.iter().map(|s| s.dx).fold(f32::MIN, f32::max);
            let min = values.iter().map(|s| s.dx).fold(f32::MAX, f32::min);
            max - min
        };
        assert!(spread(&smoothed) < spread(&trajectory));
    }

    #[test]
    fn offsets_respect_the_crop_limit() {
        let shifts: Vec<Shift> = (0..30)
            .map(|index| Shift {
                dx: if index < 15 { 30.0 } else { -30.0 },
                dy: 0.0,
            })
            .collect();
        let options = StabilizeOptions::default();
        let offsets = stabilization_offsets(&shifts, 100, 100, &options);
        for offset in offsets {
            assert!(
                offset.dx.abs() <= 10.0 + 1e-6,
                "offset too big: {}",
                offset.dx
            );
        }
    }
}
