use crate::defaults::{
    MAX_WATERMARK_SIZE, MAX_WATERMARK_TILE_SPACING, MAX_WATERMARK_TILES, MIN_WATERMARK_SIZE,
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
    if value.is_finite() { value } else { 0.0 }
}

pub fn clamp_watermark_size(size: f64) -> f64 {
    if !size.is_finite() {
        return MIN_WATERMARK_SIZE;
    }
    size.clamp(MIN_WATERMARK_SIZE, MAX_WATERMARK_SIZE)
}

pub fn clamp_watermark_opacity(opacity: f64) -> f64 {
    if !opacity.is_finite() {
        return 1.0;
    }
    opacity.clamp(0.0, 1.0)
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
    spacing.clamp(MIN_WATERMARK_TILE_SPACING, MAX_WATERMARK_TILE_SPACING)
}

pub fn compute_watermark_tile_rects(
    canvas_size: CanvasSize,
    source_size: SourceSize,
    watermark: &TWatermark,
) -> Vec<WatermarkRect> {
    let (lattice, base_step_x, base_step_y) = tile_lattice(canvas_size, source_size, watermark);

    let covered_width = canvas_size.width + 2.0 * lattice.cull_radius;
    let covered_height = canvas_size.height + 2.0 * lattice.cull_radius;
    let estimated_tiles = covered_width * covered_height / (base_step_x * base_step_y);
    let mut stretch = if estimated_tiles > MAX_WATERMARK_TILES as f64 {
        (estimated_tiles / MAX_WATERMARK_TILES as f64).sqrt()
    } else {
        1.0
    };
    loop {
        let rects = lattice.rects(base_step_x * stretch, base_step_y * stretch);
        if rects.len() <= MAX_WATERMARK_TILES || !stretch.is_finite() {
            return rects;
        }
        stretch *= 1.05;
    }
}

fn tile_lattice(
    canvas_size: CanvasSize,
    source_size: SourceSize,
    watermark: &TWatermark,
) -> (TileLattice, f64, f64) {
    let (width, height) = compute_watermark_size(canvas_size, source_size, watermark);
    let spacing = clamp_watermark_tile_spacing(watermark.tiling.spacing);
    let base_step_x = (width * (1.0 + spacing)).max(1.0);
    let base_step_y = (height * (1.0 + spacing)).max(1.0);

    let lattice_angle = finite(watermark.tiling.angle);
    let radians = lattice_angle * std::f64::consts::PI / 180.0;
    let lattice = TileLattice {
        canvas_size,
        width,
        height,
        cos: radians.cos(),
        sin: radians.sin(),
        origin_x: canvas_size.width / 2.0 + finite(watermark.offset.x) * canvas_size.width,
        origin_y: canvas_size.height / 2.0 + finite(watermark.offset.y) * canvas_size.height,
        rotation: finite(watermark.rotation) + lattice_angle,
        cull_radius: width.hypot(height) / 2.0,
    };
    (lattice, base_step_x, base_step_y)
}

struct TileLattice {
    canvas_size: CanvasSize,
    width: f64,
    height: f64,
    cos: f64,
    sin: f64,
    origin_x: f64,
    origin_y: f64,
    rotation: f64,
    cull_radius: f64,
}

impl TileLattice {
    fn reach(&self) -> f64 {
        let corners = [
            (0.0, 0.0),
            (self.canvas_size.width, 0.0),
            (0.0, self.canvas_size.height),
            (self.canvas_size.width, self.canvas_size.height),
        ];
        corners
            .iter()
            .map(|(x, y)| (x - self.origin_x).hypot(y - self.origin_y))
            .fold(0.0, f64::max)
            + self.cull_radius
    }

