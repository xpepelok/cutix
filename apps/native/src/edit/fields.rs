//! The numeric fields of an element: which ones exist, where each lives in the
//! document, and the range each is clamped to.

use super::*;

pub const MIN_TRANSFORM_SCALE: f64 = 0.01;
pub const VOLUME_DB_MIN: f64 = -60.0;
pub const VOLUME_DB_MAX: f64 = 20.0;
pub const MIN_RETIME_RATE: f64 = 0.01;
pub const MAX_RETIME_RATE: f64 = 5.0;
pub const MIN_FONT_SIZE: f64 = 5.0;
pub const MAX_FONT_SIZE: f64 = 300.0;
pub const DEFAULT_TEXT_FONT_SIZE: f64 = 15.0;
pub const DEFAULT_TEXT_LINE_HEIGHT: f64 = 1.2;
pub const DEFAULT_TEXT_PADDING_X: f64 = 30.0;
pub const DEFAULT_TEXT_PADDING_Y: f64 = 42.0;
pub const CORNER_RADIUS_MAX: f64 = 100.0;
pub const MIN_CROP_SPAN: f64 = 0.02;

pub(crate) const FADE_IN_START: &str = "audio-fade-in-start";
pub(crate) const FADE_IN_END: &str = "audio-fade-in-end";
pub(crate) const FADE_OUT_START: &str = "audio-fade-out-start";
pub(crate) const FADE_OUT_END: &str = "audio-fade-out-end";

#[derive(Clone, Copy, Debug)]
pub enum Field {
    PositionX,
    PositionY,
    ScaleX,
    ScaleY,
    Rotate,
    Opacity,
    CropLeft,
    CropTop,
    CropRight,
    CropBottom,
    Volume,
    SpeedRate,
    FontSize,
    LetterSpacing,
    LineHeight,
    BackgroundPaddingX,
    BackgroundPaddingY,
    BackgroundOffsetX,
    BackgroundOffsetY,
    BackgroundCornerRadius,
    StrokeWidth,
    ShadowBlur,
    ShadowOffsetX,
    ShadowOffsetY,
    GraphicParam(&'static stickers::ParamDefinition),
}

pub(crate) const GRAPHIC_PARAM_PATHS: &[(&str, &str)] = &[
    ("strokeWidth", "params.strokeWidth"),
    ("cornerRadius", "params.cornerRadius"),
    ("sides", "params.sides"),
    ("points", "params.points"),
    ("depth", "params.depth"),
];

pub fn graphic_param_path(key: &str) -> Option<&'static str> {
    GRAPHIC_PARAM_PATHS
        .iter()
        .find(|(name, _)| *name == key)
        .map(|(_, path)| *path)
}

pub fn graphic_of(element: &TimelineElement) -> Option<&cutix_project::GraphicElement> {
    match element {
        TimelineElement::Graphic(graphic) => Some(graphic),
        _ => None,
    }
}

pub(crate) fn graphic_mut(
    element: &mut TimelineElement,
) -> Option<&mut cutix_project::GraphicElement> {
    match element {
        TimelineElement::Graphic(graphic) => Some(graphic),
        _ => None,
    }
}

pub fn graphic_param_number(element: &TimelineElement, param: &stickers::ParamDefinition) -> f64 {
    graphic_of(element)
        .and_then(|graphic| graphic.params.get(param.key))
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(param.default_number)
}

pub fn graphic_param_text(element: &TimelineElement, param: &stickers::ParamDefinition) -> String {
    let fallback = match param.kind {
        stickers::ParamKind::Color => param.default_color,
        _ => param.default_select,
    };
    graphic_of(element)
        .and_then(|graphic| graphic.params.get(param.key))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(fallback)
        .to_owned()
}

impl PartialEq for Field {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Field::GraphicParam(left), Field::GraphicParam(right)) => left.key == right.key,
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}

impl Eq for Field {}

