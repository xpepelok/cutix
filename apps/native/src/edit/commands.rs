//! `Editor`: every mutation the user can make to a timeline, as a method.
//!
//! Each one takes a snapshot for the undo stack before it changes anything, so a
//! command that fails partway cannot leave a state the history cannot describe.

use super::*;

pub struct Editor<'a> {
    pub project: &'a mut Project,
    pub history: &'a mut History,
    pub selection: &'a mut Vec<String>,
    pub ripple: bool,
    pub fps: f32,

    pub coalesce: Option<String>,
}

impl<'a> Editor<'a> {
    /// Only the tests in this file ask for this; compiled for them alone so the
    /// shipping binary does not carry a method nothing calls.
    #[cfg(test)]
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
                apply_reverse_swap(video, media_id, source);
            }
        })
    }

    /// Only the tests in this file ask for this; compiled for them alone so the
    /// shipping binary does not carry a method nothing calls.
    #[cfg(test)]
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

    /// Runs `edit` against the scene's tracks, taking an undo snapshot first.
    ///
    /// Open to the rest of the editor: `captions` builds its own multi-step commands out
    /// of this rather than repeating the snapshot bookkeeping.
    pub(crate) fn commit<F>(&mut self, label: &'static str, edit: F) -> bool
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
            let exclude = same_track.then_some(element_id.as_str());
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
