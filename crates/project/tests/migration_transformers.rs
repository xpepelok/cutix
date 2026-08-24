#![recursion_limit = "512"]

use cutix_project::migrate::transformers::*;
use cutix_project::migrate::util::get_project_id;
use serde_json::{Value, json};

const NOW: &str = "2024-06-01T12:00:00.000Z";

fn v0_project() -> Value {
    json!({
        "id": "project-v0-123",
        "name": "My V0 Project",
        "createdAt": "2024-01-15T10:00:00.000Z",
        "updatedAt": "2024-01-15T12:00:00.000Z",
        "fps": 30,
        "canvasSize": { "width": 1920, "height": 1080 },
        "backgroundColor": "#000000",
        "backgroundType": "color",
        "bookmarks": [1.5, 3.0]
    })
}

fn v0_project_with_metadata() -> Value {
    json!({
        "id": "project-v0-456",
        "metadata": {
            "id": "project-v0-456",
            "name": "V0 With Metadata",
            "createdAt": "2024-02-01T08:00:00.000Z",
            "updatedAt": "2024-02-01T09:00:00.000Z"
        },
        "fps": 24,
        "canvasSize": { "width": 1280, "height": 720 },
        "backgroundType": "blur",
        "blurIntensity": 20
    })
}

fn v0_project_empty() -> Value {
    json!({
        "id": "project-empty",
        "name": "Empty Project",
        "createdAt": "2024-03-01T00:00:00.000Z",
        "updatedAt": "2024-03-01T00:00:00.000Z"
    })
}

fn project_with_no_id() -> Value {
    json!({ "name": "No ID Project", "version": 1, "scenes": [] })
}

fn project_with_null_values() -> Value {
    json!({
        "id": "project-nulls",
        "version": 1,
        "name": null,
        "metadata": null,
        "scenes": null,
        "settings": null
    })
}

fn project_malformed() -> Value {
    json!({ "id": "project-malformed" })
}

fn v1_project() -> Value {
    json!({
        "id": "project-v1-123",
        "version": 1,
        "name": "My V1 Project",
        "createdAt": "2024-01-15T10:00:00.000Z",
        "updatedAt": "2024-01-15T12:00:00.000Z",
        "fps": 30,
        "canvasSize": { "width": 1920, "height": 1080 },
        "backgroundColor": "#1a1a1a",
        "backgroundType": "color",
        "currentSceneId": "scene-main",
        "bookmarks": [2.0, 4.5, 7.0],
        "scenes": [{
            "id": "scene-main",
            "name": "Main scene",
            "isMain": true,
            "tracks": [],
            "bookmarks": [],
            "createdAt": "2024-01-15T10:00:00.000Z",
            "updatedAt": "2024-01-15T12:00:00.000Z"
        }]
    })
}

fn v1_project_with_multiple_scenes() -> Value {
    json!({
        "id": "project-v1-multi",
        "version": 1,
        "metadata": {
            "id": "project-v1-multi",
            "name": "Multi-Scene Project",
            "createdAt": "2024-02-20T14:00:00.000Z",
            "updatedAt": "2024-02-20T16:00:00.000Z"
        },
        "currentSceneId": "scene-1",
        "fps": 60,
        "canvasSize": { "width": 3840, "height": 2160 },
        "background": { "type": "blur", "blurIntensity": 15 },
        "scenes": [
            {
                "id": "scene-1",
                "name": "Intro",
                "isMain": true,
                "tracks": [],
                "bookmarks": [1.0],
                "createdAt": "2024-02-20T14:00:00.000Z",
                "updatedAt": "2024-02-20T16:00:00.000Z"
            },
            {
                "id": "scene-2",
                "name": "Content",
                "isMain": false,
                "tracks": [],
                "bookmarks": [],
                "createdAt": "2024-02-20T14:30:00.000Z",
                "updatedAt": "2024-02-20T16:00:00.000Z"
            }
        ]
    })
}

fn v2_project() -> Value {
    json!({
        "id": "project-v2-123",
        "version": 2,
        "metadata": {
            "id": "project-v2-123",
            "name": "My V2 Project",
            "thumbnail": "data:image/png;base64,abc123",
            "createdAt": "2024-03-01T10:00:00.000Z",
            "updatedAt": "2024-03-01T14:00:00.000Z"
        },
        "settings": {
            "fps": 30,
            "canvasSize": { "width": 1920, "height": 1080 },
            "background": { "type": "color", "color": "#000000" }
        },
        "currentSceneId": "scene-main",
        "scenes": [{
            "id": "scene-main",
            "name": "Main scene",
            "isMain": true,
            "tracks": [
                {
                    "id": "track-1",
                    "type": "video",
                    "name": "Video Track",
                    "isMain": true,
                    "elements": [{
                        "id": "element-1",
                        "type": "video",
                        "mediaId": "media-1",
                        "startTime": 0,
                        "duration": 15.5,
                        "trimStart": 0,
                        "trimEnd": 0
                    }]
                },
                {
                    "id": "track-2",
                    "type": "text",
                    "name": "Text Track",
                    "elements": [{
                        "id": "element-2",
                        "type": "text",
                        "content": "Hello World",
                        "startTime": 2,
                        "duration": 5
                    }]
                }
            ],
            "bookmarks": [5.0, 10.0],
            "createdAt": "2024-03-01T10:00:00.000Z",
            "updatedAt": "2024-03-01T14:00:00.000Z"
        }]
    })
}

fn v2_project_with_blur_background() -> Value {
    json!({
        "id": "project-v2-blur",
        "version": 2,
        "metadata": {
            "id": "project-v2-blur",
            "name": "Blur Background Project",
            "createdAt": "2024-03-15T08:00:00.000Z",
            "updatedAt": "2024-03-15T10:00:00.000Z"
        },
        "settings": {
            "fps": 24,
            "canvasSize": { "width": 1080, "height": 1920 },
            "background": { "type": "blur", "blurIntensity": 25 }
        },
        "currentSceneId": "scene-1",
        "scenes": [{
            "id": "scene-1",
            "name": "Main scene",
            "isMain": true,
            "tracks": [{
                "id": "track-1",
                "type": "video",
                "isMain": true,
                "elements": [{
                    "id": "el-1",
                    "type": "video",
                    "mediaId": "m1",
                    "startTime": 0,
                    "duration": 30
                }]
            }],
            "bookmarks": [],
            "createdAt": "2024-03-15T08:00:00.000Z",
            "updatedAt": "2024-03-15T10:00:00.000Z"
        }]
    })
}

fn v2_project_empty_scenes() -> Value {
    json!({
        "id": "project-v2-empty",
        "version": 2,
        "metadata": {
            "id": "project-v2-empty",
            "name": "Empty Scenes Project",
            "createdAt": "2024-04-01T00:00:00.000Z",
            "updatedAt": "2024-04-01T00:00:00.000Z"
        },
        "settings": {
            "fps": 30,
            "canvasSize": { "width": 1920, "height": 1080 },
            "background": { "type": "color", "color": "#ffffff" }
        },
        "currentSceneId": "",
        "scenes": []
    })
}

fn v2_project_scene_without_tracks() -> Value {
    json!({
        "id": "project-v2-no-tracks",
        "version": 2,
        "metadata": {
            "id": "project-v2-no-tracks",
            "name": "Scene Without Tracks",
            "createdAt": "2024-04-01T00:00:00.000Z",
            "updatedAt": "2024-04-01T00:00:00.000Z"
        },
        "settings": {
            "fps": 30,
            "canvasSize": { "width": 1920, "height": 1080 },
            "background": { "type": "color", "color": "#000000" }
        },
        "currentSceneId": "scene-1",
        "scenes": [{
            "id": "scene-1",
            "name": "Main Scene",
            "isMain": true,
            "createdAt": "2024-04-01T00:00:00.000Z",
            "updatedAt": "2024-04-01T00:00:00.000Z"
        }]
    })
}

