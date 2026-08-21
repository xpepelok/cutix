use serde_json::{Map, Value, json};

use super::util::*;
use crate::color;

const VOLUME_DB_MIN: f64 = -60.0;
const VOLUME_DB_MAX: f64 = 20.0;
const INTENSITY_TO_SIGMA_DIVISOR: f64 = 5.0;
const LEGACY_DEFAULT_BACKGROUND_BLUR_INTENSITY: f64 = 50.0;
const STICKER_INTRINSIC_SIZE_FALLBACK: f64 = 200.0;
const DEFAULT_BACKGROUND_BLUR_INTENSITY: f64 = 10.0;
const DEFAULT_BACKGROUND_COLOR: &str = "#000000";
const DEFAULT_CANVAS_WIDTH: f64 = 1920.0;
const DEFAULT_CANVAS_HEIGHT: f64 = 1080.0;
const DEFAULT_FPS: f64 = 30.0;
const TICKS_PER_SECOND: f64 = ::time::TICKS_PER_SECOND as f64;

pub fn v0_to_v1(mut project: Value, now_iso: &str) -> MigrationResult {
    let has_scenes = project
        .get("scenes")
        .and_then(Value::as_array)
        .is_some_and(|scenes| !scenes.is_empty());
    if has_scenes {
        return MigrationResult::skipped(project, "already has scenes");
    }

    let scene_id = uuid::Uuid::new_v4().to_string();
    let main_scene = json!({
        "id": scene_id,
        "name": "Main scene",
        "isMain": true,
        "tracks": [],
        "bookmarks": [],
        "createdAt": now_iso,
        "updatedAt": now_iso,
    });

    let Some(object) = project.as_object_mut() else {
        return MigrationResult::skipped(project, "not an object");
    };

    object.insert("scenes".to_string(), json!([main_scene]));
    object.insert("currentSceneId".to_string(), Value::from(scene_id));
    object.insert("version".to_string(), Value::from(1));

    let metadata_is_object = object.get("metadata").is_some_and(Value::is_object);
    if metadata_is_object {
        if let Some(metadata) = object.get_mut("metadata").and_then(Value::as_object_mut) {
            metadata.insert("updatedAt".to_string(), Value::from(now_iso));
        }
    } else {
        object.insert("updatedAt".to_string(), Value::from(now_iso));
    }

    MigrationResult::migrated(project)
}

pub fn v1_to_v2(mut project: Value, now_iso: &str) -> MigrationResult {
    let Some(project_id) = get_project_id(&project) else {
        return MigrationResult::skipped(project, "no project id");
    };

    let already_v2 = version_of(&project).is_some_and(|version| version >= 2)
        || (project.get("metadata").is_some_and(Value::is_object)
            && project.get("settings").is_some_and(Value::is_object));
    if already_v2 {
        return MigrationResult::skipped(project, "already v2");
    }

    let normalize_date = |value: Option<&Value>| -> String {
        value
            .and_then(Value::as_str)
            .map(ToString::to_string)
            .unwrap_or_else(|| now_iso.to_string())
    };

    let metadata = match project.get("metadata") {
        Some(metadata) if metadata.is_object() => json!({
            "id": metadata.get("id").and_then(Value::as_str).unwrap_or(&project_id),
            "name": metadata.get("name").and_then(Value::as_str).unwrap_or(""),
            "thumbnail": metadata.get("thumbnail").cloned().unwrap_or(Value::Null),
            "createdAt": normalize_date(metadata.get("createdAt")),
            "updatedAt": normalize_date(metadata.get("updatedAt")),
        }),
        _ => json!({
            "id": project_id,
            "name": project.get("name").and_then(Value::as_str).unwrap_or(""),
            "thumbnail": project.get("thumbnail").cloned().unwrap_or(Value::Null),
            "createdAt": normalize_date(project.get("createdAt")),
            "updatedAt": normalize_date(project.get("updatedAt")),
        }),
    };

    let settings = match project.get("settings") {
        Some(settings) if settings.is_object() => json!({
            "fps": number_field(settings, "fps").unwrap_or(DEFAULT_FPS),
            "canvasSize": canvas_size_value(settings.get("canvasSize")),
            "background": background_value(settings.get("background"), None, None, None),
            "originalCanvasSize": Value::Null,
        }),
        _ => json!({
            "fps": number_field(&project, "fps").unwrap_or(DEFAULT_FPS),
            "canvasSize": canvas_size_value(project.get("canvasSize")),
            "background": background_value(
                project.get("background"),
                project.get("backgroundType"),
                project.get("backgroundColor"),
                project.get("blurIntensity"),
            ),
            "originalCanvasSize": Value::Null,
        }),
    };

    let legacy_bookmarks = project
        .get("bookmarks")
        .and_then(Value::as_array)
        .filter(|bookmarks| !bookmarks.is_empty())
        .cloned();

    let main_scene_id = find_main_scene_id(&project);
    if let Some(bookmarks) = legacy_bookmarks {
        map_scenes(&mut project, |scene| {
            let Some(scene_object) = scene.as_object_mut() else {
                return;
            };
            if let Some(main_id) = &main_scene_id {
                if scene_object.get("id").and_then(Value::as_str) != Some(main_id.as_str()) {
                    return;
                }
            }
            let has_bookmarks = scene_object
                .get("bookmarks")
                .and_then(Value::as_array)
                .is_some_and(|existing| !existing.is_empty());
            if has_bookmarks {
                return;
            }
            scene_object.insert("bookmarks".to_string(), Value::Array(bookmarks.clone()));
        });
    }

    let current_scene_id = project
        .get("currentSceneId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(ToString::to_string)
        .or_else(|| find_main_scene_id(&project))
        .unwrap_or_default();

    if let Some(object) = project.as_object_mut() {
        object.insert("metadata".to_string(), metadata);
        object.insert("settings".to_string(), settings);
        object.insert("currentSceneId".to_string(), Value::from(current_scene_id));
        object.insert("version".to_string(), Value::from(2));
    }

    MigrationResult::migrated(project)
}

fn find_main_scene_id(project: &Value) -> Option<String> {
    let scenes = project.get("scenes")?.as_array()?;
    for scene in scenes {
        if scene.get("isMain") == Some(&Value::Bool(true)) {
            if let Some(id) = scene.get("id").and_then(Value::as_str) {
                return Some(id.to_string());
            }
        }
    }
    for scene in scenes {
        if let Some(id) = scene.get("id").and_then(Value::as_str) {
            return Some(id.to_string());
        }
    }
    None
}

fn canvas_size_value(value: Option<&Value>) -> Value {
    let width = value
        .and_then(|size| number_field(size, "width"))
        .unwrap_or(DEFAULT_CANVAS_WIDTH);
    let height = value
        .and_then(|size| number_field(size, "height"))
        .unwrap_or(DEFAULT_CANVAS_HEIGHT);
    let number = |value: f64| {
        if value.fract() == 0.0 {
            Value::from(value as i64)
        } else {
            Value::from(value)
        }
    };
    json!({ "width": number(width), "height": number(height) })
}

fn background_value(
    value: Option<&Value>,
    background_type: Option<&Value>,
    background_color: Option<&Value>,
    blur_intensity: Option<&Value>,
) -> Value {
    if let Some(background) = value.filter(|background| background.is_object()) {
        if background.get("type").and_then(Value::as_str) == Some("blur") {
            return json!({
                "type": "blur",
                "blurIntensity": number_field(background, "blurIntensity")
                    .unwrap_or(DEFAULT_BACKGROUND_BLUR_INTENSITY),
            });
        }
        return json!({
            "type": "color",
            "color": string_field(background, "color")
                .unwrap_or_else(|| DEFAULT_BACKGROUND_COLOR.to_string()),
        });
    }

    if background_type.and_then(Value::as_str) == Some("blur") {
        return json!({
            "type": "blur",
            "blurIntensity": blur_intensity
                .and_then(Value::as_f64)
                .unwrap_or(DEFAULT_BACKGROUND_BLUR_INTENSITY),
        });
    }

    json!({
        "type": "color",
        "color": background_color.and_then(Value::as_str).unwrap_or(DEFAULT_BACKGROUND_COLOR),
    })
}

pub fn v2_to_v3(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }

    let already = version_of(&project).is_some_and(|version| version >= 3)
        || project
            .get("metadata")
            .and_then(|metadata| metadata.get("duration"))
            .is_some_and(Value::is_number);
    if already {
        return MigrationResult::skipped(project, "already v3");
    }

    let duration = duration_from_scenes(&project);

    if let Some(object) = project.as_object_mut() {
        match object.get_mut("metadata").and_then(Value::as_object_mut) {
            Some(metadata) => {
                metadata.insert("duration".to_string(), Value::from(duration));
            }
            None => {
                object.insert("metadata".to_string(), json!({ "duration": duration }));
            }
        }
        object.insert("version".to_string(), Value::from(3));
    }

    MigrationResult::migrated(project)
}

