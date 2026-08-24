use serde_json::{Map, Value};
use tiny_skia::{
    BlendMode, Color, FillRule, Mask, Paint, PathBuilder, Pixmap, Shader, Stroke, Transform,
};

pub const DEFAULT_GRAPHIC_SOURCE_SIZE: u32 = 512;

pub type ParamValues = Map<String, Value>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParamKind {
    Color,
    Number,
    Select,
}

#[derive(Clone, Copy, Debug)]
pub struct ParamDefinition {
    pub key: &'static str,
    pub label_key: &'static str,
    pub kind: ParamKind,
    pub default_color: &'static str,
    pub default_number: f64,
    pub default_select: &'static str,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub group: Option<&'static str>,
    pub options: &'static [(&'static str, &'static str)],
}

const fn color_param(
    key: &'static str,
    label_key: &'static str,
    default: &'static str,
    group: Option<&'static str>,
) -> ParamDefinition {
    ParamDefinition {
        key,
        label_key,
        kind: ParamKind::Color,
        default_color: default,
        default_number: 0.0,
        default_select: "",
        min: 0.0,
        max: 0.0,
        step: 0.0,
        group,
        options: &[],
    }
}

const fn number_param(
    key: &'static str,
    label_key: &'static str,
    default: f64,
    min: f64,
    max: f64,
    group: Option<&'static str>,
) -> ParamDefinition {
    ParamDefinition {
        key,
        label_key,
        kind: ParamKind::Number,
        default_color: "",
        default_number: default,
        default_select: "",
        min,
        max,
        step: 1.0,
        group,
        options: &[],
    }
}

const STROKE_ALIGN_PARAM: ParamDefinition = ParamDefinition {
    key: "strokeAlign",
    label_key: "graphics.param.strokeAlign",
    kind: ParamKind::Select,
    default_color: "",
    default_number: 0.0,
    default_select: "center",
    min: 0.0,
    max: 0.0,
    step: 0.0,
    group: Some("stroke"),
    options: &[
        ("inside", "graphics.strokeAlign.inside"),
        ("center", "graphics.strokeAlign.center"),
        ("outside", "graphics.strokeAlign.outside"),
    ],
};

const FILL_PARAM: ParamDefinition = color_param("fill", "graphics.param.fill", "#ffffff", None);
const STROKE_PARAM: ParamDefinition =
    color_param("stroke", "properties.color", "#000000", Some("stroke"));
const STROKE_WIDTH_PARAM: ParamDefinition = number_param(
    "strokeWidth",
    "common.width",
    0.0,
    0.0,
    64.0,
    Some("stroke"),
);
const CORNER_RADIUS_PARAM: ParamDefinition = number_param(
    "cornerRadius",
    "properties.cornerRadius",
    0.0,
    0.0,
    50.0,
    None,
);

pub struct GraphicDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub name_key: &'static str,
    pub keywords: &'static [&'static str],
    pub params: &'static [ParamDefinition],
}

const RECTANGLE_PARAMS: &[ParamDefinition] = &[
    FILL_PARAM,
    STROKE_PARAM,
    STROKE_WIDTH_PARAM,
    STROKE_ALIGN_PARAM,
    CORNER_RADIUS_PARAM,
];

const ELLIPSE_PARAMS: &[ParamDefinition] = &[
    FILL_PARAM,
    STROKE_PARAM,
    STROKE_WIDTH_PARAM,
    STROKE_ALIGN_PARAM,
];

const POLYGON_PARAMS: &[ParamDefinition] = &[
    FILL_PARAM,
    STROKE_PARAM,
    STROKE_WIDTH_PARAM,
    STROKE_ALIGN_PARAM,
    number_param("sides", "graphics.param.sides", 5.0, 3.0, 12.0, None),
    CORNER_RADIUS_PARAM,
];

const STAR_PARAMS: &[ParamDefinition] = &[
    FILL_PARAM,
    STROKE_PARAM,
    STROKE_WIDTH_PARAM,
    STROKE_ALIGN_PARAM,
    number_param("points", "graphics.param.points", 5.0, 3.0, 12.0, None),
    number_param("depth", "graphics.param.depth", 45.0, 1.0, 99.0, None),
];

