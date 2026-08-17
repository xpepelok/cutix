use cutix_project::model::{
    AnimationChannel, BaseElementFields, Crop, Effect, ElementAnimations, ElementTransition,
    JsonMap, Mask, MotionSettings, ParamValues, RetimeConfig, ScalarAnimationKey, TextBackground,
    Transform, Vector2,
};
use cutix_project::{
    AudioElement, ImageElement, MediaAssetData, MediaType, Project, Scene, SceneTracks,
    TextElement, TimelineElement, Track, VideoElement,
};
use serde_json::{json, Value};
use time::MediaTime;

pub const SNAP_THRESHOLD_PX: f32 = 10.0;

pub const DEFAULT_NEW_ELEMENT_SECONDS: f64 = 5.0;

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

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn ticks(time: MediaTime) -> i64 {
    time.as_ticks()
}

fn add(left: MediaTime, right: MediaTime) -> MediaTime {
    MediaTime::from_ticks(ticks(left) + ticks(right))
}

fn sub(left: MediaTime, right: MediaTime) -> MediaTime {
    MediaTime::from_ticks(ticks(left) - ticks(right))
}

pub fn seconds(value: f64) -> MediaTime {
    MediaTime::from_seconds_f64(value).unwrap_or(MediaTime::ZERO)
}

pub fn min_duration(fps: f32) -> MediaTime {
    let fps = if fps > 0.0 { fps as f64 } else { 30.0 };
    MediaTime::from_ticks(((time::TICKS_PER_SECOND as f64) / fps).round() as i64)
}

pub fn snap_to_frame(time: MediaTime, fps: f32) -> MediaTime {
    let step = ticks(min_duration(fps)).max(1);
    let rounded = ((ticks(time) as f64) / step as f64).round() as i64 * step;
    MediaTime::from_ticks(rounded.max(0))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retain {
    Both,
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Start,
    End,
}

#[derive(Clone)]
enum Snapshot {
    Tracks(SceneTracks),
    Scenes { scenes: Vec<Scene>, current: String },
}

struct HistoryEntry {
    label: &'static str,
    snapshot: Snapshot,
    scene_id: String,
    selection: Vec<String>,
    coalesce: Option<String>,
}

pub const MAX_UNDO_DEPTH: usize = 100;

#[derive(Default)]
pub struct History {
    undo_stack: std::collections::VecDeque<HistoryEntry>,
    redo_stack: std::collections::VecDeque<HistoryEntry>,
}

impl History {
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    pub fn depth(&self) -> (usize, usize) {
        (self.undo_stack.len(), self.redo_stack.len())
    }

    fn push(stack: &mut std::collections::VecDeque<HistoryEntry>, entry: HistoryEntry) {
        stack.push_back(entry);
        while stack.len() > MAX_UNDO_DEPTH {
            stack.pop_front();
        }
    }
}

pub struct Editor<'a> {
    pub project: &'a mut Project,
    pub history: &'a mut History,
    pub selection: &'a mut Vec<String>,
    pub ripple: bool,
    pub fps: f32,

    pub coalesce: Option<String>,
}

impl<'a> Editor<'a> {
    fn scene_id(&self) -> Option<String> {
        let project = &self.project;
        project
            .scenes
            .iter()
            .find(|scene| scene.id == project.current_scene_id)
            .or_else(|| project.scenes.first())
            .map(|scene| scene.id.clone())
    }

    fn scene_mut(&mut self) -> Option<&mut Scene> {
        let id = self.scene_id()?;
        self.project.scenes.iter_mut().find(|scene| scene.id == id)
    }

    fn commit<F>(&mut self, label: &'static str, edit: F) -> bool
    where
        F: FnOnce(&mut SceneTracks, &mut Vec<String>) -> bool,
    {
        let Some(scene_id) = self.scene_id() else {
            return false;
        };
        let mut selection = self.selection.clone();
        let ripple = self.ripple;

        let Some(scene) = self.scene_mut() else {
            return false;
        };
        let before = scene.tracks.clone();
        let changed = edit(&mut scene.tracks, &mut selection);
        if changed && ripple {
            apply_ripple(&before, &mut scene.tracks);
        }
        if !changed || scene.tracks == before {
            scene.tracks = before;
            return false;
        }
        scene.updated_at = cutix_project::now_iso();

        let selection_before = std::mem::replace(self.selection, selection);
        self.refresh_duration();

        if let Some(key) = self.coalesce.as_ref() {
            if let Some(entry) = self.history.undo_stack.back() {
                if entry.coalesce.as_deref() == Some(key.as_str()) {
                    self.history.redo_stack.clear();
                    return true;
                }
            }
        }

        History::push(
            &mut self.history.undo_stack,
            HistoryEntry {
                label,
                snapshot: Snapshot::Tracks(before),
                scene_id,
                selection: selection_before,
                coalesce: self.coalesce.clone(),
            },
        );
        self.history.redo_stack.clear();
        true
    }

    fn commit_scenes<F>(&mut self, label: &'static str, edit: F) -> bool
    where
        F: FnOnce(&mut Vec<Scene>, &mut String, &mut Vec<String>) -> bool,
    {
        let scenes_before = self.project.scenes.clone();
        let current_before = self.project.current_scene_id.clone();
        let selection_before = self.selection.clone();

        let mut scenes = scenes_before.clone();
        let mut current = current_before.clone();
        let mut selection = selection_before.clone();
        if !edit(&mut scenes, &mut current, &mut selection) {
            return false;
        }
        if scenes == scenes_before && current == current_before {
            return false;
        }

        self.project.scenes = scenes;
        self.project.current_scene_id = current;
        *self.selection = selection;
        self.refresh_duration();

        History::push(
            &mut self.history.undo_stack,
            HistoryEntry {
                label,
                snapshot: Snapshot::Scenes {
                    scenes: scenes_before,
                    current: current_before,
                },
                scene_id: String::new(),
                selection: selection_before,
                coalesce: None,
            },
        );
        self.history.redo_stack.clear();
        true
    }

    fn step(&mut self, entry: HistoryEntry) -> (&'static str, HistoryEntry) {
        let current = self
            .capture(&entry.scene_id, &entry.snapshot)
            .unwrap_or_else(|| entry.snapshot.clone());
        let selection = std::mem::take(self.selection);
        let label = entry.label;
        let scene_id = entry.scene_id;
        self.restore(&scene_id, entry.snapshot, entry.selection);
        (
            label,
            HistoryEntry {
                label,
                snapshot: current,
                scene_id,
                selection,
                coalesce: None,
            },
        )
    }

    fn capture(&self, scene_id: &str, shape: &Snapshot) -> Option<Snapshot> {
        match shape {
            Snapshot::Tracks(_) => self
                .project
                .scenes
                .iter()
                .find(|scene| scene.id == scene_id)
                .map(|scene| Snapshot::Tracks(scene.tracks.clone())),
            Snapshot::Scenes { .. } => Some(Snapshot::Scenes {
                scenes: self.project.scenes.clone(),
                current: self.project.current_scene_id.clone(),
            }),
        }
    }

    pub fn undo(&mut self) -> Option<&'static str> {
        let entry = self.history.undo_stack.pop_back()?;
        let (label, inverse) = self.step(entry);
        History::push(&mut self.history.redo_stack, inverse);
        Some(label)
    }

    pub fn redo(&mut self) -> Option<&'static str> {
        let entry = self.history.redo_stack.pop_back()?;
        let (label, inverse) = self.step(entry);
        History::push(&mut self.history.undo_stack, inverse);
        Some(label)
    }

    fn restore(&mut self, scene_id: &str, snapshot: Snapshot, selection: Vec<String>) {
        match snapshot {
            Snapshot::Tracks(tracks) => {
                if let Some(scene) = self
                    .project
                    .scenes
                    .iter_mut()
                    .find(|scene| scene.id == scene_id)
                {
                    scene.tracks = tracks;
                    scene.updated_at = cutix_project::now_iso();
                }
            }
            Snapshot::Scenes { scenes, current } => {
                self.project.scenes = scenes;
                self.project.current_scene_id = current;
            }
        }
        *self.selection = selection;
        self.refresh_duration();
    }

    fn refresh_duration(&mut self) {
        let longest = self
            .project
            .scenes
            .iter()
            .flat_map(|scene| scene.tracks.all())
            .flat_map(Track::elements)
            .map(TimelineElement::end_time)
            .max()
            .unwrap_or(MediaTime::ZERO);
        self.project.metadata.duration = longest;
        self.project.metadata.updated_at = cutix_project::now_iso();
    }

    pub fn insert_media(
        &mut self,
        asset: &MediaAssetData,
        start_time: MediaTime,
        track_id: Option<&str>,
    ) -> bool {
        let element = element_for(asset);
        let start = snap_to_frame(start_time, self.fps);
        let track_id = track_id.map(str::to_string);
        self.commit("insert", move |tracks, selection| {
            let element = place(&element, start);
            let id = element.base().id.clone();
            let target = track_id
                .as_deref()
                .filter(|id| {
                    track_by_id(tracks, id).is_some_and(|track| {
                        accepts(track, &element) && fits(track, start, element.end_time(), None)
                    })
                })
                .map(str::to_string)
                .or_else(|| first_available(tracks, &element, start));

            match target {
                Some(id) => {
                    let start = enforce_main_start(tracks, &id, start);
                    let element = place(&element, start);
                    let Some(track) = track_by_id_mut(tracks, &id) else {
                        return false;
                    };
                    track.elements_mut().push(element);
                }
                None => {
                    let mut track = empty_track_for(&element);
                    track.elements_mut().push(place(&element, start));
                    insert_track(tracks, track);
                }
            }
            *selection = vec![id];
            true
        })
    }

    pub fn move_element(
        &mut self,
        element_id: &str,
        target_track_id: &str,
        start_time: MediaTime,
    ) -> bool {
        let element_id = element_id.to_string();
        let target = target_track_id.to_string();
        let start = snap_to_frame(start_time.max(MediaTime::ZERO), self.fps);
        self.commit("move", move |tracks, selection| {
            let Some((source_id, index)) = locate(tracks, &element_id) else {
                return false;
            };
            let element = track_by_id(tracks, &source_id).unwrap().elements()[index].clone();
            let same_track = source_id == target;
            if same_track && element.base().start_time == start {
                return false;
            }
            let Some(destination) = track_by_id(tracks, &target) else {
                return false;
            };
            if !accepts(destination, &element) {
                return false;
            }
            let start = enforce_main_start(tracks, &target, start);
            let moved = place(&element, start);
            let exclude = same_track.then(|| element_id.as_str());
            if !fits(
                track_by_id(tracks, &target).unwrap(),
                start,
                moved.end_time(),
                exclude,
            ) {
                return false;
            }
            track_by_id_mut(tracks, &source_id)
                .unwrap()
                .elements_mut()
                .remove(index);
            track_by_id_mut(tracks, &target)
                .unwrap()
                .elements_mut()
                .push(moved);
            *selection = vec![element_id.clone()];
            true
        })
    }

