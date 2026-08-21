use cutix_playback::resolve::{normalize_crop, MIN_CROP_SPAN};
use cutix_playback::ElementRect;
use cutix_project::Crop;

pub const HANDLE_SIZE_PX: f32 = 10.0;
pub const HANDLE_HIT_AREA_PX: f32 = 18.0;
pub const ICON_HANDLE_RADIUS_PX: f32 = 10.0;
pub const EDGE_HANDLE_THIN_PX: f32 = 6.0;
pub const EDGE_HANDLE_THICK_PX: f32 = 14.0;
pub const LINE_HIT_AREA_PX: f32 = 48.0;
pub const ROTATION_HANDLE_OFFSET_PX: f32 = 24.0;
pub const OUTLINE_OPACITY: f32 = 0.75;
pub const SNAP_LINE_OPACITY: f32 = 0.7;
pub const GUIDE_LINE_OPACITY: f32 = 0.35;
pub const OUTLINE_DASH_PX: f32 = 4.0;
pub const SNAP_THRESHOLD_SCREEN_PX: f32 = 8.0;
pub const ROTATION_SNAP_STEP_DEGREES: f64 = 90.0;
pub const ROTATION_SNAP_THRESHOLD_DEGREES: f64 = 5.0;
pub const MIN_SCALE: f64 = 0.01;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CropHandle {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Left,
    Right,
    Top,
    Bottom,
}

impl CropHandle {
    pub const ALL: [CropHandle; 8] = [
        CropHandle::TopLeft,
        CropHandle::TopRight,
        CropHandle::BottomLeft,
        CropHandle::BottomRight,
        CropHandle::Left,
        CropHandle::Right,
        CropHandle::Top,
        CropHandle::Bottom,
    ];

