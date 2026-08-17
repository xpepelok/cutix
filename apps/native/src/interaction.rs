#![allow(dead_code)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use gpui::Hsla;

pub const TRANSITION: Duration = Duration::from_millis(150);
pub const OVERLAY_IN: Duration = Duration::from_millis(150);
pub const OVERLAY_OUT: Duration = Duration::from_millis(110);

pub const EASE_STANDARD: [f32; 4] = [0.4, 0.0, 0.2, 1.0];
pub const EASE_OVERLAY_IN: [f32; 4] = [0.16, 1.0, 0.3, 1.0];

pub fn ease(curve: [f32; 4], t: f32) -> f32 {
    let [x1, y1, x2, y2] = curve;
    let t = t.clamp(0.0, 1.0);

    let bezier = |a: f32, b: f32, u: f32| {
        let v = 1.0 - u;
        3.0 * v * v * u * a + 3.0 * v * u * u * b + u * u * u
    };

    let mut low = 0.0;
    let mut high = 1.0;
    let mut u = t;
    for _ in 0..24 {
        let x = bezier(x1, x2, u);
        if x < t {
            low = u;
        } else {
            high = u;
        }
        u = (low + high) / 2.0;
    }

    bezier(y1, y2, u)
}

pub fn ease_in(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t
}

pub fn mix(from: Hsla, to: Hsla, t: f32) -> Hsla {
    let t = t.clamp(0.0, 1.0);
    let from: gpui::Rgba = from.into();
    let to: gpui::Rgba = to.into();
    gpui::Rgba {
        r: from.r + (to.r - from.r) * t,
        g: from.g + (to.g - from.g) * t,
        b: from.b + (to.b - from.b) * t,
        a: from.a + (to.a - from.a) * t,
    }
    .into()
}

#[cfg(windows)]
pub fn reduced_motion() -> bool {
    use windows::core::BOOL;
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };

    let mut enabled = BOOL(1);
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut enabled as *mut BOOL as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };

    ok.is_ok() && !enabled.as_bool()
}

#[cfg(not(windows))]
pub fn reduced_motion() -> bool {
    false
}

#[derive(Clone, Copy, Debug)]
struct Track {
    target: f32,
    from: f32,
    started: Instant,
}

#[derive(Default)]
pub struct Transitions {
    tracks: HashMap<String, Track>,
    instant: Option<bool>,
}

impl Transitions {
    pub fn new() -> Self {
        Self::default()
    }

    fn instant(&mut self) -> bool {
        *self.instant.get_or_insert_with(reduced_motion)
    }

    pub fn set(&mut self, key: impl Into<String>, on: bool) {
        let target = if on { 1.0 } else { 0.0 };
        let key = key.into();
        let from = self.value(&key);
        if let Some(track) = self.tracks.get(&key) {
            if track.target == target {
                return;
            }
        }
        self.tracks.insert(
            key,
            Track {
                target,
                from,
                started: Instant::now(),
            },
        );
    }

    pub fn value(&self, key: &str) -> f32 {
        let Some(track) = self.tracks.get(key) else {
            return 0.0;
        };
        let elapsed = track.started.elapsed().as_secs_f32() / TRANSITION.as_secs_f32();
        if elapsed >= 1.0 {
            return track.target;
        }
        let progress = ease(EASE_STANDARD, elapsed);
        track.from + (track.target - track.from) * progress
    }

    pub fn eased(&mut self, key: &str) -> f32 {
        if self.instant() {
            return self
                .tracks
                .get(key)
                .map(|track| track.target)
                .unwrap_or(0.0);
        }
        self.value(key)
    }

