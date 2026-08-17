use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::types::{TWatermark, TWatermarkSource};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredWatermarkPreset {
    pub id: String,
    pub name: String,
    pub saved_at: i64,
    pub watermark: TWatermark,
}

pub fn save_watermark_preset(
    presets: &mut Vec<StoredWatermarkPreset>,
    id: String,
    name: &str,
    saved_at: i64,
    watermark: &TWatermark,
) -> Option<StoredWatermarkPreset> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return None;
    }
    let entry = StoredWatermarkPreset {
        id,
        name: trimmed.to_string(),
        saved_at,
        watermark: watermark.clone(),
    };
    presets.insert(0, entry.clone());
    Some(entry)
}

pub fn list_watermark_presets(presets: &[StoredWatermarkPreset]) -> Vec<StoredWatermarkPreset> {
    let mut sorted = presets.to_vec();
    sorted.sort_by(|left, right| right.saved_at.cmp(&left.saved_at));
    sorted
}

pub fn rename_watermark_preset(presets: &mut [StoredWatermarkPreset], id: &str, name: &str) {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return;
    }
    for entry in presets.iter_mut() {
        if entry.id == id {
            entry.name = trimmed.to_string();
        }
    }
}

pub fn remove_watermark_preset(presets: &mut Vec<StoredWatermarkPreset>, id: &str) {
    presets.retain(|entry| entry.id != id);
}

pub fn apply_watermark_preset(
    preset: &StoredWatermarkPreset,
    available_image_ids: &HashSet<String>,
) -> TWatermark {
    let mut watermark = preset.watermark.clone();
    if let Some(TWatermarkSource::Image { media_id }) = &watermark.source {
        if !available_image_ids.contains(media_id) {
            watermark.source = None;
            watermark.enabled = false;
        }
    }
    watermark
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defaults::create_default_watermark;
    use crate::types::{
        TWatermarkSource, TWatermarkTiling, TWatermarkTiming, TextShadow, TextStroke, Vec2,
        WatermarkAnchor, WatermarkBlendMode, WatermarkTimingMode,
    };

    fn configured_watermark() -> TWatermark {
        let mut mark = create_default_watermark();
        mark.enabled = true;
        mark.source = Some(TWatermarkSource::Text {
            text: "CONFIDENTIAL".to_string(),
            color: "#ff8800".to_string(),
            font_family: "Georgia".to_string(),
            font_weight: 700.0,
            stroke: TextStroke {
                enabled: true,
                color: "#000000".to_string(),
                width: 6.0,
            },
            shadow: TextShadow {
                enabled: true,
                color: "#101010".to_string(),
                blur: 14.0,
                offset_x: 2.0,
                offset_y: 3.0,
            },
        });
        mark.anchor = WatermarkAnchor::Center;
        mark.offset = Vec2 { x: 0.01, y: -0.02 };
        mark.size = 0.33;
        mark.opacity = 0.42;
        mark.rotation = -25.0;
        mark.blend_mode = WatermarkBlendMode::Multiply;
        mark.tiling = TWatermarkTiling {
            enabled: true,
            spacing: 0.8,
            angle: 15.0,
        };
        mark.timing = TWatermarkTiming {
            mode: WatermarkTimingMode::Range,
            start: 1.5,
            end: 8.0,
            fade_in: 0.5,
            fade_out: 0.75,
        };
        mark
    }

    #[test]
    fn round_trips_a_configured_watermark_through_storage() {
        let watermark = configured_watermark();
        let mut presets = Vec::new();
        save_watermark_preset(
            &mut presets,
            "id-1".to_string(),
            "Proof copy",
            1,
            &watermark,
        );

        let serialized = serde_json::to_string(&presets).unwrap();
        let restored: Vec<StoredWatermarkPreset> = serde_json::from_str(&serialized).unwrap();

        let first = list_watermark_presets(&restored)
            .into_iter()
            .next()
            .unwrap();
        assert_eq!(first.name, "Proof copy");
        assert_eq!(first.watermark, watermark);
    }

    #[test]
    fn renames_and_removes_presets() {
        let mut presets = Vec::new();
        let saved = save_watermark_preset(
            &mut presets,
            "id-1".to_string(),
            "One",
            1,
            &configured_watermark(),
        );
        assert!(saved.is_some());
        let id = saved.unwrap().id;

        rename_watermark_preset(&mut presets, &id, "Two");
        assert_eq!(list_watermark_presets(&presets)[0].name, "Two");

        remove_watermark_preset(&mut presets, &id);
        assert!(list_watermark_presets(&presets).is_empty());
    }

    #[test]
    fn rejects_blank_names() {
        let mut presets = Vec::new();
        assert!(save_watermark_preset(
            &mut presets,
            "id".to_string(),
            "  ",
            1,
            &configured_watermark()
        )
        .is_none());
    }

    #[test]
    fn drops_an_image_source_the_current_project_does_not_have() {
        let mut watermark = create_default_watermark();
        watermark.enabled = true;
        watermark.source = Some(TWatermarkSource::Image {
            media_id: "missing".to_string(),
        });
        let mut presets = Vec::new();
        let preset =
            save_watermark_preset(&mut presets, "id".to_string(), "Logo", 1, &watermark).unwrap();
        let available: HashSet<String> = ["other".to_string()].into_iter().collect();
        let applied = apply_watermark_preset(&preset, &available);
        assert!(applied.source.is_none());
        assert!(!applied.enabled);
    }

    #[test]
    fn keeps_an_image_source_the_project_still_has() {
        let mut watermark = create_default_watermark();
        watermark.enabled = true;
        watermark.source = Some(TWatermarkSource::Image {
            media_id: "logo".to_string(),
        });
        let mut presets = Vec::new();
        let preset =
            save_watermark_preset(&mut presets, "id".to_string(), "Logo", 1, &watermark).unwrap();
        let available: HashSet<String> = ["logo".to_string()].into_iter().collect();
        let applied = apply_watermark_preset(&preset, &available);
        assert_eq!(applied, watermark);
    }
}
