use std::path::{Path, PathBuf};

use cutix_i18n::t;
use cutix_project::model::Transform;

use crate::ai::Job;

pub const TRACKING_SAMPLE_SIZE: usize = 128;
pub const MAX_TRACKING_SAMPLES: usize = 240;
pub const DEFAULT_TRACKING_CONFIDENCE: f32 = 0.3;
pub const MIN_TRACKING_REGION: f32 = 0.02;
pub const TICKS_PER_SECOND: i64 = 120_000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackingRegion {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for TrackingRegion {
    fn default() -> Self {
        Self {
            x: 0.35,
            y: 0.35,
            width: 0.3,
            height: 0.3,
        }
    }
}

impl TrackingRegion {
    pub fn clamped(self) -> Self {
        let width = self.width.clamp(MIN_TRACKING_REGION, 1.0);
        let height = self.height.clamp(MIN_TRACKING_REGION, 1.0);
        Self {
            width,
            height,
            x: self.x.clamp(0.0, 1.0 - width),
            y: self.y.clamp(0.0, 1.0 - height),
        }
    }

    pub fn center(self) -> (f32, f32) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackingSample {
    pub region: TrackingRegion,
    pub time_seconds: f64,
    pub confidence: f32,
}

#[derive(Clone, Debug, Default)]
pub struct TrackingAnalysis {
    pub samples: Vec<TrackingSample>,
    pub lost_at_seconds: Option<f64>,
    pub min_confidence: f32,
}

pub fn tracking_sample_count(duration_seconds: f64, fps: f32) -> usize {
    let planned = (duration_seconds * fps as f64).floor();
    let planned = if planned.is_finite() && planned > 0.0 {
        planned as usize
    } else {
        0
    };
    planned.clamp(2, MAX_TRACKING_SAMPLES)
}

fn luma_frame(path: &Path, seconds: f64) -> Option<Vec<f32>> {
    let frame = video::decode::frame_at(path, seconds.max(0.0)).ok()?;
    let width = frame.width as usize;
    let height = frame.height as usize;
    if width == 0 || height == 0 {
        return None;
    }
    let mut luma = vec![0.0f32; TRACKING_SAMPLE_SIZE * TRACKING_SAMPLE_SIZE];
    for y in 0..TRACKING_SAMPLE_SIZE {
        let source_y = (y * height / TRACKING_SAMPLE_SIZE).min(height - 1);
        for x in 0..TRACKING_SAMPLE_SIZE {
            let source_x = (x * width / TRACKING_SAMPLE_SIZE).min(width - 1);
            let index = (source_y * width + source_x) * 4;
            if index + 2 >= frame.rgba.len() {
                continue;
            }
            let red = frame.rgba[index] as f32;
            let green = frame.rgba[index + 1] as f32;
            let blue = frame.rgba[index + 2] as f32;
            luma[y * TRACKING_SAMPLE_SIZE + x] =
                (0.299 * red + 0.587 * green + 0.114 * blue) / 255.0;
        }
    }
    Some(luma)
}

pub struct TrackRequest {
    pub source: PathBuf,
    pub region: TrackingRegion,
    pub start_seconds: f64,
    pub duration_seconds: f64,
    pub fps: f32,
    pub confidence_threshold: f32,
}

pub fn analyze(request: &TrackRequest, job: &Job) -> Result<TrackingAnalysis, String> {
    if request.duration_seconds <= 0.0 {
        return Err(t("tracking.failed"));
    }
    let count = tracking_sample_count(request.duration_seconds, request.fps);
    let step = request.duration_seconds / count as f64;

    job.publish(t("tracking.sampling"), 0.0);
    let mut frames: Vec<(f64, Vec<f32>)> = Vec::with_capacity(count);
    for index in 0..count {
        if job.is_cancelled() {
            return Err(t("cutout.cancel"));
        }
        let seconds = request.start_seconds + index as f64 * step;
        if let Some(luma) = luma_frame(&request.source, seconds) {
            frames.push((seconds, luma));
        }
        job.publish(t("tracking.sampling"), index as f32 / count as f32);
    }
    if frames.len() < 2 {
        return Err(t("tracking.failed"));
    }

    analyze_frames(&frames, request.region, request.confidence_threshold, job)
}

pub fn analyze_frames(
    frames: &[(f64, Vec<f32>)],
    region: TrackingRegion,
    confidence_threshold: f32,
    job: &Job,
) -> Result<TrackingAnalysis, String> {
    let side = TRACKING_SAMPLE_SIZE as f32;
    let mut current = video::track::Region {
        x: region.x * side,
        y: region.y * side,
        width: region.width * side,
        height: region.height * side,
    };
    let options = video::track::TrackOptions {
        search_margin: 0.5,
        max_step: (side / 4.0).max(8.0),
    };

    let mut samples = vec![TrackingSample {
        region,
        time_seconds: frames[0].0,
        confidence: 1.0,
    }];
    let mut min_confidence = 1.0f32;
    let mut lost_at_seconds = None;

    job.publish(t("tracking.tracking"), 0.0);
    for index in 1..frames.len() {
        if job.is_cancelled() {
            return Err(t("cutout.cancel"));
        }
        let step_result = video::track::track_region_scored(
            &frames[index - 1].1,
            &frames[index].1,
            TRACKING_SAMPLE_SIZE,
            TRACKING_SAMPLE_SIZE,
            current,
            &options,
        );
        min_confidence = min_confidence.min(step_result.confidence);
        if step_result.confidence < confidence_threshold {
            lost_at_seconds = Some(frames[index].0);
            break;
        }
        current = step_result.region;
        samples.push(TrackingSample {
            region: TrackingRegion {
                x: current.x / side,
                y: current.y / side,
                width: current.width / side,
                height: current.height / side,
            },
            time_seconds: frames[index].0,
            confidence: step_result.confidence,
        });
        job.publish(
            t("tracking.tracking"),
            index as f32 / (frames.len() - 1).max(1) as f32,
        );
    }

    Ok(TrackingAnalysis {
        samples,
        lost_at_seconds,
        min_confidence,
    })
}

pub fn source_to_canvas(
    normalized_x: f64,
    normalized_y: f64,
    transform: &Transform,
    source_width: f64,
    source_height: f64,
    canvas_width: f64,
    canvas_height: f64,
) -> (f64, f64) {
    let contain = (canvas_width / source_width).min(canvas_height / source_height);
    let displayed_width = source_width * contain * transform.scale_x;
    let displayed_height = source_height * contain * transform.scale_y;
    let local_x = (normalized_x - 0.5) * displayed_width;
    let local_y = (normalized_y - 0.5) * displayed_height;
    let radians = transform.rotate.to_radians();
    let cos = radians.cos();
    let sin = radians.sin();
    (
        canvas_width / 2.0 + transform.position.x + local_x * cos - local_y * sin,
        canvas_height / 2.0 + transform.position.y + local_x * sin + local_y * cos,
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackingKeyframe {
    pub time: i64,
    pub x: f64,
    pub y: f64,
}

pub struct BindRequest<'a> {
    pub samples: &'a [TrackingSample],
    pub transform: &'a Transform,
    pub source_width: f64,
    pub source_height: f64,
    pub canvas_width: f64,
    pub canvas_height: f64,
    pub video_start_ticks: i64,
    pub video_trim_start_ticks: i64,
    pub bound_start_ticks: i64,
    pub bound_duration_ticks: i64,
    pub bound_position: (f64, f64),
}

pub fn build_tracking_keyframes(request: &BindRequest<'_>) -> Vec<TrackingKeyframe> {
    let Some(first) = request.samples.first() else {
        return Vec::new();
    };
    let to_canvas = |sample: &TrackingSample| {
        let (u, v) = sample.region.center();
        source_to_canvas(
            u as f64,
            v as f64,
            request.transform,
            request.source_width,
            request.source_height,
            request.canvas_width,
            request.canvas_height,
        )
    };

    let anchor = to_canvas(first);
    let offset_x = request.bound_position.0 - (anchor.0 - request.canvas_width / 2.0);
    let offset_y = request.bound_position.1 - (anchor.1 - request.canvas_height / 2.0);

    let trim_start_seconds = request.video_trim_start_ticks as f64 / TICKS_PER_SECOND as f64;
    let mut keyframes = Vec::with_capacity(request.samples.len());
    for sample in request.samples {
        let timeline_time = request.video_start_ticks
            + ((sample.time_seconds - trim_start_seconds) * TICKS_PER_SECOND as f64).round() as i64;
        let local_time = timeline_time - request.bound_start_ticks;
        if local_time < 0 || local_time > request.bound_duration_ticks {
            continue;
        }
        let point = to_canvas(sample);
        keyframes.push(TrackingKeyframe {
            time: local_time,
            x: point.0 - request.canvas_width / 2.0 + offset_x,
            y: point.1 - request.canvas_height / 2.0 + offset_y,
        });
    }
    keyframes
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutix_project::model::Vector2;

    fn transform(scale_x: f64, scale_y: f64, rotate: f64, x: f64, y: f64) -> Transform {
        Transform {
            scale_x,
            scale_y,
            rotate,
            position: Vector2 { x, y },
        }
    }

    fn sample(x: f32, y: f32, seconds: f64) -> TrackingSample {
        TrackingSample {
            region: TrackingRegion {
                x,
                y,
                width: 0.2,
                height: 0.2,
            },
            time_seconds: seconds,
            confidence: 0.9,
        }
    }

    /// The sizes a placement is worked out against: the source frame and the canvas it
    /// is being laid onto.
    #[derive(Clone, Copy)]
    struct Frames {
        source_width: f64,
        source_height: f64,
        canvas_width: f64,
        canvas_height: f64,
    }

    fn renderer_oracle(
        u: f64,
        v: f64,
        transform: &Transform,
        crop: (f64, f64, f64, f64),
        frames: Frames,
    ) -> (f64, f64) {
        let Frames {
            source_width,
            source_height,
            canvas_width,
            canvas_height,
        } = frames;
        let (left, top, right, bottom) = crop;
        let contain = (canvas_width / source_width).min(canvas_height / source_height);
        let abs_width = source_width * contain * transform.scale_x.abs();
        let abs_height = source_height * contain * transform.scale_y.abs();
        let crop_width = 1.0 - left - right;
        let crop_height = 1.0 - top - bottom;
        let flip_x = transform.scale_x < 0.0;
        let flip_y = transform.scale_y < 0.0;

        let quad_width = abs_width * crop_width;
        let quad_height = abs_height * crop_height;
        let offset_x = ((left - right) / 2.0) * abs_width * if flip_x { -1.0 } else { 1.0 };
        let offset_y = ((top - bottom) / 2.0) * abs_height * if flip_y { -1.0 } else { 1.0 };
        let radians = transform.rotate.to_radians();
        let cos = radians.cos();
        let sin = radians.sin();
        let center_x = canvas_width / 2.0 + transform.position.x + offset_x * cos - offset_y * sin;
        let center_y = canvas_height / 2.0 + transform.position.y + offset_x * sin + offset_y * cos;

        let mut fx = (u - left) / crop_width;
        let mut fy = (v - top) / crop_height;
        if flip_x {
            fx = 1.0 - fx;
        }
        if flip_y {
            fy = 1.0 - fy;
        }
        let local_x = (fx - 0.5) * quad_width;
        let local_y = (fy - 0.5) * quad_height;
        (
            center_x + local_x * cos - local_y * sin,
            center_y + local_x * sin + local_y * cos,
        )
    }

    #[test]
    fn the_binding_geometry_agrees_with_the_renderer_quad() {
        let cases: Vec<(Transform, (f64, f64, f64, f64))> = vec![
            (transform(1.0, 1.0, 30.0, 0.0, 0.0), (0.0, 0.0, 0.0, 0.0)),
            (transform(1.0, 1.0, 0.0, 0.0, 0.0), (0.2, 0.1, 0.05, 0.3)),
            (
                transform(1.4, 0.9, 37.0, 120.0, -80.0),
                (0.2, 0.1, 0.05, 0.3),
            ),
            (
                transform(-1.2, 1.0, -25.0, -60.0, 40.0),
                (0.15, 0.25, 0.1, 0.05),
            ),
        ];
        let probes = [(0.5, 0.5), (0.3, 0.7), (0.62, 0.41), (0.75, 0.25)];

        for (index, (transform, crop)) in cases.iter().enumerate() {
            for (u, v) in probes {
                let mine = source_to_canvas(u, v, transform, 1280.0, 720.0, 1920.0, 1080.0);
                let oracle = renderer_oracle(
                    u,
                    v,
                    transform,
                    *crop,
                    Frames {
                        source_width: 1280.0,
                        source_height: 720.0,
                        canvas_width: 1920.0,
                        canvas_height: 1080.0,
                    },
                );
                assert!(
                    (mine.0 - oracle.0).abs() < 1e-6 && (mine.1 - oracle.1).abs() < 1e-6,
                    "case {index} probe ({u},{v}): {mine:?} vs {oracle:?}"
                );
            }
        }
    }

    #[test]
    fn an_identity_transform_maps_the_centre_to_the_canvas_centre() {
        let point = source_to_canvas(
            0.5,
            0.5,
            &transform(1.0, 1.0, 0.0, 0.0, 0.0),
            1920.0,
            1080.0,
            1920.0,
            1080.0,
        );
        assert!((point.0 - 960.0).abs() < 1e-9);
        assert!((point.1 - 540.0).abs() < 1e-9);
    }

    #[test]
    fn rotation_actually_moves_an_off_centre_probe() {
        let straight = source_to_canvas(
            0.75,
            0.25,
            &transform(1.0, 1.0, 0.0, 0.0, 0.0),
            1280.0,
            720.0,
            1920.0,
            1080.0,
        );
        let rotated = source_to_canvas(
            0.75,
            0.25,
            &transform(1.0, 1.0, 30.0, 0.0, 0.0),
            1280.0,
            720.0,
            1920.0,
            1080.0,
        );
        let moved = ((straight.0 - rotated.0).powi(2) + (straight.1 - rotated.1).powi(2)).sqrt();
        assert!(moved > 100.0, "{moved}");
    }

    fn bind(samples: &[TrackingSample], bound_duration: i64) -> Vec<TrackingKeyframe> {
        build_tracking_keyframes(&BindRequest {
            samples,
            transform: &transform(1.0, 1.0, 0.0, 0.0, 0.0),
            source_width: 1920.0,
            source_height: 1080.0,
            canvas_width: 1920.0,
            canvas_height: 1080.0,
            video_start_ticks: 0,
            video_trim_start_ticks: 0,
            bound_start_ticks: 0,
            bound_duration_ticks: bound_duration,
            bound_position: (100.0, -50.0),
        })
    }

    #[test]
    fn the_bound_element_keeps_its_offset_and_follows_the_delta() {
        let samples = [
            sample(0.1, 0.4, 0.0),
            sample(0.3, 0.4, 1.0),
            sample(0.5, 0.6, 2.0),
        ];
        let keyframes = bind(&samples, TICKS_PER_SECOND * 10);
        assert_eq!(keyframes.len(), 3);
        assert_eq!(keyframes[0].time, 0);
        assert!((keyframes[0].x - 100.0).abs() < 1e-9);
        assert!((keyframes[0].y + 50.0).abs() < 1e-9);
        assert_eq!(keyframes[1].time, TICKS_PER_SECOND);
        assert!((keyframes[1].x - keyframes[0].x - 0.2 * 1920.0).abs() < 1e-3);
        assert!((keyframes[1].y - keyframes[0].y).abs() < 1e-3);
        assert_eq!(keyframes[2].time, TICKS_PER_SECOND * 2);
        assert!((keyframes[2].x - keyframes[0].x - 0.4 * 1920.0).abs() < 1e-3);
        assert!((keyframes[2].y - keyframes[0].y - 0.2 * 1080.0).abs() < 1e-3);
    }

    #[test]
    fn samples_outside_the_bound_span_are_dropped() {
        let samples = [sample(0.1, 0.4, 0.0), sample(0.3, 0.4, 4.0)];
        assert_eq!(bind(&samples, TICKS_PER_SECOND).len(), 1);
    }

    #[test]
    fn keyframe_times_are_relative_to_the_bound_element() {
        let samples = [sample(0.1, 0.4, 5.0), sample(0.3, 0.4, 6.0)];
        let keyframes = build_tracking_keyframes(&BindRequest {
            samples: &samples,
            transform: &transform(1.0, 1.0, 0.0, 0.0, 0.0),
            source_width: 1920.0,
            source_height: 1080.0,
            canvas_width: 1920.0,
            canvas_height: 1080.0,
            video_start_ticks: TICKS_PER_SECOND * 10,
            video_trim_start_ticks: TICKS_PER_SECOND * 4,
            bound_start_ticks: TICKS_PER_SECOND * 11,
            bound_duration_ticks: TICKS_PER_SECOND * 10,
            bound_position: (0.0, 0.0),
        });
        assert_eq!(keyframes.len(), 2);
        assert_eq!(keyframes[0].time, 0);
        assert_eq!(keyframes[1].time, TICKS_PER_SECOND);
    }

    #[test]
    fn no_samples_bind_to_no_keyframes() {
        assert!(bind(&[], TICKS_PER_SECOND).is_empty());
    }

    #[test]
    fn a_region_stays_inside_the_frame() {
        let clamped = TrackingRegion {
            x: -0.5,
            y: 0.9,
            width: 0.4,
            height: 0.4,
        }
        .clamped();
        assert!((clamped.x - 0.0).abs() < 1e-6);
        assert!((clamped.y - 0.6).abs() < 1e-6);
        assert!((clamped.width - 0.4).abs() < 1e-6);
    }

    #[test]
    fn a_degenerate_region_grows_to_the_minimum() {
        let clamped = TrackingRegion {
            x: 0.5,
            y: 0.5,
            width: 0.0,
            height: 0.0,
        }
        .clamped();
        assert!((clamped.width - MIN_TRACKING_REGION).abs() < 1e-6);
    }

    #[test]
    fn the_sample_count_is_capped_and_floored() {
        assert_eq!(tracking_sample_count(10.0, 30.0), 240);
        assert_eq!(tracking_sample_count(2.0, 30.0), 60);
        assert_eq!(tracking_sample_count(0.01, 30.0), 2);
    }

    fn frame_with_square(x: f32, y: f32) -> Vec<f32> {
        let side = TRACKING_SAMPLE_SIZE;
        let mut luma = vec![0.05f32; side * side];
        for row in 0..24usize {
            for column in 0..24usize {
                let py = y as usize + row;
                let px = x as usize + column;
                if py >= side || px >= side {
                    continue;
                }
                let checker = if (row / 4 + column / 4) % 2 == 0 {
                    0.95
                } else {
                    0.35
                };
                luma[py * side + px] = checker;
            }
        }
        luma
    }

    #[test]
    fn a_moving_subject_is_followed_and_baked_into_keyframes() {
        let frames: Vec<(f64, Vec<f32>)> = (0..10)
            .map(|index| {
                (
                    index as f64 * 0.1,
                    frame_with_square(30.0 + index as f32 * 4.0, 40.0 + index as f32 * 2.0),
                )
            })
            .collect();
        let start = TrackingRegion {
            x: 26.0 / 128.0,
            y: 36.0 / 128.0,
            width: 32.0 / 128.0,
            height: 32.0 / 128.0,
        };
        let analysis =
            analyze_frames(&frames, start, 0.3, &Job::new(String::new())).expect("analysis");

        assert_eq!(analysis.samples.len(), 10, "every frame should be tracked");
        assert!(analysis.lost_at_seconds.is_none());

        let first = analysis.samples[0].region;
        let last = analysis.samples[9].region;

        let moved_x = (last.x - first.x) * 128.0;
        let moved_y = (last.y - first.y) * 128.0;
        assert!((moved_x - 36.0).abs() < 3.0, "x moved {moved_x}");
        assert!((moved_y - 18.0).abs() < 3.0, "y moved {moved_y}");

        let keyframes = build_tracking_keyframes(&BindRequest {
            samples: &analysis.samples,
            transform: &transform(1.0, 1.0, 0.0, 0.0, 0.0),
            source_width: 1280.0,
            source_height: 720.0,
            canvas_width: 1280.0,
            canvas_height: 720.0,
            video_start_ticks: 0,
            video_trim_start_ticks: 0,
            bound_start_ticks: 0,
            bound_duration_ticks: TICKS_PER_SECOND * 2,
            bound_position: (0.0, 0.0),
        });
        assert_eq!(keyframes.len(), 10);
        assert_eq!(keyframes[0].time, 0);
        assert!((keyframes[0].x).abs() < 1e-9 && (keyframes[0].y).abs() < 1e-9);

        let dx = keyframes[9].x - keyframes[0].x;
        let dy = keyframes[9].y - keyframes[0].y;
        assert!((dx - 360.0).abs() < 1e-3, "dx {dx}");
        assert!((dy - 101.25).abs() < 1e-3, "dy {dy}");

        for pair in keyframes.windows(2) {
            assert!(pair[1].x >= pair[0].x - 1e-6);
            assert!(pair[1].y >= pair[0].y - 1e-6);
        }
    }

    #[test]
    fn tracking_stops_when_the_subject_vanishes() {
        let mut frames: Vec<(f64, Vec<f32>)> = (0..4)
            .map(|index| {
                (
                    index as f64 * 0.1,
                    frame_with_square(30.0 + index as f32 * 4.0, 40.0),
                )
            })
            .collect();

        frames.push((
            0.4,
            vec![0.5f32; TRACKING_SAMPLE_SIZE * TRACKING_SAMPLE_SIZE],
        ));
        frames.push((
            0.5,
            vec![0.5f32; TRACKING_SAMPLE_SIZE * TRACKING_SAMPLE_SIZE],
        ));
        let start = TrackingRegion {
            x: 26.0 / 128.0,
            y: 36.0 / 128.0,
            width: 32.0 / 128.0,
            height: 32.0 / 128.0,
        };
        let analysis =
            analyze_frames(&frames, start, 0.5, &Job::new(String::new())).expect("analysis");
        assert_eq!(analysis.lost_at_seconds, Some(0.4));
        assert_eq!(analysis.samples.len(), 4);
        assert!(analysis.min_confidence < 0.5);
    }

    #[test]
    fn a_cancelled_run_stops_early() {
        let frames: Vec<(f64, Vec<f32>)> = (0..4)
            .map(|index| (index as f64 * 0.1, frame_with_square(30.0, 40.0)))
            .collect();
        let job = Job::new(String::new());
        job.request_cancel();
        assert!(analyze_frames(&frames, TrackingRegion::default(), 0.3, &job).is_err());
    }

    #[test]
    fn every_tracking_string_is_translated() {
        for key in [
            "tracking.title",
            "tracking.run",
            "tracking.target",
            "tracking.targetPlaceholder",
            "tracking.sensitivity",
            "tracking.sampling",
            "tracking.tracking",
            "tracking.done",
            "tracking.lost",
            "tracking.failed",
            "tracking.noMedia",
            "tracking.noTarget",
            "tracking.noOverlap",
            "tracking.hint",
        ] {
            assert_ne!(t(key), key, "{key}");
        }
    }
}
