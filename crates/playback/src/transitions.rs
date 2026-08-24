use std::collections::HashMap;

use compositor::QuadTransformDescriptor;
use cutix_project::model::{TimelineElement, Track};
use time::MediaTime;

use crate::resolve::apply_transition_easing;

pub const ADJACENCY_TOLERANCE_TICKS: i64 = 2;
pub const DEFAULT_TRANSITION_EASING: &str = "easeInOut";

pub const DEFAULT_TRANSITION_DURATION_TICKS: i64 = time::TICKS_PER_SECOND;
pub const MIN_TRANSITION_DURATION_TICKS: i64 = time::TICKS_PER_SECOND / 10;
const ZOOM_OUTGOING_GAIN: f64 = 0.35;
const ZOOM_INCOMING_GAIN: f64 = 0.6;

pub const TRANSITION_TYPES: [&str; 11] = [
    "crossfade",
    "fadeToBlack",
    "slideLeft",
    "slideRight",
    "slideUp",
    "slideDown",
    "wipeLeft",
    "wipeRight",
    "wipeUp",
    "wipeDown",
    "zoom",
];

pub const TRANSITION_EASINGS: [&str; 4] = ["linear", "easeIn", "easeOut", "easeInOut"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionRole {
    Outgoing,
    Incoming,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransitionEdge {
    pub transition_type: String,
    pub easing: String,
    pub role: TransitionRole,
    pub start_time: MediaTime,
    pub end_time: MediaTime,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ElementEdges {
    pub incoming: Option<TransitionEdge>,
    pub outgoing: Option<TransitionEdge>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedTransition<'a> {
    pub transition_type: &'a str,
    pub role: TransitionRole,
    pub progress: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayerState {
    pub transform: QuadTransformDescriptor,
    pub opacity: f64,
}

pub fn can_element_have_transition(element: &TimelineElement) -> bool {
    matches!(
        element,
        TimelineElement::Video(_) | TimelineElement::Image(_)
    )
}

fn element_transition(
    element: &TimelineElement,
) -> Option<&cutix_project::model::ElementTransition> {
    match element {
        TimelineElement::Video(video) => video.transition.as_ref(),
        TimelineElement::Image(image) => image.transition.as_ref(),
        _ => None,
    }
}

fn element_hidden(element: &TimelineElement) -> bool {
    match element {
        TimelineElement::Video(video) => video.hidden.unwrap_or(false),
        TimelineElement::Image(image) => image.hidden.unwrap_or(false),
        _ => false,
    }
}

pub fn clamp_transition_duration(
    duration: MediaTime,
    previous: MediaTime,
    next: MediaTime,
) -> MediaTime {
    let ceiling = previous.min(next).as_ticks();
    let requested = duration.as_ticks();
    if requested <= 0 {
        return MediaTime::from_ticks(DEFAULT_TRANSITION_DURATION_TICKS.min(ceiling).max(0));
    }
    MediaTime::from_ticks(
        requested
            .max(MIN_TRANSITION_DURATION_TICKS)
            .min(ceiling)
            .max(0),
    )
}

pub fn find_transition_neighbour<'a>(
    track: &'a Track,
    element_id: &str,
) -> Option<&'a TimelineElement> {
    let elements = transitionable(track);
    let index = elements
        .iter()
        .position(|element| element.base().id == element_id)?;
    if index == 0 {
        return None;
    }
    let previous = elements[index - 1].base();
    let current = elements[index].base();
    let gap = (previous.start_time.as_ticks() + previous.duration.as_ticks()
        - current.start_time.as_ticks())
    .abs();
    (gap <= ADJACENCY_TOLERANCE_TICKS).then(|| elements[index - 1])
}

fn transitionable(track: &Track) -> Vec<&TimelineElement> {
    if !matches!(track, Track::Video { .. }) {
        return Vec::new();
    }
    let mut elements: Vec<&TimelineElement> = track
        .elements()
        .iter()
        .filter(|element| can_element_have_transition(element) && !element_hidden(element))
        .collect();
    elements.sort_by_key(|element| element.base().start_time.as_ticks());
    elements
}

pub fn build_track_transition_edges(track: &Track) -> HashMap<String, ElementEdges> {
    let mut edges: HashMap<String, ElementEdges> = HashMap::new();
    let elements = transitionable(track);

    for index in 1..elements.len() {
        let previous = elements[index - 1];
        let current = elements[index];
        let Some(transition) = element_transition(current) else {
            continue;
        };

        let previous_base = previous.base();
        let current_base = current.base();
        let gap = (previous_base.start_time.as_ticks() + previous_base.duration.as_ticks()
            - current_base.start_time.as_ticks())
        .abs();
        if gap > ADJACENCY_TOLERANCE_TICKS {
            continue;
        }

        let duration = clamp_transition_duration(
            transition.duration,
            previous_base.duration,
            current_base.duration,
        );
        if duration.as_ticks() <= 0 {
            continue;
        }

        let cut = current_base.start_time.as_ticks();
        let half = duration.as_ticks() / 2;
        let start_time = MediaTime::from_ticks(cut - half);
        let end_time = MediaTime::from_ticks(cut + half);
        let easing = transition
            .easing
            .clone()
            .unwrap_or_else(|| DEFAULT_TRANSITION_EASING.to_owned());

        edges.entry(previous_base.id.clone()).or_default().outgoing = Some(TransitionEdge {
            transition_type: transition.transition_type.clone(),
            easing: easing.clone(),
            role: TransitionRole::Outgoing,
            start_time,
            end_time,
        });
        edges.entry(current_base.id.clone()).or_default().incoming = Some(TransitionEdge {
            transition_type: transition.transition_type.clone(),
            easing,
            role: TransitionRole::Incoming,
            start_time,
            end_time,
        });
    }

    edges
}

fn resolve_edge(edge: Option<&TransitionEdge>, time: MediaTime) -> Option<ResolvedTransition<'_>> {
    let edge = edge?;
    let span = (edge.end_time.as_ticks() - edge.start_time.as_ticks()) as f64;
    if span <= 0.0 {
        return None;
    }
    if time < edge.start_time || time >= edge.end_time {
        return None;
    }
    Some(ResolvedTransition {
        transition_type: &edge.transition_type,
        role: edge.role,
        progress: apply_transition_easing(
            (time.as_ticks() - edge.start_time.as_ticks()) as f64 / span,
            Some(&edge.easing),
        ),
    })
}

pub fn resolve_active_transition(
    edges: Option<&ElementEdges>,
    time: MediaTime,
) -> Option<ResolvedTransition<'_>> {
    let edges = edges?;
    resolve_edge(edges.incoming.as_ref(), time)
        .or_else(|| resolve_edge(edges.outgoing.as_ref(), time))
}

