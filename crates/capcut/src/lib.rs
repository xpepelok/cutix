mod materials;

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub use materials::{LocalAsset, Media, MediaKind, Segment, Sticker, TextMaterial};
use materials::{
    RawDraft, collect_media, collect_segments, collect_speeds, collect_stickers, collect_texts,
};
use template::{
    CanvasSpec, FpsSpec, SlotKind, TEMPLATE_FORMAT_VERSION, TemplateManifest, TemplateSlot,
};

pub const CONTENT_FILE: &str = "draft_content.json";
pub const META_FILE: &str = "draft_meta_info.json";

#[derive(Debug)]
pub enum CapCutError {
    NotJson(String),
    NoTracks,
    NoSegments,
    Io(String),
}

impl std::fmt::Display for CapCutError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotJson(detail) => write!(formatter, "draft is not valid JSON: {detail}"),
            Self::NoTracks => write!(formatter, "draft has no tracks"),
            Self::NoSegments => write!(formatter, "draft has no usable segments"),
            Self::Io(detail) => write!(formatter, "cannot read draft: {detail}"),
        }
    }
}

impl std::error::Error for CapCutError {}

pub struct Draft {
    pub name: String,
    pub canvas: (u32, u32),
    pub fps: u32,
    pub duration_seconds: f32,
    pub media: HashMap<String, Media>,
    pub texts: HashMap<String, TextMaterial>,
    pub stickers: HashMap<String, Sticker>,
    pub segments: Vec<Segment>,
    pub transitions: usize,
    pub video_effects: usize,
}

pub fn default_drafts_directory() -> Option<PathBuf> {
    let local = std::env::var("LOCALAPPDATA").ok()?;
    Some(
        Path::new(&local)
            .join("CapCut")
            .join("User Data")
            .join("Projects")
            .join("com.lveditor.draft"),
    )
}

pub fn find_drafts(directory: impl AsRef<Path>) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(directory) else {
        return found;
    };

    for entry in entries.flatten() {
        let content = entry.path().join(CONTENT_FILE);
        if content.is_file() {
            found.push(content);
        }
    }

    found.sort();
    found
}

pub fn read_draft(raw: &str, name: &str) -> Result<Draft, CapCutError> {
    let parsed: RawDraft =
        serde_json::from_str(raw).map_err(|error| CapCutError::NotJson(error.to_string()))?;

    if parsed.tracks.is_empty() {
        return Err(CapCutError::NoTracks);
    }

    let speeds = collect_speeds(&parsed.materials);
    let segments = collect_segments(&parsed.tracks, &speeds);
    let longest = segments
        .iter()
        .map(|segment| segment.start_seconds + segment.duration_seconds)
        .fold(0.0_f32, f32::max);

    Ok(Draft {
        name: name.to_string(),
        canvas: (
            if parsed.canvas_config.width == 0 {
                1080
            } else {
                parsed.canvas_config.width
            },
            if parsed.canvas_config.height == 0 {
                1920
            } else {
                parsed.canvas_config.height
            },
        ),
        fps: if parsed.fps <= 0.0 {
            30
        } else {
            parsed.fps.round() as u32
        },
        duration_seconds: ((parsed.duration / 1_000_000.0) as f32).max(longest),
        media: collect_media(&parsed.materials),
        texts: collect_texts(&parsed.materials),
        stickers: collect_stickers(&parsed.materials),
        segments,
        transitions: parsed.materials.transitions.len(),
        video_effects: parsed.materials.video_effects.len(),
    })
}

pub fn load_draft(path: impl AsRef<Path>) -> Result<Draft, CapCutError> {
    let path = path.as_ref();
    let raw = fs::read_to_string(path).map_err(|error| CapCutError::Io(error.to_string()))?;
    let name = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("CapCut draft")
        .to_string();
    read_draft(&raw, &name)
}

pub fn collect_local_assets(draft: &Draft) -> Vec<LocalAsset> {
    let mut assets: Vec<LocalAsset> = draft
        .media
        .values()
        .filter(|media| !media.path.is_empty())
        .map(|media| LocalAsset {
            kind: match media.kind {
                MediaKind::Video => "video".to_string(),
                MediaKind::Image => "image".to_string(),
                MediaKind::Audio => "audio".to_string(),
            },
            path: media.path.clone(),
            name: media.name.clone(),
        })
        .collect();

    assets.extend(
        draft
            .stickers
            .values()
            .filter(|sticker| !sticker.path.is_empty())
            .map(|sticker| LocalAsset {
                kind: "sticker".to_string(),
                path: sticker.path.clone(),
                name: sticker.name.clone(),
            }),
    );

    assets.sort_by(|left, right| left.name.cmp(&right.name));
    assets
}

