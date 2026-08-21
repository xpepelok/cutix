use time::{MediaTime, TICKS_PER_SECOND};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    In,
    Out,
}

impl Direction {
    pub fn key(self) -> &'static str {
        match self {
            Direction::In => "in",
            Direction::Out => "out",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Direction::In => "text.animation.in",
            Direction::Out => "text.animation.out",
        }
    }
}

pub const MIN_DURATION: MediaTime = MediaTime::from_ticks(TICKS_PER_SECOND / 10);
pub const MAX_DURATION: MediaTime = MediaTime::from_ticks(TICKS_PER_SECOND * 5);
pub const DEFAULT_DURATION: MediaTime = MediaTime::from_ticks(TICKS_PER_SECOND / 2);

pub const REVEAL_MIN_DURATION: MediaTime = MediaTime::from_ticks(TICKS_PER_SECOND / 10);
pub const REVEAL_MAX_DURATION: MediaTime = MediaTime::from_ticks(TICKS_PER_SECOND * 10);
pub const REVEAL_DEFAULT_DURATION: MediaTime = MediaTime::from_ticks(TICKS_PER_SECOND * 3 / 2);

const SLIDE_RATIO: f64 = 0.35;

pub const PATH_POSITION_X: &str = "transform.positionX";
pub const PATH_POSITION_Y: &str = "transform.positionY";
pub const PATH_SCALE_X: &str = "transform.scaleX";
pub const PATH_SCALE_Y: &str = "transform.scaleY";
pub const PATH_OPACITY: &str = "opacity";

#[derive(Debug, Clone, Copy)]
enum Shape {
    Fade { from: f64, to: f64 },
    Slide { axis: Axis, sign: f64 },
    Scale(&'static [(f64, f64)]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

pub struct Preset {
    pub id: &'static str,
    pub name_key: &'static str,
    pub direction: Direction,
    shape: Shape,
}

const POP_IN: &[(f64, f64)] = &[(0.0, 0.3), (0.7, 1.12), (1.0, 1.0)];
const POP_OUT: &[(f64, f64)] = &[(0.0, 1.0), (0.3, 1.12), (1.0, 0.3)];
const BOUNCE_IN: &[(f64, f64)] = &[
    (0.0, 0.4),
    (0.45, 1.25),
    (0.7, 0.9),
    (0.87, 1.06),
    (1.0, 1.0),
];

pub const PRESETS: &[Preset] = &[
    Preset {
        id: "fade-in",
        name_key: "text.animation.fadeIn",
        direction: Direction::In,
        shape: Shape::Fade { from: 0.0, to: 1.0 },
    },
    Preset {
        id: "slide-in-left",
        name_key: "text.animation.slideInLeft",
        direction: Direction::In,
        shape: Shape::Slide {
            axis: Axis::X,
            sign: -1.0,
        },
    },
    Preset {
        id: "slide-in-right",
        name_key: "text.animation.slideInRight",
        direction: Direction::In,
        shape: Shape::Slide {
            axis: Axis::X,
            sign: 1.0,
        },
    },
    Preset {
        id: "slide-in-up",
        name_key: "text.animation.slideInUp",
        direction: Direction::In,
        shape: Shape::Slide {
            axis: Axis::Y,
            sign: 1.0,
        },
    },
    Preset {
        id: "slide-in-down",
        name_key: "text.animation.slideInDown",
        direction: Direction::In,
        shape: Shape::Slide {
            axis: Axis::Y,
            sign: -1.0,
        },
    },
    Preset {
        id: "pop-in",
        name_key: "text.animation.popIn",
        direction: Direction::In,
        shape: Shape::Scale(POP_IN),
    },
    Preset {
        id: "bounce-in",
        name_key: "text.animation.bounceIn",
        direction: Direction::In,
        shape: Shape::Scale(BOUNCE_IN),
    },
    Preset {
        id: "fade-out",
        name_key: "text.animation.fadeOut",
        direction: Direction::Out,
        shape: Shape::Fade { from: 1.0, to: 0.0 },
    },
    Preset {
        id: "slide-out-left",
        name_key: "text.animation.slideOutLeft",
        direction: Direction::Out,
        shape: Shape::Slide {
            axis: Axis::X,
            sign: -1.0,
        },
    },
    Preset {
        id: "slide-out-right",
        name_key: "text.animation.slideOutRight",
        direction: Direction::Out,
        shape: Shape::Slide {
            axis: Axis::X,
            sign: 1.0,
        },
    },
    Preset {
        id: "slide-out-up",
        name_key: "text.animation.slideOutUp",
        direction: Direction::Out,
        shape: Shape::Slide {
            axis: Axis::Y,
            sign: -1.0,
        },
    },
    Preset {
        id: "slide-out-down",
        name_key: "text.animation.slideOutDown",
        direction: Direction::Out,
        shape: Shape::Slide {
            axis: Axis::Y,
            sign: 1.0,
        },
    },
    Preset {
        id: "pop-out",
        name_key: "text.animation.popOut",
        direction: Direction::Out,
        shape: Shape::Scale(POP_OUT),
    },
];

#[derive(Debug, Clone, Copy)]
pub struct RevealPreset {
    pub id: &'static str,
    pub name_key: &'static str,
}

pub const REVEAL_PRESETS: &[RevealPreset] = &[
    RevealPreset {
        id: "typewriter",
        name_key: "text.animation.typewriter",
    },
    RevealPreset {
        id: "char-fade",
        name_key: "text.animation.charFade",
    },
    RevealPreset {
        id: "char-pop",
        name_key: "text.animation.charPop",
    },
];

pub fn is_reveal_style(value: &str) -> bool {
    REVEAL_PRESETS.iter().any(|preset| preset.id == value)
}

pub fn presets_for(direction: Direction) -> impl Iterator<Item = &'static Preset> {
    PRESETS
        .iter()
        .filter(move |preset| preset.direction == direction)
}

pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|preset| preset.id == id)
}