pub fn is_visible_with_transition(
    element: &TimelineElement,
    edges: Option<&ElementEdges>,
    time: MediaTime,
) -> bool {
    let base = element.base();
    let mut start = base.start_time.as_ticks();
    let mut end = base.start_time.as_ticks() + base.duration.as_ticks();
    if let Some(edges) = edges {
        if let Some(incoming) = &edges.incoming {
            start = start.min(incoming.start_time.as_ticks());
        }
        if let Some(outgoing) = &edges.outgoing {
            end = end.max(outgoing.end_time.as_ticks());
        }
    }
    let ticks = time.as_ticks();
    ticks >= start && ticks < end
}

fn clip_quad_local_axis(
    transform: &QuadTransformDescriptor,
    axis_x: bool,
    min: f64,
    max: f64,
) -> Option<QuadTransformDescriptor> {
    let size = if axis_x {
        transform.width as f64
    } else {
        transform.height as f64
    };
    if size <= 0.0 {
        return None;
    }

    let start = -size / 2.0;
    let clipped_start = start.max(min);
    let clipped_end = (size / 2.0).min(max);
    if clipped_end <= clipped_start {
        return None;
    }

    let fraction_start = (clipped_start - start) / size;
    let fraction_end = (clipped_end - start) / size;
    let flipped = if axis_x {
        transform.flip_x
    } else {
        transform.flip_y
    };
    let source_offset = if axis_x {
        transform.source_rect.x as f64
    } else {
        transform.source_rect.y as f64
    };
    let source_scale = if axis_x {
        transform.source_rect.width as f64
    } else {
        transform.source_rect.height as f64
    };

    let next_scale = (fraction_end - fraction_start) * source_scale;
    let next_offset = if flipped {
        source_offset + (1.0 - fraction_end) * source_scale
    } else {
        source_offset + fraction_start * source_scale
    };

    let shift = (clipped_start + clipped_end) / 2.0;
    let radians = (transform.rotation_degrees as f64).to_radians();
    let cos = radians.cos();
    let sin = radians.sin();
    let dx = if axis_x { shift * cos } else { -shift * sin };
    let dy = if axis_x { shift * sin } else { shift * cos };

    let mut clipped = transform.clone();
    clipped.center_x = transform.center_x + dx as f32;
    clipped.center_y = transform.center_y + dy as f32;
    if axis_x {
        clipped.width = (clipped_end - clipped_start) as f32;
        clipped.source_rect.x = next_offset as f32;
        clipped.source_rect.width = next_scale as f32;
    } else {
        clipped.height = (clipped_end - clipped_start) as f32;
        clipped.source_rect.y = next_offset as f32;
        clipped.source_rect.height = next_scale as f32;
    }
    Some(clipped)
}

