//! Element-level state that is not a field: cutouts, motion, masks, keyframe
//! presence, and building a new element from a media asset.

use super::*;

pub(crate) fn cutout_mut(element: &mut TimelineElement) -> Option<&mut Option<Value>> {
    match element {
        TimelineElement::Video(inner) => Some(&mut inner.cutout),
        TimelineElement::Image(inner) => Some(&mut inner.cutout),
        _ => None,
    }
}

pub fn cutout_of(element: &TimelineElement) -> Option<ml::ElementCutout> {
    let raw = match element {
        TimelineElement::Video(inner) => inner.cutout.as_ref(),
        TimelineElement::Image(inner) => inner.cutout.as_ref(),
        _ => None,
    }?;
    serde_json::from_value(raw.clone()).ok()
}

pub(crate) fn motion_mut(element: &mut TimelineElement) -> Option<&mut Option<MotionSettings>> {
    match element {
        TimelineElement::Video(inner) => Some(&mut inner.motion),
        TimelineElement::Image(inner) => Some(&mut inner.motion),
        TimelineElement::Sticker(inner) => Some(&mut inner.motion),
        _ => None,
    }
}

pub fn motion_of(element: &TimelineElement) -> Option<&MotionSettings> {
    match element {
        TimelineElement::Video(inner) => inner.motion.as_ref(),
        TimelineElement::Image(inner) => inner.motion.as_ref(),
        TimelineElement::Sticker(inner) => inner.motion.as_ref(),
        _ => None,
    }
}

pub(crate) fn masks_mut(element: &mut TimelineElement) -> Option<&mut Option<Vec<Mask>>> {
    match element {
        TimelineElement::Video(inner) => Some(&mut inner.masks),
        TimelineElement::Image(inner) => Some(&mut inner.masks),
        TimelineElement::Graphic(inner) => Some(&mut inner.masks),
        _ => None,
    }
}

pub fn mask_of(element: &TimelineElement) -> Option<&Mask> {
    let list = match element {
        TimelineElement::Video(inner) => inner.masks.as_ref(),
        TimelineElement::Image(inner) => inner.masks.as_ref(),
        TimelineElement::Graphic(inner) => inner.masks.as_ref(),
        _ => None,
    };
    list.and_then(|masks| masks.first())
}

pub(crate) fn animations_mut(element: &mut TimelineElement) -> &mut ElementAnimations {
    let base = element_base_mut(element);
    base.animations
        .get_or_insert_with(ElementAnimations::default)
}

pub fn has_key_at(element: &TimelineElement, path: &str, time: MediaTime) -> bool {
    let Some(animations) = element.base().animations.as_ref() else {
        return false;
    };
    let channel = animations
        .channels
        .get(&channel_id(path, "value"))
        .or_else(|| animations.channels.get(path));
    let Some(AnimationChannel::Scalar { keys, .. }) = channel else {
        return false;
    };
    keys.iter().any(|key| key.time == time)
}

pub fn has_color_key_at(element: &TimelineElement, path: &str, time: MediaTime) -> bool {
    let Some(animations) = element.base().animations.as_ref() else {
        return false;
    };
    let Some(AnimationChannel::Scalar { keys, .. }) =
        animations.channels.get(&channel_id(path, "r"))
    else {
        return false;
    };
    keys.iter().any(|key| key.time == time)
}

pub fn is_animated(element: &TimelineElement, path: &str) -> bool {
    let Some(animations) = element.base().animations.as_ref() else {
        return false;
    };
    ["value", "r"].iter().any(|component| {
        matches!(
            animations.channels.get(&channel_id(path, component)),
            Some(AnimationChannel::Scalar { keys, .. }) if !keys.is_empty()
        )
    })
}

pub fn local_time(element: &TimelineElement, playhead: MediaTime) -> MediaTime {
    let base = element.base();
    MediaTime::from_ticks((ticks(playhead) - ticks(base.start_time)).clamp(0, ticks(base.duration)))
}

pub fn playhead_within(element: &TimelineElement, playhead: MediaTime) -> bool {
    let base = element.base();
    playhead >= base.start_time && playhead <= element.end_time()
}

pub fn element_base_mut(element: &mut TimelineElement) -> &mut BaseElementFields {
    match element {
        TimelineElement::Video(inner) => &mut inner.base,
        TimelineElement::Image(inner) => &mut inner.base,
        TimelineElement::Audio(inner) => &mut inner.base,
        TimelineElement::Text(inner) => &mut inner.base,
        TimelineElement::Sticker(inner) => &mut inner.base,
        TimelineElement::Graphic(inner) => &mut inner.base,
        TimelineElement::Effect(inner) => &mut inner.base,
    }
}