fn v3_project() -> Value {
    json!({
        "id": "project-v3-123",
        "version": 3,
        "metadata": {
            "id": "project-v3-123",
            "name": "My V3 Project",
            "thumbnail": "data:image/png;base64,xyz789",
            "duration": 25.5,
            "createdAt": "2024-05-01T10:00:00.000Z",
            "updatedAt": "2024-05-01T14:00:00.000Z"
        },
        "settings": {
            "fps": 30,
            "canvasSize": { "width": 1920, "height": 1080 },
            "background": { "type": "color", "color": "#000000" }
        },
        "currentSceneId": "scene-main",
        "scenes": [{
            "id": "scene-main",
            "name": "Main scene",
            "isMain": true,
            "tracks": [{
                "id": "track-1",
                "type": "video",
                "name": "Video Track",
                "isMain": true,
                "elements": [{
                    "id": "element-1",
                    "type": "video",
                    "mediaId": "media-1",
                    "startTime": 0,
                    "duration": 25.5,
                    "trimStart": 0,
                    "trimEnd": 0
                }]
            }],
            "bookmarks": [],
            "createdAt": "2024-05-01T10:00:00.000Z",
            "updatedAt": "2024-05-01T14:00:00.000Z"
        }]
    })
}

fn v5_project() -> Value {
    json!({
        "id": "project-v5-456",
        "version": 5,
        "metadata": {
            "id": "project-v5-456",
            "name": "My V5 Project",
            "thumbnail": "data:image/png;base64,abc123",
            "duration": 30,
            "createdAt": "2024-06-01T10:00:00.000Z",
            "updatedAt": "2024-06-01T14:00:00.000Z"
        },
        "settings": {
            "fps": 30,
            "canvasSize": { "width": 1920, "height": 1080 },
            "background": { "type": "color", "color": "#000000" }
        },
        "currentSceneId": "scene-main",
        "scenes": [
            {
                "id": "scene-main",
                "name": "Main scene",
                "isMain": true,
                "tracks": [{
                    "id": "track-1",
                    "type": "video",
                    "name": "Video Track",
                    "isMain": true,
                    "elements": []
                }],
                "bookmarks": [2.0, 5.5, 12.0],
                "createdAt": "2024-06-01T10:00:00.000Z",
                "updatedAt": "2024-06-01T14:00:00.000Z"
            },
            {
                "id": "scene-intro",
                "name": "Intro",
                "isMain": false,
                "tracks": [{
                    "id": "track-2",
                    "type": "video",
                    "name": "Video Track",
                    "isMain": true,
                    "elements": []
                }],
                "bookmarks": [],
                "createdAt": "2024-06-01T10:00:00.000Z",
                "updatedAt": "2024-06-01T14:00:00.000Z"
            }
        ]
    })
}

fn scene_tracks(project: &Value) -> &Vec<Value> {
    project["scenes"][0]["tracks"].as_array().unwrap()
}

fn first_element(project: &Value) -> &Value {
    &scene_tracks(project)[0]["elements"][0]
}

#[test]
fn v0_to_v1_adds_scenes_array() {
    let result = v0_to_v1(v0_project(), NOW);

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(1));
    assert!(result.project["scenes"].is_array());
    assert_eq!(result.project["scenes"].as_array().unwrap().len(), 1);
    assert!(result.project.get("currentSceneId").is_some());
}

#[test]
fn v0_to_v1_creates_main_scene_with_correct_structure() {
    let result = v0_to_v1(v0_project(), NOW);
    let scene = &result.project["scenes"][0];

    assert_eq!(scene["isMain"].as_bool(), Some(true));
    assert_eq!(scene["name"].as_str(), Some("Main scene"));
    assert!(scene["id"].is_string());
    assert!(scene["tracks"].is_array());
    assert!(scene["bookmarks"].is_array());
}

#[test]
fn v0_to_v1_updates_metadata_updated_at_when_metadata_exists() {
    let result = v0_to_v1(v0_project_with_metadata(), NOW);

    assert_eq!(result.project["metadata"]["updatedAt"].as_str(), Some(NOW));
}

#[test]
fn v0_to_v1_updates_root_updated_at_when_no_metadata() {
    let result = v0_to_v1(v0_project_empty(), NOW);

    assert_eq!(result.project["updatedAt"].as_str(), Some(NOW));
}

#[test]
fn v0_to_v1_skips_project_that_already_has_scenes() {
    let result = v0_to_v1(v1_project(), NOW);

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("already has scenes"));
}

#[test]
fn v0_to_v1_preserves_original_project_properties() {
    let source = v0_project();
    let result = v0_to_v1(v0_project(), NOW);

    assert_eq!(result.project["id"], source["id"]);
    assert_eq!(result.project["name"], source["name"]);
    assert_eq!(result.project["fps"], source["fps"]);
    assert_eq!(result.project["canvasSize"], source["canvasSize"]);
}

#[test]
fn get_project_id_reads_root_metadata_and_missing() {
    assert_eq!(
        get_project_id(&v0_project()).as_deref(),
        Some("project-v0-123")
    );
    assert_eq!(
        get_project_id(&json!({ "metadata": { "id": "from-metadata" } })).as_deref(),
        Some("from-metadata")
    );
    assert_eq!(get_project_id(&project_with_no_id()), None);
    assert_eq!(
        get_project_id(&project_malformed()).as_deref(),
        Some("project-malformed")
    );
    assert_eq!(
        get_project_id(&json!({ "id": "root-id", "metadata": { "id": "metadata-id" } })).as_deref(),
        Some("root-id")
    );
    assert_eq!(
        get_project_id(&v1_project_with_multiple_scenes()).as_deref(),
        Some("project-v1-multi")
    );
}

#[test]
fn v1_to_v2_creates_metadata_object_from_flat_properties() {
    let result = v1_to_v2(v1_project(), NOW);

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(2));
    assert_eq!(
        result.project["metadata"]["id"].as_str(),
        Some("project-v1-123")
    );
    assert_eq!(
        result.project["metadata"]["name"].as_str(),
        Some("My V1 Project")
    );
    assert!(result.project["metadata"]["createdAt"].is_string());
    assert!(result.project["metadata"]["updatedAt"].is_string());
}

#[test]
fn v1_to_v2_creates_settings_object_from_flat_properties() {
    let result = v1_to_v2(v1_project(), NOW);
    let settings = &result.project["settings"];

    assert_eq!(settings["fps"].as_f64(), Some(30.0));
    assert_eq!(
        settings["canvasSize"],
        json!({ "width": 1920, "height": 1080 })
    );
    assert!(settings["originalCanvasSize"].is_null());
}

#[test]
fn v1_to_v2_converts_color_background() {
    let result = v1_to_v2(v1_project(), NOW);
    let background = &result.project["settings"]["background"];

    assert_eq!(background["type"].as_str(), Some("color"));
    assert_eq!(background["color"].as_str(), Some("#1a1a1a"));
}

