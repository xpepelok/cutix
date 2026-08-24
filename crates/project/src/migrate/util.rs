use serde_json::{Map, Value};

pub type Record = Map<String, Value>;

#[derive(Debug, Clone)]
pub struct MigrationResult {
    pub project: Value,
    pub skipped: bool,
    pub reason: Option<String>,
}

impl MigrationResult {
    pub fn migrated(project: Value) -> Self {
        Self {
            project,
            skipped: false,
            reason: None,
        }
    }

    pub fn skipped(project: Value, reason: &str) -> Self {
        Self {
            project,
            skipped: true,
            reason: Some(reason.to_string()),
        }
    }
}

pub fn get_project_id(project: &Value) -> Option<String> {
    let object = project.as_object()?;
    if let Some(id) = object.get("id").and_then(Value::as_str)
        && !id.is_empty()
    {
        return Some(id.to_string());
    }

    let metadata = object.get("metadata")?.as_object()?;
    let id = metadata.get("id")?.as_str()?;
    if id.is_empty() {
        None
    } else {
        Some(id.to_string())
    }
}

pub fn version_of(project: &Value) -> Option<i64> {
    project.get("version").and_then(Value::as_i64)
}

pub fn number_field(record: &Value, key: &str) -> Option<f64> {
    record.get(key).and_then(Value::as_f64)
}

pub fn string_field(record: &Value, key: &str) -> Option<String> {
    record
        .get(key)
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub fn map_scenes(project: &mut Value, mut transform: impl FnMut(&mut Value)) {
    let Some(scenes) = project.get_mut("scenes").and_then(Value::as_array_mut) else {
        return;
    };
    for scene in scenes.iter_mut() {
        transform(scene);
    }
}

pub fn map_flat_elements(project: &mut Value, mut transform: impl FnMut(&mut Value)) {
    map_flat_tracks(project, |track| {
        let Some(elements) = track.get_mut("elements").and_then(Value::as_array_mut) else {
            return;
        };
        for element in elements.iter_mut() {
            transform(element);
        }
    });
}

pub fn map_flat_tracks(project: &mut Value, mut transform: impl FnMut(&mut Value)) {
    map_scenes(project, |scene| {
        let Some(tracks) = scene.get_mut("tracks").and_then(Value::as_array_mut) else {
            return;
        };
        for track in tracks.iter_mut() {
            transform(track);
        }
    });
}

pub fn map_grouped_elements(project: &mut Value, mut transform: impl FnMut(&mut Value)) {
    map_grouped_tracks(project, |track| {
        let Some(elements) = track.get_mut("elements").and_then(Value::as_array_mut) else {
            return;
        };
        for element in elements.iter_mut() {
            transform(element);
        }
    });
}

pub fn map_grouped_tracks(project: &mut Value, mut transform: impl FnMut(&mut Value)) {
    map_scenes(project, |scene| {
        let Some(tracks) = scene.get_mut("tracks") else {
            return;
        };
        if !tracks.is_object() {
            return;
        }

        if let Some(main) = tracks.get_mut("main")
            && main.is_object()
        {
            transform(main);
        }
        for key in ["overlay", "audio"] {
            if let Some(list) = tracks.get_mut(key).and_then(Value::as_array_mut) {
                for track in list.iter_mut() {
                    transform(track);
                }
            }
        }
    });
}

pub fn set_version(project: &mut Value, version: i64) {
    if let Some(object) = project.as_object_mut() {
        object.insert("version".to_string(), Value::from(version));
    }
}

pub fn finite(value: Option<&Value>) -> f64 {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite())
        .unwrap_or(0.0)
}

pub fn clamp_with_fallback(value: f64, min: f64, max: f64, fallback: f64) -> f64 {
    if value <= 0.0 {
        fallback
    } else {
        value.clamp(min, max)
    }
}