    fn rects(&self, step_x: f64, step_y: f64) -> Vec<WatermarkRect> {
        let canvas_size = self.canvas_size;
        let reach = self.reach();
        let count_x = (reach / step_x).ceil() as i64 + 1;
        let count_y = (reach / step_y).ceil() as i64 + 1;

        let mut rects: Vec<WatermarkRect> = Vec::new();
        for j in -count_y..=count_y {
            for i in -count_x..=count_x {
                let local_x = i as f64 * step_x;
                let local_y = j as f64 * step_y;
                let center_x = self.origin_x + local_x * self.cos - local_y * self.sin;
                let center_y = self.origin_y + local_x * self.sin + local_y * self.cos;

                if center_x + self.cull_radius < 0.0
                    || center_y + self.cull_radius < 0.0
                    || center_x - self.cull_radius > canvas_size.width
                    || center_y - self.cull_radius > canvas_size.height
                {
                    continue;
                }

                rects.push(WatermarkRect {
                    x: center_x - self.width / 2.0,
                    y: center_y - self.height / 2.0,
                    width: self.width,
                    height: self.height,
                    rotation: self.rotation,
                });
            }
        }
        rects
    }
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
    fn dense_tiling_stays_under_the_cap_and_still_covers_top_and_bottom() {
        let mut mark = tiled();
        mark.size = MIN_WATERMARK_SIZE;
        mark.tiling.spacing = MIN_WATERMARK_TILE_SPACING;
        mark.tiling.angle = 20.0;
        let rects = compute_watermark_tile_rects(CANVAS, SOURCE, &mark);
        assert!(rects.len() <= MAX_WATERMARK_TILES);
        assert!(rects.len() > MAX_WATERMARK_TILES / 2);

        let centers_y: Vec<f64> = rects
            .iter()
            .map(|rect| rect.y + rect.height / 2.0)
            .collect();
        let top = centers_y.iter().copied().fold(f64::INFINITY, f64::min);
        let bottom = centers_y.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert!(top < CANVAS.height * 0.1, "top tile at {top}");
        assert!(bottom > CANVAS.height * 0.9, "bottom tile at {bottom}");

        let centers_x: Vec<f64> = rects.iter().map(|rect| rect.x + rect.width / 2.0).collect();
        let left = centers_x.iter().copied().fold(f64::INFINITY, f64::min);
        let right = centers_x.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert!(left < CANVAS.width * 0.1, "left tile at {left}");
        assert!(right > CANVAS.width * 0.9, "right tile at {right}");
    }

    fn brute_force_centers(lattice: &TileLattice, step_x: f64, step_y: f64) -> Vec<(f64, f64)> {
        let canvas = lattice.canvas_size;
        let span = canvas.width.max(canvas.height) * 4.0;
        let count_x = (span / step_x).ceil() as i64;
        let count_y = (span / step_y).ceil() as i64;
        let mut centers = Vec::new();
        for j in -count_y..=count_y {
            for i in -count_x..=count_x {
                let local_x = i as f64 * step_x;
                let local_y = j as f64 * step_y;
                let x = lattice.origin_x + local_x * lattice.cos - local_y * lattice.sin;
                let y = lattice.origin_y + local_x * lattice.sin + local_y * lattice.cos;
                let touches = x + lattice.cull_radius >= 0.0
                    && y + lattice.cull_radius >= 0.0
                    && x - lattice.cull_radius <= canvas.width
                    && y - lattice.cull_radius <= canvas.height;
                if touches {
                    centers.push((x, y));
                }
            }
        }
        sort_centers(&mut centers);
        centers
    }

    fn sort_centers(centers: &mut [(f64, f64)]) {
        centers.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap()
                .then(a.1.partial_cmp(&b.1).unwrap())
        });
    }

    #[test]
    fn an_offset_lattice_still_reaches_the_far_corner() {
        let square = CanvasSize {
            width: 1080.0,
            height: 1080.0,
        };
        let tall = SourceSize {
            width: 200.0,
            height: 400.0,
        };
        let cases = [
            (square, tall, 0.01, 0.05, 45.0),
            (square, tall, 0.02, 0.6, 45.0),
            (square, tall, 0.015, 1.0, 45.0),
            (CANVAS, SOURCE, 0.03, 0.2, 30.0),
            (CANVAS, SOURCE, 0.05, 0.6, 30.0),
        ];
        for (canvas, source, size, spacing, angle) in cases {
            let mut mark = tiled();
            mark.size = size;
            mark.offset = Vec2 { x: 0.08, y: 0.08 };
            mark.tiling = TWatermarkTiling {
                enabled: true,
                spacing,
                angle,
            };
            let (lattice, step_x, step_y) = tile_lattice(canvas, source, &mark);
            for stretch in [1.0, 2.5] {
                let (step_x, step_y) = (step_x * stretch, step_y * stretch);
                let mut got: Vec<(f64, f64)> = lattice
                    .rects(step_x, step_y)
                    .iter()
                    .map(|rect| (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0))
                    .collect();
                sort_centers(&mut got);
                let wanted = brute_force_centers(&lattice, step_x, step_y);
                assert_eq!(
                    got.len(),
                    wanted.len(),
                    "size {size} spacing {spacing} angle {angle} stretch {stretch}: \
                     {} of {} visible tiles found",
                    got.len(),
                    wanted.len()
                );
                for (a, b) in got.iter().zip(&wanted) {
                    assert!((a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6);
                }
            }
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
