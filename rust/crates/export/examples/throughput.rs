use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use cutix_export::{default_backend, run, ExportQuality, ExportRequest};
use cutix_playback::MediaMap;
use cutix_project::Project;
use serde_json::json;
use time::{FrameRate, MediaTime};

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
    let frames: u32 = arguments.next().and_then(|v| v.parse().ok()).unwrap_or(90);
    let clip = arguments.next();

    let directory = std::env::temp_dir().join("cutix-throughput");
    std::fs::create_dir_all(&directory).expect("temp dir");
    let source = directory.join("still.png");
    let mut buffer = image::RgbaImage::new(256, 256);
    for y in 0..256u32 {
        for x in 0..256u32 {
            buffer.put_pixel(x, y, image::Rgba([x as u8, y as u8, 128, 255]));
        }
    }
    buffer.save(&source).expect("write png");

    let seconds = frames as f64 / 30.0;
    let duration = MediaTime::from_seconds_f64(seconds).expect("time");
    let mut project = Project::new("throughput", "1970-01-01T00:00:00.000Z".into());
    project.settings.canvas_size.width = width;
    project.settings.canvas_size.height = height;
    project.settings.fps = FrameRate::FPS_30;
    let element = serde_json::from_value(json!({
        "type": if clip.is_some() { "video" } else { "image" },
        "id": "still-element",
        "name": "still",
        "duration": duration.as_ticks(),
        "startTime": 0,
        "trimStart": 0,
        "trimEnd": 0,
        "mediaId": "still",
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

    let request = ExportRequest {
        project: Arc::new(project),
        scene_id: None,
        destination: directory.join("out.mp4"),
        width,
        height,
        frame_rate: FrameRate::FPS_30,
        quality: ExportQuality::VeryHigh,
        include_audio: false,
        matte_root: None,
        backend: default_backend().expect("backend"),
    };

    let media = MediaMap::new().with(
        "still",
        clip.map(std::path::PathBuf::from).unwrap_or(source),
    );
    let cancel = AtomicBool::new(false);
    let outcome = run(&request, &media, &cancel, &mut |_| {}).expect("export");
    println!(
        "{width}x{height}: {} frames in {:?} ({:.2} fps)",
        outcome.artifacts.frames,
        outcome.elapsed,
        outcome.frames_per_second()
    );
}