#[derive(Debug, Clone, Copy)]
pub struct Base {
    pub position_x: f64,
    pub position_y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub opacity: f64,
    pub canvas_width: f64,
    pub canvas_height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub path: &'static str,
    pub keys: Vec<(f64, f64)>,
}

impl Preset {
    pub fn tracks(&self, base: &Base) -> Vec<Track> {
        match self.shape {
            Shape::Fade { from, to } => vec![Track {
                path: PATH_OPACITY,
                keys: vec![(0.0, base.opacity * from), (1.0, base.opacity * to)],
            }],
            Shape::Slide { axis, sign } => {
                let (path, resting, span) = match axis {
                    Axis::X => (
                        PATH_POSITION_X,
                        base.position_x,
                        base.canvas_width * SLIDE_RATIO,
                    ),
                    Axis::Y => (
                        PATH_POSITION_Y,
                        base.position_y,
                        base.canvas_height * SLIDE_RATIO,
                    ),
                };
                let offscreen = resting + sign * span;

                let position = match self.direction {
                    Direction::In => vec![(0.0, offscreen), (1.0, resting)],
                    Direction::Out => vec![(0.0, resting), (1.0, offscreen)],
                };
                let opacity = match self.direction {
                    Direction::In => vec![(0.0, 0.0), (0.6, base.opacity), (1.0, base.opacity)],
                    Direction::Out => vec![(0.0, base.opacity), (0.4, base.opacity), (1.0, 0.0)],
                };

                vec![
                    Track {
                        path,
                        keys: position,
                    },
                    Track {
                        path: PATH_OPACITY,
                        keys: opacity,
                    },
                ]
            }
            Shape::Scale(factors) => {
                let opacity = match self.direction {
                    Direction::In => vec![(0.0, 0.0), (0.4, base.opacity), (1.0, base.opacity)],
                    Direction::Out => vec![(0.0, base.opacity), (0.6, base.opacity), (1.0, 0.0)],
                };
                vec![
                    Track {
                        path: PATH_SCALE_X,
                        keys: factors
                            .iter()
                            .map(|(offset, factor)| (*offset, base.scale_x * factor))
                            .collect(),
                    },
                    Track {
                        path: PATH_SCALE_Y,
                        keys: factors
                            .iter()
                            .map(|(offset, factor)| (*offset, base.scale_y * factor))
                            .collect(),
                    },
                    Track {
                        path: PATH_OPACITY,
                        keys: opacity,
                    },
                ]
            }
        }
    }
}

