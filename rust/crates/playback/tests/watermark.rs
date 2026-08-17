use std::path::PathBuf;

use cutix_playback::render::{ComposeRequest, FrameComposer};
use cutix_playback::MediaMap;
use cutix_project::model::TimelineElement;
use cutix_project::Project;
use serde_json::json;
use time::MediaTime;

fn seconds(value: f64) -> MediaTime {
    MediaTime::from_seconds_f64(value).unwrap()
}

fn solid_png(name: &str, size: u32, rgba: [u8; 4]) -> PathBuf {
    let directory = std::env::temp_dir().join("cutix-playback-tests");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(name);
    let mut buffer = image::RgbaImage::new(size, size);
    for pixel in buffer.pixels_mut() {
        *pixel = image::Rgba(rgba);
    }
    buffer.save(&path).unwrap();
    path
}

fn background_element() -> TimelineElement {
    serde_json::from_value(json!({
        "type": "image",
        "id": "bg",
        "name": "bg",
        "duration": seconds(5.0).as_ticks(),
        "startTime": 0,
        "trimStart": 0,
        "trimEnd": 0,
        "mediaId": "bg",
        "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
        "opacity": 1.0
    }))
    .unwrap()
}

fn watermark_project(watermark: serde_json::Value) -> Project {
    let mut project = Project::new("test", "1970-01-01T00:00:00.000Z".into());
    project
        .scenes
        .first_mut()
        .unwrap()
        .tracks
        .main
        .elements_mut()
        .push(background_element());
    project.settings.canvas_size = cutix_project::model::CanvasSize {
        width: 64,
        height: 64,
    };
    project.settings.watermark = Some(watermark);
    project
}

fn watermark_settings(patch: serde_json::Value) -> serde_json::Value {
    let mut value = json!({
        "enabled": true,
        "source": { "type": "image", "mediaId": "wm" },
        "anchor": "bottomRight",
        "offset": { "x": 0.0, "y": 0.0 },
        "size": 0.25,
        "opacity": 1.0,
        "rotation": 0.0,
        "blendMode": "normal",
        "tiling": { "enabled": false, "spacing": 0.6, "angle": 0.0 },
        "timing": { "mode": "always", "start": 0.0, "end": 0.0, "fadeIn": 0.0, "fadeOut": 0.0 }
    });
    let object = value.as_object_mut().unwrap();
    for (key, entry) in patch.as_object().unwrap() {
        object.insert(key.clone(), entry.clone());
    }
    value
}

fn media() -> MediaMap {
    MediaMap::new()
        .with("bg", solid_png("wm-bg.png", 8, [0, 255, 0, 255]))
        .with("wm", solid_png("wm-mark.png", 8, [255, 0, 255, 255]))
}

fn is_magenta(pixel: [u8; 4]) -> bool {
    pixel[0] > 200 && pixel[1] < 55 && pixel[2] > 200
}

fn is_green(pixel: [u8; 4]) -> bool {
    pixel[0] < 55 && pixel[1] > 200 && pixel[2] < 55
}

#[test]
fn a_watermark_image_is_burned_into_the_bottom_right_corner_and_nowhere_else() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let media = media();

    let project = watermark_project(watermark_settings(json!({})));
    let frame = composer
        .compose(
            &ComposeRequest {
                project: &project,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 64,
                height: 64,
            },
            &media,
        )
        .unwrap();

    assert!(
        is_magenta(frame.pixel(60, 60)),
        "corner = {:?}",
        frame.pixel(60, 60)
    );
    assert!(
        is_magenta(frame.pixel(52, 52)),
        "corner = {:?}",
        frame.pixel(52, 52)
    );

    assert!(
        is_green(frame.pixel(4, 4)),
        "top-left = {:?}",
        frame.pixel(4, 4)
    );
    assert!(
        is_green(frame.pixel(60, 4)),
        "top-right = {:?}",
        frame.pixel(60, 4)
    );
    assert!(
        is_green(frame.pixel(4, 60)),
        "bottom-left = {:?}",
        frame.pixel(4, 60)
    );
    assert!(
        is_green(frame.pixel(32, 32)),
        "centre = {:?}",
        frame.pixel(32, 32)
    );
}

#[test]
fn moving_the_anchor_moves_the_burned_mark() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let media = media();

    let project = watermark_project(watermark_settings(json!({ "anchor": "topLeft" })));
    let frame = composer
        .compose(
            &ComposeRequest {
                project: &project,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 64,
                height: 64,
            },
            &media,
        )
        .unwrap();

    assert!(
        is_magenta(frame.pixel(4, 4)),
        "top-left = {:?}",
        frame.pixel(4, 4)
    );
    assert!(
        is_green(frame.pixel(60, 60)),
        "bottom-right = {:?}",
        frame.pixel(60, 60)
    );
}

