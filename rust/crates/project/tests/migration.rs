#![recursion_limit = "512"]

use cutix_project::migrate::{detect_version, migrate_to_current, CURRENT_PROJECT_VERSION};
use cutix_project::model::Project;
use serde_json::{json, Value};

const NOW: &str = "2024-01-01T00:00:00.000Z";
const TICKS_PER_SECOND: f64 = 120_000.0;

fn v0_document() -> Value {
    json!({
        "id": "project-v0",
        "name": "Legacy project",
        "createdAt": "2023-01-01T00:00:00.000Z",
        "updatedAt": "2023-01-02T00:00:00.000Z",
        "fps": 30,
        "canvasSize": { "width": 1920, "height": 1080 },
        "backgroundColor": "#123456"
    })
}

fn v2_document() -> Value {
    json!({
        "metadata": {
            "id": "project-v2",
            "name": "Rich project",
            "createdAt": "2023-01-01T00:00:00.000Z",
            "updatedAt": "2023-01-02T00:00:00.000Z"
        },
        "currentSceneId": "scene-1",
        "version": 2,
        "settings": {
            "fps": 29.97,
            "canvasSize": { "width": 1920, "height": 1080 },
            "background": { "type": "blur", "blurIntensity": 10 },
            "originalCanvasSize": null
        },
        "scenes": [{
            "id": "scene-1",
            "name": "Main scene",
            "isMain": true,
            "createdAt": "2023-01-01T00:00:00.000Z",
            "updatedAt": "2023-01-01T00:00:00.000Z",
            "bookmarks": [1.5],
            "tracks": [
                {
                    "id": "track-main",
                    "name": "Main",
                    "type": "video",
                    "isMain": true,
                    "muted": false,
                    "hidden": false,
                    "elements": [{
                        "id": "element-video",
                        "name": "clip.mp4",
                        "type": "video",
                        "mediaId": "media-1",
                        "duration": 4.0,
                        "startTime": 1.0,
                        "trimStart": 0.5,
                        "trimEnd": 0.25,
                        "opacity": 1,
                        "volume": 0.5,
                        "transform": { "scale": 2, "position": { "x": 10, "y": 20 }, "rotate": 0 },
                        "animations": {
                            "channels": {
                                "transform.position.x": {
                                    "valueKind": "number",
                                    "keyframes": [
                                        { "id": "kx1", "time": 0, "value": 0, "interpolation": "linear" },
                                        { "id": "kx2", "time": 2, "value": 100, "interpolation": "linear" }
                                    ]
                                },
                                "transform.position.y": {
                                    "valueKind": "number",
                                    "keyframes": [
                                        { "id": "ky1", "time": 0, "value": 5, "interpolation": "linear" }
                                    ]
                                }
                            }
                        },
                        "masks": [{
                            "id": "mask-1",
                            "type": "split",
                            "feather": 3,
                            "inverted": true,
                            "stroke": { "color": "#ff0000", "width": 2 },
                            "params": { "position": 0.75, "rotation": 0 }
                        }],
                        "transition": { "type": "crossfade", "duration": 0.5 }
                    }]
                },
                {
                    "id": "track-text",
                    "name": "Text",
                    "type": "text",
                    "hidden": false,
                    "elements": [{
                        "id": "element-text",
                        "name": "Title",
                        "type": "text",
                        "content": "Hello",
                        "fontSize": 48,
                        "fontFamily": "Arial",
                        "color": "#ffffff",
                        "backgroundColor": "#000000",
                        "textAlign": "center",
                        "fontWeight": "bold",
                        "fontStyle": "normal",
                        "textDecoration": "none",
                        "duration": 2.0,
                        "startTime": 0.0,
                        "trimStart": 0.0,
                        "trimEnd": 0.0,
                        "opacity": 1,
                        "transform": { "scale": 1, "position": { "x": 0, "y": 0 }, "rotate": 0 }
                    }]
                },
                {
                    "id": "track-sticker",
                    "name": "Stickers",
                    "type": "sticker",
                    "hidden": false,
                    "elements": [{
                        "id": "element-sticker",
                        "name": "Star",
                        "type": "sticker",
                        "iconName": "star",
                        "color": "#ff00ff",
                        "duration": 1.0,
                        "startTime": 0.0,
                        "trimStart": 0.0,
                        "trimEnd": 0.0,
                        "opacity": 1,
                        "transform": { "scale": 1, "position": { "x": 0, "y": 0 }, "rotate": 0 }
                    }]
                },
                {
                    "id": "track-audio",
                    "name": "Audio",
                    "type": "audio",
                    "muted": false,
                    "elements": [{
                        "id": "element-audio",
                        "name": "music.mp3",
                        "type": "audio",
                        "sourceType": "upload",
                        "mediaId": "media-2",
                        "volume": 1,
                        "duration": 3.0,
                        "startTime": 0.0,
                        "trimStart": 0.0,
                        "trimEnd": 0.0
                    }]
                }
            ]
        }]
    })
}