    pub fn trim_element(&mut self, element_id: &str, edge: Edge, delta: MediaTime) -> bool {
        let element_id = element_id.to_string();
        let minimum = min_duration(self.fps);
        let fps = self.fps;
        self.commit("trim", move |tracks, _| {
            let Some((track_id, index)) = locate(tracks, &element_id) else {
                return false;
            };
            let neighbours: Vec<(MediaTime, MediaTime)> = track_by_id(tracks, &track_id)
                .unwrap()
                .elements()
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index)
                .map(|(_, element)| (element.base().start_time, element.end_time()))
                .collect();
            let track = track_by_id_mut(tracks, &track_id).unwrap();
            let element = &mut track.elements_mut()[index];
            let source_duration = element.base().source_duration;
            let base = element.base().clone();

            let (start_time, duration, trim_start, trim_end) = match edge {
                Edge::Start => {
                    let left_bound = neighbours
                        .iter()
                        .filter(|(_, end)| *end <= base.start_time)
                        .map(|(_, end)| *end)
                        .max()
                        .unwrap_or(MediaTime::ZERO);
                    let requested = snap_to_frame(add(base.start_time, delta), fps);
                    let earliest = left_bound.max(sub(base.start_time, base.trim_start));
                    let latest = sub(add(base.start_time, base.duration), minimum);
                    let start = requested.clamp(earliest, latest.max(earliest));
                    let shift = sub(start, base.start_time);
                    let trim_start = add(base.trim_start, shift);
                    (start, sub(base.duration, shift), trim_start, base.trim_end)
                }
                Edge::End => {
                    let right_bound = neighbours
                        .iter()
                        .filter(|(start, _)| *start >= add(base.start_time, base.duration))
                        .map(|(start, _)| *start)
                        .min();
                    let requested =
                        snap_to_frame(add(add(base.start_time, base.duration), delta), fps);
                    let earliest = add(base.start_time, minimum);
                    let mut end = requested.max(earliest);
                    if let Some(bound) = right_bound {
                        end = end.min(bound.max(earliest));
                    }
                    let growth = sub(end, add(base.start_time, base.duration));
                    let trim_end = sub(base.trim_end, growth);
                    if ticks(trim_end) < 0 && source_duration.is_some() {
                        return false;
                    }
                    (
                        base.start_time,
                        sub(end, base.start_time),
                        base.trim_start,
                        trim_end.max(MediaTime::ZERO),
                    )
                }
            };

            if ticks(duration) < ticks(minimum) {
                return false;
            }
            let fields = element_base_mut(element);
            fields.start_time = start_time;
            fields.duration = duration;
            fields.trim_start = trim_start;
            fields.trim_end = trim_end;
            true
        })
    }

    pub fn split_at(&mut self, time: MediaTime) -> bool {
        self.split_retaining(time, Retain::Both)
    }

    pub fn split_retaining(&mut self, time: MediaTime, retain: Retain) -> bool {
        let time = snap_to_frame(time, self.fps);
        let targets = self.selection.clone();
        self.commit("split", move |tracks, selection| {
            let mut created = Vec::new();
            let mut dropped: Vec<String> = Vec::new();
            for track in tracks_mut(tracks) {
                let mut additions = Vec::new();
                for element in track.elements_mut().iter_mut() {
                    let base = element.base().clone();
                    let end = MediaTime::from_ticks(ticks(base.start_time) + ticks(base.duration));
                    if !targets.is_empty() && !targets.contains(&base.id) {
                        continue;
                    }
                    if time <= base.start_time || time >= end {
                        continue;
                    }
                    let left_visible = sub(time, base.start_time);
                    let right_visible = sub(base.duration, left_visible);

                    let retime = retime_of(element).cloned();
                    let left_span =
                        cutix_playback::retime::source_span_ticks(retime.as_ref(), left_visible);
                    let total_span =
                        cutix_playback::retime::source_span_ticks(retime.as_ref(), base.duration);
                    let right_span = sub(total_span, left_span);

                    let mut right = element.clone();
                    let right_fields = element_base_mut(&mut right);
                    right_fields.id = new_id();
                    right_fields.name = format!("{} (right)", base.name);
                    right_fields.start_time = time;
                    right_fields.duration = right_visible;
                    right_fields.trim_start = add(base.trim_start, left_span);
                    if retain != Retain::Left {
                        created.push(right_fields.id.clone());
                        additions.push(right);
                    }

                    let left_fields = element_base_mut(element);
                    left_fields.name = format!("{} (left)", base.name);
                    left_fields.duration = left_visible;
                    left_fields.trim_end = add(base.trim_end, right_span);
                    if retain == Retain::Right {
                        dropped.push(base.id.clone());
                    }
                }
                track.elements_mut().extend(additions);
                track
                    .elements_mut()
                    .retain(|element| !dropped.contains(&element.base().id));
            }
            if created.is_empty() && dropped.is_empty() {
                return false;
            }
            *selection = created;
            true
        })
    }

    pub fn reverse_element(
        &mut self,
        element_id: &str,
        next_media_id: &str,
        next_source_duration: MediaTime,
    ) -> bool {
        let next_media_id = next_media_id.to_string();
        self.mutate("reverse", element_id, move |element| {
            if let TimelineElement::Video(video) = element {
                if video.reversed_from.is_none() {
                    apply_reverse_swap(video, &next_media_id, next_source_duration.as_ticks());
                }
            }
        })
    }

    pub fn restore_reverse(&mut self, element_id: &str) -> bool {
        self.mutate("reverse", element_id, move |element| {
            let TimelineElement::Video(video) = element else {
                return;
            };
            let Some(link) = video.reversed_from.clone() else {
                return;
            };
            let media_id = link.get("mediaId").and_then(Value::as_str);
            let source = link.get("sourceDuration").and_then(Value::as_i64);
            if let (Some(media_id), Some(source)) = (media_id, source) {
                apply_reverse_swap(video, &media_id.to_string(), source);
            }
        })
    }

    pub fn delete_selected(&mut self) -> bool {
        let targets = self.selection.clone();
        if targets.is_empty() {
            return false;
        }
        self.commit("delete", move |tracks, selection| {
            let mut removed = false;
            for track in tracks_mut(tracks) {
                let before = track.elements().len();
                track
                    .elements_mut()
                    .retain(|element| !targets.contains(&element.base().id));
                removed |= track.elements().len() != before;
            }
            selection.clear();
            removed
        })
    }

    pub fn duplicate_selected(&mut self) -> bool {
        let targets = self.selection.clone();
        if targets.is_empty() {
            return false;
        }
        self.commit("duplicate", move |tracks, selection| {
            let sources: Vec<TimelineElement> = tracks
                .all()
                .flat_map(Track::elements)
                .filter(|element| targets.contains(&element.base().id))
                .cloned()
                .collect();
            if sources.is_empty() {
                return false;
            }
            let mut created = Vec::new();
            for source in sources {
                let mut copy = source.clone();
                let fields = element_base_mut(&mut copy);
                fields.id = new_id();
                fields.name = format!("{} (copy)", source.base().name);
                created.push(fields.id.clone());
                let mut track = empty_track_for(&copy);
                track.elements_mut().push(copy);
                insert_track(tracks, track);
            }
            *selection = created;
            true
        })
    }

    fn mutate<F>(&mut self, label: &'static str, element_id: &str, change: F) -> bool
    where
        F: FnOnce(&mut TimelineElement),
    {
        let element_id = element_id.to_string();
        self.commit(label, move |tracks, _| {
            let Some((track_id, index)) = locate(tracks, &element_id) else {
                return false;
            };
            let Some(track) = track_by_id_mut(tracks, &track_id) else {
                return false;
            };
            change(&mut track.elements_mut()[index]);
            true
        })
    }

    pub fn set_property(&mut self, element_id: &str, field: Field, value: f64) -> bool {
        self.mutate("property", element_id, move |element| {
            set_field(element, field, value)
        })
    }

    pub fn apply_setting(&mut self, element_id: &str, setting: Setting) -> bool {
        self.mutate("property", element_id, move |element| {
            apply_setting(element, setting)
        })
    }

    pub fn set_keyframe(
        &mut self,
        element_id: &str,
        field: Field,
        local: MediaTime,
        value: f64,
    ) -> bool {
        let Some(path) = field.path() else {
            return self.set_property(element_id, field, value);
        };
        let value = clamp_field(field, value);
        self.mutate("keyframe", element_id, move |element| {
            let animations = animations_mut(element);
            ensure_binding(animations, path, "number");
            upsert_scalar(animations, channel_id(path, "value"), local, value, None);
        })
    }

    pub fn toggle_keyframe(
        &mut self,
        element_id: &str,
        field: Field,
        local: MediaTime,
        value: f64,
    ) -> bool {
        let Some(path) = field.path() else {
            return false;
        };
        let value = clamp_field(field, value);
        self.mutate("keyframe", element_id, move |element| {
            let existing = has_key_at(element, path, local);
            let animations = animations_mut(element);
            if existing {
                remove_scalar(animations, &channel_id(path, "value"), local);
                return;
            }
            ensure_binding(animations, path, "number");
            upsert_scalar(animations, channel_id(path, "value"), local, value, None);
        })
    }

    pub fn toggle_color_keyframe(
        &mut self,
        element_id: &str,
        path: &'static str,
        local: MediaTime,
        color: [f64; 4],
    ) -> bool {
        self.mutate("keyframe", element_id, move |element| {
            let existing = has_color_key_at(element, path, local);
            let animations = animations_mut(element);
            let components = ["r", "g", "b", "a"];
            if existing {
                for component in components {
                    remove_scalar(animations, &channel_id(path, component), local);
                }
                return;
            }
            ensure_binding(animations, path, "color");
            for (index, component) in components.iter().enumerate() {
                let value = if index == 3 {
                    color[3]
                } else {
                    cutix_project::color::srgb_to_linear_channel(color[index])
                };
                upsert_scalar(animations, channel_id(path, component), local, value, None);
            }
        })
    }

    pub fn set_audio_fade(
        &mut self,
        element_id: &str,
        fade_in: MediaTime,
        fade_out: MediaTime,
    ) -> bool {
        self.mutate("fade", element_id, move |element| {
            let duration = element.base().duration;
            let base_volume = volume_of(element).unwrap_or(0.0);
            let fade_in = fade_in.clamp(MediaTime::ZERO, duration);
            let fade_out = fade_out.clamp(MediaTime::ZERO, duration);
            let animations = animations_mut(element);
            ensure_binding(animations, "volume", "number");
            let channel = channel_id("volume", "value");
            for id in [FADE_IN_START, FADE_IN_END, FADE_OUT_START, FADE_OUT_END] {
                remove_scalar_by_id(animations, &channel, id);
            }
            if ticks(fade_in) > 0 {
                upsert_scalar(
                    animations,
                    channel.clone(),
                    MediaTime::ZERO,
                    VOLUME_DB_MIN,
                    Some(FADE_IN_START),
                );
                upsert_scalar(
                    animations,
                    channel.clone(),
                    fade_in,
                    base_volume,
                    Some(FADE_IN_END),
                );
            }
            if ticks(fade_out) > 0 {
                upsert_scalar(
                    animations,
                    channel.clone(),
                    sub(duration, fade_out),
                    base_volume,
                    Some(FADE_OUT_START),
                );
                upsert_scalar(
                    animations,
                    channel,
                    duration,
                    VOLUME_DB_MIN,
                    Some(FADE_OUT_END),
                );
            }
        })
    }

    pub fn apply_text_animation(
        &mut self,
        element_id: &str,
        direction: crate::text_anim::Direction,
        preset_id: Option<&str>,
        duration: Option<MediaTime>,
        canvas: (f64, f64),
    ) -> bool {
        let preset_id = preset_id.map(str::to_string);
        self.mutate("animation.preset", element_id, move |element| {
            let element_duration = element.base().duration;
            let settings = text_animation_settings(element);
            let previous = settings
                .get(direction.key())
                .and_then(|entry| entry.get("presetId"))
                .and_then(Value::as_str)
                .and_then(crate::text_anim::preset);

            let base = crate::text_anim::Base {
                position_x: transform_of(element).map(|t| t.position.x).unwrap_or(0.0),
                position_y: transform_of(element).map(|t| t.position.y).unwrap_or(0.0),
                scale_x: transform_of(element).map(|t| t.scale_x).unwrap_or(1.0),
                scale_y: transform_of(element).map(|t| t.scale_y).unwrap_or(1.0),
                opacity: opacity_of(element).unwrap_or(1.0),
                canvas_width: canvas.0,
                canvas_height: canvas.1,
            };

            let mut settings = settings;
            {
                let animations = animations_mut(element);
                if let Some(previous) = previous {
                    for track in previous.tracks(&base) {
                        let channel = channel_id(track.path, "value");
                        for index in 0..track.keys.len() {
                            let id = crate::text_anim::keyframe_id(direction, track.path, index);
                            remove_scalar_by_id(animations, &channel, &id);
                        }
                        prune_empty_channel(animations, track.path, &channel);
                    }
                }

                match preset_id
                    .as_deref()
                    .and_then(crate::text_anim::preset)
                    .filter(|preset| preset.direction == direction)
                {
                    Some(preset) => {
                        let stored = settings
                            .get(direction.key())
                            .and_then(|entry| entry.get("duration"))
                            .and_then(Value::as_i64)
                            .map(MediaTime::from_ticks);
                        let window = crate::text_anim::clamp_duration(
                            duration
                                .or(stored)
                                .unwrap_or(crate::text_anim::DEFAULT_DURATION),
                            element_duration,
                        );
                        let start =
                            crate::text_anim::window_start(direction, element_duration, window);

                        for track in preset.tracks(&base) {
                            ensure_binding(animations, track.path, "number");
                            let channel = channel_id(track.path, "value");
                            for (index, (offset, value)) in track.keys.iter().enumerate() {
                                let id =
                                    crate::text_anim::keyframe_id(direction, track.path, index);
                                upsert_scalar(
                                    animations,
                                    channel.clone(),
                                    crate::text_anim::key_time(start, *offset, window),
                                    *value,
                                    Some(&id),
                                );
                            }
                        }

                        settings.insert(
                            direction.key().to_string(),
                            json!({ "presetId": preset.id, "duration": window.as_ticks() }),
                        );
                    }
                    None => {
                        settings.remove(direction.key());
                    }
                }
            }

            set_text_animation_settings(element, settings);
        })
    }

    pub fn set_text_reveal(
        &mut self,
        element_id: &str,
        preset_id: Option<&str>,
        duration: Option<MediaTime>,
    ) -> bool {
        let preset_id = preset_id.map(str::to_string);
        self.mutate("animation.reveal", element_id, move |element| {
            let element_duration = element.base().duration;
            let mut settings = text_animation_settings(element);
            match preset_id
                .as_deref()
                .filter(|id| crate::text_anim::is_reveal_style(id))
            {
                Some(id) => {
                    let stored = settings
                        .get("reveal")
                        .and_then(|entry| entry.get("duration"))
                        .and_then(Value::as_i64)
                        .map(MediaTime::from_ticks);
                    let window = crate::text_anim::clamp_reveal_duration(
                        duration
                            .or(stored)
                            .unwrap_or(crate::text_anim::REVEAL_DEFAULT_DURATION),
                        element_duration,
                    );
                    settings.insert(
                        "reveal".to_string(),
                        json!({ "presetId": id, "duration": window.as_ticks() }),
                    );
                }
                None => {
                    settings.remove("reveal");
                }
            }
            set_text_animation_settings(element, settings);
        })
    }

    pub fn set_mask_shape(&mut self, element_id: &str, shape: Option<&str>) -> bool {
        let shape = shape.map(str::to_string);
        self.mutate("mask", element_id, move |element| {
            let Some(slot) = masks_mut(element) else {
                return;
            };
            let Some(shape) = shape.as_deref() else {
                *slot = None;
                return;
            };

            let existing = slot.as_ref().and_then(|list| list.first());
            let feather = existing
                .and_then(|mask| mask.params.get("feather").cloned())
                .unwrap_or_else(|| json!(0.0));
            let inverted = existing
                .and_then(|mask| mask.params.get("inverted").cloned())
                .unwrap_or_else(|| json!(false));
            let id = existing.map(|mask| mask.id.clone()).unwrap_or_else(new_id);

            let defaults = masks::MaskParams::default();
            let mut params = ParamValues::new();
            params.insert("centerX".into(), json!(defaults.center_x));
            params.insert("centerY".into(), json!(defaults.center_y));
            params.insert("width".into(), json!(defaults.width));
            params.insert("height".into(), json!(defaults.height));
            params.insert("rotation".into(), json!(defaults.rotation));
            params.insert("feather".into(), feather);
            params.insert("inverted".into(), inverted);

            *slot = Some(vec![Mask {
                id,
                mask_type: shape.to_string(),
                params,
            }]);
        })
    }

    pub fn set_cutout(&mut self, element_id: &str, cutout: Option<Value>) -> bool {
        self.mutate("cutout", element_id, move |element| {
            let Some(slot) = cutout_mut(element) else {
                return;
            };
            *slot = cutout.clone();
        })
    }

    pub fn set_cutout_flag(&mut self, element_id: &str, key: &'static str, value: bool) -> bool {
        self.mutate("cutout", element_id, move |element| {
            let Some(Some(cutout)) = cutout_mut(element) else {
                return;
            };
            if let Some(object) = cutout.as_object_mut() {
                object.insert(key.to_string(), Value::from(value));
            }
        })
    }

    pub fn bake_tracking_keyframes(
        &mut self,
        element_id: &str,
        keyframes: &[crate::tracking::TrackingKeyframe],
    ) -> bool {
        if keyframes.is_empty() {
            return false;
        }
        let keyframes = keyframes.to_vec();
        self.mutate("tracking", element_id, move |element| {
            let animations = animations_mut(element);
            ensure_binding(animations, "transform.positionX", "number");
            ensure_binding(animations, "transform.positionY", "number");
            for keyframe in &keyframes {
                let local = MediaTime::from_ticks(keyframe.time);
                upsert_scalar(
                    animations,
                    channel_id("transform.positionX", "value"),
                    local,
                    keyframe.x,
                    None,
                );
                upsert_scalar(
                    animations,
                    channel_id("transform.positionY", "value"),
                    local,
                    keyframe.y,
                    None,
                );
            }
        })
    }

    pub fn bake_motion(
        &mut self,
        element_id: &str,
        clear_times: &[i64],
        keyframes: &[crate::motion::MotionKeyframe],
        settings: Option<MotionSettings>,
    ) -> bool {
        let clear_times = clear_times.to_vec();
        let keyframes = keyframes.to_vec();
        self.mutate("motion", element_id, move |element| {
            let animations = animations_mut(element);
            for path in crate::motion::MOTION_PROPERTY_PATHS {
                for ticks in &clear_times {
                    remove_scalar(
                        animations,
                        &channel_id(path, "value"),
                        MediaTime::from_ticks(*ticks),
                    );
                }
            }
            for key in &keyframes {
                let local = MediaTime::from_ticks(key.time);
                for (path, value) in [
                    ("transform.scaleX", key.scale_x),
                    ("transform.scaleY", key.scale_y),
                    ("transform.positionX", key.position_x),
                    ("transform.positionY", key.position_y),
                ] {
                    ensure_binding(animations, path, "number");
                    upsert_scalar(animations, channel_id(path, "value"), local, value, None);
                }
            }
            if let Some(slot) = motion_mut(element) {
                *slot = settings;
            }
        })
    }

    pub fn set_mask_param(&mut self, element_id: &str, key: &'static str, value: Value) -> bool {
        self.mutate("mask.param", element_id, move |element| {
            let Some(Some(list)) = masks_mut(element) else {
                return;
            };
            let Some(mask) = list.first_mut() else {
                return;
            };
            mask.params.insert(key.to_string(), value);
        })
    }

    pub fn apply_text_preset(&mut self, element_id: &str, patch: crate::text::PresetPatch) -> bool {
        self.mutate("preset", element_id, move |element| {
            let Some(text) = text_mut(element) else {
                return;
            };
            text.font_family = patch.font_family;
            text.font_size = patch.font_size;
            text.font_weight = patch.font_weight;
            text.color = patch.color;
            text.letter_spacing = Some(patch.letter_spacing);
            text.line_height = Some(patch.line_height);
            text.background.enabled = patch.background_enabled;
            text.background.color = patch.background_color;
            text.background.corner_radius = Some(patch.corner_radius);
            text.background.padding_x = Some(patch.padding_x);
            text.background.padding_y = Some(patch.padding_y);
            text.stroke = Some(patch.stroke);
            text.shadow = Some(patch.shadow);
            text.gradient = Some(patch.gradient);
        })
    }

    pub fn insert_element(&mut self, element: TimelineElement, start_time: MediaTime) -> bool {
        let start = snap_to_frame(start_time, self.fps);
        self.commit("insert", move |tracks, selection| {
            let element = place(&element, start);
            let id = element.base().id.clone();
            match first_available(tracks, &element, start) {
                Some(track_id) => {
                    let start = enforce_main_start(tracks, &track_id, start);
                    let element = place(&element, start);
                    let Some(track) = track_by_id_mut(tracks, &track_id) else {
                        return false;
                    };
                    track.elements_mut().push(element);
                }
                None => {
                    let mut track = empty_track_for(&element);
                    track.elements_mut().push(element);
                    insert_track(tracks, track);
                }
            }
            *selection = vec![id];
            true
        })
    }

    pub fn paste_elements(&mut self, elements: Vec<TimelineElement>, at: MediaTime) -> bool {
        if elements.is_empty() {
            return false;
        }
        let anchor = elements
            .iter()
            .map(|element| element.base().start_time)
            .min()
            .unwrap_or(MediaTime::ZERO);
        let fps = self.fps;
        let at = snap_to_frame(at, fps);

        self.commit("paste", move |tracks, selection| {
            let mut created = Vec::new();
            for source in &elements {
                let offset = sub(source.base().start_time, anchor);
                let start = snap_to_frame(add(at, offset), fps);
                let mut copy = place(source, start);
                element_base_mut(&mut copy).id = new_id();
                created.push(copy.base().id.clone());
                match first_available(tracks, &copy, start) {
                    Some(track_id) => {
                        let start = enforce_main_start(tracks, &track_id, start);
                        let copy = place(&copy, start);
                        let Some(track) = track_by_id_mut(tracks, &track_id) else {
                            continue;
                        };
                        track.elements_mut().push(copy);
                    }
                    None => {
                        let mut track = empty_track_for(&copy);
                        track.elements_mut().push(copy);
                        insert_track(tracks, track);
                    }
                }
            }
            if created.is_empty() {
                return false;
            }
            *selection = created;
            true
        })
    }

    pub fn toggle_track_mute(&mut self, track_id: &str) -> bool {
        let track_id = track_id.to_string();
        self.commit("mute", move |tracks, _| {
            let Some(track) = track_by_id_mut(tracks, &track_id) else {
                return false;
            };
            match track {
                Track::Video { muted, .. } | Track::Audio { muted, .. } => {
                    *muted = !*muted;
                    true
                }
                _ => false,
            }
        })
    }

    pub fn toggle_track_hidden(&mut self, track_id: &str) -> bool {
        let track_id = track_id.to_string();
        self.commit("hide", move |tracks, _| {
            let Some(track) = track_by_id_mut(tracks, &track_id) else {
                return false;
            };
            match track {
                Track::Video { hidden, .. }
                | Track::Text { hidden, .. }
                | Track::Graphic { hidden, .. }
                | Track::Effect { hidden, .. } => {
                    *hidden = !*hidden;
                    true
                }
                Track::Audio { .. } => false,
            }
        })
    }

    pub fn add_track(&mut self, kind: TrackKind) -> Option<String> {
        let id = new_id();
        let track = empty_track_of(kind, id.clone());
        self.commit("add-track", move |tracks, _| {
            insert_track(tracks, track);
            true
        })
        .then_some(id)
    }

    pub fn remove_track(&mut self, track_id: &str) -> bool {
        let track_id = track_id.to_string();
        self.commit("remove-track", move |tracks, selection| {
            if tracks.main.id() == track_id {
                return false;
            }
            let dropped: Vec<String> = tracks
                .all()
                .filter(|track| track.id() == track_id)
                .flat_map(Track::elements)
                .map(|element| element.base().id.clone())
                .collect();
            let before = tracks.overlay.len() + tracks.audio.len();
            tracks.overlay.retain(|track| track.id() != track_id);
            tracks.audio.retain(|track| track.id() != track_id);
            if before == tracks.overlay.len() + tracks.audio.len() {
                return false;
            }
            selection.retain(|id| !dropped.contains(id));
            true
        })
    }

    pub fn create_scene(&mut self, name: String) -> Option<String> {
        let id = new_id();
        let scene = build_default_scene(id.clone(), name);
        self.commit_scenes("create-scene", move |scenes, _, _| {
            scenes.push(scene);
            true
        })
        .then_some(id)
    }

    pub fn delete_scene(&mut self, scene_id: &str) -> bool {
        let scene_id = scene_id.to_string();
        self.commit_scenes("delete-scene", move |scenes, current, selection| {
            let Some(index) = scenes.iter().position(|scene| scene.id == scene_id) else {
                return false;
            };
            if scenes[index].is_main {
                return false;
            }
            scenes.remove(index);
            if *current == scene_id {
                let fallback = scenes
                    .get(index)
                    .or_else(|| scenes.get(index.saturating_sub(1)))
                    .or_else(|| scenes.first());
                *current = fallback.map(|scene| scene.id.clone()).unwrap_or_default();
                selection.clear();
            }
            true
        })
    }

    pub fn rename_scene(&mut self, scene_id: &str, name: String) -> bool {
        let scene_id = scene_id.to_string();
        self.commit_scenes("rename-scene", move |scenes, _, _| {
            let Some(scene) = scenes.iter_mut().find(|scene| scene.id == scene_id) else {
                return false;
            };
            if scene.name == name {
                return false;
            }
            scene.name = name;
            scene.updated_at = cutix_project::now_iso();
            true
        })
    }

    pub fn toggle_bookmark(&mut self, time: MediaTime) -> bool {
        let frame_time = snap_to_frame(time, self.fps);
        self.edit_bookmarks("bookmark", move |bookmarks| {
            match find_bookmark(bookmarks, frame_time) {
                Some(index) => {
                    bookmarks.remove(index);
                }
                None => {
                    bookmarks.push(cutix_project::model::Bookmark {
                        time: frame_time,
                        note: None,
                        color: None,
                        duration: None,
                    });
                    bookmarks.sort_by_key(|bookmark| bookmark.time.as_ticks());
                }
            }
            true
        })
    }

    pub fn remove_bookmark(&mut self, time: MediaTime) -> bool {
        let frame_time = snap_to_frame(time, self.fps);
        self.edit_bookmarks("bookmark", move |bookmarks| {
            let Some(index) = find_bookmark(bookmarks, frame_time) else {
                return false;
            };
            bookmarks.remove(index);
            true
        })
    }

    pub fn move_bookmark(&mut self, from: MediaTime, to: MediaTime) -> bool {
        let from = snap_to_frame(from, self.fps);
        let to = snap_to_frame(to, self.fps);
        self.edit_bookmarks("bookmark", move |bookmarks| {
            let Some(index) = find_bookmark(bookmarks, from) else {
                return false;
            };
            bookmarks[index].time = to;
            bookmarks.sort_by_key(|bookmark| bookmark.time.as_ticks());
            true
        })
    }

    pub fn update_bookmark(&mut self, time: MediaTime, update: BookmarkUpdate) -> bool {
        let frame_time = snap_to_frame(time, self.fps);
        self.edit_bookmarks("bookmark", move |bookmarks| {
            let Some(index) = find_bookmark(bookmarks, frame_time) else {
                return false;
            };
            let bookmark = &mut bookmarks[index];
            match update {
                BookmarkUpdate::Note(note) => {
                    bookmark.note = note.filter(|value| !value.is_empty())
                }
                BookmarkUpdate::Color(color) => bookmark.color = color,
                BookmarkUpdate::Duration(duration) => {
                    bookmark.duration = duration.filter(|value| value.as_ticks() > 0)
                }
            }
            true
        })
    }

    fn edit_bookmarks<F>(&mut self, label: &'static str, change: F) -> bool
    where
        F: FnOnce(&mut Vec<cutix_project::model::Bookmark>) -> bool,
    {
        let Some(scene_id) = self.scene_id() else {
            return false;
        };
        self.commit_scenes(label, move |scenes, _, _| {
            let Some(scene) = scenes.iter_mut().find(|scene| scene.id == scene_id) else {
                return false;
            };
            if !change(&mut scene.bookmarks) {
                return false;
            }
            scene.updated_at = cutix_project::now_iso();
            true
        })
    }

    pub fn toggle_elements_muted(&mut self, element_ids: &[String]) -> bool {
        let ids = element_ids.to_vec();
        self.commit("mute-element", move |tracks, _| {
            let targets: Vec<String> = tracks
                .all()
                .flat_map(Track::elements)
                .filter(|element| {
                    ids.contains(&element.base().id) && element_can_have_audio(element)
                })
                .map(|element| element.base().id.clone())
                .collect();
            if targets.is_empty() {
                return false;
            }
            let muted = tracks
                .all()
                .flat_map(Track::elements)
                .filter(|element| targets.contains(&element.base().id))
                .any(|element| !element_muted(element));
            for track in tracks_mut(tracks) {
                for element in track.elements_mut() {
                    if !targets.contains(&element.base().id) {
                        continue;
                    }
                    match element {
                        TimelineElement::Video(video) => video.muted = Some(muted),
                        TimelineElement::Audio(audio) => audio.muted = Some(muted),
                        _ => {}
                    }
                }
            }
            true
        })
    }

    pub fn toggle_elements_hidden(&mut self, element_ids: &[String]) -> bool {
        let ids = element_ids.to_vec();
        self.commit("hide-element", move |tracks, _| {
            let targets: Vec<String> = tracks
                .all()
                .flat_map(Track::elements)
                .filter(|element| {
                    ids.contains(&element.base().id) && element_can_be_hidden(element)
                })
                .map(|element| element.base().id.clone())
                .collect();
            if targets.is_empty() {
                return false;
            }
            let hidden = tracks
                .all()
                .flat_map(Track::elements)
                .filter(|element| targets.contains(&element.base().id))
                .any(|element| !element_hidden(element));
            for track in tracks_mut(tracks) {
                for element in track.elements_mut() {
                    if !targets.contains(&element.base().id) {
                        continue;
                    }
                    match element {
                        TimelineElement::Video(video) => video.hidden = Some(hidden),
                        TimelineElement::Image(image) => image.hidden = Some(hidden),
                        TimelineElement::Text(text) => text.hidden = Some(hidden),
                        TimelineElement::Sticker(sticker) => sticker.hidden = Some(hidden),
                        TimelineElement::Graphic(graphic) => graphic.hidden = Some(hidden),
                        _ => {}
                    }
                }
            }
            true
        })
    }

    pub fn toggle_source_audio(&mut self, element_id: &str, has_audio: bool) -> bool {
        let element_id = element_id.to_string();
        self.commit("source-audio", move |tracks, _| {
            let Some((track_id, index)) = locate(tracks, &element_id) else {
                return false;
            };
            let element = &track_by_id(tracks, &track_id).unwrap().elements()[index];
            let TimelineElement::Video(video) = element else {
                return false;
            };
            if source_audio_separated(video) {
                let Some(TimelineElement::Video(video)) = track_by_id_mut(tracks, &track_id)
                    .map(|track| &mut track.elements_mut()[index])
                else {
                    return false;
                };
                video.is_source_audio_enabled = Some(true);
                return true;
            }

            if !has_audio || ticks(video.base.duration) <= 0 {
                return false;
            }
            let separated = separated_audio_element(video);
            let mut audio_track = empty_track_of(TrackKind::Audio, new_id());
            audio_track.elements_mut().push(separated);
            tracks.audio.push(audio_track);

            let Some(TimelineElement::Video(video)) =
                track_by_id_mut(tracks, &track_id).map(|track| &mut track.elements_mut()[index])
            else {
                return false;
            };
            video.is_source_audio_enabled = Some(false);
            true
        })
    }

    pub fn add_clip_effect(&mut self, element_id: &str, effect_type: &str) -> Option<String> {
        let effect_id = new_id();
        let effect = Effect {
            id: effect_id.clone(),
            effect_type: effect_type.to_owned(),
            params: crate::effects_ui::default_params(effect_type),
            enabled: true,
        };
        let changed = self.mutate("effect", element_id, move |element| {
            if let Some(effects) = effects_mut(element) {
                effects.get_or_insert_with(Vec::new).push(effect);
            }
        });
        changed.then_some(effect_id)
    }

    pub fn update_clip_effect_params(
        &mut self,
        element_id: &str,
        effect_id: &str,
        patch: Vec<(String, Value)>,
    ) -> bool {
        let effect_id = effect_id.to_string();
        self.mutate("effect", element_id, move |element| {
            let Some(effect) = effect_mut(element, &effect_id) else {
                return;
            };
            for (key, value) in patch {
                effect.params.insert(key, value);
            }
        })
    }

    pub fn toggle_clip_effect(&mut self, element_id: &str, effect_id: &str) -> bool {
        let effect_id = effect_id.to_string();
        self.mutate("effect", element_id, move |element| {
            if let Some(effect) = effect_mut(element, &effect_id) {
                effect.enabled = !effect.enabled;
            }
        })
    }

    pub fn reorder_clip_effect(&mut self, element_id: &str, from: usize, to: usize) -> bool {
        if from == to {
            return false;
        }
        let mut moved = false;
        let changed = self.mutate("effect", element_id, |element| {
            let Some(Some(effects)) = effects_mut(element) else {
                return;
            };
            if from >= effects.len() || to >= effects.len() {
                return;
            }
            let effect = effects.remove(from);
            effects.insert(to, effect);
            moved = true;
        });
        changed && moved
    }

    pub fn remove_clip_effect(&mut self, element_id: &str, effect_id: &str) -> bool {
        let effect_id = effect_id.to_string();
        self.mutate("effect", element_id, move |element| {
            if let Some(Some(effects)) = effects_mut(element) {
                effects.retain(|effect| effect.id != effect_id);
            }
        })
    }

    pub fn set_element_transition(
        &mut self,
        element_id: &str,
        transition: Option<ElementTransition>,
    ) -> bool {
        self.mutate("transition", element_id, move |element| match element {
            TimelineElement::Video(video) => video.transition = transition,
            TimelineElement::Image(image) => image.transition = transition,
            _ => {}
        })
    }
}

pub fn effects_mut(element: &mut TimelineElement) -> Option<&mut Option<Vec<Effect>>> {
    match element {
        TimelineElement::Video(inner) => Some(&mut inner.effects),
        TimelineElement::Image(inner) => Some(&mut inner.effects),
        TimelineElement::Text(inner) => Some(&mut inner.effects),
        TimelineElement::Sticker(inner) => Some(&mut inner.effects),
        TimelineElement::Graphic(inner) => Some(&mut inner.effects),
        _ => None,
    }
}

fn effect_mut<'a>(element: &'a mut TimelineElement, effect_id: &str) -> Option<&'a mut Effect> {
    effects_mut(element)?
        .as_mut()?
        .iter_mut()
        .find(|effect| effect.id == effect_id)
}

