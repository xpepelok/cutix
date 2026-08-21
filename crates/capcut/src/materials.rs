use std::collections::HashMap;

use serde::Deserialize;

const MICROSECONDS_PER_SECOND: f64 = 1_000_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaKind {
    Video,
    Image,
    Audio,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Media {
    pub id: String,
    pub kind: MediaKind,
    pub path: String,
    pub name: String,
    pub duration_seconds: f32,
    pub has_audio: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextMaterial {
    pub id: String,
    pub content: String,
    pub font_size: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sticker {
    pub id: String,
    pub resource_id: String,
    pub name: String,
    pub path: String,
    pub category: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub material_id: String,
    pub start_seconds: f32,
    pub duration_seconds: f32,
    pub source_start_seconds: f32,
    pub speed: f32,
    pub volume: f32,
    pub track_type: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LocalAsset {
    pub kind: String,
    pub path: String,
    pub name: String,
}

#[derive(Deserialize, Default)]
pub(crate) struct RawTimerange {
    #[serde(default)]
    pub start: f64,
    #[serde(default)]
    pub duration: f64,
}

#[derive(Deserialize)]
pub(crate) struct RawSegment {
    #[serde(default)]
    pub material_id: String,
    #[serde(default)]
    pub target_timerange: Option<RawTimerange>,
    #[serde(default)]
    pub source_timerange: Option<RawTimerange>,
    #[serde(default)]
    pub extra_material_refs: Vec<String>,
    #[serde(default)]
    pub speed: Option<f64>,
    #[serde(default)]
    pub volume: Option<f64>,
}

#[derive(Deserialize)]
pub(crate) struct RawTrack {
    #[serde(default, rename = "type")]
    pub track_type: String,
    #[serde(default)]
    pub segments: Vec<RawSegment>,
}

#[derive(Deserialize, Default)]
pub(crate) struct RawCanvas {
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
}

#[derive(Deserialize, Default)]
pub(crate) struct RawMaterialEntry {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub material_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub has_audio: bool,
    #[serde(default, rename = "type")]
    pub entry_type: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub font_size: Option<f64>,
    #[serde(default)]
    pub resource_id: String,
    #[serde(default)]
    pub category_name: String,
    #[serde(default)]
    pub speed: Option<f64>,
}

#[derive(Deserialize, Default)]
pub(crate) struct RawMaterials {
    #[serde(default)]
    pub videos: Vec<RawMaterialEntry>,
    #[serde(default)]
    pub images: Vec<RawMaterialEntry>,
    #[serde(default)]
    pub audios: Vec<RawMaterialEntry>,
    #[serde(default)]
    pub texts: Vec<RawMaterialEntry>,
    #[serde(default)]
    pub stickers: Vec<RawMaterialEntry>,
    #[serde(default)]
    pub text_templates: Vec<RawMaterialEntry>,
    #[serde(default)]
    pub speeds: Vec<RawMaterialEntry>,
    #[serde(default)]
    pub transitions: Vec<RawMaterialEntry>,
    #[serde(default)]
    pub video_effects: Vec<RawMaterialEntry>,
}

#[derive(Deserialize)]
pub(crate) struct RawDraft {
    #[serde(default, deserialize_with = "null_as_default")]
    pub canvas_config: RawCanvas,
    #[serde(default)]
    pub fps: f64,
    #[serde(default)]
    pub duration: f64,
    #[serde(default)]
    pub tracks: Vec<RawTrack>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub materials: RawMaterials,
}

fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

fn to_seconds(microseconds: f64) -> f32 {
    (microseconds / MICROSECONDS_PER_SECOND) as f32
}

fn file_name_of(path: &str) -> String {
    path.rsplit(['\\', '/']).next().unwrap_or(path).to_string()
}

fn decode_text_content(raw: &str) -> String {
    if raw.is_empty() {
        return String::new();
    }
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|value| {
            value
                .get("text")
                .and_then(|text| text.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| raw.to_string())
}

pub(crate) fn collect_media(materials: &RawMaterials) -> HashMap<String, Media> {
    let mut media = HashMap::new();

    for (entries, kind) in [
        (&materials.videos, MediaKind::Video),
        (&materials.images, MediaKind::Image),
        (&materials.audios, MediaKind::Audio),
    ] {
        for entry in entries {
            if entry.id.is_empty() {
                continue;
            }
            let resolved = if kind == MediaKind::Video && entry.entry_type == "photo" {
                MediaKind::Image
            } else {
                kind
            };
            let name = if !entry.material_name.is_empty() {
                entry.material_name.clone()
            } else if !entry.path.is_empty() {
                file_name_of(&entry.path)
            } else {
                entry.id.clone()
            };

            media.insert(
                entry.id.clone(),
                Media {
                    id: entry.id.clone(),
                    kind: resolved,
                    path: entry.path.clone(),
                    name,
                    duration_seconds: to_seconds(entry.duration),
                    has_audio: entry.has_audio,
                },
            );
        }
    }

    media
}

pub(crate) fn collect_texts(materials: &RawMaterials) -> HashMap<String, TextMaterial> {
    materials
        .texts
        .iter()
        .filter(|entry| !entry.id.is_empty())
        .map(|entry| {
            (
                entry.id.clone(),
                TextMaterial {
                    id: entry.id.clone(),
                    content: decode_text_content(&entry.content),
                    font_size: entry.font_size.map(|size| size as f32),
                },
            )
        })
        .collect()
}

pub(crate) fn collect_stickers(materials: &RawMaterials) -> HashMap<String, Sticker> {
    let mut stickers = HashMap::new();

    for entries in [&materials.stickers, &materials.text_templates] {
        for entry in entries {
            if entry.id.is_empty() {
                continue;
            }
            let name = if !entry.name.is_empty() {
                entry.name.clone()
            } else if !entry.material_name.is_empty() {
                entry.material_name.clone()
            } else {
                entry.id.clone()
            };

            stickers.insert(
                entry.id.clone(),
                Sticker {
                    id: entry.id.clone(),
                    resource_id: entry.resource_id.clone(),
                    name,
                    path: entry.path.clone(),
                    category: entry.category_name.clone(),
                },
            );
        }
    }

    stickers
}

pub(crate) fn collect_speeds(materials: &RawMaterials) -> HashMap<String, f32> {
    materials
        .speeds
        .iter()
        .filter(|entry| !entry.id.is_empty())
        .map(|entry| (entry.id.clone(), entry.speed.unwrap_or(1.0).max(0.0) as f32))
        .collect()
}

pub(crate) fn collect_segments(tracks: &[RawTrack], speeds: &HashMap<String, f32>) -> Vec<Segment> {
    let mut segments = Vec::new();

    for track in tracks {
        for segment in &track.segments {
            let target = segment.target_timerange.as_ref();
            let duration = target
                .map(|range| to_seconds(range.duration))
                .unwrap_or(0.0);
            if duration <= 0.0 {
                continue;
            }

            let referenced = segment
                .extra_material_refs
                .iter()
                .find_map(|reference| speeds.get(reference).copied());
            let speed = referenced
                .or_else(|| segment.speed.map(|value| value as f32))
                .unwrap_or(1.0);

            segments.push(Segment {
                material_id: segment.material_id.clone(),
                start_seconds: target.map(|range| to_seconds(range.start)).unwrap_or(0.0),
                duration_seconds: duration,
                source_start_seconds: segment
                    .source_timerange
                    .as_ref()
                    .map(|range| to_seconds(range.start))
                    .unwrap_or(0.0),
                speed: if speed > 0.0 { speed } else { 1.0 },
                volume: segment.volume.unwrap_or(1.0) as f32,
                track_type: track.track_type.clone(),
            });
        }
    }

    segments
}
