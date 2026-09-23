use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gpui::{px, Animation, AnimationElement, AnimationExt, ElementId, IntoElement, Styled};

use crate::interaction::{ease, EASE_OVERLAY_IN, EASE_STANDARD};

pub const CARD: Duration = Duration::from_millis(220);
pub const PAGE: Duration = Duration::from_millis(200);
pub const ITEM: Duration = Duration::from_millis(260);
pub const STAGGER: Duration = Duration::from_millis(28);
pub const STAGGER_CAP: usize = 12;

const CARD_RISE_PX: f32 = 14.0;
const PAGE_RISE_PX: f32 = 8.0;
const ITEM_RISE_PX: f32 = 10.0;
const TOAST_RISE_PX: f32 = 12.0;

pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var_os("CUTIX_NO_ANIMATIONS").is_none() && !crate::interaction::reduced_motion()
    })
}

fn settle(progress: f32) -> f32 {
    if enabled() {
        progress
    } else {
        1.0
    }
}

fn out_curve(t: f32) -> f32 {
    ease(EASE_OVERLAY_IN, t)
}

fn standard_curve(t: f32) -> f32 {
    ease(EASE_STANDARD, t)
}

pub fn rise_frame(progress: f32, distance: f32) -> (f32, f32) {
    let progress = settle(progress).clamp(0.0, 1.0);
    let opacity = (progress * 1.6).min(1.0);
    (distance * (1.0 - progress), opacity)
}

pub fn stagger_delay(index: usize) -> Duration {
    STAGGER * index.min(STAGGER_CAP) as u32
}

pub fn modal<E>(element: E, id: impl Into<ElementId>) -> AnimationElement<E>
where
    E: IntoElement + Styled + 'static,
{
    element.with_animation(
        id,
        Animation::new(CARD).with_easing(out_curve),
        |element, progress| {
            let (offset, opacity) = rise_frame(progress, CARD_RISE_PX);
            element.pt(px(offset * 2.0)).opacity(opacity)
        },
    )
}

pub fn page<E>(element: E, id: impl Into<ElementId>) -> AnimationElement<E>
where
    E: IntoElement + Styled + 'static,
{
    rise(element, id, PAGE, PAGE_RISE_PX)
}

pub fn toast<E>(element: E, id: impl Into<ElementId>) -> AnimationElement<E>
where
    E: IntoElement + Styled + 'static,
{
    rise(element, id, CARD, TOAST_RISE_PX)
}

pub fn fade<E>(element: E, id: impl Into<ElementId>, duration: Duration) -> AnimationElement<E>
where
    E: IntoElement + Styled + 'static,
{
    element.with_animation(
        id,
        Animation::new(duration).with_easing(standard_curve),
        |element, progress| element.opacity(settle(progress)),
    )
}

fn rise<E>(
    element: E,
    id: impl Into<ElementId>,
    duration: Duration,
    distance: f32,
) -> AnimationElement<E>
where
    E: IntoElement + Styled + 'static,
{
    element.with_animation(
        id,
        Animation::new(duration).with_easing(out_curve),
        move |element, progress| {
            let (offset, opacity) = rise_frame(progress, distance);
            element.relative().top(px(offset)).opacity(opacity)
        },
    )
}

thread_local! {
    static SLIDES: std::cell::RefCell<std::collections::HashMap<&'static str, Slide>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slide {
    pub from: usize,
    pub to: usize,
    pub generation: u64,
    pub at: Instant,
}

impl Slide {
    fn settled(target: usize, now: Instant) -> Self {
        Self {
            from: target,
            to: target,
            generation: 0,
            at: now,
        }
    }

    fn step(self, target: usize, now: Instant) -> Self {
        if self.to != target {
            return Self {
                from: self.to,
                to: target,
                generation: self.generation + 1,
                at: now,
            };
        }
        if self.from != self.to && now.saturating_duration_since(self.at) >= CARD {
            return Self {
                from: self.to,
                ..self
            };
        }
        self
    }
}

