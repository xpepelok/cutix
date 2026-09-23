//! One-shot entrance animations.
//!
//! Everything here rides on gpui's `with_animation`, whose state lives with the element
//! id: an element animates the first frame it is drawn under an id and stays settled
//! for as long as it keeps being drawn. Closing a dialog or leaving a screen drops that
//! state, so the next time it appears it animates again — no bookkeeping on our side.
//!
//! Every helper honours the system "reduce motion" setting by collapsing the animation
//! to its settled frame.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use gpui::{px, Animation, AnimationElement, AnimationExt, ElementId, IntoElement, Styled};

use crate::interaction::{ease, EASE_OVERLAY_IN, EASE_STANDARD};

/// A dialog's card rising into place.
pub const CARD: Duration = Duration::from_millis(220);
/// A whole screen settling in after a route change.
pub const PAGE: Duration = Duration::from_millis(200);
/// One card of a grid or list.
pub const ITEM: Duration = Duration::from_millis(260);
/// The gap between neighbouring cards of a staggered list.
pub const STAGGER: Duration = Duration::from_millis(28);
/// Past this many cards the stagger stops growing, so a long grid does not keep the
/// last card waiting after the first ones have long settled.
pub const STAGGER_CAP: usize = 12;

const CARD_RISE_PX: f32 = 14.0;
const PAGE_RISE_PX: f32 = 8.0;
const ITEM_RISE_PX: f32 = 10.0;
const TOAST_RISE_PX: f32 = 12.0;

/// Whether animations should play at all. Read once: the setting is a system
/// preference, not something that flips while a dialog is opening.
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

/// The offset and opacity of an element that rises `distance` pixels into place.
///
/// Kept separate from the gpui wrapper so the curve itself can be tested.
pub fn rise_frame(progress: f32, distance: f32) -> (f32, f32) {
    let progress = settle(progress).clamp(0.0, 1.0);
    let opacity = (progress * 1.6).min(1.0);
    (distance * (1.0 - progress), opacity)
}

/// The delay before the `index`-th card of a list starts moving.
pub fn stagger_delay(index: usize) -> Duration {
    STAGGER * index.min(STAGGER_CAP) as u32
}

/// Opens a whole dialog layer: the backdrop fades in and the card rises into place.
///
/// Meant for the full-window layer that centres its card with flex. The rise is top
/// padding on that layer rather than an offset on the card, so it can wrap the layer
/// from outside without reaching into how the card was built; the backdrop colour
/// still covers the padding.
pub fn modal<E>(element: E, id: impl Into<ElementId>) -> AnimationElement<E>
where
    E: IntoElement + Styled + 'static,
{
    element.with_animation(
        id,
        Animation::new(CARD).with_easing(out_curve),
        |element, progress| {
            let (offset, opacity) = rise_frame(progress, CARD_RISE_PX);
            // Centred content moves by half the padding added above it.
            element.pt(px(offset * 2.0)).opacity(opacity)
        },
    )
}

/// Settles a whole screen in after a route change.
pub fn page<E>(element: E, id: impl Into<ElementId>) -> AnimationElement<E>
where
    E: IntoElement + Styled + 'static,
{
    rise(element, id, PAGE, PAGE_RISE_PX)
}

/// Rises a toast or banner up from below.
///
/// Like every rise, the element is moved with `relative` + `top`, which overwrites
/// its own positioning: never hand it an `absolute` element — anchor an outer
/// wrapper and animate the inner box.
pub fn toast<E>(element: E, id: impl Into<ElementId>) -> AnimationElement<E>
where
    E: IntoElement + Styled + 'static,
{
    rise(element, id, CARD, TOAST_RISE_PX)
}

/// Fades an element in without moving it — fresh thumbnails, swapped panels.
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

/// Moves `element` with `relative` + `top` on every frame, the settled one included,
/// so it must not be positioned `absolute` itself (see [`toast`]).
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

/// Where a segmented control's highlight is travelling from and to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Slide {
    pub from: usize,
    pub to: usize,
    /// Changes with every move, so each move gets a fresh animation.
    pub generation: u64,
    /// When the highlight last set off; once the move is over the slide settles.
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

    /// The slide after the control asks for `target` at `now`: a new target starts a
    /// move, and a move that has run its course collapses to its destination — so a
    /// strip drawn again later (gpui drops the animation state with the element)
    /// renders settled instead of replaying its last move.
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

/// Records that the control `key` now highlights `target` and returns the move.
///
/// The first sighting starts settled: a control opening should not slide in from
/// its first segment.
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

/// The highlight behind a segmented control of `count` equal segments, gliding from
/// the previous segment to the current one. Place it as the first child of a
/// `relative` row so the segments paint over it.
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

/// The `index`-th card of a grid or list: waits its turn, then rises in.
///
/// The card must be given an id that is stable for what it shows (a path, a project
/// id), so a re-render — a search narrowing the grid, a rename — does not replay it.
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

        // Drawn again mid-move: the same move keeps playing.
        let midway = moving.step(1, start + CARD / 2);
        assert_eq!(midway, moving);

        // Shown again after the move ended (the strip was closed and reopened): it
        // must render in place, under the same animation id.
        let reopened = moving.step(1, start + CARD);
        assert_eq!((reopened.from, reopened.to), (1, 1));
        assert_eq!(reopened.generation, moving.generation);

        let back = reopened.step(0, start + CARD * 3);
        assert_eq!((back.from, back.to, back.generation), (1, 0, 2));
        assert_eq!(back.at, start + CARD * 3);
    }
}