impl Field {
    pub fn path(self) -> Option<&'static str> {
        match self {
            Field::PositionX => Some("transform.positionX"),
            Field::PositionY => Some("transform.positionY"),
            Field::ScaleX => Some("transform.scaleX"),
            Field::ScaleY => Some("transform.scaleY"),
            Field::Rotate => Some("transform.rotate"),
            Field::Opacity => Some("opacity"),
            Field::CropLeft => Some("crop.left"),
            Field::CropTop => Some("crop.top"),
            Field::CropRight => Some("crop.right"),
            Field::CropBottom => Some("crop.bottom"),
            Field::Volume => Some("volume"),
            Field::BackgroundPaddingX => Some("background.paddingX"),
            Field::BackgroundPaddingY => Some("background.paddingY"),
            Field::BackgroundOffsetX => Some("background.offsetX"),
            Field::BackgroundOffsetY => Some("background.offsetY"),
            Field::BackgroundCornerRadius => Some("background.cornerRadius"),
            Field::SpeedRate
            | Field::FontSize
            | Field::LetterSpacing
            | Field::LineHeight
            | Field::StrokeWidth
            | Field::ShadowBlur
            | Field::ShadowOffsetX
            | Field::ShadowOffsetY => None,
            Field::GraphicParam(param) => graphic_param_path(param.key),
        }
    }

    pub fn default_value(self) -> f64 {
        match self {
            Field::GraphicParam(param) => param.default_number,
            Field::ScaleX | Field::ScaleY | Field::Opacity | Field::SpeedRate => 1.0,
            Field::FontSize => DEFAULT_TEXT_FONT_SIZE,
            Field::LineHeight => DEFAULT_TEXT_LINE_HEIGHT,
            Field::BackgroundPaddingX => DEFAULT_TEXT_PADDING_X,
            Field::BackgroundPaddingY => DEFAULT_TEXT_PADDING_Y,
            _ => 0.0,
        }
    }

    pub fn range(self) -> (Option<f64>, Option<f64>) {
        match self {
            Field::GraphicParam(param) => (Some(param.min), Some(param.max)),
            Field::ScaleX | Field::ScaleY => (Some(MIN_TRANSFORM_SCALE), None),
            Field::Rotate => (Some(-360.0), Some(360.0)),
            Field::Opacity => (Some(0.0), Some(1.0)),
            Field::CropLeft | Field::CropTop | Field::CropRight | Field::CropBottom => {
                (Some(0.0), Some(1.0 - MIN_CROP_SPAN))
            }
            Field::Volume => (Some(VOLUME_DB_MIN), Some(VOLUME_DB_MAX)),
            Field::SpeedRate => (Some(MIN_RETIME_RATE), Some(MAX_RETIME_RATE)),
            Field::FontSize => (Some(MIN_FONT_SIZE), Some(MAX_FONT_SIZE)),
            Field::LineHeight => (Some(0.1), None),
            Field::BackgroundCornerRadius => (Some(0.0), Some(CORNER_RADIUS_MAX)),
            Field::StrokeWidth | Field::ShadowBlur => (Some(0.0), None),
            _ => (None, None),
        }
    }

    #[allow(dead_code)]
    pub fn step(self) -> f64 {
        match self {
            Field::GraphicParam(param) => param.step,
            Field::ScaleX | Field::ScaleY | Field::Opacity => 0.01,
            Field::CropLeft | Field::CropTop | Field::CropRight | Field::CropBottom => 0.001,
            Field::Volume | Field::SpeedRate => 0.1,
            Field::LineHeight => 0.1,
            _ => 1.0,
        }
    }
}

pub fn transform_of(element: &TimelineElement) -> Option<&Transform> {
    match element {
        TimelineElement::Video(inner) => Some(&inner.transform),
        TimelineElement::Image(inner) => Some(&inner.transform),
        TimelineElement::Text(inner) => Some(&inner.transform),
        TimelineElement::Sticker(inner) => Some(&inner.transform),
        TimelineElement::Graphic(inner) => Some(&inner.transform),
        _ => None,
    }
}