pub fn effects_of(element: &TimelineElement) -> &[Effect] {
    let effects = match element {
        TimelineElement::Video(inner) => inner.effects.as_ref(),
        TimelineElement::Image(inner) => inner.effects.as_ref(),
        TimelineElement::Text(inner) => inner.effects.as_ref(),
        TimelineElement::Sticker(inner) => inner.effects.as_ref(),
        TimelineElement::Graphic(inner) => inner.effects.as_ref(),
        _ => None,
    };
    effects.map(|effects| effects.as_slice()).unwrap_or(&[])
}

pub fn transition_of(element: &TimelineElement) -> Option<&ElementTransition> {
    match element {
        TimelineElement::Video(inner) => inner.transition.as_ref(),
        TimelineElement::Image(inner) => inner.transition.as_ref(),
        _ => None,
    }
}

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

const FADE_IN_START: &str = "audio-fade-in-start";
const FADE_IN_END: &str = "audio-fade-in-end";
const FADE_OUT_START: &str = "audio-fade-out-start";
const FADE_OUT_END: &str = "audio-fade-out-end";

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

const GRAPHIC_PARAM_PATHS: &[(&str, &str)] = &[
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

fn graphic_mut(element: &mut TimelineElement) -> Option<&mut cutix_project::GraphicElement> {
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

fn transform_mut(element: &mut TimelineElement) -> Option<&mut Transform> {
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

fn opacity_mut(element: &mut TimelineElement) -> Option<&mut f64> {
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

fn set_crop(element: &mut TimelineElement, crop: Crop) {
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

fn set_blend_mode(element: &mut TimelineElement, value: String) {
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

fn apply_reverse_swap(video: &mut VideoElement, next_media_id: &str, next_source_ticks: i64) {
    use cutix_playback::retime::{build_reverse_swap, ReversedFromPatch};
    let was_reversed = video.reversed_from.is_some();
    let source_duration = video
        .base
        .source_duration
        .map(|value| value.as_ticks() as f64);
    let swap = build_reverse_swap(
        &video.media_id,
        video.base.trim_start.as_ticks() as f64,
        video.base.trim_end.as_ticks() as f64,
        source_duration,
        video.base.duration.as_ticks() as f64,
        video.retime.as_ref(),
        was_reversed,
        next_media_id,
        next_source_ticks as f64,
    );
    video.media_id = swap.media_id;
    video.base.source_duration = Some(MediaTime::from_ticks(swap.source_duration.round() as i64));
    video.base.trim_start = MediaTime::from_ticks(swap.trim_start.round() as i64);
    video.base.trim_end = MediaTime::from_ticks(swap.trim_end.round() as i64);
    video.base.duration = MediaTime::from_ticks(swap.duration.round() as i64);
    video.reversed_from = match swap.reversed_from {
        ReversedFromPatch::Clear => None,
        ReversedFromPatch::Set(link) => Some(json!({
            "mediaId": link.media_id,
            "sourceDuration": link.source_duration.round() as i64,
        })),
    };
}

pub fn retime_of(element: &TimelineElement) -> Option<&RetimeConfig> {
    match element {
        TimelineElement::Audio(inner) => inner.retime.as_ref(),
        TimelineElement::Video(inner) => inner.retime.as_ref(),
        _ => None,
    }
}

fn set_retime(element: &mut TimelineElement, retime: Option<RetimeConfig>) {
    match element {
        TimelineElement::Audio(inner) => inner.retime = retime,
        TimelineElement::Video(inner) => inner.retime = retime,
        _ => {}
    }
}

fn nested(value: &Option<Value>, key: &str, fallback: f64) -> f64 {
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

fn set_nested(slot: &mut Option<Value>, defaults: Value, key: &str, value: Value) {
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

fn set_text_animation_settings(element: &mut TimelineElement, settings: JsonMap) {
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

pub fn text_animation_duration(element: &TimelineElement, key: &str) -> Option<MediaTime> {
    text_animation_settings(element)
        .get(key)
        .and_then(|entry| entry.get("duration"))
        .and_then(Value::as_i64)
        .map(MediaTime::from_ticks)
}

fn text_mut(element: &mut TimelineElement) -> Option<&mut TextElement> {
    match element {
        TimelineElement::Text(text) => Some(text),
        _ => None,
    }
}

fn clamp_field(field: Field, value: f64) -> f64 {
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

fn set_field(element: &mut TimelineElement, field: Field, value: f64) {
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

fn apply_setting(element: &mut TimelineElement, setting: Setting) {
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

fn channel_id(path: &str, component: &str) -> String {
    format!("{path}:{component}")
}

fn ensure_binding(animations: &mut ElementAnimations, path: &str, kind: &str) {
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

fn upsert_scalar(
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

fn remove_scalar(animations: &mut ElementAnimations, channel: &str, time: MediaTime) -> bool {
    let Some(AnimationChannel::Scalar { keys, .. }) = animations.channels.get_mut(channel) else {
        return false;
    };
    let before = keys.len();
    keys.retain(|key| key.time != time);
    before != keys.len()
}

fn remove_scalar_by_id(animations: &mut ElementAnimations, channel: &str, id: &str) -> bool {
    let Some(AnimationChannel::Scalar { keys, .. }) = animations.channels.get_mut(channel) else {
        return false;
    };
    let before = keys.len();
    keys.retain(|key| key.id != id);
    before != keys.len()
}

fn prune_empty_channel(animations: &mut ElementAnimations, path: &str, channel: &str) {
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

fn cutout_mut(element: &mut TimelineElement) -> Option<&mut Option<Value>> {
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

fn motion_mut(element: &mut TimelineElement) -> Option<&mut Option<MotionSettings>> {
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

fn masks_mut(element: &mut TimelineElement) -> Option<&mut Option<Vec<Mask>>> {
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

pub fn supports_masks(element: &TimelineElement) -> bool {
    matches!(
        element,
        TimelineElement::Video(_) | TimelineElement::Image(_) | TimelineElement::Graphic(_)
    )
}

fn animations_mut(element: &mut TimelineElement) -> &mut ElementAnimations {
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

fn place(element: &TimelineElement, start: MediaTime) -> TimelineElement {
    let mut copy = element.clone();
    element_base_mut(&mut copy).start_time = start;
    copy
}

fn tracks_mut(tracks: &mut SceneTracks) -> impl Iterator<Item = &mut Track> {
    std::iter::once(&mut tracks.main)
        .chain(tracks.overlay.iter_mut())
        .chain(tracks.audio.iter_mut())
}

pub fn track_by_id<'t>(tracks: &'t SceneTracks, id: &str) -> Option<&'t Track> {
    tracks.all().find(|track| track.id() == id)
}

fn track_by_id_mut<'t>(tracks: &'t mut SceneTracks, id: &str) -> Option<&'t mut Track> {
    tracks_mut(tracks).find(|track| track.id() == id)
}

fn locate(tracks: &SceneTracks, element_id: &str) -> Option<(String, usize)> {
    tracks.all().find_map(|track| {
        track
            .elements()
            .iter()
            .position(|element| element.base().id == element_id)
            .map(|index| (track.id().to_string(), index))
    })
}

fn fits(track: &Track, start: MediaTime, end: MediaTime, exclude: Option<&str>) -> bool {
    track.elements().iter().all(|element| {
        if exclude == Some(element.base().id.as_str()) {
            return true;
        }
        start >= element.end_time() || end <= element.base().start_time
    })
}

fn enforce_main_start(tracks: &SceneTracks, track_id: &str, requested: MediaTime) -> MediaTime {
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

fn first_available(
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

    pub fn glyph(self) -> &'static str {
        match self {
            TrackKind::Video => "video01",
            TrackKind::Text => "text",
            TrackKind::Graphic => "happy01",
            TrackKind::Effect => "magic-wand05",
            TrackKind::Audio => "volume-high",
        }
    }

    pub fn of(track: &Track) -> Self {
        match track {
            Track::Video { .. } => TrackKind::Video,
            Track::Text { .. } => TrackKind::Text,
            Track::Graphic { .. } => TrackKind::Graphic,
            Track::Effect { .. } => TrackKind::Effect,
            Track::Audio { .. } => TrackKind::Audio,
        }
    }
}

fn empty_track_of(kind: TrackKind, id: String) -> Track {
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

fn build_default_scene(id: String, name: String) -> Scene {
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

#[derive(Clone, Debug, PartialEq)]
pub enum BookmarkUpdate {
    Note(Option<String>),
    Color(Option<String>),
    Duration(Option<MediaTime>),
}

fn find_bookmark(
    bookmarks: &[cutix_project::model::Bookmark],
    frame_time: MediaTime,
) -> Option<usize> {
    bookmarks
        .iter()
        .position(|bookmark| bookmark.time == frame_time)
}

pub fn bookmark_at(
    bookmarks: &[cutix_project::model::Bookmark],
    frame_time: MediaTime,
) -> Option<&cutix_project::model::Bookmark> {
    find_bookmark(bookmarks, frame_time).map(|index| &bookmarks[index])
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

fn separated_audio_element(video: &VideoElement) -> TimelineElement {
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

fn volume_animations_only(animations: Option<&ElementAnimations>) -> Option<ElementAnimations> {
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

fn empty_track_for(element: &TimelineElement) -> Track {
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

fn insert_track(tracks: &mut SceneTracks, track: Track) {
    match track {
        Track::Audio { .. } => tracks.audio.push(track),
        _ => tracks.overlay.insert(0, track),
    }
}

fn apply_ripple(before: &SceneTracks, after: &mut SceneTracks) {
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

impl Editor<'_> {
    pub fn insert_caption_track(&mut self, elements: Vec<TimelineElement>) -> bool {
        if elements.is_empty() {
            return false;
        }
        let id = new_id();
        self.commit("captions.import", |tracks, selection| {
            tracks.overlay.insert(
                0,
                Track::Text {
                    id: id.clone(),
                    name: cutix_i18n::t("editor.tab.captions"),
                    elements,
                    hidden: false,
                },
            );
            selection.clear();
            true
        })
    }

    pub fn replace_tracks(&mut self, replacement: SceneTracks) -> bool {
        self.commit("template.apply", |tracks, selection| {
            if *tracks == replacement {
                return false;
            }
            *tracks = replacement;
            selection.clear();
            true
        })
    }
}

pub const SUBTITLE_FONT_SIZE: f64 = 5.0;

pub const SUBTITLE_BOTTOM_MARGIN_RATIO: f64 = 0.05;

pub fn subtitle_position_y(canvas_height: f64, line_count: usize) -> f64 {
    let scaled_font_size =
        SUBTITLE_FONT_SIZE * canvas_height / cutix_playback::text_render::FONT_SIZE_SCALE_REFERENCE;
    let block_height = scaled_font_size
        * cutix_playback::text_render::DEFAULT_LINE_HEIGHT
        * line_count.max(1) as f64;
    canvas_height / 2.0 - canvas_height * SUBTITLE_BOTTOM_MARGIN_RATIO - block_height / 2.0
}

#[derive(Clone)]
pub struct CaptionStyle {
    pub font_family: String,
    pub font_size: f64,
    pub font_weight: String,
    pub color: String,
    pub letter_spacing: f64,
    pub line_height: f64,
    pub background: TextBackground,
    pub stroke: Option<serde_json::Value>,
    pub shadow: Option<serde_json::Value>,
    pub gradient: Option<serde_json::Value>,
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self {
            font_family: String::from("Arial"),
            font_size: SUBTITLE_FONT_SIZE,
            font_weight: String::from("bold"),
            color: String::from("#ffffff"),
            letter_spacing: 0.0,
            line_height: cutix_playback::text_render::DEFAULT_LINE_HEIGHT,
            background: TextBackground {
                enabled: false,
                color: String::from("#000000"),
                corner_radius: Some(0.0),
                padding_x: Some(0.0),
                padding_y: Some(0.0),
                offset_x: Some(0.0),
                offset_y: Some(0.0),
            },
            stroke: None,
            shadow: None,
            gradient: None,
        }
    }
}

impl CaptionStyle {
    pub fn from_preset(preset: &crate::text::TextPreset) -> Self {
        let patch = crate::text::patch_for(preset);
        let font_size = SUBTITLE_FONT_SIZE * preset.font_size_ratio;
        Self {
            font_family: patch.font_family,
            font_size,
            font_weight: patch.font_weight,
            color: patch.color,
            letter_spacing: patch.letter_spacing,
            line_height: patch.line_height,
            background: TextBackground {
                enabled: patch.background_enabled,
                color: patch.background_color,
                corner_radius: Some(patch.corner_radius),
                padding_x: Some(patch.padding_x),
                padding_y: Some(patch.padding_y),
                offset_x: Some(0.0),
                offset_y: Some(0.0),
            },
            stroke: rescaled_span(&patch.stroke, &["width"], font_size),
            shadow: rescaled_span(&patch.shadow, &["blur", "offsetX", "offsetY"], font_size),
            gradient: Some(patch.gradient),
        }
    }
}

fn rescaled_span(
    value: &serde_json::Value,
    keys: &[&str],
    font_size: f64,
) -> Option<serde_json::Value> {
    if !value
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    let mut value = value.clone();
    let scale = font_size / crate::text::DEFAULT_FONT_SIZE;
    for key in keys {
        let Some(number) = value.get(*key).and_then(serde_json::Value::as_f64) else {
            continue;
        };
        value[*key] = serde_json::Value::from(number * scale);
    }
    Some(value)
}

impl CaptionStyle {
    pub fn with_overrides(&self, overrides: &cutix_project::SubtitleStyleOverrides) -> Self {
        let mut merged = self.clone();
        if let Some(family) = &overrides.font_family {
            merged.font_family = family.clone();
        }
        if let Some(ratio) = overrides.font_size_ratio_of_play_height {
            merged.font_size = ratio * cutix_playback::text_render::FONT_SIZE_SCALE_REFERENCE;
        }
        if let Some(color) = &overrides.color {
            merged.color = color.clone();
        }
        if let Some(weight) = &overrides.font_weight {
            merged.font_weight = weight.clone();
        }
        if let Some(spacing) = overrides.letter_spacing {
            merged.letter_spacing = spacing;
        }
        if let Some(background) = &overrides.background {
            merged.background.enabled = background.enabled;
            merged.background.color = background.color.clone();
        }
        merged
    }
}

fn placement_position_y(
    placement: Option<&cutix_project::SubtitlePlacement>,
    canvas_height: f64,
    line_count: usize,
    font_size: f64,
) -> Option<f64> {
    let placement = placement?;
    let margin = placement
        .margin_vertical_ratio
        .filter(|ratio| ratio.is_finite())
        .map(|ratio| ratio * canvas_height);
    let line_height = font_size * canvas_height
        / cutix_playback::text_render::FONT_SIZE_SCALE_REFERENCE
        * cutix_playback::text_render::DEFAULT_LINE_HEIGHT;
    let block = line_height * line_count as f64;
    match placement.vertical_align.as_str() {
        "top" => Some(-canvas_height / 2.0 + margin.unwrap_or(0.0) + block / 2.0),
        "middle" => Some(0.0),
        _ => {
            let margin = margin?;
            Some(canvas_height / 2.0 - margin - block / 2.0)
        }
    }
}

pub fn subtitle_text_element(
    index: usize,
    cue: &cutix_project::SubtitleCue,
    canvas_width: f64,
    canvas_height: f64,
    base_style: &CaptionStyle,
    rasterizer: &mut cutix_playback::TextRasterizer,
) -> Option<TimelineElement> {
    let merged;
    let style = match cue.style.as_ref() {
        Some(overrides) => {
            merged = base_style.with_overrides(overrides);
            &merged
        }
        None => base_style,
    };
    let overrides = cue.style.as_ref();
    let scaled_font_size =
        style.font_size * canvas_height / cutix_playback::text_render::FONT_SIZE_SCALE_REFERENCE;
    let content = rasterizer.wrap_text(
        cue.text.trim(),
        &style.font_family,
        style.font_weight == "bold",
        scaled_font_size,
        canvas_width * cutix_playback::text_render::SUBTITLE_MAX_WIDTH_RATIO,
    );
    if content.is_empty() {
        return None;
    }
    let start_time = MediaTime::from_seconds_f64(cue.start_time.max(0.0))?;
    let duration = MediaTime::from_seconds_f64(cue.duration)?;
    if duration.as_ticks() <= 0 {
        return None;
    }

    let line_count = content.lines().count();
    let mut base = new_base(format!(
        "{} {}",
        cutix_i18n::t("editor.tab.captions"),
        index + 1
    ));
    base.start_time = start_time;
    base.duration = duration;

    Some(TimelineElement::Text(TextElement {
        base,
        content,
        font_size: style.font_size,
        font_family: style.font_family.clone(),
        color: style.color.clone(),
        background: style.background.clone(),
        stroke: style.stroke.clone(),
        shadow: style.shadow.clone(),
        gradient: style.gradient.clone(),
        text_animations: None,
        text_align: overrides
            .and_then(|overrides| overrides.text_align.clone())
            .unwrap_or_else(|| String::from("center")),
        font_weight: style.font_weight.clone(),
        font_style: overrides
            .and_then(|overrides| overrides.font_style.clone())
            .unwrap_or_else(|| String::from("normal")),
        text_decoration: overrides
            .and_then(|overrides| overrides.text_decoration.clone())
            .unwrap_or_else(|| String::from("none")),
        letter_spacing: Some(style.letter_spacing),
        line_height: Some(style.line_height),
        hidden: None,
        transform: Transform {
            scale_x: 1.0,
            scale_y: 1.0,
            position: Vector2 {
                x: 0.0,
                y: placement_position_y(
                    overrides.and_then(|overrides| overrides.placement.as_ref()),
                    canvas_height,
                    line_count,
                    style.font_size,
                )
                .unwrap_or_else(|| subtitle_position_y(canvas_height, line_count)),
            },
            rotate: 0.0,
        },
        opacity: 1.0,
        blend_mode: None,
        effects: None,
        extra: JsonMap::new(),
    }))
}

pub fn text_elements_as_cues(tracks: &SceneTracks) -> Vec<cutix_project::SubtitleCue> {
    let mut cues: Vec<cutix_project::SubtitleCue> = tracks
        .overlay
        .iter()
        .chain(std::iter::once(&tracks.main))
        .flat_map(|track| track.elements())
        .filter_map(|element| match element {
            TimelineElement::Text(text) => Some(cutix_project::SubtitleCue::new(
                text.content.clone(),
                text.base.start_time.to_seconds_f64(),
                text.base.duration.to_seconds_f64(),
            )),
            _ => None,
        })
        .collect();
    cues.sort_by(|left, right| {
        left.start_time
            .partial_cmp(&right.start_time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    cues
}

fn new_base(name: String) -> BaseElementFields {
    BaseElementFields {
        id: new_id(),
        name,
        duration: seconds(DEFAULT_NEW_ELEMENT_SECONDS),
        start_time: MediaTime::ZERO,
        trim_start: MediaTime::ZERO,
        trim_end: MediaTime::ZERO,
        source_duration: None,
        animations: None,
    }
}

pub fn sticker_element(sticker_id: String, name: String) -> TimelineElement {
    let (intrinsic_width, intrinsic_height) = stickers::sticker_intrinsic_size(&sticker_id)
        .unwrap_or((
            stickers::DEFAULT_INTRINSIC_SIZE,
            stickers::DEFAULT_INTRINSIC_SIZE,
        ));

    TimelineElement::Sticker(cutix_project::model::StickerElement {
        base: new_base(name),
        sticker_id,
        intrinsic_width: Some(intrinsic_width),
        intrinsic_height: Some(intrinsic_height),
        hidden: None,
        transform: Transform::default(),
        crop: None,
        opacity: 1.0,
        blend_mode: None,
        effects: None,
        cutout: None,
        motion: None,
        extra: JsonMap::new(),
    })
}

pub fn graphic_element(
    definition_id: String,
    name: String,
    overrides: cutix_project::model::ParamValues,
) -> TimelineElement {
    let params = stickers::resolve_params(&definition_id, &overrides);
    TimelineElement::Graphic(cutix_project::model::GraphicElement {
        base: new_base(name),
        definition_id,
        params,
        hidden: None,
        transform: Transform::default(),
        crop: None,
        opacity: 1.0,
        blend_mode: None,
        effects: None,
        masks: None,
        extra: JsonMap::new(),
    })
}

pub fn snap_time(
    tracks: &SceneTracks,
    candidate: MediaTime,
    playhead: MediaTime,
    exclude: Option<&str>,
    threshold: MediaTime,
) -> MediaTime {
    let mut best = candidate;
    let mut distance = ticks(threshold);
    let mut consider = |target: MediaTime| {
        let delta = (ticks(target) - ticks(candidate)).abs();
        if delta < distance {
            distance = delta;
            best = target;
        }
    };

    consider(playhead);
    for track in tracks.all() {
        for element in track.elements() {
            if exclude == Some(element.base().id.as_str()) {
                continue;
            }
            consider(element.base().start_time);
            consider(element.end_time());
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project() -> Project {
        Project::new("test", cutix_project::now_iso())
    }

    fn asset(name: &str, duration: f64) -> MediaAssetData {
        MediaAssetData {
            id: new_id(),
            name: name.to_string(),
            media_type: MediaType::Video,
            size: 0,
            last_modified: 0,
            width: None,
            height: None,
            duration: Some(duration),
            fps: None,
            has_audio: None,
            ephemeral: false,
            thumbnail_url: None,
            file_name: None,
        }
    }

    fn mk<'a>(
        project: &'a mut Project,
        history: &'a mut History,
        selection: &'a mut Vec<String>,
    ) -> Editor<'a> {
        Editor {
            project,
            history,
            selection,
            ripple: false,
            fps: 30.0,
            coalesce: None,
        }
    }

    fn with_clip() -> (Project, History, Vec<String>, String) {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.insert_media(&asset("clip", 4.0), MediaTime::ZERO, None);
        }
        let id = selection[0].clone();
        (project, history, selection, id)
    }

    fn element_by_id<'a>(project: &'a Project, id: &str) -> &'a TimelineElement {
        project.scenes[0]
            .tracks
            .main
            .elements()
            .iter()
            .find(|element| element.base().id == id)
            .expect("element")
    }

    fn video_of<'a>(project: &'a Project, id: &str) -> &'a VideoElement {
        match element_by_id(project, id) {
            TimelineElement::Video(video) => video,
            _ => panic!("not a video"),
        }
    }

    #[test]
    fn reversing_swaps_media_mirrors_trims_and_records_provenance() {
        let (mut project, mut history, mut selection, id) = with_clip();
        let original_media = video_of(&project, &id).media_id.clone();

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.trim_element(&id, Edge::Start, seconds(0.5));
        }
        let before = video_of(&project, &id).clone();
        let source_ticks = before.base.source_duration.unwrap().as_ticks();

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.reverse_element(
                &id,
                "reversed-media",
                MediaTime::from_ticks(source_ticks)
            ));
        }
        let reversed = video_of(&project, &id).clone();
        assert_eq!(reversed.media_id, "reversed-media");

        assert_eq!(
            reversed.base.trim_start.as_ticks(),
            before.base.trim_end.as_ticks()
        );
        assert_eq!(
            reversed.base.trim_end.as_ticks(),
            before.base.trim_start.as_ticks()
        );
        assert_eq!(
            reversed.base.duration.as_ticks(),
            before.base.duration.as_ticks()
        );
        let link = reversed.reversed_from.as_ref().expect("provenance");
        assert_eq!(
            link.get("mediaId").and_then(Value::as_str),
            Some(original_media.as_str())
        );

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.undo().is_some());
        }
        let undone = video_of(&project, &id);
        assert_eq!(undone.media_id, original_media);
        assert!(undone.reversed_from.is_none());
        assert_eq!(
            undone.base.trim_start.as_ticks(),
            before.base.trim_start.as_ticks()
        );
        assert_eq!(
            undone.base.trim_end.as_ticks(),
            before.base.trim_end.as_ticks()
        );
    }

    #[test]
    fn restore_reverse_round_trips_to_the_original_media() {
        let (mut project, mut history, mut selection, id) = with_clip();
        let original_media = video_of(&project, &id).media_id.clone();
        let source_ticks = video_of(&project, &id)
            .base
            .source_duration
            .unwrap()
            .as_ticks();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.reverse_element(&id, "reversed-media", MediaTime::from_ticks(source_ticks));
        }
        assert_eq!(video_of(&project, &id).media_id, "reversed-media");
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.restore_reverse(&id));
        }
        let restored = video_of(&project, &id);
        assert_eq!(restored.media_id, original_media);
        assert!(restored.reversed_from.is_none());
    }

    #[test]
    fn splitting_a_speed_curved_clip_keeps_source_frames_aligned() {
        let (mut project, mut history, mut selection, id) = with_clip();

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.set_property(&id, Field::SpeedRate, 2.0));
        }
        let before = video_of(&project, &id).clone();
        let split = MediaTime::from_ticks(before.base.duration.as_ticks() / 2);

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.split_at(split);
        }

        let elements = project.scenes[0].tracks.main.elements();
        assert_eq!(elements.len(), 2);
        let left = elements.iter().find(|e| e.base().id == id).expect("left");
        let right = elements.iter().find(|e| e.base().id != id).expect("right");

        let left_source_span = left.base().duration.as_ticks() * 2;

        assert_eq!(
            right.base().trim_start.as_ticks(),
            before.base.trim_start.as_ticks() + left_source_span
        );

        let right_source_span = right.base().duration.as_ticks() * 2;
        assert_eq!(
            left.base().trim_end.as_ticks(),
            before.base.trim_end.as_ticks() + right_source_span
        );
    }

    #[test]
    fn a_mask_shape_is_stored_with_default_geometry_and_can_be_cleared() {
        let (mut project, mut history, mut selection, id) = with_clip();

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.set_mask_shape(&id, Some("ellipse")));
        }
        let mask = mask_of(element_by_id(&project, &id)).expect("mask");
        assert_eq!(mask.mask_type, "ellipse");
        assert_eq!(
            mask.params.get("feather").and_then(Value::as_f64),
            Some(0.0)
        );
        assert_eq!(
            mask.params.get("inverted").and_then(Value::as_bool),
            Some(false)
        );
        assert_eq!(
            mask.params.get("width").and_then(Value::as_f64),
            Some(masks::MaskParams::default().width)
        );

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.set_mask_shape(&id, None));
        }
        assert!(mask_of(element_by_id(&project, &id)).is_none());
    }

    #[test]
    fn switching_shapes_keeps_feather_invert_and_the_mask_id() {
        let (mut project, mut history, mut selection, id) = with_clip();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.set_mask_shape(&id, Some("rectangle"));
            editor.set_mask_param(&id, "feather", json!(24.0));
            editor.set_mask_param(&id, "inverted", json!(true));
        }
        let first = mask_of(element_by_id(&project, &id)).expect("mask").clone();

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.set_mask_shape(&id, Some("star"));
        }
        let second = mask_of(element_by_id(&project, &id)).expect("mask");
        assert_eq!(second.mask_type, "star");
        assert_eq!(second.id, first.id);
        assert_eq!(
            second.params.get("feather").and_then(Value::as_f64),
            Some(24.0)
        );
        assert_eq!(
            second.params.get("inverted").and_then(Value::as_bool),
            Some(true)
        );
    }

    #[test]
    fn a_mask_change_is_one_undo_step() {
        let (mut project, mut history, mut selection, id) = with_clip();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.set_mask_shape(&id, Some("heart"));
        }
        assert!(mask_of(element_by_id(&project, &id)).is_some());

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.undo().is_some());
        }
        assert!(mask_of(element_by_id(&project, &id)).is_none());
    }

    #[test]
    fn every_mask_shape_the_panel_offers_round_trips_through_the_command() {
        let (mut project, mut history, mut selection, id) = with_clip();
        for shape in masks::ALL_SHAPES {
            {
                let mut editor = mk(&mut project, &mut history, &mut selection);
                assert!(
                    editor.set_mask_shape(&id, Some(shape.key())),
                    "{}",
                    shape.key()
                );
            }
            let stored = mask_of(element_by_id(&project, &id)).expect("mask");
            assert_eq!(stored.mask_type, shape.key());
            assert_eq!(
                masks::MaskShape::from_key(&stored.mask_type),
                Some(*shape),
                "{}",
                shape.key()
            );
        }
    }

    fn text_clip() -> (Project, History, Vec<String>, String) {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();
        let element = text_element(
            "Text".to_string(),
            "Hello".to_string(),
            crate::text::patch_for(&crate::text::presets()[0]),
        );
        let id = element.base().id.clone();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.insert_element(element, MediaTime::ZERO);
        }
        (project, history, selection, id)
    }

    fn overlay_element<'a>(project: &'a Project, id: &str) -> &'a TimelineElement {
        project.scenes[0]
            .tracks
            .overlay
            .iter()
            .flat_map(|track| track.elements())
            .find(|element| element.base().id == id)
            .expect("text element")
    }

    #[test]
    fn a_text_entrance_writes_its_keyframes_at_the_head_of_the_clip() {
        let (mut project, mut history, mut selection, id) = text_clip();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.apply_text_animation(
                &id,
                crate::text_anim::Direction::In,
                Some("slide-in-left"),
                None,
                (1920.0, 1080.0),
            ));
        }

        let element = overlay_element(&project, &id);
        assert_eq!(
            text_animation_preset(element, "in").as_deref(),
            Some("slide-in-left")
        );
        assert_eq!(
            text_animation_duration(element, "in"),
            Some(crate::text_anim::DEFAULT_DURATION)
        );

        let animations = element.base().animations.as_ref().expect("animations");
        let channel = animations
            .channels
            .get("transform.positionX:value")
            .expect("positionX channel");
        let AnimationChannel::Scalar { keys, .. } = channel else {
            panic!("expected a scalar channel");
        };
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].time, MediaTime::ZERO);

        assert!((keys[0].value + 672.0).abs() < 1e-6, "{}", keys[0].value);
        assert!((keys[1].value).abs() < 1e-6);
        assert_eq!(keys[1].time, crate::text_anim::DEFAULT_DURATION);
    }

    #[test]
    fn a_text_exit_is_pinned_to_the_tail_of_the_clip() {
        let (mut project, mut history, mut selection, id) = text_clip();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.apply_text_animation(
                &id,
                crate::text_anim::Direction::Out,
                Some("fade-out"),
                None,
                (1920.0, 1080.0),
            );
        }

        let element = overlay_element(&project, &id);
        let animations = element.base().animations.as_ref().expect("animations");
        let AnimationChannel::Scalar { keys, .. } =
            animations.channels.get("opacity:value").expect("opacity")
        else {
            panic!("expected a scalar channel");
        };
        let window = crate::text_anim::DEFAULT_DURATION;
        let clip = element.base().duration;
        assert_eq!(keys[0].time, clip - window);
        assert_eq!(keys[1].time, clip);
        assert!((keys[1].value).abs() < 1e-6);
    }

    #[test]
    fn switching_entrance_presets_leaves_no_orphaned_keyframes() {
        let (mut project, mut history, mut selection, id) = text_clip();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.apply_text_animation(
                &id,
                crate::text_anim::Direction::In,
                Some("slide-in-left"),
                None,
                (1920.0, 1080.0),
            );
            editor.apply_text_animation(
                &id,
                crate::text_anim::Direction::In,
                Some("fade-in"),
                None,
                (1920.0, 1080.0),
            );
        }

        let element = overlay_element(&project, &id);
        let animations = element.base().animations.as_ref().expect("animations");
        assert!(
            !animations
                .channels
                .contains_key("transform.positionX:value"),
            "the slide channel should have been pruned"
        );
        assert!(!animations.bindings.contains_key("transform.positionX"));
        assert_eq!(
            text_animation_preset(element, "in").as_deref(),
            Some("fade-in")
        );
    }

    #[test]
    fn clearing_an_entrance_removes_its_keyframes_and_its_setting() {
        let (mut project, mut history, mut selection, id) = text_clip();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.apply_text_animation(
                &id,
                crate::text_anim::Direction::In,
                Some("pop-in"),
                None,
                (1920.0, 1080.0),
            );
            editor.apply_text_animation(
                &id,
                crate::text_anim::Direction::In,
                None,
                None,
                (1920.0, 1080.0),
            );
        }

        let element = overlay_element(&project, &id);
        assert_eq!(text_animation_preset(element, "in"), None);
        let animations = element.base().animations.as_ref().expect("animations");
        assert!(!animations.channels.contains_key("transform.scaleX:value"));
        assert!(!animations.channels.contains_key("opacity:value"));
    }

    #[test]
    fn the_reveal_is_stored_without_touching_the_keyframe_channels() {
        let (mut project, mut history, mut selection, id) = text_clip();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.set_text_reveal(&id, Some("typewriter"), None));
        }

        let element = overlay_element(&project, &id);
        assert_eq!(
            text_animation_preset(element, "reveal").as_deref(),
            Some("typewriter")
        );
        assert_eq!(
            text_animation_duration(element, "reveal"),
            Some(crate::text_anim::REVEAL_DEFAULT_DURATION)
        );
        assert!(element
            .base()
            .animations
            .as_ref()
            .map(|animations| animations.channels.is_empty())
            .unwrap_or(true));

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.set_text_reveal(&id, None, None);
        }
        assert_eq!(
            text_animation_preset(overlay_element(&project, &id), "reveal"),
            None
        );
    }

    #[test]
    fn an_animation_window_never_outruns_a_short_clip() {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();
        let element = text_element(
            "Text".to_string(),
            "Hi".to_string(),
            crate::text::patch_for(&crate::text::presets()[0]),
        );
        let id = element.base().id.clone();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.insert_element(element, MediaTime::ZERO);
            editor.trim_element(
                &id,
                Edge::End,
                seconds(0.2) - seconds(DEFAULT_NEW_ELEMENT_SECONDS),
            );
            editor.apply_text_animation(
                &id,
                crate::text_anim::Direction::In,
                Some("fade-in"),
                None,
                (1920.0, 1080.0),
            );
        }

        let stored = text_animation_duration(overlay_element(&project, &id), "in").expect("stored");
        assert!(stored <= seconds(0.2), "{stored:?}");
        assert!(stored >= crate::text_anim::MIN_DURATION);
    }

    #[test]
    fn dropping_media_creates_a_clip_and_extends_the_project() {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();
        let mut editor = mk(&mut project, &mut history, &mut selection);
        assert!(editor.insert_media(&asset("clip", 4.0), seconds(2.0), None));
        assert_eq!(selection.len(), 1);
        assert_eq!(project.metadata.duration, seconds(4.0));
    }

    #[test]
    fn the_main_track_head_never_carries_a_gap() {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();
        let main = project.scenes[0].tracks.main.id().to_string();
        let mut editor = mk(&mut project, &mut history, &mut selection);
        editor.insert_media(&asset("clip", 4.0), seconds(2.0), Some(&main));
        let start = project.scenes[0].tracks.main.elements()[0]
            .base()
            .start_time;
        assert_eq!(start, MediaTime::ZERO);
    }

    #[test]
    fn a_split_keeps_the_left_id_and_mints_a_right_one() {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();
        let mut editor = mk(&mut project, &mut history, &mut selection);
        editor.insert_media(&asset("clip", 4.0), MediaTime::ZERO, None);
        let original = selection[0].clone();
        let mut editor = mk(&mut project, &mut history, &mut selection);
        assert!(editor.split_at(seconds(1.0)));

        let elements: Vec<_> = project.scenes[0]
            .tracks
            .all()
            .flat_map(Track::elements)
            .cloned()
            .collect();
        assert_eq!(elements.len(), 2);
        let left = elements
            .iter()
            .find(|element| element.base().id == original)
            .expect("left half keeps the id");
        assert_eq!(left.base().duration, seconds(1.0));
        assert_eq!(left.base().trim_end, seconds(3.0));
        let right = elements
            .iter()
            .find(|element| element.base().id != original)
            .expect("right half");
        assert_eq!(right.base().start_time, seconds(1.0));
        assert_eq!(right.base().trim_start, seconds(1.0));
        assert_eq!(selection, vec![right.base().id.clone()]);
    }

    #[test]
    fn undo_and_redo_walk_the_same_states() {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();
        let mut editor = mk(&mut project, &mut history, &mut selection);
        editor.insert_media(&asset("clip", 4.0), MediaTime::ZERO, None);
        let after = project.clone();

        let mut editor = mk(&mut project, &mut history, &mut selection);
        assert_eq!(editor.undo(), Some("insert"));
        assert_eq!(
            project.scenes[0]
                .tracks
                .all()
                .flat_map(Track::elements)
                .count(),
            0
        );
        let mut editor = mk(&mut project, &mut history, &mut selection);
        assert_eq!(editor.redo(), Some("insert"));
        assert_eq!(
            project.scenes[0]
                .tracks
                .all()
                .flat_map(Track::elements)
                .count(),
            after.scenes[0]
                .tracks
                .all()
                .flat_map(Track::elements)
                .count()
        );
    }

    #[test]
    fn overlapping_moves_are_rejected_rather_than_pushed() {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();
        let main = project.scenes[0].tracks.main.id().to_string();
        let mut editor = mk(&mut project, &mut history, &mut selection);
        editor.insert_media(&asset("a", 4.0), MediaTime::ZERO, Some(&main));
        let mut editor = mk(&mut project, &mut history, &mut selection);
        editor.insert_media(&asset("b", 4.0), seconds(4.0), Some(&main));
        let second = selection[0].clone();

        let mut editor = mk(&mut project, &mut history, &mut selection);
        assert!(!editor.move_element(&second, &main, seconds(2.0)));
    }

    #[test]
    fn trimming_the_end_shortens_the_visible_span() {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();
        let mut editor = mk(&mut project, &mut history, &mut selection);
        editor.insert_media(&asset("clip", 4.0), MediaTime::ZERO, None);
        let id = selection[0].clone();
        let mut editor = mk(&mut project, &mut history, &mut selection);
        assert!(editor.trim_element(&id, Edge::End, seconds(-1.0)));

        let element = project.scenes[0]
            .tracks
            .all()
            .flat_map(Track::elements)
            .find(|element| element.base().id == id)
            .cloned()
            .expect("element");
        assert_eq!(element.base().duration, seconds(3.0));
        assert_eq!(element.base().trim_end, seconds(1.0));
    }

    #[test]
    fn a_trim_never_goes_below_one_frame() {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();
        let mut editor = mk(&mut project, &mut history, &mut selection);
        editor.insert_media(&asset("clip", 4.0), MediaTime::ZERO, None);
        let id = selection[0].clone();
        let mut editor = mk(&mut project, &mut history, &mut selection);
        editor.trim_element(&id, Edge::End, seconds(-100.0));
        let element = project.scenes[0]
            .tracks
            .all()
            .flat_map(Track::elements)
            .find(|element| element.base().id == id)
            .cloned()
            .expect("element");
        assert_eq!(element.base().duration, min_duration(30.0));
    }

    #[test]
    fn ripple_closes_the_gap_a_delete_left_behind() {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();
        let main = project.scenes[0].tracks.main.id().to_string();
        let mut editor = mk(&mut project, &mut history, &mut selection);
        editor.insert_media(&asset("a", 4.0), MediaTime::ZERO, Some(&main));
        let first = selection[0].clone();
        let mut editor = mk(&mut project, &mut history, &mut selection);
        editor.insert_media(&asset("b", 4.0), seconds(4.0), Some(&main));
        let second = selection[0].clone();

        selection = vec![first];
        let mut editor = Editor {
            project: &mut project,
            history: &mut history,
            selection: &mut selection,
            ripple: true,
            fps: 30.0,
            coalesce: None,
        };
        assert!(editor.delete_selected());
        let start = project.scenes[0]
            .tracks
            .main
            .elements()
            .iter()
            .find(|element| element.base().id == second)
            .expect("survivor")
            .base()
            .start_time;
        assert_eq!(start, MediaTime::ZERO);
    }

    #[test]
    fn snapping_prefers_the_nearest_edge_within_the_threshold() {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();
        let mut editor = mk(&mut project, &mut history, &mut selection);
        editor.insert_media(&asset("a", 4.0), MediaTime::ZERO, None);
        let tracks = &project.scenes[0].tracks;
        let snapped = snap_time(tracks, seconds(4.05), seconds(100.0), None, seconds(0.2));
        assert_eq!(snapped, seconds(4.0));
        let untouched = snap_time(tracks, seconds(9.0), seconds(100.0), None, seconds(0.2));
        assert_eq!(untouched, seconds(9.0));
    }

    #[test]
    fn track_chrome_matches_the_web_heights() {
        let main = Track::empty_video("t".into(), "main".into());
        assert_eq!(track_height(&main), 65.0);
        assert!(track_can_mute(&main));
        assert!(track_can_hide(&main));
        let audio = Track::Audio {
            id: "a".into(),
            name: "audio".into(),
            elements: Vec::new(),
            muted: false,
        };
        assert_eq!(track_height(&audio), 50.0);
        assert!(track_can_mute(&audio));
        assert!(!track_can_hide(&audio));
    }

    fn audio_asset(name: &str, duration: f64, has_audio: Option<bool>) -> MediaAssetData {
        let mut asset = asset(name, duration);
        asset.has_audio = has_audio;
        asset
    }

    fn track_ids(project: &Project) -> Vec<String> {
        let tracks = &project.scenes[0].tracks;
        tracks
            .overlay
            .iter()
            .chain(std::iter::once(&tracks.main))
            .chain(tracks.audio.iter())
            .map(|track| track.id().to_string())
            .collect()
    }

    #[test]
    fn an_added_track_is_kept_even_though_it_holds_no_elements() {
        let (mut project, mut history, mut selection, _) = with_clip();
        let before = track_ids(&project).len();

        let added = {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.add_track(TrackKind::Text).expect("track id")
        };
        assert_eq!(track_ids(&project).len(), before + 1);
        assert!(track_ids(&project).contains(&added));

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.insert_media(&asset("second", 2.0), seconds(10.0), None);
        }
        assert!(
            track_ids(&project).contains(&added),
            "the intentionally empty track survived an unrelated insert"
        );
    }

    #[test]
    fn adding_and_removing_a_track_both_undo() {
        let (mut project, mut history, mut selection, _) = with_clip();
        let before = track_ids(&project);

        let added = {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.add_track(TrackKind::Audio).expect("track id")
        };
        assert_eq!(project.scenes[0].tracks.audio.len(), 1);

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert_eq!(editor.undo(), Some("add-track"));
        }
        assert_eq!(track_ids(&project), before);

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert_eq!(editor.redo(), Some("add-track"));
            assert!(editor.remove_track(&added));
        }
        assert_eq!(track_ids(&project), before);

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert_eq!(editor.undo(), Some("remove-track"));
        }
        assert!(track_ids(&project).contains(&added));
    }

    #[test]
    fn removing_a_track_drops_its_elements_from_the_selection() {
        let (mut project, mut history, mut selection, _) = with_clip();
        let mut audio = asset("voice", 3.0);
        audio.media_type = MediaType::Audio;
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.insert_media(&audio, MediaTime::ZERO, None);
        }
        let audio_track = project.scenes[0].tracks.audio[0].id().to_string();
        let audio_element = selection[0].clone();

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.remove_track(&audio_track));
        }
        assert!(project.scenes[0].tracks.audio.is_empty());
        assert!(!selection.contains(&audio_element));
    }

    #[test]
    fn the_main_track_cannot_be_removed() {
        let (mut project, mut history, mut selection, _) = with_clip();
        let main = project.scenes[0].tracks.main.id().to_string();
        let mut editor = mk(&mut project, &mut history, &mut selection);
        assert!(!editor.remove_track(&main));
    }

    #[test]
    fn a_scene_can_be_created_renamed_and_deleted_with_undo_for_each_step() {
        let (mut project, mut history, mut selection, _) = with_clip();
        assert_eq!(project.scenes.len(), 1);

        let scene_id = {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.create_scene(String::from("Second")).expect("scene")
        };
        assert_eq!(project.scenes.len(), 2);
        assert_eq!(project.scenes[1].name, "Second");
        assert!(!project.scenes[1].is_main);
        assert_eq!(project.current_scene_id, project.scenes[0].id);

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.rename_scene(&scene_id, String::from("Renamed")));
        }
        assert_eq!(project.scenes[1].name, "Renamed");

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert_eq!(editor.undo(), Some("rename-scene"));
        }
        assert_eq!(project.scenes[1].name, "Second");

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.delete_scene(&scene_id));
        }
        assert_eq!(project.scenes.len(), 1);

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert_eq!(editor.undo(), Some("delete-scene"));
        }
        assert_eq!(project.scenes.len(), 2);
        assert_eq!(project.scenes[1].id, scene_id);

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert_eq!(editor.undo(), Some("create-scene"));
        }
        assert_eq!(project.scenes.len(), 1);
    }

    #[test]
    fn deleting_the_active_scene_falls_back_to_a_neighbour() {
        let (mut project, mut history, mut selection, _) = with_clip();
        let scene_id = {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.create_scene(String::from("Second")).expect("scene")
        };
        project.current_scene_id = scene_id.clone();

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.delete_scene(&scene_id));
        }
        assert_eq!(project.current_scene_id, project.scenes[0].id);
        assert!(selection.is_empty());
    }

    #[test]
    fn the_main_scene_cannot_be_deleted() {
        let (mut project, mut history, mut selection, _) = with_clip();
        let main = project.scenes[0].id.clone();
        let mut editor = mk(&mut project, &mut history, &mut selection);
        assert!(!editor.delete_scene(&main));
    }

    #[test]
    fn a_bookmark_toggles_on_a_frame_boundary_and_undoes() {
        let (mut project, mut history, mut selection, _) = with_clip();
        let requested = MediaTime::from_ticks(122_000);

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.toggle_bookmark(requested));
        }
        assert_eq!(project.scenes[0].bookmarks.len(), 1);
        assert_eq!(project.scenes[0].bookmarks[0].time.as_ticks(), 124_000);

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.toggle_bookmark(requested));
        }
        assert!(project.scenes[0].bookmarks.is_empty());

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert_eq!(editor.undo(), Some("bookmark"));
        }
        assert_eq!(project.scenes[0].bookmarks[0].time.as_ticks(), 124_000);
    }

    #[test]
    fn bookmarks_stay_sorted_by_tick_when_added_and_moved() {
        let (mut project, mut history, mut selection, _) = with_clip();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.toggle_bookmark(seconds(3.0)));
            assert!(editor.toggle_bookmark(seconds(1.0)));
            assert!(editor.toggle_bookmark(seconds(2.0)));
        }
        let times: Vec<i64> = project.scenes[0]
            .bookmarks
            .iter()
            .map(|bookmark| bookmark.time.as_ticks())
            .collect();
        assert_eq!(times, vec![120_000, 240_000, 360_000]);

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.move_bookmark(seconds(3.0), seconds(0.5)));
        }
        let times: Vec<i64> = project.scenes[0]
            .bookmarks
            .iter()
            .map(|bookmark| bookmark.time.as_ticks())
            .collect();
        assert_eq!(times, vec![60_000, 120_000, 240_000]);

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert_eq!(editor.undo(), Some("bookmark"));
        }
        let times: Vec<i64> = project.scenes[0]
            .bookmarks
            .iter()
            .map(|bookmark| bookmark.time.as_ticks())
            .collect();
        assert_eq!(times, vec![120_000, 240_000, 360_000]);
    }

    #[test]
    fn a_bookmark_note_is_written_and_removed_with_undo() {
        let (mut project, mut history, mut selection, _) = with_clip();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.toggle_bookmark(seconds(2.0)));
            assert!(editor.update_bookmark(
                seconds(2.0),
                BookmarkUpdate::Note(Some(String::from("cut here")))
            ));
        }
        assert_eq!(
            project.scenes[0].bookmarks[0].note.as_deref(),
            Some("cut here")
        );
        assert_eq!(project.scenes[0].bookmarks[0].time.as_ticks(), 240_000);

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert_eq!(editor.undo(), Some("bookmark"));
        }
        assert_eq!(project.scenes[0].bookmarks[0].note, None);

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.remove_bookmark(seconds(2.0)));
        }
        assert!(project.scenes[0].bookmarks.is_empty());

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert_eq!(editor.undo(), Some("bookmark"));
        }
        assert_eq!(project.scenes[0].bookmarks[0].time.as_ticks(), 240_000);
    }

    #[test]
    fn bookmarks_belong_to_their_own_scene() {
        let (mut project, mut history, mut selection, _) = with_clip();
        let second = {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.create_scene(String::from("Second")).expect("scene")
        };
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.toggle_bookmark(seconds(1.0)));
        }
        project.current_scene_id = second;
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.toggle_bookmark(seconds(4.0)));
        }
        assert_eq!(project.scenes[0].bookmarks[0].time.as_ticks(), 120_000);
        assert_eq!(project.scenes[1].bookmarks[0].time.as_ticks(), 480_000);
    }

    #[test]
    fn removing_a_bookmark_that_does_not_exist_is_a_no_op() {
        let (mut project, mut history, mut selection, _) = with_clip();
        let mut editor = mk(&mut project, &mut history, &mut selection);
        assert!(!editor.remove_bookmark(seconds(1.0)));
    }

    #[test]
    fn per_element_mute_flips_the_whole_selection_and_undoes() {
        let (mut project, mut history, mut selection, first) = with_clip();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.insert_media(&asset("second", 2.0), seconds(6.0), None);
        }
        let second = selection[0].clone();
        let both = vec![first.clone(), second.clone()];

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.toggle_elements_muted(&both));
        }
        assert!(element_muted(element_by_id(&project, &first)));
        assert!(element_muted(element_by_id(&project, &second)));

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.toggle_elements_muted(&both));
        }
        assert!(!element_muted(element_by_id(&project, &first)));

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert_eq!(editor.undo(), Some("mute-element"));
        }
        assert!(element_muted(element_by_id(&project, &first)));
        assert!(element_muted(element_by_id(&project, &second)));
    }

    #[test]
    fn a_mixed_selection_mutes_rather_than_unmutes() {
        let (mut project, mut history, mut selection, first) = with_clip();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.insert_media(&asset("second", 2.0), seconds(6.0), None);
        }
        let second = selection[0].clone();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.toggle_elements_muted(&[first.clone()]));
        }
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.toggle_elements_muted(&[first.clone(), second.clone()]));
        }
        assert!(element_muted(element_by_id(&project, &first)));
        assert!(element_muted(element_by_id(&project, &second)));
    }

    #[test]
    fn per_element_visibility_flips_and_undoes() {
        let (mut project, mut history, mut selection, id) = with_clip();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.toggle_elements_hidden(&[id.clone()]));
        }
        assert!(element_hidden(element_by_id(&project, &id)));

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert_eq!(editor.undo(), Some("hide-element"));
        }
        assert!(!element_hidden(element_by_id(&project, &id)));
    }

    #[test]
    fn an_audio_element_cannot_be_hidden_but_can_be_muted() {
        let (mut project, mut history, mut selection, _) = with_clip();
        let mut audio = asset("voice", 3.0);
        audio.media_type = MediaType::Audio;
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.insert_media(&audio, MediaTime::ZERO, None);
        }
        let audio_id = selection[0].clone();

        let mut editor = mk(&mut project, &mut history, &mut selection);
        assert!(!editor.toggle_elements_hidden(&[audio_id.clone()]));
        assert!(editor.toggle_elements_muted(&[audio_id]));
    }

    #[test]
    fn source_audio_detaches_onto_a_new_track_and_recovers() {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.insert_media(&audio_asset("clip", 4.0, Some(true)), MediaTime::ZERO, None);
        }
        let id = selection[0].clone();
        assert!(project.scenes[0].tracks.audio.is_empty());

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.toggle_source_audio(&id, true));
        }
        assert_eq!(project.scenes[0].tracks.audio.len(), 1);
        let detached = &project.scenes[0].tracks.audio[0].elements()[0];
        assert_ne!(detached.base().id, id);
        assert_eq!(detached.base().start_time, MediaTime::ZERO);
        assert_eq!(detached.base().duration.as_ticks(), 480_000);
        let TimelineElement::Video(video) = element_by_id(&project, &id) else {
            panic!("video");
        };
        assert!(source_audio_separated(&video));

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert_eq!(editor.undo(), Some("source-audio"));
        }
        assert!(project.scenes[0].tracks.audio.is_empty());
        let TimelineElement::Video(video) = element_by_id(&project, &id) else {
            panic!("video");
        };
        assert!(!source_audio_separated(&video));

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert_eq!(editor.redo(), Some("source-audio"));
            assert!(editor.toggle_source_audio(&id, true));
        }
        assert_eq!(project.scenes[0].tracks.audio.len(), 1);
        let TimelineElement::Video(video) = element_by_id(&project, &id) else {
            panic!("video");
        };
        assert_eq!(video.is_source_audio_enabled, Some(true));
    }

    #[test]
    fn source_audio_refuses_a_clip_whose_media_reports_no_audio() {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.insert_media(
                &audio_asset("silent", 4.0, Some(false)),
                MediaTime::ZERO,
                None,
            );
        }
        let id = selection[0].clone();
        let element = element_by_id(&project, &id).clone();
        assert!(!can_toggle_source_audio(&element, false));

        let mut editor = mk(&mut project, &mut history, &mut selection);
        assert!(!editor.toggle_source_audio(&id, false));
    }

    #[test]
    fn copy_and_paste_still_round_trips_alongside_the_new_commands() {
        let (mut project, mut history, mut selection, id) = with_clip();
        let copied = vec![element_by_id(&project, &id).clone()];
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.add_track(TrackKind::Text).is_some());
            assert!(editor.toggle_bookmark(seconds(1.0)));
            assert!(editor.paste_elements(copied, seconds(6.0)));
        }
        assert_eq!(project.scenes[0].tracks.main.elements().len(), 2);
        assert_eq!(project.scenes[0].bookmarks.len(), 1);

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert_eq!(editor.undo(), Some("paste"));
        }
        assert_eq!(project.scenes[0].tracks.main.elements().len(), 1);
        assert_eq!(project.scenes[0].bookmarks.len(), 1);
    }

    fn insert(project: &mut Project, history: &mut History, selection: &mut Vec<String>, at: f64) {
        let mut editor = mk(project, history, selection);
        assert!(editor.insert_media(&asset("clip", 1.0), seconds(at), None));
    }

    #[test]
    fn the_undo_stack_stops_growing_at_its_cap() {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();

        for step in 0..(MAX_UNDO_DEPTH + 40) {
            insert(
                &mut project,
                &mut history,
                &mut selection,
                step as f64 * 2.0,
            );
        }

        assert_eq!(history.depth(), (MAX_UNDO_DEPTH, 0));
    }

    #[test]
    fn undo_still_walks_back_correctly_once_the_cap_has_dropped_the_oldest_steps() {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();

        let total = MAX_UNDO_DEPTH + 10;
        for step in 0..total {
            insert(
                &mut project,
                &mut history,
                &mut selection,
                step as f64 * 2.0,
            );
        }
        let elements = |project: &Project| {
            project.scenes[0]
                .tracks
                .all()
                .flat_map(Track::elements)
                .count()
        };
        assert_eq!(elements(&project), total);

        for _ in 0..MAX_UNDO_DEPTH {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.undo().is_some());
        }
        let mut editor = mk(&mut project, &mut history, &mut selection);
        assert!(
            editor.undo().is_none(),
            "the dropped steps are not undoable"
        );
        assert_eq!(elements(&project), total - MAX_UNDO_DEPTH);

        for _ in 0..MAX_UNDO_DEPTH {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.redo().is_some());
        }
        assert_eq!(elements(&project), total);
        assert_eq!(history.depth(), (MAX_UNDO_DEPTH, 0));
    }

    #[test]
    fn a_coalesced_gesture_records_one_step_however_long_it_runs() {
        let mut project = project();
        let mut history = History::default();
        let mut selection = Vec::new();
        insert(&mut project, &mut history, &mut selection, 0.0);
        let id = selection[0].clone();
        let track = project.scenes[0].tracks.main.id().to_string();
        let (before, _) = history.depth();

        for step in 1..200 {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.coalesce = Some("drag".to_string());
            editor.move_element(&id, &track, seconds(step as f64 * 0.1));
        }

        assert_eq!(
            history.depth(),
            (before + 1, 0),
            "the drag is one undo step"
        );

        let start_of = |project: &Project| {
            project.scenes[0]
                .tracks
                .all()
                .flat_map(Track::elements)
                .next()
                .expect("element")
                .base()
                .start_time
        };
        let settled = start_of(&project);

        let mut editor = mk(&mut project, &mut history, &mut selection);
        assert!(editor.undo().is_some());
        assert_eq!(
            start_of(&project),
            MediaTime::ZERO,
            "undo goes back to where the gesture started, not one step into it"
        );

        let mut editor = mk(&mut project, &mut history, &mut selection);
        assert!(editor.redo().is_some());
        assert_eq!(
            start_of(&project),
            settled,
            "redo returns to where the gesture ended, not to its first step"
        );
    }
    #[test]
    fn reordering_effects_moves_one_entry_and_is_undoable() {
        let (mut project, mut history, mut selection, id) = with_clip();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.add_clip_effect(&id, "blur").expect("blur");
            editor
                .add_clip_effect(&id, "adjustment")
                .expect("adjustment");
            editor.add_clip_effect(&id, "mosaic").expect("mosaic");
        }
        let kinds = |project: &Project| -> Vec<String> {
            effects_of(element_by_id(project, &id))
                .iter()
                .map(|effect| effect.effect_type.clone())
                .collect()
        };
        assert_eq!(kinds(&project), ["blur", "adjustment", "mosaic"]);

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert!(editor.reorder_clip_effect(&id, 2, 0));
        }
        assert_eq!(kinds(&project), ["mosaic", "blur", "adjustment"]);

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.undo();
        }
        assert_eq!(kinds(&project), ["blur", "adjustment", "mosaic"]);
    }

    #[test]
    fn an_out_of_range_or_no_op_reorder_changes_nothing() {
        let (mut project, mut history, mut selection, id) = with_clip();
        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            editor.add_clip_effect(&id, "blur");
            editor.add_clip_effect(&id, "mosaic");
        }
        let mut editor = mk(&mut project, &mut history, &mut selection);
        assert!(!editor.reorder_clip_effect(&id, 1, 1));
        assert!(!editor.reorder_clip_effect(&id, 0, 7));
        assert!(!editor.reorder_clip_effect("missing", 0, 1));
    }
}

