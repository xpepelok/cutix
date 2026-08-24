use std::sync::Arc;
use std::time::{Duration, Instant};

use cutix_playback::{MediaMap, PlaybackController};
use cutix_project::Project;
use serde_json::json;
use time::{FrameRate, MediaTime};

fn main() {
    let clip = std::env::args().nth(1).expect("clip path");
    let width: u32 = std::env::args()
        .nth(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(830);
    let height: u32 = std::env::args()
        .nth(3)
        .and_then(|value| value.parse().ok())
        .unwrap_or(470);
    let fps: u32 = std::env::args()
        .nth(4)
        .and_then(|value| value.parse().ok())
        .unwrap_or(60);
    let seconds: f64 = std::env::args()
        .nth(5)
        .and_then(|value| value.parse().ok())
        .unwrap_or(6.0);

    let rate = FrameRate::new(fps, 1);
    let mut project = Project::new("player-rate", "1970-01-01T00:00:00.000Z".into());
    project.settings.canvas_size.width = 1920;
    project.settings.canvas_size.height = 1080;
    project.settings.fps = rate;
    let element = serde_json::from_value(json!({
        "type": "video",
        "id": "clip",
        "name": "clip",
        "duration": MediaTime::from_seconds_f64(60.0).expect("time").as_ticks(),
        "startTime": 0,
        "trimStart": 0,
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

    let media = MediaMap::new().with("clip", clip);
    let controller = PlaybackController::new(Arc::new(project), None, Box::new(media), None)
        .expect("controller");
    while !controller.is_ready() {
        std::thread::sleep(Duration::from_millis(5));
    }

    let frame = rate.frame_duration().expect("a valid frame rate");
    controller.play();
    controller.stream_from(MediaTime::ZERO, width, height, frame);

    let started = Instant::now();
    let mut presented = 0u64;
    let mut last = 0u64;
    while started.elapsed().as_secs_f64() < seconds {
        if let Some(slot) = controller.latest_frame()
            && slot.revision != last
        {
            last = slot.revision;
            presented += 1;
        }
        std::thread::sleep(Duration::from_micros(500));
    }
    let elapsed = started.elapsed().as_secs_f64();
    println!(
        "player {width}x{height} at {fps} fps: {presented} frames in {elapsed:.2}s ({:.1} fps presented)",
        presented as f64 / elapsed
    );
}
