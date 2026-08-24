use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use template::{
    CanvasSpec, FpsSpec, SlotKind, TEMPLATE_FORMAT_VERSION, TemplateManifest, TemplateSlot,
};
use time::MediaTime;

use crate::model::{MediaAssetData, MediaType, SceneTracks, TimelineElement, Track};

pub const TEMPLATE_SCENES_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateMediaRef {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub media_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateScenes {
    pub version: u32,
    pub tracks: SceneTracks,
    pub media: Vec<TemplateMediaRef>,
}

#[derive(Debug, PartialEq)]
pub enum TemplateProjectError {
    NoSlots,
}

impl std::fmt::Display for TemplateProjectError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoSlots => write!(formatter, "template has no slots"),
        }
    }
}

impl std::error::Error for TemplateProjectError {}

fn slot_kind_for(element: &TimelineElement) -> Option<SlotKind> {
    match element {
        TimelineElement::Video(_) => Some(SlotKind::Video),
        TimelineElement::Image(_) => Some(SlotKind::Image),
        TimelineElement::Audio(_) => Some(SlotKind::Audio),
        TimelineElement::Text(_) => Some(SlotKind::Text),
        _ => None,
    }
}

fn collect_elements(tracks: &SceneTracks) -> Vec<&TimelineElement> {
    let mut elements = Vec::new();
    for track in &tracks.overlay {
        elements.extend(track.elements());
    }
    elements.extend(tracks.main.elements());
    for track in &tracks.audio {
        elements.extend(track.elements());
    }
    elements
}

fn media_type_name(media_type: MediaType) -> &'static str {
    match media_type {
        MediaType::Image => "image",
        MediaType::Video => "video",
        MediaType::Audio => "audio",
    }
}

pub fn parse_scenes(value: &Value) -> Option<TemplateScenes> {
    let scenes: TemplateScenes = serde_json::from_value(value.clone()).ok()?;
    (scenes.version == TEMPLATE_SCENES_VERSION).then_some(scenes)
}