pub fn keyframe_id(direction: Direction, path: &str, index: usize) -> String {
    format!("text-anim-{}-{path}-{index}", direction.key())
}

pub fn clamp_duration(duration: MediaTime, element_duration: MediaTime) -> MediaTime {
    let ceiling = MAX_DURATION.min(element_duration).max(MIN_DURATION);
    duration.max(MIN_DURATION).min(ceiling)
}

pub fn clamp_reveal_duration(duration: MediaTime, element_duration: MediaTime) -> MediaTime {
    let ceiling = REVEAL_MAX_DURATION
        .min(element_duration)
        .max(REVEAL_MIN_DURATION);
    duration.max(REVEAL_MIN_DURATION).min(ceiling)
}

pub fn window_start(
    direction: Direction,
    element_duration: MediaTime,
    window: MediaTime,
) -> MediaTime {
    match direction {
        Direction::In => MediaTime::ZERO,
        Direction::Out => (element_duration - window).max(MediaTime::ZERO),
    }
}

pub fn key_time(start: MediaTime, offset: f64, window: MediaTime) -> MediaTime {
    MediaTime::from_ticks(start.as_ticks() + (offset * window.as_ticks() as f64).round() as i64)
}

/// Only the tests in this file ask for this; compiled for them alone so the shipping
/// binary does not carry something nothing calls.
#[cfg(test)]
pub fn character_reveal_progress(progress: f64, index: usize, count: usize, style: &str) -> f64 {
    if count == 0 {
        return 1.0;
    }
    if style == "typewriter" {
        return if (index as f64) < (progress * count as f64 + 1e-6).floor() {
            1.0
        } else {
            0.0
        };
    }

    let spread = if count > 1 { STAGGER_SPREAD } else { 0.0 };
    let start = if count > 1 {
        (index as f64 / (count - 1) as f64) * spread
    } else {
        0.0
    };
    let span = 1.0 - spread;
    if span <= 0.0 {
        return if progress >= start { 1.0 } else { 0.0 };
    }
    ((progress - start) / span).clamp(0.0, 1.0)
}

/// Only the tests in this file ask for this; compiled for them alone so the shipping
/// binary does not carry something nothing calls.
#[cfg(test)]
pub fn character_reveal_scale(progress: f64, style: &str) -> f64 {
    if style != "char-pop" || progress >= 1.0 {
        return 1.0;
    }
    let shifted = progress - 1.0;
    let eased = 1.0
        + (BACK_OVERSHOOT + 1.0) * shifted * shifted * shifted
        + BACK_OVERSHOOT * shifted * shifted;
    POP_MIN_SCALE + (1.0 - POP_MIN_SCALE) * eased
}

/// Only the tests in this file ask for this; compiled for them alone so the shipping
/// binary does not carry something nothing calls.
#[cfg(test)]
const BACK_OVERSHOOT: f64 = 1.70158;

/// Only the tests in this file ask for this; compiled for them alone so the shipping
/// binary does not carry something nothing calls.
#[cfg(test)]
const POP_MIN_SCALE: f64 = 0.35;