fn duration_from_scenes(project: &Value) -> f64 {
    let Some(scenes) = project.get("scenes").and_then(Value::as_array) else {
        return 0.0;
    };

    let main_scene = scenes
        .iter()
        .find(|scene| scene.get("isMain") == Some(&Value::Bool(true)))
        .or_else(|| scenes.iter().find(|scene| scene.is_object()));

    let Some(tracks) = main_scene
        .and_then(|scene| scene.get("tracks"))
        .and_then(Value::as_array)
    else {
        return 0.0;
    };

    let mut max_end: f64 = 0.0;
    for track in tracks {
        let Some(elements) = track.get("elements").and_then(Value::as_array) else {
            continue;
        };
        for element in elements {
            let start = number_field(element, "startTime").unwrap_or(0.0);
            let duration = number_field(element, "duration").unwrap_or(0.0);
            max_end = max_end.max(start + duration);
        }
    }

    max_end
}

pub fn v3_to_v4(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 4) {
        return MigrationResult::skipped(project, "already v4");
    }

    map_flat_tracks(&mut project, |track| {
        if track.get("type").and_then(Value::as_str) != Some("text") {
            return;
        }
        let Some(elements) = track.get_mut("elements").and_then(Value::as_array_mut) else {
            return;
        };
        for element in elements.iter_mut() {
            if element.get("type").and_then(Value::as_str) != Some("text") {
                continue;
            }
            let normalized = normalize_font_weight(element.get("fontWeight"));
            if let (Some(weight), Some(object)) = (normalized, element.as_object_mut()) {
                object.insert("fontWeight".to_string(), Value::from(weight));
            }
        }
    });

    set_version(&mut project, 4);
    MigrationResult::migrated(project)
}

fn normalize_font_weight(value: Option<&Value>) -> Option<String> {
    const VALID: [&str; 9] = [
        "100", "200", "300", "400", "500", "600", "700", "800", "900",
    ];

    match value {
        Some(Value::Number(number)) => {
            let text = number
                .as_f64()
                .filter(|value| value.fract() == 0.0)
                .map(|value| format!("{}", value as i64))
                .unwrap_or_default();
            VALID.contains(&text.as_str()).then_some(text)
        }
        Some(Value::String(text)) => {
            let normalized = text.trim().to_lowercase();
            match normalized.as_str() {
                "normal" => Some("400".to_string()),
                "bold" => Some("700".to_string()),
                other if VALID.contains(&other) => Some(other.to_string()),
                _ => None,
            }
        }
        _ => None,
    }
}

pub fn v4_to_v5(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 5) {
        return MigrationResult::skipped(project, "already v5");
    }

    const KNOWN_PROVIDERS: [&str; 4] = ["icons", "emoji", "flags", "shapes"];

    map_flat_elements(&mut project, |element| {
        if element.get("type").and_then(Value::as_str) != Some("sticker") {
            return;
        }
        let existing = string_field(element, "stickerId");
        let legacy = string_field(element, "iconName");
        let source = existing.clone().or(legacy);
        let normalized = source.and_then(|value| {
            if value.is_empty() {
                return None;
            }
            match value.find(':') {
                None => Some(format!("icons:{value}")),
                Some(index) => {
                    if KNOWN_PROVIDERS.contains(&&value[..index]) {
                        Some(value)
                    } else {
                        Some(format!("icons:{value}"))
                    }
                }
            }
        });

        let Some(object) = element.as_object_mut() else {
            return;
        };
        object.remove("iconName");
        object.remove("color");
        match normalized {
            Some(sticker_id) => {
                object.insert("stickerId".to_string(), Value::from(sticker_id));
            }
            None => {
                object.remove("stickerId");
            }
        }
    });

    set_version(&mut project, 5);
    MigrationResult::migrated(project)
}

pub fn v5_to_v6(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 6) {
        return MigrationResult::skipped(project, "already v6");
    }

    map_scenes(&mut project, |scene| {
        let Some(bookmarks) = scene.get_mut("bookmarks").and_then(Value::as_array_mut) else {
            return;
        };
        for bookmark in bookmarks.iter_mut() {
            if bookmark.is_number() {
                *bookmark = json!({ "time": bookmark.clone() });
            }
        }
    });

    set_version(&mut project, 6);
    MigrationResult::migrated(project)
}

pub fn v6_to_v7(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 7) {
        return MigrationResult::skipped(project, "already v7");
    }

    map_flat_elements(&mut project, |element| {
        if element.get("type").and_then(Value::as_str) != Some("text") {
            return;
        }
        if element.get("background").is_some_and(Value::is_object) {
            return;
        }
        let background_color =
            string_field(element, "backgroundColor").unwrap_or_else(|| "transparent".to_string());
        let Some(object) = element.as_object_mut() else {
            return;
        };
        object.remove("backgroundColor");
        object.insert(
            "background".to_string(),
            json!({
                "color": background_color,
                "cornerRadius": 0,
                "paddingX": 8,
                "paddingY": 4,
                "offsetX": 0,
                "offsetY": 0,
            }),
        );
    });

    set_version(&mut project, 7);
    MigrationResult::migrated(project)
}

pub fn v7_to_v8(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 8) {
        return MigrationResult::skipped(project, "already v8");
    }

    map_flat_elements(&mut project, |element| {
        let element_type = element.get("type").and_then(Value::as_str);
        if element_type != Some("video") && element_type != Some("audio") {
            return;
        }
        if element.get("sourceDuration").is_some_and(Value::is_number) {
            return;
        }
        let source_duration = number_field(element, "trimStart").unwrap_or(0.0)
            + number_field(element, "duration").unwrap_or(0.0)
            + number_field(element, "trimEnd").unwrap_or(0.0);
        if let Some(object) = element.as_object_mut() {
            object.insert("sourceDuration".to_string(), Value::from(source_duration));
        }
    });

    set_version(&mut project, 8);
    MigrationResult::migrated(project)
}

