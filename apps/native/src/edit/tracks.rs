//! Tracks: what each kind accepts, where an element fits, and how a scene is built.

use super::*;

pub fn track_height(track: &Track) -> f32 {
    match track {
        Track::Video { .. } => 65.0,
        Track::Audio { .. } => 50.0,
        Track::Text { .. } | Track::Graphic { .. } | Track::Effect { .. } => 25.0,
    }
}

pub fn track_can_mute(track: &Track) -> bool {
    matches!(track, Track::Audio { .. } | Track::Video { .. })
}

pub fn track_can_hide(track: &Track) -> bool {
    !matches!(track, Track::Audio { .. })
}

pub fn track_muted(track: &Track) -> bool {
    match track {
        Track::Video { muted, .. } | Track::Audio { muted, .. } => *muted,
        _ => false,
    }
}

pub fn track_hidden(track: &Track) -> bool {
    match track {
        Track::Video { hidden, .. }
        | Track::Text { hidden, .. }
        | Track::Graphic { hidden, .. }
        | Track::Effect { hidden, .. } => *hidden,
        Track::Audio { .. } => false,
    }
}

pub fn accepts(track: &Track, element: &TimelineElement) -> bool {
    matches!(
        (track, element),
        (Track::Audio { .. }, TimelineElement::Audio(_))
            | (Track::Text { .. }, TimelineElement::Text(_))
            | (
                Track::Graphic { .. },
                TimelineElement::Sticker(_) | TimelineElement::Graphic(_)
            )
            | (Track::Effect { .. }, TimelineElement::Effect(_))
            | (
                Track::Video { .. },
                TimelineElement::Video(_) | TimelineElement::Image(_)
            )
    )
}
pub(crate) fn tracks_mut(tracks: &mut SceneTracks) -> impl Iterator<Item = &mut Track> {
    std::iter::once(&mut tracks.main)
        .chain(tracks.overlay.iter_mut())
        .chain(tracks.audio.iter_mut())
}

pub fn track_by_id<'t>(tracks: &'t SceneTracks, id: &str) -> Option<&'t Track> {
    tracks.all().find(|track| track.id() == id)
}

pub(crate) fn track_by_id_mut<'t>(tracks: &'t mut SceneTracks, id: &str) -> Option<&'t mut Track> {
    tracks_mut(tracks).find(|track| track.id() == id)
}

pub(crate) fn locate(tracks: &SceneTracks, element_id: &str) -> Option<(String, usize)> {
    tracks.all().find_map(|track| {
        track
            .elements()
            .iter()
            .position(|element| element.base().id == element_id)
            .map(|index| (track.id().to_string(), index))
    })
}

pub(crate) fn fits(track: &Track, start: MediaTime, end: MediaTime, exclude: Option<&str>) -> bool {
    track.elements().iter().all(|element| {
        if exclude == Some(element.base().id.as_str()) {
            return true;
        }
        start >= element.end_time() || end <= element.base().start_time
    })
}

pub(crate) fn enforce_main_start(
    tracks: &SceneTracks,
    track_id: &str,
    requested: MediaTime,
) -> MediaTime {
    if tracks.main.id() != track_id {
        return requested;
    }
    match tracks
        .main
        .elements()
        .iter()
        .map(|element| element.base().start_time)
        .min()
    {
        None => MediaTime::ZERO,
        Some(earliest) if requested <= earliest => MediaTime::ZERO,
        Some(_) => requested,
    }
}