pub(crate) fn place(element: &TimelineElement, start: MediaTime) -> TimelineElement {
    let mut copy = element.clone();
    element_base_mut(&mut copy).start_time = start;
    copy
}
#[derive(Clone, Debug, PartialEq)]
pub enum BookmarkUpdate {
    Note(Option<String>),
}

pub(crate) fn find_bookmark(
    bookmarks: &[cutix_project::model::Bookmark],
    frame_time: MediaTime,
) -> Option<usize> {
    bookmarks
        .iter()
        .position(|bookmark| bookmark.time == frame_time)
}

pub fn element_can_have_audio(element: &TimelineElement) -> bool {
    matches!(
        element,
        TimelineElement::Video(_) | TimelineElement::Audio(_)
    )
}

pub fn element_muted(element: &TimelineElement) -> bool {
    match element {
        TimelineElement::Video(video) => video.muted.unwrap_or(false),
        TimelineElement::Audio(audio) => audio.muted.unwrap_or(false),
        _ => false,
    }
}

pub fn element_can_be_hidden(element: &TimelineElement) -> bool {
    matches!(
        element,
        TimelineElement::Video(_)
            | TimelineElement::Image(_)
            | TimelineElement::Text(_)
            | TimelineElement::Sticker(_)
            | TimelineElement::Graphic(_)
    )
}

pub fn element_hidden(element: &TimelineElement) -> bool {
    match element {
        TimelineElement::Video(video) => video.hidden.unwrap_or(false),
        TimelineElement::Image(image) => image.hidden.unwrap_or(false),
        TimelineElement::Text(text) => text.hidden.unwrap_or(false),
        TimelineElement::Sticker(sticker) => sticker.hidden.unwrap_or(false),
        TimelineElement::Graphic(graphic) => graphic.hidden.unwrap_or(false),
        _ => false,
    }
}

pub fn source_audio_separated(video: &VideoElement) -> bool {
    video.is_source_audio_enabled == Some(false)
}

pub fn can_toggle_source_audio(element: &TimelineElement, has_audio: bool) -> bool {
    match element {
        TimelineElement::Video(video) => source_audio_separated(video) || has_audio,
        _ => false,
    }
}

pub(crate) fn separated_audio_element(video: &VideoElement) -> TimelineElement {
    let mut base = video.base.clone();
    base.id = new_id();
    base.animations = volume_animations_only(video.base.animations.as_ref());
    TimelineElement::Audio(AudioElement {
        base,
        source_type: String::from("media"),
        media_id: Some(video.media_id.clone()),
        source_url: None,
        volume: video.volume.unwrap_or(0.0),
        muted: Some(video.muted.unwrap_or(false)),
        retime: video.retime.clone(),
        extra: JsonMap::new(),
    })
}

pub(crate) fn volume_animations_only(
    animations: Option<&ElementAnimations>,
) -> Option<ElementAnimations> {
    let animations = animations?;
    let binding = animations.bindings.get("volume")?.clone();
    let mut channels = std::collections::BTreeMap::new();
    for component in binding
        .get("components")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(id) = component.get("channelId").and_then(Value::as_str) else {
            continue;
        };
        if let Some(channel) = animations.channels.get(id) {
            channels.insert(id.to_string(), channel.clone());
        }
    }
    if channels.is_empty() {
        return None;
    }
    let mut bindings = std::collections::BTreeMap::new();
    bindings.insert(String::from("volume"), binding);
    Some(ElementAnimations { bindings, channels })
}

pub(crate) fn empty_track_for(element: &TimelineElement) -> Track {
    let id = new_id();
    match element {
        TimelineElement::Audio(_) => Track::Audio {
            id,
            name: cutix_i18n::t("timeline.track.audio"),
            elements: Vec::new(),
            muted: false,
        },
        TimelineElement::Text(_) => Track::Text {
            id,
            name: cutix_i18n::t("timeline.track.text"),
            elements: Vec::new(),
            hidden: false,
        },
        TimelineElement::Sticker(_) | TimelineElement::Graphic(_) => Track::Graphic {
            id,
            name: cutix_i18n::t("timeline.track.graphic"),
            elements: Vec::new(),
            hidden: false,
        },
        TimelineElement::Effect(_) => Track::Effect {
            id,
            name: cutix_i18n::t("timeline.track.effect"),
            elements: Vec::new(),
            hidden: false,
        },
        _ => Track::empty_video(id, cutix_i18n::t("timeline.track.video")),
    }
}