    pub fn id(self) -> &'static str {
        match self {
            CropHandle::TopLeft => "top-left",
            CropHandle::TopRight => "top-right",
            CropHandle::BottomLeft => "bottom-left",
            CropHandle::BottomRight => "bottom-right",
            CropHandle::Left => "left",
            CropHandle::Right => "right",
            CropHandle::Top => "top",
            CropHandle::Bottom => "bottom",
        }
    }

    pub fn offset(self) -> (f32, f32) {
        match self {
            CropHandle::TopLeft => (-0.5, -0.5),
            CropHandle::TopRight => (0.5, -0.5),
            CropHandle::BottomLeft => (-0.5, 0.5),
            CropHandle::BottomRight => (0.5, 0.5),
            CropHandle::Left => (-0.5, 0.0),
            CropHandle::Right => (0.5, 0.0),
            CropHandle::Top => (0.0, -0.5),
            CropHandle::Bottom => (0.0, 0.5),
        }
    }

    pub fn is_corner(self) -> bool {
        matches!(
            self,
            CropHandle::TopLeft
                | CropHandle::TopRight
                | CropHandle::BottomLeft
                | CropHandle::BottomRight
        )
    }

    pub fn touches_left(self) -> bool {
        matches!(
            self,
            CropHandle::Left | CropHandle::TopLeft | CropHandle::BottomLeft
        )
    }

    pub fn touches_right(self) -> bool {
        matches!(
            self,
            CropHandle::Right | CropHandle::TopRight | CropHandle::BottomRight
        )
    }

    pub fn touches_top(self) -> bool {
        matches!(
            self,
            CropHandle::Top | CropHandle::TopLeft | CropHandle::TopRight
        )
    }

    pub fn touches_bottom(self) -> bool {
        matches!(
            self,
            CropHandle::Bottom | CropHandle::BottomLeft | CropHandle::BottomRight
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizeCursor {
    EastWest,
    NorthWestSouthEast,
    NorthSouth,
    NorthEastSouthWest,
}

pub fn resize_cursor(angle_degrees: f64) -> ResizeCursor {
    let normalized = angle_degrees.rem_euclid(180.0);
    if !(22.5..157.5).contains(&normalized) {
        ResizeCursor::EastWest
    } else if normalized < 67.5 {
        ResizeCursor::NorthWestSouthEast
    } else if normalized < 112.5 {
        ResizeCursor::NorthSouth
    } else {
        ResizeCursor::NorthEastSouthWest
    }
}

pub fn handle_point(rect: &ElementRect, offset: (f32, f32)) -> (f32, f32) {
    rect.canvas_from_local(offset.0, offset.1)
}

pub fn rotation_handle_point(rect: &ElementRect, offset_px: f32) -> (f32, f32) {
    let (sin, cos) = rect.rotation_degrees.to_radians().sin_cos();
    let (x, y) = rect.canvas_from_local(0.0, -0.5);
    (x + sin * offset_px, y - cos * offset_px)
}

pub fn pointer_angle_degrees(dx: f64, dy: f64) -> f64 {
    dy.atan2(dx).to_degrees()
}

pub fn angle_delta_degrees(initial: f64, current: f64) -> f64 {
    let mut delta = current - initial;
    if delta > 180.0 {
        delta -= 360.0;
    }
    if delta < -180.0 {
        delta += 360.0;
    }
    delta
}

pub fn snap_rotation(proposed: f64) -> (f64, bool) {
    let nearest = (proposed / ROTATION_SNAP_STEP_DEGREES).round() * ROTATION_SNAP_STEP_DEGREES;
    if (proposed - nearest).abs() <= ROTATION_SNAP_THRESHOLD_DEGREES {
        (nearest, true)
    } else {
        (proposed, false)
    }
}

pub fn rotation_from_pointer(
    initial_rotation: f64,
    initial_angle: f64,
    current_angle: f64,
    snapping: bool,
) -> f64 {
    let proposed = initial_rotation + angle_delta_degrees(initial_angle, current_angle);
    if snapping {
        snap_rotation(proposed).0
    } else {
        proposed
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FullBounds {
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    pub flip_x: bool,
    pub flip_y: bool,
}

pub fn uncropped_bounds(rect: &ElementRect, crop: &Crop) -> FullBounds {
    let crop = normalize_crop(Some(crop));
    let crop_width = 1.0 - crop.left - crop.right;
    let crop_height = 1.0 - crop.top - crop.bottom;
    let full_width = rect.width as f64 / crop_width;
    let full_height = rect.height as f64 / crop_height;
    let flip_x = full_width < 0.0;
    let flip_y = full_height < 0.0;
    let offset_x =
        ((crop.left - crop.right) / 2.0) * full_width.abs() * if flip_x { -1.0 } else { 1.0 };
    let offset_y =
        ((crop.top - crop.bottom) / 2.0) * full_height.abs() * if flip_y { -1.0 } else { 1.0 };
    let (sin, cos) = (rect.rotation_degrees as f64).to_radians().sin_cos();
    FullBounds {
        center_x: (rect.center_x as f64 - (offset_x * cos - offset_y * sin)) as f32,
        center_y: (rect.center_y as f64 - (offset_x * sin + offset_y * cos)) as f32,
        width: full_width.abs() as f32,
        height: full_height.abs() as f32,
        flip_x,
        flip_y,
    }
}

pub fn crop_display_uv(full: &FullBounds, rotation_degrees: f32, x: f32, y: f32) -> (f64, f64) {
    let (sin, cos) = (rotation_degrees as f64).to_radians().sin_cos();
    let dx = (x - full.center_x) as f64;
    let dy = (y - full.center_y) as f64;
    let local_x = dx * cos + dy * sin;
    let local_y = -dx * sin + dy * cos;
    let width = if full.width.abs() > f32::EPSILON {
        full.width as f64
    } else {
        1.0
    };
    let height = if full.height.abs() > f32::EPSILON {
        full.height as f64
    } else {
        1.0
    };
    (local_x / width + 0.5, local_y / height + 0.5)
}

pub fn apply_crop_handle_drag(
    handle: CropHandle,
    crop: &Crop,
    display_u: f64,
    display_v: f64,
    flip_x: bool,
    flip_y: bool,
) -> Crop {
    let normalized = normalize_crop(Some(crop));
    let mut next = Crop {
        left: normalized.left,
        top: normalized.top,
        right: normalized.right,
        bottom: normalized.bottom,
    };

    if handle.touches_left() {
        set_horizontal(&mut next, true, display_u, flip_x);
    }
    if handle.touches_right() {
        set_horizontal(&mut next, false, display_u, flip_x);
    }
    if handle.touches_top() {
        set_vertical(&mut next, true, display_v, flip_y);
    }
    if handle.touches_bottom() {
        set_vertical(&mut next, false, display_v, flip_y);
    }

    let normalized = normalize_crop(Some(&next));
    Crop {
        left: normalized.left,
        top: normalized.top,
        right: normalized.right,
        bottom: normalized.bottom,
    }
}

fn set_horizontal(crop: &mut Crop, leading: bool, display_u: f64, flip_x: bool) {
    let inset = if leading { display_u } else { 1.0 - display_u };
    if leading == !flip_x {
        crop.left = inset.max(0.0).min(1.0 - MIN_CROP_SPAN - crop.right);
    } else {
        crop.right = inset.max(0.0).min(1.0 - MIN_CROP_SPAN - crop.left);
    }
}

fn set_vertical(crop: &mut Crop, leading: bool, display_v: f64, flip_y: bool) {
    let inset = if leading { display_v } else { 1.0 - display_v };
    if leading == !flip_y {
        crop.top = inset.max(0.0).min(1.0 - MIN_CROP_SPAN - crop.bottom);
    } else {
        crop.bottom = inset.max(0.0).min(1.0 - MIN_CROP_SPAN - crop.top);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapAxis {
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SnapLine {
    pub axis: SnapAxis,
    pub position: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SnapResult {
    pub x: f64,
    pub y: f64,
    pub lines: Vec<SnapLine>,
}

#[derive(Clone, Copy, Debug)]
struct AxisCandidate {
    snapped: f64,
    line: SnapLine,
    distance: f64,
}

fn closest(candidates: &[AxisCandidate], threshold: f64) -> Option<AxisCandidate> {
    candidates
        .iter()
        .filter(|candidate| candidate.distance <= threshold)
        .copied()
        .reduce(|best, candidate| {
            if candidate.distance < best.distance {
                candidate
            } else {
                best
            }
        })
}

pub fn aabb_half_extents(width: f64, height: f64, rotation_degrees: f64) -> (f64, f64) {
    let (sin, cos) = rotation_degrees.to_radians().sin_cos();
    let (sin, cos) = (sin.abs(), cos.abs());
    (
        (width * cos + height * sin) / 2.0,
        (width * sin + height * cos) / 2.0,
    )
}

pub fn snap_position(
    proposed: (f64, f64),
    canvas: (f64, f64),
    element: (f64, f64),
    rotation_degrees: f64,
    threshold: (f64, f64),
) -> SnapResult {
    let left = -canvas.0 / 2.0;
    let right = canvas.0 / 2.0;
    let top = -canvas.1 / 2.0;
    let bottom = canvas.1 / 2.0;
    let (half_width, half_height) = aabb_half_extents(element.0, element.1, rotation_degrees);

    let mut x_candidates = Vec::with_capacity(9);
    for target in [0.0, left, right] {
        let line = SnapLine {
            axis: SnapAxis::Vertical,
            position: target,
        };
        x_candidates.push(AxisCandidate {
            snapped: target,
            line,
            distance: (proposed.0 - target).abs(),
        });
        x_candidates.push(AxisCandidate {
            snapped: target + half_width,
            line,
            distance: (proposed.0 - half_width - target).abs(),
        });
        x_candidates.push(AxisCandidate {
            snapped: target - half_width,
            line,
            distance: (proposed.0 + half_width - target).abs(),
        });
    }

    let mut y_candidates = Vec::with_capacity(9);
    for target in [0.0, top, bottom] {
        let line = SnapLine {
            axis: SnapAxis::Horizontal,
            position: target,
        };
        y_candidates.push(AxisCandidate {
            snapped: target,
            line,
            distance: (proposed.1 - target).abs(),
        });
        y_candidates.push(AxisCandidate {
            snapped: target + half_height,
            line,
            distance: (proposed.1 - half_height - target).abs(),
        });
        y_candidates.push(AxisCandidate {
            snapped: target - half_height,
            line,
            distance: (proposed.1 + half_height - target).abs(),
        });
    }

    let best_x = closest(&x_candidates, threshold.0);
    let best_y = closest(&y_candidates, threshold.1);
    let mut lines = Vec::new();
    if let Some(candidate) = best_x {
        lines.push(candidate.line);
    }
    if let Some(candidate) = best_y {
        lines.push(candidate.line);
    }
    SnapResult {
        x: best_x.map_or(proposed.0, |candidate| candidate.snapped),
        y: best_y.map_or(proposed.1, |candidate| candidate.snapped),
        lines,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AxisSnap {
    pub scale: f64,
    pub distance: f64,
    pub lines: Vec<SnapLine>,
}

#[derive(Clone, Copy, Debug)]
struct ScaleCandidate {
    scale: f64,
    distance: f64,
    line: SnapLine,
}

fn best_scale(candidates: &[ScaleCandidate], proposed: f64) -> AxisSnap {
    let best = candidates.iter().copied().reduce(|best, candidate| {
        if candidate.distance < best.distance {
            candidate
        } else {
            best
        }
    });
    match best {
        Some(candidate) => AxisSnap {
            scale: candidate.scale,
            distance: candidate.distance,
            lines: vec![candidate.line],
        },
        None => AxisSnap {
            scale: proposed,
            distance: f64::INFINITY,
            lines: Vec::new(),
        },
    }
}

#[allow(clippy::too_many_arguments)]
pub fn snap_scale_axes(
    proposed: (f64, f64),
    position: (f64, f64),
    base: (f64, f64),
    rotation_degrees: f64,
    canvas: (f64, f64),
    threshold: (f64, f64),
) -> (AxisSnap, AxisSnap) {
    const EPSILON: f64 = 1e-6;
    let canvas_left = -canvas.0 / 2.0;
    let canvas_right = canvas.0 / 2.0;
    let canvas_top = -canvas.1 / 2.0;
    let canvas_bottom = canvas.1 / 2.0;
    let (sin, cos) = rotation_degrees.to_radians().sin_cos();
    let (sin, cos) = (sin.abs(), cos.abs());

    let half_width = (base.0 * proposed.0 * cos + base.1 * proposed.1 * sin) / 2.0;
    let half_height = (base.0 * proposed.0 * sin + base.1 * proposed.1 * cos) / 2.0;
    let left_edge = position.0 - half_width;
    let right_edge = position.0 + half_width;
    let top_edge = position.1 - half_height;
    let bottom_edge = position.1 + half_height;

    let y_contrib_width = base.1 * proposed.1 * sin;
    let y_contrib_height = base.1 * proposed.1 * cos;
    let x_contrib_width = base.0 * proposed.0 * cos;
    let x_contrib_height = base.0 * proposed.0 * sin;

    let mut x_candidates = Vec::new();
    let mut y_candidates = Vec::new();

    let push = |candidates: &mut Vec<ScaleCandidate>, scale: f64, distance: f64, line: SnapLine| {
        if scale.abs() > MIN_SCALE && scale.is_finite() {
            candidates.push(ScaleCandidate {
                scale,
                distance,
                line,
            });
        }
    };

    if cos > EPSILON {
        for target in [canvas_left, 0.0, canvas_right] {
            let line = SnapLine {
                axis: SnapAxis::Vertical,
                position: target,
            };
            let distance = (left_edge - target).abs();
            if distance <= threshold.0 {
                push(
                    &mut x_candidates,
                    (2.0 * (position.0 - target) - y_contrib_width) / (base.0 * cos),
                    distance,
                    line,
                );
            }
            let distance = (right_edge - target).abs();
            if distance <= threshold.0 {
                push(
                    &mut x_candidates,
                    (2.0 * (target - position.0) - y_contrib_width) / (base.0 * cos),
                    distance,
                    line,
                );
            }
        }
    }

    if sin > EPSILON {
        for target in [canvas_top, 0.0, canvas_bottom] {
            let line = SnapLine {
                axis: SnapAxis::Horizontal,
                position: target,
            };
            let distance = (top_edge - target).abs();
            if distance <= threshold.1 {
                push(
                    &mut x_candidates,
                    (2.0 * (position.1 - target) - y_contrib_height) / (base.0 * sin),
                    distance,
                    line,
                );
            }
            let distance = (bottom_edge - target).abs();
            if distance <= threshold.1 {
                push(
                    &mut x_candidates,
                    (2.0 * (target - position.1) - y_contrib_height) / (base.0 * sin),
                    distance,
                    line,
                );
            }
        }
        for target in [canvas_left, 0.0, canvas_right] {
            let line = SnapLine {
                axis: SnapAxis::Vertical,
                position: target,
            };
            let distance = (left_edge - target).abs();
            if distance <= threshold.0 {
                push(
                    &mut y_candidates,
                    (2.0 * (position.0 - target) - x_contrib_width) / (base.1 * sin),
                    distance,
                    line,
                );
            }
            let distance = (right_edge - target).abs();
            if distance <= threshold.0 {
                push(
                    &mut y_candidates,
                    (2.0 * (target - position.0) - x_contrib_width) / (base.1 * sin),
                    distance,
                    line,
                );
            }
        }
    }

    if cos > EPSILON {
        for target in [canvas_top, 0.0, canvas_bottom] {
            let line = SnapLine {
                axis: SnapAxis::Horizontal,
                position: target,
            };
            let distance = (top_edge - target).abs();
            if distance <= threshold.1 {
                push(
                    &mut y_candidates,
                    (2.0 * (position.1 - target) - x_contrib_height) / (base.1 * cos),
                    distance,
                    line,
                );
            }
            let distance = (bottom_edge - target).abs();
            if distance <= threshold.1 {
                push(
                    &mut y_candidates,
                    (2.0 * (target - position.1) - x_contrib_height) / (base.1 * cos),
                    distance,
                    line,
                );
            }
        }
    }

    (
        best_scale(&x_candidates, proposed.0),
        best_scale(&y_candidates, proposed.1),
    )
}

pub const GRID_MIN: u32 = 1;
pub const GRID_MAX: u32 = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridConfig {
    pub rows: u32,
    pub cols: u32,
}

impl Default for GridConfig {
    fn default() -> Self {
        Self { rows: 3, cols: 3 }
    }
}

impl GridConfig {
    pub fn clamped(self) -> Self {
        Self {
            rows: self.rows.clamp(GRID_MIN, GRID_MAX),
            cols: self.cols.clamp(GRID_MIN, GRID_MAX),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CustomLine {
    pub axis: SnapAxis,
    pub fraction: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GuideShape {
    VLine {
        x: f32,
    },
    HLine {
        y: f32,
    },
    Rect {
        left: f32,
        top: f32,
        width: f32,
        height: f32,
    },
}

pub struct GuideEntry {
    pub id: &'static str,
    pub label: &'static str,
    pub label_key: Option<&'static str>,
}

pub const GUIDE_REGISTRY: &[GuideEntry] = &[
    GuideEntry {
        id: "grid",
        label: "Grid",
        label_key: Some("guides.grid"),
    },
    GuideEntry {
        id: "tiktok",
        label: "TikTok",
        label_key: None,
    },
    GuideEntry {
        id: "ig-reels",
        label: "Reels",
        label_key: None,
    },
    GuideEntry {
        id: "yt-shorts",
        label: "Shorts",
        label_key: None,
    },
    GuideEntry {
        id: "spotlight",
        label: "Spotlight",
        label_key: None,
    },
    GuideEntry {
        id: "custom",
        label: "Custom",
        label_key: Some("guides.custom"),
    },
];

/// A rectangle of a preview canvas that a platform's own interface covers, in fractions
/// of the canvas: `(left, top, width, height)`.
type SafeAreaBand = (f32, f32, f32, f32);

/// The bands each platform overlays, keyed by platform id.
const PLATFORM_BANDS: &[(&str, &[SafeAreaBand])] = &[
    (
        "tiktok",
        &[
            (0.0, 0.0, 1.0, 0.099),
            (0.87, 0.729, 0.13, 0.187),
            (0.0, 0.833, 0.833, 0.104),
            (0.0, 0.937, 1.0, 0.063),
        ],
    ),
    (
        "ig-reels",
        &[
            (0.0, 0.0, 1.0, 0.094),
            (0.87, 0.500, 0.13, 0.310),
            (0.0, 0.797, 0.833, 0.109),
            (0.0, 0.937, 1.0, 0.063),
        ],
    ),
    (
        "yt-shorts",
        &[
            (0.0, 0.0, 1.0, 0.083),
            (0.87, 0.521, 0.13, 0.323),
            (0.0, 0.813, 0.833, 0.099),
            (0.0, 0.927, 1.0, 0.073),
        ],
    ),
    (
        "spotlight",
        &[
            (0.0, 0.0, 1.0, 0.089),
            (0.87, 0.615, 0.13, 0.229),
            (0.0, 0.833, 0.750, 0.094),
            (0.0, 0.937, 1.0, 0.063),
        ],
    ),
];

pub fn guide_shapes(
    id: &str,
    grid: GridConfig,
    custom: &[CustomLine],
    width: f32,
    height: f32,
) -> Vec<GuideShape> {
    if width <= 0.0 || height <= 0.0 {
        return Vec::new();
    }
    if id == "grid" {
        let grid = grid.clamped();
        let mut shapes = Vec::new();
        for index in 1..grid.cols {
            shapes.push(GuideShape::VLine {
                x: index as f32 / grid.cols as f32 * width,
            });
        }
        for index in 1..grid.rows {
            shapes.push(GuideShape::HLine {
                y: index as f32 / grid.rows as f32 * height,
            });
        }
        return shapes;
    }
    if id == "custom" {
        return custom
            .iter()
            .map(|line| match line.axis {
                SnapAxis::Vertical => GuideShape::VLine {
                    x: line.fraction * width,
                },
                SnapAxis::Horizontal => GuideShape::HLine {
                    y: line.fraction * height,
                },
            })
            .collect();
    }
    PLATFORM_BANDS
        .iter()
        .find(|(platform, _)| *platform == id)
        .map(|(_, bands)| {
            bands
                .iter()
                .map(|(left, top, band_width, band_height)| GuideShape::Rect {
                    left: left * width,
                    top: top * height,
                    width: band_width * width,
                    height: band_height * height,
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(rotation: f32) -> ElementRect {
        ElementRect {
            center_x: 200.0,
            center_y: 100.0,
            width: 100.0,
            height: 50.0,
            rotation_degrees: rotation,
        }
    }

    fn crop(left: f64, top: f64, right: f64, bottom: f64) -> Crop {
        Crop {
            left,
            top,
            right,
            bottom,
        }
    }

    #[test]
    fn an_unrotated_corner_sits_on_the_rect_corner() {
        let point = handle_point(&rect(0.0), CropHandle::TopLeft.offset());
        assert!((point.0 - 150.0).abs() < 1e-4);
        assert!((point.1 - 75.0).abs() < 1e-4);
    }

    #[test]
    fn a_quarter_turn_moves_the_top_left_corner_to_the_bottom_left() {
        let point = handle_point(&rect(90.0), CropHandle::TopLeft.offset());
        assert!((point.0 - 225.0).abs() < 1e-3);
        assert!((point.1 - 50.0).abs() < 1e-3);
    }

    #[test]
    fn the_rotate_handle_stands_off_the_top_edge_by_the_web_offset() {
        let point = rotation_handle_point(&rect(0.0), ROTATION_HANDLE_OFFSET_PX);
        assert!((point.0 - 200.0).abs() < 1e-4);
        assert!((point.1 - (75.0 - 24.0)).abs() < 1e-4);
    }

    #[test]
    fn the_rotate_handle_swings_with_the_element() {
        let point = rotation_handle_point(&rect(90.0), ROTATION_HANDLE_OFFSET_PX);
        assert!((point.0 - (200.0 + 25.0 + 24.0)).abs() < 1e-3);
        assert!((point.1 - 100.0).abs() < 1e-3);
    }

    #[test]
    fn the_pointer_angle_is_measured_from_the_positive_x_axis() {
        assert!((pointer_angle_degrees(1.0, 0.0) - 0.0).abs() < 1e-9);
        assert!((pointer_angle_degrees(0.0, 1.0) - 90.0).abs() < 1e-9);
        assert!((pointer_angle_degrees(-1.0, 0.0) - 180.0).abs() < 1e-9);
    }

    #[test]
    fn the_angle_delta_takes_the_short_way_round() {
        assert!((angle_delta_degrees(170.0, -170.0) - 20.0).abs() < 1e-9);
        assert!((angle_delta_degrees(-170.0, 170.0) + 20.0).abs() < 1e-9);
    }

    #[test]
    fn dragging_the_rotate_handle_a_quarter_turn_adds_ninety_degrees() {
        let rotated = rotation_from_pointer(30.0, -90.0, 0.0, false);
        assert!((rotated - 120.0).abs() < 1e-9);
    }

    #[test]
    fn rotation_snaps_to_the_quarter_turns_within_five_degrees() {
        assert_eq!(snap_rotation(88.0), (90.0, true));
        assert_eq!(snap_rotation(93.0), (90.0, true));
        assert_eq!(snap_rotation(84.0), (84.0, false));
        assert_eq!(snap_rotation(3.0), (0.0, true));
    }

    #[test]
    fn an_uncropped_element_reports_its_own_bounds() {
        let bounds = uncropped_bounds(&rect(0.0), &crop(0.0, 0.0, 0.0, 0.0));
        assert!((bounds.center_x - 200.0).abs() < 1e-4);
        assert!((bounds.center_y - 100.0).abs() < 1e-4);
        assert!((bounds.width - 100.0).abs() < 1e-4);
        assert!((bounds.height - 50.0).abs() < 1e-4);
    }

    #[test]
    fn a_left_crop_puts_the_full_frame_back_to_the_left() {
        let bounds = uncropped_bounds(&rect(0.0), &crop(0.5, 0.0, 0.0, 0.0));
        assert!((bounds.width - 200.0).abs() < 1e-4);
        assert!((bounds.center_x - 150.0).abs() < 1e-4);
        assert!((bounds.center_y - 100.0).abs() < 1e-4);
    }

    #[test]
    fn the_display_uv_of_the_full_frame_centre_is_the_middle() {
        let bounds = uncropped_bounds(&rect(0.0), &crop(0.25, 0.0, 0.0, 0.0));
        let (u, v) = crop_display_uv(&bounds, 0.0, bounds.center_x, bounds.center_y);
        assert!((u - 0.5).abs() < 1e-9);
        assert!((v - 0.5).abs() < 1e-9);
    }

    #[test]
    fn dragging_the_left_handle_to_a_quarter_sets_a_quarter_left_inset() {
        let next = apply_crop_handle_drag(
            CropHandle::Left,
            &crop(0.0, 0.0, 0.0, 0.0),
            0.25,
            0.5,
            false,
            false,
        );
        assert!((next.left - 0.25).abs() < 1e-9);
        assert_eq!(next.right, 0.0);
        assert_eq!(next.top, 0.0);
        assert_eq!(next.bottom, 0.0);
    }

    #[test]
    fn a_corner_drag_sets_both_axes() {
        let next = apply_crop_handle_drag(
            CropHandle::BottomRight,
            &crop(0.0, 0.0, 0.0, 0.0),
            0.8,
            0.6,
            false,
            false,
        );
        assert!((next.right - 0.2).abs() < 1e-9);
        assert!((next.bottom - 0.4).abs() < 1e-9);
        assert_eq!(next.left, 0.0);
        assert_eq!(next.top, 0.0);
    }

    #[test]
    fn a_flipped_element_drags_its_left_handle_into_the_right_inset() {
        let next = apply_crop_handle_drag(
            CropHandle::Left,
            &crop(0.0, 0.0, 0.0, 0.0),
            0.25,
            0.5,
            true,
            false,
        );
        assert!((next.right - 0.25).abs() < 1e-9);
        assert_eq!(next.left, 0.0);
    }

    #[test]
    fn dragging_a_handle_past_the_opposite_edge_stops_at_the_minimum_span() {
        let next = apply_crop_handle_drag(
            CropHandle::Left,
            &crop(0.0, 0.0, 0.3, 0.0),
            1.4,
            0.5,
            false,
            false,
        );
        assert!((next.left - (1.0 - MIN_CROP_SPAN - 0.3)).abs() < 1e-9);
        assert!(next.left + next.right <= 1.0 - MIN_CROP_SPAN + 1e-9);
    }

    #[test]
    fn dragging_a_handle_off_the_near_edge_clamps_to_zero() {
        let next = apply_crop_handle_drag(
            CropHandle::Top,
            &crop(0.0, 0.2, 0.0, 0.0),
            0.5,
            -0.4,
            false,
            false,
        );
        assert_eq!(next.top, 0.0);
    }

    #[test]
    fn a_centre_snap_wins_when_the_element_is_close_to_the_middle() {
        let result = snap_position(
            (3.0, -2.0),
            (1920.0, 1080.0),
            (200.0, 100.0),
            0.0,
            (8.0, 8.0),
        );
        assert_eq!(result.x, 0.0);
        assert_eq!(result.y, 0.0);
        assert_eq!(
            result.lines,
            vec![
                SnapLine {
                    axis: SnapAxis::Vertical,
                    position: 0.0
                },
                SnapLine {
                    axis: SnapAxis::Horizontal,
                    position: 0.0
                }
            ]
        );
    }

    #[test]
    fn an_edge_snap_puts_the_element_bound_on_the_canvas_edge() {
        let result = snap_position(
            (-855.0, 500.0),
            (1920.0, 1080.0),
            (200.0, 100.0),
            0.0,
            (8.0, 8.0),
        );
        assert!((result.x - -860.0).abs() < 1e-9);
        assert_eq!(
            result.lines[0],
            SnapLine {
                axis: SnapAxis::Vertical,
                position: -960.0
            }
        );
    }

    #[test]
    fn nothing_snaps_outside_the_threshold() {
        let result = snap_position(
            (400.0, 300.0),
            (1920.0, 1080.0),
            (200.0, 100.0),
            0.0,
            (8.0, 8.0),
        );
        assert_eq!(result.x, 400.0);
        assert_eq!(result.y, 300.0);
        assert!(result.lines.is_empty());
    }

    #[test]
    fn a_rotated_element_snaps_on_its_axis_aligned_bound() {
        let (half_width, half_height) = aabb_half_extents(200.0, 100.0, 90.0);
        assert!((half_width - 50.0).abs() < 1e-9);
        assert!((half_height - 100.0).abs() < 1e-9);
        let result = snap_position(
            (-908.0, 0.0),
            (1920.0, 1080.0),
            (200.0, 100.0),
            90.0,
            (8.0, 8.0),
        );
        assert!((result.x - -910.0).abs() < 1e-9);
    }

    #[test]
    fn the_closest_of_two_candidates_wins() {
        let result = snap_position((-956.0, 0.0), (1920.0, 1080.0), (8.0, 8.0), 0.0, (8.0, 8.0));
        assert!((result.x - -956.0).abs() < 1e-9);

        let result = snap_position((-959.0, 0.0), (1920.0, 1080.0), (8.0, 8.0), 0.0, (8.0, 8.0));
        assert!((result.x - -960.0).abs() < 1e-9);
    }

    #[test]
    fn an_unrotated_scale_snaps_the_right_edge_to_the_canvas_edge() {
        let (x, _) = snap_scale_axes(
            (1.0, 1.0),
            (860.0, 0.0),
            (200.0, 100.0),
            0.0,
            (1920.0, 1080.0),
            (8.0, 8.0),
        );
        assert!((x.scale - 1.0).abs() < 1e-9);

        let (x, _) = snap_scale_axes(
            (1.02, 1.0),
            (858.0, 0.0),
            (200.0, 100.0),
            0.0,
            (1920.0, 1080.0),
            (8.0, 8.0),
        );
        assert!((x.scale - 1.02).abs() < 1e-9);
    }

    #[test]
    fn a_scale_within_threshold_lands_exactly_on_the_edge() {
        let (x, _) = snap_scale_axes(
            (1.0, 1.0),
            (855.0, 0.0),
            (200.0, 100.0),
            0.0,
            (1920.0, 1080.0),
            (8.0, 8.0),
        );
        assert!((x.scale - 1.05).abs() < 1e-9);
        assert_eq!(
            x.lines,
            vec![SnapLine {
                axis: SnapAxis::Vertical,
                position: 960.0
            }]
        );
        assert!((855.0 + 200.0 * x.scale / 2.0 - 960.0).abs() < 1e-9);
    }

    #[test]
    fn a_scale_out_of_threshold_is_left_alone() {
        let (x, y) = snap_scale_axes(
            (1.0, 1.0),
            (0.0, 0.0),
            (200.0, 100.0),
            0.0,
            (1920.0, 1080.0),
            (8.0, 8.0),
        );
        assert_eq!(x.scale, 1.0);
        assert_eq!(y.scale, 1.0);
        assert!(x.distance.is_infinite());
    }

    #[test]
    fn a_three_by_three_grid_draws_two_lines_each_way() {
        let shapes = guide_shapes("grid", GridConfig::default(), &[], 900.0, 600.0);
        assert_eq!(
            shapes,
            vec![
                GuideShape::VLine { x: 300.0 },
                GuideShape::VLine { x: 600.0 },
                GuideShape::HLine { y: 200.0 },
                GuideShape::HLine { y: 400.0 },
            ]
        );
    }

    #[test]
    fn a_single_row_grid_draws_no_horizontal_line() {
        let shapes = guide_shapes("grid", GridConfig { rows: 1, cols: 2 }, &[], 900.0, 600.0);
        assert_eq!(shapes, vec![GuideShape::VLine { x: 450.0 }]);
    }

    #[test]
    fn the_grid_config_is_clamped_to_the_web_range() {
        let clamped = GridConfig { rows: 99, cols: 0 }.clamped();
        assert_eq!(clamped, GridConfig { rows: 24, cols: 1 });
    }

    #[test]
    fn every_platform_guide_draws_bands_inside_the_frame() {
        for id in ["tiktok", "ig-reels", "yt-shorts", "spotlight"] {
            let shapes = guide_shapes(id, GridConfig::default(), &[], 1080.0, 1920.0);
            assert!(!shapes.is_empty(), "{id} drew nothing");
            for shape in shapes {
                let GuideShape::Rect {
                    left,
                    top,
                    width,
                    height,
                } = shape
                else {
                    panic!("{id} drew a line rather than a band");
                };
                assert!(left >= 0.0 && top >= 0.0, "{id} band starts off-frame");
                assert!(
                    left + width <= 1080.0 + 1e-3,
                    "{id} band runs off the right"
                );
                assert!(
                    top + height <= 1920.0 + 1e-3,
                    "{id} band runs off the bottom"
                );
            }
        }
    }

    #[test]
    fn custom_lines_land_at_their_fraction_of_the_frame() {
        let shapes = guide_shapes(
            "custom",
            GridConfig::default(),
            &[
                CustomLine {
                    axis: SnapAxis::Vertical,
                    fraction: 0.25,
                },
                CustomLine {
                    axis: SnapAxis::Horizontal,
                    fraction: 0.5,
                },
            ],
            800.0,
            600.0,
        );
        assert_eq!(
            shapes,
            vec![
                GuideShape::VLine { x: 200.0 },
                GuideShape::HLine { y: 300.0 }
            ]
        );
    }

    #[test]
    fn an_unknown_guide_draws_nothing() {
        assert!(guide_shapes("nope", GridConfig::default(), &[], 800.0, 600.0).is_empty());
    }

    #[test]
    fn the_resize_cursor_follows_the_screen_angle() {
        assert_eq!(resize_cursor(0.0), ResizeCursor::EastWest);
        assert_eq!(resize_cursor(45.0), ResizeCursor::NorthWestSouthEast);
        assert_eq!(resize_cursor(90.0), ResizeCursor::NorthSouth);
        assert_eq!(resize_cursor(135.0), ResizeCursor::NorthEastSouthWest);
        assert_eq!(resize_cursor(-135.0), ResizeCursor::NorthWestSouthEast);
    }
}
