use serde::{Deserialize, Serialize};

pub const WATERMARK_ANCHORS: [WatermarkAnchor; 9] = [
    WatermarkAnchor::TopLeft,
    WatermarkAnchor::Top,
    WatermarkAnchor::TopRight,
    WatermarkAnchor::Left,
    WatermarkAnchor::Center,
    WatermarkAnchor::Right,
    WatermarkAnchor::BottomLeft,
    WatermarkAnchor::Bottom,
    WatermarkAnchor::BottomRight,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WatermarkAnchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl WatermarkAnchor {
    pub fn key(self) -> &'static str {
        match self {
            WatermarkAnchor::TopLeft => "topLeft",
            WatermarkAnchor::Top => "top",
            WatermarkAnchor::TopRight => "topRight",
            WatermarkAnchor::Left => "left",
            WatermarkAnchor::Center => "center",
            WatermarkAnchor::Right => "right",
            WatermarkAnchor::BottomLeft => "bottomLeft",
            WatermarkAnchor::Bottom => "bottom",
            WatermarkAnchor::BottomRight => "bottomRight",
        }
    }
}

pub const WATERMARK_BLEND_MODES: [WatermarkBlendMode; 8] = [
    WatermarkBlendMode::Normal,
    WatermarkBlendMode::Multiply,
    WatermarkBlendMode::Screen,
    WatermarkBlendMode::Overlay,
    WatermarkBlendMode::Darken,
    WatermarkBlendMode::Lighten,
    WatermarkBlendMode::Difference,
    WatermarkBlendMode::Luminosity,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WatermarkBlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
    Luminosity,
}

impl WatermarkBlendMode {
    pub fn key(self) -> &'static str {
        match self {
            WatermarkBlendMode::Normal => "normal",
            WatermarkBlendMode::Multiply => "multiply",
            WatermarkBlendMode::Screen => "screen",
            WatermarkBlendMode::Overlay => "overlay",
            WatermarkBlendMode::Darken => "darken",
            WatermarkBlendMode::Lighten => "lighten",
            WatermarkBlendMode::Difference => "difference",
            WatermarkBlendMode::Luminosity => "luminosity",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WatermarkTimingMode {
    Always,
    Range,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextStroke {
    pub enabled: bool,
    pub color: String,
    pub width: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextShadow {
    pub enabled: bool,
    pub color: String,
    pub blur: f64,
    pub offset_x: f64,
    pub offset_y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TWatermarkSource {
    #[serde(rename_all = "camelCase")]
    Image { media_id: String },
    #[serde(rename_all = "camelCase")]
    Text {
        text: String,
        color: String,
        font_family: String,
        font_weight: f64,
        stroke: TextStroke,
        shadow: TextShadow,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TWatermarkTiling {
    pub enabled: bool,
    pub spacing: f64,
    pub angle: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TWatermarkTiming {
    pub mode: WatermarkTimingMode,
    pub start: f64,
    pub end: f64,
    pub fade_in: f64,
    pub fade_out: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TWatermark {
    pub enabled: bool,
    pub source: Option<TWatermarkSource>,
    pub anchor: WatermarkAnchor,
    pub offset: Vec2,
    pub size: f64,
    pub opacity: f64,
    pub rotation: f64,
    pub blend_mode: WatermarkBlendMode,
    pub tiling: TWatermarkTiling,
    pub timing: TWatermarkTiming,
}
