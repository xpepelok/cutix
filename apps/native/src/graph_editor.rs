#![allow(dead_code)]

use cutix_project::model::{AnimationChannel, CurveHandle, ElementAnimations, ScalarAnimationKey};
use time::MediaTime;

pub const TICKS_PER_SECOND: i64 = 120_000;

pub const GRAPH_WIDTH: f32 = 140.0;
pub const GRAPH_HEIGHT: f32 = 94.0;
pub const GRAPH_PADDING: f32 = 12.0;
pub const SVG_WIDTH: f32 = GRAPH_WIDTH + GRAPH_PADDING * 2.0;
pub const SVG_HEIGHT: f32 = GRAPH_HEIGHT + GRAPH_PADDING * 2.0;
pub const HANDLE_RADIUS: f32 = 3.5;
pub const ENDPOINT_RADIUS: f32 = 2.0;
pub const CURVE_SEGMENTS: usize = 64;
pub const SNAP_THRESHOLD: f64 = 0.06;
pub const SNAP_TARGETS: [f64; 2] = [0.0, 1.0];
pub const Y_CLAMP_MIN: f64 = -0.5;
pub const Y_CLAMP_MAX: f64 = 1.5;

const VALUE_EPSILON: f64 = 1e-6;
const LINEAR_CURVE_EPSILON: f64 = 1e-6;

pub type Curve = [f64; 4];

pub const LINEAR_CURVE: Curve = [0.0, 0.0, 1.0, 1.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handle {
    pub dt_ticks: i64,
    pub dv: f64,
}

impl Handle {
    pub fn to_model(self) -> CurveHandle {
        CurveHandle {
            dt: MediaTime::from_ticks(self.dt_ticks),
            dv: self.dv,
        }
    }

    pub fn from_model(handle: &CurveHandle) -> Self {
        Handle {
            dt_ticks: handle.dt.as_ticks(),
            dv: handle.dv,
        }
    }
}

pub struct EasingPreset {
    pub id: &'static str,
    pub label: &'static str,
    pub label_key: &'static str,
    pub value: Curve,
}

pub const PRESET_MATCH_TOLERANCE: f64 = 0.02;

pub const BUILTIN_PRESETS: &[EasingPreset] = &[
    EasingPreset {
        id: "smooth",
        label: "Smooth",
        label_key: "timeline.graph.preset.smooth",
        value: [0.25, 0.1, 0.25, 1.0],
    },
    EasingPreset {
        id: "ease-out",
        label: "Ease out",
        label_key: "transitions.easing.easeOut",
        value: [0.0, 0.0, 0.2, 1.0],
    },
    EasingPreset {
        id: "ease-in",
        label: "Ease in",
        label_key: "transitions.easing.easeIn",
        value: [0.8, 0.0, 1.0, 1.0],
    },
    EasingPreset {
        id: "ease-in-out",
        label: "In out",
        label_key: "timeline.graph.preset.inOut",
        value: [0.4, 0.0, 0.2, 1.0],
    },
    EasingPreset {
        id: "pop",
        label: "Pop",
        label_key: "timeline.graph.preset.pop",
        value: [0.175, 0.885, 0.32, 1.275],
    },
    EasingPreset {
        id: "linear",
        label: "Linear",
        label_key: "transitions.easing.linear",
        value: LINEAR_CURVE,
    },
];

pub fn matching_preset(curve: Curve) -> Option<&'static EasingPreset> {
    BUILTIN_PRESETS.iter().find(|preset| {
        preset
            .value
            .iter()
            .zip(curve.iter())
            .all(|(a, b)| (a - b).abs() < PRESET_MATCH_TOLERANCE)
    })
}

pub fn bezier_point(progress: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let mt = 1.0 - progress;
    mt * mt * mt * p0
        + 3.0 * mt * mt * progress * p1
        + 3.0 * mt * progress * progress * p2
        + progress * progress * progress * p3
}

pub fn to_svg_x(value: f64) -> f32 {
    GRAPH_PADDING + value as f32 * GRAPH_WIDTH
}

pub fn to_svg_y(value: f64) -> f32 {
    GRAPH_PADDING + (1.0 - value as f32) * GRAPH_HEIGHT
}

