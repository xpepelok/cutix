use std::path::{Path, PathBuf};

use cutix_i18n::{t, t_args};
use ml::{models, segmentation_size, MlError, SegmentationModel};
use video::reframe::{CropWindow, Point, ReframeOptions};

use crate::ai::Job;
use crate::tracking::TICKS_PER_SECOND;

pub const REFRAME_SAMPLE_SIZE: usize = 320;
pub const MAX_REFRAME_SAMPLES: usize = 120;
pub const REFRAME_SAMPLES_PER_SECOND: f64 = 6.0;
pub const DEFAULT_ZOOM: f32 = 1.0;
pub const MAX_ZOOM: f32 = 2.0;

pub const REFRAME_ASPECTS: &[(&str, f32)] = &[
    ("9:16", 9.0 / 16.0),
    ("1:1", 1.0),
    ("4:5", 4.0 / 5.0),
    ("16:9", 16.0 / 9.0),
];

pub fn aspect_value(key: &str) -> f32 {
    REFRAME_ASPECTS
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| *value)
        .unwrap_or(9.0 / 16.0)
}

pub fn reframe_sample_count(duration_seconds: f64) -> usize {
    let planned = (duration_seconds * REFRAME_SAMPLES_PER_SECOND).round();
    let planned = if planned.is_finite() && planned > 0.0 {
        planned as usize
    } else {
        0
    };
    planned.min(MAX_REFRAME_SAMPLES).max(2)
}