pub fn v8_to_v9(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 9) {
        return MigrationResult::skipped(project, "already v9");
    }

    map_flat_tracks(&mut project, |track| {
        if track.get("type").and_then(Value::as_str) != Some("text") {
            return;
        }
        let Some(elements) = track.get_mut("elements").and_then(Value::as_array_mut) else {
            return;
        };
        for element in elements.iter_mut() {
            if element.get("type").and_then(Value::as_str) != Some("text") {
                continue;
            }
            let Some(background) = element.get("background").filter(|value| value.is_object())
            else {
                continue;
            };
            if background.get("enabled").is_some_and(Value::is_boolean) {
                continue;
            }
            let color = background
                .get("color")
                .and_then(Value::as_str)
                .unwrap_or("transparent")
                .to_string();
            let enabled = color != "transparent";
            if let Some(background) = element.get_mut("background").and_then(Value::as_object_mut) {
                background.insert("enabled".to_string(), Value::from(enabled));
            }
        }
    });

    set_version(&mut project, 9);
    MigrationResult::migrated(project)
}

pub fn v9_to_v10(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 10) {
        return MigrationResult::skipped(project, "already v10");
    }

    set_version(&mut project, 10);
    MigrationResult::migrated(project)
}

pub fn v10_to_v11(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 11) {
        return MigrationResult::skipped(project, "already v11");
    }

    map_flat_elements(&mut project, |element| {
        let scale = element
            .get("transform")
            .filter(|transform| transform.is_object())
            .and_then(|transform| transform.get("scale"))
            .and_then(Value::as_f64);
        let Some(scale) = scale else {
            return;
        };

        if let Some(transform) = element.get_mut("transform").and_then(Value::as_object_mut) {
            transform.remove("scale");
            transform.insert("scaleX".to_string(), Value::from(scale));
            transform.insert("scaleY".to_string(), Value::from(scale));
        }

        let scale_channel = element
            .get("animations")
            .and_then(|animations| animations.get("channels"))
            .filter(|channels| channels.is_object())
            .and_then(|channels| channels.get("transform.scale"))
            .filter(|channel| channel.is_object())
            .filter(|channel| channel.get("keyframes").is_some_and(Value::is_array))
            .cloned();
        let Some(scale_channel) = scale_channel else {
            return;
        };

        if let Some(channels) = element
            .get_mut("animations")
            .and_then(|animations| animations.get_mut("channels"))
            .and_then(Value::as_object_mut)
        {
            channels.remove("transform.scale");
            channels.insert("transform.scaleX".to_string(), scale_channel.clone());
            channels.insert("transform.scaleY".to_string(), scale_channel);
        }
    });

    set_version(&mut project, 11);
    MigrationResult::migrated(project)
}

pub fn v11_to_v12(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 12) {
        return MigrationResult::skipped(project, "already v12");
    }

    map_flat_elements(&mut project, migrate_position_channels);

    set_version(&mut project, 12);
    MigrationResult::migrated(project)
}

#[derive(Clone)]
struct ScalarKeyframe {
    id: String,
    time: f64,
    value: f64,
    interpolation: String,
}

fn read_scalar_keyframes(channel: Option<&Value>) -> Vec<ScalarKeyframe> {
    let Some(keyframes) = channel
        .filter(|channel| channel.is_object())
        .and_then(|channel| channel.get("keyframes"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };

    keyframes
        .iter()
        .filter_map(|keyframe| {
            Some(ScalarKeyframe {
                id: keyframe.get("id")?.as_str()?.to_string(),
                time: keyframe.get("time")?.as_f64()?,
                value: keyframe.get("value")?.as_f64()?,
                interpolation: keyframe
                    .get("interpolation")
                    .and_then(Value::as_str)
                    .unwrap_or("linear")
                    .to_string(),
            })
        })
        .collect()
}

fn interpolate_scalar_at(keyframes: &[ScalarKeyframe], time: f64, fallback: f64) -> f64 {
    if keyframes.is_empty() {
        return fallback;
    }

    let mut sorted = keyframes.to_vec();
    sorted.sort_by(|left, right| left.time.total_cmp(&right.time));
    let first = &sorted[0];
    let last = &sorted[sorted.len() - 1];

    if time <= first.time {
        return first.value;
    }
    if time >= last.time {
        return last.value;
    }

    for window in sorted.windows(2) {
        let (left, right) = (&window[0], &window[1]);
        if time < left.time || time > right.time {
            continue;
        }
        if left.interpolation == "hold" {
            return left.value;
        }
        let ratio = (time - left.time) / (right.time - left.time);
        return left.value + (right.value - left.value) * ratio;
    }

    last.value
}

fn migrate_position_channels(element: &mut Value) {
    let channels = element
        .get("animations")
        .filter(|animations| animations.is_object())
        .and_then(|animations| animations.get("channels"))
        .filter(|channels| channels.is_object())
        .cloned();
    let Some(channels) = channels else {
        return;
    };

    let x_channel = channels.get("transform.position.x").cloned();
    let y_channel = channels.get("transform.position.y").cloned();
    if x_channel.is_none() && y_channel.is_none() {
        return;
    }

    let base_position = element
        .get("transform")
        .and_then(|transform| transform.get("position"));
    let base_x = base_position
        .and_then(|position| position.get("x"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let base_y = base_position
        .and_then(|position| position.get("y"))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);

    let x_keyframes = read_scalar_keyframes(x_channel.as_ref());
    let y_keyframes = read_scalar_keyframes(y_channel.as_ref());

    let mut times: Vec<f64> = Vec::new();
    for keyframe in x_keyframes.iter().chain(y_keyframes.iter()) {
        if !times.contains(&keyframe.time) {
            times.push(keyframe.time);
        }
    }
    times.sort_by(f64::total_cmp);

    let vector_keyframes: Vec<Value> = times
        .iter()
        .map(|time| {
            let x_key = x_keyframes
                .iter()
                .find(|keyframe| (keyframe.time - time).abs() < 0.001);
            let y_key = y_keyframes
                .iter()
                .find(|keyframe| (keyframe.time - time).abs() < 0.001);
            let x = x_key
                .map(|keyframe| keyframe.value)
                .unwrap_or_else(|| interpolate_scalar_at(&x_keyframes, *time, base_x));
            let y = y_key
                .map(|keyframe| keyframe.value)
                .unwrap_or_else(|| interpolate_scalar_at(&y_keyframes, *time, base_y));
            let interpolation = x_key
                .or(y_key)
                .map(|keyframe| keyframe.interpolation.clone())
                .unwrap_or_else(|| "linear".to_string());
            let id = x_key
                .or(y_key)
                .map(|keyframe| keyframe.id.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            json!({
                "id": id,
                "time": time,
                "value": { "x": x, "y": y },
                "interpolation": interpolation,
            })
        })
        .collect();

    if let Some(channels) = element
        .get_mut("animations")
        .and_then(|animations| animations.get_mut("channels"))
        .and_then(Value::as_object_mut)
    {
        channels.remove("transform.position.x");
        channels.remove("transform.position.y");
        channels.insert(
            "transform.position".to_string(),
            json!({ "valueKind": "vector", "keyframes": vector_keyframes }),
        );
    }
}

pub fn v12_to_v13(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 13) {
        return MigrationResult::skipped(project, "already v13");
    }

    map_flat_elements(&mut project, |element| {
        let Some(masks) = element.get_mut("masks").and_then(Value::as_array_mut) else {
            return;
        };
        for mask in masks.iter_mut() {
            let feather = mask.get("feather").and_then(Value::as_f64).unwrap_or(0.0);
            let inverted = mask
                .get("inverted")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let stroke_color = mask
                .get("stroke")
                .and_then(|stroke| stroke.get("color"))
                .and_then(Value::as_str)
                .unwrap_or("#ffffff")
                .to_string();
            let stroke_width = mask
                .get("stroke")
                .and_then(|stroke| stroke.get("width"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);

            let mut params = mask
                .get("params")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            params.insert("feather".to_string(), Value::from(feather));
            params.insert("inverted".to_string(), Value::from(inverted));
            params.insert("strokeColor".to_string(), Value::from(stroke_color));
            params.insert("strokeWidth".to_string(), Value::from(stroke_width));

            let Some(mask_object) = mask.as_object_mut() else {
                continue;
            };
            mask_object.insert("params".to_string(), Value::Object(params));
            mask_object.remove("feather");
            mask_object.remove("inverted");
            mask_object.remove("stroke");
        }
    });

    set_version(&mut project, 13);
    MigrationResult::migrated(project)
}

pub fn v13_to_v14(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 14) {
        return MigrationResult::skipped(project, "already v14");
    }

    map_flat_elements(&mut project, |element| {
        let Some(masks) = element.get_mut("masks").and_then(Value::as_array_mut) else {
            return;
        };
        for mask in masks.iter_mut() {
            if mask.get("type").and_then(Value::as_str) != Some("split") {
                continue;
            }
            let params = mask.get("params").filter(|params| params.is_object());
            let position = params
                .and_then(|params| params.get("position"))
                .and_then(Value::as_f64)
                .unwrap_or(0.5);
            let rotation = params
                .and_then(|params| params.get("rotation"))
                .and_then(Value::as_f64)
                .unwrap_or(0.0);
            let angle = rotation.to_radians();
            let x = (position - 0.5) * angle.cos();
            let y = (position - 0.5) * angle.sin();

            let mut next_params = params
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            next_params.remove("position");
            next_params.insert("x".to_string(), Value::from(x));
            next_params.insert("y".to_string(), Value::from(y));
            if let Some(mask_object) = mask.as_object_mut() {
                mask_object.insert("params".to_string(), Value::Object(next_params));
            }
        }
    });

    set_version(&mut project, 14);
    MigrationResult::migrated(project)
}

pub fn v14_to_v15(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 15) {
        return MigrationResult::skipped(project, "already v15");
    }

    map_flat_elements(&mut project, |element| {
        if element.get("type").and_then(Value::as_str) != Some("sticker") {
            return;
        }
        let has_dimensions = element.get("intrinsicWidth").is_some_and(Value::is_number)
            && element.get("intrinsicHeight").is_some_and(Value::is_number);
        if has_dimensions {
            return;
        }
        if let Some(object) = element.as_object_mut() {
            object.insert(
                "intrinsicWidth".to_string(),
                Value::from(STICKER_INTRINSIC_SIZE_FALLBACK),
            );
            object.insert(
                "intrinsicHeight".to_string(),
                Value::from(STICKER_INTRINSIC_SIZE_FALLBACK),
            );
        }
    });

    set_version(&mut project, 15);
    MigrationResult::migrated(project)
}

pub fn v15_to_v16(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 16) {
        return MigrationResult::skipped(project, "already v16");
    }

    map_flat_tracks(&mut project, |track| {
        if track.get("type").and_then(Value::as_str) != Some("sticker") {
            return;
        }
        if let Some(object) = track.as_object_mut() {
            object.insert("type".to_string(), Value::from("graphic"));
        }
    });

    set_version(&mut project, 16);
    MigrationResult::migrated(project)
}