pub fn from_svg_x(svg_x: f32) -> f64 {
    (((svg_x - GRAPH_PADDING) / GRAPH_WIDTH) as f64).clamp(0.0, 1.0)
}

pub fn from_svg_y(svg_y: f32) -> f64 {
    (1.0 - ((svg_y - GRAPH_PADDING) / GRAPH_HEIGHT) as f64).clamp(Y_CLAMP_MIN, Y_CLAMP_MAX)
}

pub fn snap_handle_value(value: f64, shift_held: bool) -> f64 {
    if shift_held {
        return value;
    }
    for target in SNAP_TARGETS {
        if (value - target).abs() < SNAP_THRESHOLD {
            return target;
        }
    }
    value
}

pub fn drag_curve_handle(
    curve: Curve,
    handle: usize,
    svg_x: f32,
    svg_y: f32,
    shift_held: bool,
) -> Curve {
    let x = from_svg_x(svg_x);
    let y = snap_handle_value(from_svg_y(svg_y), shift_held);
    let mut next = curve;
    if handle == 0 {
        next[0] = x;
        next[1] = y;
    } else {
        next[2] = x;
        next[3] = y;
    }
    next
}

fn effective_span_value(span_value: f64, reference: Option<f64>) -> Option<f64> {
    if span_value.abs() > VALUE_EPSILON {
        Some(span_value)
    } else {
        match reference {
            Some(reference) if reference.abs() > VALUE_EPSILON => Some(reference),
            _ => None,
        }
    }
}

pub fn default_right_handle(
    left_tick: i64,
    left_value: f64,
    right_tick: i64,
    right_value: f64,
) -> Handle {
    Handle {
        dt_ticks: (right_tick - left_tick) / 3,
        dv: (right_value - left_value) / 3.0,
    }
}

pub fn default_left_handle(
    left_tick: i64,
    left_value: f64,
    right_tick: i64,
    right_value: f64,
) -> Handle {
    Handle {
        dt_ticks: -((right_tick - left_tick) / 3),
        dv: -((right_value - left_value) / 3.0),
    }
}

pub fn normalized_bezier_for_segment(
    left_tick: i64,
    left_value: f64,
    right_tick: i64,
    right_value: f64,
    left_right_handle: Option<Handle>,
    right_left_handle: Option<Handle>,
    reference_span_value: Option<f64>,
) -> Option<Curve> {
    let span_time = (right_tick - left_tick) as f64;
    let span_value = right_value - left_value;
    let effective = effective_span_value(span_value, reference_span_value)?;
    if span_time == 0.0 {
        return None;
    }

    let right_handle = left_right_handle
        .unwrap_or_else(|| default_right_handle(left_tick, left_value, right_tick, right_value));
    let left_handle = right_left_handle
        .unwrap_or_else(|| default_left_handle(left_tick, left_value, right_tick, right_value));

    Some([
        (right_handle.dt_ticks as f64 / span_time).clamp(0.0, 1.0),
        right_handle.dv / effective,
        (1.0 + left_handle.dt_ticks as f64 / span_time).clamp(0.0, 1.0),
        1.0 + left_handle.dv / effective,
    ])
}

pub fn curve_handles_for_normalized_bezier(
    left_tick: i64,
    left_value: f64,
    right_tick: i64,
    right_value: f64,
    curve: Curve,
    reference_span_value: Option<f64>,
) -> Option<(Handle, Handle)> {
    let span_time = (right_tick - left_tick) as f64;
    let span_value = right_value - left_value;
    let effective = effective_span_value(span_value, reference_span_value)?;
    if span_time == 0.0 {
        return None;
    }

    let x1 = curve[0].clamp(0.0, 1.0);
    let x2 = curve[2].clamp(0.0, 1.0);

    Some((
        Handle {
            dt_ticks: (span_time * x1).round() as i64,
            dv: effective * curve[1],
        },
        Handle {
            dt_ticks: (span_time * (x2 - 1.0)).round() as i64,
            dv: effective * (curve[3] - 1.0),
        },
    ))
}