#[test]
fn v1_to_v2_converts_blur_background() {
    let mut project = v1_project();
    project["backgroundType"] = json!("blur");
    project["blurIntensity"] = json!(30);

    let result = v1_to_v2(project, NOW);
    let background = &result.project["settings"]["background"];

    assert_eq!(background["type"].as_str(), Some("blur"));
    assert_eq!(background["blurIntensity"].as_f64(), Some(30.0));
}

#[test]
fn v1_to_v2_applies_legacy_bookmarks_to_main_scene() {
    let result = v1_to_v2(v1_project(), NOW);
    let scenes = result.project["scenes"].as_array().unwrap();
    let main = scenes
        .iter()
        .find(|scene| scene["isMain"] == json!(true))
        .unwrap();

    assert_eq!(main["bookmarks"], json!([2.0, 4.5, 7.0]));
}

#[test]
fn v1_to_v2_preserves_existing_scene_bookmarks() {
    let result = v1_to_v2(v1_project_with_multiple_scenes(), NOW);
    let scenes = result.project["scenes"].as_array().unwrap();
    let intro = scenes
        .iter()
        .find(|scene| scene["name"] == json!("Intro"))
        .unwrap();

    assert_eq!(intro["bookmarks"], json!([1.0]));
}

#[test]
fn v1_to_v2_skips_project_that_is_already_v2() {
    let result = v1_to_v2(v2_project(), NOW);

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("already v2"));
}

#[test]
fn v1_to_v2_skips_project_with_no_id() {
    let result = v1_to_v2(project_with_no_id(), NOW);

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("no project id"));
}

#[test]
fn v1_to_v2_handles_null_values_gracefully() {
    let result = v1_to_v2(project_with_null_values(), NOW);

    assert!(!result.skipped);
    assert_eq!(result.project["settings"]["fps"].as_f64(), Some(30.0));
    assert_eq!(
        result.project["settings"]["canvasSize"],
        json!({ "width": 1920, "height": 1080 })
    );
}

#[test]
fn v1_to_v2_uses_default_values_for_missing_properties() {
    let result = v1_to_v2(json!({ "id": "minimal", "version": 1, "scenes": [] }), NOW);
    let settings = &result.project["settings"];

    assert_eq!(settings["fps"].as_f64(), Some(30.0));
    assert_eq!(
        settings["canvasSize"],
        json!({ "width": 1920, "height": 1080 })
    );
    assert_eq!(settings["background"]["type"].as_str(), Some("color"));
    assert_eq!(settings["background"]["color"].as_str(), Some("#000000"));
}

#[test]
fn v1_to_v2_uses_default_blur_intensity_when_missing() {
    let result = v1_to_v2(
        json!({
            "id": "blur-no-intensity",
            "version": 1,
            "backgroundType": "blur",
            "scenes": []
        }),
        NOW,
    );

    assert_eq!(
        result.project["settings"]["background"]["blurIntensity"].as_f64(),
        Some(10.0)
    );
}

#[test]
fn v1_to_v2_preserves_current_scene_id() {
    let result = v1_to_v2(v1_project(), NOW);

    assert_eq!(
        result.project["currentSceneId"].as_str(),
        Some("scene-main")
    );
}

#[test]
fn v1_to_v2_finds_main_scene_id_when_current_scene_id_missing() {
    let mut project = v1_project();
    project.as_object_mut().unwrap().remove("currentSceneId");

    let result = v1_to_v2(project, NOW);

    assert_eq!(
        result.project["currentSceneId"].as_str(),
        Some("scene-main")
    );
}

#[test]
fn v1_to_v2_keeps_scene_tracks_that_already_exist() {
    let mut project = v1_project();
    project["scenes"] = json!([{
        "id": "scene-main",
        "name": "Main scene",
        "isMain": true,
        "tracks": [{
            "id": "track-1",
            "type": "video",
            "name": "Existing Track",
            "elements": []
        }],
        "bookmarks": [],
        "createdAt": "2024-01-15T10:00:00.000Z",
        "updatedAt": "2024-01-15T12:00:00.000Z"
    }]);

    let result = v1_to_v2(project, NOW);
    let tracks = scene_tracks(&result.project);

    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0]["name"].as_str(), Some("Existing Track"));
}

#[test]
fn v2_to_v3_adds_duration_to_metadata() {
    let result = v2_to_v3(v2_project());

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(3));
    assert!(result.project["metadata"]["duration"].is_number());
    assert_eq!(result.project["metadata"]["duration"].as_f64(), Some(15.5));
}

#[test]
fn v2_to_v3_handles_project_with_blur_background() {
    let result = v2_to_v3(v2_project_with_blur_background());

    assert!(!result.skipped);
    assert_eq!(result.project["metadata"]["duration"].as_f64(), Some(30.0));
}

#[test]
fn v2_to_v3_handles_empty_scenes_with_zero_duration() {
    let result = v2_to_v3(v2_project_empty_scenes());

    assert!(!result.skipped);
    assert_eq!(result.project["metadata"]["duration"].as_f64(), Some(0.0));
}

#[test]
fn v2_to_v3_handles_scene_without_tracks_property() {
    let result = v2_to_v3(v2_project_scene_without_tracks());

    assert!(!result.skipped);
    assert_eq!(result.project["metadata"]["duration"].as_f64(), Some(0.0));
}

#[test]
fn v2_to_v3_skips_project_that_is_already_v3() {
    let result = v2_to_v3(v3_project());

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("already v3"));
}

#[test]
fn v2_to_v3_skips_project_that_has_duration_in_metadata() {
    let mut project = v2_project();
    project["metadata"]["duration"] = json!(10);

    let result = v2_to_v3(project);

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("already v3"));
}

#[test]
fn v2_to_v3_skips_project_with_no_id() {
    let result = v2_to_v3(project_with_no_id());

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("no project id"));
}

#[test]
fn v2_to_v3_preserves_existing_metadata_settings_and_scenes() {
    let source = v2_project();
    let result = v2_to_v3(v2_project());
    let metadata = &result.project["metadata"];

    assert_eq!(metadata["id"], source["metadata"]["id"]);
    assert_eq!(metadata["name"], source["metadata"]["name"]);
    assert_eq!(metadata["thumbnail"], source["metadata"]["thumbnail"]);
    assert_eq!(metadata["createdAt"], source["metadata"]["createdAt"]);
    assert_eq!(metadata["updatedAt"], source["metadata"]["updatedAt"]);
    assert_eq!(result.project["settings"], source["settings"]);
    assert_eq!(result.project["scenes"], source["scenes"]);
}

#[test]
fn v2_to_v3_handles_project_without_metadata_object() {
    let result = v2_to_v3(json!({ "id": "no-metadata", "version": 2, "scenes": [] }));

    assert!(!result.skipped);
    assert_eq!(result.project["metadata"]["duration"].as_f64(), Some(0.0));
}

#[test]
fn v2_to_v3_calculates_duration_from_main_scene_only() {
    let result = v2_to_v3(json!({
        "id": "multi-scene",
        "version": 2,
        "metadata": { "id": "multi-scene", "name": "Multi" },
        "scenes": [
            {
                "id": "scene-1",
                "isMain": true,
                "tracks": [{ "type": "video", "elements": [{ "startTime": 0, "duration": 10 }] }]
            },
            {
                "id": "scene-2",
                "isMain": false,
                "tracks": [{ "type": "video", "elements": [{ "startTime": 0, "duration": 20 }] }]
            }
        ]
    }));

    assert_eq!(result.project["metadata"]["duration"].as_f64(), Some(10.0));
}