fn slot_kind_for(track_type: &str, draft: &Draft, material_id: &str) -> Option<SlotKind> {
    match track_type {
        "text" | "sticker" => Some(SlotKind::Text),
        "audio" => Some(SlotKind::Audio),
        "video" => Some(match draft.media.get(material_id).map(|media| media.kind) {
            Some(MediaKind::Image) => SlotKind::Image,
            _ => SlotKind::Video,
        }),
        _ => None,
    }
}

fn label_for(draft: &Draft, material_id: &str, kind: SlotKind, index: usize) -> String {
    if let Some(media) = draft.media.get(material_id) {
        return media.name.clone();
    }
    if let Some(sticker) = draft.stickers.get(material_id) {
        return sticker.name.clone();
    }
    if let Some(text) = draft.texts.get(material_id)
        && !text.content.is_empty()
    {
        return text.content.chars().take(40).collect();
    }
    format!("{kind:?} {index}")
}

pub fn draft_to_template(draft: &Draft) -> Result<TemplateManifest, CapCutError> {
    let mut slots = Vec::new();

    for segment in &draft.segments {
        let Some(kind) = slot_kind_for(&segment.track_type, draft, &segment.material_id) else {
            continue;
        };

        let index = slots.len() + 1;
        slots.push(TemplateSlot {
            id: format!("{kind:?}-{index}").to_lowercase(),
            kind,
            label: label_for(draft, &segment.material_id, kind, index),
            start_seconds: segment.start_seconds,
            duration_seconds: segment.duration_seconds,
            required: kind != SlotKind::Text,
            placeholder_text: draft
                .texts
                .get(&segment.material_id)
                .map(|text| text.content.clone())
                .filter(|content| !content.is_empty()),
        });
    }

    if slots.is_empty() {
        return Err(CapCutError::NoSegments);
    }

    let mut notes = format!(
        "{} media, {} texts, {} stickers",
        draft.media.len(),
        draft.texts.len(),
        draft.stickers.len()
    );
    if draft.transitions > 0 {
        notes.push_str(&format!(", {} transitions", draft.transitions));
    }
    if draft.video_effects > 0 {
        notes.push_str(&format!(", {} effects", draft.video_effects));
    }

    Ok(TemplateManifest {
        format_version: TEMPLATE_FORMAT_VERSION,
        name: draft.name.clone(),
        description: format!("Imported from a local CapCut draft — {notes}"),
        author: String::new(),
        license: String::new(),
        canvas: CanvasSpec {
            width: draft.canvas.0,
            height: draft.canvas.1,
        },
        fps: FpsSpec {
            numerator: draft.fps,
            denominator: 1,
        },
        duration_seconds: draft.duration_seconds,
        slots,
        scenes: serde_json::Value::Null,
    })
}