pub fn is_linear_curve(curve: Curve) -> bool {
    curve[0].abs() <= LINEAR_CURVE_EPSILON
        && curve[1].abs() <= LINEAR_CURVE_EPSILON
        && (curve[2] - 1.0).abs() <= LINEAR_CURVE_EPSILON
        && (curve[3] - 1.0).abs() <= LINEAR_CURVE_EPSILON
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct CurvePatch {
    pub keyframe_id: String,
    pub segment_to_next: Option<&'static str>,
    pub left_handle: Option<Option<Handle>>,
    pub right_handle: Option<Option<Handle>>,
}

/// One end of a keyframe segment: which keyframe it is, when, and at what value.
#[derive(Clone, Copy, Debug)]
pub struct SegmentEnd<'id> {
    pub keyframe_id: &'id str,
    pub tick: i64,
    pub value: f64,
}

pub fn build_curve_patches(
    left: SegmentEnd<'_>,
    right: SegmentEnd<'_>,
    curve: Curve,
    reference_span_value: Option<f64>,
) -> Option<Vec<CurvePatch>> {
    let (left_id, left_tick, left_value) = (left.keyframe_id, left.tick, left.value);
    let (right_id, right_tick, right_value) = (right.keyframe_id, right.tick, right.value);
    if is_linear_curve(curve) {
        return Some(vec![
            CurvePatch {
                keyframe_id: left_id.to_string(),
                segment_to_next: Some("linear"),
                right_handle: Some(None),
                left_handle: None,
            },
            CurvePatch {
                keyframe_id: right_id.to_string(),
                left_handle: Some(None),
                ..Default::default()
            },
        ]);
    }

    let (right_handle, left_handle) = curve_handles_for_normalized_bezier(
        left_tick,
        left_value,
        right_tick,
        right_value,
        curve,
        reference_span_value,
    )?;

    Some(vec![
        CurvePatch {
            keyframe_id: left_id.to_string(),
            segment_to_next: Some("bezier"),
            right_handle: Some(Some(right_handle)),
            left_handle: None,
        },
        CurvePatch {
            keyframe_id: right_id.to_string(),
            left_handle: Some(Some(left_handle)),
            ..Default::default()
        },
    ])
}

#[derive(Clone, Copy, Debug)]
pub struct Axis {
    pub min: f64,
    pub max: f64,
    pub size: f32,
    pub pad: f32,
    pub invert: bool,
}

impl Axis {
    pub fn new(min: f64, max: f64, size: f32, pad: f32, invert: bool) -> Self {
        Axis {
            min,
            max,
            size,
            pad,
            invert,
        }
    }

    fn usable(&self) -> f32 {
        (self.size - self.pad * 2.0).max(1.0)
    }

    fn span(&self) -> f64 {
        let span = self.max - self.min;
        if span.abs() < VALUE_EPSILON {
            1.0
        } else {
            span
        }
    }

    pub fn to_px(self, value: f64) -> f32 {
        let fraction = ((value - self.min) / self.span()) as f32;
        let fraction = if self.invert {
            1.0 - fraction
        } else {
            fraction
        };
        self.pad + fraction * self.usable()
    }

    pub fn to_value(self, px: f32) -> f64 {
        let fraction = ((px - self.pad) / self.usable()) as f64;
        let fraction = if self.invert {
            1.0 - fraction
        } else {
            fraction
        };
        self.min + fraction * self.span()
    }
}

pub fn drag_keyframe(
    start_tick: i64,
    start_value: f64,
    dx_px: f32,
    dy_px: f32,
    time_axis: &Axis,
    value_axis: &Axis,
) -> (i64, f64) {
    let ticks_per_px = time_axis.span() / time_axis.usable() as f64;
    let value_per_px = value_axis.span() / value_axis.usable() as f64;
    let tick = start_tick + (dx_px as f64 * ticks_per_px).round() as i64;

    let value =
        start_value - dy_px as f64 * value_per_px * if value_axis.invert { 1.0 } else { -1.0 };
    (tick, value)
}

/// A segment between two keyframes, with the easing that joins them.
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub left_tick: i64,
    pub left_value: f64,
    pub right_tick: i64,
    pub right_value: f64,
    /// The handle leaving the left keyframe, for a bezier segment.
    pub left_right_handle: Option<Handle>,
    /// The handle arriving at the right keyframe, for a bezier segment.
    pub right_left_handle: Option<Handle>,
}