pub fn v16_to_v17(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 17) {
        return MigrationResult::skipped(project, "already v17");
    }

    map_flat_elements(&mut project, |element| {
        let Some(masks) = element.get_mut("masks").and_then(Value::as_array_mut) else {
            return;
        };
        for mask in masks.iter_mut() {
            let Some(params) = mask.get_mut("params").and_then(Value::as_object_mut) else {
                continue;
            };
            if params.get("strokeAlign").is_some_and(Value::is_string) {
                continue;
            }
            params.insert("strokeAlign".to_string(), Value::from("center"));
        }
    });

    set_version(&mut project, 17);
    MigrationResult::migrated(project)
}

pub fn v17_to_v18(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 18) {
        return MigrationResult::skipped(project, "already v18");
    }

    map_flat_elements(&mut project, |element| {
        let element_type = element.get("type").and_then(Value::as_str);
        if element_type != Some("audio") && element_type != Some("video") {
            return;
        }
        let volume = element
            .get("volume")
            .and_then(Value::as_f64)
            .map(linear_gain_to_db)
            .unwrap_or(0.0);
        if let Some(object) = element.as_object_mut() {
            object.insert("volume".to_string(), Value::from(volume));
        }
    });

    set_version(&mut project, 18);
    MigrationResult::migrated(project)
}

fn linear_gain_to_db(gain: f64) -> f64 {
    if !gain.is_finite() {
        return 0.0;
    }
    if gain <= 0.0 {
        return VOLUME_DB_MIN;
    }
    let db = 20.0 * gain.log10();
    if !db.is_finite() {
        return 0.0;
    }
    db.clamp(VOLUME_DB_MIN, VOLUME_DB_MAX)
}

pub fn v18_to_v19(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 19) {
        return MigrationResult::skipped(project, "already v19");
    }

    if let Some(settings) = project.get_mut("settings").and_then(Value::as_object_mut) {
        settings.insert("canvasSizeMode".to_string(), Value::from("preset"));
        settings.insert("lastCustomCanvasSize".to_string(), Value::Null);
    }

    set_version(&mut project, 19);
    MigrationResult::migrated(project)
}

pub fn v19_to_v20(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    if version_of(&project).is_some_and(|version| version >= 20) {
        return MigrationResult::skipped(project, "already v20");
    }

    map_flat_elements(&mut project, |element| {
        if element.get("type").and_then(Value::as_str) != Some("video") {
            return;
        }
        let enabled = element
            .get("isSourceAudioEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        if let Some(object) = element.as_object_mut() {
            object.insert("isSourceAudioEnabled".to_string(), Value::from(enabled));
        }
    });

    set_version(&mut project, 20);
    MigrationResult::migrated(project)
}

pub fn v20_to_v21(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    let Some(version) = version_of(&project) else {
        return MigrationResult::skipped(project, "invalid version");
    };
    if version >= 21 {
        return MigrationResult::skipped(project, "already v21");
    }
    if version != 20 {
        return MigrationResult::skipped(project, "not v20");
    }

    let is_blur = project
        .get("settings")
        .filter(|settings| settings.is_object())
        .and_then(|settings| settings.get("background"))
        .filter(|background| background.is_object())
        .is_some_and(|background| background.get("type") == Some(&Value::from("blur")));

    if is_blur {
        let raw = project
            .get("settings")
            .and_then(|settings| settings.get("background"))
            .and_then(|background| background.get("blurIntensity"))
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite());
        let blur_intensity = raw
            .map(|value| value * INTENSITY_TO_SIGMA_DIVISOR)
            .unwrap_or(LEGACY_DEFAULT_BACKGROUND_BLUR_INTENSITY);
        if let Some(background) = project
            .get_mut("settings")
            .and_then(|settings| settings.get_mut("background"))
            .and_then(Value::as_object_mut)
        {
            background.insert("blurIntensity".to_string(), Value::from(blur_intensity));
        }
    }

    set_version(&mut project, 21);
    MigrationResult::migrated(project)
}

pub fn v21_to_v22(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    let Some(version) = version_of(&project) else {
        return MigrationResult::skipped(project, "invalid version");
    };
    if version >= 22 {
        return MigrationResult::skipped(project, "already v22");
    }
    if version != 21 {
        return MigrationResult::skipped(project, "not v21");
    }

    map_flat_elements(&mut project, migrate_legacy_animations);

    set_version(&mut project, 22);
    MigrationResult::migrated(project)
}

