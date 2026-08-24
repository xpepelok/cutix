//! Non-numeric element settings, and the keyframe channels a setting writes into.

use super::*;

#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub enum Setting {
    BlendMode(String),
    Content(String),
    FontFamily(String),
    FontWeight(String),
    FontStyle(String),
    TextAlign(String),
    TextDecoration(String),
    TextColor(String),
    BackgroundColor(String),
    BackgroundEnabled(bool),
    StrokeEnabled(bool),
    StrokeColor(String),
    ShadowEnabled(bool),
    ShadowColor(String),
    MaintainPitch(bool),
    BlendFrames(bool),
    Muted(bool),
    Hidden(bool),
    Crop(Crop),
    GraphicParamText(&'static str, String),
}

pub(crate) fn apply_setting(element: &mut TimelineElement, setting: Setting) {
    match setting {
        Setting::BlendMode(value) => set_blend_mode(element, value),
        Setting::Crop(crop) => set_crop(element, crop),
        Setting::GraphicParamText(key, value) => {
            if let Some(graphic) = graphic_mut(element) {
                graphic
                    .params
                    .insert(key.to_owned(), serde_json::Value::String(value));
            }
        }
        Setting::Muted(value) => match element {
            TimelineElement::Audio(inner) => inner.muted = Some(value),
            TimelineElement::Video(inner) => inner.muted = Some(value),
            _ => {}
        },
        Setting::Hidden(value) => match element {
            TimelineElement::Video(inner) => inner.hidden = Some(value),
            TimelineElement::Image(inner) => inner.hidden = Some(value),
            TimelineElement::Text(inner) => inner.hidden = Some(value),
            TimelineElement::Sticker(inner) => inner.hidden = Some(value),
            TimelineElement::Graphic(inner) => inner.hidden = Some(value),
            _ => {}
        },
        Setting::MaintainPitch(value) | Setting::BlendFrames(value) => {
            let rate = retime_of(element).map_or(1.0, |retime| retime.rate);
            let existing = retime_of(element).cloned();
            let mut maintain_pitch = existing.as_ref().and_then(|retime| retime.maintain_pitch);
            let mut blend_frames = existing.as_ref().and_then(|retime| retime.blend_frames);
            if matches!(setting, Setting::MaintainPitch(_)) {
                maintain_pitch = Some(value);
            } else {
                blend_frames = Some(value);
            }
            set_retime(
                element,
                Some(RetimeConfig {
                    rate,
                    maintain_pitch,
                    curve: None,
                    blend_frames,
                }),
            );
        }
        other => {
            let Some(text) = text_mut(element) else {
                return;
            };
            match other {
                Setting::Content(value) => text.content = value,
                Setting::FontFamily(value) => text.font_family = value,
                Setting::FontWeight(value) => text.font_weight = value,
                Setting::FontStyle(value) => text.font_style = value,
                Setting::TextAlign(value) => text.text_align = value,
                Setting::TextDecoration(value) => text.text_decoration = value,
                Setting::TextColor(value) => text.color = value,
                Setting::BackgroundColor(value) => text.background.color = value,
                Setting::BackgroundEnabled(value) => text.background.enabled = value,
                Setting::StrokeEnabled(value) => {
                    set_nested(&mut text.stroke, default_stroke(), "enabled", json!(value))
                }
                Setting::StrokeColor(value) => {
                    set_nested(&mut text.stroke, default_stroke(), "color", json!(value))
                }
                Setting::ShadowEnabled(value) => {
                    set_nested(&mut text.shadow, default_shadow(), "enabled", json!(value))
                }
                Setting::ShadowColor(value) => {
                    set_nested(&mut text.shadow, default_shadow(), "color", json!(value))
                }
                _ => {}
            }
        }
    }
}

pub(crate) fn channel_id(path: &str, component: &str) -> String {
    format!("{path}:{component}")
}

pub(crate) fn ensure_binding(animations: &mut ElementAnimations, path: &str, kind: &str) {
    let components: Vec<&str> = if kind == "color" {
        vec!["r", "g", "b", "a"]
    } else {
        vec!["value"]
    };
    let binding = json!({
        "path": path,
        "kind": kind,
        "components": components
            .iter()
            .map(|component| json!({ "key": component, "channelId": channel_id(path, component) }))
            .collect::<Vec<_>>(),
    });
    animations.bindings.insert(path.to_string(), binding);
}

pub(crate) fn upsert_scalar(
    animations: &mut ElementAnimations,
    channel: String,
    time: MediaTime,
    value: f64,
    key_id: Option<&str>,
) {
    let entry = animations
        .channels
        .entry(channel)
        .or_insert_with(|| AnimationChannel::Scalar {
            keys: Vec::new(),
            extrapolation: None,
        });
    let AnimationChannel::Scalar { keys, .. } = entry else {
        return;
    };
    let existing = key_id
        .and_then(|id| keys.iter().position(|key| key.id == id))
        .or_else(|| keys.iter().position(|key| key.time == time));
    match existing {
        Some(index) => {
            keys[index].value = value;
            keys[index].time = time;
        }
        None => keys.push(ScalarAnimationKey {
            id: key_id.map(str::to_string).unwrap_or_else(new_id),
            time,
            value,
            left_handle: None,
            right_handle: None,
            segment_to_next: String::from("linear"),
            tangent_mode: String::from("flat"),
        }),
    }
    keys.sort_by_key(|key| key.time.as_ticks());
}

pub(crate) fn remove_scalar(
    animations: &mut ElementAnimations,
    channel: &str,
    time: MediaTime,
) -> bool {
    let Some(AnimationChannel::Scalar { keys, .. }) = animations.channels.get_mut(channel) else {
        return false;
    };
    let before = keys.len();
    keys.retain(|key| key.time != time);
    before != keys.len()
}

pub(crate) fn remove_scalar_by_id(
    animations: &mut ElementAnimations,
    channel: &str,
    id: &str,
) -> bool {
    let Some(AnimationChannel::Scalar { keys, .. }) = animations.channels.get_mut(channel) else {
        return false;
    };
    let before = keys.len();
    keys.retain(|key| key.id != id);
    before != keys.len()
}

pub(crate) fn prune_empty_channel(animations: &mut ElementAnimations, path: &str, channel: &str) {
    let empty = matches!(
        animations.channels.get(channel),
        Some(AnimationChannel::Scalar { keys, .. }) if keys.is_empty()
    );
    if !empty {
        return;
    }
    animations.channels.remove(channel);
    animations.bindings.remove(path);
}