#[cfg(test)]
mod graphic_param_tests {
    use super::*;

    fn star() -> TimelineElement {
        graphic_element(
            String::from("star"),
            String::from("Star"),
            cutix_project::model::ParamValues::new(),
        )
    }

    fn param(definition_id: &str, key: &str) -> &'static stickers::ParamDefinition {
        stickers::definition(definition_id)
            .expect("definition")
            .params
            .iter()
            .find(|param| param.key == key)
            .expect("param")
    }

    #[test]
    fn numeric_params_read_and_write_through_the_field() {
        let mut element = star();
        let field = Field::GraphicParam(param("star", "points"));
        assert_eq!(field_value(&element, field), 5.0);
        set_field(&mut element, field, 8.4);
        assert_eq!(field_value(&element, field), 8.0);
    }

    #[test]
    fn numeric_params_clamp_to_their_declared_range() {
        let mut element = star();
        let field = Field::GraphicParam(param("star", "depth"));
        set_field(&mut element, field, 500.0);
        assert_eq!(field_value(&element, field), 99.0);
        set_field(&mut element, field, -20.0);
        assert_eq!(field_value(&element, field), 1.0);
    }

    #[test]
    fn animatable_params_bind_to_the_channel_the_composer_reads() {
        assert_eq!(
            Field::GraphicParam(param("star", "points")).path(),
            Some("params.points")
        );
        assert_eq!(
            Field::GraphicParam(param("rectangle", "cornerRadius")).path(),
            Some("params.cornerRadius")
        );
        assert_eq!(Field::GraphicParam(param("rectangle", "fill")).path(), None);
    }

    #[test]
    fn colour_and_select_params_go_through_settings() {
        let mut element = star();
        apply_setting(
            &mut element,
            Setting::GraphicParamText("fill", String::from("#00ff88")),
        );
        assert_eq!(
            graphic_param_text(&element, param("star", "fill")),
            "#00ff88"
        );
        apply_setting(
            &mut element,
            Setting::GraphicParamText("strokeAlign", String::from("outside")),
        );
        assert_eq!(
            graphic_param_text(&element, param("star", "strokeAlign")),
            "outside"
        );
    }

    #[test]
    fn a_missing_param_falls_back_to_its_default() {
        let element = graphic_element(
            String::from("polygon"),
            String::from("Polygon"),
            cutix_project::model::ParamValues::new(),
        );
        assert_eq!(
            graphic_param_number(&element, param("polygon", "sides")),
            5.0
        );
        assert_eq!(
            graphic_param_text(&element, param("polygon", "stroke")),
            "#000000"
        );
    }
}