fn migrate_legacy_animations(element: &mut Value) {
    let Some(animations) = element.get("animations").filter(|value| value.is_object()) else {
        return;
    };
    if animations.get("bindings").is_some_and(Value::is_object) {
        return;
    }

    let migrated = animations
        .get("channels")
        .filter(|channels| channels.is_object())
        .and_then(build_v22_animations);

    let Some(object) = element.as_object_mut() else {
        return;
    };
    match migrated {
        Some(animations) => {
            object.insert("animations".to_string(), animations);
        }
        None => {
            object.remove("animations");
        }
    }
}

fn build_v22_animations(legacy_channels: &Value) -> Option<Value> {
    let mut bindings = Map::new();
    let mut channels = Map::new();

    for (property_path, channel) in legacy_channels.as_object()? {
        let Some((binding, new_channels)) = migrate_legacy_channel(property_path, channel) else {
            continue;
        };
        bindings.insert(property_path.clone(), binding);
        for (channel_id, channel_value) in new_channels {
            channels.insert(channel_id, channel_value);
        }
    }

    if bindings.is_empty() {
        return None;
    }

    Some(json!({ "bindings": bindings, "channels": channels }))
}

fn channel_id(property_path: &str, component_key: &str) -> String {
    format!("{property_path}:{component_key}")
}

fn scalar_key(id: &str, time: f64, value: f64, interpolation: &str) -> Value {
    json!({
        "id": id,
        "time": time,
        "value": value,
        "segmentToNext": if interpolation == "hold" { "step" } else { "linear" },
        "tangentMode": "flat",
    })
}

struct LegacyKey {
    id: String,
    time: f64,
    value: Value,
    interpolation: String,
}

fn legacy_keys(channel: &Value) -> Vec<LegacyKey> {
    let Some(keyframes) = channel.get("keyframes").and_then(Value::as_array) else {
        return Vec::new();
    };
    keyframes
        .iter()
        .filter_map(|keyframe| {
            let id = keyframe.get("id")?.as_str()?.to_string();
            let time = keyframe
                .get("time")?
                .as_f64()
                .filter(|time| time.is_finite())?;
            Some(LegacyKey {
                id,
                time,
                value: keyframe.get("value").cloned().unwrap_or(Value::Null),
                interpolation: if keyframe.get("interpolation") == Some(&Value::from("hold")) {
                    "hold".to_string()
                } else {
                    "linear".to_string()
                },
            })
        })
        .collect()
}

fn migrate_legacy_channel(
    property_path: &str,
    channel: &Value,
) -> Option<(Value, Vec<(String, Value)>)> {
    if !channel.is_object() {
        return None;
    }

    match channel.get("valueKind").and_then(Value::as_str)? {
        "number" => {
            let keys: Vec<Value> = legacy_keys(channel)
                .into_iter()
                .filter_map(|key| {
                    let value = key.value.as_f64().filter(|value| value.is_finite())?;
                    Some(scalar_key(&key.id, key.time, value, &key.interpolation))
                })
                .collect();
            if keys.is_empty() {
                return None;
            }
            let id = channel_id(property_path, "value");
            Some((
                json!({
                    "path": property_path,
                    "kind": "number",
                    "components": [{ "key": "value", "channelId": id }],
                }),
                vec![(id, json!({ "kind": "scalar", "keys": keys }))],
            ))
        }
        "discrete" => {
            let keys: Vec<Value> = legacy_keys(channel)
                .into_iter()
                .filter(|key| key.value.is_string() || key.value.is_boolean())
                .map(|key| json!({ "id": key.id, "time": key.time, "value": key.value }))
                .collect();
            if keys.is_empty() {
                return None;
            }
            let id = channel_id(property_path, "value");
            Some((
                json!({
                    "path": property_path,
                    "kind": "discrete",
                    "components": [{ "key": "value", "channelId": id }],
                }),
                vec![(id, json!({ "kind": "discrete", "keys": keys }))],
            ))
        }
        "vector" => {
            let keys: Vec<LegacyKey> = legacy_keys(channel)
                .into_iter()
                .filter(|key| {
                    key.value
                        .get("x")
                        .and_then(Value::as_f64)
                        .is_some_and(f64::is_finite)
                        && key
                            .value
                            .get("y")
                            .and_then(Value::as_f64)
                            .is_some_and(f64::is_finite)
                })
                .collect();
            if keys.is_empty() {
                return None;
            }
            let x_id = channel_id(property_path, "x");
            let y_id = channel_id(property_path, "y");
            let component_keys = |component: &str| -> Vec<Value> {
                keys.iter()
                    .map(|key| {
                        scalar_key(
                            &key.id,
                            key.time,
                            key.value
                                .get(component)
                                .and_then(Value::as_f64)
                                .unwrap_or(0.0),
                            &key.interpolation,
                        )
                    })
                    .collect()
            };
            Some((
                json!({
                    "path": property_path,
                    "kind": "vector2",
                    "components": [
                        { "key": "x", "channelId": x_id },
                        { "key": "y", "channelId": y_id },
                    ],
                }),
                vec![
                    (
                        x_id.clone(),
                        json!({ "kind": "scalar", "keys": component_keys("x") }),
                    ),
                    (
                        y_id.clone(),
                        json!({ "kind": "scalar", "keys": component_keys("y") }),
                    ),
                ],
            ))
        }
        "color" => {
            let keys: Vec<(LegacyKey, [f64; 4])> = legacy_keys(channel)
                .into_iter()
                .filter_map(|key| {
                    let text = key.value.as_str()?;
                    let rgba = color::parse_to_linear_rgba(text)?;
                    Some((key, rgba))
                })
                .collect();
            if keys.is_empty() {
                return None;
            }

            let components = ["r", "g", "b", "a"];
            let mut channels = Vec::new();
            let mut binding_components = Vec::new();
            for (index, component) in components.iter().enumerate() {
                let id = channel_id(property_path, component);
                let component_keys: Vec<Value> = keys
                    .iter()
                    .map(|(key, rgba)| {
                        scalar_key(&key.id, key.time, rgba[index], &key.interpolation)
                    })
                    .collect();
                binding_components.push(json!({ "key": component, "channelId": id }));
                channels.push((id, json!({ "kind": "scalar", "keys": component_keys })));
            }

            Some((
                json!({
                    "path": property_path,
                    "kind": "color",
                    "colorSpace": "srgb-linear",
                    "components": binding_components,
                }),
                channels,
            ))
        }
        _ => None,
    }
}

