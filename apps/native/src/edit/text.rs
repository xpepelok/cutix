//! Text elements: their content, their styling, and the animation settings that
//! drive a reveal.

use super::*;

pub fn text_of(element: &TimelineElement) -> Option<&TextElement> {
    match element {
        TimelineElement::Text(text) => Some(text),
        _ => None,
    }
}

pub fn text_animation_settings(element: &TimelineElement) -> JsonMap {
    let TimelineElement::Text(text) = element else {
        return JsonMap::new();
    };
    text.text_animations
        .as_ref()
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn set_text_animation_settings(element: &mut TimelineElement, settings: JsonMap) {
    let Some(text) = text_mut(element) else {
        return;
    };
    text.text_animations = if settings.is_empty() {
        None
    } else {
        Some(Value::Object(settings))
    };
}

pub fn text_animation_preset(element: &TimelineElement, key: &str) -> Option<String> {
    text_animation_settings(element)
        .get(key)
        .and_then(|entry| entry.get("presetId"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(crate) fn text_mut(element: &mut TimelineElement) -> Option<&mut TextElement> {
    match element {
        TimelineElement::Text(text) => Some(text),
        _ => None,
    }
}

pub(crate) fn clamp_field(field: Field, value: f64) -> f64 {
    let (min, max) = field.range();
    let mut value = value;
    if let Some(min) = min {
        value = value.max(min);
    }
    if let Some(max) = max {
        value = value.min(max);
    }
    value
}

pub(crate) fn set_field(element: &mut TimelineElement, field: Field, value: f64) {
    let value = clamp_field(field, value);
    match field {
        Field::GraphicParam(param) => {
            let value = if param.step >= 1.0 {
                value.round()
            } else {
                value
            };
            if let Some(graphic) = graphic_mut(element) {
                graphic
                    .params
                    .insert(param.key.to_owned(), serde_json::Value::from(value));
            }
        }
        Field::PositionX => {
            if let Some(transform) = transform_mut(element) {
                transform.position.x = value;
            }
        }
        Field::PositionY => {
            if let Some(transform) = transform_mut(element) {
                transform.position.y = value;
            }
        }
        Field::ScaleX => {
            if let Some(transform) = transform_mut(element) {
                transform.scale_x = value;
            }
        }
        Field::ScaleY => {
            if let Some(transform) = transform_mut(element) {
                transform.scale_y = value;
            }
        }
        Field::Rotate => {
            if let Some(transform) = transform_mut(element) {
                transform.rotate = value;
            }
        }
        Field::Opacity => {
            if let Some(opacity) = opacity_mut(element) {
                *opacity = value;
            }
        }
        Field::CropLeft | Field::CropTop | Field::CropRight | Field::CropBottom => {
            let mut crop = crop_of(element);
            match field {
                Field::CropLeft => crop.left = value.min(1.0 - MIN_CROP_SPAN - crop.right),
                Field::CropTop => crop.top = value.min(1.0 - MIN_CROP_SPAN - crop.bottom),
                Field::CropRight => crop.right = value.min(1.0 - MIN_CROP_SPAN - crop.left),
                _ => crop.bottom = value.min(1.0 - MIN_CROP_SPAN - crop.top),
            }
            set_crop(element, crop);
        }
        Field::Volume => match element {
            TimelineElement::Audio(inner) => inner.volume = value,
            TimelineElement::Video(inner) => inner.volume = Some(value),
            _ => {}
        },
        Field::SpeedRate => {
            let existing = retime_of(element).cloned();
            let maintain_pitch = existing.as_ref().and_then(|retime| retime.maintain_pitch);
            let blend_frames = existing.as_ref().and_then(|retime| retime.blend_frames);
            if (value - 1.0).abs() < f64::EPSILON
                && maintain_pitch != Some(true)
                && blend_frames != Some(true)
            {
                set_retime(element, None);
            } else {
                set_retime(
                    element,
                    Some(RetimeConfig {
                        rate: value,
                        maintain_pitch,
                        curve: None,
                        blend_frames,
                    }),
                );
            }
        }
        Field::FontSize => {
            if let Some(text) = text_mut(element) {
                text.font_size = value.round();
            }
        }
        Field::LetterSpacing => {
            if let Some(text) = text_mut(element) {
                text.letter_spacing = Some(value.round());
            }
        }
        Field::LineHeight => {
            if let Some(text) = text_mut(element) {
                text.line_height = Some((value * 10.0).round() / 10.0);
            }
        }
        Field::BackgroundPaddingX => {
            if let Some(text) = text_mut(element) {
                text.background.padding_x = Some(value);
            }
        }
        Field::BackgroundPaddingY => {
            if let Some(text) = text_mut(element) {
                text.background.padding_y = Some(value);
            }
        }
        Field::BackgroundOffsetX => {
            if let Some(text) = text_mut(element) {
                text.background.offset_x = Some(value);
            }
        }
        Field::BackgroundOffsetY => {
            if let Some(text) = text_mut(element) {
                text.background.offset_y = Some(value);
            }
        }
        Field::BackgroundCornerRadius => {
            if let Some(text) = text_mut(element) {
                text.background.corner_radius = Some(value);
            }
        }
        Field::StrokeWidth => {
            if let Some(text) = text_mut(element) {
                set_nested(&mut text.stroke, default_stroke(), "width", json!(value));
            }
        }
        Field::ShadowBlur => {
            if let Some(text) = text_mut(element) {
                set_nested(&mut text.shadow, default_shadow(), "blur", json!(value));
            }
        }
        Field::ShadowOffsetX => {
            if let Some(text) = text_mut(element) {
                set_nested(&mut text.shadow, default_shadow(), "offsetX", json!(value));
            }
        }
        Field::ShadowOffsetY => {
            if let Some(text) = text_mut(element) {
                set_nested(&mut text.shadow, default_shadow(), "offsetY", json!(value));
            }
        }
    }
}