fn project_wipe_window_to_local(
    transform: &QuadTransformDescriptor,
    axis_x: bool,
    min: f64,
    max: f64,
) -> (bool, f64, f64) {
    let radians = (transform.rotation_degrees as f64).to_radians();
    let cos = radians.cos();
    let sin = radians.sin();
    let along_x = if axis_x { cos } else { sin };
    let along_y = if axis_x { -sin } else { cos };
    let use_local_x = along_x.abs() >= along_y.abs();
    let denominator = if use_local_x { along_x } else { along_y };
    let center = if axis_x {
        transform.center_x as f64
    } else {
        transform.center_y as f64
    };
    let a = (min - center) / denominator;
    let b = (max - center) / denominator;
    (use_local_x, a.min(b), a.max(b))
}

fn wipe_window(
    transition_type: &str,
    progress: f64,
    canvas_width: f64,
    canvas_height: f64,
) -> Option<(bool, f64, f64)> {
    match transition_type {
        "wipeRight" => Some((true, 0.0, canvas_width * progress)),
        "wipeLeft" => Some((true, canvas_width * (1.0 - progress), canvas_width)),
        "wipeDown" => Some((false, 0.0, canvas_height * progress)),
        "wipeUp" => Some((false, canvas_height * (1.0 - progress), canvas_height)),
        _ => None,
    }
}

fn slide_offset(
    transition_type: &str,
    progress: f64,
    role: TransitionRole,
    canvas_width: f64,
    canvas_height: f64,
) -> Option<(f64, f64)> {
    let shift = if role == TransitionRole::Outgoing {
        progress
    } else {
        -(1.0 - progress)
    };
    match transition_type {
        "slideLeft" => Some((-canvas_width * shift, 0.0)),
        "slideRight" => Some((canvas_width * shift, 0.0)),
        "slideUp" => Some((0.0, -canvas_height * shift)),
        "slideDown" => Some((0.0, canvas_height * shift)),
        _ => None,
    }
}