pub fn v22_to_v23(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    let Some(version) = version_of(&project) else {
        return MigrationResult::skipped(project, "invalid version");
    };
    if version >= 23 {
        return MigrationResult::skipped(project, "already v23");
    }
    if version != 22 {
        return MigrationResult::skipped(project, "not v22");
    }

    if let Some(metadata) = project
        .get_mut("metadata")
        .filter(|value| value.is_object())
    {
        seconds_to_ticks_fields(metadata, &["duration"]);
    }

    if let Some(settings) = project
        .get_mut("settings")
        .filter(|value| value.is_object())
    {
        let fps = settings.get("fps").cloned();
        if let Some(fps) = fps {
            let migrated = migrate_frame_rate(&fps);
            if let Some(object) = settings.as_object_mut() {
                object.insert("fps".to_string(), migrated);
            }
        }
    }

    if let Some(view_state) = project
        .get_mut("timelineViewState")
        .filter(|value| value.is_object())
    {
        seconds_to_ticks_fields(view_state, &["playheadTime"]);
    }

    map_scenes(&mut project, |scene| {
        if let Some(bookmarks) = scene.get_mut("bookmarks").and_then(Value::as_array_mut) {
            for bookmark in bookmarks.iter_mut() {
                if bookmark.is_object() {
                    seconds_to_ticks_fields(bookmark, &["time", "duration"]);
                }
            }
        }
    });

    map_flat_elements(&mut project, |element| {
        seconds_to_ticks_fields(
            element,
            &[
                "duration",
                "startTime",
                "trimStart",
                "trimEnd",
                "sourceDuration",
            ],
        );

        let Some(channels) = element
            .get_mut("animations")
            .filter(|animations| animations.is_object())
            .and_then(|animations| animations.get_mut("channels"))
            .and_then(Value::as_object_mut)
        else {
            return;
        };
        for (_, channel) in channels.iter_mut() {
            let Some(keys) = channel.get_mut("keys").and_then(Value::as_array_mut) else {
                continue;
            };
            for keyframe in keys.iter_mut() {
                if !keyframe.is_object() {
                    continue;
                }
                seconds_to_ticks_fields(keyframe, &["time"]);
                for handle in ["leftHandle", "rightHandle"] {
                    if let Some(handle) = keyframe.get_mut(handle).filter(|value| value.is_object())
                    {
                        seconds_to_ticks_fields(handle, &["dt"]);
                    }
                }
            }
        }
    });

    set_version(&mut project, 23);
    MigrationResult::migrated(project)
}

fn seconds_to_ticks_fields(record: &mut Value, keys: &[&str]) {
    let Some(object) = record.as_object_mut() else {
        return;
    };
    for key in keys {
        let Some(value) = object.get(*key) else {
            continue;
        };
        let Some(seconds) = value.as_f64().filter(|value| value.is_finite()) else {
            continue;
        };
        object.insert(
            (*key).to_string(),
            Value::from((seconds * TICKS_PER_SECOND).round() as i64),
        );
    }
}

fn migrate_frame_rate(fps: &Value) -> Value {
    if fps.is_object() {
        return fps.clone();
    }
    let Some(value) = fps
        .as_f64()
        .filter(|value| value.is_finite() && *value > 0.0)
    else {
        return fps.clone();
    };

    const STANDARD: [(f64, i64, i64); 10] = [
        (24_000.0 / 1_001.0, 24_000, 1_001),
        (24.0, 24, 1),
        (25.0, 25, 1),
        (30_000.0 / 1_001.0, 30_000, 1_001),
        (30.0, 30, 1),
        (48.0, 48, 1),
        (50.0, 50, 1),
        (60_000.0 / 1_001.0, 60_000, 1_001),
        (60.0, 60, 1),
        (120.0, 120, 1),
    ];

    for (candidate, numerator, denominator) in STANDARD {
        if (value - candidate).abs() <= 0.01 {
            return json!({ "numerator": numerator, "denominator": denominator });
        }
    }

    if value.fract() == 0.0 {
        return json!({ "numerator": value as i64, "denominator": 1 });
    }

    const ARBITRARY_DENOMINATOR: i64 = 1_000_000;
    let scaled = (value * ARBITRARY_DENOMINATOR as f64).round() as i64;
    let divisor = gcd(scaled, ARBITRARY_DENOMINATOR);
    json!({
        "numerator": scaled / divisor,
        "denominator": ARBITRARY_DENOMINATOR / divisor,
    })
}

fn gcd(left: i64, right: i64) -> i64 {
    let mut left = left.abs();
    let mut right = right.abs();
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    if left == 0 { 1 } else { left }
}

pub fn v23_to_v24(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    let Some(version) = version_of(&project) else {
        return MigrationResult::skipped(project, "invalid version");
    };
    if version >= 24 {
        return MigrationResult::skipped(project, "already v24");
    }
    if version != 23 {
        return MigrationResult::skipped(project, "not v23");
    }

    map_scenes(&mut project, |scene| {
        let Some(tracks) = scene.get("tracks").and_then(Value::as_array).cloned() else {
            return;
        };

        let main_index = tracks
            .iter()
            .position(|track| {
                track.get("type") == Some(&Value::from("video"))
                    && track.get("isMain") == Some(&Value::Bool(true))
            })
            .or_else(|| {
                tracks
                    .iter()
                    .position(|track| track.get("type") == Some(&Value::from("video")))
            });
        let strip = |track: &Value| -> Value {
            let mut next = track.clone();
            if let Some(object) = next.as_object_mut() {
                object.remove("isMain");
            }
            next
        };

        let overlay: Vec<Value> = tracks
            .iter()
            .enumerate()
            .filter(|(index, _)| Some(*index) != main_index)
            .map(|(_, track)| strip(track))
            .filter(|track| track.is_object() && track.get("type") != Some(&Value::from("audio")))
            .collect();
        let audio: Vec<Value> = tracks
            .iter()
            .map(strip)
            .filter(|track| track.is_object() && track.get("type") == Some(&Value::from("audio")))
            .collect();

        let main = match main_index {
            Some(index) => strip(&tracks[index]),
            None => json!({
                "id": uuid::Uuid::new_v4().to_string(),
                "name": "Main",
                "type": "video",
                "elements": [],
                "muted": false,
                "hidden": false,
            }),
        };

        if let Some(object) = scene.as_object_mut() {
            object.insert(
                "tracks".to_string(),
                json!({ "overlay": overlay, "main": main, "audio": audio }),
            );
        }
    });

    set_version(&mut project, 24);
    MigrationResult::migrated(project)
}

pub fn v24_to_v25(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    let Some(version) = version_of(&project) else {
        return MigrationResult::skipped(project, "invalid version");
    };
    if version >= 25 {
        return MigrationResult::skipped(project, "already v25");
    }
    if version != 24 {
        return MigrationResult::skipped(project, "not v24");
    }

    map_grouped_elements(&mut project, |element| {
        let animations = element.get("animations").filter(|value| value.is_object());
        let Some(animations) = animations else {
            return;
        };
        let (Some(bindings), Some(channels)) = (
            animations.get("bindings").filter(|value| value.is_object()),
            animations.get("channels").filter(|value| value.is_object()),
        ) else {
            return;
        };

        let position_binding = bindings.get("transform.position");
        let is_vector2 = position_binding
            .filter(|binding| binding.is_object())
            .is_some_and(|binding| binding.get("kind") == Some(&Value::from("vector2")));
        if !is_vector2 {
            return;
        }

        let x_channel = channels.get("transform.position:x").cloned();
        let y_channel = channels.get("transform.position:y").cloned();

        if let Some(bindings) = element
            .get_mut("animations")
            .and_then(|animations| animations.get_mut("bindings"))
            .and_then(Value::as_object_mut)
        {
            bindings.remove("transform.position");
            bindings.insert(
                "transform.positionX".to_string(),
                json!({
                    "path": "transform.positionX",
                    "kind": "number",
                    "components": [{ "key": "value", "channelId": "transform.positionX:value" }],
                }),
            );
            bindings.insert(
                "transform.positionY".to_string(),
                json!({
                    "path": "transform.positionY",
                    "kind": "number",
                    "components": [{ "key": "value", "channelId": "transform.positionY:value" }],
                }),
            );
        }

        if let Some(channels) = element
            .get_mut("animations")
            .and_then(|animations| animations.get_mut("channels"))
            .and_then(Value::as_object_mut)
        {
            channels.remove("transform.position:x");
            channels.remove("transform.position:y");
            if let Some(channel) = x_channel.filter(|value| value.is_object()) {
                channels.insert("transform.positionX:value".to_string(), channel);
            }
            if let Some(channel) = y_channel.filter(|value| value.is_object()) {
                channels.insert("transform.positionY:value".to_string(), channel);
            }
        }
    });

    set_version(&mut project, 25);
    MigrationResult::migrated(project)
}

