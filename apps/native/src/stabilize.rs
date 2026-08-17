use std::path::{Path, PathBuf};

use cutix_i18n::t;
use cutix_project::model::Transform;

use crate::ai::Job;
use crate::tracking::{TrackingKeyframe, TICKS_PER_SECOND};

pub const LUMA_SAMPLE_SIZE: usize = 128;
pub const MAX_STABILIZATION_SAMPLES: usize = 150;
pub const DEFAULT_STRENGTH: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StabilizeSample {
    pub time_seconds: f64,
    pub dx: f32,
    pub dy: f32,
}

#[derive(Clone, Debug, Default)]
pub struct StabilizeAnalysis {
    pub samples: Vec<StabilizeSample>,
    pub sample_size: usize,

    pub motion_before: f32,
    pub motion_after: f32,
}

impl StabilizeAnalysis {
    pub fn improvement(&self) -> f32 {
        if self.motion_before <= f32::EPSILON {
            return 0.0;
        }
        (1.0 - self.motion_after / self.motion_before).clamp(0.0, 1.0)
    }

    pub fn required_zoom(&self) -> f64 {
        if self.sample_size == 0 {
            return 1.0;
        }
        let side = self.sample_size as f64;
        let peak_x = self
            .samples
            .iter()
            .map(|sample| sample.dx.abs() as f64)
            .fold(0.0f64, f64::max);
        let peak_y = self
            .samples
            .iter()
            .map(|sample| sample.dy.abs() as f64)
            .fold(0.0f64, f64::max);
        (1.0 + 2.0 * peak_x / side)
            .max(1.0 + 2.0 * peak_y / side)
            .max(1.0)
    }
}

pub fn strength_to_options(strength: f32) -> (usize, f32) {
    let clamped = strength.clamp(0.0, 1.0);
    let smoothing_radius = (2.0 + clamped * 22.0).round().max(2.0) as usize;
    let max_correction = 0.02 + clamped * 0.13;
    (smoothing_radius, max_correction)
}

pub fn stabilize_sample_count(duration_seconds: f64, fps: f32) -> usize {
    let planned = (duration_seconds * fps as f64).floor();
    let planned = if planned.is_finite() && planned > 0.0 {
        planned as usize
    } else {
        0
    };
    planned.min(MAX_STABILIZATION_SAMPLES).max(2)
}

fn luma_frame(path: &Path, seconds: f64) -> Option<Vec<f32>> {
    let frame = video::decode::frame_at(path, seconds.max(0.0)).ok()?;
    let width = frame.width as usize;
    let height = frame.height as usize;
    if width == 0 || height == 0 {
        return None;
    }
    let side = LUMA_SAMPLE_SIZE;
    let mut luma = vec![0.0f32; side * side];
    for y in 0..side {
        let source_y = (y * height / side).min(height - 1);
        for x in 0..side {
            let source_x = (x * width / side).min(width - 1);
            let index = (source_y * width + source_x) * 4;
            if index + 2 >= frame.rgba.len() {
                continue;
            }
            let red = frame.rgba[index] as f32;
            let green = frame.rgba[index + 1] as f32;
            let blue = frame.rgba[index + 2] as f32;
            luma[y * side + x] = (0.299 * red + 0.587 * green + 0.114 * blue) / 255.0;
        }
    }
    Some(luma)
}

pub struct StabilizeRequest {
    pub source: PathBuf,
    pub start_seconds: f64,
    pub duration_seconds: f64,
    pub fps: f32,
    pub strength: f32,
}

pub fn analyze(request: &StabilizeRequest, job: &Job) -> Result<StabilizeAnalysis, String> {
    if request.duration_seconds <= 0.0 {
        return Err(t("stabilize.failed"));
    }
    let count = stabilize_sample_count(request.duration_seconds, request.fps);
    let step = request.duration_seconds / count as f64;

    job.publish(t("stabilize.sampling"), 0.0);
    let mut frames: Vec<(f64, Vec<f32>)> = Vec::with_capacity(count);
    for index in 0..count {
        if job.is_cancelled() {
            return Err(t("cutout.cancel"));
        }
        let seconds = request.start_seconds + index as f64 * step;
        if let Some(luma) = luma_frame(&request.source, seconds) {
            frames.push((seconds, luma));
        }
        job.publish(t("stabilize.sampling"), index as f32 / count as f32);
    }
    if frames.len() < 2 {
        return Err(t("stabilize.failed"));
    }

    analyze_frames(&frames, LUMA_SAMPLE_SIZE, request.strength, job)
}