pub(crate) fn first_available(
    tracks: &SceneTracks,
    element: &TimelineElement,
    start: MediaTime,
) -> Option<String> {
    let end = MediaTime::from_ticks(ticks(start) + ticks(element.base().duration));
    tracks
        .all()
        .find(|track| accepts(track, element) && fits(track, start, end, None))
        .map(|track| track.id().to_string())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackKind {
    Video,
    Text,
    Graphic,
    Effect,
    Audio,
}

impl TrackKind {
    /// Only the tests in this file ask for this; compiled for them alone so the
    /// shipping binary does not carry a method nothing calls.
    #[cfg(test)]
    pub fn glyph(self) -> &'static str {
        match self {
            TrackKind::Video => "video01",
            TrackKind::Text => "text",
            TrackKind::Graphic => "happy01",
            TrackKind::Effect => "magic-wand05",
            TrackKind::Audio => "volume-high",
        }
    }

    pub const ALL: [TrackKind; 5] = [
        TrackKind::Video,
        TrackKind::Text,
        TrackKind::Graphic,
        TrackKind::Effect,
        TrackKind::Audio,
    ];

    pub fn id(self) -> &'static str {
        match self {
            TrackKind::Video => "video",
            TrackKind::Text => "text",
            TrackKind::Graphic => "graphic",
            TrackKind::Effect => "effect",
            TrackKind::Audio => "audio",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            TrackKind::Video => "timeline.track.video",
            TrackKind::Text => "timeline.track.text",
            TrackKind::Graphic => "timeline.track.graphic",
            TrackKind::Effect => "timeline.track.effect",
            TrackKind::Audio => "timeline.track.audio",
        }
    }
}

pub(crate) fn empty_track_of(kind: TrackKind, id: String) -> Track {
    let name = cutix_i18n::t(kind.label_key());
    match kind {
        TrackKind::Video => Track::empty_video(id, name),
        TrackKind::Text => Track::Text {
            id,
            name,
            elements: Vec::new(),
            hidden: false,
        },
        TrackKind::Graphic => Track::Graphic {
            id,
            name,
            elements: Vec::new(),
            hidden: false,
        },
        TrackKind::Effect => Track::Effect {
            id,
            name,
            elements: Vec::new(),
            hidden: false,
        },
        TrackKind::Audio => Track::Audio {
            id,
            name,
            elements: Vec::new(),
            muted: false,
        },
    }
}

pub(crate) fn build_default_scene(id: String, name: String) -> Scene {
    let now = cutix_project::now_iso();
    Scene {
        id,
        name,
        is_main: false,
        tracks: SceneTracks {
            overlay: Vec::new(),
            main: Track::empty_video(new_id(), cutix_i18n::t("timeline.track.main")),
            audio: Vec::new(),
        },
        bookmarks: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
        extra: JsonMap::new(),
    }
}
pub fn text_element(
    name: String,
    content: String,
    patch: crate::text::PresetPatch,
) -> TimelineElement {
    let base = BaseElementFields {
        id: new_id(),
        name,
        duration: seconds(DEFAULT_NEW_ELEMENT_SECONDS),
        start_time: MediaTime::ZERO,
        trim_start: MediaTime::ZERO,
        trim_end: MediaTime::ZERO,
        source_duration: None,
        animations: None,
    };

    TimelineElement::Text(TextElement {
        base,
        content,
        font_size: patch.font_size,
        font_family: patch.font_family,
        color: patch.color,
        background: TextBackground {
            enabled: patch.background_enabled,
            color: patch.background_color,
            corner_radius: Some(patch.corner_radius),
            padding_x: Some(patch.padding_x),
            padding_y: Some(patch.padding_y),
            offset_x: Some(0.0),
            offset_y: Some(0.0),
        },
        stroke: Some(patch.stroke),
        shadow: Some(patch.shadow),
        gradient: Some(patch.gradient),
        text_animations: None,
        text_align: String::from("center"),
        font_weight: patch.font_weight,
        font_style: String::from("normal"),
        text_decoration: String::from("none"),
        letter_spacing: Some(patch.letter_spacing),
        line_height: Some(patch.line_height),
        hidden: None,
        transform: Transform {
            scale_x: 1.0,
            scale_y: 1.0,
            position: Vector2 { x: 0.0, y: 0.0 },
            rotate: 0.0,
        },
        opacity: 1.0,
        blend_mode: None,
        effects: None,
        extra: JsonMap::new(),
    })
}
