use std::time::Instant;

use cutix_playback::{ComposeRequest, FrameComposer, MediaMap};
use cutix_project::Project;
use serde_json::json;
use time::{FrameRate, MediaTime};

fn main() {
    let clip = std::env::args().nth(1).expect("clip path");
    let width: u32 = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1920);
    let height: u32 = std::env::args()
        .nth(3)
        .and_then(|v| v.parse().ok())
        .unwrap_or(1080);
    let frames: u32 = std::env::args()
        .nth(4)
        .and_then(|v| v.parse().ok())
        .unwrap_or(60);

    let mut project = Project::new("compose-rate", "1970-01-01T00:00:00.000Z".into());
    project.settings.canvas_size.width = width;
    project.settings.canvas_size.height = height;
    project.settings.fps = FrameRate::FPS_60;
    let element = serde_json::from_value(json!({
        "type": "video",
        "id": "clip",
        "name": "clip",
        "duration": MediaTime::from_seconds_f64(30.0).expect("time").as_ticks(),
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
    let mut composer = FrameComposer::new().expect("composer");

    let realtime = std::env::args().any(|argument| argument == "--realtime");

    let mut warm = 0u32;
    let started = Instant::now();
    for index in 0..frames {
        let seconds = if realtime {
            started.elapsed().as_secs_f64()
        } else {
            index as f64 / 60.0
        };
        let time = MediaTime::from_seconds_f64(seconds).expect("time");
        let request = ComposeRequest {
            project: &project,
            scene_id: None,
            time,
            width,
            height,
        };
        if composer.compose(&request, &media).is_ok() {
            warm += 1;
        }
    }
    let elapsed = started.elapsed();
    println!(
        "compose {width}x{height} realtime={realtime}: {warm} frames in {elapsed:?} ({:.1} fps)",
        warm as f64 / elapsed.as_secs_f64()
    );
}