pub fn analyze_frames(
    frames: &[(f64, Vec<f32>)],
    side: usize,
    strength: f32,
    job: &Job,
) -> Result<StabilizeAnalysis, String> {
    if frames.len() < 2 || side == 0 {
        return Err(t("stabilize.failed"));
    }

    job.publish(t("stabilize.analyzing"), 0.0);
    let mut shifts = Vec::with_capacity(frames.len());
    shifts.push(video::stabilize::Shift::default());
    for index in 1..frames.len() {
        if job.is_cancelled() {
            return Err(t("cutout.cancel"));
        }
        shifts.push(video::stabilize::estimate_shift(
            &frames[index - 1].1,
            &frames[index].1,
            side,
            side,
        ));
        job.publish(
            t("stabilize.analyzing"),
            index as f32 / (frames.len() - 1).max(1) as f32,
        );
    }

    let (smoothing_radius, max_correction) = strength_to_options(strength);
    let options = video::stabilize::StabilizeOptions {
        smoothing_radius,
        max_correction,
    };
    let offsets = video::stabilize::stabilization_offsets(&shifts, side, side, &options);

    let samples: Vec<StabilizeSample> = frames
        .iter()
        .zip(offsets.iter())
        .map(|((seconds, _), offset)| StabilizeSample {
            time_seconds: *seconds,
            dx: offset.dx,
            dy: offset.dy,
        })
        .collect();

    let raw = video::stabilize::cumulative_trajectory(&shifts);
    let corrected: Vec<video::stabilize::Shift> = raw
        .iter()
        .zip(offsets.iter())
        .map(|(point, offset)| video::stabilize::Shift {
            dx: point.dx + offset.dx,
            dy: point.dy + offset.dy,
        })
        .collect();

    Ok(StabilizeAnalysis {
        samples,
        sample_size: side,
        motion_before: mean_step(&raw, side as f32),
        motion_after: mean_step(&corrected, side as f32),
    })
}

fn mean_step(path: &[video::stabilize::Shift], scale: f32) -> f32 {
    if path.len() < 2 || scale <= 0.0 {
        return 0.0;
    }
    let total: f32 = path
        .windows(2)
        .map(|pair| {
            let dx = pair[1].dx - pair[0].dx;
            let dy = pair[1].dy - pair[0].dy;
            (dx * dx + dy * dy).sqrt()
        })
        .sum();
    total / (path.len() - 1) as f32 / scale
}

pub struct BakeRequest<'a> {
    pub samples: &'a [StabilizeSample],
    pub sample_size: usize,
    pub transform: &'a Transform,

    pub zoom: f64,
    pub source_width: f64,
    pub source_height: f64,
    pub canvas_width: f64,
    pub canvas_height: f64,
    pub trim_start_ticks: i64,
    pub duration_ticks: i64,
}