#[test]
fn detects_legacy_versions() {
    assert_eq!(detect_version(&v0_document()), 0);
    assert_eq!(detect_version(&json!({ "scenes": [{ "id": "a" }] })), 1);
    assert_eq!(detect_version(&v2_document()), 2);
}

#[test]
fn migrates_v0_document_to_current_and_parses() {
    let (migrated, report) = migrate_to_current(v0_document(), NOW);

    assert_eq!(report.from_version, 0);
    assert_eq!(report.to_version, i64::from(CURRENT_PROJECT_VERSION));
    assert_eq!(report.applied.len(), 30);
    assert_eq!(
        migrated["version"].as_u64(),
        Some(u64::from(CURRENT_PROJECT_VERSION))
    );

    let project: Project = serde_json::from_value(migrated).expect("v0 document parses at v30");
    assert_eq!(project.metadata.id, "project-v0");
    assert_eq!(project.metadata.name, "Legacy project");
    assert_eq!(project.scenes.len(), 1);
    assert!(project.scenes[0].is_main);
    assert_eq!(project.current_scene_id, project.scenes[0].id);
    assert_eq!(project.settings.fps.numerator, 30);
    assert_eq!(project.settings.fps.denominator, 1);
}

#[test]
fn migrates_rich_v2_document_with_web_semantics() {
    let (migrated, report) = migrate_to_current(v2_document(), NOW);
    assert_eq!(report.to_version, 30);

    let project: Project =
        serde_json::from_value(migrated.clone()).expect("v2 document parses at v30");

    let scene = &project.scenes[0];
    let main_elements = scene.tracks.main.elements();
    assert_eq!(main_elements.len(), 1);
    let base = main_elements[0].base();
    assert_eq!(base.duration.as_ticks(), (4.0 * TICKS_PER_SECOND) as i64);
    assert_eq!(base.start_time.as_ticks(), (1.0 * TICKS_PER_SECOND) as i64);

    assert_eq!(
        base.source_duration.map(|value| value.as_ticks()),
        Some((4.75 * TICKS_PER_SECOND) as i64)
    );
    assert_eq!(
        project.metadata.duration.as_ticks(),
        (5.0 * TICKS_PER_SECOND) as i64
    );

    assert_eq!(project.settings.fps.numerator, 30_000);
    assert_eq!(project.settings.fps.denominator, 1_001);

    match &project.settings.background {
        cutix_project::Background::Blur { blur_intensity } => {
            assert!((blur_intensity - 50.0).abs() < 1e-9);
        }
        other => panic!("expected blur background, got {other:?}"),
    }

    assert_eq!(scene.bookmarks.len(), 1);
    assert_eq!(
        scene.bookmarks[0].time.as_ticks(),
        (1.5 * TICKS_PER_SECOND) as i64
    );

    assert_eq!(scene.tracks.main.id(), "track-main");
    assert_eq!(scene.tracks.audio.len(), 1);
    assert_eq!(scene.tracks.overlay.len(), 2);
    assert!(matches!(
        scene.tracks.overlay[1],
        cutix_project::Track::Graphic { .. }
    ));

    let element = &migrated["scenes"][0]["tracks"]["main"]["elements"][0];

    assert_eq!(element["transform"]["scaleX"].as_f64(), Some(2.0));
    assert_eq!(element["transform"]["scaleY"].as_f64(), Some(2.0));
    assert!(element["transform"].get("scale").is_none());

    let volume = element["volume"].as_f64().expect("volume is numeric");
    assert!((volume - (20.0 * 0.5_f64.log10())).abs() < 1e-9);

    assert_eq!(element["isSourceAudioEnabled"].as_bool(), Some(true));

    assert_eq!(element["crop"]["left"].as_f64(), Some(0.0));
    assert_eq!(element["crop"]["bottom"].as_f64(), Some(0.0));

    let mask = &element["masks"][0];
    assert_eq!(mask["params"]["feather"].as_f64(), Some(3.0));
    assert_eq!(mask["params"]["inverted"].as_bool(), Some(true));
    assert_eq!(mask["params"]["strokeColor"].as_str(), Some("#ff0000"));
    assert_eq!(mask["params"]["strokeWidth"].as_f64(), Some(2.0));
    assert_eq!(mask["params"]["strokeAlign"].as_str(), Some("center"));
    assert!((mask["params"]["x"].as_f64().unwrap() - 0.25).abs() < 1e-9);
    assert!(mask["params"].get("position").is_none());

    assert_eq!(element["transition"]["easing"].as_str(), Some("easeInOut"));
    assert_eq!(element["transition"]["duration"].as_f64(), Some(1.0));

    let bindings = &element["animations"]["bindings"];
    assert!(bindings.get("transform.position").is_none());
    assert_eq!(
        bindings["transform.positionX"]["kind"].as_str(),
        Some("number")
    );
    assert_eq!(
        bindings["transform.positionY"]["components"][0]["channelId"].as_str(),
        Some("transform.positionY:value")
    );
    let x_keys = element["animations"]["channels"]["transform.positionX:value"]["keys"]
        .as_array()
        .expect("x channel keys");
    assert_eq!(x_keys.len(), 2);
    assert_eq!(x_keys[0]["segmentToNext"].as_str(), Some("linear"));
    assert_eq!(x_keys[0]["tangentMode"].as_str(), Some("flat"));
    assert_eq!(x_keys[1]["value"].as_f64(), Some(100.0));
    assert_eq!(x_keys[1]["time"].as_f64(), Some(2.0 * TICKS_PER_SECOND));

    let y_keys = element["animations"]["channels"]["transform.positionY:value"]["keys"]
        .as_array()
        .expect("y channel keys");
    assert_eq!(y_keys.len(), 2);
    assert_eq!(y_keys[1]["value"].as_f64(), Some(5.0));

    let text = &migrated["scenes"][0]["tracks"]["overlay"][0]["elements"][0];
    assert_eq!(text["fontWeight"].as_str(), Some("700"));
    assert_eq!(text["background"]["enabled"].as_bool(), Some(true));
    assert_eq!(text["background"]["color"].as_str(), Some("#000000"));
    assert!(text.get("backgroundColor").is_none());

    let sticker = &migrated["scenes"][0]["tracks"]["overlay"][1]["elements"][0];
    assert_eq!(sticker["stickerId"].as_str(), Some("icons:star"));
    assert!(sticker.get("iconName").is_none());
    assert!(sticker.get("color").is_none());
    assert_eq!(sticker["intrinsicWidth"].as_f64(), Some(200.0));
}