#[test]
fn the_watermark_time_gate_renders_only_inside_its_window() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let media = media();

    let project = watermark_project(watermark_settings(json!({
        "timing": { "mode": "range", "start": 1.0, "end": 3.0, "fadeIn": 0.0, "fadeOut": 0.0 }
    })));

    let corner_at = |composer: &mut FrameComposer, t: f64| {
        composer
            .compose(
                &ComposeRequest {
                    project: &project,
                    scene_id: None,
                    time: seconds(t),
                    width: 64,
                    height: 64,
                },
                &media,
            )
            .unwrap()
            .pixel(60, 60)
    };

    assert!(
        is_green(corner_at(&mut composer, 0.5)),
        "pre-window should be clean"
    );
    assert!(
        is_green(corner_at(&mut composer, 4.0)),
        "post-window should be clean"
    );

    assert!(
        is_magenta(corner_at(&mut composer, 2.0)),
        "in-window should show mark"
    );
}

#[test]
fn the_watermark_fade_ramps_the_burned_opacity() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let media = media();

    let project = watermark_project(watermark_settings(json!({
        "timing": { "mode": "range", "start": 0.0, "end": 5.0, "fadeIn": 2.0, "fadeOut": 0.0 }
    })));

    let red_at = |composer: &mut FrameComposer, t: f64| {
        composer
            .compose(
                &ComposeRequest {
                    project: &project,
                    scene_id: None,
                    time: seconds(t),
                    width: 64,
                    height: 64,
                },
                &media,
            )
            .unwrap()
            .pixel(60, 60)[0] as i32
    };

    let early = red_at(&mut composer, 0.5);
    let mid = red_at(&mut composer, 1.0);
    let full = red_at(&mut composer, 3.0);
    eprintln!("fade red channel: early {early}, mid {mid}, full {full}");
    assert!(early < mid, "fade should climb: {early} < {mid}");
    assert!(mid < full, "fade should climb: {mid} < {full}");
    assert!((full - 255).abs() <= 2, "settled = {full}");
    assert!((mid - 128).abs() <= 14, "half fade = {mid}");
}

#[test]
fn a_tiled_watermark_repeats_across_the_frame() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let media = media();

    let single = watermark_project(watermark_settings(json!({
        "anchor": "center",
        "size": 0.12,
        "offset": { "x": 0.0, "y": 0.0 }
    })));
    let tiled = watermark_project(watermark_settings(json!({
        "anchor": "center",
        "size": 0.12,
        "offset": { "x": 0.0, "y": 0.0 },
        "tiling": { "enabled": true, "spacing": 0.2, "angle": 0.0 }
    })));

    let count_magenta = |composer: &mut FrameComposer, project: &Project| {
        let frame = composer
            .compose(
                &ComposeRequest {
                    project,
                    scene_id: None,
                    time: MediaTime::ZERO,
                    width: 64,
                    height: 64,
                },
                &media,
            )
            .unwrap();
        (0..64u32)
            .flat_map(|y| (0..64u32).map(move |x| (x, y)))
            .filter(|(x, y)| is_magenta(frame.pixel(*x, *y)))
            .count()
    };

    let single_count = count_magenta(&mut composer, &single);
    let tiled_count = count_magenta(&mut composer, &tiled);
    eprintln!("magenta pixels: single {single_count}, tiled {tiled_count}");
    assert!(single_count > 0, "single mark should burn in");

    assert!(
        tiled_count > single_count * 4,
        "tiled {tiled_count} vs single {single_count}"
    );
}

#[test]
fn a_text_watermark_burns_visible_ink() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let media = MediaMap::new().with("bg", solid_png("wm-bg.png", 8, [0, 0, 0, 255]));

    let project = watermark_project(watermark_settings(json!({
        "anchor": "center",
        "size": 0.6,
        "offset": { "x": 0.0, "y": 0.0 },
        "source": {
            "type": "text",
            "text": "WM",
            "color": "#ff0000",
            "fontFamily": "Arial",
            "fontWeight": 600.0,
            "stroke": { "enabled": false, "color": "#000000", "width": 0.0 },
            "shadow": { "enabled": false, "color": "#000000", "blur": 0.0, "offsetX": 0.0, "offsetY": 0.0 }
        }
    })));
    let frame = composer
        .compose(
            &ComposeRequest {
                project: &project,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 128,
                height: 128,
            },
            &media,
        )
        .unwrap();
    let red = (0..128u32)
        .flat_map(|y| (0..128u32).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let p = frame.pixel(*x, *y);
            p[0] > 150 && p[1] < 80 && p[2] < 80
        })
        .count();
    eprintln!("text watermark red ink pixels: {red}");
    assert!(red > 40, "text watermark should paint red ink, got {red}");
}
