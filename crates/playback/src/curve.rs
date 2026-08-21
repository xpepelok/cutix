use serde_json::Value;

pub const CURVE_TABLE_SIZE: usize = 256;

pub const CURVE_CHANNELS: [&str; 4] = ["master", "r", "g", "b"];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurvePoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CurveSet {
    pub master: Vec<CurvePoint>,
    pub r: Vec<CurvePoint>,
    pub g: Vec<CurvePoint>,
    pub b: Vec<CurvePoint>,
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn identity_points() -> Vec<CurvePoint> {
    vec![CurvePoint { x: 0.0, y: 0.0 }, CurvePoint { x: 1.0, y: 1.0 }]
}

pub fn identity_curve_set() -> CurveSet {
    CurveSet {
        master: identity_points(),
        r: identity_points(),
        g: identity_points(),
        b: identity_points(),
    }
}

fn sanitize_points(value: Option<&Value>) -> Vec<CurvePoint> {
    let Some(Value::Array(entries)) = value else {
        return identity_points();
    };
    let mut points: Vec<CurvePoint> = entries
        .iter()
        .filter_map(|entry| {
            let x = entry.get("x")?.as_f64()?;
            let y = entry.get("y")?.as_f64()?;
            if !x.is_finite() || !y.is_finite() {
                return None;
            }
            Some(CurvePoint {
                x: clamp01(x),
                y: clamp01(y),
            })
        })
        .collect();
    points.sort_by(|left, right| left.x.total_cmp(&right.x));
    if points.len() < 2 {
        return identity_points();
    }
    points
}

pub fn parse_curve_set(value: Option<&Value>) -> CurveSet {
    let owned;
    let raw = match value {
        Some(Value::String(text)) => {
            if text.trim().is_empty() {
                return identity_curve_set();
            }
            match serde_json::from_str::<Value>(text) {
                Ok(parsed) => {
                    owned = parsed;
                    Some(&owned)
                }
                Err(_) => return identity_curve_set(),
            }
        }
        other => other,
    };
    let Some(record) = raw else {
        return identity_curve_set();
    };
    CurveSet {
        master: sanitize_points(record.get("master")),
        r: sanitize_points(record.get("r")),
        g: sanitize_points(record.get("g")),
        b: sanitize_points(record.get("b")),
    }
}

pub fn is_identity_curve(points: &[CurvePoint]) -> bool {
    points.len() == 2
        && points[0].x == 0.0
        && points[0].y == 0.0
        && points[1].x == 1.0
        && points[1].y == 1.0
}

pub fn is_identity_curve_set(curves: &CurveSet) -> bool {
    is_identity_curve(&curves.master)
        && is_identity_curve(&curves.r)
        && is_identity_curve(&curves.g)
        && is_identity_curve(&curves.b)
}

pub fn sample_curve(points: &[CurvePoint], x: f64) -> f64 {
    let position = clamp01(x);
    let count = points.len();
    if count == 0 {
        return position;
    }
    if count == 1 {
        return clamp01(points[0].y);
    }
    if position <= points[0].x {
        return clamp01(points[0].y);
    }
    if position >= points[count - 1].x {
        return clamp01(points[count - 1].y);
    }

    let mut slopes = Vec::with_capacity(count - 1);
    for index in 0..count - 1 {
        let dx = points[index + 1].x - points[index].x;
        slopes.push(if dx <= 0.0 {
            0.0
        } else {
            (points[index + 1].y - points[index].y) / dx
        });
    }

    let mut tangents = vec![0.0f64; count];
    tangents[0] = slopes[0];
    tangents[count - 1] = slopes[count - 2];
    for index in 1..count - 1 {
        if slopes[index - 1] * slopes[index] <= 0.0 {
            tangents[index] = 0.0;
        } else {
            tangents[index] = (slopes[index - 1] + slopes[index]) / 2.0;
        }
    }
    for index in 0..count - 1 {
        if slopes[index] == 0.0 {
            tangents[index] = 0.0;
            tangents[index + 1] = 0.0;
            continue;
        }
        let alpha = tangents[index] / slopes[index];
        let beta = tangents[index + 1] / slopes[index];
        let magnitude = alpha.hypot(beta);
        if magnitude > 3.0 {
            let scale = 3.0 / magnitude;
            tangents[index] = scale * alpha * slopes[index];
            tangents[index + 1] = scale * beta * slopes[index];
        }
    }

    let mut index = 0;
    for candidate in 0..count - 1 {
        if position >= points[candidate].x && position <= points[candidate + 1].x {
            index = candidate;
            break;
        }
    }
    let dx = points[index + 1].x - points[index].x;
    if dx <= 0.0 {
        return clamp01(points[index].y);
    }
    let t = (position - points[index].x) / dx;
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
    let h10 = t3 - 2.0 * t2 + t;
    let h01 = -2.0 * t3 + 3.0 * t2;
    let h11 = t3 - t2;
    clamp01(
        h00 * points[index].y
            + h10 * dx * tangents[index]
            + h01 * points[index + 1].y
            + h11 * dx * tangents[index + 1],
    )
}

pub fn bake_curve_table(curves: &CurveSet) -> Vec<f32> {
    let mut table = vec![0.0f32; CURVE_TABLE_SIZE * 4];
    for index in 0..CURVE_TABLE_SIZE {
        let input = index as f64 / (CURVE_TABLE_SIZE - 1) as f64;
        table[index * 4] = sample_curve(&curves.master, sample_curve(&curves.r, input)) as f32;
        table[index * 4 + 1] = sample_curve(&curves.master, sample_curve(&curves.g, input)) as f32;
        table[index * 4 + 2] = sample_curve(&curves.master, sample_curve(&curves.b, input)) as f32;
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_curve_bakes_to_a_ramp() {
        let table = bake_curve_table(&identity_curve_set());
        assert_eq!(table.len(), CURVE_TABLE_SIZE * 4);
        assert!((table[0] - 0.0).abs() < 1e-6);
        assert!((table[128 * 4] - 128.0 / 255.0).abs() < 1e-6);
        assert!((table[255 * 4] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn a_lifted_midpoint_raises_the_mid_tone_only_on_that_channel() {
        let mut curves = identity_curve_set();
        curves.r = vec![
            CurvePoint { x: 0.0, y: 0.0 },
            CurvePoint { x: 0.5, y: 0.75 },
            CurvePoint { x: 1.0, y: 1.0 },
        ];
        let table = bake_curve_table(&curves);
        let mid = 128 * 4;
        assert!(table[mid] > 0.70, "red mid should lift, got {}", table[mid]);
        assert!((table[mid + 1] - 128.0 / 255.0).abs() < 1e-6);
        assert!((table[mid + 2] - 128.0 / 255.0).abs() < 1e-6);
        assert!((table[0] - 0.0).abs() < 1e-6);
        assert!((table[255 * 4] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn parses_a_serialised_curve_set_from_a_string_param() {
        let raw = serde_json::json!(
            "{\"master\":[{\"x\":0,\"y\":0},{\"x\":1,\"y\":1}],\"r\":[{\"x\":0,\"y\":0.25},{\"x\":1,\"y\":1}],\"g\":[],\"b\":null}"
        );
        let curves = parse_curve_set(Some(&raw));
        assert_eq!(curves.r[0].y, 0.25);
        assert!(is_identity_curve(&curves.g));
        assert!(is_identity_curve(&curves.b));
        assert!(!is_identity_curve_set(&curves));
    }

    #[test]
    fn a_missing_param_is_the_identity() {
        assert!(is_identity_curve_set(&parse_curve_set(None)));
    }
}
