use cutix_project::model::{MotionSettings, Transform};
use time::MediaTime;

use crate::tracking::TICKS_PER_SECOND;

pub const MOTION_MIN_DURATION: i64 = (TICKS_PER_SECOND as f64 * 0.2) as i64;
pub const MOTION_MAX_DURATION: i64 = TICKS_PER_SECOND * 60;
pub const MOTION_DEFAULT_DURATION: i64 = TICKS_PER_SECOND * 3;
pub const MOTION_MIN_INTENSITY: f64 = 0.1;
pub const MOTION_MAX_INTENSITY: f64 = 2.0;
pub const MOTION_DEFAULT_INTENSITY: f64 = 1.0;

const MAX_ZOOM_FACTOR: f64 = 4.0;
const SCALE_STEP: f64 = 0.01;
const COVER_MARGIN_PX: f64 = 2.0;

pub const MOTION_PROPERTY_PATHS: &[&str] = &[
    "transform.scaleX",
    "transform.scaleY",
    "transform.positionX",
    "transform.positionY",
];

const SHAKE_OFFSETS: &[f64] = &[
    0.0, 0.08, 0.17, 0.25, 0.33, 0.42, 0.5, 0.58, 0.67, 0.75, 0.83, 0.92, 1.0,
];
const SHAKE_ZOOM: &[f64] = &[
    1.06, 1.06, 1.06, 1.06, 1.06, 1.06, 1.06, 1.06, 1.06, 1.06, 1.06, 1.06, 1.06,
];
const SHAKE_X: &[f64] = &[
    0.0, 0.9, -0.7, 1.0, -0.55, 0.75, -1.0, 0.5, -0.85, 0.65, -0.45, 0.3, 0.0,
];
const SHAKE_Y: &[f64] = &[
    0.0, -0.6, 0.85, -0.4, 0.95, -0.8, 0.45, -1.0, 0.6, -0.35, 0.8, -0.5, 0.0,
];

pub struct MotionPresetSpec {
    pub offsets: &'static [f64],
    pub zoom: &'static [f64],
    pub x: &'static [f64],
    pub y: &'static [f64],
    pub amplitude: f64,
}

pub struct MotionPreset {
    pub id: &'static str,
    pub name_key: &'static str,
    pub spec: MotionPresetSpec,
}