pub fn apply_transition_to_layer(
    transform: &QuadTransformDescriptor,
    opacity: f64,
    transition: &ResolvedTransition<'_>,
    canvas_width: f64,
    canvas_height: f64,
) -> Option<LayerState> {
    let progress = transition.progress.clamp(0.0, 1.0);
    let incoming = transition.role == TransitionRole::Incoming;

    if let Some((dx, dy)) = slide_offset(
        transition.transition_type,
        progress,
        transition.role,
        canvas_width,
        canvas_height,
    ) {
        let mut slid = transform.clone();
        slid.center_x = transform.center_x + dx as f32;
        slid.center_y = transform.center_y + dy as f32;
        return Some(LayerState {
            transform: slid,
            opacity,
        });
    }

    if let Some((axis_x, min, max)) = wipe_window(
        transition.transition_type,
        progress,
        canvas_width,
        canvas_height,
    ) {
        if !incoming {
            return Some(LayerState {
                transform: transform.clone(),
                opacity,
            });
        }
        let (local_axis_x, local_min, local_max) =
            project_wipe_window_to_local(transform, axis_x, min, max);
        let clipped = clip_quad_local_axis(transform, local_axis_x, local_min, local_max)?;
        return Some(LayerState {
            transform: clipped,
            opacity,
        });
    }

    if transition.transition_type == "zoom" {
        let gain = if incoming {
            1.0 + ZOOM_INCOMING_GAIN * (1.0 - progress)
        } else {
            1.0 + ZOOM_OUTGOING_GAIN * progress
        };
        let mut zoomed = transform.clone();
        zoomed.width = transform.width * gain as f32;
        zoomed.height = transform.height * gain as f32;
        return Some(LayerState {
            transform: zoomed,
            opacity: if incoming {
                opacity * progress
            } else {
                opacity
            },
        });
    }

    if transition.transition_type == "fadeToBlack" {
        let factor = if incoming {
            (progress * 2.0 - 1.0).clamp(0.0, 1.0)
        } else {
            (1.0 - progress * 2.0).clamp(0.0, 1.0)
        };
        if factor <= 0.0 {
            return None;
        }
        return Some(LayerState {
            transform: transform.clone(),
            opacity: opacity * factor,
        });
    }

    if !incoming {
        return Some(LayerState {
            transform: transform.clone(),
            opacity,
        });
    }
    if progress <= 0.0 {
        return None;
    }
    Some(LayerState {
        transform: transform.clone(),
        opacity: opacity * progress,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use compositor::SourceRectDescriptor;

    fn quad() -> QuadTransformDescriptor {
        QuadTransformDescriptor {
            center_x: 960.0,
            center_y: 540.0,
            width: 1920.0,
            height: 1080.0,
            rotation_degrees: 0.0,
            flip_x: false,
            flip_y: false,
            source_rect: SourceRectDescriptor {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
        }
    }

    fn resolved(kind: &str, role: TransitionRole, progress: f64) -> ResolvedTransition<'_> {
        ResolvedTransition {
            transition_type: kind,
            role,
            progress,
        }
    }

    #[test]
    fn crossfade_ramps_the_incoming_layer_and_leaves_the_outgoing_one_alone() {
        let incoming = apply_transition_to_layer(
            &quad(),
            1.0,
            &resolved("crossfade", TransitionRole::Incoming, 0.25),
            1920.0,
            1080.0,
        )
        .unwrap();
        assert!((incoming.opacity - 0.25).abs() < 1e-9);
        assert_eq!(incoming.transform, quad());

        let outgoing = apply_transition_to_layer(
            &quad(),
            1.0,
            &resolved("crossfade", TransitionRole::Outgoing, 0.25),
            1920.0,
            1080.0,
        )
        .unwrap();
        assert!((outgoing.opacity - 1.0).abs() < 1e-9);

        assert!(
            apply_transition_to_layer(
                &quad(),
                1.0,
                &resolved("crossfade", TransitionRole::Incoming, 0.0),
                1920.0,
                1080.0
            )
            .is_none()
        );
    }

    #[test]
    fn fade_to_black_hides_both_sides_at_the_midpoint() {
        assert!(
            apply_transition_to_layer(
                &quad(),
                1.0,
                &resolved("fadeToBlack", TransitionRole::Outgoing, 0.5),
                1920.0,
                1080.0
            )
            .is_none()
        );
        assert!(
            apply_transition_to_layer(
                &quad(),
                1.0,
                &resolved("fadeToBlack", TransitionRole::Incoming, 0.5),
                1920.0,
                1080.0
            )
            .is_none()
        );
        let early = apply_transition_to_layer(
            &quad(),
            1.0,
            &resolved("fadeToBlack", TransitionRole::Outgoing, 0.25),
            1920.0,
            1080.0,
        )
        .unwrap();
        assert!((early.opacity - 0.5).abs() < 1e-9);
        let late = apply_transition_to_layer(
            &quad(),
            1.0,
            &resolved("fadeToBlack", TransitionRole::Incoming, 0.75),
            1920.0,
            1080.0,
        )
        .unwrap();
        assert!((late.opacity - 0.5).abs() < 1e-9);
    }

    #[test]
    fn slide_left_pushes_the_outgoing_layer_out_and_pulls_the_incoming_one_in() {
        let out = apply_transition_to_layer(
            &quad(),
            1.0,
            &resolved("slideLeft", TransitionRole::Outgoing, 0.5),
            1920.0,
            1080.0,
        )
        .unwrap();
        assert!((out.transform.center_x - 0.0).abs() < 1e-4);
        assert!((out.transform.center_y - 540.0).abs() < 1e-4);
        assert!((out.opacity - 1.0).abs() < 1e-9);

        let incoming = apply_transition_to_layer(
            &quad(),
            1.0,
            &resolved("slideLeft", TransitionRole::Incoming, 0.5),
            1920.0,
            1080.0,
        )
        .unwrap();
        assert!((incoming.transform.center_x - 1920.0).abs() < 1e-4);

        let landed = apply_transition_to_layer(
            &quad(),
            1.0,
            &resolved("slideLeft", TransitionRole::Incoming, 1.0),
            1920.0,
            1080.0,
        )
        .unwrap();
        assert!((landed.transform.center_x - 960.0).abs() < 1e-4);
    }

    #[test]
    fn slide_up_and_down_move_along_y_only() {
        let up = apply_transition_to_layer(
            &quad(),
            1.0,
            &resolved("slideUp", TransitionRole::Outgoing, 1.0),
            1920.0,
            1080.0,
        )
        .unwrap();
        assert!((up.transform.center_y + 540.0).abs() < 1e-4);
        assert!((up.transform.center_x - 960.0).abs() < 1e-4);

        let down = apply_transition_to_layer(
            &quad(),
            1.0,
            &resolved("slideDown", TransitionRole::Outgoing, 1.0),
            1920.0,
            1080.0,
        )
        .unwrap();
        assert!((down.transform.center_y - 1620.0).abs() < 1e-4);
    }

    #[test]
    fn wipe_right_reveals_the_incoming_layer_from_the_left_edge() {
        let half = apply_transition_to_layer(
            &quad(),
            1.0,
            &resolved("wipeRight", TransitionRole::Incoming, 0.5),
            1920.0,
            1080.0,
        )
        .unwrap();
        assert!((half.transform.width - 960.0).abs() < 1e-3);
        assert!((half.transform.center_x - 480.0).abs() < 1e-3);
        assert!((half.transform.source_rect.x - 0.0).abs() < 1e-6);
        assert!((half.transform.source_rect.width - 0.5).abs() < 1e-6);
        assert!((half.transform.height - 1080.0).abs() < 1e-6);

        assert!(
            apply_transition_to_layer(
                &quad(),
                1.0,
                &resolved("wipeRight", TransitionRole::Incoming, 0.0),
                1920.0,
                1080.0
            )
            .is_none()
        );

        let full = apply_transition_to_layer(
            &quad(),
            1.0,
            &resolved("wipeRight", TransitionRole::Incoming, 1.0),
            1920.0,
            1080.0,
        )
        .unwrap();
        assert!((full.transform.width - 1920.0).abs() < 1e-3);
        assert!((full.transform.center_x - 960.0).abs() < 1e-3);
    }

    #[test]
    fn wipe_up_reveals_from_the_bottom_and_offsets_the_source_rect() {
        let half = apply_transition_to_layer(
            &quad(),
            1.0,
            &resolved("wipeUp", TransitionRole::Incoming, 0.5),
            1920.0,
            1080.0,
        )
        .unwrap();
        assert!((half.transform.height - 540.0).abs() < 1e-3);
        assert!((half.transform.center_y - 810.0).abs() < 1e-3);
        assert!((half.transform.source_rect.y - 0.5).abs() < 1e-6);
        assert!((half.transform.source_rect.height - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_wipe_leaves_the_outgoing_layer_untouched() {
        let out = apply_transition_to_layer(
            &quad(),
            1.0,
            &resolved("wipeRight", TransitionRole::Outgoing, 0.5),
            1920.0,
            1080.0,
        )
        .unwrap();
        assert_eq!(out.transform, quad());
        assert!((out.opacity - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_rotated_wipe_clips_along_the_layers_own_axis() {
        let mut rotated = quad();
        rotated.rotation_degrees = 90.0;
        rotated.width = 1000.0;
        rotated.height = 1000.0;
        let clipped = apply_transition_to_layer(
            &rotated,
            1.0,
            &resolved("wipeRight", TransitionRole::Incoming, 0.5),
            1920.0,
            1080.0,
        )
        .unwrap();
        assert!(
            (clipped.transform.height - 500.0).abs() < 1e-2,
            "expected the local y axis to be clipped, got {}",
            clipped.transform.height
        );
        assert!((clipped.transform.source_rect.height - 0.5).abs() < 1e-4);
        assert!((clipped.transform.width - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn zoom_grows_both_layers_and_fades_the_incoming_one_in() {
        let incoming = apply_transition_to_layer(
            &quad(),
            1.0,
            &resolved("zoom", TransitionRole::Incoming, 0.5),
            1920.0,
            1080.0,
        )
        .unwrap();
        assert!((incoming.transform.width - 1920.0 * 1.3).abs() < 1e-2);
        assert!((incoming.opacity - 0.5).abs() < 1e-9);

        let outgoing = apply_transition_to_layer(
            &quad(),
            1.0,
            &resolved("zoom", TransitionRole::Outgoing, 1.0),
            1920.0,
            1080.0,
        )
        .unwrap();
        assert!((outgoing.transform.width - 1920.0 * 1.35).abs() < 1e-2);
        assert!((outgoing.opacity - 1.0).abs() < 1e-9);
    }

    fn clip(
        id: &str,
        start_ticks: i64,
        duration_ticks: i64,
        transition: Option<(&str, i64, &str)>,
    ) -> TimelineElement {
        let mut value = serde_json::json!({
            "type": "video",
            "id": id,
            "name": id,
            "duration": duration_ticks,
            "startTime": start_ticks,
            "trimStart": 0,
            "trimEnd": 0,
            "mediaId": "media",
            "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
            "opacity": 1.0
        });
        if let Some((kind, duration, easing)) = transition {
            value["transition"] = serde_json::json!({
                "type": kind,
                "duration": duration,
                "easing": easing,
            });
        }
        serde_json::from_value(value).unwrap()
    }

    fn two_clip_track(transition: Option<(&str, i64, &str)>) -> Track {
        let second = 120_000i64;
        Track::Video {
            id: "track".into(),
            name: "track".into(),
            elements: vec![
                clip("a", 0, 4 * second, None),
                clip("b", 4 * second, 4 * second, transition),
            ],
            muted: false,
            hidden: false,
        }
    }

    #[test]
    fn a_two_second_crossfade_blends_half_and_half_at_the_cut() {
        let second = 120_000i64;
        let track = two_clip_track(Some(("crossfade", 2 * second, "linear")));
        let edges = build_track_transition_edges(&track);
        let cut = MediaTime::from_ticks(4 * second);

        let incoming = edges.get("b").unwrap().incoming.as_ref().unwrap();
        assert_eq!(incoming.start_time.as_ticks(), 3 * second);
        assert_eq!(incoming.end_time.as_ticks(), 5 * second);

        let resolved = resolve_active_transition(edges.get("b"), cut).unwrap();
        assert!((resolved.progress - 0.5).abs() < 1e-9, "{resolved:?}");
        let layer = apply_transition_to_layer(&quad(), 1.0, &resolved, 1920.0, 1080.0).unwrap();
        assert!((layer.opacity - 0.5).abs() < 1e-9);

        let quarter = resolve_active_transition(
            edges.get("b"),
            MediaTime::from_ticks(3 * second + second / 2),
        )
        .unwrap();
        assert!((quarter.progress - 0.25).abs() < 1e-9);
    }

    #[test]
    fn a_shorter_crossfade_reaches_the_same_blend_later() {
        let second = 120_000i64;
        let long = build_track_transition_edges(&two_clip_track(Some((
            "crossfade",
            2 * second,
            "linear",
        ))));
        let short =
            build_track_transition_edges(&two_clip_track(Some(("crossfade", second, "linear"))));
        let probe = MediaTime::from_ticks(4 * second - second / 4);

        let long_progress = resolve_active_transition(long.get("b"), probe)
            .unwrap()
            .progress;
        let short_progress = resolve_active_transition(short.get("b"), probe)
            .unwrap()
            .progress;
        assert!((long_progress - 0.375).abs() < 1e-9, "{long_progress}");
        assert!((short_progress - 0.25).abs() < 1e-9, "{short_progress}");

        assert!(
            resolve_active_transition(short.get("b"), MediaTime::from_ticks(3 * second)).is_none()
        );
    }

    #[test]
    fn easing_changes_the_ramp_but_not_the_midpoint() {
        let second = 120_000i64;
        let eased = build_track_transition_edges(&two_clip_track(Some((
            "crossfade",
            2 * second,
            "easeInOut",
        ))));
        let cut = MediaTime::from_ticks(4 * second);
        assert!(
            (resolve_active_transition(eased.get("b"), cut)
                .unwrap()
                .progress
                - 0.5)
                .abs()
                < 1e-9
        );
        let quarter = resolve_active_transition(
            eased.get("b"),
            MediaTime::from_ticks(3 * second + second / 2),
        )
        .unwrap()
        .progress;

        assert!((quarter - 0.125).abs() < 1e-9, "{quarter}");
    }

    #[test]
    fn the_neighbour_is_the_adjacent_clip_before_the_transition() {
        let track = two_clip_track(Some(("crossfade", 120_000, "linear")));
        assert_eq!(
            find_transition_neighbour(&track, "b").unwrap().base().id,
            "a"
        );
        assert!(find_transition_neighbour(&track, "a").is_none());
        assert!(find_transition_neighbour(&track, "missing").is_none());
    }

    #[test]
    fn a_duration_is_clamped_to_the_web_floor_ceiling_and_default() {
        let second = MediaTime::from_ticks(120_000);
        let half = MediaTime::from_ticks(60_000);

        assert_eq!(
            clamp_transition_duration(MediaTime::from_ticks(10), second, second).as_ticks(),
            MIN_TRANSITION_DURATION_TICKS
        );

        assert_eq!(
            clamp_transition_duration(MediaTime::from_ticks(10 * 120_000), second, half).as_ticks(),
            half.as_ticks()
        );

        assert_eq!(
            clamp_transition_duration(
                MediaTime::ZERO,
                MediaTime::from_ticks(600_000),
                MediaTime::from_ticks(600_000)
            )
            .as_ticks(),
            DEFAULT_TRANSITION_DURATION_TICKS
        );
    }
}