pub const DEFINITIONS: &[GraphicDefinition] = &[
    GraphicDefinition {
        id: "rectangle",
        name: "Rectangle",
        name_key: "graphics.rectangle.name",
        keywords: &["rectangle", "square", "box"],
        params: RECTANGLE_PARAMS,
    },
    GraphicDefinition {
        id: "ellipse",
        name: "Ellipse",
        name_key: "graphics.ellipse.name",
        keywords: &["ellipse", "circle", "oval"],
        params: ELLIPSE_PARAMS,
    },
    GraphicDefinition {
        id: "polygon",
        name: "Polygon",
        name_key: "graphics.polygon.name",
        keywords: &["polygon", "triangle", "pentagon", "hexagon", "diamond"],
        params: POLYGON_PARAMS,
    },
    GraphicDefinition {
        id: "star",
        name: "Star",
        name_key: "graphics.star.name",
        keywords: &["star", "sparkle", "burst"],
        params: STAR_PARAMS,
    },
];

/// One named parameter of a shape preset, as `(name, value)`.
pub type ShapeParameter = (&'static str, f64);

/// A shape name from an older project file, the current shape it maps onto, and the
/// parameters that reproduce it: `(legacy name, shape, parameters)`.
pub type LegacyShapePreset = (&'static str, &'static str, &'static [ShapeParameter]);

pub const LEGACY_SHAPE_PRESETS: &[LegacyShapePreset] = &[
    ("square", "rectangle", &[]),
    ("circle", "ellipse", &[]),
    ("triangle", "polygon", &[("sides", 3.0)]),
    ("hexagon", "polygon", &[("sides", 6.0)]),
    ("diamond", "polygon", &[("sides", 4.0)]),
    ("star", "star", &[]),
];

pub fn definition(definition_id: &str) -> Option<&'static GraphicDefinition> {
    DEFINITIONS.iter().find(|entry| entry.id == definition_id)
}

pub fn default_params(definition_id: &str) -> ParamValues {
    let mut values = ParamValues::new();
    let Some(definition) = definition(definition_id) else {
        return values;
    };
    for param in definition.params {
        let value = match param.kind {
            ParamKind::Color => Value::String(param.default_color.to_owned()),
            ParamKind::Number => Value::from(param.default_number),
            ParamKind::Select => Value::String(param.default_select.to_owned()),
        };
        values.insert(param.key.to_owned(), value);
    }
    values
}

pub fn resolve_params(definition_id: &str, overrides: &ParamValues) -> ParamValues {
    let mut values = default_params(definition_id);
    for (key, value) in overrides {
        values.insert(key.clone(), value.clone());
    }
    values
}

fn number(params: &ParamValues, key: &str, fallback: f64) -> f64 {
    params
        .get(key)
        .and_then(|value| match value {
            Value::Number(number) => number.as_f64(),
            Value::String(text) => text.parse().ok(),
            _ => None,
        })
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
}

fn text<'a>(params: &'a ParamValues, key: &str, fallback: &'a str) -> &'a str {
    params
        .get(key)
        .and_then(|value| value.as_str())
        .unwrap_or(fallback)
}

pub fn parse_hex_color(raw: &str) -> Option<Color> {
    let hex = raw.trim().strip_prefix('#')?;
    let digits: Vec<u8> = hex
        .chars()
        .map(|character| character.to_digit(16).map(|value| value as u8))
        .collect::<Option<_>>()?;
    let (red, green, blue, alpha) = match digits.len() {
        3 => (digits[0] * 17, digits[1] * 17, digits[2] * 17, 255),
        4 => (
            digits[0] * 17,
            digits[1] * 17,
            digits[2] * 17,
            digits[3] * 17,
        ),
        6 => (
            digits[0] * 16 + digits[1],
            digits[2] * 16 + digits[3],
            digits[4] * 16 + digits[5],
            255,
        ),
        8 => (
            digits[0] * 16 + digits[1],
            digits[2] * 16 + digits[3],
            digits[4] * 16 + digits[5],
            digits[6] * 16 + digits[7],
        ),
        _ => return None,
    };
    Some(Color::from_rgba8(red, green, blue, alpha))
}

fn solid(color: Color) -> Paint<'static> {
    Paint {
        shader: Shader::SolidColor(color),
        anti_alias: true,
        ..Paint::default()
    }
}

struct ShapeStroke {
    color: Color,
    width: f32,
    align: String,
}

fn shape_stroke(params: &ParamValues) -> Option<ShapeStroke> {
    let width = number(params, "strokeWidth", 0.0).max(0.0) as f32;
    if width <= 0.0 {
        return None;
    }
    let color = parse_hex_color(text(params, "stroke", "#000000"))?;
    Some(ShapeStroke {
        color,
        width,
        align: text(params, "strokeAlign", "center").to_owned(),
    })
}