pub const MOTION_PRESETS: &[MotionPreset] = &[
    MotionPreset {
        id: "zoom-in",
        name_key: "motion.preset.zoomIn",
        spec: MotionPresetSpec {
            offsets: &[0.0, 1.0],
            zoom: &[1.0, 1.25],
            x: &[],
            y: &[],
            amplitude: 1.0,
        },
    },
    MotionPreset {
        id: "zoom-out",
        name_key: "motion.preset.zoomOut",
        spec: MotionPresetSpec {
            offsets: &[0.0, 1.0],
            zoom: &[1.25, 1.0],
            x: &[],
            y: &[],
            amplitude: 1.0,
        },
    },
    MotionPreset {
        id: "pan-left",
        name_key: "motion.preset.panLeft",
        spec: MotionPresetSpec {
            offsets: &[0.0, 1.0],
            zoom: &[1.18, 1.18],
            x: &[0.08, -0.08],
            y: &[],
            amplitude: 1.0,
        },
    },
    MotionPreset {
        id: "pan-right",
        name_key: "motion.preset.panRight",
        spec: MotionPresetSpec {
            offsets: &[0.0, 1.0],
            zoom: &[1.18, 1.18],
            x: &[-0.08, 0.08],
            y: &[],
            amplitude: 1.0,
        },
    },
    MotionPreset {
        id: "pan-up",
        name_key: "motion.preset.panUp",
        spec: MotionPresetSpec {
            offsets: &[0.0, 1.0],
            zoom: &[1.18, 1.18],
            x: &[],
            y: &[0.08, -0.08],
            amplitude: 1.0,
        },
    },
    MotionPreset {
        id: "pan-down",
        name_key: "motion.preset.panDown",
        spec: MotionPresetSpec {
            offsets: &[0.0, 1.0],
            zoom: &[1.18, 1.18],
            x: &[],
            y: &[-0.08, 0.08],
            amplitude: 1.0,
        },
    },
    MotionPreset {
        id: "zoom-in-pan-left",
        name_key: "motion.preset.zoomInPanLeft",
        spec: MotionPresetSpec {
            offsets: &[0.0, 1.0],
            zoom: &[1.0, 1.3],
            x: &[0.07, -0.07],
            y: &[],
            amplitude: 1.0,
        },
    },
    MotionPreset {
        id: "zoom-out-pan-right",
        name_key: "motion.preset.zoomOutPanRight",
        spec: MotionPresetSpec {
            offsets: &[0.0, 1.0],
            zoom: &[1.3, 1.0],
            x: &[-0.07, 0.07],
            y: &[],
            amplitude: 1.0,
        },
    },
    MotionPreset {
        id: "ken-burns",
        name_key: "motion.preset.kenBurns",
        spec: MotionPresetSpec {
            offsets: &[0.0, 1.0],
            zoom: &[1.0, 1.22],
            x: &[0.06, -0.06],
            y: &[0.035, -0.035],
            amplitude: 1.0,
        },
    },
    MotionPreset {
        id: "photo-motion",
        name_key: "motion.preset.photoMotion",
        spec: MotionPresetSpec {
            offsets: &[0.0, 0.5, 1.0],
            zoom: &[1.02, 1.07, 1.12],
            x: &[-0.03, 0.0, 0.03],
            y: &[0.02, 0.005, -0.02],
            amplitude: 1.0,
        },
    },
    MotionPreset {
        id: "pulse",
        name_key: "motion.preset.pulse",
        spec: MotionPresetSpec {
            offsets: &[0.0, 0.5, 1.0],
            zoom: &[1.0, 1.14, 1.0],
            x: &[],
            y: &[],
            amplitude: 1.0,
        },
    },
    MotionPreset {
        id: "shake",
        name_key: "motion.preset.shake",
        spec: MotionPresetSpec {
            offsets: SHAKE_OFFSETS,
            zoom: SHAKE_ZOOM,
            x: SHAKE_X,
            y: SHAKE_Y,
            amplitude: 0.022,
        },
    },
];

pub fn find_preset(id: &str) -> Option<&'static MotionPreset> {
    MOTION_PRESETS.iter().find(|preset| preset.id == id)
}

pub fn clamp_motion_duration(duration: i64, element_duration: i64) -> i64 {
    let max = MOTION_MAX_DURATION
        .min(element_duration)
        .max(MOTION_MIN_DURATION);
    duration.clamp(MOTION_MIN_DURATION, max)
}

pub fn clamp_motion_intensity(intensity: f64) -> f64 {
    intensity.clamp(MOTION_MIN_INTENSITY, MOTION_MAX_INTENSITY)
}

#[derive(Clone, Debug, PartialEq)]
pub struct MotionGeometry {
    pub factors: Vec<f64>,
    pub offsets_x: Vec<f64>,
    pub offsets_y: Vec<f64>,
}

pub struct GeometryRequest<'a> {
    pub transform: &'a Transform,
    pub canvas_width: f64,
    pub canvas_height: f64,
    pub source_width: f64,
    pub source_height: f64,
    pub intensity: f64,
}

fn at(values: &[f64], index: usize, amplitude: f64) -> f64 {
    values.get(index).copied().unwrap_or(0.0) * amplitude
}