#[cfg(test)]
mod caption_tests {
    use super::*;
    use cutix_project::SubtitleCue;

    fn build(index: usize, cue: &SubtitleCue) -> Option<TimelineElement> {
        let mut rasterizer = cutix_playback::TextRasterizer::new();
        subtitle_text_element(
            index,
            cue,
            1920.0,
            1080.0,
            &CaptionStyle::default(),
            &mut rasterizer,
        )
    }

    fn build_styled(preset_id: &str, cue: &SubtitleCue) -> Option<TimelineElement> {
        let presets = crate::text::presets();
        let preset = presets
            .iter()
            .find(|preset| preset.id == preset_id)
            .expect("preset");
        let style = CaptionStyle::from_preset(preset);
        let mut rasterizer = cutix_playback::TextRasterizer::new();
        subtitle_text_element(0, cue, 1920.0, 1080.0, &style, &mut rasterizer)
    }

    #[test]
    fn the_default_style_keeps_the_plain_subtitle_look() {
        let element = build(0, &cue("Plain", 0.0, 1.0)).expect("element");
        let TimelineElement::Text(text) = &element else {
            panic!("expected a text element");
        };
        assert!(text.stroke.is_none());
        assert!(text.shadow.is_none());
        assert!(!text.background.enabled);
        assert_eq!(text.color, "#ffffff");
    }