pub fn parse_draft(raw: &str, name: &str) -> Result<TemplateManifest, CapCutError> {
    draft_to_template(&read_draft(raw, name)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: f64 = 1_000_000.0;

    fn draft_json() -> String {
        format!(
            r#"{{
              "canvas_config": {{ "width": 1080, "height": 1920 }},
              "fps": 30,
              "duration": {},
              "materials": {{
                "videos": [
                  {{ "id": "v1", "path": "C:/clips/beach.mp4", "material_name": "beach.mp4", "duration": {}, "has_audio": true }},
                  {{ "id": "img1", "type": "photo", "path": "C:/clips/logo.png", "material_name": "logo.png" }}
                ],
                "audios": [
                  {{ "id": "a1", "path": "C:/music/track.mp3", "material_name": "track.mp3", "duration": {} }}
                ],
                "texts": [ {{ "id": "t1", "content": "{{\"text\":\"Hello there\"}}", "font_size": 24 }} ],
                "stickers": [ {{ "id": "s1", "resource_id": "res-77", "name": "Crown", "path": "C:/cache/crown.png" }} ],
                "speeds": [ {{ "id": "sp1", "speed": 2 }} ],
                "transitions": [ {{ "id": "tr1" }} ],
                "video_effects": [ {{ "id": "fx1" }}, {{ "id": "fx2" }} ]
              }},
              "tracks": [
                {{ "type": "video", "segments": [
                  {{ "material_id": "v1", "target_timerange": {{ "start": 0, "duration": {} }},
                     "source_timerange": {{ "start": {}, "duration": {} }},
                     "extra_material_refs": ["sp1"], "volume": 0.5 }},
                  {{ "material_id": "img1", "target_timerange": {{ "start": {}, "duration": {} }} }}
                ]}},
                {{ "type": "audio", "segments": [
                  {{ "material_id": "a1", "target_timerange": {{ "start": 0, "duration": {} }} }}
                ]}},
                {{ "type": "text", "segments": [
                  {{ "material_id": "t1", "target_timerange": {{ "start": {}, "duration": {} }} }}
                ]}},
                {{ "type": "sticker", "segments": [
                  {{ "material_id": "s1", "target_timerange": {{ "start": {}, "duration": {} }} }}
                ]}},
                {{ "type": "filter", "segments": [] }}
              ]
            }}"#,
            10.0 * SECOND,
            20.0 * SECOND,
            30.0 * SECOND,
            4.0 * SECOND,
            2.0 * SECOND,
            4.0 * SECOND,
            4.0 * SECOND,
            6.0 * SECOND,
            10.0 * SECOND,
            1.0 * SECOND,
            3.0 * SECOND,
            2.0 * SECOND,
            2.0 * SECOND
        )
    }

    #[test]
    fn produces_a_valid_template() {
        let manifest = parse_draft(&draft_json(), "My draft").expect("parse");
        assert!(template::validate(&manifest).is_ok());
        assert_eq!(manifest.name, "My draft");
    }

    #[test]
    fn collects_media_with_paths() {
        let draft = read_draft(&draft_json(), "d").expect("read");
        assert_eq!(draft.media["v1"].path, "C:/clips/beach.mp4");
        assert_eq!(draft.media["a1"].name, "track.mp3");
        assert!(draft.media["v1"].has_audio);
    }

    #[test]
    fn detects_photos_inside_the_video_section() {
        let draft = read_draft(&draft_json(), "d").expect("read");
        assert_eq!(draft.media["img1"].kind, MediaKind::Image);
    }

    #[test]
    fn decodes_text_content() {
        let draft = read_draft(&draft_json(), "d").expect("read");
        assert_eq!(draft.texts["t1"].content, "Hello there");
        assert_eq!(draft.texts["t1"].font_size, Some(24.0));
    }

    #[test]
    fn collects_stickers_with_resource_ids() {
        let draft = read_draft(&draft_json(), "d").expect("read");
        assert_eq!(draft.stickers["s1"].resource_id, "res-77");
        assert_eq!(draft.stickers["s1"].name, "Crown");
    }

    #[test]
    fn resolves_speed_through_extra_refs() {
        let draft = read_draft(&draft_json(), "d").expect("read");
        assert_eq!(draft.segments[0].speed, 2.0);
        assert_eq!(draft.segments[0].volume, 0.5);
    }

    #[test]
    fn keeps_the_source_trim_offset() {
        let draft = read_draft(&draft_json(), "d").expect("read");
        assert!((draft.segments[0].source_start_seconds - 2.0).abs() < 1e-3);
    }

    #[test]
    fn counts_transitions_and_effects() {
        let draft = read_draft(&draft_json(), "d").expect("read");
        assert_eq!(draft.transitions, 1);
        assert_eq!(draft.video_effects, 2);
    }

    #[test]
    fn lists_local_assets_including_stickers() {
        let draft = read_draft(&draft_json(), "d").expect("read");
        let assets = collect_local_assets(&draft);
        assert_eq!(assets.len(), 4);
        assert!(assets.iter().any(|asset| asset.kind == "sticker"));
        assert!(assets.iter().any(|asset| asset.kind == "audio"));
    }

    #[test]
    fn maps_every_track_type() {
        let manifest = parse_draft(&draft_json(), "d").expect("parse");
        let kinds: Vec<SlotKind> = manifest.slots.iter().map(|slot| slot.kind).collect();
        assert_eq!(
            kinds,
            vec![
                SlotKind::Video,
                SlotKind::Image,
                SlotKind::Audio,
                SlotKind::Text,
                SlotKind::Text
            ]
        );
    }

    #[test]
    fn names_slots_after_the_original_media() {
        let manifest = parse_draft(&draft_json(), "d").expect("parse");
        assert_eq!(manifest.slots[0].label, "beach.mp4");
        assert_eq!(manifest.slots[2].label, "track.mp3");
    }

    #[test]
    fn carries_text_into_the_placeholder() {
        let manifest = parse_draft(&draft_json(), "d").expect("parse");
        let text = manifest
            .slots
            .iter()
            .find(|slot| slot.placeholder_text.is_some())
            .expect("text slot");
        assert_eq!(text.placeholder_text.as_deref(), Some("Hello there"));
    }

    #[test]
    fn rejects_malformed_json() {
        assert!(matches!(
            parse_draft("{ nope", "d"),
            Err(CapCutError::NotJson(_))
        ));
    }

    #[test]
    fn rejects_a_draft_without_tracks() {
        assert!(matches!(parse_draft("{}", "d"), Err(CapCutError::NoTracks)));
    }

    #[test]
    fn rejects_a_draft_without_segments() {
        let empty = r#"{ "tracks": [{ "type": "filter", "segments": [] }] }"#;
        assert!(matches!(
            parse_draft(empty, "d"),
            Err(CapCutError::NoSegments)
        ));
    }

    #[test]
    fn missing_directory_yields_no_drafts() {
        assert!(find_drafts("this-path-does-not-exist").is_empty());
    }
}