pub(crate) fn transform_mut(element: &mut TimelineElement) -> Option<&mut Transform> {
    match element {
        TimelineElement::Video(inner) => Some(&mut inner.transform),
        TimelineElement::Image(inner) => Some(&mut inner.transform),
        TimelineElement::Text(inner) => Some(&mut inner.transform),
        TimelineElement::Sticker(inner) => Some(&mut inner.transform),
        TimelineElement::Graphic(inner) => Some(&mut inner.transform),
        _ => None,
    }
}

pub fn opacity_of(element: &TimelineElement) -> Option<f64> {
    match element {
        TimelineElement::Video(inner) => Some(inner.opacity),
        TimelineElement::Image(inner) => Some(inner.opacity),
        TimelineElement::Text(inner) => Some(inner.opacity),
        TimelineElement::Sticker(inner) => Some(inner.opacity),
        TimelineElement::Graphic(inner) => Some(inner.opacity),
        _ => None,
    }
}

pub(crate) fn opacity_mut(element: &mut TimelineElement) -> Option<&mut f64> {
    match element {
        TimelineElement::Video(inner) => Some(&mut inner.opacity),
        TimelineElement::Image(inner) => Some(&mut inner.opacity),
        TimelineElement::Text(inner) => Some(&mut inner.opacity),
        TimelineElement::Sticker(inner) => Some(&mut inner.opacity),
        TimelineElement::Graphic(inner) => Some(&mut inner.opacity),
        _ => None,
    }
}

pub fn crop_of(element: &TimelineElement) -> Crop {
    let crop = match element {
        TimelineElement::Video(inner) => inner.crop,
        TimelineElement::Image(inner) => inner.crop,
        TimelineElement::Sticker(inner) => inner.crop,
        TimelineElement::Graphic(inner) => inner.crop,
        _ => None,
    };
    crop.unwrap_or_default()
}

pub(crate) fn set_crop(element: &mut TimelineElement, crop: Crop) {
    let slot = match element {
        TimelineElement::Video(inner) => &mut inner.crop,
        TimelineElement::Image(inner) => &mut inner.crop,
        TimelineElement::Sticker(inner) => &mut inner.crop,
        TimelineElement::Graphic(inner) => &mut inner.crop,
        _ => return,
    };
    *slot = Some(crop);
}

pub fn blend_mode_of(element: &TimelineElement) -> &str {
    let mode = match element {
        TimelineElement::Video(inner) => inner.blend_mode.as_deref(),
        TimelineElement::Image(inner) => inner.blend_mode.as_deref(),
        TimelineElement::Text(inner) => inner.blend_mode.as_deref(),
        TimelineElement::Sticker(inner) => inner.blend_mode.as_deref(),
        TimelineElement::Graphic(inner) => inner.blend_mode.as_deref(),
        _ => None,
    };
    mode.unwrap_or("normal")
}

pub(crate) fn set_blend_mode(element: &mut TimelineElement, value: String) {
    let slot = match element {
        TimelineElement::Video(inner) => &mut inner.blend_mode,
        TimelineElement::Image(inner) => &mut inner.blend_mode,
        TimelineElement::Text(inner) => &mut inner.blend_mode,
        TimelineElement::Sticker(inner) => &mut inner.blend_mode,
        TimelineElement::Graphic(inner) => &mut inner.blend_mode,
        _ => return,
    };
    *slot = Some(value);
}

pub fn volume_of(element: &TimelineElement) -> Option<f64> {
    match element {
        TimelineElement::Audio(inner) => Some(inner.volume),
        TimelineElement::Video(inner) => Some(inner.volume.unwrap_or(0.0)),
        _ => None,
    }
}

pub fn retime_of(element: &TimelineElement) -> Option<&RetimeConfig> {
    match element {
        TimelineElement::Audio(inner) => inner.retime.as_ref(),
        TimelineElement::Video(inner) => inner.retime.as_ref(),
        _ => None,
    }
}

pub(crate) fn set_retime(element: &mut TimelineElement, retime: Option<RetimeConfig>) {
    match element {
        TimelineElement::Audio(inner) => inner.retime = retime,
        TimelineElement::Video(inner) => inner.retime = retime,
        _ => {}
    }
}

