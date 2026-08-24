//! The editor's own tests.

use super::*;

#[cfg(test)]
mod editor {
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
            source_path: None,
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
            assert!(editor.toggle_elements_muted(std::slice::from_ref(&first)));
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
            assert!(editor.toggle_elements_hidden(std::slice::from_ref(&id)));
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
        assert!(!editor.toggle_elements_hidden(std::slice::from_ref(&audio_id)));
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
        assert!(source_audio_separated(video));

        {
            let mut editor = mk(&mut project, &mut history, &mut selection);
            assert_eq!(editor.undo(), Some("source-audio"));
        }
        assert!(project.scenes[0].tracks.audio.is_empty());
        let TimelineElement::Video(video) = element_by_id(&project, &id) else {
            panic!("video");
        };
        assert!(!source_audio_separated(video));

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
