use cosmic_text::{
    Attrs, Buffer, Family, FontSystem, Metrics, Shaping, Style, SwashCache, SwashContent, Weight,
};
use cutix_project::color::parse_to_srgb_rgba;
use cutix_project::model::TextElement;
use serde_json::Value;
use time::MediaTime;

use crate::animation::{color_at, scalar_at};

pub const FONT_SIZE_SCALE_REFERENCE: f64 = 90.0;
pub const DEFAULT_FONT_SIZE: f64 = 15.0;
pub const DEFAULT_LINE_HEIGHT: f64 = 1.2;
pub const DEFAULT_PADDING_X: f64 = 30.0;
pub const DEFAULT_PADDING_Y: f64 = 42.0;
pub const CORNER_RADIUS_MIN: f64 = 0.0;
pub const CORNER_RADIUS_MAX: f64 = 100.0;
const DECORATION_THICKNESS_RATIO: f64 = 0.07;
const STRIKETHROUGH_VERTICAL_RATIO: f64 = 0.35;
const BITMAP_MARGIN_PX: f64 = 2.0;
const MAX_BITMAP_SIDE: u32 = 8192;

#[derive(Clone, Debug)]
pub struct TextLayer {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub anchor_x: f64,
    pub anchor_y: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Rect {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

impl Rect {
    fn right(&self) -> f64 {
        self.left + self.width
    }

    fn bottom(&self) -> f64 {
        self.top + self.height
    }

    fn inflate(&self, by: f64) -> Rect {
        Rect {
            left: self.left - by,
            top: self.top - by,
            width: self.width + by * 2.0,
            height: self.height + by * 2.0,
        }
    }
}

struct Paint {
    color: [f64; 4],
}

struct Stroke {
    color: [f64; 4],
    width: f64,
}

struct Shadow {
    color: [f64; 4],
    blur: f64,
    offset_x: f64,
    offset_y: f64,
}

struct Gradient {
    from: [f64; 4],
    to: [f64; 4],
    angle: f64,
}

fn flag(value: Option<&Value>, key: &str) -> bool {
    value
        .and_then(|value| value.get(key))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn number(value: Option<&Value>, key: &str, fallback: f64) -> f64 {
    value
        .and_then(|value| value.get(key))
        .and_then(Value::as_f64)
        .unwrap_or(fallback)
}

fn color(value: Option<&Value>, key: &str, fallback: [f64; 4]) -> [f64; 4] {
    value
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .and_then(parse_to_srgb_rgba)
        .unwrap_or(fallback)
}

fn stroke_of(element: &TextElement, unit_scale: f64) -> Option<Stroke> {
    let source = element.stroke.as_ref();
    if !flag(source, "enabled") {
        return None;
    }
    let width = number(source, "width", 0.0) * unit_scale;
    if width <= 0.0 {
        return None;
    }
    let color = color(source, "color", [0.0, 0.0, 0.0, 1.0]);
    (color[3] > 0.0).then_some(Stroke { color, width })
}

fn shadow_of(element: &TextElement, unit_scale: f64) -> Option<Shadow> {
    let source = element.shadow.as_ref();
    if !flag(source, "enabled") {
        return None;
    }
    let blur = number(source, "blur", 0.0).max(0.0) * unit_scale;
    let offset_x = number(source, "offsetX", 0.0) * unit_scale;
    let offset_y = number(source, "offsetY", 0.0) * unit_scale;
    if blur <= 0.0 && offset_x == 0.0 && offset_y == 0.0 {
        return None;
    }
    let color = color(source, "color", [0.0, 0.0, 0.0, 1.0]);
    (color[3] > 0.0).then_some(Shadow {
        color,
        blur,
        offset_x,
        offset_y,
    })
}

fn gradient_of(element: &TextElement) -> Option<Gradient> {
    let source = element.gradient.as_ref();
    if !flag(source, "enabled") {
        return None;
    }
    Some(Gradient {
        from: color(source, "from", [1.0, 1.0, 1.0, 1.0]),
        to: color(source, "to", [1.0, 1.0, 1.0, 1.0]),
        angle: number(source, "angle", 90.0),
    })
}

struct GlyphDraw {
    left: i32,
    top: i32,
    width: usize,
    height: usize,
    coverage: Vec<u8>,
}

pub struct TextRasterizer {
    fonts: FontSystem,
    cache: SwashCache,
}

impl Default for TextRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextRasterizer {
    pub fn new() -> Self {
        Self {
            fonts: FontSystem::new(),
            cache: SwashCache::new(),
        }
    }

    pub fn rasterize(
        &mut self,
        element: &TextElement,
        canvas_height: f64,
        local: MediaTime,
    ) -> Option<TextLayer> {
        let unit_scale = canvas_height / FONT_SIZE_SCALE_REFERENCE;
        let font_size = element.font_size;
        let scaled_font_size = font_size * unit_scale;
        if !scaled_font_size.is_finite() || scaled_font_size <= 0.0 {
            return None;
        }
        if element.content.trim().is_empty() {
            return None;
        }
        let animations = element.base.animations.as_ref();
        let size_ratio = if DEFAULT_FONT_SIZE > 0.0 {
            font_size / DEFAULT_FONT_SIZE
        } else {
            1.0
        };
        let line_height_px =
            scaled_font_size * element.line_height.unwrap_or(DEFAULT_LINE_HEIGHT).max(0.1);
        let letter_spacing = element.letter_spacing.unwrap_or(0.0);

        let mut buffer = Buffer::new(
            &mut self.fonts,
            Metrics::new(scaled_font_size as f32, line_height_px as f32),
        );
        buffer.set_size(&mut self.fonts, None, None);
        let mut attrs = Attrs::new().family(Family::Name(&element.font_family));
        if element.font_weight == "bold" {
            attrs = attrs.weight(Weight::BOLD);
        }
        if element.font_style == "italic" {
            attrs = attrs.style(Style::Italic);
        }
        if letter_spacing != 0.0 {
            attrs = attrs.letter_spacing(letter_spacing as f32);
        }
        buffer.set_text(&mut self.fonts, &element.content, &attrs, Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.fonts, false);

        let mut lines: Vec<(f64, f64, Vec<cosmic_text::PhysicalGlyph>)> = Vec::new();
        let mut max_width: f64 = 0.0;
        for run in buffer.layout_runs() {
            let baseline = (run.line_y - run.line_top) as f64 - line_height_px / 2.0;
            let glyphs = run
                .glyphs
                .iter()
                .map(|glyph| glyph.physical((0.0, 0.0), 1.0))
                .collect();
            max_width = max_width.max(run.line_w as f64);
            lines.push((run.line_w as f64, baseline, glyphs));
        }
        if lines.is_empty() {
            return None;
        }

        let line_count = lines.len() as f64;
        let block = Rect {
            left: match element.text_align.as_str() {
                "left" => 0.0,
                "right" => -max_width,
                _ => -max_width / 2.0,
            },
            top: -(line_count * line_height_px) / 2.0,
            width: max_width,
            height: line_count * line_height_px,
        };
        let visual_center_offset = ((line_count - 1.0) * line_height_px) / 2.0;

        let text_color = color_at(animations, "color", &element.color, local);
        let background = self.background_rect(element, &block, size_ratio, local);
        let stroke = stroke_of(element, unit_scale);
        let shadow = shadow_of(element, unit_scale);
        let gradient = gradient_of(element);

        let mut draws: Vec<GlyphDraw> = Vec::new();
        let mut decorations: Vec<Rect> = Vec::new();
        let mut ink = Rect {
            left: block.left,
            top: block.top,
            width: block.width,
            height: block.height,
        };
        let mut ink_left = ink.left;
        let mut ink_top = ink.top;
        let mut ink_right = ink.right();
        let mut ink_bottom = ink.bottom();

        for (index, (line_width, baseline, glyphs)) in lines.iter().enumerate() {
            let line_center = index as f64 * line_height_px - visual_center_offset;
            let line_start = match element.text_align.as_str() {
                "left" => 0.0,
                "right" => -line_width,
                _ => -line_width / 2.0,
            };
            let baseline_y = line_center + baseline;

            for physical in glyphs {
                let Some(image) = self.cache.get_image(&mut self.fonts, physical.cache_key) else {
                    continue;
                };
                if image.placement.width == 0 || image.placement.height == 0 {
                    continue;
                }
                if image.content != SwashContent::Mask
                    && image.content != SwashContent::SubpixelMask
                {
                    continue;
                }

                let left = line_start + physical.x as f64 + image.placement.left as f64;
                let top = baseline_y + physical.y as f64 - image.placement.top as f64;
                let width = image.placement.width as usize;
                let height = image.placement.height as usize;
                let stride = image.data.len() / height.max(1);
                let coverage = if stride >= width * 4 {
                    image
                        .data
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .map(|pixel| pixel[3])
                        .collect::<Vec<u8>>()
                } else {
                    image.data.clone()
                };
                if coverage.len() < width * height {
                    continue;
                }
                ink_left = ink_left.min(left);
                ink_top = ink_top.min(top);
                ink_right = ink_right.max(left + width as f64);
                ink_bottom = ink_bottom.max(top + height as f64);
                draws.push(GlyphDraw {
                    left: left.floor() as i32,
                    top: top.floor() as i32,
                    width,
                    height,
                    coverage,
                });
            }

            if let Some(rect) = decoration_rect(
                element,
                *line_width,
                line_center,
                line_start,
                scaled_font_size,
            ) {
                ink_left = ink_left.min(rect.left);
                ink_top = ink_top.min(rect.top);
                ink_right = ink_right.max(rect.right());
                ink_bottom = ink_bottom.max(rect.bottom());
                decorations.push(rect);
            }
        }

        ink = Rect {
            left: ink_left,
            top: ink_top,
            width: ink_right - ink_left,
            height: ink_bottom - ink_top,
        };

        apply_character_reveal(&mut draws, element, local);
        if draws.is_empty() && decorations.is_empty() && background.is_none() {
            return None;
        }

        let stroke_spread = stroke
            .as_ref()
            .map(|stroke| stroke.width / 2.0)
            .unwrap_or(0.0);
        let mut bounds = ink.inflate(stroke_spread);
        if let Some(rect) = background {
            bounds = union(bounds, rect);
        }
        if let Some(shadow) = shadow.as_ref() {
            let shifted = Rect {
                left: ink.left + shadow.offset_x,
                top: ink.top + shadow.offset_y,
                width: ink.width,
                height: ink.height,
            };
            bounds = union(bounds, shifted.inflate(shadow.blur + stroke_spread));
        }
        bounds = bounds.inflate(BITMAP_MARGIN_PX);

        let width = bounds.width.ceil().max(1.0) as u32;
        let height = bounds.height.ceil().max(1.0) as u32;
        if width > MAX_BITMAP_SIDE || height > MAX_BITMAP_SIDE {
            return None;
        }
        let anchor_x = -bounds.left;
        let anchor_y = -bounds.top;
        let pixels = (width as usize) * (height as usize);

        let mut mask = vec![0.0f32; pixels];
        for draw in &draws {
            blit_coverage(&mut mask, width, height, draw, anchor_x, anchor_y);
        }
        for rect in &decorations {
            fill_rect(&mut mask, width, height, rect, anchor_x, anchor_y);
        }

        let mut canvas = vec![0.0f32; pixels * 4];
        if let Some(rect) = background {
            let radius = background_radius(element, &rect, local, animations);
            let fill = color_at(
                animations,
                "background.color",
                &element.background.color,
                local,
            );
            let mut coverage = vec![0.0f32; pixels];
            fill_rounded_rect(
                &mut coverage,
                width,
                height,
                &rect,
                radius,
                anchor_x,
                anchor_y,
            );
            composite(&mut canvas, &coverage, Paint { color: fill }, None);
        }
        if let Some(shadow) = shadow.as_ref() {
            let mut spread = mask.clone();
            if stroke_spread > 0.0 {
                dilate(&mut spread, width, height, stroke_spread);
            }
            let mut shifted = vec![0.0f32; pixels];
            shift(
                &spread,
                &mut shifted,
                width,
                height,
                shadow.offset_x,
                shadow.offset_y,
            );
            if shadow.blur > 0.0 {
                blur(&mut shifted, width, height, shadow.blur / 2.0);
            }
            composite(
                &mut canvas,
                &shifted,
                Paint {
                    color: shadow.color,
                },
                None,
            );
        }
        if let Some(stroke) = stroke.as_ref() {
            let mut spread = mask.clone();
            dilate(&mut spread, width, height, stroke.width / 2.0);
            composite(
                &mut canvas,
                &spread,
                Paint {
                    color: stroke.color,
                },
                None,
            );
        }
        let fill_gradient = gradient
            .map(|gradient| gradient_field(&gradient, &block, width, height, anchor_x, anchor_y));
        composite(
            &mut canvas,
            &mask,
            Paint { color: text_color },
            fill_gradient.as_deref(),
        );

        let mut rgba = vec![0u8; pixels * 4];
        for index in 0..pixels {
            let alpha = canvas[index * 4 + 3];
            let to_byte = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            if alpha <= 0.0 {
                continue;
            }
            rgba[index * 4] = to_byte(canvas[index * 4] / alpha);
            rgba[index * 4 + 1] = to_byte(canvas[index * 4 + 1] / alpha);
            rgba[index * 4 + 2] = to_byte(canvas[index * 4 + 2] / alpha);
            rgba[index * 4 + 3] = to_byte(alpha);
        }

        Some(TextLayer {
            width,
            height,
            rgba,
            anchor_x,
            anchor_y,
        })
    }

    fn background_rect(
        &self,
        element: &TextElement,
        block: &Rect,
        size_ratio: f64,
        local: MediaTime,
    ) -> Option<Rect> {
        let background = &element.background;
        if !background.enabled {
            return None;
        }
        if parse_to_srgb_rgba(&background.color).map(|rgba| rgba[3]) == Some(0.0) {
            return None;
        }
        let animations = element.base.animations.as_ref();
        let padding_x = scalar_at(
            animations,
            "background.paddingX",
            background.padding_x.unwrap_or(DEFAULT_PADDING_X),
            local,
        ) * size_ratio;
        let padding_y = scalar_at(
            animations,
            "background.paddingY",
            background.padding_y.unwrap_or(DEFAULT_PADDING_Y),
            local,
        ) * size_ratio;
        let offset_x = scalar_at(
            animations,
            "background.offsetX",
            background.offset_x.unwrap_or(0.0),
            local,
        );
        let offset_y = scalar_at(
            animations,
            "background.offsetY",
            background.offset_y.unwrap_or(0.0),
            local,
        );

        Some(Rect {
            left: block.left - padding_x + offset_x,
            top: block.top - padding_y + offset_y,
            width: block.width + padding_x * 2.0,
            height: block.height + padding_y * 2.0,
        })
    }
}

fn background_radius(
    element: &TextElement,
    rect: &Rect,
    local: MediaTime,
    animations: Option<&cutix_project::ElementAnimations>,
) -> f64 {
    let percent = scalar_at(
        animations,
        "background.cornerRadius",
        element
            .background
            .corner_radius
            .unwrap_or(CORNER_RADIUS_MIN),
        local,
    )
    .clamp(CORNER_RADIUS_MIN, CORNER_RADIUS_MAX)
        / 100.0;
    (rect.width.min(rect.height) / 2.0) * percent
}

fn decoration_rect(
    element: &TextElement,
    line_width: f64,
    line_center: f64,
    line_start: f64,
    scaled_font_size: f64,
) -> Option<Rect> {
    let thickness = (scaled_font_size * DECORATION_THICKNESS_RATIO).max(1.0);
    let ascent = scaled_font_size * 0.8;
    let descent = scaled_font_size * 0.2;
    match element.text_decoration.as_str() {
        "underline" => Some(Rect {
            left: line_start,
            top: line_center + descent + thickness,
            width: line_width,
            height: thickness,
        }),
        "line-through" => Some(Rect {
            left: line_start,
            top: line_center - (ascent - descent) * STRIKETHROUGH_VERTICAL_RATIO,
            width: line_width,
            height: thickness,
        }),
        _ => None,
    }
}

fn union(left: Rect, right: Rect) -> Rect {
    let l = left.left.min(right.left);
    let t = left.top.min(right.top);
    let r = left.right().max(right.right());
    let b = left.bottom().max(right.bottom());
    Rect {
        left: l,
        top: t,
        width: r - l,
        height: b - t,
    }
}

fn blit_coverage(
    mask: &mut [f32],
    width: u32,
    height: u32,
    draw: &GlyphDraw,
    anchor_x: f64,
    anchor_y: f64,
) {
    let base_x = draw.left + anchor_x.floor() as i32;
    let base_y = draw.top + anchor_y.floor() as i32;
    for row in 0..draw.height {
        let y = base_y + row as i32;
        if y < 0 || y >= height as i32 {
            continue;
        }
        for column in 0..draw.width {
            let x = base_x + column as i32;
            if x < 0 || x >= width as i32 {
                continue;
            }
            let value = draw.coverage[row * draw.width + column] as f32 / 255.0;
            let index = (y as usize) * (width as usize) + x as usize;
            mask[index] = mask[index].max(value);
        }
    }
}

fn fill_rect(mask: &mut [f32], width: u32, height: u32, rect: &Rect, anchor_x: f64, anchor_y: f64) {
    let left = rect.left + anchor_x;
    let top = rect.top + anchor_y;
    for y in 0..height {
        let center_y = y as f64 + 0.5;
        if center_y < top || center_y > top + rect.height {
            continue;
        }
        for x in 0..width {
            let center_x = x as f64 + 0.5;
            if center_x < left || center_x > left + rect.width {
                continue;
            }
            let index = (y as usize) * (width as usize) + x as usize;
            mask[index] = 1.0;
        }
    }
}

fn fill_rounded_rect(
    coverage: &mut [f32],
    width: u32,
    height: u32,
    rect: &Rect,
    radius: f64,
    anchor_x: f64,
    anchor_y: f64,
) {
    let left = rect.left + anchor_x;
    let top = rect.top + anchor_y;
    let half_width = rect.width / 2.0;
    let half_height = rect.height / 2.0;
    let center_x = left + half_width;
    let center_y = top + half_height;
    let radius = radius.clamp(0.0, half_width.min(half_height));

    for y in 0..height {
        for x in 0..width {
            let point_x = (x as f64 + 0.5 - center_x).abs() - (half_width - radius);
            let point_y = (y as f64 + 0.5 - center_y).abs() - (half_height - radius);
            let outside = (point_x.max(0.0).powi(2) + point_y.max(0.0).powi(2)).sqrt();
            let inside = point_x.max(point_y).min(0.0);
            let distance = outside + inside - radius;
            let value = (0.5 - distance).clamp(0.0, 1.0);
            if value > 0.0 {
                coverage[(y as usize) * (width as usize) + x as usize] = value as f32;
            }
        }
    }
}

fn composite(canvas: &mut [f32], coverage: &[f32], paint: Paint, gradient: Option<&[f32]>) {
    for index in 0..coverage.len() {
        let alpha = coverage[index] * paint.color[3] as f32;
        if alpha <= 0.0 {
            continue;
        }
        let (red, green, blue) = match gradient {
            Some(field) => (field[index * 3], field[index * 3 + 1], field[index * 3 + 2]),
            None => (
                paint.color[0] as f32,
                paint.color[1] as f32,
                paint.color[2] as f32,
            ),
        };
        let slot = index * 4;
        canvas[slot] = red * alpha + canvas[slot] * (1.0 - alpha);
        canvas[slot + 1] = green * alpha + canvas[slot + 1] * (1.0 - alpha);
        canvas[slot + 2] = blue * alpha + canvas[slot + 2] * (1.0 - alpha);
        canvas[slot + 3] = alpha + canvas[slot + 3] * (1.0 - alpha);
    }
}

fn gradient_field(
    gradient: &Gradient,
    block: &Rect,
    width: u32,
    height: u32,
    anchor_x: f64,
    anchor_y: f64,
) -> Vec<f32> {
    let radians = gradient.angle.to_radians();
    let half_width = block.width / 2.0;
    let half_height = block.height / 2.0;
    let center_x = block.left + half_width + anchor_x;
    let center_y = block.top + half_height + anchor_y;
    let extent_x = radians.cos() * half_width;
    let extent_y = radians.sin() * half_height;
    let span = extent_x * extent_x + extent_y * extent_y;

    let mut field = vec![0.0f32; (width as usize) * (height as usize) * 3];
    for y in 0..height {
        for x in 0..width {
            let index = (y as usize) * (width as usize) + x as usize;
            let progress = if span <= 0.0 {
                0.0
            } else {
                (((x as f64 + 0.5 - (center_x - extent_x)) * (2.0 * extent_x)
                    + (y as f64 + 0.5 - (center_y - extent_y)) * (2.0 * extent_y))
                    / (4.0 * span))
                    .clamp(0.0, 1.0)
            };
            for channel in 0..3 {
                field[index * 3 + channel] = (gradient.from[channel]
                    + (gradient.to[channel] - gradient.from[channel]) * progress)
                    as f32;
            }
        }
    }
    field
}

fn shift(source: &[f32], target: &mut [f32], width: u32, height: u32, dx: f64, dy: f64) {
    let dx = dx.round() as i32;
    let dy = dy.round() as i32;
    for y in 0..height as i32 {
        let from_y = y - dy;
        if from_y < 0 || from_y >= height as i32 {
            continue;
        }
        for x in 0..width as i32 {
            let from_x = x - dx;
            if from_x < 0 || from_x >= width as i32 {
                continue;
            }
            target[(y as usize) * (width as usize) + x as usize] =
                source[(from_y as usize) * (width as usize) + from_x as usize];
        }
    }
}

fn dilate(mask: &mut [f32], width: u32, height: u32, radius: f64) {
    let radius = radius.round().max(0.0) as i32;
    if radius == 0 {
        return;
    }
    let width = width as i32;
    let height = height as i32;
    let mut row_pass = mask.to_vec();
    for y in 0..height {
        for x in 0..width {
            let mut best = 0.0f32;
            for offset in -radius..=radius {
                let sample = x + offset;
                if sample < 0 || sample >= width {
                    continue;
                }
                best = best.max(mask[(y * width + sample) as usize]);
            }
            row_pass[(y * width + x) as usize] = best;
        }
    }
    for y in 0..height {
        for x in 0..width {
            let mut best = 0.0f32;
            for offset in -radius..=radius {
                let sample = y + offset;
                if sample < 0 || sample >= height {
                    continue;
                }
                best = best.max(row_pass[(sample * width + x) as usize]);
            }
            mask[(y * width + x) as usize] = best;
        }
    }
}

fn blur(mask: &mut [f32], width: u32, height: u32, radius: f64) {
    let radius = radius.round().max(0.0) as i32;
    if radius == 0 {
        return;
    }
    for _ in 0..2 {
        box_pass(mask, width as i32, height as i32, radius, true);
        box_pass(mask, width as i32, height as i32, radius, false);
    }
}

fn box_pass(mask: &mut [f32], width: i32, height: i32, radius: i32, horizontal: bool) {
    let source = mask.to_vec();
    let count = (radius * 2 + 1) as f32;
    for y in 0..height {
        for x in 0..width {
            let mut total = 0.0f32;
            for offset in -radius..=radius {
                let (sample_x, sample_y) = if horizontal {
                    (x + offset, y)
                } else {
                    (x, y + offset)
                };
                if sample_x < 0 || sample_x >= width || sample_y < 0 || sample_y >= height {
                    continue;
                }
                total += source[(sample_y * width + sample_x) as usize];
            }
            mask[(y * width + x) as usize] = total / count;
        }
    }
}

fn apply_character_reveal(draws: &mut Vec<GlyphDraw>, element: &TextElement, local: MediaTime) {
    let Some((style, duration)) = reveal_setting(element) else {
        return;
    };
    if duration <= 0 || draws.is_empty() {
        return;
    }

    let progress = (local.as_ticks() as f64 / duration as f64).clamp(0.0, 1.0);
    if progress >= 1.0 {
        return;
    }

    let count = draws.len();
    let mut index = 0;
    draws.retain_mut(|draw| {
        let character = character_reveal_progress(progress, index, count, &style);
        index += 1;
        if character <= 0.0 {
            return false;
        }
        if character < 1.0 {
            let scale = character_reveal_scale(character, &style);
            if scale < 1.0 {
                scale_coverage(draw, scale);
            }
            let alpha = if style == "typewriter" {
                1.0
            } else {
                character
            };
            if alpha < 1.0 {
                for value in draw.coverage.iter_mut() {
                    *value = (*value as f64 * alpha).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
        true
    });
}

fn reveal_setting(element: &TextElement) -> Option<(String, i64)> {
    let reveal = element.text_animations.as_ref()?.get("reveal")?;
    let style = reveal.get("presetId")?.as_str()?.to_string();
    if !matches!(style.as_str(), "typewriter" | "char-fade" | "char-pop") {
        return None;
    }
    Some((style, reveal.get("duration")?.as_i64()?))
}

const REVEAL_STAGGER_SPREAD: f64 = 0.75;
const REVEAL_POP_MIN_SCALE: f64 = 0.35;
const REVEAL_BACK_OVERSHOOT: f64 = 1.70158;

fn character_reveal_progress(progress: f64, index: usize, count: usize, style: &str) -> f64 {
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

    let spread = if count > 1 {
        REVEAL_STAGGER_SPREAD
    } else {
        0.0
    };
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

fn character_reveal_scale(progress: f64, style: &str) -> f64 {
    if style != "char-pop" || progress >= 1.0 {
        return 1.0;
    }
    let shifted = progress - 1.0;
    let eased = 1.0
        + (REVEAL_BACK_OVERSHOOT + 1.0) * shifted * shifted * shifted
        + REVEAL_BACK_OVERSHOOT * shifted * shifted;
    REVEAL_POP_MIN_SCALE + (1.0 - REVEAL_POP_MIN_SCALE) * eased
}

fn scale_coverage(draw: &mut GlyphDraw, scale: f64) {
    if scale <= 0.0 || draw.width == 0 || draw.height == 0 {
        draw.coverage.iter_mut().for_each(|value| *value = 0);
        return;
    }

    let width = draw.width;
    let height = draw.height;
    let center_x = width as f64 / 2.0;
    let center_y = height as f64 / 2.0;
    let source = draw.coverage.clone();
    let mut scaled = vec![0u8; width * height];

    for y in 0..height {
        let source_y = center_y + (y as f64 + 0.5 - center_y) / scale - 0.5;
        if source_y < 0.0 || source_y >= height as f64 {
            continue;
        }
        let row = source_y.round().clamp(0.0, (height - 1) as f64) as usize;
        for x in 0..width {
            let source_x = center_x + (x as f64 + 0.5 - center_x) / scale - 0.5;
            if source_x < 0.0 || source_x >= width as f64 {
                continue;
            }
            let column = source_x.round().clamp(0.0, (width - 1) as f64) as usize;
            scaled[y * width + x] = source[row * width + column];
        }
    }

    draw.coverage = scaled;
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutix_project::model::{BaseElementFields, JsonMap, TextBackground, Transform};

    fn text(content: &str) -> TextElement {
        TextElement {
            base: BaseElementFields {
                id: "text".into(),
                name: "text".into(),
                duration: MediaTime::from_seconds_f64(5.0).unwrap(),
                start_time: MediaTime::ZERO,
                trim_start: MediaTime::ZERO,
                trim_end: MediaTime::ZERO,
                source_duration: None,
                animations: None,
            },
            content: content.into(),
            font_size: 15.0,
            font_family: "Arial".into(),
            color: "#ffffff".into(),
            background: TextBackground {
                enabled: false,
                color: "#000000".into(),
                corner_radius: None,
                padding_x: None,
                padding_y: None,
                offset_x: None,
                offset_y: None,
            },
            stroke: None,
            shadow: None,
            gradient: None,
            text_animations: None,
            text_align: "center".into(),
            font_weight: "normal".into(),
            font_style: "normal".into(),
            text_decoration: "none".into(),
            letter_spacing: None,
            line_height: None,
            hidden: None,
            transform: Transform::default(),
            opacity: 1.0,
            blend_mode: None,
            effects: None,
            extra: JsonMap::new(),
        }
    }

    #[test]
    fn a_rasterised_line_has_opaque_ink() {
        let mut rasterizer = TextRasterizer::new();
        let layer = rasterizer
            .rasterize(&text("Hello"), 1080.0, MediaTime::ZERO)
            .expect("layer");
        assert!(layer.width > 10 && layer.height > 10);
        let opaque = layer
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 200)
            .count();
        assert!(opaque > 50, "{opaque} opaque pixels");
    }

    #[test]
    fn empty_content_rasterises_to_nothing() {
        let mut rasterizer = TextRasterizer::new();
        assert!(
            rasterizer
                .rasterize(&text(""), 1080.0, MediaTime::ZERO)
                .is_none()
        );
    }

    #[test]
    fn a_background_paints_behind_the_glyphs() {
        let mut rasterizer = TextRasterizer::new();
        let mut element = text("Hi");
        element.background.enabled = true;
        element.background.color = "#ff0000".into();
        let layer = rasterizer
            .rasterize(&element, 1080.0, MediaTime::ZERO)
            .expect("layer");
        let red = layer
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[0] > 200 && pixel[1] < 40 && pixel[3] > 200)
            .count();
        assert!(red > 100, "{red} background pixels");
    }

    fn with_reveal(content: &str, style: &str, seconds: f64) -> TextElement {
        let mut element = text(content);
        element.text_animations = Some(serde_json::json!({
            "reveal": {
                "presetId": style,
                "duration": (time::TICKS_PER_SECOND as f64 * seconds) as i64,
            }
        }));
        element
    }

    fn ink(layer: &TextLayer) -> usize {
        layer
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 8)
            .count()
    }

    #[test]
    fn the_typewriter_grows_the_visible_ink_over_the_window() {
        let mut rasterizer = TextRasterizer::new();
        let element = with_reveal("ABCDEFGH", "typewriter", 1.0);
        let quarter = rasterizer
            .rasterize(
                &element,
                1080.0,
                MediaTime::from_ticks(time::TICKS_PER_SECOND / 4),
            )
            .expect("quarter");
        let three_quarters = rasterizer
            .rasterize(
                &element,
                1080.0,
                MediaTime::from_ticks(time::TICKS_PER_SECOND * 3 / 4),
            )
            .expect("three quarters");
        let finished = rasterizer
            .rasterize(
                &element,
                1080.0,
                MediaTime::from_ticks(time::TICKS_PER_SECOND),
            )
            .expect("finished");

        assert!(
            ink(&quarter) < ink(&three_quarters),
            "{} vs {}",
            ink(&quarter),
            ink(&three_quarters)
        );
        assert!(ink(&three_quarters) < ink(&finished));
    }

    #[test]
    fn a_finished_reveal_matches_the_unanimated_layer() {
        let mut rasterizer = TextRasterizer::new();
        let plain = rasterizer
            .rasterize(&text("ABCDEFGH"), 1080.0, MediaTime::ZERO)
            .expect("plain");
        let done = rasterizer
            .rasterize(
                &with_reveal("ABCDEFGH", "char-fade", 1.0),
                1080.0,
                MediaTime::from_ticks(time::TICKS_PER_SECOND * 2),
            )
            .expect("done");
        assert_eq!(plain.rgba, done.rgba);
    }

    #[test]
    fn char_fade_ramps_alpha_rather_than_snapping_glyphs_on() {
        let mut rasterizer = TextRasterizer::new();
        let element = with_reveal("ABCDEFGH", "char-fade", 1.0);
        let mid = rasterizer
            .rasterize(
                &element,
                1080.0,
                MediaTime::from_ticks(time::TICKS_PER_SECOND / 2),
            )
            .expect("mid");
        let partial = mid
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 8 && pixel[3] < 240)
            .count();
        assert!(partial > 20, "{partial} partially transparent pixels");
    }

    #[test]
    fn char_pop_shrinks_the_ink_below_the_settled_size() {
        let mut rasterizer = TextRasterizer::new();
        let early = rasterizer
            .rasterize(
                &with_reveal("ABCDEFGH", "char-pop", 1.0),
                1080.0,
                MediaTime::from_ticks(time::TICKS_PER_SECOND / 10),
            )
            .expect("early");
        let settled = rasterizer
            .rasterize(&text("ABCDEFGH"), 1080.0, MediaTime::ZERO)
            .expect("settled");
        assert!(
            ink(&early) < ink(&settled),
            "{} vs {}",
            ink(&early),
            ink(&settled)
        );
    }

    #[test]
    fn a_reveal_that_has_not_started_hides_every_glyph() {
        let mut rasterizer = TextRasterizer::new();
        assert!(
            rasterizer
                .rasterize(
                    &with_reveal("ABCDEFGH", "typewriter", 1.0),
                    1080.0,
                    MediaTime::ZERO
                )
                .is_none()
        );
    }

    #[test]
    fn two_lines_are_taller_than_one() {
        let mut rasterizer = TextRasterizer::new();
        let single = rasterizer
            .rasterize(&text("Hello"), 1080.0, MediaTime::ZERO)
            .expect("single");
        let double = rasterizer
            .rasterize(&text("Hello\nWorld"), 1080.0, MediaTime::ZERO)
            .expect("double");
        assert!(double.height > single.height);
    }
}

pub const SUBTITLE_MAX_WIDTH_RATIO: f64 = 0.8;

impl TextRasterizer {
    pub fn measure_line(
        &mut self,
        text: &str,
        font_family: &str,
        bold: bool,
        font_size_px: f64,
    ) -> f64 {
        if text.is_empty() || font_size_px <= 0.0 {
            return 0.0;
        }
        let mut buffer = Buffer::new(
            &mut self.fonts,
            Metrics::new(font_size_px as f32, font_size_px as f32),
        );
        buffer.set_size(&mut self.fonts, None, None);
        let mut attrs = Attrs::new().family(Family::Name(font_family));
        if bold {
            attrs = attrs.weight(Weight::BOLD);
        }
        buffer.set_text(&mut self.fonts, text, &attrs, Shaping::Advanced);
        buffer.shape_until_scroll(&mut self.fonts, false);
        buffer
            .layout_runs()
            .fold(0.0_f64, |widest, run| widest.max(run.line_w as f64))
    }

    pub fn wrap_text(
        &mut self,
        text: &str,
        font_family: &str,
        bold: bool,
        font_size_px: f64,
        max_width_px: f64,
    ) -> String {
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let mut paragraphs = Vec::new();

        for paragraph in normalized.trim().split('\n') {
            let trimmed = paragraph.trim();
            if trimmed.is_empty() {
                paragraphs.push(String::new());
                continue;
            }
            let words: Vec<&str> = trimmed.split_whitespace().collect();
            let mut lines: Vec<String> = Vec::new();
            let mut current = String::from(words[0]);
            for word in &words[1..] {
                let candidate = format!("{current} {word}");
                if self.measure_line(&candidate, font_family, bold, font_size_px) <= max_width_px {
                    current = candidate;
                    continue;
                }
                lines.push(std::mem::replace(&mut current, (*word).to_owned()));
            }
            lines.push(current);
            paragraphs.push(lines.join("\n"));
        }

        paragraphs.join("\n")
    }
}

#[cfg(test)]
mod wrap_tests {
    use super::*;

    #[test]
    fn short_lines_are_left_alone() {
        let mut rasterizer = TextRasterizer::new();
        assert_eq!(
            rasterizer.wrap_text("Hello", "Arial", true, 60.0, 1536.0),
            "Hello"
        );
    }

    #[test]
    fn a_long_line_breaks_and_every_line_fits() {
        let mut rasterizer = TextRasterizer::new();
        let text =
            "The quick brown fox jumps over the lazy dog while the whole timeline scrubs past";
        let max_width = 600.0;
        let wrapped = rasterizer.wrap_text(text, "Arial", true, 60.0, max_width);

        assert!(wrapped.contains('\n'), "expected a wrap: {wrapped:?}");
        for line in wrapped.lines() {
            let width = rasterizer.measure_line(line, "Arial", true, 60.0);
            let single_word = !line.contains(' ');
            assert!(
                width <= max_width || single_word,
                "line {line:?} is {width} wide, limit {max_width}"
            );
        }
        let joined: Vec<&str> = wrapped.split_whitespace().collect();
        assert_eq!(joined.join(" "), text, "wrapping must not lose words");
    }

    #[test]
    fn source_line_breaks_are_kept() {
        let mut rasterizer = TextRasterizer::new();
        let wrapped = rasterizer.wrap_text("one\ntwo", "Arial", true, 60.0, 4000.0);
        assert_eq!(wrapped, "one\ntwo");
    }

    #[test]
    fn measuring_grows_with_the_text() {
        let mut rasterizer = TextRasterizer::new();
        let short = rasterizer.measure_line("ab", "Arial", true, 60.0);
        let long = rasterizer.measure_line("abcdefgh", "Arial", true, 60.0);
        assert!(short > 0.0);
        assert!(long > short * 2.0, "{short} vs {long}");
        assert_eq!(rasterizer.measure_line("", "Arial", true, 60.0), 0.0);
    }
}