pub fn v25_to_v26(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    let Some(version) = version_of(&project) else {
        return MigrationResult::skipped(project, "invalid version");
    };
    if version >= 26 {
        return MigrationResult::skipped(project, "already v26");
    }
    if version != 25 {
        return MigrationResult::skipped(project, "not v25");
    }

    const CROPPABLE: [&str; 4] = ["video", "image", "sticker", "graphic"];

    map_grouped_elements(&mut project, |element| {
        let Some(element_type) = element.get("type").and_then(Value::as_str) else {
            return;
        };
        if !CROPPABLE.contains(&element_type) {
            return;
        }

        let read_side = |side: &str| -> f64 {
            element
                .get("crop")
                .filter(|crop| crop.is_object())
                .and_then(|crop| crop.get(side))
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .map(|value| value.min(0.98))
                .unwrap_or(0.0)
        };
        let crop = json!({
            "left": read_side("left"),
            "top": read_side("top"),
            "right": read_side("right"),
            "bottom": read_side("bottom"),
        });

        if let Some(object) = element.as_object_mut() {
            object.insert("crop".to_string(), crop);
        }
    });

    set_version(&mut project, 26);
    MigrationResult::migrated(project)
}

pub fn v26_to_v27(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    let Some(version) = version_of(&project) else {
        return MigrationResult::skipped(project, "invalid version");
    };
    if version >= 27 {
        return MigrationResult::skipped(project, "already v27");
    }
    if version != 26 {
        return MigrationResult::skipped(project, "not v26");
    }

    const TRANSITIONABLE: [&str; 2] = ["video", "image"];
    const TRANSITION_TYPES: [&str; 11] = [
        "crossfade",
        "fadeToBlack",
        "slideLeft",
        "slideRight",
        "slideUp",
        "slideDown",
        "wipeLeft",
        "wipeRight",
        "wipeUp",
        "wipeDown",
        "zoom",
    ];
    const EASINGS: [&str; 4] = ["linear", "easeIn", "easeOut", "easeInOut"];

    map_grouped_elements(&mut project, |element| {
        let Some(element_type) = element
            .get("type")
            .and_then(Value::as_str)
            .map(ToString::to_string)
        else {
            return;
        };
        if !element
            .as_object()
            .is_some_and(|object| object.contains_key("transition"))
        {
            return;
        }

        let transition = element.get("transition").filter(|value| value.is_object());
        let normalized = transition.and_then(|transition| {
            let transition_type = transition.get("type").and_then(Value::as_str)?;
            if !TRANSITION_TYPES.contains(&transition_type) {
                return None;
            }
            let duration = transition
                .get("duration")
                .and_then(Value::as_f64)
                .filter(|value| value.is_finite() && *value > 0.0)?;
            let easing = transition
                .get("easing")
                .and_then(Value::as_str)
                .filter(|easing| EASINGS.contains(easing))
                .unwrap_or("easeInOut");
            Some(json!({
                "type": transition_type,
                "duration": duration.round() as i64,
                "easing": easing,
            }))
        });

        let Some(object) = element.as_object_mut() else {
            return;
        };
        match normalized.filter(|_| TRANSITIONABLE.contains(&element_type.as_str())) {
            Some(transition) => {
                object.insert("transition".to_string(), transition);
            }
            None => {
                object.remove("transition");
            }
        }
    });

    set_version(&mut project, 27);
    MigrationResult::migrated(project)
}

pub fn v27_to_v28(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    let Some(version) = version_of(&project) else {
        return MigrationResult::skipped(project, "invalid version");
    };
    if version >= 28 {
        return MigrationResult::skipped(project, "already v28");
    }
    if version != 27 {
        return MigrationResult::skipped(project, "not v27");
    }

    map_grouped_elements(&mut project, |element| {
        let has_cutout = element
            .as_object()
            .is_some_and(|object| object.contains_key("cutout"));
        if has_cutout {
            let normalized = normalize_cutout(element.get("cutout"));
            if let Some(object) = element.as_object_mut() {
                match normalized {
                    Some(cutout) => {
                        object.insert("cutout".to_string(), cutout);
                    }
                    None => {
                        object.remove("cutout");
                    }
                }
            }
        }

        let has_text_animations = element
            .as_object()
            .is_some_and(|object| object.contains_key("textAnimations"));
        if has_text_animations {
            let text_animations = element
                .get("textAnimations")
                .filter(|value| value.is_object())
                .filter(|value| {
                    ["in", "out", "reveal"]
                        .iter()
                        .any(|key| value.get(*key).is_some_and(Value::is_object))
                })
                .cloned();
            if let Some(object) = element.as_object_mut() {
                match text_animations {
                    Some(value) => {
                        object.insert("textAnimations".to_string(), value);
                    }
                    None => {
                        object.remove("textAnimations");
                    }
                }
            }
        }
    });

    set_version(&mut project, 28);
    MigrationResult::migrated(project)
}

fn normalize_cutout(cutout: Option<&Value>) -> Option<Value> {
    let cutout = cutout.filter(|value| value.is_object())?;
    let mode = cutout
        .get("mode")
        .and_then(Value::as_str)
        .filter(|mode| *mode == "static" || *mode == "perFrame")
        .unwrap_or("static");
    let frames: Option<Vec<Value>> = cutout
        .get("frames")
        .and_then(Value::as_array)
        .map(|frames| {
            frames
                .iter()
                .filter(|frame| {
                    frame
                        .as_object()
                        .is_some_and(|object| object.contains_key("png"))
                })
                .cloned()
                .collect()
        });

    let mut object = cutout.as_object().cloned().unwrap_or_default();
    match frames {
        Some(mut frames) if mode == "perFrame" && !frames.is_empty() => {
            frames.sort_by(|left, right| {
                let left_time = left
                    .get("sourceTime")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                let right_time = right
                    .get("sourceTime")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0);
                left_time.total_cmp(&right_time)
            });
            object.insert("mode".to_string(), Value::from("perFrame"));
            object.insert("frames".to_string(), Value::Array(frames));
        }
        _ => {
            object.remove("frames");
            object.remove("sampleInterval");
            object.insert("mode".to_string(), Value::from("static"));
        }
    }

    Some(Value::Object(object))
}

pub fn v28_to_v29(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    let Some(version) = version_of(&project) else {
        return MigrationResult::skipped(project, "invalid version");
    };
    if version >= 29 {
        return MigrationResult::skipped(project, "already v29");
    }
    if version != 28 {
        return MigrationResult::skipped(project, "not v28");
    }

    const ANCHORS: [&str; 5] = ["topLeft", "topRight", "bottomLeft", "bottomRight", "center"];

    let has_watermark = project
        .get("settings")
        .and_then(Value::as_object)
        .is_some_and(|settings| settings.contains_key("watermark"));

    if has_watermark {
        let watermark = project
            .get("settings")
            .and_then(|settings| settings.get("watermark"))
            .filter(|value| value.is_object())
            .map(|watermark| {
                let source = normalize_watermark_source(watermark.get("source"), false);
                let offset = watermark.get("offset").filter(|value| value.is_object());
                json!({
                    "enabled": watermark.get("enabled") == Some(&Value::Bool(true)) && source.is_some(),
                    "source": source.clone().unwrap_or(Value::Null),
                    "anchor": watermark
                        .get("anchor")
                        .and_then(Value::as_str)
                        .filter(|anchor| ANCHORS.contains(anchor))
                        .unwrap_or("bottomRight"),
                    "offset": {
                        "x": finite(offset.and_then(|offset| offset.get("x"))),
                        "y": finite(offset.and_then(|offset| offset.get("y"))),
                    },
                    "size": clamp_with_fallback(finite(watermark.get("size")), 0.01, 1.0, 0.18),
                    "opacity": clamp_with_fallback(finite(watermark.get("opacity")), 0.0, 1.0, 0.7),
                })
            });

        if let Some(settings) = project.get_mut("settings").and_then(Value::as_object_mut) {
            match watermark {
                Some(watermark) => {
                    settings.insert("watermark".to_string(), watermark);
                }
                None => {
                    settings.remove("watermark");
                }
            }
        }
    }

    set_version(&mut project, 29);
    MigrationResult::migrated(project)
}

