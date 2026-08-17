use gpui::{div, prelude::*, px, Div, ScrollHandle};

use crate::theme::{opacity, Palette, SCROLLBAR_SIZE};

pub const SCROLLBAR_THICKNESS: f32 = 6.0;
pub const SCROLLBAR_MIN_THUMB_PX: f32 = 24.0;
pub const SCROLLBAR_INSET_PX: f32 = 2.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThumbGeometry {
    pub length: f32,
    pub offset: f32,
}

pub fn thumb(viewport: f32, max: f32, scrolled: f32) -> Option<ThumbGeometry> {
    if viewport <= 0.0 || max <= 1.0 {
        return None;
    }
    let content = viewport + max;
    let length = (viewport * viewport / content).max(SCROLLBAR_MIN_THUMB_PX.min(viewport));
    let travel = (viewport - length).max(0.0);
    let progress = (scrolled / max).clamp(0.0, 1.0);
    Some(ThumbGeometry {
        length,
        offset: travel * progress,
    })
}

fn geometry(handle: &ScrollHandle, vertical: bool) -> Option<ThumbGeometry> {
    let bounds = handle.bounds().size;
    let max = handle.max_offset();
    let offset = handle.offset();
    if vertical {
        thumb(
            f32::from(bounds.height),
            f32::from(max.height),
            -f32::from(offset.y),
        )
    } else {
        thumb(
            f32::from(bounds.width),
            f32::from(max.width),
            -f32::from(offset.x),
        )
    }
}

pub fn scrollbar_v(handle: &ScrollHandle, colors: Palette) -> Option<Div> {
    let ThumbGeometry { length, offset } = geometry(handle, true)?;

    Some(
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(SCROLLBAR_INSET_PX))
            .w(px(SCROLLBAR_SIZE))
            .flex()
            .justify_center()
            .child(
                div()
                    .absolute()
                    .top(px(offset))
                    .w(px(SCROLLBAR_THICKNESS))
                    .h(px(length))
                    .rounded(px(SCROLLBAR_THICKNESS / 2.0))
                    .bg(opacity(colors.scrollbar_thumb, 0.9)),
            ),
    )
}

pub fn scrollbar_h(handle: &ScrollHandle, colors: Palette) -> Option<Div> {
    let ThumbGeometry { length, offset } = geometry(handle, false)?;

    Some(
        div()
            .absolute()
            .left_0()
            .right_0()
            .bottom(px(SCROLLBAR_INSET_PX))
            .h(px(SCROLLBAR_SIZE))
            .flex()
            .items_center()
            .child(
                div()
                    .absolute()
                    .left(px(offset))
                    .h(px(SCROLLBAR_THICKNESS))
                    .w(px(length))
                    .rounded(px(SCROLLBAR_THICKNESS / 2.0))
                    .bg(opacity(colors.scrollbar_thumb, 0.9)),
            ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_that_fits_has_no_thumb() {
        assert!(thumb(400.0, 0.0, 0.0).is_none());
        assert!(thumb(0.0, 500.0, 0.0).is_none());
    }

    #[test]
    fn the_thumb_shrinks_as_the_content_grows() {
        let short = thumb(400.0, 100.0, 0.0).unwrap();
        let long = thumb(400.0, 1600.0, 0.0).unwrap();
        assert!(long.length < short.length);
        assert!(long.length >= SCROLLBAR_MIN_THUMB_PX);
    }

    #[test]
    fn the_thumb_reaches_the_bottom_at_the_maximum_offset() {
        let viewport = 400.0;
        let max = 600.0;
        let bottom = thumb(viewport, max, max).unwrap();
        assert!((bottom.offset + bottom.length - viewport).abs() < 1e-3);
        assert_eq!(thumb(viewport, max, 0.0).unwrap().offset, 0.0);
    }

    #[test]
    fn overscroll_is_clamped() {
        let clamped = thumb(400.0, 600.0, 5000.0).unwrap();
        assert!(clamped.offset + clamped.length <= 400.0 + 1e-3);
    }
}
