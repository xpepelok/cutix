use crate::types::{
    TWatermark, TWatermarkSource, TWatermarkTiling, TWatermarkTiming, TextShadow, TextStroke, Vec2,
    WatermarkAnchor, WatermarkBlendMode, WatermarkTimingMode,
};

pub const DEFAULT_WATERMARK_SIZE: f64 = 0.18;
pub const DEFAULT_WATERMARK_OPACITY: f64 = 0.7;
pub const DEFAULT_WATERMARK_MARGIN_RATIO: f64 = 0.03;
pub const DEFAULT_WATERMARK_TEXT_COLOR: &str = "#ffffff";
pub const DEFAULT_WATERMARK_FONT_FAMILY: &str = "Arial";
pub const DEFAULT_WATERMARK_FONT_WEIGHT: f64 = 600.0;
pub const DEFAULT_WATERMARK_TILE_SPACING: f64 = 0.6;

pub const MIN_WATERMARK_SIZE: f64 = 0.01;
pub const MAX_WATERMARK_SIZE: f64 = 1.0;
pub const MIN_WATERMARK_TILE_SPACING: f64 = 0.05;
pub const MAX_WATERMARK_TILE_SPACING: f64 = 4.0;
pub const MAX_WATERMARK_TILES: usize = 400;

pub fn default_text_stroke() -> TextStroke {
    TextStroke {
        enabled: false,
        color: "#000000".to_string(),
        width: 0.0,
    }
}

pub fn default_text_shadow() -> TextShadow {
    TextShadow {
        enabled: false,
        color: "#000000".to_string(),
        blur: 0.0,
        offset_x: 0.0,
        offset_y: 0.0,
    }
}

pub fn default_watermark_tiling() -> TWatermarkTiling {
    TWatermarkTiling {
        enabled: false,
        spacing: DEFAULT_WATERMARK_TILE_SPACING,
        angle: 0.0,
    }
}

pub fn default_watermark_timing() -> TWatermarkTiming {
    TWatermarkTiming {
        mode: WatermarkTimingMode::Always,
        start: 0.0,
        end: 0.0,
        fade_in: 0.0,
        fade_out: 0.0,
    }
}

pub fn create_default_watermark_text_source(text: &str) -> TWatermarkSource {
    TWatermarkSource::Text {
        text: text.to_string(),
        color: DEFAULT_WATERMARK_TEXT_COLOR.to_string(),
        font_family: DEFAULT_WATERMARK_FONT_FAMILY.to_string(),
        font_weight: DEFAULT_WATERMARK_FONT_WEIGHT,
        stroke: default_text_stroke(),
        shadow: default_text_shadow(),
    }
}

pub fn create_default_watermark() -> TWatermark {
    TWatermark {
        enabled: false,
        source: None,
        anchor: WatermarkAnchor::BottomRight,
        offset: Vec2 {
            x: DEFAULT_WATERMARK_MARGIN_RATIO,
            y: DEFAULT_WATERMARK_MARGIN_RATIO,
        },
        size: DEFAULT_WATERMARK_SIZE,
        opacity: DEFAULT_WATERMARK_OPACITY,
        rotation: 0.0,
        blend_mode: WatermarkBlendMode::Normal,
        tiling: default_watermark_tiling(),
        timing: default_watermark_timing(),
    }
}