fn normalize_watermark_source(source: Option<&Value>, upgrade_text: bool) -> Option<Value> {
    let source = source.filter(|value| value.is_object())?;
    let source_type = source.get("type").and_then(Value::as_str)?;

    if source_type == "image" {
        let media_id = source.get("mediaId").and_then(Value::as_str)?;
        return Some(json!({ "type": "image", "mediaId": media_id }));
    }

    if source_type == "text" {
        let text = source.get("text").and_then(Value::as_str)?;
        let color = source
            .get("color")
            .and_then(Value::as_str)
            .unwrap_or("#ffffff");
        if !upgrade_text {
            return Some(json!({ "type": "text", "text": text, "color": color }));
        }

        let stroke = source.get("stroke").filter(|value| value.is_object());
        let shadow = source.get("shadow").filter(|value| value.is_object());
        let font_weight = finite(source.get("fontWeight"));
        return Some(json!({
            "type": "text",
            "text": text,
            "color": color,
            "fontFamily": source
                .get("fontFamily")
                .and_then(Value::as_str)
                .filter(|family| !family.is_empty())
                .unwrap_or("Arial"),
            "fontWeight": if font_weight > 0.0 { font_weight } else { 600.0 },
            "stroke": {
                "enabled": stroke.and_then(|stroke| stroke.get("enabled")) == Some(&Value::Bool(true)),
                "color": stroke
                    .and_then(|stroke| stroke.get("color"))
                    .and_then(Value::as_str)
                    .unwrap_or("#000000"),
                "width": finite(stroke.and_then(|stroke| stroke.get("width"))).max(0.0),
            },
            "shadow": {
                "enabled": shadow.and_then(|shadow| shadow.get("enabled")) == Some(&Value::Bool(true)),
                "color": shadow
                    .and_then(|shadow| shadow.get("color"))
                    .and_then(Value::as_str)
                    .unwrap_or("#000000"),
                "blur": finite(shadow.and_then(|shadow| shadow.get("blur"))).max(0.0),
                "offsetX": finite(shadow.and_then(|shadow| shadow.get("offsetX"))),
                "offsetY": finite(shadow.and_then(|shadow| shadow.get("offsetY"))),
            },
        }));
    }

    None
}

pub fn v29_to_v30(mut project: Value) -> MigrationResult {
    if get_project_id(&project).is_none() {
        return MigrationResult::skipped(project, "no project id");
    }
    let Some(version) = version_of(&project) else {
        return MigrationResult::skipped(project, "invalid version");
    };
    if version >= 30 {
        return MigrationResult::skipped(project, "already v30");
    }
    if version != 29 {
        return MigrationResult::skipped(project, "not v29");
    }

    const ANCHORS: [&str; 9] = [
        "topLeft",
        "top",
        "topRight",
        "left",
        "center",
        "right",
        "bottomLeft",
        "bottom",
        "bottomRight",
    ];
    const BLEND_MODES: [&str; 8] = [
        "normal",
        "multiply",
        "screen",
        "overlay",
        "darken",
        "lighten",
        "difference",
        "luminosity",
    ];

    let has_watermark = project
        .get("settings")
        .and_then(Value::as_object)
        .is_some_and(|settings| settings.contains_key("watermark"));

    if has_watermark {
        let settings = project.get("settings").cloned().unwrap_or(Value::Null);
        let canvas_size = settings.get("canvasSize").filter(|value| value.is_object());
        let canvas_width = canvas_size
            .and_then(|size| size.get("width"))
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(DEFAULT_CANVAS_WIDTH);
        let canvas_height = canvas_size
            .and_then(|size| size.get("height"))
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(DEFAULT_CANVAS_HEIGHT);

        let watermark = settings
            .get("watermark")
            .filter(|value| value.is_object())
            .map(|watermark| {
                let source = normalize_watermark_source(watermark.get("source"), true);
                let offset = watermark.get("offset").filter(|value| value.is_object());
                let to_ratio = |value: Option<&Value>, extent: f64| -> f64 {
                    value
                        .and_then(Value::as_f64)
                        .filter(|value| value.is_finite())
                        .map(|value| value / extent)
                        .unwrap_or(0.0)
                };
                let tiling = watermark.get("tiling").filter(|value| value.is_object());
                let spacing = finite(tiling.and_then(|tiling| tiling.get("spacing")));
                let timing = watermark.get("timing").filter(|value| value.is_object());

                json!({
                    "enabled": watermark.get("enabled") == Some(&Value::Bool(true)) && source.is_some(),
                    "source": source.clone().unwrap_or(Value::Null),
                    "anchor": watermark
                        .get("anchor")
                        .and_then(Value::as_str)
                        .filter(|anchor| ANCHORS.contains(anchor))
                        .unwrap_or("bottomRight"),
                    "offset": {
                        "x": to_ratio(offset.and_then(|offset| offset.get("x")), canvas_width),
                        "y": to_ratio(offset.and_then(|offset| offset.get("y")), canvas_height),
                    },
                    "size": clamp_with_fallback(finite(watermark.get("size")), 0.01, 1.0, 0.18),
                    "opacity": clamp_with_fallback(finite(watermark.get("opacity")), 0.0, 1.0, 0.7),
                    "rotation": finite(watermark.get("rotation")),
                    "blendMode": watermark
                        .get("blendMode")
                        .and_then(Value::as_str)
                        .filter(|mode| BLEND_MODES.contains(mode))
                        .unwrap_or("normal"),
                    "tiling": {
                        "enabled": tiling.and_then(|tiling| tiling.get("enabled")) == Some(&Value::Bool(true)),
                        "spacing": if spacing > 0.0 { spacing.min(4.0) } else { 0.6 },
                        "angle": finite(tiling.and_then(|tiling| tiling.get("angle"))),
                    },
                    "timing": {
                        "mode": if timing.and_then(|timing| timing.get("mode")) == Some(&Value::from("range")) { "range" } else { "always" },
                        "start": finite(timing.and_then(|timing| timing.get("start"))).max(0.0),
                        "end": finite(timing.and_then(|timing| timing.get("end"))).max(0.0),
                        "fadeIn": finite(timing.and_then(|timing| timing.get("fadeIn"))).max(0.0),
                        "fadeOut": finite(timing.and_then(|timing| timing.get("fadeOut"))).max(0.0),
                    },
                })
            });

        if let Some(settings) = project.get_mut("settings").and_then(Value::as_object_mut) {
            match watermark {
                Some(watermark) => {
                    settings.insert("watermark".to_string(), watermark);
                }
                None => {
                    settings.remove("watermark");
                }
            }
        }
    }

    set_version(&mut project, 30);
    MigrationResult::migrated(project)
}