fn stroke_inset(params: &ParamValues) -> f32 {
    match shape_stroke(params) {
        Some(stroke) if stroke.align == "center" => stroke.width / 2.0,
        _ => 0.0,
    }
}

fn rounded_rect_path(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    radius: f32,
) -> Option<tiny_skia::Path> {
    let radius = radius.max(0.0).min(width / 2.0).min(height / 2.0);
    let mut builder = PathBuilder::new();
    if radius <= 0.0 {
        builder.push_rect(tiny_skia::Rect::from_xywh(x, y, width, height)?);
        return builder.finish();
    }
    let kappa = radius * 0.552_284_8;
    let (right, bottom) = (x + width, y + height);
    builder.move_to(x + radius, y);
    builder.line_to(right - radius, y);
    builder.cubic_to(
        right - radius + kappa,
        y,
        right,
        y + radius - kappa,
        right,
        y + radius,
    );
    builder.line_to(right, bottom - radius);
    builder.cubic_to(
        right,
        bottom - radius + kappa,
        right - radius + kappa,
        bottom,
        right - radius,
        bottom,
    );
    builder.line_to(x + radius, bottom);
    builder.cubic_to(
        x + radius - kappa,
        bottom,
        x,
        bottom - radius + kappa,
        x,
        bottom - radius,
    );
    builder.line_to(x, y + radius);
    builder.cubic_to(x, y + radius - kappa, x + radius - kappa, y, x + radius, y);
    builder.close();
    builder.finish()
}

fn ellipse_path(
    center_x: f32,
    center_y: f32,
    radius_x: f32,
    radius_y: f32,
) -> Option<tiny_skia::Path> {
    let kappa_x = radius_x * 0.552_284_8;
    let kappa_y = radius_y * 0.552_284_8;
    let mut builder = PathBuilder::new();
    builder.move_to(center_x + radius_x, center_y);
    builder.cubic_to(
        center_x + radius_x,
        center_y + kappa_y,
        center_x + kappa_x,
        center_y + radius_y,
        center_x,
        center_y + radius_y,
    );
    builder.cubic_to(
        center_x - kappa_x,
        center_y + radius_y,
        center_x - radius_x,
        center_y + kappa_y,
        center_x - radius_x,
        center_y,
    );
    builder.cubic_to(
        center_x - radius_x,
        center_y - kappa_y,
        center_x - kappa_x,
        center_y - radius_y,
        center_x,
        center_y - radius_y,
    );
    builder.cubic_to(
        center_x + kappa_x,
        center_y - radius_y,
        center_x + radius_x,
        center_y - kappa_y,
        center_x + radius_x,
        center_y,
    );
    builder.close();
    builder.finish()
}

fn polygon_vertices(center_x: f32, center_y: f32, radius: f32, sides: usize) -> Vec<(f32, f32)> {
    (0..sides)
        .map(|index| {
            let angle = -std::f32::consts::FRAC_PI_2
                + (index as f32) * std::f32::consts::TAU / (sides as f32);
            (
                center_x + angle.cos() * radius,
                center_y + angle.sin() * radius,
            )
        })
        .collect()
}

fn normalize(x: f32, y: f32) -> (f32, f32) {
    let length = x.hypot(y);
    if length <= f32::EPSILON {
        return (0.0, 0.0);
    }
    (x / length, y / length)
}

fn rounded_polygon_path(vertices: &[(f32, f32)], radius: f32) -> Option<tiny_skia::Path> {
    if vertices.len() < 3 {
        return None;
    }
    let mut builder = PathBuilder::new();
    if radius <= 0.0 {
        builder.move_to(vertices[0].0, vertices[0].1);
        for vertex in &vertices[1..] {
            builder.line_to(vertex.0, vertex.1);
        }
        builder.close();
        return builder.finish();
    }

    let count = vertices.len();
    for index in 0..count {
        let previous = vertices[(index + count - 1) % count];
        let current = vertices[index];
        let next = vertices[(index + 1) % count];
        let to_previous = normalize(previous.0 - current.0, previous.1 - current.1);
        let to_next = normalize(next.0 - current.0, next.1 - current.1);
        let dot = (to_previous.0 * to_next.0 + to_previous.1 * to_next.1).clamp(-1.0, 1.0);
        let angle = dot.acos();
        let max_offset = (previous.0 - current.0)
            .hypot(previous.1 - current.1)
            .min((next.0 - current.0).hypot(next.1 - current.1))
            / 2.0;
        let tangent = (angle / 2.0).tan();
        let offset = if tangent.abs() <= f32::EPSILON {
            max_offset
        } else {
            (radius / tangent).min(max_offset)
        };
        let start = (
            current.0 + to_previous.0 * offset,
            current.1 + to_previous.1 * offset,
        );
        let end = (
            current.0 + to_next.0 * offset,
            current.1 + to_next.1 * offset,
        );

        if index == 0 {
            builder.move_to(start.0, start.1);
        } else {
            builder.line_to(start.0, start.1);
        }
        builder.quad_to(current.0, current.1, end.0, end.1);
    }
    builder.close();
    builder.finish()
}