/// Only the tests in this file ask for this; compiled for them alone so the shipping
/// binary does not carry something nothing calls.
#[cfg(test)]
const STAGGER_SPREAD: f64 = 0.75;

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Base {
        Base {
            position_x: 0.0,
            position_y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            opacity: 1.0,
            canvas_width: 1920.0,
            canvas_height: 1080.0,
        }
    }

    #[test]
    fn the_preset_list_matches_the_web_ids_and_order() {
        let ids: Vec<&str> = PRESETS.iter().map(|preset| preset.id).collect();
        assert_eq!(
            ids,
            [
                "fade-in",
                "slide-in-left",
                "slide-in-right",
                "slide-in-up",
                "slide-in-down",
                "pop-in",
                "bounce-in",
                "fade-out",
                "slide-out-left",
                "slide-out-right",
                "slide-out-up",
                "slide-out-down",
                "pop-out",
            ]
        );
        assert_eq!(presets_for(Direction::In).count(), 7);
        assert_eq!(presets_for(Direction::Out).count(), 6);
    }

    #[test]
    fn the_reveal_list_matches_the_web() {
        let ids: Vec<&str> = REVEAL_PRESETS.iter().map(|preset| preset.id).collect();
        assert_eq!(ids, ["typewriter", "char-fade", "char-pop"]);
        assert!(is_reveal_style("char-pop"));
        assert!(!is_reveal_style("fade-in"));
    }

    #[test]
    fn fade_in_runs_opacity_from_zero_to_the_resting_value() {
        let mut base = base();
        base.opacity = 0.8;
        let tracks = preset("fade-in").unwrap().tracks(&base);
        assert_eq!(
            tracks,
            vec![Track {
                path: PATH_OPACITY,
                keys: vec![(0.0, 0.0), (1.0, 0.8)],
            }]
        );
    }

    #[test]
    fn slide_in_left_starts_offscreen_and_lands_on_the_resting_position() {
        let mut base = base();
        base.position_x = 100.0;
        let tracks = preset("slide-in-left").unwrap().tracks(&base);
        assert_eq!(tracks[0].path, PATH_POSITION_X);
        assert_eq!(tracks[0].keys[0], (0.0, 100.0 - 1920.0 * 0.35));
        assert_eq!(tracks[0].keys[1], (1.0, 100.0));
        assert_eq!(tracks[1].path, PATH_OPACITY);
        assert_eq!(tracks[1].keys.first(), Some(&(0.0, 0.0)));
    }

    #[test]
    fn slide_out_reverses_the_same_geometry() {
        let tracks = preset("slide-out-right").unwrap().tracks(&base());
        assert_eq!(tracks[0].keys[0], (0.0, 0.0));
        assert_eq!(tracks[0].keys[1], (1.0, 1920.0 * 0.35));
        assert_eq!(tracks[1].keys.last(), Some(&(1.0, 0.0)));
    }

    #[test]
    fn slide_on_the_y_axis_uses_the_canvas_height() {
        let tracks = preset("slide-in-up").unwrap().tracks(&base());
        assert_eq!(tracks[0].path, PATH_POSITION_Y);
        assert_eq!(tracks[0].keys[0], (0.0, 1080.0 * 0.35));
    }

    #[test]
    fn bounce_in_overshoots_past_the_resting_scale_then_settles() {
        let tracks = preset("bounce-in").unwrap().tracks(&base());
        assert_eq!(tracks[0].path, PATH_SCALE_X);
        assert_eq!(tracks[1].path, PATH_SCALE_Y);
        let peak = tracks[0]
            .keys
            .iter()
            .map(|(_, value)| *value)
            .fold(f64::MIN, f64::max);
        assert!(peak > 1.0, "{peak}");
        assert_eq!(tracks[0].keys.last(), Some(&(1.0, 1.0)));
    }

    #[test]
    fn scale_presets_respect_a_non_unit_resting_scale() {
        let mut base = base();
        base.scale_x = 2.0;
        base.scale_y = 3.0;
        let tracks = preset("pop-in").unwrap().tracks(&base);
        assert!((tracks[0].keys[0].1 - 0.6).abs() < 1e-9);
        assert!((tracks[1].keys[0].1 - 0.9).abs() < 1e-9);
        assert_eq!(tracks[0].keys.last(), Some(&(1.0, 2.0)));
    }

    #[test]
    fn keyframe_ids_are_scoped_to_the_direction_path_and_index() {
        assert_eq!(
            keyframe_id(Direction::In, PATH_OPACITY, 1),
            "text-anim-in-opacity-1"
        );
        assert_eq!(
            keyframe_id(Direction::Out, PATH_SCALE_X, 0),
            "text-anim-out-transform.scaleX-0"
        );
    }

    #[test]
    fn durations_clamp_into_the_clip() {
        let clip = MediaTime::from_ticks(TICKS_PER_SECOND * 2);
        assert_eq!(clamp_duration(DEFAULT_DURATION, clip), DEFAULT_DURATION);
        assert_eq!(clamp_duration(MAX_DURATION, clip), clip);
        assert_eq!(clamp_duration(MediaTime::ZERO, clip), MIN_DURATION);

        let tiny = MediaTime::from_ticks(TICKS_PER_SECOND / 100);
        assert_eq!(clamp_duration(DEFAULT_DURATION, tiny), MIN_DURATION);
    }

    #[test]
    fn entrance_windows_sit_at_the_head_and_exits_at_the_tail() {
        let clip = MediaTime::from_ticks(TICKS_PER_SECOND * 4);
        let window = MediaTime::from_ticks(TICKS_PER_SECOND);
        assert_eq!(window_start(Direction::In, clip, window), MediaTime::ZERO);
        assert_eq!(
            window_start(Direction::Out, clip, window),
            MediaTime::from_ticks(TICKS_PER_SECOND * 3)
        );
        assert_eq!(
            window_start(
                Direction::Out,
                window,
                MediaTime::from_ticks(TICKS_PER_SECOND * 9)
            ),
            MediaTime::ZERO
        );
    }

    #[test]
    fn key_times_interpolate_across_the_window() {
        let start = MediaTime::from_ticks(1000);
        let window = MediaTime::from_ticks(400);
        assert_eq!(key_time(start, 0.0, window).as_ticks(), 1000);
        assert_eq!(key_time(start, 0.5, window).as_ticks(), 1200);
        assert_eq!(key_time(start, 1.0, window).as_ticks(), 1400);
    }

    #[test]
    fn the_typewriter_reveals_whole_characters_only() {
        assert_eq!(character_reveal_progress(0.0, 0, 4, "typewriter"), 0.0);
        assert_eq!(character_reveal_progress(0.5, 0, 4, "typewriter"), 1.0);
        assert_eq!(character_reveal_progress(0.5, 2, 4, "typewriter"), 0.0);
        assert_eq!(character_reveal_progress(1.0, 3, 4, "typewriter"), 1.0);
    }

    #[test]
    fn staggered_reveals_start_later_for_later_characters() {
        let early = character_reveal_progress(0.5, 0, 8, "char-fade");
        let late = character_reveal_progress(0.5, 7, 8, "char-fade");
        assert!(early > late, "{early} vs {late}");
        assert_eq!(character_reveal_progress(1.0, 7, 8, "char-fade"), 1.0);
        assert_eq!(character_reveal_progress(0.0, 0, 8, "char-fade"), 0.0);
    }

    #[test]
    fn char_pop_overshoots_then_returns_to_one() {
        assert_eq!(character_reveal_scale(1.0, "char-pop"), 1.0);
        assert_eq!(character_reveal_scale(0.5, "char-fade"), 1.0);
        assert!(character_reveal_scale(0.0, "char-pop") < 1.0);
        let peak = (0..100)
            .map(|step| character_reveal_scale(step as f64 / 100.0, "char-pop"))
            .fold(f64::MIN, f64::max);
        assert!(peak > 1.0, "{peak}");
    }
}