pub fn build_stabilize_keyframes(request: &BakeRequest<'_>) -> Vec<TrackingKeyframe> {
    if request.source_width <= 0.0 || request.source_height <= 0.0 || request.sample_size == 0 {
        return Vec::new();
    }
    let contain = (request.canvas_width / request.source_width)
        .min(request.canvas_height / request.source_height);
    let displayed_width =
        (request.source_width * contain * request.transform.scale_x * request.zoom).abs();
    let displayed_height =
        (request.source_height * contain * request.transform.scale_y * request.zoom).abs();
    let side = request.sample_size as f64;

    let trim_start_seconds = request.trim_start_ticks as f64 / TICKS_PER_SECOND as f64;
    let mut keyframes = Vec::with_capacity(request.samples.len());
    for sample in request.samples {
        let local_time =
            ((sample.time_seconds - trim_start_seconds) * TICKS_PER_SECOND as f64).round() as i64;
        if local_time < 0 || local_time > request.duration_ticks {
            continue;
        }
        keyframes.push(TrackingKeyframe {
            time: local_time,
            x: request.transform.position.x + sample.dx as f64 / side * displayed_width,
            y: request.transform.position.y + sample.dy as f64 / side * displayed_height,
        });
    }
    keyframes
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutix_project::model::Vector2;

    fn transform(scale_x: f64, scale_y: f64, x: f64, y: f64) -> Transform {
        Transform {
            scale_x,
            scale_y,
            rotate: 0.0,
            position: Vector2 { x, y },
        }
    }

    fn texture_value(x: i32, y: i32) -> f32 {
        let mut state = (x as u32).wrapping_mul(0x9e37_79b9) ^ (y as u32).wrapping_mul(0x85eb_ca6b);
        state ^= state >> 13;
        state = state.wrapping_mul(0xc2b2_ae35);
        state ^= state >> 16;
        state as f32 / u32::MAX as f32
    }

    fn shaky_frame(side: usize, offset_x: i32, offset_y: i32) -> Vec<f32> {
        let mut frame = vec![0.0f32; side * side];
        for row in 0..side {
            for column in 0..side {
                frame[row * side + column] =
                    texture_value(column as i32 + offset_x, row as i32 + offset_y);
            }
        }
        frame
    }

    fn shaky_clip(side: usize, count: usize) -> Vec<(f64, Vec<f32>)> {
        (0..count)
            .map(|index| {
                let drift = index as i32;
                let jitter_x = if index % 2 == 0 { 6 } else { -6 };
                let jitter_y = if index % 3 == 0 { 5 } else { -4 };
                (
                    index as f64 / 30.0,
                    shaky_frame(side, drift + jitter_x, jitter_y),
                )
            })
            .collect()
    }

    #[test]
    fn stabilisation_measurably_calms_a_shaky_clip() {
        let side = 128;
        let frames = shaky_clip(side, 48);
        let analysis =
            analyze_frames(&frames, side, 1.0, &Job::new(String::new())).expect("analysis");

        assert_eq!(analysis.samples.len(), frames.len());
        assert!(
            analysis.motion_before > 0.0,
            "the synthetic clip should move at all"
        );
        assert!(
            analysis.motion_after < analysis.motion_before * 0.6,
            "frame-to-frame motion not reduced: {} -> {}",
            analysis.motion_before,
            analysis.motion_after
        );
        assert!(
            analysis.improvement() > 0.4,
            "improvement was {}",
            analysis.improvement()
        );
    }

    #[test]
    fn a_locked_off_shot_needs_no_correction_and_no_zoom() {
        let side = 128;
        let frames: Vec<(f64, Vec<f32>)> = (0..20)
            .map(|index| (index as f64 / 30.0, shaky_frame(side, 0, 0)))
            .collect();
        let analysis = analyze_frames(&frames, side, DEFAULT_STRENGTH, &Job::new(String::new()))
            .expect("analysis");
        for sample in &analysis.samples {
            assert!(
                sample.dx.abs() < 1e-6 && sample.dy.abs() < 1e-6,
                "{sample:?}"
            );
        }
        assert!((analysis.required_zoom() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn corrections_never_exceed_the_strength_budget() {
        let side = 128;
        let frames = shaky_clip(side, 60);
        let strength = 0.5;
        let (_, max_correction) = strength_to_options(strength);
        let analysis =
            analyze_frames(&frames, side, strength, &Job::new(String::new())).expect("analysis");
        let limit = max_correction * side as f32;
        for sample in &analysis.samples {
            assert!(
                sample.dx.abs() <= limit + 1e-3 && sample.dy.abs() <= limit + 1e-3,
                "correction {sample:?} escaped the budget of {limit}"
            );
        }
    }

    #[test]
    fn a_stronger_setting_smooths_harder_and_allows_a_bigger_crop() {
        let (weak_radius, weak_correction) = strength_to_options(0.0);
        let (mid_radius, mid_correction) = strength_to_options(0.5);
        let (strong_radius, strong_correction) = strength_to_options(1.0);
        assert_eq!(weak_radius, 2);
        assert_eq!(mid_radius, 13);
        assert_eq!(strong_radius, 24);
        assert!((weak_correction - 0.02).abs() < 1e-6);
        assert!((mid_correction - 0.085).abs() < 1e-6);
        assert!((strong_correction - 0.15).abs() < 1e-6);

        assert_eq!(strength_to_options(-1.0).0, weak_radius);
        assert_eq!(strength_to_options(5.0).0, strong_radius);
    }

    #[test]
    fn the_zoom_covers_twice_the_largest_correction() {
        let analysis = StabilizeAnalysis {
            samples: vec![
                StabilizeSample {
                    time_seconds: 0.0,
                    dx: 6.4,
                    dy: 0.0,
                },
                StabilizeSample {
                    time_seconds: 1.0,
                    dx: -3.0,
                    dy: 12.8,
                },
            ],
            sample_size: 128,
            motion_before: 0.0,
            motion_after: 0.0,
        };

        assert!((analysis.required_zoom() - 1.2).abs() < 1e-6);
    }

    #[test]
    fn a_cancelled_scan_stops_early() {
        let side = 64;
        let frames = shaky_clip(side, 8);
        let job = Job::new(String::new());
        job.request_cancel();
        assert!(analyze_frames(&frames, side, DEFAULT_STRENGTH, &job).is_err());
    }

    #[test]
    fn a_single_frame_cannot_be_stabilised() {
        let frames = vec![(0.0, shaky_frame(64, 0, 0))];
        assert!(analyze_frames(&frames, 64, DEFAULT_STRENGTH, &Job::new(String::new())).is_err());
    }

    #[test]
    fn keyframes_carry_the_correction_in_canvas_pixels() {
        let samples = [
            StabilizeSample {
                time_seconds: 0.0,
                dx: 0.0,
                dy: 0.0,
            },
            StabilizeSample {
                time_seconds: 1.0,
                dx: 12.8,
                dy: -25.6,
            },
        ];

        let keyframes = build_stabilize_keyframes(&BakeRequest {
            samples: &samples,
            sample_size: 128,
            transform: &transform(1.0, 1.0, 0.0, 0.0),
            zoom: 1.0,
            source_width: 1920.0,
            source_height: 1080.0,
            canvas_width: 1920.0,
            canvas_height: 1080.0,
            trim_start_ticks: 0,
            duration_ticks: TICKS_PER_SECOND * 10,
        });
        assert_eq!(keyframes.len(), 2);
        assert_eq!(keyframes[0].time, 0);
        assert_eq!(keyframes[1].time, TICKS_PER_SECOND);

        assert!(
            (keyframes[1].x - 192.0).abs() < 1e-3,
            "x {}",
            keyframes[1].x
        );
        assert!(
            (keyframes[1].y + 216.0).abs() < 1e-3,
            "y {}",
            keyframes[1].y
        );
    }

    #[test]
    fn the_zoom_scales_the_baked_offsets() {
        let samples = [StabilizeSample {
            time_seconds: 0.0,
            dx: 12.8,
            dy: 0.0,
        }];
        let bake = |zoom: f64| {
            build_stabilize_keyframes(&BakeRequest {
                samples: &samples,
                sample_size: 128,
                transform: &transform(1.0, 1.0, 0.0, 0.0),
                zoom,
                source_width: 1920.0,
                source_height: 1080.0,
                canvas_width: 1920.0,
                canvas_height: 1080.0,
                trim_start_ticks: 0,
                duration_ticks: TICKS_PER_SECOND,
            })[0]
                .x
        };
        assert!((bake(1.25) - bake(1.0) * 1.25).abs() < 1e-6);
    }

    #[test]
    fn keyframes_keep_the_offset_the_clip_already_had() {
        let samples = [StabilizeSample {
            time_seconds: 0.0,
            dx: 12.8,
            dy: 0.0,
        }];
        let keyframes = build_stabilize_keyframes(&BakeRequest {
            samples: &samples,
            sample_size: 128,
            transform: &transform(1.0, 1.0, 40.0, -25.0),
            zoom: 1.0,
            source_width: 1920.0,
            source_height: 1080.0,
            canvas_width: 1920.0,
            canvas_height: 1080.0,
            trim_start_ticks: 0,
            duration_ticks: TICKS_PER_SECOND,
        });
        assert!((keyframes[0].x - (40.0 + 192.0)).abs() < 1e-3);
        assert!((keyframes[0].y + 25.0).abs() < 1e-9);
    }

    #[test]
    fn samples_outside_the_trimmed_span_are_dropped() {
        let samples = [
            StabilizeSample {
                time_seconds: 0.0,
                dx: 0.0,
                dy: 0.0,
            },
            StabilizeSample {
                time_seconds: 9.0,
                dx: 0.0,
                dy: 0.0,
            },
        ];
        let keyframes = build_stabilize_keyframes(&BakeRequest {
            samples: &samples,
            sample_size: 128,
            transform: &transform(1.0, 1.0, 0.0, 0.0),
            zoom: 1.0,
            source_width: 1920.0,
            source_height: 1080.0,
            canvas_width: 1920.0,
            canvas_height: 1080.0,
            trim_start_ticks: 0,
            duration_ticks: TICKS_PER_SECOND,
        });
        assert_eq!(keyframes.len(), 1);
    }

    #[test]
    fn keyframe_times_are_relative_to_the_trim_point() {
        let samples = [StabilizeSample {
            time_seconds: 5.0,
            dx: 0.0,
            dy: 0.0,
        }];
        let keyframes = build_stabilize_keyframes(&BakeRequest {
            samples: &samples,
            sample_size: 128,
            transform: &transform(1.0, 1.0, 0.0, 0.0),
            zoom: 1.0,
            source_width: 1920.0,
            source_height: 1080.0,
            canvas_width: 1920.0,
            canvas_height: 1080.0,
            trim_start_ticks: TICKS_PER_SECOND * 4,
            duration_ticks: TICKS_PER_SECOND * 10,
        });
        assert_eq!(keyframes[0].time, TICKS_PER_SECOND);
    }

    #[test]
    fn the_sample_count_is_capped_and_floored() {
        assert_eq!(stabilize_sample_count(10.0, 30.0), 150);
        assert_eq!(stabilize_sample_count(2.0, 30.0), 60);
        assert_eq!(stabilize_sample_count(0.01, 30.0), 2);
    }

    #[test]
    fn every_stabilize_string_is_translated() {
        for key in [
            "stabilize.title",
            "stabilize.run",
            "stabilize.strength",
            "stabilize.sampling",
            "stabilize.analyzing",
            "stabilize.done",
            "stabilize.failed",
            "stabilize.noMedia",
            "stabilize.hint",
        ] {
            assert_ne!(t(key), key, "{key}");
        }
    }
}