pub fn smoothing_radius_for(sample_count: usize) -> usize {
    (sample_count / 6).clamp(1, 8)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReframeCrop {
    pub time: i64,
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

#[derive(Clone, Debug, Default)]
pub struct ReframeAnalysis {
    pub times: Vec<f64>,
    pub centers: Vec<Point>,
    pub windows: Vec<CropWindow>,
    pub source_width: f32,
    pub source_height: f32,
}

fn read_frame(
    path: &Path,
    is_video: bool,
    seconds: f64,
) -> Result<(Vec<u8>, usize, usize), String> {
    if is_video {
        let frame =
            video::decode::frame_at(path, seconds.max(0.0)).map_err(|error| error.to_string())?;
        return Ok((frame.rgba, frame.width as usize, frame.height as usize));
    }
    let image = image::open(path)
        .map_err(|error| error.to_string())?
        .to_rgba8();
    let (width, height) = image.dimensions();
    Ok((image.into_raw(), width as usize, height as usize))
}

fn sample_size_for(width: usize, height: usize) -> (usize, usize) {
    let longest = width.max(height);
    if longest == 0 {
        return (0, 0);
    }
    if longest <= REFRAME_SAMPLE_SIZE {
        return (width, height);
    }
    let scale = REFRAME_SAMPLE_SIZE as f64 / longest as f64;
    (
        ((width as f64 * scale).round() as usize).max(1),
        ((height as f64 * scale).round() as usize).max(1),
    )
}

pub struct ReframeRequest {
    pub source: PathBuf,
    pub is_video: bool,
    pub start_seconds: f64,
    pub duration_seconds: f64,
    pub target_aspect: f32,
    pub zoom: f32,
    pub model_key: String,
}

pub fn analyze(request: &ReframeRequest, job: &Job) -> Result<ReframeAnalysis, String> {
    if request.duration_seconds <= 0.0 {
        return Err(t("reframe.failed"));
    }
    let count = reframe_sample_count(request.duration_seconds);
    if count < 2 {
        return Err(t("reframe.failed"));
    }

    let spec = models::find_model(&request.model_key).ok_or_else(|| t("reframe.failed"))?;
    let cached = models::is_cached(spec);
    let path = models::ensure_downloaded(spec, |progress| {
        if !cached {
            job.publish(
                t_args(
                    "reframe.downloading",
                    &[("percent", &((progress * 100.0).round() as i64).to_string())],
                ),
                progress,
            );
        }
    })
    .map_err(|error| error.to_string())?;

    if job.is_cancelled() {
        return Err(t("cutout.cancel"));
    }

    let mut model =
        SegmentationModel::load(&path, spec.input_size).map_err(|error| error.to_string())?;

    let step = request.duration_seconds / (count - 1) as f64;
    let mut times = Vec::with_capacity(count);
    let mut centers = Vec::with_capacity(count);
    let mut source_width = 0.0f32;
    let mut source_height = 0.0f32;

    for index in 0..count {
        if job.is_cancelled() {
            return Err(t("cutout.cancel"));
        }
        let seconds = request.start_seconds + index as f64 * step;
        let Ok((rgba, width, height)) = read_frame(&request.source, request.is_video, seconds)
        else {
            continue;
        };
        if width == 0 || height == 0 {
            continue;
        }
        source_width = width as f32;
        source_height = height as f32;

        let (sample_width, sample_height) = sample_size_for(width, height);
        let scaled = ml::matte::downscale_rgba(&rgba, width, height, sample_width, sample_height);
        let (target_width, target_height) = segmentation_size(sample_width, sample_height);
        let for_model = ml::matte::downscale_rgba(
            &scaled,
            sample_width,
            sample_height,
            target_width,
            target_height,
        );
        let alpha = model
            .matte(&for_model, target_width, target_height)
            .map_err(|error: MlError| error.to_string())?;

        let center = video::reframe::subject_center(&alpha, target_width, target_height)
            .map(|point| Point {
                x: point.x / target_width as f32 * source_width,
                y: point.y / target_height as f32 * source_height,
            })
            .unwrap_or(Point {
                x: source_width / 2.0,
                y: source_height / 2.0,
            });
        times.push(seconds);
        centers.push(center);

        let completed = index + 1;
        job.publish(
            t_args(
                "reframe.sampling",
                &[
                    ("completed", &completed.to_string()),
                    ("total", &count.to_string()),
                ],
            ),
            completed as f32 / count as f32,
        );
    }

    if centers.is_empty() {
        return Err(t("reframe.failed"));
    }

    let windows = reframe_windows(
        &centers,
        source_width,
        source_height,
        request.target_aspect,
        request.zoom,
    );

    Ok(ReframeAnalysis {
        times,
        centers,
        windows,
        source_width,
        source_height,
    })
}

pub fn reframe_windows(
    centers: &[Point],
    source_width: f32,
    source_height: f32,
    target_aspect: f32,
    zoom: f32,
) -> Vec<CropWindow> {
    if centers.is_empty() || source_width <= 0.0 || source_height <= 0.0 {
        return Vec::new();
    }
    let options = ReframeOptions {
        target_aspect,
        smoothing_radius: smoothing_radius_for(centers.len()),
        zoom,
    };
    video::reframe::reframe_path(centers, source_width, source_height, &options)
}

pub fn window_to_crop(
    window: &CropWindow,
    source_width: f32,
    source_height: f32,
) -> (f64, f64, f64, f64) {
    let left = (window.x / source_width) as f64;
    let top = (window.y / source_height) as f64;
    let right = (1.0 - (window.x + window.width) / source_width).max(0.0) as f64;
    let bottom = (1.0 - (window.y + window.height) / source_height).max(0.0) as f64;
    (left, top, right, bottom)
}

pub struct BakeRequest<'a> {
    pub analysis: &'a ReframeAnalysis,
    pub start_seconds: f64,
    pub duration_ticks: i64,
}

pub fn build_reframe_keyframes(request: &BakeRequest<'_>) -> Vec<ReframeCrop> {
    let analysis = request.analysis;
    if analysis.source_width <= 0.0 || analysis.source_height <= 0.0 {
        return Vec::new();
    }
    let mut crops = Vec::with_capacity(analysis.windows.len());
    for (seconds, window) in analysis.times.iter().zip(analysis.windows.iter()) {
        let time =
            (((seconds - request.start_seconds) * TICKS_PER_SECOND as f64).round() as i64).max(0);
        if time > request.duration_ticks {
            continue;
        }
        let (left, top, right, bottom) =
            window_to_crop(window, analysis.source_width, analysis.source_height);
        crops.push(ReframeCrop {
            time,
            left,
            top,
            right,
            bottom,
        });
    }
    crops
}

#[cfg(test)]
mod tests {
    use super::*;

    fn centers_from(path: &[(f32, f32)]) -> Vec<Point> {
        path.iter().map(|(x, y)| Point { x: *x, y: *y }).collect()
    }

    fn walking_subject(count: usize) -> Vec<Point> {
        (0..count)
            .map(|index| {
                let fraction = index as f32 / (count - 1) as f32;
                Point {
                    x: 300.0 + fraction * 1300.0,
                    y: 540.0 + if index % 2 == 0 { 30.0 } else { -30.0 },
                }
            })
            .collect()
    }

    #[test]
    fn the_subject_stays_inside_the_output_rect() {
        let centers = walking_subject(60);
        let windows = reframe_windows(&centers, 1920.0, 1080.0, 9.0 / 16.0, 1.0);
        assert_eq!(windows.len(), centers.len());

        for (index, (center, window)) in centers.iter().zip(windows.iter()).enumerate() {
            assert!(
                center.x >= window.x && center.x <= window.x + window.width,
                "sample {index}: subject x {} escaped window {window:?}",
                center.x
            );
            assert!(
                center.y >= window.y && center.y <= window.y + window.height,
                "sample {index}: subject y {} escaped window {window:?}",
                center.y
            );
        }
    }

    #[test]
    fn every_window_stays_inside_the_source_frame() {
        for (name, aspect) in REFRAME_ASPECTS {
            let centers = walking_subject(40);
            let windows = reframe_windows(&centers, 1920.0, 1080.0, *aspect, 1.0);
            for window in &windows {
                assert!(window.x >= -1e-3, "{name}: left edge escaped: {window:?}");
                assert!(window.y >= -1e-3, "{name}: top edge escaped: {window:?}");
                assert!(
                    window.x + window.width <= 1920.0 + 1e-3,
                    "{name}: right edge escaped: {window:?}"
                );
                assert!(
                    window.y + window.height <= 1080.0 + 1e-3,
                    "{name}: bottom edge escaped: {window:?}"
                );
            }
        }
    }

    #[test]
    fn the_window_carries_the_requested_aspect() {
        for (name, aspect) in REFRAME_ASPECTS {
            let windows = reframe_windows(
                &centers_from(&[(960.0, 540.0)]),
                1920.0,
                1080.0,
                *aspect,
                1.0,
            );
            let window = windows[0];
            let actual = window.width / window.height;
            assert!(
                (actual - aspect).abs() < 1e-3,
                "{name}: wanted {aspect}, got {actual}"
            );
        }
    }

    #[test]
    fn the_window_follows_the_subject_across_the_frame() {
        let centers = walking_subject(40);
        let windows = reframe_windows(&centers, 1920.0, 1080.0, 9.0 / 16.0, 1.0);
        let first = windows.first().expect("first");
        let last = windows.last().expect("last");
        assert!(
            last.x > first.x + 500.0,
            "window did not follow: {} -> {}",
            first.x,
            last.x
        );
    }

    #[test]
    fn smoothing_removes_the_wobble_the_subject_had() {
        let centers: Vec<Point> = (0..48)
            .map(|index| Point {
                x: if index % 2 == 0 { 760.0 } else { 1160.0 },
                y: 540.0,
            })
            .collect();
        let windows = reframe_windows(&centers, 1920.0, 1080.0, 9.0 / 16.0, 1.0);
        let step = windows
            .windows(2)
            .map(|pair| (pair[1].x - pair[0].x).abs())
            .fold(0.0f32, f32::max);
        assert!(step < 100.0, "window still jumps by {step}px per frame");
    }

    #[test]
    fn zoom_tightens_the_window() {
        let centers = centers_from(&[(960.0, 540.0)]);
        let plain = reframe_windows(&centers, 1920.0, 1080.0, 9.0 / 16.0, 1.0)[0];
        let zoomed = reframe_windows(&centers, 1920.0, 1080.0, 9.0 / 16.0, 2.0)[0];
        assert!(zoomed.width < plain.width);
        assert!(zoomed.height < plain.height);
        assert!((zoomed.width / zoomed.height - 9.0 / 16.0).abs() < 1e-3);
    }

    #[test]
    fn a_centred_vertical_crop_becomes_symmetric_insets() {
        let window = CropWindow {
            x: 656.25,
            y: 0.0,
            width: 607.5,
            height: 1080.0,
        };
        let (left, top, right, bottom) = window_to_crop(&window, 1920.0, 1080.0);
        assert!((left - right).abs() < 1e-6, "{left} vs {right}");
        assert!(top.abs() < 1e-9 && bottom.abs() < 1e-9);
        assert!((left + right + 607.5 / 1920.0 - 1.0).abs() < 1e-6);
    }

    #[test]
    fn crop_insets_never_go_negative() {
        let window = CropWindow {
            x: 0.0,
            y: 0.0,
            width: 1920.5,
            height: 1080.5,
        };
        let (left, top, right, bottom) = window_to_crop(&window, 1920.0, 1080.0);
        assert!(left >= 0.0 && top >= 0.0 && right >= 0.0 && bottom >= 0.0);
    }

    #[test]
    fn keyframes_are_timed_from_the_element_start() {
        let analysis = ReframeAnalysis {
            times: vec![4.0, 5.0, 6.0],
            centers: Vec::new(),
            windows: vec![
                CropWindow {
                    x: 0.0,
                    y: 0.0,
                    width: 1920.0,
                    height: 1080.0,
                },
                CropWindow {
                    x: 100.0,
                    y: 0.0,
                    width: 1920.0,
                    height: 1080.0,
                },
                CropWindow {
                    x: 200.0,
                    y: 0.0,
                    width: 1920.0,
                    height: 1080.0,
                },
            ],
            source_width: 1920.0,
            source_height: 1080.0,
        };
        let crops = build_reframe_keyframes(&BakeRequest {
            analysis: &analysis,
            start_seconds: 4.0,
            duration_ticks: TICKS_PER_SECOND * 10,
        });
        assert_eq!(crops.len(), 3);
        assert_eq!(crops[0].time, 0);
        assert_eq!(crops[1].time, TICKS_PER_SECOND);
        assert_eq!(crops[2].time, TICKS_PER_SECOND * 2);
    }

    #[test]
    fn samples_past_the_element_end_are_dropped() {
        let analysis = ReframeAnalysis {
            times: vec![0.0, 9.0],
            centers: Vec::new(),
            windows: vec![
                CropWindow {
                    x: 0.0,
                    y: 0.0,
                    width: 1920.0,
                    height: 1080.0,
                },
                CropWindow {
                    x: 0.0,
                    y: 0.0,
                    width: 1920.0,
                    height: 1080.0,
                },
            ],
            source_width: 1920.0,
            source_height: 1080.0,
        };
        let crops = build_reframe_keyframes(&BakeRequest {
            analysis: &analysis,
            start_seconds: 0.0,
            duration_ticks: TICKS_PER_SECOND,
        });
        assert_eq!(crops.len(), 1);
    }

    #[test]
    fn the_sample_plan_matches_the_web() {
        assert_eq!(reframe_sample_count(30.0), 120);
        assert_eq!(reframe_sample_count(5.0), 30);
        assert_eq!(reframe_sample_count(0.01), 2);
        assert_eq!(smoothing_radius_for(2), 1);
        assert_eq!(smoothing_radius_for(30), 5);
        assert_eq!(smoothing_radius_for(120), 8);
    }

    #[test]
    fn the_sample_frame_keeps_its_aspect() {
        assert_eq!(sample_size_for(1920, 1080), (320, 180));
        assert_eq!(sample_size_for(1080, 1920), (180, 320));

        assert_eq!(sample_size_for(200, 100), (200, 100));
    }

    #[test]
    fn unknown_aspect_keys_fall_back_to_vertical() {
        assert!((aspect_value("9:16") - 9.0 / 16.0).abs() < 1e-6);
        assert!((aspect_value("16:9") - 16.0 / 9.0).abs() < 1e-6);
        assert!((aspect_value("nonsense") - 9.0 / 16.0).abs() < 1e-6);
    }

    #[test]
    fn every_reframe_string_is_translated() {
        for key in [
            "reframe.title",
            "reframe.aspect",
            "reframe.zoom",
            "reframe.run",
            "reframe.sampling",
            "reframe.downloading",
            "reframe.done",
            "reframe.failed",
            "reframe.noMedia",
            "reframe.hint",
        ] {
            assert_ne!(t(key), key, "{key}");
        }
    }
}