    #[test]
    fn a_style_preset_reaches_the_caption_element() {
        let element = build_styled("neon-glow", &cue("Glow", 0.0, 1.0)).expect("element");
        let TimelineElement::Text(text) = &element else {
            panic!("expected a text element");
        };
        assert_eq!(text.color, "#eafcff");
        assert_eq!(
            text.stroke
                .as_ref()
                .and_then(|stroke| stroke["enabled"].as_bool()),
            Some(true)
        );
        assert_eq!(
            text.shadow
                .as_ref()
                .and_then(|shadow| shadow["enabled"].as_bool()),
            Some(true)
        );
    }

    #[test]
    fn a_boxed_style_turns_the_background_on() {
        let element = build_styled("subtitle-box", &cue("Boxed", 0.0, 1.0)).expect("element");
        let TimelineElement::Text(text) = &element else {
            panic!("expected a text element");
        };
        assert!(text.background.enabled);
        assert_eq!(text.background.color, "#101014");
    }

    #[test]
    fn em_relative_spans_rescale_to_the_caption_font_size() {
        let element = build_styled("bold-outline", &cue("Outline", 0.0, 1.0)).expect("element");
        let TimelineElement::Text(text) = &element else {
            panic!("expected a text element");
        };
        let width = text
            .stroke
            .as_ref()
            .and_then(|stroke| stroke["width"].as_f64())
            .unwrap();
        assert!((width - 0.14 * SUBTITLE_FONT_SIZE).abs() < 1e-9, "{width}");
    }

