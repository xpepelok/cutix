use std::f64::consts::PI;

pub const MIN_MASK_DIMENSION: f64 = 0.01;
pub const DEFAULT_SHAPE_SHORT_SIDE_RATIO: f64 = 0.6;

const STAR_INNER_RADIUS_RATIO: f64 = 0.45;
const STAR_VERTEX_COUNT: usize = 10;
const ELLIPSE_SEGMENTS: usize = 96;
const BEZIER_SEGMENTS: usize = 48;
const NORMAL_SNAP_EPSILON: f64 = 1e-9;

const SUPERSAMPLES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MaskShape {
    Rectangle,
    Ellipse,
    Star,
    Heart,
    Diamond,
    Split,
    CinematicBars,
}

impl MaskShape {
    pub fn key(self) -> &'static str {
        match self {
            MaskShape::Rectangle => "rectangle",
            MaskShape::Ellipse => "ellipse",
            MaskShape::Star => "star",
            MaskShape::Heart => "heart",
            MaskShape::Diamond => "diamond",
            MaskShape::Split => "split",
            MaskShape::CinematicBars => "cinematic-bars",
        }
    }

    pub fn name_key(self) -> &'static str {
        match self {
            MaskShape::Rectangle => "masks.rectangle.name",
            MaskShape::Ellipse => "masks.ellipse.name",
            MaskShape::Star => "masks.star.name",
            MaskShape::Heart => "masks.heart.name",
            MaskShape::Diamond => "masks.diamond.name",
            MaskShape::Split => "masks.split.name",
            MaskShape::CinematicBars => "masks.cinematicBars.name",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        ALL_SHAPES.iter().copied().find(|shape| shape.key() == key)
    }
}