pub fn build_template_from_project(
    name: &str,
    description: &str,
    canvas: CanvasSpec,
    fps: FpsSpec,
    tracks: &SceneTracks,
    media_assets: &[MediaAssetData],
) -> Result<TemplateManifest, TemplateProjectError> {
    let mut slots = Vec::new();
    let mut used_media: HashSet<&str> = HashSet::new();

    for element in collect_elements(tracks) {
        let Some(kind) = slot_kind_for(element) else {
            continue;
        };
        let base = element.base();
        if base.duration.as_ticks() <= 0 {
            continue;
        }
        if let Some(media_id) = element.media_id() {
            used_media.insert(media_id);
        }

        slots.push(TemplateSlot {
            id: base.id.clone(),
            kind,
            label: base.name.clone(),
            start_seconds: base.start_time.to_seconds_f64().max(0.0) as f32,
            duration_seconds: base.duration.to_seconds_f64() as f32,
            required: kind != SlotKind::Text,
            placeholder_text: match element {
                TimelineElement::Text(text) => Some(text.content.clone()),
                _ => None,
            },
        });
    }

    if slots.is_empty() {
        return Err(TemplateProjectError::NoSlots);
    }

    let duration_seconds = slots.iter().fold(0.0_f32, |longest, slot| {
        longest.max(slot.start_seconds + slot.duration_seconds)
    });

    let scenes = TemplateScenes {
        version: TEMPLATE_SCENES_VERSION,
        tracks: tracks.clone(),
        media: media_assets
            .iter()
            .filter(|asset| used_media.contains(asset.id.as_str()))
            .map(|asset| TemplateMediaRef {
                id: asset.id.clone(),
                name: asset.name.clone(),
                media_type: media_type_name(asset.media_type).to_owned(),
            })
            .collect(),
    };

    let trimmed = name.trim();
    Ok(TemplateManifest {
        format_version: TEMPLATE_FORMAT_VERSION,
        name: if trimmed.is_empty() {
            String::from("Untitled")
        } else {
            trimmed.to_owned()
        },
        description: description.to_owned(),
        author: String::new(),
        license: String::new(),
        canvas,
        fps,
        duration_seconds,
        slots,
        scenes: serde_json::to_value(&scenes).unwrap_or(Value::Null),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct TemplateInstantiation {
    pub tracks: SceneTracks,
    pub restored_elements: usize,
    pub dropped_elements: usize,
    pub missing_media: Vec<TemplateMediaRef>,
}

pub fn instantiate_template(
    manifest: &TemplateManifest,
    media_assets: &[MediaAssetData],
) -> Option<TemplateInstantiation> {
    let scenes = parse_scenes(&manifest.scenes)?;
    let available: HashSet<&str> = media_assets.iter().map(|asset| asset.id.as_str()).collect();
    let mut restored = 0usize;
    let mut dropped = 0usize;

    let mut filter_track = |track: &Track| -> Track {
        let mut copy = track.clone();
        copy.elements_mut()
            .retain(|element| match element.media_id() {
                Some(media_id) if !available.contains(media_id) => {
                    dropped += 1;
                    false
                }
                _ => {
                    restored += 1;
                    true
                }
            });
        copy
    };

    let tracks = SceneTracks {
        overlay: scenes
            .tracks
            .overlay
            .iter()
            .map(&mut filter_track)
            .collect(),
        main: filter_track(&scenes.tracks.main),
        audio: scenes.tracks.audio.iter().map(&mut filter_track).collect(),
    };

    Some(TemplateInstantiation {
        tracks,
        restored_elements: restored,
        dropped_elements: dropped,
        missing_media: scenes
            .media
            .into_iter()
            .filter(|reference| !available.contains(reference.id.as_str()))
            .collect(),
    })
}

pub fn manifest_duration(manifest: &TemplateManifest) -> MediaTime {
    MediaTime::from_seconds_f64(manifest.duration_seconds as f64).unwrap_or(MediaTime::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Project;

    fn project_with_text() -> Project {
        let mut project = Project::new("test", "1970-01-01T00:00:00.000Z".into());
        let scene = project.scenes.first_mut().unwrap();
        let element: TimelineElement = serde_json::from_value(serde_json::json!({
            "type": "text",
            "id": "text-1",
            "name": "Title",
            "duration": MediaTime::from_seconds_f64(3.0).unwrap().as_ticks(),
            "startTime": MediaTime::from_seconds_f64(1.0).unwrap().as_ticks(),
            "trimStart": 0,
            "trimEnd": 0,
            "content": "Hello",
            "fontSize": 15.0,
            "fontFamily": "Arial",
            "color": "#ffffff",
            "background": { "enabled": false, "color": "#000000" },
            "textAlign": "center",
            "fontWeight": "bold",
            "fontStyle": "normal",
            "textDecoration": "none",
            "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
            "opacity": 1.0
        }))
        .unwrap();
        scene.tracks.main.elements_mut().push(element);
        project
    }

    fn canvas() -> CanvasSpec {
        CanvasSpec {
            width: 1920,
            height: 1080,
        }
    }

    fn fps() -> FpsSpec {
        FpsSpec {
            numerator: 30,
            denominator: 1,
        }
    }

    #[test]
    fn saving_a_project_yields_a_slot_per_element() {
        let project = project_with_text();
        let tracks = &project.scenes[0].tracks;
        let manifest =
            build_template_from_project("  My template  ", "notes", canvas(), fps(), tracks, &[])
                .expect("manifest");

        assert_eq!(manifest.name, "My template");
        assert_eq!(manifest.slots.len(), 1);
        assert_eq!(manifest.slots[0].kind, SlotKind::Text);
        assert_eq!(manifest.slots[0].label, "Title");
        assert!(!manifest.slots[0].required);
        assert_eq!(manifest.slots[0].placeholder_text.as_deref(), Some("Hello"));
        assert!((manifest.slots[0].start_seconds - 1.0).abs() < 1e-3);
        assert!((manifest.duration_seconds - 4.0).abs() < 1e-3);
        template::validate(&manifest).expect("valid");
    }

    #[test]
    fn an_empty_timeline_cannot_become_a_template() {
        let project = Project::new("test", "1970-01-01T00:00:00.000Z".into());
        let error = build_template_from_project(
            "empty",
            "",
            canvas(),
            fps(),
            &project.scenes[0].tracks,
            &[],
        )
        .unwrap_err();
        assert_eq!(error, TemplateProjectError::NoSlots);
    }

    #[test]
    fn a_saved_template_round_trips_through_json() {
        let project = project_with_text();
        let manifest = build_template_from_project(
            "round trip",
            "",
            canvas(),
            fps(),
            &project.scenes[0].tracks,
            &[],
        )
        .expect("manifest");
        let raw = template::to_json(&manifest).expect("json");
        let parsed = template::parse(&raw).expect("parse");

        let restored = instantiate_template(&parsed, &[]).expect("scenes");
        assert_eq!(restored.restored_elements, 1);
        assert_eq!(restored.dropped_elements, 0);
        assert_eq!(restored.tracks.main.elements().len(), 1);
        assert_eq!(restored.tracks.main.elements()[0].base().name, "Title");
    }

    #[test]
    fn elements_whose_media_is_missing_are_dropped() {
        let mut project = Project::new("test", "1970-01-01T00:00:00.000Z".into());
        let element: TimelineElement = serde_json::from_value(serde_json::json!({
            "type": "image",
            "id": "image-1",
            "name": "Photo",
            "duration": MediaTime::from_seconds_f64(2.0).unwrap().as_ticks(),
            "startTime": 0,
            "trimStart": 0,
            "trimEnd": 0,
            "mediaId": "asset-1",
            "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
            "opacity": 1.0
        }))
        .unwrap();
        project.scenes[0].tracks.main.elements_mut().push(element);

        let asset = MediaAssetData {
            id: String::from("asset-1"),
            name: String::from("Photo.png"),
            media_type: MediaType::Image,
            ..serde_json::from_value(serde_json::json!({
                "id": "asset-1",
                "name": "Photo.png",
                "type": "image",
                "size": 0,
                "lastModified": 0
            }))
            .unwrap()
        };

        let manifest = build_template_from_project(
            "with media",
            "",
            canvas(),
            fps(),
            &project.scenes[0].tracks,
            std::slice::from_ref(&asset),
        )
        .expect("manifest");

        let with_media =
            instantiate_template(&manifest, std::slice::from_ref(&asset)).expect("scenes");
        assert_eq!(with_media.restored_elements, 1);
        assert!(with_media.missing_media.is_empty());

        let without_media = instantiate_template(&manifest, &[]).expect("scenes");
        assert_eq!(without_media.restored_elements, 0);
        assert_eq!(without_media.dropped_elements, 1);
        assert_eq!(without_media.missing_media.len(), 1);
        assert_eq!(without_media.missing_media[0].name, "Photo.png");
    }

    #[test]
    fn builtin_templates_carry_no_timeline_to_restore() {
        for manifest in template::builtin_templates() {
            assert!(
                instantiate_template(&manifest, &[]).is_none(),
                "{}",
                manifest.name
            );
            assert!(!manifest.slots.is_empty());
        }
    }
}