pub fn build_motion_geometry(
    spec: &MotionPresetSpec,
    request: &GeometryRequest<'_>,
) -> MotionGeometry {
    let count = spec.offsets.len();
    if count == 0 || request.source_width <= 0.0 || request.source_height <= 0.0 {
        return MotionGeometry {
            factors: Vec::new(),
            offsets_x: Vec::new(),
            offsets_y: Vec::new(),
        };
    }

    let contain = (request.canvas_width / request.source_width)
        .min(request.canvas_height / request.source_height);
    let scale_x = if request.transform.scale_x == 0.0 {
        1.0
    } else {
        request.transform.scale_x
    };
    let scale_y = if request.transform.scale_y == 0.0 {
        1.0
    } else {
        request.transform.scale_y
    };
    let base_width = request.source_width * contain * scale_x.abs();
    let base_height = request.source_height * contain * scale_y.abs();
    let base_position_x = request.transform.position.x.abs();
    let base_position_y = request.transform.position.y.abs();

    let zoom_curve: Vec<f64> = (0..count)
        .map(|index| 1.0 + (spec.zoom.get(index).copied().unwrap_or(1.0) - 1.0) * request.intensity)
        .collect();
    let raw_x: Vec<f64> = (0..count)
        .map(|index| at(spec.x, index, spec.amplitude) * request.intensity * request.canvas_width)
        .collect();
    let raw_y: Vec<f64> = (0..count)
        .map(|index| at(spec.y, index, spec.amplitude) * request.intensity * request.canvas_height)
        .collect();

    let wanted_x = raw_x
        .iter()
        .fold(0.0f64, |peak, value| peak.max(value.abs()));
    let wanted_y = raw_y
        .iter()
        .fold(0.0f64, |peak, value| peak.max(value.abs()));

    let required_factor = if base_width <= 0.0 || base_height <= 0.0 {
        1.0
    } else {
        1.0f64
            .max(
                (request.canvas_width + 2.0 * (base_position_x + wanted_x) + COVER_MARGIN_PX)
                    / base_width,
            )
            .max(
                (request.canvas_height + 2.0 * (base_position_y + wanted_y) + COVER_MARGIN_PX)
                    / base_height,
            )
    };
    let smallest_zoom = zoom_curve.iter().copied().fold(f64::INFINITY, f64::min);
    let lift = (required_factor - smallest_zoom).max(0.0);

    let factors: Vec<f64> = zoom_curve
        .iter()
        .map(|zoom| {
            let snapped = ((zoom + lift) / SCALE_STEP - 1e-9).ceil() * SCALE_STEP;
            snapped.min(MAX_ZOOM_FACTOR)
        })
        .collect();

    let smallest = factors.iter().copied().fold(f64::INFINITY, f64::min);
    let slack_x = ((base_width * smallest - request.canvas_width) / 2.0 - base_position_x).max(0.0);
    let slack_y =
        ((base_height * smallest - request.canvas_height) / 2.0 - base_position_y).max(0.0);
    let travel_x = if wanted_x > slack_x && wanted_x > 0.0 {
        slack_x / wanted_x
    } else {
        1.0
    };
    let travel_y = if wanted_y > slack_y && wanted_y > 0.0 {
        slack_y / wanted_y
    } else {
        1.0
    };

    MotionGeometry {
        factors,
        offsets_x: raw_x
            .iter()
            .map(|value| (value * travel_x).trunc())
            .collect(),
        offsets_y: raw_y
            .iter()
            .map(|value| (value * travel_y).trunc())
            .collect(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionKeyframe {
    pub time: i64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub position_x: f64,
    pub position_y: f64,
}

pub struct MotionPatch {
    pub keyframes: Vec<MotionKeyframe>,
    pub settings: MotionSettings,
}

pub struct PatchRequest<'a> {
    pub preset_id: &'a str,
    pub transform: &'a Transform,
    pub element_duration: i64,
    pub canvas_width: f64,
    pub canvas_height: f64,
    pub source_width: f64,
    pub source_height: f64,
    pub duration: Option<i64>,
    pub intensity: Option<f64>,
}

pub fn build_motion_patch(request: &PatchRequest<'_>) -> Option<MotionPatch> {
    let preset = find_preset(request.preset_id)?;
    let window = clamp_motion_duration(
        request.duration.unwrap_or(MOTION_DEFAULT_DURATION),
        request.element_duration,
    );
    let intensity = clamp_motion_intensity(request.intensity.unwrap_or(MOTION_DEFAULT_INTENSITY));

    let geometry = build_motion_geometry(
        &preset.spec,
        &GeometryRequest {
            transform: request.transform,
            canvas_width: request.canvas_width,
            canvas_height: request.canvas_height,
            source_width: request.source_width,
            source_height: request.source_height,
            intensity,
        },
    );

    let keyframes = preset
        .spec
        .offsets
        .iter()
        .enumerate()
        .map(|(index, offset)| MotionKeyframe {
            time: (offset * window as f64).round() as i64,
            scale_x: request.transform.scale_x * geometry.factors[index],
            scale_y: request.transform.scale_y * geometry.factors[index],
            position_x: request.transform.position.x + geometry.offsets_x[index],
            position_y: request.transform.position.y + geometry.offsets_y[index],
        })
        .collect();

    Some(MotionPatch {
        keyframes,
        settings: MotionSettings {
            preset_id: preset.id.to_string(),
            duration: MediaTime::from_ticks(window),
            intensity,
        },
    })
}

/// Only the tests in this file ask for this; compiled for them alone so the shipping
/// binary does not carry something nothing calls.
#[cfg(test)]
pub fn sample_at(keyframes: &[MotionKeyframe], time: i64) -> Option<MotionKeyframe> {
    let first = keyframes.first()?;
    let last = keyframes.last()?;
    if time <= first.time {
        return Some(*first);
    }
    if time >= last.time {
        return Some(*last);
    }
    let index = keyframes.iter().position(|key| key.time > time)?;
    let before = keyframes[index - 1];
    let after = keyframes[index];
    let span = (after.time - before.time) as f64;
    let fraction = if span <= 0.0 {
        0.0
    } else {
        (time - before.time) as f64 / span
    };
    let lerp = |a: f64, b: f64| a + (b - a) * fraction;
    Some(MotionKeyframe {
        time,
        scale_x: lerp(before.scale_x, after.scale_x),
        scale_y: lerp(before.scale_y, after.scale_y),
        position_x: lerp(before.position_x, after.position_x),
        position_y: lerp(before.position_y, after.position_y),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutix_i18n::t;
    use cutix_project::model::Vector2;

    fn transform() -> Transform {
        Transform {
            scale_x: 1.0,
            scale_y: 1.0,
            rotate: 0.0,
            position: Vector2 { x: 0.0, y: 0.0 },
        }
    }

    fn patch(preset_id: &str) -> MotionPatch {
        build_motion_patch(&PatchRequest {
            preset_id,
            transform: &transform(),
            element_duration: TICKS_PER_SECOND * 10,
            canvas_width: 1920.0,
            canvas_height: 1080.0,
            source_width: 1920.0,
            source_height: 1080.0,
            duration: None,
            intensity: None,
        })
        .expect("preset")
    }

    #[test]
    fn every_advertised_preset_exists_exactly_once() {
        for id in [
            "zoom-in",
            "zoom-out",
            "pan-left",
            "pan-right",
            "pan-up",
            "pan-down",
            "zoom-in-pan-left",
            "zoom-out-pan-right",
            "ken-burns",
            "photo-motion",
            "pulse",
            "shake",
        ] {
            assert_eq!(
                MOTION_PRESETS
                    .iter()
                    .filter(|preset| preset.id == id)
                    .count(),
                1,
                "{id}"
            );
        }
        assert_eq!(MOTION_PRESETS.len(), 12);
    }

    #[test]
    fn every_preset_table_is_rectangular() {
        for preset in MOTION_PRESETS {
            let count = preset.spec.offsets.len();
            assert!(count >= 2, "{}: too few offsets", preset.id);
            assert_eq!(preset.spec.zoom.len(), count, "{}: zoom", preset.id);
            assert!(
                preset.spec.x.is_empty() || preset.spec.x.len() == count,
                "{}: x",
                preset.id
            );
            assert!(
                preset.spec.y.is_empty() || preset.spec.y.len() == count,
                "{}: y",
                preset.id
            );
            assert!(
                (preset.spec.offsets[0] - 0.0).abs() < 1e-9,
                "{}: starts late",
                preset.id
            );
            assert!(
                (preset.spec.offsets[count - 1] - 1.0).abs() < 1e-9,
                "{}: ends early",
                preset.id
            );

            for pair in preset.spec.offsets.windows(2) {
                assert!(pair[1] > pair[0], "{}: offsets not sorted", preset.id);
            }
        }
    }

    #[test]
    fn unknown_presets_produce_no_patch() {
        assert!(build_motion_patch(&PatchRequest {
            preset_id: "nonsense",
            transform: &transform(),
            element_duration: TICKS_PER_SECOND,
            canvas_width: 1920.0,
            canvas_height: 1080.0,
            source_width: 1920.0,
            source_height: 1080.0,
            duration: None,
            intensity: None,
        })
        .is_none());
    }

    #[test]
    fn zoom_in_grows_the_scale_from_start_to_end() {
        let patch = patch("zoom-in");
        assert_eq!(patch.keyframes.len(), 2);
        let start = patch.keyframes[0];
        let end = patch.keyframes[1];
        assert_eq!(start.time, 0);
        assert_eq!(end.time, MOTION_DEFAULT_DURATION);

        assert!((start.scale_x - 1.01).abs() < 1e-9, "{}", start.scale_x);
        assert!((end.scale_x - 1.26).abs() < 1e-9, "{}", end.scale_x);

        let middle = sample_at(&patch.keyframes, MOTION_DEFAULT_DURATION / 2).expect("mid");
        assert!((middle.scale_x - 1.135).abs() < 1e-6, "{}", middle.scale_x);
    }

    #[test]
    fn zoom_out_is_zoom_in_reversed() {
        let patch = patch("zoom-out");
        assert!((patch.keyframes[0].scale_x - 1.26).abs() < 1e-9);
        assert!((patch.keyframes[1].scale_x - 1.01).abs() < 1e-9);
    }

    #[test]
    fn pulse_returns_to_where_it_started() {
        let patch = patch("pulse");
        assert_eq!(patch.keyframes.len(), 3);
        let start = patch.keyframes[0];
        let middle = patch.keyframes[1];
        let end = patch.keyframes[2];
        assert_eq!(middle.time, MOTION_DEFAULT_DURATION / 2);
        assert!((start.scale_x - end.scale_x).abs() < 1e-9);
        assert!(middle.scale_x > start.scale_x);

        assert!((middle.scale_x - 1.15).abs() < 1e-9, "{}", middle.scale_x);
    }

    #[test]
    fn the_pan_presets_travel_in_the_direction_they_name() {
        let left = patch("pan-left");
        assert!(
            left.keyframes[1].position_x < left.keyframes[0].position_x,
            "pan-left went right"
        );
        let right = patch("pan-right");
        assert!(right.keyframes[1].position_x > right.keyframes[0].position_x);
        let up = patch("pan-up");
        assert!(up.keyframes[1].position_y < up.keyframes[0].position_y);
        let down = patch("pan-down");
        assert!(down.keyframes[1].position_y > down.keyframes[0].position_y);

        assert!((left.keyframes[0].scale_x - left.keyframes[1].scale_x).abs() < 1e-9);
    }

    #[test]
    fn ken_burns_zooms_and_drifts_on_both_axes() {
        let patch = patch("ken-burns");
        let start = patch.keyframes[0];
        let end = patch.keyframes[1];
        assert!(end.scale_x > start.scale_x, "ken-burns did not zoom");
        assert!(end.position_x < start.position_x, "ken-burns did not pan");
        assert!(end.position_y < start.position_y, "ken-burns did not tilt");

        let middle = sample_at(&patch.keyframes, MOTION_DEFAULT_DURATION / 2).expect("mid");
        assert!(
            (middle.position_x - (start.position_x + end.position_x) / 2.0).abs() < 1e-6,
            "{}",
            middle.position_x
        );
    }

    #[test]
    fn photo_motion_walks_through_three_rising_zoom_steps() {
        let patch = patch("photo-motion");
        assert_eq!(patch.keyframes.len(), 3);
        assert!(patch.keyframes[0].scale_x < patch.keyframes[1].scale_x);
        assert!(patch.keyframes[1].scale_x < patch.keyframes[2].scale_x);

        assert!(patch.keyframes[0].position_x < patch.keyframes[2].position_x);
        assert!(patch.keyframes[0].position_y > patch.keyframes[2].position_y);
    }

    #[test]
    fn shake_starts_and_ends_at_rest_and_alternates_between() {
        let patch = patch("shake");
        assert_eq!(patch.keyframes.len(), 13);
        let start = patch.keyframes[0];
        let end = patch.keyframes[12];
        assert!((start.position_x - end.position_x).abs() < 1e-9);
        assert!((start.position_y - end.position_y).abs() < 1e-9);

        let mut sign_changes = 0;
        for pair in patch.keyframes.windows(2) {
            let delta = pair[1].position_x - pair[0].position_x;
            if delta != 0.0 {
                sign_changes += 1;
            }
        }
        assert!(sign_changes >= 10, "shake barely moved: {sign_changes}");
        assert!(
            patch.keyframes.iter().any(|key| key.position_x > 0.0)
                && patch.keyframes.iter().any(|key| key.position_x < 0.0),
            "shake never crossed the centre"
        );
    }

    #[test]
    fn a_pan_lifts_the_zoom_until_the_frame_still_covers_the_canvas() {
        let patch = patch("pan-left");

        let factor = patch.keyframes[0].scale_x;
        assert!(factor >= 1.18, "factor {factor} was not lifted");
        let travel = (patch.keyframes[0].position_x - patch.keyframes[1].position_x).abs() / 2.0;
        let displayed = 1920.0 * factor;
        let slack = (displayed - 1920.0) / 2.0;
        assert!(
            travel <= slack + 1e-6,
            "pan travelled {travel} into {slack} of slack"
        );
    }

    #[test]
    fn factors_snap_to_one_percent_steps() {
        for preset in MOTION_PRESETS {
            let patch = patch(preset.id);
            for key in &patch.keyframes {
                let steps = key.scale_x / SCALE_STEP;
                assert!(
                    (steps - steps.round()).abs() < 1e-6,
                    "{}: {} is not a 1% step",
                    preset.id,
                    key.scale_x
                );
            }
        }
    }

    #[test]
    fn pixel_offsets_are_whole_pixels() {
        for preset in MOTION_PRESETS {
            let patch = patch(preset.id);
            for key in &patch.keyframes {
                assert!(
                    (key.position_x - key.position_x.trunc()).abs() < 1e-9,
                    "{}: {} is not a whole pixel",
                    preset.id,
                    key.position_x
                );
                assert!((key.position_y - key.position_y.trunc()).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn no_preset_ever_exposes_the_canvas_edge() {
        for preset in MOTION_PRESETS {
            let patch = patch(preset.id);
            for key in &patch.keyframes {
                let half_width = 1920.0 * key.scale_x / 2.0;
                let half_height = 1080.0 * key.scale_y / 2.0;
                assert!(
                    half_width - key.position_x.abs() >= 960.0 - 1e-6,
                    "{}: {key:?} exposed a vertical edge",
                    preset.id
                );
                assert!(
                    half_height - key.position_y.abs() >= 540.0 - 1e-6,
                    "{}: {key:?} exposed a horizontal edge",
                    preset.id
                );
            }
        }
    }

    #[test]
    fn intensity_scales_the_deviation_from_rest() {
        let build = |intensity: f64| {
            build_motion_patch(&PatchRequest {
                preset_id: "zoom-in",
                transform: &transform(),
                element_duration: TICKS_PER_SECOND * 10,
                canvas_width: 1920.0,
                canvas_height: 1080.0,
                source_width: 1920.0,
                source_height: 1080.0,
                duration: None,
                intensity: Some(intensity),
            })
            .expect("preset")
        };

        assert!((build(0.5).keyframes[1].scale_x - 1.13).abs() < 1e-9);
        assert!((build(1.0).keyframes[1].scale_x - 1.26).abs() < 1e-9);
        assert!((build(2.0).keyframes[1].scale_x - 1.51).abs() < 1e-9);

        assert!((build(2.0).keyframes[0].scale_x - 1.01).abs() < 1e-9);
    }

    #[test]
    fn intensity_is_clamped_into_range() {
        let settings = build_motion_patch(&PatchRequest {
            preset_id: "zoom-in",
            transform: &transform(),
            element_duration: TICKS_PER_SECOND * 10,
            canvas_width: 1920.0,
            canvas_height: 1080.0,
            source_width: 1920.0,
            source_height: 1080.0,
            duration: None,
            intensity: Some(99.0),
        })
        .expect("preset")
        .settings;
        assert!((settings.intensity - MOTION_MAX_INTENSITY).abs() < 1e-9);
        assert!((clamp_motion_intensity(0.0) - MOTION_MIN_INTENSITY).abs() < 1e-9);
    }

    #[test]
    fn the_window_never_outruns_the_element() {
        let patch = build_motion_patch(&PatchRequest {
            preset_id: "zoom-in",
            transform: &transform(),
            element_duration: TICKS_PER_SECOND,
            canvas_width: 1920.0,
            canvas_height: 1080.0,
            source_width: 1920.0,
            source_height: 1080.0,
            duration: None,
            intensity: None,
        })
        .expect("preset");
        assert_eq!(patch.settings.duration.as_ticks(), TICKS_PER_SECOND);
        assert_eq!(patch.keyframes[1].time, TICKS_PER_SECOND);
    }

    #[test]
    fn durations_are_clamped_at_both_ends() {
        assert_eq!(
            clamp_motion_duration(0, TICKS_PER_SECOND * 10),
            MOTION_MIN_DURATION
        );
        assert_eq!(
            clamp_motion_duration(TICKS_PER_SECOND * 999, TICKS_PER_SECOND * 10),
            TICKS_PER_SECOND * 10
        );

        assert_eq!(clamp_motion_duration(1000, 1000), MOTION_MIN_DURATION);
    }

    #[test]
    fn the_settings_round_trip_the_preset_id() {
        for preset in MOTION_PRESETS {
            assert_eq!(patch(preset.id).settings.preset_id, preset.id);
        }
    }

    #[test]
    fn the_curve_holds_flat_outside_the_window() {
        let patch = patch("zoom-in");
        let before = sample_at(&patch.keyframes, -5).expect("before");
        let after = sample_at(&patch.keyframes, MOTION_DEFAULT_DURATION * 3).expect("after");
        assert!((before.scale_x - patch.keyframes[0].scale_x).abs() < 1e-9);
        assert!((after.scale_x - patch.keyframes[1].scale_x).abs() < 1e-9);
    }

    #[test]
    fn sampling_an_empty_table_yields_nothing() {
        assert!(sample_at(&[], 0).is_none());
    }

    #[test]
    fn a_clip_that_is_already_scaled_keeps_its_scale_as_the_base() {
        let scaled = Transform {
            scale_x: 2.0,
            scale_y: 2.0,
            rotate: 0.0,
            position: Vector2 { x: 0.0, y: 0.0 },
        };
        let patch = build_motion_patch(&PatchRequest {
            preset_id: "zoom-in",
            transform: &scaled,
            element_duration: TICKS_PER_SECOND * 10,
            canvas_width: 1920.0,
            canvas_height: 1080.0,
            source_width: 1920.0,
            source_height: 1080.0,
            duration: None,
            intensity: None,
        })
        .expect("preset");

        assert!((patch.keyframes[0].scale_x - 2.0).abs() < 1e-9);
        assert!((patch.keyframes[1].scale_x - 2.5).abs() < 1e-9);
    }

    #[test]
    fn every_motion_string_is_translated() {
        for key in [
            "motion.title",
            "motion.preset",
            "motion.duration",
            "motion.intensity",
        ] {
            assert_ne!(t(key), key, "{key}");
        }
        for preset in MOTION_PRESETS {
            assert_ne!(t(preset.name_key), preset.name_key, "{}", preset.name_key);
        }
        assert_ne!(t("common.none"), "common.none");
    }
}