pub fn slide(key: &'static str, target: usize) -> Slide {
    let now = Instant::now();
    SLIDES.with(|slides| {
        let mut slides = slides.borrow_mut();
        let entry = slides
            .entry(key)
            .or_insert_with(|| Slide::settled(target, now));
        *entry = entry.step(target, now);
        *entry
    })
}

pub fn segment_highlight(
    key: &'static str,
    target: usize,
    count: usize,
    highlight: gpui::Div,
) -> AnimationElement<gpui::Div> {
    let count = count.max(1) as f32;
    let moved = slide(key, target);
    let id = ElementId::Name(format!("{key}-slide-{}", moved.generation).into());
    highlight
        .absolute()
        .top_0()
        .bottom_0()
        .w(gpui::relative(1.0 / count))
        .with_animation(
            id,
            Animation::new(CARD).with_easing(out_curve),
            move |element, progress| {
                let progress = settle(progress);
                let at = moved.from as f32 + (moved.to as f32 - moved.from as f32) * progress;
                element.left(gpui::relative(at / count))
            },
        )
}

pub fn item<E>(element: E, id: impl Into<ElementId>, index: usize) -> AnimationElement<E>
where
    E: IntoElement + Styled + 'static,
{
    let delay = stagger_delay(index);
    if delay.is_zero() || !enabled() {
        return rise(element, id, ITEM, ITEM_RISE_PX);
    }
    element.with_animations(
        id,
        vec![
            Animation::new(delay),
            Animation::new(ITEM).with_easing(out_curve),
        ],
        |element, phase, progress| {
            let progress = if phase == 0 { 0.0 } else { progress };
            let (offset, opacity) = rise_frame(progress, ITEM_RISE_PX);
            element.relative().top(px(offset)).opacity(opacity)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rise_starts_below_and_hidden_and_ends_in_place() {
        if !enabled() {
            return;
        }
        let (offset, opacity) = rise_frame(0.0, 10.0);
        assert_eq!(offset, 10.0);
        assert_eq!(opacity, 0.0);

        let (offset, opacity) = rise_frame(1.0, 10.0);
        assert_eq!(offset, 0.0);
        assert_eq!(opacity, 1.0);
    }

    #[test]
    fn opacity_is_full_before_the_movement_ends() {
        if !enabled() {
            return;
        }
        let (offset, opacity) = rise_frame(0.7, 10.0);
        assert!(offset > 0.0);
        assert_eq!(opacity, 1.0);
    }

    #[test]
    fn progress_outside_the_unit_range_is_clamped() {
        let (offset, opacity) = rise_frame(3.0, 10.0);
        assert_eq!((offset, opacity), (0.0, 1.0));
        let (offset, _) = rise_frame(-1.0, 10.0);
        assert!(offset <= 10.0);
    }

    #[test]
    fn the_stagger_stops_growing_at_the_cap() {
        assert!(stagger_delay(0).is_zero());
        assert_eq!(stagger_delay(1), STAGGER);
        assert_eq!(stagger_delay(STAGGER_CAP), stagger_delay(STAGGER_CAP + 50));
    }

    #[test]
    fn a_slide_settles_once_its_move_is_over_and_only_a_new_target_moves_again() {
        let start = Instant::now();
        let opened = Slide::settled(0, start);
        assert_eq!((opened.from, opened.to, opened.generation), (0, 0, 0));

        let moving = opened.step(1, start);
        assert_eq!((moving.from, moving.to, moving.generation), (0, 1, 1));

        let midway = moving.step(1, start + CARD / 2);
        assert_eq!(midway, moving);

        let reopened = moving.step(1, start + CARD);
        assert_eq!((reopened.from, reopened.to), (1, 1));
        assert_eq!(reopened.generation, moving.generation);

        let back = reopened.step(0, start + CARD * 3);
        assert_eq!((back.from, back.to, back.generation), (1, 0, 2));
        assert_eq!(back.at, start + CARD * 3);
    }
}
