use crate::reframe::Point;
use crate::stabilize::phase_correlate;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Region {
    pub fn center(self) -> Point {
        Point {
            x: self.x + self.width / 2.0,
            y: self.y + self.height / 2.0,
        }
    }

    pub fn clamped(self, frame_width: f32, frame_height: f32) -> Self {
        let width = self.width.min(frame_width);
        let height = self.height.min(frame_height);
        Self {
            x: self.x.clamp(0.0, (frame_width - width).max(0.0)),
            y: self.y.clamp(0.0, (frame_height - height).max(0.0)),
            width,
            height,
        }
    }
}

pub struct TrackOptions {
    pub search_margin: f32,
    pub max_step: f32,
}

impl Default for TrackOptions {
    fn default() -> Self {
        Self {
            search_margin: 0.5,
            max_step: 64.0,
        }
    }
}

fn crop_projections(
    luma: &[f32],
    frame_width: usize,
    frame_height: usize,
    region: Region,
) -> (Vec<f32>, Vec<f32>) {
    let left = region.x.max(0.0) as usize;
    let top = region.y.max(0.0) as usize;
    let right = ((region.x + region.width) as usize).min(frame_width);
    let bottom = ((region.y + region.height) as usize).min(frame_height);

    if right <= left || bottom <= top {
        return (Vec::new(), Vec::new());
    }

    let mut columns = vec![0.0; right - left];
    let mut rows = vec![0.0; bottom - top];

    for row in top..bottom {
        for column in left..right {
            let value = luma[row * frame_width + column];
            columns[column - left] += value;
            rows[row - top] += value;
        }
    }

    subtract_mean(&mut columns);
    subtract_mean(&mut rows);
    (columns, rows)
}