fn star_path(
    center_x: f32,
    center_y: f32,
    outer_radius: f32,
    inner_radius: f32,
    points: usize,
) -> Option<tiny_skia::Path> {
    let mut builder = PathBuilder::new();
    for index in 0..points * 2 {
        let radius = if index % 2 == 0 {
            outer_radius
        } else {
            inner_radius
        };
        let angle =
            -std::f32::consts::FRAC_PI_2 + (index as f32) * std::f32::consts::PI / (points as f32);
        let x = center_x + angle.cos() * radius;
        let y = center_y + angle.sin() * radius;
        if index == 0 {
            builder.move_to(x, y);
        } else {
            builder.line_to(x, y);
        }
    }
    builder.close();
    builder.finish()
}

fn shape_path(
    definition_id: &str,
    params: &ParamValues,
    width: f32,
    height: f32,
) -> Option<tiny_skia::Path> {
    let inset = stroke_inset(params);
    match definition_id {
        "rectangle" => {
            let draw_width = (width - inset * 2.0).max(1.0);
            let draw_height = (height - inset * 2.0).max(1.0);
            let percent = (number(params, "cornerRadius", 0.0).max(0.0) as f32).min(50.0);
            let radius = (draw_width.min(draw_height) / 2.0) * percent / 50.0;
            rounded_rect_path(inset, inset, draw_width, draw_height, radius)
        }
        "ellipse" => ellipse_path(
            width / 2.0,
            height / 2.0,
            (width / 2.0 - inset).max(1.0),
            (height / 2.0 - inset).max(1.0),
        ),
        "polygon" => {
            let sides = (number(params, "sides", 5.0).round() as i64).clamp(3, 12) as usize;
            let radius = (width.min(height) / 2.0 - inset).max(1.0);
            let max_corner = radius * (std::f32::consts::PI / sides as f32).sin();
            let percent = (number(params, "cornerRadius", 0.0).max(0.0) as f32).min(50.0);
            let vertices = polygon_vertices(width / 2.0, height / 2.0, radius, sides);
            rounded_polygon_path(&vertices, max_corner * percent / 50.0)
        }
        "star" => {
            let points = (number(params, "points", 5.0).round() as i64).clamp(3, 12) as usize;
            let depth = (number(params, "depth", 45.0).clamp(1.0, 99.0) as f32) / 100.0;
            let outer = (width.min(height) / 2.0 - inset).max(1.0);
            star_path(width / 2.0, height / 2.0, outer, outer * depth, points)
        }
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

fn unpremultiply(pixmap: &Pixmap) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(pixmap.data().len());
    for pixel in pixmap.pixels() {
        let demuxed = pixel.demultiply();
        rgba.extend_from_slice(&[
            demuxed.red(),
            demuxed.green(),
            demuxed.blue(),
            demuxed.alpha(),
        ]);
    }
    rgba
}

pub fn render_graphic(
    definition_id: &str,
    params: &ParamValues,
    width: u32,
    height: u32,
) -> Option<Raster> {
    let width = width.max(1);
    let height = height.max(1);
    let resolved = resolve_params(definition_id, params);
    let path = shape_path(definition_id, &resolved, width as f32, height as f32)?;
    let mut pixmap = Pixmap::new(width, height)?;

    let fill = parse_hex_color(text(&resolved, "fill", "#ffffff")).unwrap_or(Color::WHITE);
    pixmap.fill_path(
        &path,
        &solid(fill),
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    if let Some(stroke) = shape_stroke(&resolved) {
        let paint = solid(stroke.color);
        match stroke.align.as_str() {
            "inside" => {
                let mut mask = Mask::new(width, height)?;
                mask.fill_path(&path, FillRule::Winding, true, Transform::identity());
                pixmap.stroke_path(
                    &path,
                    &paint,
                    &Stroke {
                        width: stroke.width * 2.0,
                        ..Stroke::default()
                    },
                    Transform::identity(),
                    Some(&mask),
                );
            }
            "outside" => {
                let mut layer = Pixmap::new(width, height)?;
                layer.stroke_path(
                    &path,
                    &paint,
                    &Stroke {
                        width: stroke.width * 2.0,
                        ..Stroke::default()
                    },
                    Transform::identity(),
                    None,
                );
                layer.fill_path(
                    &path,
                    &Paint {
                        blend_mode: BlendMode::DestinationOut,
                        ..solid(Color::BLACK)
                    },
                    FillRule::Winding,
                    Transform::identity(),
                    None,
                );
                pixmap.draw_pixmap(
                    0,
                    0,
                    layer.as_ref(),
                    &tiny_skia::PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
            }
            _ => {
                pixmap.stroke_path(
                    &path,
                    &paint,
                    &Stroke {
                        width: stroke.width,
                        ..Stroke::default()
                    },
                    Transform::identity(),
                    None,
                );
            }
        }
    }

    Some(Raster {
        width,
        height,
        rgba: unpremultiply(&pixmap),
    })
}

pub(crate) fn raster_from_pixmap(pixmap: &Pixmap) -> Raster {
    Raster {
        width: pixmap.width(),
        height: pixmap.height(),
        rgba: unpremultiply(pixmap),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(raster: &Raster, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * raster.width + x) * 4) as usize;
        [
            raster.rgba[offset],
            raster.rgba[offset + 1],
            raster.rgba[offset + 2],
            raster.rgba[offset + 3],
        ]
    }

    #[test]
    fn hex_colors_parse_in_every_length() {
        assert_eq!(
            parse_hex_color("#f00").unwrap(),
            Color::from_rgba8(255, 0, 0, 255)
        );
        assert_eq!(
            parse_hex_color("#00ff00").unwrap(),
            Color::from_rgba8(0, 255, 0, 255)
        );
        assert_eq!(
            parse_hex_color("#0000ff80").unwrap(),
            Color::from_rgba8(0, 0, 255, 128)
        );
        assert!(parse_hex_color("nope").is_none());
    }

    #[test]
    fn rectangle_fills_the_whole_canvas() {
        let mut params = ParamValues::new();
        params.insert("fill".into(), Value::String("#ff0000".into()));
        let raster = render_graphic("rectangle", &params, 64, 64).expect("rectangle");
        assert_eq!(raster.width, 64);
        assert_eq!(pixel(&raster, 32, 32), [255, 0, 0, 255]);
        assert_eq!(pixel(&raster, 1, 1), [255, 0, 0, 255]);
    }

    #[test]
    fn ellipse_is_opaque_at_the_centre_and_clear_in_the_corner() {
        let raster = render_graphic("ellipse", &ParamValues::new(), 64, 64).expect("ellipse");
        assert_eq!(pixel(&raster, 32, 32)[3], 255);
        assert_eq!(pixel(&raster, 0, 0)[3], 0);
    }

    #[test]
    fn triangle_polygon_leaves_the_bottom_corners_clear() {
        let mut params = ParamValues::new();
        params.insert("sides".into(), Value::from(3));
        let raster = render_graphic("polygon", &params, 64, 64).expect("polygon");
        assert_eq!(pixel(&raster, 32, 40)[3], 255);
        assert_eq!(pixel(&raster, 1, 62)[3], 0);
    }

    #[test]
    fn star_notches_between_points() {
        let raster = render_graphic("star", &ParamValues::new(), 128, 128).expect("star");
        assert_eq!(pixel(&raster, 64, 64)[3], 255);
        assert_eq!(pixel(&raster, 2, 2)[3], 0);
    }

    #[test]
    fn centred_stroke_paints_the_border_colour() {
        let mut params = ParamValues::new();
        params.insert("fill".into(), Value::String("#ffffff".into()));
        params.insert("stroke".into(), Value::String("#ff0000".into()));
        params.insert("strokeWidth".into(), Value::from(8));
        let raster = render_graphic("rectangle", &params, 64, 64).expect("rectangle");
        assert_eq!(pixel(&raster, 32, 2), [255, 0, 0, 255]);
        assert_eq!(pixel(&raster, 32, 32), [255, 255, 255, 255]);
    }

    #[test]
    fn defaults_cover_every_declared_param() {
        for definition in DEFINITIONS {
            let defaults = default_params(definition.id);
            for param in definition.params {
                assert!(
                    defaults.contains_key(param.key),
                    "{}/{}",
                    definition.id,
                    param.key
                );
            }
        }
    }

    #[test]
    fn unknown_definition_renders_nothing() {
        assert!(render_graphic("not-a-shape", &ParamValues::new(), 32, 32).is_none());
    }
}