pub(crate) fn insert_track(tracks: &mut SceneTracks, track: Track) {
    match track {
        Track::Audio { .. } => tracks.audio.push(track),
        _ => tracks.overlay.insert(0, track),
    }
}

pub(crate) fn apply_ripple(before: &SceneTracks, after: &mut SceneTracks) {
    let survivors: Vec<String> = after
        .all()
        .flat_map(Track::elements)
        .map(|element| element.base().id.clone())
        .collect();
    let mut adjustments: Vec<(String, MediaTime, MediaTime)> = Vec::new();

    for old_track in before.all() {
        let Some(new_track) = track_by_id(after, old_track.id()) else {
            continue;
        };
        for element in old_track.elements() {
            let id = &element.base().id;
            match new_track
                .elements()
                .iter()
                .find(|candidate| candidate.base().id == *id)
            {
                Some(current) => {
                    let shrink = ticks(element.base().duration) - ticks(current.base().duration);
                    if shrink > 0 {
                        adjustments.push((
                            new_track.id().to_string(),
                            current.end_time(),
                            MediaTime::from_ticks(shrink),
                        ));
                    }
                }
                None if !survivors.contains(id) => adjustments.push((
                    new_track.id().to_string(),
                    element.base().start_time,
                    element.base().duration,
                )),
                None => {}
            }
        }
    }

    for (track_id, after_time, shift) in adjustments {
        let Some(track) = track_by_id_mut(after, &track_id) else {
            continue;
        };
        for element in track.elements_mut() {
            if element.base().start_time >= after_time {
                let fields = element_base_mut(element);
                fields.start_time = sub(fields.start_time, shift).max(MediaTime::ZERO);
            }
        }
    }
}

pub fn element_for(asset: &MediaAssetData) -> TimelineElement {
    let duration = asset
        .duration
        .and_then(MediaTime::from_seconds_f64)
        .filter(|value| ticks(*value) > 0)
        .unwrap_or_else(|| seconds(DEFAULT_NEW_ELEMENT_SECONDS));
    let base = BaseElementFields {
        id: new_id(),
        name: asset.name.clone(),
        duration,
        start_time: MediaTime::ZERO,
        trim_start: MediaTime::ZERO,
        trim_end: MediaTime::ZERO,
        source_duration: matches!(asset.media_type, MediaType::Video | MediaType::Audio)
            .then_some(duration),
        animations: None,
    };

    match asset.media_type {
        MediaType::Audio => TimelineElement::Audio(AudioElement {
            base,
            source_type: String::from("media"),
            media_id: Some(asset.id.clone()),
            source_url: None,
            volume: 0.0,
            muted: None,
            retime: None,
            extra: JsonMap::new(),
        }),
        MediaType::Image => TimelineElement::Image(ImageElement {
            base,
            media_id: asset.id.clone(),
            hidden: None,
            transform: Transform::default(),
            crop: None,
            opacity: 1.0,
            blend_mode: None,
            effects: None,
            masks: None,
            cutout: None,
            transition: None,
            motion: None,
            extra: JsonMap::new(),
        }),
        MediaType::Video => TimelineElement::Video(VideoElement {
            base,
            media_id: asset.id.clone(),
            volume: None,
            muted: None,
            is_source_audio_enabled: None,
            hidden: None,
            retime: None,
            reversed_from: None,
            transform: Transform::default(),
            crop: None,
            opacity: 1.0,
            blend_mode: None,
            effects: None,
            masks: None,
            cutout: None,
            transition: None,
            motion: None,
            extra: JsonMap::new(),
        }),
    }
}

pub fn read_audio_fade(element: &TimelineElement) -> (MediaTime, MediaTime) {
    let Some(animations) = element.base().animations.as_ref() else {
        return (MediaTime::ZERO, MediaTime::ZERO);
    };
    let Some(AnimationChannel::Scalar { keys, .. }) =
        animations.channels.get(&channel_id("volume", "value"))
    else {
        return (MediaTime::ZERO, MediaTime::ZERO);
    };
    let time_of = |id: &str| keys.iter().find(|key| key.id == id).map(|key| key.time);
    let fade_in = match (time_of(FADE_IN_START), time_of(FADE_IN_END)) {
        (Some(_), Some(end)) => end.max(MediaTime::ZERO),
        _ => MediaTime::ZERO,
    };
    let fade_out = match (time_of(FADE_OUT_START), time_of(FADE_OUT_END)) {
        (Some(start), Some(_)) => sub(element.base().duration, start).max(MediaTime::ZERO),
        _ => MediaTime::ZERO,
    };
    (fade_in, fade_out)
}