fn subtract_mean(values: &mut [f32]) {
    if values.is_empty() {
        return;
    }
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    for value in values.iter_mut() {
        *value -= mean;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackStep {
    pub region: Region,
    pub confidence: f32,
}

fn normalized_correlation(reference: &[f32], target: &[f32]) -> f32 {
    let length = reference.len().min(target.len());
    if length == 0 {
        return 0.0;
    }

    let mut dot = 0.0;
    let mut reference_energy = 0.0;
    let mut target_energy = 0.0;
    for index in 0..length {
        dot += reference[index] * target[index];
        reference_energy += reference[index] * reference[index];
        target_energy += target[index] * target[index];
    }

    let denominator = (reference_energy * target_energy).sqrt();
    if denominator <= f32::EPSILON {
        return 0.0;
    }
    (dot / denominator).clamp(-1.0, 1.0)
}

pub fn track_region(
    previous: &[f32],
    current: &[f32],
    frame_width: usize,
    frame_height: usize,
    region: Region,
    options: &TrackOptions,
) -> Region {
    track_region_scored(
        previous,
        current,
        frame_width,
        frame_height,
        region,
        options,
    )
    .region
}

pub fn track_region_scored(
    previous: &[f32],
    current: &[f32],
    frame_width: usize,
    frame_height: usize,
    region: Region,
    options: &TrackOptions,
) -> TrackStep {
    if frame_width == 0 || frame_height == 0 {
        return TrackStep {
            region,
            confidence: 0.0,
        };
    }

    let margin_x = region.width * options.search_margin;
    let margin_y = region.height * options.search_margin;
    let search = Region {
        x: region.x - margin_x,
        y: region.y - margin_y,
        width: region.width + margin_x * 2.0,
        height: region.height + margin_y * 2.0,
    }
    .clamped(frame_width as f32, frame_height as f32);

    let (previous_columns, previous_rows) =
        crop_projections(previous, frame_width, frame_height, search);
    let (current_columns, current_rows) =
        crop_projections(current, frame_width, frame_height, search);

    if previous_columns.is_empty() || current_columns.is_empty() {
        return TrackStep {
            region,
            confidence: 0.0,
        };
    }

    let dx = phase_correlate(&previous_columns, &current_columns)
        .clamp(-options.max_step, options.max_step);
    let dy =
        phase_correlate(&previous_rows, &current_rows).clamp(-options.max_step, options.max_step);

    let moved = Region {
        x: region.x + dx,
        y: region.y + dy,
        width: region.width,
        height: region.height,
    }
    .clamped(frame_width as f32, frame_height as f32);

    let (before_columns, before_rows) =
        crop_projections(previous, frame_width, frame_height, region);
    let (after_columns, after_rows) = crop_projections(current, frame_width, frame_height, moved);
    let confidence = normalized_correlation(&before_columns, &after_columns)
        .min(normalized_correlation(&before_rows, &after_rows))
        .max(0.0);

    TrackStep {
        region: moved,
        confidence,
    }
}

pub fn track_sequence_scored(
    frames: &[Vec<f32>],
    frame_width: usize,
    frame_height: usize,
    start: Region,
    options: &TrackOptions,
) -> Vec<TrackStep> {
    let mut path = Vec::with_capacity(frames.len());
    let mut current = start.clamped(frame_width as f32, frame_height as f32);
    path.push(TrackStep {
        region: current,
        confidence: 1.0,
    });

    for pair in frames.windows(2) {
        let step = track_region_scored(
            &pair[0],
            &pair[1],
            frame_width,
            frame_height,
            current,
            options,
        );
        current = step.region;
        path.push(step);
    }
    path
}

pub fn track_sequence(
    frames: &[Vec<f32>],
    frame_width: usize,
    frame_height: usize,
    start: Region,
    options: &TrackOptions,
) -> Vec<Region> {
    let mut path = Vec::with_capacity(frames.len());
    let mut current = start.clamped(frame_width as f32, frame_height as f32);
    path.push(current);

    for pair in frames.windows(2) {
        current = track_region(
            &pair[0],
            &pair[1],
            frame_width,
            frame_height,
            current,
            options,
        );
        path.push(current);
    }
    path
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

    fn frame(width: usize, height: usize, offset_x: i32, offset_y: i32) -> Vec<f32> {
        let mut data = vec![0.0; width * height];
        for row in 0..height {
            for column in 0..width {
                let x = column as i32 - offset_x;
                let y = row as i32 - offset_y;
                data[row * width + column] = if x >= 0 && y >= 0 {
                    texture_value(x, y)
                } else {
                    0.0
                };
            }
        }
        data
    }

    #[test]
    fn region_reports_its_centre() {
        let region = Region {
            x: 10.0,
            y: 20.0,
            width: 40.0,
            height: 60.0,
        };
        assert_eq!(region.center(), Point { x: 30.0, y: 50.0 });
    }

    #[test]
    fn clamping_keeps_the_region_inside() {
        let region = Region {
            x: -20.0,
            y: 300.0,
            width: 50.0,
            height: 50.0,
        }
        .clamped(200.0, 200.0);
        assert_eq!(region.x, 0.0);
        assert_eq!(region.y, 150.0);
    }

    #[test]
    fn follows_a_moving_region() {
        let previous = frame(128, 128, 0, 0);
        let current = frame(128, 128, 6, 0);
        let start = Region {
            x: 40.0,
            y: 40.0,
            width: 40.0,
            height: 40.0,
        };
        let tracked = track_region(
            &previous,
            &current,
            128,
            128,
            start,
            &TrackOptions::default(),
        );
        assert!(
            (tracked.x - 46.0).abs() <= 2.0,
            "tracked x was {}",
            tracked.x
        );
    }

    #[test]
    fn stays_put_on_identical_frames() {
        let still = frame(128, 128, 0, 0);
        let start = Region {
            x: 40.0,
            y: 40.0,
            width: 40.0,
            height: 40.0,
        };
        let tracked = track_region(&still, &still, 128, 128, start, &TrackOptions::default());
        assert_eq!(tracked.x, start.x);
        assert_eq!(tracked.y, start.y);
    }

    #[test]
    fn sequence_length_matches_frame_count() {
        let frames: Vec<Vec<f32>> = (0..5).map(|index| frame(64, 64, index * 2, 0)).collect();
        let start = Region {
            x: 10.0,
            y: 10.0,
            width: 20.0,
            height: 20.0,
        };
        let path = track_sequence(&frames, 64, 64, start, &TrackOptions::default());
        assert_eq!(path.len(), frames.len());
    }

    #[test]
    fn confidence_is_high_when_the_content_is_followed() {
        let previous = frame(128, 128, 0, 0);
        let current = frame(128, 128, 6, 0);
        let start = Region {
            x: 40.0,
            y: 40.0,
            width: 40.0,
            height: 40.0,
        };
        let step = track_region_scored(
            &previous,
            &current,
            128,
            128,
            start,
            &TrackOptions::default(),
        );
        assert!(step.confidence > 0.5, "confidence was {}", step.confidence);
    }

    #[test]
    fn confidence_collapses_when_the_subject_is_lost() {
        let previous = frame(128, 128, 0, 0);
        let mut current = frame(128, 128, 0, 0);
        for row in 0..128 {
            for column in 0..128 {
                current[row * 128 + column] = texture_value(column as i32 + 977, row as i32 + 613);
            }
        }
        let start = Region {
            x: 40.0,
            y: 40.0,
            width: 40.0,
            height: 40.0,
        };
        let step = track_region_scored(
            &previous,
            &current,
            128,
            128,
            start,
            &TrackOptions::default(),
        );
        assert!(step.confidence < 0.5, "confidence was {}", step.confidence);
    }

    #[test]
    fn scored_sequence_length_matches_frame_count() {
        let frames: Vec<Vec<f32>> = (0..5).map(|index| frame(64, 64, index * 2, 0)).collect();
        let start = Region {
            x: 10.0,
            y: 10.0,
            width: 20.0,
            height: 20.0,
        };
        let path = track_sequence_scored(&frames, 64, 64, start, &TrackOptions::default());
        assert_eq!(path.len(), frames.len());
        assert_eq!(path[0].confidence, 1.0);
    }

    #[test]
    fn step_is_limited() {
        let previous = frame(128, 128, 0, 0);
        let current = frame(128, 128, 100, 0);
        let options = TrackOptions {
            max_step: 8.0,
            ..TrackOptions::default()
        };
        let start = Region {
            x: 40.0,
            y: 40.0,
            width: 40.0,
            height: 40.0,
        };
        let tracked = track_region(&previous, &current, 128, 128, start, &options);
        assert!((tracked.x - start.x).abs() <= 8.0 + 1e-6);
    }
}