#[test]
fn v3_to_v4_normalizes_legacy_text_font_weight() {
    let mut project = v3_project();
    project["scenes"][0]["tracks"] = json!([{
        "id": "track-text",
        "type": "text",
        "name": "Text Track",
        "hidden": false,
        "elements": [{
            "id": "text-1",
            "type": "text",
            "name": "Title",
            "content": "Hello",
            "duration": 5,
            "startTime": 0,
            "trimStart": 0,
            "trimEnd": 0,
            "fontSize": 64,
            "fontFamily": "Inter",
            "color": "#ffffff",
            "backgroundColor": "transparent",
            "textAlign": "center",
            "fontWeight": "bold",
            "fontStyle": "normal",
            "textDecoration": "none",
            "transform": { "scale": 1, "position": { "x": 0, "y": 0 }, "rotate": 0 },
            "opacity": 1
        }]
    }]);

    let result = v3_to_v4(project);

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(4));
    assert_eq!(
        first_element(&result.project)["fontWeight"].as_str(),
        Some("700")
    );
}

#[test]
fn v3_to_v4_does_not_mutate_non_text_tracks() {
    let mut project = v3_project();
    project["scenes"][0]["tracks"] = json!([{
        "id": "track-sticker",
        "type": "sticker",
        "name": "Sticker Track",
        "hidden": false,
        "elements": [{
            "id": "sticker-1",
            "type": "sticker",
            "name": "Flag",
            "iconName": "mdi:home",
            "duration": 5,
            "startTime": 0,
            "trimStart": 0,
            "trimEnd": 0,
            "transform": { "scale": 1, "position": { "x": 0, "y": 0 }, "rotate": 0 },
            "opacity": 1
        }]
    }]);

    let result = v3_to_v4(project);

    assert_eq!(
        first_element(&result.project)["iconName"].as_str(),
        Some("mdi:home")
    );
}

#[test]
fn v3_to_v4_skips_projects_that_are_already_v4() {
    let mut project = v3_project();
    project["version"] = json!(4);

    let result = v3_to_v4(project);

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("already v4"));
}

#[test]
fn v3_to_v4_skips_projects_with_no_id() {
    let result = v3_to_v4(json!({ "version": 3, "scenes": [] }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("no project id"));
}

#[test]
fn v4_to_v5_migrates_sticker_icon_name_to_sticker_id() {
    let mut project = v3_project();
    project["version"] = json!(4);
    project["scenes"][0]["tracks"] = json!([{
        "id": "track-sticker",
        "type": "sticker",
        "name": "Sticker Track",
        "hidden": false,
        "elements": [{
            "id": "sticker-1",
            "type": "sticker",
            "name": "Home",
            "iconName": "mdi:home",
            "color": "#ff0000",
            "duration": 5,
            "startTime": 0,
            "trimStart": 0,
            "trimEnd": 0,
            "transform": { "scale": 1, "position": { "x": 0, "y": 0 }, "rotate": 0 },
            "opacity": 1
        }]
    }]);

    let result = v4_to_v5(project);

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(5));

    let element = first_element(&result.project);
    assert_eq!(element["stickerId"].as_str(), Some("icons:mdi:home"));
    assert!(element.get("iconName").is_none());
    assert!(element.get("color").is_none());
}

#[test]
fn v4_to_v5_keeps_provider_prefixed_sticker_ids() {
    let mut project = v3_project();
    project["version"] = json!(4);
    project["scenes"][0]["tracks"] = json!([{
        "id": "track-sticker",
        "type": "sticker",
        "name": "Sticker Track",
        "hidden": false,
        "elements": [{
            "id": "sticker-1",
            "type": "sticker",
            "name": "Flag",
            "stickerId": "flags:AD",
            "duration": 5,
            "startTime": 0,
            "trimStart": 0,
            "trimEnd": 0,
            "transform": { "scale": 1, "position": { "x": 0, "y": 0 }, "rotate": 0 },
            "opacity": 1
        }]
    }]);

    let result = v4_to_v5(project);

    assert_eq!(
        first_element(&result.project)["stickerId"].as_str(),
        Some("flags:AD")
    );
}

#[test]
fn v4_to_v5_skips_projects_that_are_already_v5() {
    let mut project = v3_project();
    project["version"] = json!(5);

    let result = v4_to_v5(project);

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("already v5"));
}

#[test]
fn v4_to_v5_skips_projects_with_no_id() {
    let result = v4_to_v5(json!({ "version": 4, "scenes": [] }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("no project id"));
}

#[test]
fn v5_to_v6_converts_number_bookmarks_to_objects() {
    let result = v5_to_v6(v5_project());

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(6));
    assert_eq!(
        result.project["scenes"][0]["bookmarks"],
        json!([{ "time": 2.0 }, { "time": 5.5 }, { "time": 12.0 }])
    );
    assert_eq!(result.project["scenes"][1]["bookmarks"], json!([]));
}

#[test]
fn v5_to_v6_skips_projects_that_are_already_v6() {
    let mut project = v5_project();
    project["version"] = json!(6);
    project["scenes"][0]["bookmarks"] = json!([{ "time": 2 }, { "time": 5 }]);

    let result = v5_to_v6(project);

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("already v6"));
}

#[test]
fn v5_to_v6_skips_projects_with_no_id() {
    let result = v5_to_v6(json!({ "version": 5, "scenes": [] }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("no project id"));
}

#[test]
fn v5_to_v6_preserves_existing_bookmark_objects() {
    let mut project = v5_project();
    project["version"] = json!(5);
    project["scenes"][0]["bookmarks"] = json!([
        { "time": 1, "note": "Intro", "color": "#ef4444" },
        { "time": 5.5, "duration": 2 }
    ]);

    let result = v5_to_v6(project);

    assert!(!result.skipped);
    assert_eq!(
        result.project["scenes"][0]["bookmarks"],
        json!([
            { "time": 1, "note": "Intro", "color": "#ef4444" },
            { "time": 5.5, "duration": 2 }
        ])
    );
}

fn v8_project_with_text() -> Value {
    json!({
        "id": "project-v8-text",
        "version": 8,
        "metadata": {
            "id": "project-v8-text",
            "name": "V8 Project with Text",
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z"
        },
        "settings": {
            "fps": 30,
            "canvasSize": { "width": 1920, "height": 1080 },
            "background": { "type": "color", "color": "#000000" }
        },
        "currentSceneId": "scene-main",
        "scenes": [{
            "id": "scene-main",
            "name": "Main scene",
            "isMain": true,
            "tracks": [{
                "id": "track-text",
                "type": "text",
                "name": "Text Track",
                "hidden": false,
                "elements": [
                    {
                        "id": "el-1",
                        "type": "text",
                        "content": "With color",
                        "startTime": 0,
                        "duration": 5,
                        "background": {
                            "color": "#ff0000",
                            "cornerRadius": 0,
                            "paddingX": 8,
                            "paddingY": 4
                        }
                    },
                    {
                        "id": "el-2",
                        "type": "text",
                        "content": "Transparent",
                        "startTime": 5,
                        "duration": 5,
                        "background": {
                            "color": "transparent",
                            "paddingX": 30,
                            "paddingY": 42
                        }
                    }
                ]
            }],
            "bookmarks": [],
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z"
        }]
    })
}

#[test]
fn v8_to_v9_adds_background_enabled_from_color() {
    let result = v8_to_v9(v8_project_with_text());

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(9));

    let elements = scene_tracks(&result.project)[0]["elements"]
        .as_array()
        .unwrap();
    assert_eq!(elements[0]["background"]["enabled"].as_bool(), Some(true));
    assert_eq!(elements[0]["background"]["color"].as_str(), Some("#ff0000"));
    assert_eq!(elements[1]["background"]["enabled"].as_bool(), Some(false));
    assert_eq!(
        elements[1]["background"]["color"].as_str(),
        Some("transparent")
    );
}

