use crate::defaults::{
    MAX_WATERMARK_SIZE, MAX_WATERMARK_TILES, MAX_WATERMARK_TILE_SPACING, MIN_WATERMARK_SIZE,
    MIN_WATERMARK_TILE_SPACING,
};
use crate::types::{TWatermark, WatermarkAnchor};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceSize {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WatermarkRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub scale_x: f64,
    pub scale_y: f64,
    pub position: Position,
    pub rotate: f64,
}

fn finite(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

pub fn clamp_watermark_size(size: f64) -> f64 {
    if !size.is_finite() {
        return MIN_WATERMARK_SIZE;
    }
    size.max(MIN_WATERMARK_SIZE).min(MAX_WATERMARK_SIZE)
}

pub fn clamp_watermark_opacity(opacity: f64) -> f64 {
    if !opacity.is_finite() {
        return 1.0;
    }
    opacity.max(0.0).min(1.0)
}

pub fn compute_watermark_size(
    canvas_size: CanvasSize,
    source_size: SourceSize,
    watermark: &TWatermark,
) -> (f64, f64) {
    let width = clamp_watermark_size(watermark.size) * canvas_size.width;
    let aspect = if source_size.width > 0.0 && source_size.height > 0.0 {
        source_size.height / source_size.width
    } else {
        1.0
    };
    (width, width * aspect)
}

pub fn compute_watermark_rect(
    canvas_size: CanvasSize,
    source_size: SourceSize,
    watermark: &TWatermark,
) -> WatermarkRect {
    let (width, height) = compute_watermark_size(canvas_size, source_size, watermark);
    let (x, y) = anchor_top_left(
        watermark.anchor,
        canvas_size,
        width,
        height,
        watermark.offset.x,
        watermark.offset.y,
    );
    WatermarkRect {
        x,
        y,
        width,
        height,
        rotation: finite(watermark.rotation),
    }
}

fn anchor_top_left(
    anchor: WatermarkAnchor,
    canvas_size: CanvasSize,
    width: f64,
    height: f64,
    offset_x: f64,
    offset_y: f64,
) -> (f64, f64) {
    let offset_x = finite(offset_x) * canvas_size.width;
    let offset_y = finite(offset_y) * canvas_size.height;

    let left = offset_x;
    let right = canvas_size.width - width - offset_x;
    let center_x = (canvas_size.width - width) / 2.0 + offset_x;
    let top = offset_y;
    let bottom = canvas_size.height - height - offset_y;
    let center_y = (canvas_size.height - height) / 2.0 + offset_y;

    match anchor {
        WatermarkAnchor::TopLeft => (left, top),
        WatermarkAnchor::Top => (center_x, top),
        WatermarkAnchor::TopRight => (right, top),
        WatermarkAnchor::Left => (left, center_y),
        WatermarkAnchor::Right => (right, center_y),
        WatermarkAnchor::BottomLeft => (left, bottom),
        WatermarkAnchor::Bottom => (center_x, bottom),
        WatermarkAnchor::BottomRight => (right, bottom),
        WatermarkAnchor::Center => (center_x, center_y),
    }
}

pub fn clamp_watermark_tile_spacing(spacing: f64) -> f64 {
    if !spacing.is_finite() {
        return MIN_WATERMARK_TILE_SPACING;
    }
    spacing
        .max(MIN_WATERMARK_TILE_SPACING)
        .min(MAX_WATERMARK_TILE_SPACING)
}

pub fn compute_watermark_tile_rects(
    canvas_size: CanvasSize,
    source_size: SourceSize,
    watermark: &TWatermark,
) -> Vec<WatermarkRect> {
    let (width, height) = compute_watermark_size(canvas_size, source_size, watermark);
    let spacing = clamp_watermark_tile_spacing(watermark.tiling.spacing);
    let step_x = (width * (1.0 + spacing)).max(1.0);
    let step_y = (height * (1.0 + spacing)).max(1.0);

    let lattice_angle = finite(watermark.tiling.angle);
    let radians = lattice_angle * std::f64::consts::PI / 180.0;
    let cos = radians.cos();
    let sin = radians.sin();

    let reach = (canvas_size.width.hypot(canvas_size.height)) / 2.0;
    let count_x = (reach / step_x).ceil() as i64 + 1;
    let count_y = (reach / step_y).ceil() as i64 + 1;

    let origin_x = canvas_size.width / 2.0 + finite(watermark.offset.x) * canvas_size.width;
    let origin_y = canvas_size.height / 2.0 + finite(watermark.offset.y) * canvas_size.height;
    let rotation = finite(watermark.rotation) + lattice_angle;
    let cull_radius = width.hypot(height) / 2.0;

    let mut rects: Vec<WatermarkRect> = Vec::new();
    for j in -count_y..=count_y {
        for i in -count_x..=count_x {
            let local_x = i as f64 * step_x;
            let local_y = j as f64 * step_y;
            let center_x = origin_x + local_x * cos - local_y * sin;
            let center_y = origin_y + local_x * sin + local_y * cos;

            if center_x + cull_radius < 0.0
                || center_y + cull_radius < 0.0
                || center_x - cull_radius > canvas_size.width
                || center_y - cull_radius > canvas_size.height
            {
                continue;
            }

            rects.push(WatermarkRect {
                x: center_x - width / 2.0,
                y: center_y - height / 2.0,
                width,
                height,
                rotation,
            });
            if rects.len() >= MAX_WATERMARK_TILES {
                return rects;
            }
        }
    }

    rects
}

pub fn watermark_rect_to_transform(
    canvas_size: CanvasSize,
    source_size: SourceSize,
    rect: WatermarkRect,
) -> Transform {
    let contain_scale =
        (canvas_size.width / source_size.width).min(canvas_size.height / source_size.height);
    let scale = if contain_scale > 0.0 && source_size.width > 0.0 {
        rect.width / (source_size.width * contain_scale)
    } else {
        1.0
    };

    Transform {
        scale_x: scale,
        scale_y: scale,
        position: Position {
            x: rect.x + rect.width / 2.0 - canvas_size.width / 2.0,
            y: rect.y + rect.height / 2.0 - canvas_size.height / 2.0,
        },
        rotate: rect.rotation,
    }
}

pub fn compute_watermark_transform(
    canvas_size: CanvasSize,
    source_size: SourceSize,
    watermark: &TWatermark,
) -> Transform {
    watermark_rect_to_transform(
        canvas_size,
        source_size,
        compute_watermark_rect(canvas_size, source_size, watermark),
    )
}

pub fn compute_watermark_rects(
    canvas_size: CanvasSize,
    source_size: SourceSize,
    watermark: &TWatermark,
) -> Vec<WatermarkRect> {
    if watermark.tiling.enabled {
        compute_watermark_tile_rects(canvas_size, source_size, watermark)
    } else {
        vec![compute_watermark_rect(canvas_size, source_size, watermark)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defaults::create_default_watermark;
    use crate::types::{TWatermarkSource, TWatermarkTiling, Vec2};

    const CANVAS: CanvasSize = CanvasSize {
        width: 1920.0,
        height: 1080.0,
    };
    const SOURCE: SourceSize = SourceSize {
        width: 400.0,
        height: 200.0,
    };

    fn watermark() -> TWatermark {
        let mut mark = create_default_watermark();
        mark.enabled = true;
        mark.source = Some(TWatermarkSource::Image {
            media_id: "m".to_string(),
        });
        mark.anchor = WatermarkAnchor::BottomRight;
        mark.offset = Vec2 {
            x: 0.025,
            y: 0.044_444_444_444_444_44,
        };
        mark.size = 0.25;
        mark.opacity = 0.5;
        mark
    }

    fn with_anchor(anchor: WatermarkAnchor, offset: Vec2) -> TWatermark {
        let mut mark = watermark();
        mark.anchor = anchor;
        mark.offset = offset;
        mark
    }

    #[test]
    fn sizes_the_watermark_as_fraction_of_width_and_keeps_aspect() {
        let rect = compute_watermark_rect(CANVAS, SOURCE, &watermark());
        assert_eq!(rect.width, 480.0);
        assert_eq!(rect.height, 240.0);
    }

    #[test]
    fn insets_from_each_of_the_nine_anchors() {
        let inset_x = 0.025 * 1920.0;
        let inset_y = 0.044_444_444_444_444_44 * 1080.0;

        let top_left = compute_watermark_rect(
            CANVAS,
            SOURCE,
            &with_anchor(WatermarkAnchor::TopLeft, watermark().offset),
        );
        assert!((top_left.x - inset_x).abs() < 1e-6);
        assert!((top_left.y - inset_y).abs() < 1e-6);

        let bottom_right = compute_watermark_rect(CANVAS, SOURCE, &watermark());
        assert!((bottom_right.x - (1920.0 - 480.0 - inset_x)).abs() < 1e-6);
        assert!((bottom_right.y - (1080.0 - 240.0 - inset_y)).abs() < 1e-6);

        let zero = Vec2 { x: 0.0, y: 0.0 };
        let top = compute_watermark_rect(CANVAS, SOURCE, &with_anchor(WatermarkAnchor::Top, zero));
        assert_eq!(top.x, (1920.0 - 480.0) / 2.0);
        assert_eq!(top.y, 0.0);

        let left =
            compute_watermark_rect(CANVAS, SOURCE, &with_anchor(WatermarkAnchor::Left, zero));
        assert_eq!(left.x, 0.0);
        assert_eq!(left.y, (1080.0 - 240.0) / 2.0);

        let right =
            compute_watermark_rect(CANVAS, SOURCE, &with_anchor(WatermarkAnchor::Right, zero));
        assert_eq!(right.x, 1920.0 - 480.0);

        let bottom =
            compute_watermark_rect(CANVAS, SOURCE, &with_anchor(WatermarkAnchor::Bottom, zero));
        assert_eq!(bottom.y, 1080.0 - 240.0);

        let center =
            compute_watermark_rect(CANVAS, SOURCE, &with_anchor(WatermarkAnchor::Center, zero));
        assert_eq!(center.x, (1920.0 - 480.0) / 2.0);
        assert_eq!(center.y, (1080.0 - 240.0) / 2.0);
    }

    #[test]
    fn keeps_the_relative_rect_identical_across_resolutions() {
        let mark = watermark();
        let full = compute_watermark_rect(
            CanvasSize {
                width: 1920.0,
                height: 1080.0,
            },
            SOURCE,
            &mark,
        );
        let preview = compute_watermark_rect(
            CanvasSize {
                width: 640.0,
                height: 360.0,
            },
            SOURCE,
            &mark,
        );
        assert!((preview.x / 640.0 - full.x / 1920.0).abs() < 1e-10);
        assert!((preview.y / 360.0 - full.y / 1080.0).abs() < 1e-10);
        assert!((preview.width / 640.0 - full.width / 1920.0).abs() < 1e-10);
        assert!((preview.height / 360.0 - full.height / 1080.0).abs() < 1e-10);
    }

    #[test]
    fn produces_a_transform_the_compositor_resolves_back_to_the_same_rect() {
        let mark = with_anchor(WatermarkAnchor::TopRight, watermark().offset);
        let rect = compute_watermark_rect(CANVAS, SOURCE, &mark);
        let transform = compute_watermark_transform(CANVAS, SOURCE, &mark);

        let contain_scale = (CANVAS.width / SOURCE.width).min(CANVAS.height / SOURCE.height);
        let rendered_width = SOURCE.width * contain_scale * transform.scale_x;
        let rendered_height = SOURCE.height * contain_scale * transform.scale_y;
        let center_x = CANVAS.width / 2.0 + transform.position.x;
        let center_y = CANVAS.height / 2.0 + transform.position.y;

        assert!((rendered_width - rect.width).abs() < 1e-6);
        assert!((rendered_height - rect.height).abs() < 1e-6);
        assert!((center_x - rendered_width / 2.0 - rect.x).abs() < 1e-6);
        assert!((center_y - rendered_height / 2.0 - rect.y).abs() < 1e-6);
    }

    #[test]
    fn carries_rotation_onto_the_transform() {
        let mut mark = watermark();
        mark.rotation = -30.0;
        let transform = compute_watermark_transform(CANVAS, SOURCE, &mark);
        assert_eq!(transform.rotate, -30.0);
    }

    #[test]
    fn clamps_out_of_range_size_inputs() {
        let mut mark = watermark();
        mark.size = 5.0;
        let rect = compute_watermark_rect(CANVAS, SOURCE, &mark);
        assert_eq!(rect.width, 1920.0);
    }

    fn tiled() -> TWatermark {
        let mut mark = with_anchor(WatermarkAnchor::Center, Vec2 { x: 0.0, y: 0.0 });
        mark.size = 0.2;
        mark.tiling = TWatermarkTiling {
            enabled: true,
            spacing: 0.5,
            angle: 0.0,
        };
        mark
    }

    #[test]
    fn repeats_at_the_configured_spacing() {
        let rects = compute_watermark_tile_rects(CANVAS, SOURCE, &tiled());
        let width = 0.2 * 1920.0;
        let height = width / 2.0;
        let expected_step_x = width * 1.5;
        let expected_step_y = height * 1.5;

        let first_y = rects[0].y;
        let mut row: Vec<WatermarkRect> = rects
            .iter()
            .filter(|rect| (rect.y - first_y).abs() < 0.001)
            .copied()
            .collect();
        row.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap());
        assert!(row.len() > 2);
        assert!((row[1].x - row[0].x - expected_step_x).abs() < 1e-6);

        let first_x = rects[0].x;
        let mut column: Vec<WatermarkRect> = rects
            .iter()
            .filter(|rect| (rect.x - first_x).abs() < 0.001)
            .copied()
            .collect();
        column.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap());
        assert!(column.len() > 2);
        assert!((column[1].y - column[0].y - expected_step_y).abs() < 1e-6);
    }

    #[test]
    fn covers_the_frame_and_applies_the_lattice_angle() {
        let mut mark = tiled();
        mark.rotation = 10.0;
        mark.tiling = TWatermarkTiling {
            enabled: true,
            spacing: 0.5,
            angle: 30.0,
        };
        let rects = compute_watermark_tile_rects(CANVAS, SOURCE, &mark);
        assert!(rects.len() > 8);
        for rect in &rects {
            assert_eq!(rect.rotation, 40.0);
        }
    }

    #[test]
    fn tiles_the_same_relative_lattice_at_any_resolution() {
        let full = compute_watermark_tile_rects(
            CanvasSize {
                width: 1920.0,
                height: 1080.0,
            },
            SOURCE,
            &tiled(),
        );
        let small = compute_watermark_tile_rects(
            CanvasSize {
                width: 640.0,
                height: 360.0,
            },
            SOURCE,
            &tiled(),
        );
        assert_eq!(small.len(), full.len());
        for index in 0..full.len() {
            assert!((small[index].x / 640.0 - full[index].x / 1920.0).abs() < 1e-8);
            assert!((small[index].y / 360.0 - full[index].y / 1080.0).abs() < 1e-8);
        }
    }

    #[test]
    fn returns_a_single_rect_when_tiling_is_off() {
        assert_eq!(
            compute_watermark_rects(CANVAS, SOURCE, &watermark()).len(),
            1
        );
    }
}