    fn cue(text: &str, start: f64, duration: f64) -> SubtitleCue {
        SubtitleCue::new(text, start, duration)
    }

    #[test]
    fn a_cue_becomes_a_timed_text_element() {
        let element = build(0, &cue("Hello there", 1.5, 2.0)).expect("element");
        let TimelineElement::Text(text) = &element else {
            panic!("expected a text element");
        };
        assert_eq!(text.content, "Hello there");
        assert_eq!(text.font_size, SUBTITLE_FONT_SIZE);
        assert_eq!(text.text_align, "center");
        assert_eq!(text.font_weight, "bold");
        assert_eq!(
            text.base.start_time,
            MediaTime::from_seconds_f64(1.5).unwrap()
        );
        assert_eq!(
            text.base.duration,
            MediaTime::from_seconds_f64(2.0).unwrap()
        );
        assert_eq!(text.transform.position.x, 0.0);
        assert!(text.transform.position.y > 0.0, "captions sit below centre");
    }

    #[test]
    fn captions_sit_above_the_bottom_edge() {
        let canvas_height = 1080.0;
        let single = subtitle_position_y(canvas_height, 1);
        let double = subtitle_position_y(canvas_height, 2);
        assert!(single < canvas_height / 2.0);
        assert!(double < single, "a two-line cue moves further up");
        let scaled = SUBTITLE_FONT_SIZE * canvas_height / 90.0;
        assert!(
            (single - (540.0 - 54.0 - scaled * 1.2 / 2.0)).abs() < 1e-6,
            "{single}"
        );
    }