#[test]
fn v8_to_v9_preserves_existing_background_enabled() {
    let mut project = v8_project_with_text();
    project["scenes"][0]["tracks"] = json!([{
        "id": "track-text",
        "type": "text",
        "name": "Text Track",
        "hidden": false,
        "elements": [{
            "id": "el-1",
            "type": "text",
            "content": "Already has enabled",
            "startTime": 0,
            "duration": 5,
            "background": { "enabled": false, "color": "#00ff00" }
        }]
    }]);

    let result = v8_to_v9(project);

    assert!(!result.skipped);
    assert_eq!(
        first_element(&result.project)["background"]["enabled"].as_bool(),
        Some(false)
    );
}

#[test]
fn v8_to_v9_skips_non_text_elements_and_tracks() {
    let mut project = v8_project_with_text();
    project["scenes"] = json!([{
        "id": "scene-main",
        "name": "Main scene",
        "isMain": true,
        "tracks": [{
            "id": "track-video",
            "type": "video",
            "name": "Video Track",
            "isMain": true,
            "elements": []
        }],
        "bookmarks": [],
        "createdAt": "2024-01-01T00:00:00.000Z",
        "updatedAt": "2024-01-01T00:00:00.000Z"
    }]);

    let result = v8_to_v9(project);

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(9));
}

#[test]
fn v8_to_v9_skips_projects_that_are_already_v9() {
    let mut project = v8_project_with_text();
    project["version"] = json!(9);

    let result = v8_to_v9(project);

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("already v9"));
}

#[test]
fn v8_to_v9_skips_projects_with_no_id() {
    let result = v8_to_v9(json!({ "version": 8, "scenes": [] }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("no project id"));
}

#[test]
fn v15_to_v16_renames_sticker_tracks_to_graphic_tracks() {
    let result = v15_to_v16(json!({
        "id": "project-v15",
        "version": 15,
        "metadata": {
            "id": "project-v15",
            "name": "Project",
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z"
        },
        "settings": {
            "fps": 30,
            "canvasSize": { "width": 1920, "height": 1080 },
            "background": { "type": "color", "color": "#000000" }
        },
        "currentSceneId": "scene-main",
        "scenes": [{
            "id": "scene-main",
            "name": "Main scene",
            "isMain": true,
            "tracks": [{
                "id": "track-graphic",
                "type": "sticker",
                "name": "Sticker Track",
                "hidden": false,
                "elements": [{
                    "id": "sticker-1",
                    "type": "sticker",
                    "name": "Logo",
                    "stickerId": "icons:mdi:home",
                    "duration": 5,
                    "startTime": 0,
                    "trimStart": 0,
                    "trimEnd": 0,
                    "transform": {
                        "scaleX": 1,
                        "scaleY": 1,
                        "position": { "x": 0, "y": 0 },
                        "rotate": 0
                    },
                    "opacity": 1
                }]
            }],
            "bookmarks": [],
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z"
        }]
    }));

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(16));
    assert_eq!(
        scene_tracks(&result.project)[0]["type"].as_str(),
        Some("graphic")
    );
}