pub fn eval_segment(segment: Segment, segment_to_next: &str, tick: i64) -> f64 {
    let Segment {
        left_tick,
        left_value,
        right_tick,
        right_value,
        left_right_handle,
        right_left_handle,
    } = segment;
    let span = (right_tick - left_tick) as f64;
    if span == 0.0 {
        return right_value;
    }
    match segment_to_next {
        "step" => left_value,
        "linear" => {
            let progress = (tick - left_tick) as f64 / span;
            left_value + (right_value - left_value) * progress
        }
        _ => {
            let right_handle = left_right_handle.unwrap_or_else(|| {
                default_right_handle(left_tick, left_value, right_tick, right_value)
            });
            let left_handle = right_left_handle.unwrap_or_else(|| {
                default_left_handle(left_tick, left_value, right_tick, right_value)
            });
            let t0 = left_tick as f64;
            let t3 = right_tick as f64;
            let t1 = t0 + right_handle.dt_ticks as f64;
            let t2 = t3 + left_handle.dt_ticks as f64;
            let mut lower = 0.0;
            let mut upper = 1.0;
            let time = tick as f64;
            for _ in 0..20 {
                let middle = (lower + upper) / 2.0;
                if bezier_point(middle, t0, t1, t2, t3) < time {
                    lower = middle;
                } else {
                    upper = middle;
                }
            }
            let progress = (lower + upper) / 2.0;
            bezier_point(
                progress,
                left_value,
                left_value + right_handle.dv,
                right_value + left_handle.dv,
                right_value,
            )
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlotKey {
    pub id: String,
    pub tick: i64,
    pub value: f64,
    pub segment_to_next: String,
    pub left_handle: Option<Handle>,
    pub right_handle: Option<Handle>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlotTrack {
    pub path: String,
    pub component: String,
    pub keys: Vec<PlotKey>,
}

fn scalar_keys_of<'a>(
    animations: &'a ElementAnimations,
    channel_id: &str,
) -> Option<&'a Vec<ScalarAnimationKey>> {
    match animations.channels.get(channel_id) {
        Some(AnimationChannel::Scalar { keys, .. }) => Some(keys),
        _ => None,
    }
}

pub fn plot_tracks(animations: &ElementAnimations) -> Vec<PlotTrack> {
    let mut tracks = Vec::new();
    for (path, binding) in animations.bindings.iter() {
        let Some(components) = binding.get("components").and_then(|value| value.as_array()) else {
            continue;
        };
        for component in components {
            let Some(key) = component.get("key").and_then(|value| value.as_str()) else {
                continue;
            };
            if key != "value" {
                continue;
            }
            let channel_id = component
                .get("channelId")
                .and_then(|value| value.as_str())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{path}:{key}"));
            let Some(keys) = scalar_keys_of(animations, &channel_id) else {
                continue;
            };
            if keys.len() < 2 {
                continue;
            }
            let mut plot_keys: Vec<PlotKey> = keys
                .iter()
                .map(|scalar| PlotKey {
                    id: scalar.id.clone(),
                    tick: scalar.time.as_ticks(),
                    value: scalar.value,
                    segment_to_next: scalar.segment_to_next.clone(),
                    left_handle: scalar.left_handle.as_ref().map(Handle::from_model),
                    right_handle: scalar.right_handle.as_ref().map(Handle::from_model),
                })
                .collect();
            plot_keys.sort_by_key(|plot_key| plot_key.tick);
            tracks.push(PlotTrack {
                path: path.clone(),
                component: key.to_string(),
                keys: plot_keys,
            });
        }
    }
    tracks.sort_by(|a, b| a.path.cmp(&b.path));
    tracks
}

pub fn reference_span_value(keys: &[PlotKey], index: usize) -> f64 {
    let mut i = index as isize - 1;
    while i >= 0 {
        let span = (keys[(i + 1) as usize].value - keys[i as usize].value).abs();
        if span > VALUE_EPSILON {
            return span;
        }
        i -= 1;
    }
    let mut j = index + 1;
    while j + 1 < keys.len() {
        let span = (keys[j + 1].value - keys[j].value).abs();
        if span > VALUE_EPSILON {
            return span;
        }
        j += 1;
    }
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::ease;

    #[test]
    fn small_graph_x_round_trips() {
        for value in [0.0, 0.25, 0.5, 0.9, 1.0] {
            let back = from_svg_x(to_svg_x(value));
            assert!((back - value).abs() < 1e-6, "{value} -> {back}");
        }
    }

    #[test]
    fn small_graph_y_round_trips_inside_clamp() {
        for value in [-0.4, 0.0, 0.5, 1.0, 1.4] {
            let back = from_svg_y(to_svg_y(value));
            assert!((back - value).abs() < 1e-6, "{value} -> {back}");
        }
    }

    #[test]
    fn axis_maps_and_inverts_round_trip() {
        let time = Axis::new(0.0, 120_000.0, 400.0, 20.0, false);
        for tick in [0.0, 30_000.0, 60_000.0, 120_000.0] {
            let back = time.to_value(time.to_px(tick));
            assert!((back - tick).abs() < 1e-3, "time {tick} -> {back}");
        }
        let value = Axis::new(-100.0, 100.0, 200.0, 12.0, true);

        assert!(value.to_px(100.0) < value.to_px(-100.0));
        for raw in [-100.0, -25.0, 0.0, 75.0, 100.0] {
            let back = value.to_value(value.to_px(raw));
            assert!((back - raw).abs() < 1e-3, "value {raw} -> {back}");
        }
    }

    #[test]
    fn normalized_curve_and_handles_round_trip() {
        let (lt, lv, rt, rv) = (0_i64, 0.0_f64, 120_000_i64, 200.0_f64);
        let curve: Curve = [0.25, 0.1, 0.25, 1.0];
        let (rh, lh) = curve_handles_for_normalized_bezier(lt, lv, rt, rv, curve, None).unwrap();
        let back = normalized_bezier_for_segment(lt, lv, rt, rv, Some(rh), Some(lh), None).unwrap();
        for (a, b) in curve.iter().zip(back.iter()) {
            assert!((a - b).abs() < 1e-3, "curve {a} != {b}");
        }
    }

    #[test]
    fn default_handles_yield_a_straight_line() {
        let (lt, lv, rt, rv) = (0_i64, 10.0_f64, 90_000_i64, 40.0_f64);
        let curve = normalized_bezier_for_segment(lt, lv, rt, rv, None, None, None).unwrap();
        assert!((curve[0] - 1.0 / 3.0).abs() < 1e-6, "{curve:?}");
        assert!((curve[2] - 2.0 / 3.0).abs() < 1e-6, "{curve:?}");
        for step in 0..=10 {
            let t = step as f64 / 10.0;
            let tick = (t * rt as f64).round() as i64;
            let got = eval_segment(
                Segment {
                    left_tick: lt,
                    left_value: lv,
                    right_tick: rt,
                    right_value: rv,
                    left_right_handle: None,
                    right_left_handle: None,
                },
                "bezier",
                tick,
            );
            let want = lv + (rv - lv) * t;
            assert!((got - want).abs() < 1e-2, "t={t}: {got} vs {want}");
        }
    }

    #[test]
    fn a_flat_segment_uses_the_reference_span() {
        assert!(normalized_bezier_for_segment(0, 5.0, 100, 5.0, None, None, None).is_none());
        let curve = normalized_bezier_for_segment(0, 5.0, 100, 5.0, None, None, Some(2.0));
        assert!(curve.is_some());
    }

    #[test]
    fn segment_eval_matches_interaction_ease() {
        let curve: Curve = [0.42, 0.0, 0.58, 1.0];
        let (rt, rv) = (1000_i64, 1.0_f64);
        let (rh, lh) = curve_handles_for_normalized_bezier(0, 0.0, rt, rv, curve, None).unwrap();
        for step in 0..=20 {
            let t = step as f64 / 20.0;
            let tick = (t * rt as f64).round() as i64;
            let got = eval_segment(
                Segment {
                    left_tick: 0,
                    left_value: 0.0,
                    right_tick: rt,
                    right_value: rv,
                    left_right_handle: Some(rh),
                    right_left_handle: Some(lh),
                },
                "bezier",
                tick,
            );
            let want = ease(
                [
                    curve[0] as f32,
                    curve[1] as f32,
                    curve[2] as f32,
                    curve[3] as f32,
                ],
                t as f32,
            ) as f64;
            assert!((got - want).abs() < 5e-3, "t={t}: got {got} want {want}");
        }
    }

    #[test]
    fn a_preset_produces_a_matching_curve() {
        let pop = BUILTIN_PRESETS.iter().find(|p| p.id == "pop").unwrap();
        assert_eq!(matching_preset(pop.value).map(|p| p.id), Some("pop"));

        assert!(matching_preset([0.5, 0.5, 0.5, 0.5]).is_none());
    }

    #[test]
    fn a_linear_curve_clears_handles() {
        let patches = build_curve_patches(
            SegmentEnd {
                keyframe_id: "a",
                tick: 0,
                value: 0.0,
            },
            SegmentEnd {
                keyframe_id: "b",
                tick: 1000,
                value: 1.0,
            },
            LINEAR_CURVE,
            None,
        )
        .unwrap();
        assert_eq!(patches[0].segment_to_next, Some("linear"));
        assert_eq!(patches[0].right_handle, Some(None));
        assert_eq!(patches[1].left_handle, Some(None));
    }

    #[test]
    fn a_bezier_curve_stores_solved_handles() {
        let curve: Curve = [0.25, 0.1, 0.25, 1.0];
        let patches = build_curve_patches(
            SegmentEnd {
                keyframe_id: "a",
                tick: 0,
                value: 0.0,
            },
            SegmentEnd {
                keyframe_id: "b",
                tick: 120_000,
                value: 200.0,
            },
            curve,
            None,
        )
        .unwrap();
        assert_eq!(patches[0].segment_to_next, Some("bezier"));
        let Some(Some(rh)) = patches[0].right_handle else {
            panic!("no right handle");
        };

        assert!((rh.dt_ticks - 30_000).abs() <= 1, "dt {}", rh.dt_ticks);
        assert!((rh.dv - 20.0).abs() < 1e-6, "dv {}", rh.dv);
    }

    #[test]
    fn dragging_a_keyframe_moves_time_and_value() {
        let time = Axis::new(0.0, 120_000.0, 400.0, 0.0, false);
        let value = Axis::new(0.0, 100.0, 200.0, 0.0, true);

        let (tick, val) = drag_keyframe(0, 0.0, 100.0, -20.0, &time, &value);
        assert!((tick - 30_000).abs() <= 1, "tick {tick}");

        assert!((val - 10.0).abs() < 1e-6, "val {val}");
    }

    #[test]
    fn handle_drag_snaps_to_endpoints() {
        let near_top_y = to_svg_y(0.98);
        let curve = drag_curve_handle(LINEAR_CURVE, 0, to_svg_x(0.3), near_top_y, false);
        assert_eq!(curve[1], 1.0);

        let curve = drag_curve_handle(LINEAR_CURVE, 0, to_svg_x(0.3), near_top_y, true);
        assert!((curve[1] - 0.98).abs() < 1e-6);
    }

    #[test]
    fn plot_tracks_reads_scalar_value_channels() {
        use serde_json::json;
        let mut animations = ElementAnimations::default();
        animations.bindings.insert(
            "opacity".to_string(),
            json!({
                "path": "opacity",
                "kind": "number",
                "components": [{ "key": "value", "channelId": "opacity:value" }],
            }),
        );
        animations.channels.insert(
            "opacity:value".to_string(),
            AnimationChannel::Scalar {
                keys: vec![
                    ScalarAnimationKey {
                        id: "k0".into(),
                        time: MediaTime::from_ticks(0),
                        value: 0.0,
                        left_handle: None,
                        right_handle: None,
                        segment_to_next: "linear".into(),
                        tangent_mode: "flat".into(),
                    },
                    ScalarAnimationKey {
                        id: "k1".into(),
                        time: MediaTime::from_ticks(120_000),
                        value: 1.0,
                        left_handle: None,
                        right_handle: None,
                        segment_to_next: "linear".into(),
                        tangent_mode: "flat".into(),
                    },
                ],
                extrapolation: None,
            },
        );
        let tracks = plot_tracks(&animations);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].path, "opacity");
        assert_eq!(tracks[0].keys.len(), 2);
        assert_eq!(reference_span_value(&tracks[0].keys, 0), 1.0);
    }
}
