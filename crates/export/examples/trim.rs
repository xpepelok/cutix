use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use cutix_export::{ExportQuality, ExportRequest, default_backend, run};
use cutix_playback::MediaMap;
use cutix_project::Project;
use serde_json::json;
use time::{FrameRate, MediaTime};

fn clip_project(kind: &str, width: u32, height: u32, trim_start: f64, duration: f64) -> Project {
    clip_project_at(kind, width, height, trim_start, duration, 30)
}

fn clip_project_at(
    kind: &str,
    width: u32,
    height: u32,
    trim_start: f64,
    duration: f64,
    fps: u32,
) -> Project {
    let mut project = Project::new("trim-bench", "1970-01-01T00:00:00.000Z".into());
    project.settings.canvas_size.width = width;
    project.settings.canvas_size.height = height;
    project.settings.fps = FrameRate::new(fps, 1);
    let element = serde_json::from_value(json!({
        "type": kind,
        "id": "clip",
        "name": "clip",
        "duration": MediaTime::from_seconds_f64(duration).expect("time").as_ticks(),
        "startTime": 0,
        "trimStart": MediaTime::from_seconds_f64(trim_start).expect("time").as_ticks(),
        "trimEnd": 0,
        "mediaId": "clip",
        "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
        "opacity": 1.0
    }))
    .expect("element");
    project
        .scenes
        .first_mut()
        .expect("scene")
        .tracks
        .main
        .elements_mut()
        .push(element);
    project
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let width: u32 = arguments
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1920);
    let height: u32 = arguments
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1080);
    let seconds: f64 = arguments
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10.0);
    let existing = arguments.next().map(std::path::PathBuf::from);
    let fps: u32 = arguments.next().and_then(|v| v.parse().ok()).unwrap_or(30);

    let directory = std::env::temp_dir().join("cutix-trim-bench");
    std::fs::create_dir_all(&directory).expect("temp dir");
    let still = directory.join("still.png");
    let mut buffer = image::RgbaImage::new(256, 256);
    for y in 0..256u32 {
        for x in 0..256u32 {
            buffer.put_pixel(x, y, image::Rgba([x as u8, y as u8, 128, 255]));
        }
    }
    buffer.save(&still).expect("write png");

    let backend = default_backend().expect("backend");
    let cancel = AtomicBool::new(false);

    if let Some(source) = existing {
        let trim = ExportRequest {
            project: Arc::new(clip_project_at("video", width, height, 4.0, 5.0, fps)),
            scene_id: None,
            destination: directory.join("trimmed-existing.mp4"),
            width,
            height,
            frame_rate: FrameRate::new(fps, 1),
            quality: ExportQuality::VeryHigh,
            include_audio: true,
            matte_root: None,
            backend,
        };
        let media = MediaMap::new().with("clip", source);
        let started = Instant::now();
        let outcome = run(&trim, &media, &cancel, &mut |_| {}).expect("trim");
        println!(
            "trim {width}x{height}@{fps}: {} frames in {:?} via {}",
            outcome.artifacts.frames,
            started.elapsed(),
            outcome.encoder
        );
        return;
    }

    let source = directory.join("source.mp4");

    let render = ExportRequest {
        project: Arc::new(clip_project("image", width, height, 0.0, seconds)),
        scene_id: None,
        destination: source.clone(),
        width,
        height,
        frame_rate: FrameRate::FPS_30,
        quality: ExportQuality::VeryHigh,
        include_audio: false,
        matte_root: None,
        backend,
    };
    let media = MediaMap::new().with("clip", still);
    let started = Instant::now();
    let outcome = run(&render, &media, &cancel, &mut |_| {}).expect("source render");
    println!(
        "render  {width}x{height}: {} frames in {:?} ({:.2} fps) via {}",
        outcome.artifacts.frames,
        started.elapsed(),
        outcome.frames_per_second(),
        outcome.encoder
    );

    let trim = ExportRequest {
        project: Arc::new(clip_project("video", width, height, 0.0, seconds / 2.0)),
        scene_id: None,
        destination: directory.join("trimmed.mp4"),
        width,
        height,
        frame_rate: FrameRate::FPS_30,
        quality: ExportQuality::VeryHigh,
        include_audio: false,
        matte_root: None,
        backend,
    };
    let media = MediaMap::new().with("clip", source);
    let started = Instant::now();
    let outcome = run(&trim, &media, &cancel, &mut |_| {}).expect("trim");
    println!(
        "trim    {width}x{height}: {} frames in {:?} ({:.2} fps) via {}",
        outcome.artifacts.frames,
        started.elapsed(),
        outcome.frames_per_second(),
        outcome.encoder
    );
}
