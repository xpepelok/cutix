use serde_json::{json, Value};

pub const DEFAULT_FONT_FAMILY: &str = "Arial";
pub const DEFAULT_FONT_SIZE: f64 = 15.0;
pub const DEFAULT_COLOR: &str = "#ffffff";
pub const DEFAULT_LETTER_SPACING: f64 = 0.0;
pub const DEFAULT_LINE_HEIGHT: f64 = 1.2;
pub const DEFAULT_PADDING_X: f64 = 30.0;
pub const DEFAULT_PADDING_Y: f64 = 42.0;

pub struct TextPreset {
    pub id: &'static str,
    pub name_key: &'static str,
    pub font_family: Option<&'static str>,
    pub font_size_ratio: f64,
    pub bold: bool,
    pub color: &'static str,
    pub letter_spacing: f64,
    pub line_height: f64,
    pub background: Option<(&'static str, f64, f64, f64)>,
    pub stroke: Option<(&'static str, f64)>,
    pub shadow: Option<(&'static str, f64, f64, f64)>,
    pub gradient: Option<(&'static str, &'static str, f64)>,
}

const fn preset(id: &'static str, name_key: &'static str) -> TextPreset {
    TextPreset {
        id,
        name_key,
        font_family: None,
        font_size_ratio: 1.0,
        bold: false,
        color: DEFAULT_COLOR,
        letter_spacing: DEFAULT_LETTER_SPACING,
        line_height: DEFAULT_LINE_HEIGHT,
        background: None,
        stroke: None,
        shadow: None,
        gradient: None,
    }
}

pub fn presets() -> Vec<TextPreset> {
    vec![
        preset("plain", "text.preset.plain"),
        TextPreset {
            bold: true,
            stroke: Some(("#000000", 0.14)),
            ..preset("bold-outline", "text.preset.boldOutline")
        },
        TextPreset {
            bold: true,
            shadow: Some(("#000000", 0.08, 0.06, 0.07)),
            ..preset("drop-shadow", "text.preset.dropShadow")
        },
        TextPreset {
            bold: true,
            color: "#eafcff",
            stroke: Some(("#22d3ee", 0.04)),
            shadow: Some(("#22d3ee", 0.45, 0.0, 0.0)),
            ..preset("neon-glow", "text.preset.neonGlow")
        },
        TextPreset {
            bold: true,
            background: Some(("#000000", 25.0, 26.0, 24.0)),
            ..preset("boxed", "text.preset.boxed")
        },
        TextPreset {
            bold: true,
            color: "#111111",
            background: Some(("#ffe14d", 8.0, 20.0, 18.0)),
            ..preset("highlight", "text.preset.highlight")
        },
        TextPreset {
            bold: true,
            color: "#ff8a3d",
            gradient: Some(("#ffb347", "#ff2d95", 90.0)),
            ..preset("gradient", "text.preset.gradient")
        },
        TextPreset {
            font_family: Some("Bebas Neue"),
            bold: true,
            color: "#ffe066",
            letter_spacing: 2.0,
            stroke: Some(("#4a1500", 0.09)),
            shadow: Some(("#ff5c00", 0.0, 0.07, 0.07)),
            ..preset("retro", "text.preset.retro")
        },
        TextPreset {
            font_family: Some("Caveat"),
            bold: true,
            color: "#fff7e6",
            line_height: 1.1,
            font_size_ratio: 1.25,
            shadow: Some(("#00000099", 0.12, 0.02, 0.04)),
            ..preset("handwritten", "text.preset.handwritten")
        },
        TextPreset {
            bold: true,
            stroke: Some(("#000000", 0.08)),
            shadow: Some(("#000000cc", 0.1, 0.0, 0.03)),
            ..preset("subtitle", "text.preset.subtitle")
        },
        TextPreset {
            bold: true,
            background: Some(("#101014", 15.0, 22.0, 20.0)),
            ..preset("subtitle-box", "text.preset.subtitleBox")
        },
        TextPreset {
            bold: true,
            font_size_ratio: 1.6,
            letter_spacing: 4.0,
            stroke: Some(("#000000", 0.05)),
            shadow: Some(("#000000aa", 0.2, 0.0, 0.05)),
            ..preset("title", "text.preset.title")
        },
    ]
}

pub struct PresetPatch {
    pub font_family: String,
    pub font_size: f64,
    pub font_weight: String,
    pub color: String,
    pub letter_spacing: f64,
    pub line_height: f64,
    pub background_enabled: bool,
    pub background_color: String,
    pub corner_radius: f64,
    pub padding_x: f64,
    pub padding_y: f64,
    pub stroke: Value,
    pub shadow: Value,
    pub gradient: Value,
}

pub fn patch_for(preset: &TextPreset) -> PresetPatch {
    let font_size = DEFAULT_FONT_SIZE * preset.font_size_ratio;
    let (background_enabled, background_color, corner_radius, padding_x, padding_y) =
        match preset.background {
            Some((color, radius, padding_x, padding_y)) => {
                (true, color.to_string(), radius, padding_x, padding_y)
            }
            None => (
                false,
                String::from("#000000"),
                0.0,
                DEFAULT_PADDING_X,
                DEFAULT_PADDING_Y,
            ),
        };

    PresetPatch {
        font_family: preset
            .font_family
            .unwrap_or(DEFAULT_FONT_FAMILY)
            .to_string(),
        font_size,
        font_weight: String::from(if preset.bold { "bold" } else { "normal" }),
        color: preset.color.to_string(),
        letter_spacing: preset.letter_spacing,
        line_height: preset.line_height,
        background_enabled,
        background_color,
        corner_radius,
        padding_x,
        padding_y,
        stroke: match preset.stroke {
            Some((color, em)) => json!({
                "enabled": true,
                "color": color,
                "width": em * font_size,
            }),
            None => crate::edit::default_stroke(),
        },
        shadow: match preset.shadow {
            Some((color, blur, offset_x, offset_y)) => json!({
                "enabled": true,
                "color": color,
                "blur": blur * font_size,
                "offsetX": offset_x * font_size,
                "offsetY": offset_y * font_size,
            }),
            None => crate::edit::default_shadow(),
        },
        gradient: match preset.gradient {
            Some((from, to, angle)) => json!({
                "enabled": true,
                "from": from,
                "to": to,
                "angle": angle,
            }),
            None => json!({ "enabled": false, "from": "#ffffff", "to": "#ffffff", "angle": 90.0 }),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preset_list_matches_the_web_order() {
        let ids: Vec<&str> = presets().iter().map(|preset| preset.id).collect();
        assert_eq!(
            ids,
            vec![
                "plain",
                "bold-outline",
                "drop-shadow",
                "neon-glow",
                "boxed",
                "highlight",
                "gradient",
                "retro",
                "handwritten",
                "subtitle",
                "subtitle-box",
                "title",
            ]
        );
    }

    #[test]
    fn every_preset_name_is_translated() {
        for preset in presets() {
            assert_ne!(cutix_i18n::t(preset.name_key), preset.name_key);
        }
    }

    #[test]
    fn em_relative_strokes_resolve_against_the_preset_font_size() {
        let list = presets();
        let outline = list
            .iter()
            .find(|preset| preset.id == "bold-outline")
            .unwrap();
        let patch = patch_for(outline);
        assert_eq!(patch.stroke["width"].as_f64(), Some(0.14 * 15.0));
        assert_eq!(patch.font_weight, "bold");

        let title = list.iter().find(|preset| preset.id == "title").unwrap();
        assert_eq!(patch_for(title).font_size, 24.0);
    }
}