#[test]
fn migration_is_idempotent_at_current_version() {
    let (migrated, _) = migrate_to_current(v2_document(), NOW);
    let (again, report) = migrate_to_current(migrated.clone(), NOW);

    assert!(report.applied.is_empty());
    assert_eq!(again, migrated);
}

#[test]
fn v18_to_v19_backfills_canvas_size_mode() {
    let document = json!({
        "metadata": { "id": "p" },
        "version": 18,
        "settings": { "canvasSize": { "width": 1080, "height": 1920 } }
    });

    let result = cutix_project::migrate::transformers::v18_to_v19(document);

    assert!(!result.skipped);
    assert_eq!(
        result.project["settings"]["canvasSizeMode"].as_str(),
        Some("preset")
    );
    assert!(result.project["settings"]["lastCustomCanvasSize"].is_null());
}

#[test]
fn v29_to_v30_converts_watermark_offsets_to_ratios() {
    let document = json!({
        "metadata": { "id": "p" },
        "version": 29,
        "settings": {
            "canvasSize": { "width": 1920, "height": 1080 },
            "watermark": {
                "enabled": true,
                "source": { "type": "text", "text": "cutix" },
                "anchor": "bottomRight",
                "offset": { "x": 192, "y": 108 },
                "size": 0.2,
                "opacity": 0.5
            }
        }
    });

    let result = cutix_project::migrate::transformers::v29_to_v30(document);
    let watermark = &result.project["settings"]["watermark"];

    assert!(!result.skipped);
    assert_eq!(watermark["offset"]["x"].as_f64(), Some(0.1));
    assert_eq!(watermark["offset"]["y"].as_f64(), Some(0.1));
    assert_eq!(watermark["source"]["fontFamily"].as_str(), Some("Arial"));
    assert_eq!(watermark["source"]["fontWeight"].as_f64(), Some(600.0));
    assert_eq!(watermark["tiling"]["spacing"].as_f64(), Some(0.6));
    assert_eq!(watermark["timing"]["mode"].as_str(), Some("always"));
    assert_eq!(watermark["blendMode"].as_str(), Some("normal"));
}

#[test]
fn v21_to_v22_converts_color_channels_to_linear_components() {
    let document = json!({
        "metadata": { "id": "p" },
        "version": 21,
        "scenes": [{
            "id": "s",
            "tracks": [{
                "id": "t",
                "type": "text",
                "elements": [{
                    "id": "e",
                    "type": "text",
                    "animations": {
                        "channels": {
                            "color": {
                                "valueKind": "color",
                                "keyframes": [
                                    { "id": "k1", "time": 0, "value": "#ffffff", "interpolation": "hold" }
                                ]
                            }
                        }
                    }
                }]
            }]
        }]
    });

    let result = cutix_project::migrate::transformers::v21_to_v22(document);
    let animations = &result.project["scenes"][0]["tracks"][0]["elements"][0]["animations"];

    assert_eq!(
        animations["bindings"]["color"]["kind"].as_str(),
        Some("color")
    );
    assert_eq!(
        animations["bindings"]["color"]["colorSpace"].as_str(),
        Some("srgb-linear")
    );
    for component in ["r", "g", "b", "a"] {
        let key = &animations["channels"][format!("color:{component}")]["keys"][0];
        assert_eq!(key["value"].as_f64(), Some(1.0));
        assert_eq!(key["segmentToNext"].as_str(), Some("step"));
    }
}

#[test]
fn migration_stops_when_a_step_is_skipped() {
    let document = json!({ "version": 5, "scenes": [] });
    let (_, report) = migrate_to_current(document, NOW);

    assert!(report.applied.is_empty());
    assert_eq!(report.stopped_reason.as_deref(), Some("no project id"));
}