    #[test]
    fn a_long_cue_is_wrapped_to_the_caption_safe_width() {
        let long = "The quick brown fox jumps over the lazy dog while the entire timeline scrubs past the playhead";
        let element = build(0, &cue(long, 0.0, 2.0)).expect("element");
        let TimelineElement::Text(text) = &element else {
            panic!("expected a text element");
        };
        assert!(text.content.lines().count() > 1, "{:?}", text.content);

        let mut rasterizer = cutix_playback::TextRasterizer::new();
        let scaled = SUBTITLE_FONT_SIZE * 1080.0 / 90.0;
        let limit = 1920.0 * cutix_playback::text_render::SUBTITLE_MAX_WIDTH_RATIO;
        for line in text.content.lines() {
            let width = rasterizer.measure_line(line, "Arial", true, scaled);
            assert!(
                width <= limit,
                "line {line:?} is {width}px, limit {limit}px"
            );
        }
        assert_eq!(
            text.content
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            long
        );
    }

    #[test]
    fn empty_and_zero_length_cues_are_rejected() {
        assert!(build(0, &cue("   ", 0.0, 2.0)).is_none());
        assert!(build(0, &cue("text", 0.0, 0.0)).is_none());
    }

    #[test]
    fn text_elements_come_back_out_as_cues_in_timeline_order() {
        let mut project = cutix_project::Project::new("t", "1970-01-01T00:00:00.000Z".into());
        let tracks = &mut project.scenes[0].tracks;
        for (start, content) in [(4.0, "second"), (1.0, "first")] {
            let mut element = build(0, &cue(content, start, 2.0)).expect("element");
            if let TimelineElement::Text(text) = &mut element {
                text.content = content.to_owned();
            }
            tracks.main.elements_mut().push(element);
        }

        let cues = text_elements_as_cues(tracks);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].text, "first");
        assert_eq!(cues[1].text, "second");
        assert!((cues[0].start_time - 1.0).abs() < 1e-6);

        let serialized = cutix_project::serialize_srt(&cues);
        assert!(
            serialized.starts_with("1\n00:00:01,000 --> 00:00:03,000\nfirst"),
            "{serialized}"
        );
        let round_trip = cutix_project::parse_srt(&serialized);
        assert_eq!(round_trip.captions.len(), 2);
        assert_eq!(round_trip.captions[1].text, "second");
    }
}