    pub fn animating(&self) -> bool {
        self.tracks
            .values()
            .any(|track| track.started.elapsed() < TRANSITION && track.from != track.target)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OverlaySide {
    Top,
    Bottom,
    Left,
    Right,
}

impl OverlaySide {
    pub fn origin(self) -> (f32, f32) {
        match self {
            OverlaySide::Top => (0.5, 1.0),
            OverlaySide::Bottom => (0.5, 0.0),
            OverlaySide::Left => (1.0, 0.5),
            OverlaySide::Right => (0.0, 0.5),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OverlayFrame {
    pub opacity: f32,
    pub scale: f32,
    pub offset: f32,
    pub visible: bool,
}

pub const OVERLAY_SCALE_FROM: f32 = 0.95;
pub const OVERLAY_SLIDE_PX: f32 = 8.0;

#[derive(Clone, Copy, Debug)]
enum Phase {
    Closed,
    Opening(Instant),
    Open,
    Closing(Instant),
}

pub const TOOLTIP_DELAY: Duration = Duration::from_millis(500);
pub const TOOLBAR_TOOLTIP_DELAY: Duration = Duration::from_millis(200);

pub struct Tooltips {
    hovered: Option<String>,
    since: Instant,
    shown: Option<String>,
    overlay: Overlay,
    delay: Duration,
}

impl Tooltips {
    pub fn new(delay: Duration) -> Self {
        Self {
            hovered: None,
            since: Instant::now(),
            shown: None,
            overlay: Overlay::new(OverlaySide::Top),
            delay,
        }
    }

    pub fn hover(&mut self, id: &str, hovered: bool) {
        if hovered {
            if self.hovered.as_deref() != Some(id) {
                self.hovered = Some(id.to_string());
                self.since = Instant::now();
            }
        } else if self.hovered.as_deref() == Some(id) {
            self.hovered = None;
            self.overlay.set_open(false);
        }
    }

    pub fn dismiss(&mut self) {
        self.hovered = None;
        self.overlay.set_open(false);
    }

    pub fn tick(&mut self) {
        let Some(hovered) = self.hovered.clone() else {
            return;
        };
        if self.since.elapsed() < self.delay {
            return;
        }
        if self.shown.as_deref() != Some(hovered.as_str()) {
            self.shown = Some(hovered);
            self.overlay.set_open(false);
            self.overlay.set_open(true);
        } else if !self.overlay.is_open() {
            self.overlay.set_open(true);
        }
    }

    pub fn frame_for(&mut self, id: &str) -> Option<OverlayFrame> {
        if self.shown.as_deref() != Some(id) {
            return None;
        }
        let frame = self.overlay.frame();
        frame.visible.then_some(frame)
    }

    pub fn waiting(&self) -> bool {
        self.hovered.is_some() && self.since.elapsed() < self.delay
    }

    pub fn animating(&self) -> bool {
        self.overlay.animating() || self.waiting()
    }
}

fn trace(started: Instant, progress: f32, eased: f32, phase: &str) {
    use std::io::Write;
    use std::sync::{Mutex, OnceLock};

    static SINK: OnceLock<Option<Mutex<std::fs::File>>> = OnceLock::new();

    let sink = SINK.get_or_init(|| {
        let path = std::env::var("CUTIX_TRACE_OVERLAY").ok()?;
        std::fs::File::create(path).ok().map(Mutex::new)
    });

    let Some(sink) = sink else { return };
    if let Ok(mut file) = sink.lock() {
        let _ = writeln!(
            file,
            "overlay {phase} elapsed={:.1}ms progress={progress:.4} eased={eased:.4}",
            started.elapsed().as_secs_f32() * 1000.0
        );
    }
}

pub const DISMISS_GUARD: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, Debug)]
pub struct Overlay {
    phase: Phase,
    instant: bool,
    dismissed: Option<Instant>,
    pub side: OverlaySide,
}

impl Overlay {
    pub fn new(side: OverlaySide) -> Self {
        Self {
            phase: Phase::Closed,
            instant: reduced_motion(),
            dismissed: None,
            side,
        }
    }

    pub fn dismiss(&mut self) {
        if self.is_open() {
            self.dismissed = Some(Instant::now());
        }
        self.set_open(false);
    }

    pub fn is_open(&self) -> bool {
        matches!(self.phase, Phase::Opening(_) | Phase::Open)
    }

    pub fn set_open(&mut self, open: bool) {
        match (open, self.phase) {
            (true, Phase::Closed) | (true, Phase::Closing(_)) => {
                self.phase = if self.instant {
                    Phase::Open
                } else {
                    Phase::Opening(Instant::now())
                };
            }
            (false, Phase::Open) | (false, Phase::Opening(_)) => {
                self.phase = if self.instant {
                    Phase::Closed
                } else {
                    Phase::Closing(Instant::now())
                };
            }
            _ => {}
        }
    }

    pub fn toggle(&mut self) {
        if !self.is_open() {
            if let Some(at) = self.dismissed.take() {
                if at.elapsed() < DISMISS_GUARD {
                    return;
                }
            }
        }
        self.set_open(!self.is_open());
    }

    pub fn frame(&mut self) -> OverlayFrame {
        let hidden = OverlayFrame {
            opacity: 0.0,
            scale: OVERLAY_SCALE_FROM,
            offset: OVERLAY_SLIDE_PX,
            visible: false,
        };
        let shown = OverlayFrame {
            opacity: 1.0,
            scale: 1.0,
            offset: 0.0,
            visible: true,
        };

        match self.phase {
            Phase::Closed => hidden,
            Phase::Open => shown,
            Phase::Opening(started) => {
                let elapsed = started.elapsed().as_secs_f32() / OVERLAY_IN.as_secs_f32();
                if elapsed >= 1.0 {
                    self.phase = Phase::Open;
                    trace(started, 1.0, 1.0, "open");
                    return shown;
                }
                let t = ease(EASE_OVERLAY_IN, elapsed);
                trace(started, elapsed, t, "open");
                OverlayFrame {
                    opacity: t,
                    scale: OVERLAY_SCALE_FROM + (1.0 - OVERLAY_SCALE_FROM) * t,
                    offset: OVERLAY_SLIDE_PX * (1.0 - t),
                    visible: true,
                }
            }
            Phase::Closing(started) => {
                let elapsed = started.elapsed().as_secs_f32() / OVERLAY_OUT.as_secs_f32();
                if elapsed >= 1.0 {
                    self.phase = Phase::Closed;
                    trace(started, 1.0, 1.0, "close");
                    return hidden;
                }
                let t = ease_in(elapsed);
                trace(started, elapsed, t, "close");
                OverlayFrame {
                    opacity: 1.0 - t,
                    scale: 1.0 - (1.0 - OVERLAY_SCALE_FROM) * t,
                    offset: OVERLAY_SLIDE_PX * t,
                    visible: true,
                }
            }
        }
    }

    pub fn animating(&self) -> bool {
        matches!(self.phase, Phase::Opening(_) | Phase::Closing(_))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn mixing_never_wanders_through_a_hue_neither_colour_has() {
        let grey = Hsla {
            h: 0.0,
            s: 0.04,
            l: 0.22,
            a: 1.0,
        };
        let blue = Hsla {
            h: 0.58,
            s: 0.9,
            l: 0.55,
            a: 1.0,
        };
        for step in 0..=20 {
            let blended: gpui::Rgba = mix(grey, blue, step as f32 / 20.0).into();
            assert!(
                blended.g <= blended.b + 1e-3,
                "green led the mix at {step}/20: {blended:?}"
            );
        }
    }

    use super::*;

    #[test]
    fn standard_easing_spans_the_unit_interval() {
        assert!(ease(EASE_STANDARD, 0.0).abs() < 1e-3);
        assert!((ease(EASE_STANDARD, 1.0) - 1.0).abs() < 1e-3);
        assert!(ease(EASE_STANDARD, 0.5) > 0.5);
    }

    #[test]
    fn overlay_easing_decelerates_hard() {
        assert!(ease(EASE_OVERLAY_IN, 0.25) > 0.6);
    }

    #[test]
    fn mixing_walks_from_start_to_end() {
        let a = Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.0,
            a: 0.0,
        };
        let b = Hsla {
            h: 0.0,
            s: 0.0,
            l: 1.0,
            a: 1.0,
        };
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
        assert!((mix(a, b, 0.5).l - 0.5).abs() < 1e-6);
    }

    #[test]
    fn unknown_keys_read_as_rest_state() {
        let transitions = Transitions::new();
        assert_eq!(transitions.value("nope"), 0.0);
        assert!(!transitions.animating());
    }

    #[test]
    fn setting_a_target_starts_an_animation() {
        let mut transitions = Transitions::new();
        transitions.set("button", true);
        assert!(transitions.animating());
        assert!(transitions.value("button") < 1.0);
    }

    #[test]
    fn repeating_the_same_target_does_not_restart() {
        let mut transitions = Transitions::new();
        transitions.set("button", true);
        let first = transitions.value("button");
        transitions.set("button", true);
        assert!(transitions.value("button") >= first);
    }

    #[test]
    fn transition_durations_match_the_web_app() {
        assert_eq!(TRANSITION.as_millis(), 150);
        assert_eq!(OVERLAY_IN.as_millis(), 150);
        assert_eq!(OVERLAY_OUT.as_millis(), 110);
    }

    #[test]
    fn the_standard_curve_never_goes_backwards() {
        let mut previous = -1.0;
        for step in 0..=20000 {
            let value = ease(EASE_STANDARD, step as f32 / 20000.0);
            assert!(
                value >= previous,
                "ease dipped at {step}: {value} after {previous}"
            );
            previous = value;
        }
    }

    #[test]
    fn ease_in_is_monotonic() {
        assert!(ease_in(0.2) < ease_in(0.8));
        assert_eq!(ease_in(1.0), 1.0);
    }

    fn overlay(side: OverlaySide) -> Overlay {
        Overlay {
            phase: Phase::Closed,
            instant: false,
            dismissed: None,
            side,
        }
    }

    #[test]
    fn a_closed_overlay_is_invisible_and_offset() {
        let mut menu = overlay(OverlaySide::Bottom);
        let frame = menu.frame();
        assert!(!frame.visible);
        assert_eq!(frame.opacity, 0.0);
        assert_eq!(frame.scale, OVERLAY_SCALE_FROM);
    }

    #[test]
    fn opening_starts_partway_and_settles_at_rest() {
        let mut menu = overlay(OverlaySide::Bottom);
        menu.set_open(true);

        menu.phase = Phase::Opening(Instant::now() - OVERLAY_IN / 2);
        let frame = menu.frame();
        assert!(frame.visible);
        assert!(frame.opacity > 0.0 && frame.opacity < 1.0);
        assert!(frame.scale >= OVERLAY_SCALE_FROM && frame.scale < 1.0);
        assert!(menu.animating());

        menu.phase = Phase::Opening(Instant::now() - OVERLAY_IN);
        let frame = menu.frame();
        assert_eq!(frame.opacity, 1.0);
        assert_eq!(frame.scale, 1.0);
        assert_eq!(frame.offset, 0.0);
        assert!(!menu.animating());
    }

    #[test]
    fn closing_runs_the_shorter_duration_and_ends_hidden() {
        let mut menu = overlay(OverlaySide::Bottom);
        menu.set_open(true);
        menu.phase = Phase::Open;
        menu.set_open(false);
        assert!(menu.frame().visible);

        menu.phase = Phase::Closing(Instant::now() - OVERLAY_OUT);
        assert!(!menu.frame().visible);
        assert!(!menu.is_open());
    }

    #[test]
    fn reduced_motion_collapses_the_overlay_to_instant() {
        let mut menu = overlay(OverlaySide::Bottom);
        menu.instant = true;
        menu.set_open(true);
        let frame = menu.frame();
        assert_eq!(frame.opacity, 1.0);
        assert_eq!(frame.scale, 1.0);
        assert!(!menu.animating());
        menu.set_open(false);
        assert!(!menu.frame().visible);
    }

    #[test]
    fn transform_origin_is_side_aware() {
        assert_eq!(OverlaySide::Bottom.origin(), (0.5, 0.0));
        assert_eq!(OverlaySide::Top.origin(), (0.5, 1.0));
        assert_eq!(OverlaySide::Left.origin(), (1.0, 0.5));
        assert_eq!(OverlaySide::Right.origin(), (0.0, 0.5));
    }

    #[test]
    fn toggling_reverses_an_in_flight_open() {
        let mut menu = overlay(OverlaySide::Bottom);
        menu.toggle();
        assert!(menu.is_open());
        menu.toggle();
        assert!(!menu.is_open());
        assert!(menu.animating());
    }

    #[test]
    fn clicking_the_trigger_of_an_open_menu_leaves_it_closed() {
        let mut menu = overlay(OverlaySide::Bottom);
        menu.toggle();
        menu.phase = Phase::Open;

        menu.dismiss();
        menu.toggle();
        assert!(
            !menu.is_open(),
            "the trigger must not reopen after dismissal"
        );
    }

    #[test]
    fn the_guard_expires_so_a_later_click_reopens() {
        let mut menu = overlay(OverlaySide::Bottom);
        menu.phase = Phase::Open;
        menu.dismiss();
        menu.dismissed = Some(Instant::now() - DISMISS_GUARD);
        menu.toggle();
        assert!(menu.is_open());
    }

    #[test]
    fn dismissing_a_closed_menu_does_not_arm_the_guard() {
        let mut menu = overlay(OverlaySide::Bottom);
        menu.dismiss();
        menu.toggle();
        assert!(menu.is_open());
    }
}