pub const ALL_SHAPES: &[MaskShape] = &[
    MaskShape::Rectangle,
    MaskShape::Ellipse,
    MaskShape::Star,
    MaskShape::Heart,
    MaskShape::Diamond,
    MaskShape::Split,
    MaskShape::CinematicBars,
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaskParams {
    pub center_x: f64,
    pub center_y: f64,
    pub width: f64,
    pub height: f64,
    pub rotation: f64,
    pub feather: f64,
    pub inverted: bool,
}

impl Default for MaskParams {
    fn default() -> Self {
        Self {
            center_x: 0.0,
            center_y: 0.0,
            width: DEFAULT_SHAPE_SHORT_SIDE_RATIO,
            height: DEFAULT_SHAPE_SHORT_SIDE_RATIO,
            rotation: 0.0,
            feather: 0.0,
            inverted: false,
        }
    }
}

impl MaskParams {
    pub fn default_for(layer_width: f64, layer_height: f64) -> Self {
        let abs_width = layer_width.abs();
        let abs_height = layer_height.abs();
        let short_side = abs_width.min(abs_height);
        let side = if short_side > 0.0 {
            short_side * DEFAULT_SHAPE_SHORT_SIDE_RATIO
        } else {
            0.0
        };

        Self {
            width: if abs_width > 0.0 {
                side / abs_width
            } else {
                DEFAULT_SHAPE_SHORT_SIDE_RATIO
            },
            height: if abs_height > 0.0 {
                side / abs_height
            } else {
                DEFAULT_SHAPE_SHORT_SIDE_RATIO
            },
            ..Self::default()
        }
    }
}

fn rotate(x: f64, y: f64, center_x: f64, center_y: f64, radians: f64) -> (f64, f64) {
    if radians == 0.0 {
        return (x, y);
    }
    let (sin, cos) = radians.sin_cos();
    let dx = x - center_x;
    let dy = y - center_y;
    (
        center_x + dx * cos - dy * sin,
        center_y + dx * sin + dy * cos,
    )
}

struct BoxGeometry {
    center_x: f64,
    center_y: f64,
    half_width: f64,
    half_height: f64,
    radians: f64,
}

fn box_geometry(params: &MaskParams, width: f64, height: f64) -> BoxGeometry {
    BoxGeometry {
        center_x: width / 2.0 + params.center_x * width,
        center_y: height / 2.0 + params.center_y * height,
        half_width: params.width.max(MIN_MASK_DIMENSION) * width / 2.0,
        half_height: params.height.max(MIN_MASK_DIMENSION) * height / 2.0,
        radians: params.rotation * PI / 180.0,
    }
}

fn cubic(
    p0: (f64, f64),
    c0: (f64, f64),
    c1: (f64, f64),
    p1: (f64, f64),
    out: &mut Vec<(f64, f64)>,
) {
    for step in 1..=BEZIER_SEGMENTS {
        let t = step as f64 / BEZIER_SEGMENTS as f64;
        let inverse = 1.0 - t;
        let a = inverse * inverse * inverse;
        let b = 3.0 * inverse * inverse * t;
        let c = 3.0 * inverse * t * t;
        let d = t * t * t;
        out.push((
            a * p0.0 + b * c0.0 + c * c1.0 + d * p1.0,
            a * p0.1 + b * c0.1 + c * c1.1 + d * p1.1,
        ));
    }
}

pub fn polygon(shape: MaskShape, params: &MaskParams, width: f64, height: f64) -> Vec<(f64, f64)> {
    let geometry = match shape {
        MaskShape::CinematicBars => BoxGeometry {
            center_x: width / 2.0 + params.center_x * width,
            center_y: height / 2.0 + params.center_y * height,
            half_width: (params.width * width).max(width) / 2.0,
            half_height: params.height.max(MIN_MASK_DIMENSION) * height / 2.0,
            radians: params.rotation * PI / 180.0,
        },
        _ => box_geometry(params, width, height),
    };

    let BoxGeometry {
        center_x,
        center_y,
        half_width,
        half_height,
        radians,
    } = geometry;

    let local: Vec<(f64, f64)> = match shape {
        MaskShape::Rectangle | MaskShape::CinematicBars => vec![
            (-half_width, -half_height),
            (half_width, -half_height),
            (half_width, half_height),
            (-half_width, half_height),
        ],
        MaskShape::Diamond => vec![
            (0.0, -half_height),
            (half_width, 0.0),
            (0.0, half_height),
            (-half_width, 0.0),
        ],
        MaskShape::Ellipse => (0..ELLIPSE_SEGMENTS)
            .map(|index| {
                let angle = index as f64 / ELLIPSE_SEGMENTS as f64 * PI * 2.0;
                (half_width * angle.cos(), half_height * angle.sin())
            })
            .collect(),
        MaskShape::Star => (0..STAR_VERTEX_COUNT)
            .map(|index| {
                let outer = index % 2 == 0;
                let radius_x = if outer {
                    half_width
                } else {
                    half_width * STAR_INNER_RADIUS_RATIO
                };
                let radius_y = if outer {
                    half_height
                } else {
                    half_height * STAR_INNER_RADIUS_RATIO
                };
                let angle = index as f64 * PI / 5.0 - PI / 2.0;
                (radius_x * angle.cos(), radius_y * angle.sin())
            })
            .collect(),
        MaskShape::Heart => {
            let start = (0.0, -half_height * 0.2);
            let mut points = vec![start];
            cubic(
                start,
                (half_width, -half_height * 0.95),
                (half_width, half_height * 0.15),
                (0.0, half_height),
                &mut points,
            );
            cubic(
                (0.0, half_height),
                (-half_width, half_height * 0.15),
                (-half_width, -half_height * 0.95),
                start,
                &mut points,
            );
            points
        }
        MaskShape::Split => Vec::new(),
    };

    local
        .into_iter()
        .map(|(x, y)| rotate(center_x + x, center_y + y, center_x, center_y, radians))
        .collect()
}

fn point_in_polygon(polygon: &[(f64, f64)], x: f64, y: f64) -> bool {
    let mut inside = false;
    let mut j = polygon.len().wrapping_sub(1);
    for i in 0..polygon.len() {
        let (xi, yi) = polygon[i];
        let (xj, yj) = polygon[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

struct SplitLine {
    normal_x: f64,
    normal_y: f64,
    line_x: f64,
    line_y: f64,
}

fn split_line(params: &MaskParams, width: f64, height: f64) -> SplitLine {
    let radians = params.rotation * PI / 180.0;
    let cos = radians.cos();
    let sin = radians.sin();
    SplitLine {
        normal_x: if cos.abs() < NORMAL_SNAP_EPSILON {
            0.0
        } else {
            cos
        },
        normal_y: if sin.abs() < NORMAL_SNAP_EPSILON {
            0.0
        } else {
            sin
        },
        line_x: width / 2.0 + params.center_x * width,
        line_y: height / 2.0 + params.center_y * height,
    }
}

pub fn rasterize(shape: MaskShape, params: &MaskParams, width: usize, height: usize) -> Vec<u8> {
    let mut alpha = vec![0u8; width * height];
    if width == 0 || height == 0 {
        return alpha;
    }

    let canvas_width = width as f64;
    let canvas_height = height as f64;
    let step = 1.0 / SUPERSAMPLES as f64;
    let samples = (SUPERSAMPLES * SUPERSAMPLES) as f64;

    if shape == MaskShape::Split {
        let line = split_line(params, canvas_width, canvas_height);
        for y in 0..height {
            for x in 0..width {
                let mut hits = 0.0;
                for sy in 0..SUPERSAMPLES {
                    let py = y as f64 + (sy as f64 + 0.5) * step;
                    for sx in 0..SUPERSAMPLES {
                        let px = x as f64 + (sx as f64 + 0.5) * step;
                        let sign =
                            (px - line.line_x) * line.normal_x + (py - line.line_y) * line.normal_y;
                        if sign >= 0.0 {
                            hits += 1.0;
                        }
                    }
                }
                alpha[y * width + x] = (hits / samples * 255.0).round() as u8;
            }
        }
        return alpha;
    }

    let points = polygon(shape, params, canvas_width, canvas_height);
    if points.len() < 3 {
        return alpha;
    }

    let min_y = points.iter().map(|(_, y)| *y).fold(f64::MAX, f64::min);
    let max_y = points.iter().map(|(_, y)| *y).fold(f64::MIN, f64::max);
    let min_x = points.iter().map(|(x, _)| *x).fold(f64::MAX, f64::min);
    let max_x = points.iter().map(|(x, _)| *x).fold(f64::MIN, f64::max);

    let y_start = (min_y.floor().max(0.0)) as usize;
    let y_end = (max_y.ceil().min(canvas_height)) as usize;
    let x_start = (min_x.floor().max(0.0)) as usize;
    let x_end = (max_x.ceil().min(canvas_width)) as usize;

    for y in y_start..y_end {
        for x in x_start..x_end {
            let mut hits = 0.0;
            for sy in 0..SUPERSAMPLES {
                let py = y as f64 + (sy as f64 + 0.5) * step;
                for sx in 0..SUPERSAMPLES {
                    let px = x as f64 + (sx as f64 + 0.5) * step;
                    if point_in_polygon(&points, px, py) {
                        hits += 1.0;
                    }
                }
            }
            if hits > 0.0 {
                alpha[y * width + x] = (hits / samples * 255.0).round() as u8;
            }
        }
    }

    alpha
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StrokeAlign {
    Inside,
    #[default]
    Center,
    Outside,
}

impl StrokeAlign {
    pub fn from_key(key: &str) -> Self {
        match key {
            "inside" => StrokeAlign::Inside,
            "outside" => StrokeAlign::Outside,
            _ => StrokeAlign::Center,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            StrokeAlign::Inside => "inside",
            StrokeAlign::Center => "center",
            StrokeAlign::Outside => "outside",
        }
    }

    pub fn offset(self, stroke_width: f64) -> f64 {
        match self {
            StrokeAlign::Inside => -(stroke_width / 2.0),
            StrokeAlign::Center => 0.0,
            StrokeAlign::Outside => stroke_width / 2.0,
        }
    }
}

pub fn stroke_alpha(
    shape: MaskShape,
    params: &MaskParams,
    stroke_width: f64,
    align: StrokeAlign,
    width: usize,
    height: usize,
) -> Vec<u8> {
    let mut band = vec![0u8; width * height];
    if stroke_width <= 0.0 || width == 0 || height == 0 {
        return band;
    }
    let offset = align.offset(stroke_width);
    let grown = |distance: f64| -> MaskParams {
        MaskParams {
            width: (params.width + 2.0 * distance / width.max(1) as f64).max(0.0),
            height: (params.height + 2.0 * distance / height.max(1) as f64).max(0.0),
            ..*params
        }
    };

    let outer = rasterize(shape, &grown(offset + stroke_width / 2.0), width, height);
    let inner = rasterize(shape, &grown(offset - stroke_width / 2.0), width, height);
    for (index, target) in band.iter_mut().enumerate() {
        *target = outer[index].saturating_sub(inner[index]);
    }
    band
}

pub fn coverage(alpha: &[u8]) -> f64 {
    if alpha.is_empty() {
        return 0.0;
    }
    alpha.iter().map(|value| *value as f64).sum::<f64>() / (alpha.len() as f64 * 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 200;
    const H: usize = 200;

    fn full() -> MaskParams {
        MaskParams {
            width: 1.0,
            height: 1.0,
            ..MaskParams::default()
        }
    }

    fn half() -> MaskParams {
        MaskParams {
            width: 0.5,
            height: 0.5,
            ..MaskParams::default()
        }
    }

    fn band_run(alpha: &[u8]) -> (usize, usize) {
        let row = H / 2;
        let mut first = None;
        let mut last = 0usize;
        for x in 0..W / 2 {
            if alpha[row * W + x] > 127 {
                first.get_or_insert(x);
                last = x;
            }
        }
        (first.expect("no band"), last)
    }

    #[test]
    fn a_centred_stroke_straddles_the_mask_edge() {
        let band = stroke_alpha(
            MaskShape::Rectangle,
            &half(),
            20.0,
            StrokeAlign::Center,
            W,
            H,
        );
        let (first, last) = band_run(&band);
        assert_eq!((first, last), (40, 59), "{first}..{last}");
        assert_eq!(band[(H / 2) * W + 100], 0, "the interior is untouched");
    }

    #[test]
    fn an_inside_stroke_sits_wholly_within_the_mask() {
        let band = stroke_alpha(
            MaskShape::Rectangle,
            &half(),
            20.0,
            StrokeAlign::Inside,
            W,
            H,
        );
        assert_eq!(band_run(&band), (50, 69));
    }

    #[test]
    fn an_outside_stroke_sits_wholly_beyond_the_mask() {
        let band = stroke_alpha(
            MaskShape::Rectangle,
            &half(),
            20.0,
            StrokeAlign::Outside,
            W,
            H,
        );
        assert_eq!(band_run(&band), (30, 49));
    }

    #[test]
    fn a_zero_width_stroke_draws_nothing() {
        let band = stroke_alpha(
            MaskShape::Rectangle,
            &half(),
            0.0,
            StrokeAlign::Center,
            W,
            H,
        );
        assert!(band.iter().all(|value| *value == 0));
    }

    #[test]
    fn stroke_align_keys_round_trip() {
        for align in [
            StrokeAlign::Inside,
            StrokeAlign::Center,
            StrokeAlign::Outside,
        ] {
            assert_eq!(StrokeAlign::from_key(align.key()), align);
        }
        assert_eq!(StrokeAlign::from_key("nonsense"), StrokeAlign::Center);
        assert_eq!(StrokeAlign::Inside.offset(20.0), -10.0);
        assert_eq!(StrokeAlign::Outside.offset(20.0), 10.0);
        assert_eq!(StrokeAlign::Center.offset(20.0), 0.0);
    }

    #[test]
    fn shape_keys_round_trip() {
        for shape in ALL_SHAPES {
            assert_eq!(MaskShape::from_key(shape.key()), Some(*shape));
            assert!(shape.name_key().starts_with("masks."));
        }
        assert_eq!(MaskShape::from_key("nonsense"), None);
        assert_eq!(ALL_SHAPES.len(), 7);
    }

    #[test]
    fn a_full_bleed_rectangle_covers_the_whole_plane() {
        let alpha = rasterize(MaskShape::Rectangle, &full(), W, H);
        assert!(coverage(&alpha) > 0.999, "{}", coverage(&alpha));
        assert_eq!(alpha[0], 255);
        assert_eq!(alpha[alpha.len() - 1], 255);
    }

    #[test]
    fn a_half_sized_rectangle_covers_a_quarter_of_the_plane() {
        let params = MaskParams {
            width: 0.5,
            height: 0.5,
            ..MaskParams::default()
        };
        let alpha = rasterize(MaskShape::Rectangle, &params, W, H);
        assert!(
            (coverage(&alpha) - 0.25).abs() < 0.01,
            "{}",
            coverage(&alpha)
        );
    }

    #[test]
    fn an_ellipse_covers_pi_over_four_of_its_bounding_box() {
        let alpha = rasterize(MaskShape::Ellipse, &full(), W, H);
        let expected = PI / 4.0;
        assert!(
            (coverage(&alpha) - expected).abs() < 0.01,
            "{} vs {expected}",
            coverage(&alpha)
        );

        assert_eq!(alpha[(H / 2) * W + W / 2], 255);
        assert_eq!(alpha[0], 0);
    }

    #[test]
    fn a_diamond_covers_half_of_its_bounding_box() {
        let alpha = rasterize(MaskShape::Diamond, &full(), W, H);
        assert!(
            (coverage(&alpha) - 0.5).abs() < 0.01,
            "{}",
            coverage(&alpha)
        );
        assert_eq!(alpha[(H / 2) * W + W / 2], 255);
        assert_eq!(alpha[0], 0);
    }

    #[test]
    fn a_split_covers_half_the_plane_and_flips_with_rotation() {
        let alpha = rasterize(MaskShape::Split, &full(), W, H);
        assert!(
            (coverage(&alpha) - 0.5).abs() < 0.01,
            "{}",
            coverage(&alpha)
        );

        assert_eq!(alpha[(H / 2) * W + W - 1], 255);
        assert_eq!(alpha[(H / 2) * W], 0);

        let flipped = rasterize(
            MaskShape::Split,
            &MaskParams {
                rotation: 180.0,
                ..full()
            },
            W,
            H,
        );
        assert_eq!(flipped[(H / 2) * W + W - 1], 0);
        assert_eq!(flipped[(H / 2) * W], 255);
    }

    #[test]
    fn cinematic_bars_span_the_full_width_whatever_the_width_param() {
        let params = MaskParams {
            width: 0.2,
            height: 0.4,
            ..MaskParams::default()
        };
        let alpha = rasterize(MaskShape::CinematicBars, &params, W, H);
        let middle_row = H / 2;
        assert_eq!(alpha[middle_row * W], 255, "left edge");
        assert_eq!(alpha[middle_row * W + W - 1], 255, "right edge");
        assert_eq!(alpha[0], 0, "top edge stays outside the band");
        assert!(
            (coverage(&alpha) - 0.4).abs() < 0.01,
            "{}",
            coverage(&alpha)
        );
    }

    #[test]
    fn a_star_covers_less_than_its_bounding_box_but_holds_its_centre() {
        let alpha = rasterize(MaskShape::Star, &full(), W, H);
        let value = coverage(&alpha);
        assert!(value > 0.2 && value < 0.6, "{value}");
        assert_eq!(alpha[(H / 2) * W + W / 2], 255);
        assert_eq!(alpha[0], 0);
    }

    #[test]
    fn a_heart_is_solid_in_the_middle_and_empty_at_the_top_corners() {
        let alpha = rasterize(MaskShape::Heart, &full(), W, H);
        let value = coverage(&alpha);
        assert!(value > 0.25 && value < 0.5, "{value}");
        assert_eq!(alpha[(H / 2) * W + W / 2], 255, "centre");
        assert_eq!(alpha[2 * W + 2], 0, "top-left corner");
        assert_eq!(alpha[(H - 3) * W + 2], 0, "bottom-left corner");
    }

    #[test]
    fn the_centre_offset_moves_the_shape() {
        let shifted = MaskParams {
            width: 0.4,
            height: 0.4,
            center_x: 0.25,
            ..MaskParams::default()
        };
        let alpha = rasterize(MaskShape::Rectangle, &shifted, W, H);
        let row = (H / 2) * W;
        assert_eq!(alpha[row + W / 2 + W / 4], 255, "shifted right");
        assert_eq!(alpha[row + W / 4], 0, "left side vacated");
    }

    #[test]
    fn rotating_a_rectangle_by_ninety_degrees_swaps_its_extents() {
        let params = MaskParams {
            width: 0.8,
            height: 0.2,
            ..MaskParams::default()
        };
        let flat = rasterize(MaskShape::Rectangle, &params, W, H);
        let upright = rasterize(
            MaskShape::Rectangle,
            &MaskParams {
                rotation: 90.0,
                ..params
            },
            W,
            H,
        );
        assert!((coverage(&flat) - coverage(&upright)).abs() < 0.01);
        assert_eq!(flat[(H / 2) * W + 20], 255);
        assert_eq!(upright[(H / 2) * W + 20], 0);
        assert_eq!(upright[20 * W + W / 2], 255);
    }

    #[test]
    fn edges_are_antialiased_rather_than_hard_stepped() {
        let alpha = rasterize(
            MaskShape::Ellipse,
            &MaskParams {
                width: 0.7,
                height: 0.7,
                ..MaskParams::default()
            },
            W,
            H,
        );
        let partial = alpha
            .iter()
            .filter(|value| **value > 0 && **value < 255)
            .count();
        assert!(partial > 100, "{partial} partially covered pixels");
    }

    #[test]
    fn a_degenerate_size_still_rasterises_without_panicking() {
        let params = MaskParams {
            width: 0.0,
            height: 0.0,
            ..MaskParams::default()
        };
        for shape in ALL_SHAPES {
            let alpha = rasterize(*shape, &params, 16, 16);
            assert_eq!(alpha.len(), 256);
        }
        assert!(rasterize(MaskShape::Rectangle, &params, 0, 0).is_empty());
    }

    #[test]
    fn defaults_size_off_the_short_side() {
        let params = MaskParams::default_for(1920.0, 1080.0);

        assert!((params.width * 1920.0 - 648.0).abs() < 1e-6);
        assert!((params.height * 1080.0 - 648.0).abs() < 1e-6);

        let square = MaskParams::default_for(0.0, 0.0);
        assert_eq!(square.width, DEFAULT_SHAPE_SHORT_SIDE_RATIO);
    }
}