pub(crate) fn nested(value: &Option<Value>, key: &str, fallback: f64) -> f64 {
    value
        .as_ref()
        .and_then(|value| value.get(key))
        .and_then(Value::as_f64)
        .unwrap_or(fallback)
}

pub fn nested_flag(value: &Option<Value>, key: &str) -> bool {
    value
        .as_ref()
        .and_then(|value| value.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub fn nested_color(value: &Option<Value>, key: &str, fallback: &str) -> String {
    value
        .as_ref()
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .unwrap_or(fallback)
        .to_string()
}

pub(crate) fn set_nested(slot: &mut Option<Value>, defaults: Value, key: &str, value: Value) {
    let mut current = slot.take().unwrap_or(defaults);
    if let Some(object) = current.as_object_mut() {
        object.insert(key.to_string(), value);
    }
    *slot = Some(current);
}

pub fn default_stroke() -> Value {
    json!({ "enabled": false, "color": "#000000", "width": 0.0 })
}

pub fn default_shadow() -> Value {
    json!({ "enabled": false, "color": "#000000", "blur": 0.0, "offsetX": 0.0, "offsetY": 0.0 })
}

pub fn field_value(element: &TimelineElement, field: Field) -> f64 {
    match field {
        Field::GraphicParam(param) => graphic_param_number(element, param),
        Field::PositionX => transform_of(element).map_or(0.0, |t| t.position.x),
        Field::PositionY => transform_of(element).map_or(0.0, |t| t.position.y),
        Field::ScaleX => transform_of(element).map_or(1.0, |t| t.scale_x),
        Field::ScaleY => transform_of(element).map_or(1.0, |t| t.scale_y),
        Field::Rotate => transform_of(element).map_or(0.0, |t| t.rotate),
        Field::Opacity => opacity_of(element).unwrap_or(1.0),
        Field::CropLeft => crop_of(element).left,
        Field::CropTop => crop_of(element).top,
        Field::CropRight => crop_of(element).right,
        Field::CropBottom => crop_of(element).bottom,
        Field::Volume => volume_of(element).unwrap_or(0.0),
        Field::SpeedRate => retime_of(element).map_or(1.0, |retime| retime.rate),
        Field::FontSize => text_of(element).map_or(DEFAULT_TEXT_FONT_SIZE, |text| text.font_size),
        Field::LetterSpacing => text_of(element)
            .and_then(|text| text.letter_spacing)
            .unwrap_or(0.0),
        Field::LineHeight => text_of(element)
            .and_then(|text| text.line_height)
            .unwrap_or(DEFAULT_TEXT_LINE_HEIGHT),
        Field::BackgroundPaddingX => text_of(element)
            .and_then(|text| text.background.padding_x)
            .unwrap_or(DEFAULT_TEXT_PADDING_X),
        Field::BackgroundPaddingY => text_of(element)
            .and_then(|text| text.background.padding_y)
            .unwrap_or(DEFAULT_TEXT_PADDING_Y),
        Field::BackgroundOffsetX => text_of(element)
            .and_then(|text| text.background.offset_x)
            .unwrap_or(0.0),
        Field::BackgroundOffsetY => text_of(element)
            .and_then(|text| text.background.offset_y)
            .unwrap_or(0.0),
        Field::BackgroundCornerRadius => text_of(element)
            .and_then(|text| text.background.corner_radius)
            .unwrap_or(0.0),
        Field::StrokeWidth => {
            text_of(element).map_or(0.0, |text| nested(&text.stroke, "width", 0.0))
        }
        Field::ShadowBlur => text_of(element).map_or(0.0, |text| nested(&text.shadow, "blur", 0.0)),
        Field::ShadowOffsetX => {
            text_of(element).map_or(0.0, |text| nested(&text.shadow, "offsetX", 0.0))
        }
        Field::ShadowOffsetY => {
            text_of(element).map_or(0.0, |text| nested(&text.shadow, "offsetY", 0.0))
        }
    }
}
