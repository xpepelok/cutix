use crate::stabilize::{Shift, smooth_trajectory};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CropWindow {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

pub struct ReframeOptions {
    pub target_aspect: f32,
    pub smoothing_radius: usize,
    pub zoom: f32,
}

impl Default for ReframeOptions {
    fn default() -> Self {
        Self {
            target_aspect: 9.0 / 16.0,
            smoothing_radius: 8,
            zoom: 1.0,
        }
    }
}

pub fn subject_center(alpha: &[u8], width: usize, height: usize) -> Option<Point> {
    if width == 0 || height == 0 || alpha.len() < width * height {
        return None;
    }

    let mut weight_total = 0.0_f64;
    let mut x_total = 0.0_f64;
    let mut y_total = 0.0_f64;

    for row in 0..height {
        for column in 0..width {
            let weight = alpha[row * width + column] as f64 / 255.0;
            if weight <= 0.0 {
                continue;
            }
            weight_total += weight;
            x_total += weight * column as f64;
            y_total += weight * row as f64;
        }
    }

    if weight_total <= f64::EPSILON {
        return None;
    }

    Some(Point {
        x: (x_total / weight_total) as f32,
        y: (y_total / weight_total) as f32,
    })
}

pub fn crop_size(
    source_width: f32,
    source_height: f32,
    target_aspect: f32,
    zoom: f32,
) -> (f32, f32) {
    let safe_zoom = zoom.max(1.0);
    let source_aspect = source_width / source_height;

    let (mut width, mut height) = if target_aspect < source_aspect {
        (source_height * target_aspect, source_height)
    } else {
        (source_width, source_width / target_aspect)
    };

    width /= safe_zoom;
    height /= safe_zoom;
    (width.min(source_width), height.min(source_height))
}

pub fn crop_window(
    center: Point,
    source_width: f32,
    source_height: f32,
    options: &ReframeOptions,
) -> CropWindow {
    let (width, height) = crop_size(
        source_width,
        source_height,
        options.target_aspect,
        options.zoom,
    );

    let x = (center.x - width / 2.0).clamp(0.0, (source_width - width).max(0.0));
    let y = (center.y - height / 2.0).clamp(0.0, (source_height - height).max(0.0));

    CropWindow {
        x,
        y,
        width,
        height,
    }
}

pub fn reframe_path(
    centers: &[Point],
    source_width: f32,
    source_height: f32,
    options: &ReframeOptions,
) -> Vec<CropWindow> {
    if centers.is_empty() {
        return Vec::new();
    }

    let as_shifts: Vec<Shift> = centers
        .iter()
        .map(|point| Shift {
            dx: point.x,
            dy: point.y,
        })
        .collect();
    let smoothed = smooth_trajectory(&as_shifts, options.smoothing_radius);

    smoothed
        .into_iter()
        .map(|shift| {
            crop_window(
                Point {
                    x: shift.dx,
                    y: shift.dy,
                },
                source_width,
                source_height,
                options,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blob(
        width: usize,
        height: usize,
        center_x: usize,
        center_y: usize,
        radius: usize,
    ) -> Vec<u8> {
        let mut alpha = vec![0u8; width * height];
        for row in 0..height {
            for column in 0..width {
                let dx = column as i32 - center_x as i32;
                let dy = row as i32 - center_y as i32;
                if dx * dx + dy * dy <= (radius * radius) as i32 {
                    alpha[row * width + column] = 255;
                }
            }
        }
        alpha
    }

    #[test]
    fn finds_the_centre_of_a_blob() {
        let alpha = blob(200, 100, 150, 40, 15);
        let center = subject_center(&alpha, 200, 100).expect("center");
        assert!((center.x - 150.0).abs() < 1.0, "x was {}", center.x);
        assert!((center.y - 40.0).abs() < 1.0, "y was {}", center.y);
    }

    #[test]
    fn empty_matte_has_no_centre() {
        let alpha = vec![0u8; 100 * 100];
        assert!(subject_center(&alpha, 100, 100).is_none());
    }

    #[test]
    fn vertical_crop_keeps_full_height() {
        let (width, height) = crop_size(1920.0, 1080.0, 9.0 / 16.0, 1.0);
        assert!((height - 1080.0).abs() < 0.01);
        assert!((width - 607.5).abs() < 0.01);
    }

    #[test]
    fn crop_stays_inside_the_frame() {
        let options = ReframeOptions::default();
        for x in [0.0, 50.0, 960.0, 1900.0] {
            let window = crop_window(Point { x, y: 540.0 }, 1920.0, 1080.0, &options);
            assert!(window.x >= 0.0, "left edge escaped at {x}");
            assert!(
                window.x + window.width <= 1920.0 + 1e-3,
                "right edge escaped at {x}"
            );
        }
    }

    #[test]
    fn crop_follows_the_subject() {
        let options = ReframeOptions::default();
        let left = crop_window(Point { x: 400.0, y: 540.0 }, 1920.0, 1080.0, &options);
        let right = crop_window(
            Point {
                x: 1500.0,
                y: 540.0,
            },
            1920.0,
            1080.0,
            &options,
        );
        assert!(right.x > left.x, "crop did not follow the subject");
    }

    #[test]
    fn path_smoothing_reduces_jitter() {
        let jittery: Vec<Point> = (0..40)
            .map(|index| Point {
                x: if index % 2 == 0 { 800.0 } else { 1100.0 },
                y: 540.0,
            })
            .collect();
        let options = ReframeOptions::default();
        let path = reframe_path(&jittery, 1920.0, 1080.0, &options);

        let spread = |values: &[CropWindow]| {
            let max = values.iter().map(|w| w.x).fold(f32::MIN, f32::max);
            let min = values.iter().map(|w| w.x).fold(f32::MAX, f32::min);
            max - min
        };
        assert!(
            spread(&path) < 300.0,
            "path still jittery: {}",
            spread(&path)
        );
    }

    #[test]
    fn zoom_shrinks_the_window() {
        let (plain_width, _) = crop_size(1920.0, 1080.0, 9.0 / 16.0, 1.0);
        let (zoomed_width, _) = crop_size(1920.0, 1080.0, 9.0 / 16.0, 2.0);
        assert!(zoomed_width < plain_width);
    }
}