#[test]
fn v15_to_v16_skips_projects_already_on_v16() {
    let result = v15_to_v16(json!({ "id": "project-v16", "version": 16 }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("already v16"));
}

#[test]
fn v15_to_v16_skips_projects_with_no_id() {
    let result = v15_to_v16(json!({ "version": 15, "scenes": [] }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("no project id"));
}

#[test]
fn v16_to_v17_adds_center_stroke_alignment_to_masks_without_it() {
    let result = v16_to_v17(json!({
        "id": "project-v16",
        "version": 16,
        "metadata": {
            "id": "project-v16",
            "name": "Project",
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z"
        },
        "settings": {
            "fps": 30,
            "canvasSize": { "width": 1920, "height": 1080 },
            "background": { "type": "color", "color": "#000000" }
        },
        "currentSceneId": "scene-main",
        "scenes": [{
            "id": "scene-main",
            "name": "Main scene",
            "isMain": true,
            "tracks": [{
                "id": "track-video",
                "type": "video",
                "name": "Video Track",
                "hidden": false,
                "elements": [{
                    "id": "video-1",
                    "type": "video",
                    "name": "Clip",
                    "mediaId": "media-1",
                    "duration": 5,
                    "startTime": 0,
                    "trimStart": 0,
                    "trimEnd": 0,
                    "transform": {
                        "scaleX": 1,
                        "scaleY": 1,
                        "position": { "x": 0, "y": 0 },
                        "rotate": 0
                    },
                    "opacity": 1,
                    "masks": [
                        {
                            "id": "mask-1",
                            "type": "rectangle",
                            "params": {
                                "feather": 0,
                                "inverted": false,
                                "strokeColor": "#ffffff",
                                "strokeWidth": 8,
                                "centerX": 0,
                                "centerY": 0,
                                "width": 0.6,
                                "height": 0.6,
                                "rotation": 0,
                                "scale": 1
                            }
                        },
                        {
                            "id": "mask-2",
                            "type": "ellipse",
                            "params": {
                                "feather": 0,
                                "inverted": false,
                                "strokeColor": "#ffffff",
                                "strokeWidth": 8,
                                "strokeAlign": "outside",
                                "centerX": 0,
                                "centerY": 0,
                                "width": 0.6,
                                "height": 0.6,
                                "rotation": 0,
                                "scale": 1
                            }
                        }
                    ]
                }]
            }],
            "bookmarks": [],
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z"
        }]
    }));

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(17));

    let masks = &first_element(&result.project)["masks"];
    assert_eq!(masks[0]["params"]["strokeAlign"].as_str(), Some("center"));
    assert_eq!(masks[1]["params"]["strokeAlign"].as_str(), Some("outside"));
}

#[test]
fn v16_to_v17_skips_projects_already_on_v17() {
    let result = v16_to_v17(json!({ "id": "project-v17", "version": 17 }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("already v17"));
}

#[test]
fn v16_to_v17_skips_projects_with_no_id() {
    let result = v16_to_v17(json!({ "version": 16, "scenes": [] }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("no project id"));
}

#[test]
fn v18_to_v19_adds_canvas_size_mode_defaults() {
    let result = v18_to_v19(json!({
        "id": "project-v18-defaults",
        "version": 18,
        "metadata": {
            "id": "project-v18-defaults",
            "name": "Project",
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z"
        },
        "settings": {
            "fps": 30,
            "canvasSize": { "width": 1920, "height": 1080 },
            "originalCanvasSize": { "width": 1920, "height": 1080 },
            "background": { "type": "color", "color": "#000000" }
        },
        "currentSceneId": "scene-main",
        "scenes": []
    }));

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(19));
    assert_eq!(
        result.project["settings"]["canvasSizeMode"].as_str(),
        Some("preset")
    );
    assert!(result.project["settings"]["lastCustomCanvasSize"].is_null());
    assert_eq!(
        result.project["settings"]["originalCanvasSize"],
        json!({ "width": 1920, "height": 1080 })
    );
}

#[test]
fn v18_to_v19_skips_projects_already_on_v19() {
    let result = v18_to_v19(json!({ "id": "project-v19", "version": 19 }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("already v19"));
}

#[test]
fn v18_to_v19_skips_projects_with_no_id() {
    let result = v18_to_v19(json!({ "version": 18, "scenes": [] }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("no project id"));
}

#[test]
fn v19_to_v20_backfills_source_audio_enabled_on_video_elements() {
    let result = v19_to_v20(json!({
        "id": "project-v19-source-audio",
        "version": 19,
        "metadata": {
            "id": "project-v19-source-audio",
            "name": "Project",
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z"
        },
        "settings": {
            "fps": 30,
            "canvasSize": { "width": 1920, "height": 1080 },
            "background": { "type": "color", "color": "#000000" }
        },
        "currentSceneId": "scene-main",
        "scenes": [{
            "id": "scene-main",
            "name": "Main",
            "isMain": true,
            "bookmarks": [],
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z",
            "tracks": [
                {
                    "id": "track-video",
                    "type": "video",
                    "name": "Video",
                    "isMain": true,
                    "muted": false,
                    "hidden": false,
                    "elements": [{
                        "id": "video-1",
                        "type": "video",
                        "name": "Clip",
                        "mediaId": "media-1",
                        "duration": 5,
                        "startTime": 0,
                        "trimStart": 0,
                        "trimEnd": 0,
                        "transform": {
                            "position": { "x": 0, "y": 0 },
                            "scale": { "x": 1, "y": 1 },
                            "rotation": 0
                        },
                        "opacity": 1
                    }]
                },
                {
                    "id": "track-audio",
                    "type": "audio",
                    "name": "Audio",
                    "muted": false,
                    "elements": [{
                        "id": "audio-1",
                        "type": "audio",
                        "sourceType": "upload",
                        "mediaId": "media-audio-1",
                        "name": "Audio",
                        "duration": 5,
                        "startTime": 0,
                        "trimStart": 0,
                        "trimEnd": 0,
                        "volume": 0
                    }]
                }
            ]
        }]
    }));

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(20));

    let tracks = scene_tracks(&result.project);
    let video_elements = tracks[0]["elements"].as_array().unwrap();
    assert_eq!(video_elements.len(), 1);
    assert_eq!(video_elements[0]["id"].as_str(), Some("video-1"));
    assert_eq!(
        video_elements[0]["isSourceAudioEnabled"].as_bool(),
        Some(true)
    );
    assert!(
        tracks[1]["elements"][0]
            .get("isSourceAudioEnabled")
            .is_none()
    );
}

#[test]
fn v19_to_v20_preserves_existing_explicit_source_audio_state() {
    let result = v19_to_v20(json!({
        "id": "project-v19-existing-state",
        "version": 19,
        "scenes": [{
            "tracks": [{
                "elements": [{ "type": "video", "isSourceAudioEnabled": false }]
            }]
        }]
    }));

    assert_eq!(
        first_element(&result.project)["isSourceAudioEnabled"].as_bool(),
        Some(false)
    );
}

#[test]
fn v19_to_v20_skips_projects_already_on_v20() {
    let result = v19_to_v20(json!({ "id": "project-v20", "version": 20 }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("already v20"));
}

#[test]
fn v20_to_v21_multiplies_blur_intensity_by_five() {
    let result = v20_to_v21(json!({
        "id": "project-v20-blur",
        "version": 20,
        "metadata": {
            "id": "project-v20-blur",
            "name": "Project",
            "createdAt": "2024-01-01T00:00:00.000Z",
            "updatedAt": "2024-01-01T00:00:00.000Z"
        },
        "settings": {
            "fps": 30,
            "canvasSize": { "width": 1920, "height": 1080 },
            "background": { "type": "blur", "blurIntensity": 100 }
        },
        "currentSceneId": "scene-main",
        "scenes": []
    }));

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(21));
    assert_eq!(
        result.project["settings"]["background"]["blurIntensity"].as_f64(),
        Some(500.0)
    );
}

#[test]
fn v20_to_v21_uses_default_blur_intensity_when_missing() {
    let result = v20_to_v21(json!({
        "id": "project-v20-blur-no-intensity",
        "version": 20,
        "settings": { "background": { "type": "blur" } },
        "scenes": []
    }));

    assert!(!result.skipped);
    assert_eq!(
        result.project["settings"]["background"]["blurIntensity"].as_f64(),
        Some(50.0)
    );
}

#[test]
fn v20_to_v21_leaves_color_background_unchanged() {
    let result = v20_to_v21(json!({
        "id": "project-v20-color",
        "version": 20,
        "settings": { "background": { "type": "color", "color": "#000000" } },
        "scenes": []
    }));

    assert!(!result.skipped);
    assert_eq!(
        result.project["settings"]["background"],
        json!({ "type": "color", "color": "#000000" })
    );
}

#[test]
fn v20_to_v21_skips_projects_already_on_v21() {
    let result = v20_to_v21(json!({ "id": "project-v21", "version": 21 }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("already v21"));
}

#[test]
fn v20_to_v21_skips_projects_not_on_v20() {
    let result = v20_to_v21(json!({ "id": "project-v19", "version": 19 }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("not v20"));
}

#[test]
fn v21_to_v22_migrates_legacy_channels_to_bindings_and_component_channels() {
    let result = v21_to_v22(json!({
        "id": "project-v21-animations",
        "version": 21,
        "scenes": [{
            "id": "scene-1",
            "tracks": [{
                "id": "track-1",
                "elements": [{
                    "id": "element-1",
                    "type": "text",
                    "animations": {
                        "channels": {
                            "opacity": {
                                "valueKind": "number",
                                "keyframes": [{
                                    "id": "opacity-1",
                                    "time": 1,
                                    "value": 0.5,
                                    "interpolation": "linear"
                                }]
                            },
                            "transform.position": {
                                "valueKind": "vector",
                                "keyframes": [{
                                    "id": "position-1",
                                    "time": 2,
                                    "value": { "x": 10, "y": 20 },
                                    "interpolation": "hold"
                                }]
                            },
                            "color": {
                                "valueKind": "color",
                                "keyframes": [{
                                    "id": "color-1",
                                    "time": 3,
                                    "value": "#ff0000",
                                    "interpolation": "linear"
                                }]
                            },
                            "effects.effect-1.params.enabled": {
                                "valueKind": "discrete",
                                "keyframes": [{
                                    "id": "enabled-1",
                                    "time": 4,
                                    "value": true,
                                    "interpolation": "hold"
                                }]
                            }
                        }
                    }
                }]
            }]
        }]
    }));

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(22));

    let animations = &first_element(&result.project)["animations"];
    let bindings = &animations["bindings"];
    let channels = &animations["channels"];

    assert_eq!(
        bindings["opacity"],
        json!({
            "path": "opacity",
            "kind": "number",
            "components": [{ "key": "value", "channelId": "opacity:value" }]
        })
    );
    assert_eq!(
        bindings["transform.position"],
        json!({
            "path": "transform.position",
            "kind": "vector2",
            "components": [
                { "key": "x", "channelId": "transform.position:x" },
                { "key": "y", "channelId": "transform.position:y" }
            ]
        })
    );
    assert_eq!(
        bindings["color"],
        json!({
            "path": "color",
            "kind": "color",
            "colorSpace": "srgb-linear",
            "components": [
                { "key": "r", "channelId": "color:r" },
                { "key": "g", "channelId": "color:g" },
                { "key": "b", "channelId": "color:b" },
                { "key": "a", "channelId": "color:a" }
            ]
        })
    );
    assert_eq!(
        bindings["effects.effect-1.params.enabled"],
        json!({
            "path": "effects.effect-1.params.enabled",
            "kind": "discrete",
            "components": [{
                "key": "value",
                "channelId": "effects.effect-1.params.enabled:value"
            }]
        })
    );

    assert_eq!(
        channels["opacity:value"],
        json!({
            "kind": "scalar",
            "keys": [{
                "id": "opacity-1",
                "time": 1.0,
                "value": 0.5,
                "segmentToNext": "linear",
                "tangentMode": "flat"
            }]
        })
    );
    assert_eq!(
        channels["transform.position:x"],
        json!({
            "kind": "scalar",
            "keys": [{
                "id": "position-1",
                "time": 2.0,
                "value": 10.0,
                "segmentToNext": "step",
                "tangentMode": "flat"
            }]
        })
    );
    assert_eq!(
        channels["transform.position:y"],
        json!({
            "kind": "scalar",
            "keys": [{
                "id": "position-1",
                "time": 2.0,
                "value": 20.0,
                "segmentToNext": "step",
                "tangentMode": "flat"
            }]
        })
    );
    for (component, value) in [("r", 1.0), ("g", 0.0), ("b", 0.0), ("a", 1.0)] {
        let channel = &channels[format!("color:{component}")];
        assert_eq!(channel["kind"].as_str(), Some("scalar"));
        let key = &channel["keys"][0];
        assert_eq!(key["id"].as_str(), Some("color-1"));
        assert_eq!(key["time"].as_f64(), Some(3.0));
        assert_eq!(key["value"].as_f64(), Some(value));
        assert_eq!(key["segmentToNext"].as_str(), Some("linear"));
        assert_eq!(key["tangentMode"].as_str(), Some("flat"));
    }
    assert_eq!(
        channels["effects.effect-1.params.enabled:value"],
        json!({
            "kind": "discrete",
            "keys": [{ "id": "enabled-1", "time": 4.0, "value": true }]
        })
    );
}

#[test]
fn v21_to_v22_skips_projects_already_on_v22() {
    let result = v21_to_v22(json!({ "id": "project-v22", "version": 22 }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("already v22"));
}

#[test]
fn v21_to_v22_skips_projects_not_on_v21() {
    let result = v21_to_v22(json!({ "id": "project-v20", "version": 20 }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("not v21"));
}

#[test]
fn v22_to_v23_converts_seconds_to_ticks_and_fps_to_frame_rate() {
    let result = v22_to_v23(json!({
        "id": "project-v22-time",
        "version": 22,
        "metadata": {
            "id": "project-v22-time",
            "name": "Project",
            "duration": 15.5,
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": "2026-01-01T00:00:00.000Z"
        },
        "settings": {
            "fps": 29.97,
            "canvasSize": { "width": 1920, "height": 1080 },
            "background": { "type": "color", "color": "#000000" }
        },
        "timelineViewState": {
            "zoomLevel": 1,
            "scrollLeft": 120,
            "playheadTime": 1.25
        },
        "scenes": [{
            "id": "scene-1",
            "bookmarks": [
                { "time": 2.5, "duration": 0.75, "note": "Marker", "color": "#ff0000" },
                { "time": 4.5 }
            ],
            "tracks": [{
                "id": "track-1",
                "type": "video",
                "elements": [{
                    "id": "element-1",
                    "type": "video",
                    "startTime": 1.25,
                    "duration": 5.5,
                    "trimStart": 0.25,
                    "trimEnd": 0.5,
                    "sourceDuration": 6.25,
                    "animations": {
                        "bindings": {
                            "opacity": {
                                "path": "opacity",
                                "kind": "number",
                                "components": [{ "key": "value", "channelId": "opacity:value" }]
                            }
                        },
                        "channels": {
                            "opacity:value": {
                                "kind": "scalar",
                                "keys": [
                                    {
                                        "id": "key-1",
                                        "time": 0.5,
                                        "value": 1,
                                        "segmentToNext": "bezier",
                                        "tangentMode": "flat",
                                        "rightHandle": { "dt": 0.25, "dv": 0.2 }
                                    },
                                    {
                                        "id": "key-2",
                                        "time": 1.0,
                                        "value": 0.4,
                                        "segmentToNext": "linear",
                                        "tangentMode": "flat",
                                        "leftHandle": { "dt": -0.125, "dv": -0.1 }
                                    }
                                ]
                            }
                        }
                    }
                }]
            }],
            "createdAt": "2026-01-01T00:00:00.000Z",
            "updatedAt": "2026-01-01T00:00:00.000Z"
        }]
    }));

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(23));
    assert_eq!(
        result.project["metadata"]["duration"].as_i64(),
        Some(1_860_000)
    );
    assert_eq!(
        result.project["settings"]["fps"],
        json!({ "numerator": 30_000, "denominator": 1_001 })
    );
    assert_eq!(
        result.project["timelineViewState"]["playheadTime"].as_i64(),
        Some(150_000)
    );
    assert_eq!(
        result.project["timelineViewState"]["scrollLeft"].as_i64(),
        Some(120)
    );
    assert_eq!(
        result.project["scenes"][0]["bookmarks"],
        json!([
            { "time": 300_000, "duration": 90_000, "note": "Marker", "color": "#ff0000" },
            { "time": 540_000 }
        ])
    );

    let element = first_element(&result.project);
    assert_eq!(element["startTime"].as_i64(), Some(150_000));
    assert_eq!(element["duration"].as_i64(), Some(660_000));
    assert_eq!(element["trimStart"].as_i64(), Some(30_000));
    assert_eq!(element["trimEnd"].as_i64(), Some(60_000));
    assert_eq!(element["sourceDuration"].as_i64(), Some(750_000));
    assert_eq!(
        element["animations"]["channels"]["opacity:value"],
        json!({
            "kind": "scalar",
            "keys": [
                {
                    "id": "key-1",
                    "time": 60_000,
                    "value": 1,
                    "segmentToNext": "bezier",
                    "tangentMode": "flat",
                    "rightHandle": { "dt": 30_000, "dv": 0.2 }
                },
                {
                    "id": "key-2",
                    "time": 120_000,
                    "value": 0.4,
                    "segmentToNext": "linear",
                    "tangentMode": "flat",
                    "leftHandle": { "dt": -15_000, "dv": -0.1 }
                }
            ]
        })
    );
}

#[test]
fn v22_to_v23_skips_projects_already_on_v23() {
    let result = v22_to_v23(json!({ "id": "project-v23", "version": 23 }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("already v23"));
}

#[test]
fn v22_to_v23_skips_projects_not_on_v22() {
    let result = v22_to_v23(json!({ "id": "project-v21", "version": 21 }));

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("not v22"));
}

fn v27_project(elements: Value) -> Value {
    json!({
        "id": "project-v27",
        "version": 27,
        "metadata": { "id": "project-v27", "name": "V27" },
        "scenes": [{
            "id": "scene-main",
            "tracks": {
                "main": { "id": "main", "type": "video", "elements": elements },
                "overlay": [],
                "audio": []
            }
        }]
    })
}

fn v27_main_elements(project: &Value) -> &Vec<Value> {
    project["scenes"][0]["tracks"]["main"]["elements"]
        .as_array()
        .unwrap()
}

#[test]
fn v27_to_v28_normalizes_legacy_cutouts_to_static_mode() {
    let result = v27_to_v28(v27_project(json!([
        { "id": "a", "type": "video", "cutout": { "enabled": true, "png": [1, 2] } }
    ])));

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(28));
    assert_eq!(
        v27_main_elements(&result.project)[0]["cutout"]["mode"].as_str(),
        Some("static")
    );
}

#[test]
fn v27_to_v28_keeps_per_frame_cutouts_and_sorts_mattes() {
    let result = v27_to_v28(v27_project(json!([{
        "id": "a",
        "type": "video",
        "cutout": {
            "enabled": true,
            "mode": "perFrame",
            "png": [1],
            "frames": [
                { "sourceTime": 240000, "png": [3] },
                { "sourceTime": 0, "png": [1] },
                { "sourceTime": 120000, "png": [2] }
            ]
        }
    }])));

    let cutout = &v27_main_elements(&result.project)[0]["cutout"];
    assert_eq!(cutout["mode"].as_str(), Some("perFrame"));
    let times: Vec<i64> = cutout["frames"]
        .as_array()
        .unwrap()
        .iter()
        .map(|frame| frame["sourceTime"].as_i64().unwrap())
        .collect();
    assert_eq!(times, vec![0, 120000, 240000]);
}

#[test]
fn v27_to_v28_downgrades_per_frame_cutouts_with_no_frames() {
    let result = v27_to_v28(v27_project(json!([{
        "id": "a",
        "type": "video",
        "cutout": { "enabled": true, "mode": "perFrame", "png": [1], "frames": [] }
    }])));

    assert_eq!(
        v27_main_elements(&result.project)[0]["cutout"]["mode"].as_str(),
        Some("static")
    );
}

#[test]
fn v27_to_v28_keeps_text_reveal_and_drops_empty_animation_records() {
    let result = v27_to_v28(v27_project(json!([
        {
            "id": "a",
            "type": "text",
            "textAnimations": { "reveal": { "presetId": "typewriter", "duration": 180000 } }
        },
        { "id": "b", "type": "text", "textAnimations": {} }
    ])));

    let elements = v27_main_elements(&result.project);
    assert_eq!(
        elements[0]["textAnimations"]["reveal"]["presetId"].as_str(),
        Some("typewriter")
    );
    assert!(elements[1].get("textAnimations").is_none());
}

#[test]
fn v27_to_v28_skips_projects_that_are_already_v28() {
    let mut project = v27_project(json!([]));
    project["version"] = json!(28);

    let result = v27_to_v28(project);

    assert!(result.skipped);
    assert_eq!(result.reason.as_deref(), Some("already v28"));
}

#[test]
fn v27_to_v28_skips_projects_with_no_id() {
    let mut project = v27_project(json!([]));
    let object = project.as_object_mut().unwrap();
    object.remove("id");
    object.remove("metadata");

    let result = v27_to_v28(project);

    assert!(result.skipped);
}

const WATERMARK_CANVAS_WIDTH: f64 = 1920.0;
const WATERMARK_CANVAS_HEIGHT: f64 = 1080.0;

fn legacy_watermark() -> Value {
    json!({
        "enabled": true,
        "source": { "type": "image", "mediaId": "logo" },
        "anchor": "bottomRight",
        "offset": { "x": 58, "y": 58 },
        "size": 0.18,
        "opacity": 0.7
    })
}

fn legacy_watermark_project(watermark: Value) -> Value {
    json!({
        "id": "project-1",
        "version": 29,
        "settings": {
            "canvasSize": { "width": 1920, "height": 1080 },
            "background": { "type": "color", "color": "#000000" },
            "watermark": watermark
        }
    })
}

#[test]
fn v29_to_v30_bumps_version_and_fills_in_new_fields() {
    let result = v29_to_v30(legacy_watermark_project(legacy_watermark()));

    assert!(!result.skipped);
    assert_eq!(result.project["version"].as_i64(), Some(30));

    let watermark = &result.project["settings"]["watermark"];
    assert_eq!(watermark["rotation"].as_f64(), Some(0.0));
    assert_eq!(watermark["blendMode"].as_str(), Some("normal"));
    assert_eq!(
        watermark["tiling"],
        json!({ "enabled": false, "spacing": 0.6, "angle": 0.0 })
    );
    assert_eq!(
        watermark["timing"],
        json!({
            "mode": "always",
            "start": 0.0,
            "end": 0.0,
            "fadeIn": 0.0,
            "fadeOut": 0.0
        })
    );
}

#[test]
fn v29_to_v30_converts_the_pixel_offset_to_a_ratio() {
    let result = v29_to_v30(legacy_watermark_project(legacy_watermark()));
    let offset = &result.project["settings"]["watermark"]["offset"];

    assert!((offset["x"].as_f64().unwrap() - 58.0 / 1920.0).abs() < 1e-12);
    assert!((offset["y"].as_f64().unwrap() - 58.0 / 1080.0).abs() < 1e-12);
}

#[test]
fn v29_to_v30_renders_an_existing_watermark_at_the_same_place() {
    let source_width = 400.0;
    let source_height = 200.0;
    let legacy_width = 0.18 * WATERMARK_CANVAS_WIDTH;
    let legacy_height = legacy_width * (source_height / source_width);
    let before_x = WATERMARK_CANVAS_WIDTH - legacy_width - 58.0;
    let before_y = WATERMARK_CANVAS_HEIGHT - legacy_height - 58.0;

    let result = v29_to_v30(legacy_watermark_project(legacy_watermark()));
    let watermark = &result.project["settings"]["watermark"];

    let size = watermark["size"].as_f64().unwrap();
    let width = size * WATERMARK_CANVAS_WIDTH;
    let height = width * (source_height / source_width);
    let offset_x = watermark["offset"]["x"].as_f64().unwrap() * WATERMARK_CANVAS_WIDTH;
    let offset_y = watermark["offset"]["y"].as_f64().unwrap() * WATERMARK_CANVAS_HEIGHT;
    let after_x = WATERMARK_CANVAS_WIDTH - width - offset_x;
    let after_y = WATERMARK_CANVAS_HEIGHT - height - offset_y;

    assert_eq!(watermark["anchor"].as_str(), Some("bottomRight"));
    assert!((after_x - before_x).abs() < 1e-9);
    assert!((after_y - before_y).abs() < 1e-9);
    assert!((width - legacy_width).abs() < 1e-9);
    assert!((height - legacy_height).abs() < 1e-9);
}

#[test]
fn v29_to_v30_upgrades_a_legacy_text_source_with_default_typography() {
    let mut watermark = legacy_watermark();
    watermark["source"] = json!({ "type": "text", "text": "Draft", "color": "#ff0000" });

    let result = v29_to_v30(legacy_watermark_project(watermark));

    assert_eq!(
        result.project["settings"]["watermark"]["source"],
        json!({
            "type": "text",
            "text": "Draft",
            "color": "#ff0000",
            "fontFamily": "Arial",
            "fontWeight": 600.0,
            "stroke": { "enabled": false, "color": "#000000", "width": 0.0 },
            "shadow": {
                "enabled": false,
                "color": "#000000",
                "blur": 0.0,
                "offsetX": 0.0,
                "offsetY": 0.0
            }
        })
    );
}

#[test]
fn v29_to_v30_skips_projects_that_are_not_v29() {
    let mut project = legacy_watermark_project(legacy_watermark());
    project["version"] = json!(30);

    assert!(v29_to_v30(project).skipped);
}

#[test]
fn v29_to_v30_leaves_projects_without_a_watermark_untouched() {
    let result = v29_to_v30(json!({
        "id": "project-2",
        "version": 29,
        "settings": {
            "canvasSize": { "width": 1920, "height": 1080 },
            "background": { "type": "color", "color": "#000" }
        }
    }));

    assert_eq!(result.project["version"].as_i64(), Some(30));
    assert!(result.project["settings"].get("watermark").is_none());
}
