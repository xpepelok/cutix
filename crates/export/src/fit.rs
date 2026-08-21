#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContainFit {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

pub fn compute_contain_fit(
    source_width: f64,
    source_height: f64,
    target_width: f64,
    target_height: f64,
) -> ContainFit {
    if source_width <= 0.0 || source_height <= 0.0 {
        return ContainFit {
            x: 0.0,
            y: 0.0,
            width: target_width,
            height: target_height,
        };
    }

    let scale = (target_width / source_width).min(target_height / source_height);
    let width = source_width * scale;
    let height = source_height * scale;

    ContainFit {
        x: (target_width - width) / 2.0,
        y: (target_height - height) / 2.0,
        width,
        height,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FitPlan {
    pub inner_width: u32,
    pub inner_height: u32,
    pub offset_x: u32,
    pub offset_y: u32,
    pub letterboxed: bool,
}

pub fn plan_fit(
    canvas_width: u32,
    canvas_height: u32,
    target_width: u32,
    target_height: u32,
) -> FitPlan {
    let fit = compute_contain_fit(
        canvas_width as f64,
        canvas_height as f64,
        target_width as f64,
        target_height as f64,
    );
    let inner_width = even(fit.width.round().max(2.0) as u32).min(even(target_width.max(2)));
    let inner_height = even(fit.height.round().max(2.0) as u32).min(even(target_height.max(2)));
    let offset_x = (target_width.saturating_sub(inner_width)) / 2;
    let offset_y = (target_height.saturating_sub(inner_height)) / 2;
    FitPlan {
        inner_width,
        inner_height,
        offset_x,
        offset_y,
        letterboxed: inner_width != target_width || inner_height != target_height,
    }
}

pub fn even(value: u32) -> u32 {
    if value % 2 == 0 {
        value
    } else {
        value.saturating_sub(1).max(2)
    }
}

/// A borrowed RGBA image: the pixels and the width they are laid out at.
///
/// Rows are tightly packed, four bytes per pixel, so `width` is what turns an offset into
/// a coordinate. Keeping the two together is what stops a caller pairing a buffer with
/// somebody else's width.
#[derive(Clone, Copy, Debug)]
pub struct RgbaFrame<'pixels> {
    pub pixels: &'pixels [u8],
    pub width: u32,
    pub height: u32,
}

/// The same, borrowed for writing.
#[derive(Debug)]
pub struct RgbaFrameMut<'pixels> {
    pub pixels: &'pixels mut [u8],
    pub width: u32,
    pub height: u32,
}

/// Paints `source` onto `target` at `(offset_x, offset_y)`, filling the rest with black.
///
/// This is the letterbox blit: the composed frame is smaller than the output, so the
/// margin around it has to be opaque rather than left as whatever the buffer held.
/// A source that would overhang the target is clipped rather than wrapping onto the next
/// row.
pub fn blit_centre(source: RgbaFrame<'_>, target: RgbaFrameMut<'_>, offset_x: u32, offset_y: u32) {
    for pixel in target.pixels.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[0, 0, 0, 255]);
    }
    let copy_width = source.width.min(target.width.saturating_sub(offset_x));
    let copy_height = source.height.min(target.height.saturating_sub(offset_y));
    for row in 0..copy_height as usize {
        let source_start = row * source.width as usize * 4;
        let target_start =
            ((row + offset_y as usize) * target.width as usize + offset_x as usize) * 4;
        let span = copy_width as usize * 4;
        target.pixels[target_start..target_start + span]
            .copy_from_slice(&source.pixels[source_start..source_start + span]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_degenerate_source_fills_the_target() {
        let fit = compute_contain_fit(0.0, 0.0, 1920.0, 1080.0);
        assert_eq!(fit.width, 1920.0);
        assert_eq!(fit.height, 1080.0);
        assert_eq!(fit.x, 0.0);
    }

    #[test]
    fn a_wide_source_in_a_tall_target_gets_letterbox_bars() {
        let fit = compute_contain_fit(1920.0, 1080.0, 1080.0, 1920.0);
        assert_eq!(fit.width, 1080.0);
        assert_eq!(fit.height, 607.5);
        assert_eq!(fit.x, 0.0);
        assert_eq!(fit.y, (1920.0 - 607.5) / 2.0);
    }

    #[test]
    fn a_matching_aspect_ratio_is_not_letterboxed() {
        let plan = plan_fit(1920, 1080, 1280, 720);
        assert_eq!(plan.inner_width, 1280);
        assert_eq!(plan.inner_height, 720);
        assert!(!plan.letterboxed);
    }

    #[test]
    fn the_inner_frame_is_always_even_sized() {
        let plan = plan_fit(1920, 1080, 1080, 1920);
        assert_eq!(plan.inner_width % 2, 0);
        assert_eq!(plan.inner_height % 2, 0);
        assert!(plan.letterboxed);
        assert_eq!(plan.offset_x, 0);
        assert!(plan.offset_y > 0);
    }

    #[test]
    fn the_bars_are_opaque_black_and_the_inner_pixels_survive() {
        let source = vec![255u8; 2 * 2 * 4];
        let mut target = vec![7u8; 4 * 4 * 4];
        blit_centre(
            RgbaFrame {
                pixels: &source,
                width: 2,
                height: 2,
            },
            RgbaFrameMut {
                pixels: &mut target,
                width: 4,
                height: 4,
            },
            1,
            1,
        );
        assert_eq!(&target[0..4], &[0, 0, 0, 255]);
        let inner = (4 + 1) * 4;
        assert_eq!(&target[inner..inner + 4], &[255, 255, 255, 255]);
    }
}
