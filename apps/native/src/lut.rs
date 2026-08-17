use std::path::Path;

const MAX_3D_SIZE: usize = 64;
const MAX_1D_SIZE: usize = 65_536;

pub const CURVE_TABLE_SIZE: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CubeKind {
    OneD,
    ThreeD,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedCube {
    pub kind: CubeKind,
    pub size: usize,
    pub domain_min: [f32; 3],
    pub domain_max: [f32; 3],
    pub values: Vec<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoadedLut {
    pub name: String,
    pub size: usize,
    pub table: Vec<f32>,
}

fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn parse_triple(parts: &[&str], line: usize) -> Result<[f32; 3], String> {
    if parts.len() < 3 {
        return Err(format!("Expected three numbers on line {line}"));
    }
    let mut triple = [0.0f32; 3];
    for (index, part) in parts.iter().take(3).enumerate() {
        let parsed: f32 = part
            .parse()
            .map_err(|_| format!("Invalid number on line {line}"))?;
        if !parsed.is_finite() {
            return Err(format!("Invalid number on line {line}"));
        }
        triple[index] = parsed;
    }
    Ok(triple)
}

pub fn parse_cube(text: &str) -> Result<ParsedCube, String> {
    let mut kind: Option<CubeKind> = None;
    let mut size = 0usize;
    let mut domain_min = [0.0f32; 3];
    let mut domain_max = [1.0f32; 3];
    let mut values: Vec<f32> = Vec::new();

    for (index, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        let keyword = parts[0].to_ascii_uppercase();

        if keyword == "TITLE" {
            continue;
        }
        if keyword == "LUT_3D_SIZE" || keyword == "LUT_1D_SIZE" {
            let parsed: usize = parts
                .get(1)
                .and_then(|value| value.parse().ok())
                .filter(|value| *value >= 2)
                .ok_or_else(|| format!("Invalid LUT size on line {}", index + 1))?;
            let this_kind = if keyword == "LUT_3D_SIZE" {
                CubeKind::ThreeD
            } else {
                CubeKind::OneD
            };
            let limit = match this_kind {
                CubeKind::ThreeD => MAX_3D_SIZE,
                CubeKind::OneD => MAX_1D_SIZE,
            };
            if parsed > limit {
                return Err(format!("LUT size {parsed} exceeds {limit}"));
            }
            kind = Some(this_kind);
            size = parsed;
            continue;
        }
        if keyword == "DOMAIN_MIN" {
            domain_min = parse_triple(&parts[1..], index + 1)?;
            continue;
        }
        if keyword == "DOMAIN_MAX" {
            domain_max = parse_triple(&parts[1..], index + 1)?;
            continue;
        }
        if parts[0].starts_with(['-', '+', '.'])
            || parts[0].starts_with(|c: char| c.is_ascii_digit())
        {
            let triple = parse_triple(&parts, index + 1)?;
            values.extend_from_slice(&triple);
            continue;
        }
        return Err(format!("Unrecognised keyword '{}'", parts[0]));
    }

    let kind = kind.ok_or_else(|| "Missing LUT_1D_SIZE or LUT_3D_SIZE".to_string())?;
    if size == 0 {
        return Err("Missing LUT_1D_SIZE or LUT_3D_SIZE".to_string());
    }
    let entries = match kind {
        CubeKind::ThreeD => size * size * size,
        CubeKind::OneD => size,
    };
    if values.len() != entries * 3 {
        return Err(format!(
            "Expected {entries} entries, found {}",
            values.len() / 3
        ));
    }

    Ok(ParsedCube {
        kind,
        size,
        domain_min,
        domain_max,
        values,
    })
}

fn sample_1d(lut: &ParsedCube, channel: usize, position: f32) -> f32 {
    let scaled = clamp01(position) * (lut.size - 1) as f32;
    let low = scaled.floor() as usize;
    let high = (low + 1).min(lut.size - 1);
    let fraction = scaled - low as f32;
    let a = lut.values[low * 3 + channel];
    let b = lut.values[high * 3 + channel];
    a + (b - a) * fraction
}

pub fn bake_3d_table(lut: &ParsedCube) -> Vec<f32> {
    let entries = lut.size * lut.size * lut.size;
    let mut table = vec![0.0f32; entries * 4];
    for index in 0..entries {
        for channel in 0..3 {
            table[index * 4 + channel] = clamp01(lut.values[index * 3 + channel]);
        }
    }
    table
}

pub fn bake_1d_curve_table(lut: &ParsedCube) -> Vec<f32> {
    let mut table = vec![0.0f32; CURVE_TABLE_SIZE * 4];
    for i in 0..CURVE_TABLE_SIZE {
        let input = i as f32 / (CURVE_TABLE_SIZE - 1) as f32;
        for channel in 0..3 {
            let domain = lut.domain_max[channel] - lut.domain_min[channel];
            let normalized = if domain == 0.0 {
                0.0
            } else {
                (input - lut.domain_min[channel]) / domain
            };
            table[i * 4 + channel] = clamp01(sample_1d(lut, channel, normalized));
        }
    }
    table
}

pub fn load_cube(path: &Path) -> Result<LoadedLut, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let parsed = parse_cube(&text)?;
    let name = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("LUT")
        .to_string();

    match parsed.kind {
        CubeKind::ThreeD => Ok(LoadedLut {
            name,
            size: parsed.size,
            table: bake_3d_table(&parsed),
        }),
        CubeKind::OneD => {
            let curve = bake_1d_curve_table(&parsed);
            Ok(LoadedLut {
                name,
                size: CUBE_FROM_CURVE_SIZE,
                table: cube_from_curve_table(&curve),
            })
        }
    }
}

pub const CUBE_FROM_CURVE_SIZE: usize = 32;

fn sample_curve(curve: &[f32], channel: usize, position: f32) -> f32 {
    let scaled = clamp01(position) * (CURVE_TABLE_SIZE - 1) as f32;
    let low = scaled.floor() as usize;
    let high = (low + 1).min(CURVE_TABLE_SIZE - 1);
    let fraction = scaled - low as f32;
    let a = curve[low * 4 + channel];
    let b = curve[high * 4 + channel];
    a + (b - a) * fraction
}

pub fn cube_from_curve_table(curve: &[f32]) -> Vec<f32> {
    let size = CUBE_FROM_CURVE_SIZE;
    let last = (size - 1) as f32;
    let mut table = vec![0.0f32; size * size * size * 4];
    for b in 0..size {
        for g in 0..size {
            for r in 0..size {
                let index = r + g * size + b * size * size;
                table[index * 4] = sample_curve(curve, 0, r as f32 / last);
                table[index * 4 + 1] = sample_curve(curve, 1, g as f32 / last);
                table[index * 4 + 2] = sample_curve(curve, 2, b as f32 / last);
            }
        }
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDENTITY_2: &str = "TITLE \"id\"\n# comment\nLUT_3D_SIZE 2\n0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";

    #[test]
    fn a_three_d_cube_parses_into_an_rgba_table_in_shader_order() {
        let parsed = parse_cube(IDENTITY_2).unwrap();
        assert_eq!(parsed.kind, CubeKind::ThreeD);
        assert_eq!(parsed.size, 2);
        let table = bake_3d_table(&parsed);
        assert_eq!(table.len(), 2 * 2 * 2 * 4);

        assert_eq!(&table[4..8], &[1.0, 0.0, 0.0, 0.0]);

        assert_eq!(&table[28..32], &[1.0, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn a_wrong_entry_count_is_rejected() {
        let text = "LUT_3D_SIZE 2\n0 0 0\n1 1 1\n";
        assert!(parse_cube(text).unwrap_err().contains("Expected 8 entries"));
    }

    #[test]
    fn a_missing_size_is_rejected() {
        assert!(parse_cube("0 0 0\n").is_err());
        assert!(parse_cube("LUT_3D_SIZE 1\n").is_err());
        assert!(parse_cube(&format!("LUT_3D_SIZE {}\n", MAX_3D_SIZE + 1)).is_err());
    }

    #[test]
    fn an_unknown_keyword_is_rejected() {
        assert!(parse_cube("LUT_3D_SIZE 2\nWAT 1\n").is_err());
    }

    #[test]
    fn a_one_d_cube_bakes_a_two_five_six_entry_curve() {
        let text = "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n";
        let parsed = parse_cube(text).unwrap();
        assert_eq!(parsed.kind, CubeKind::OneD);
        let curve = bake_1d_curve_table(&parsed);
        assert_eq!(curve.len(), CURVE_TABLE_SIZE * 4);
        assert!((curve[0] - 0.0).abs() < 1e-6);
        assert!((curve[255 * 4] - 1.0).abs() < 1e-6);
        assert!((curve[128 * 4] - 128.0 / 255.0).abs() < 1e-3);
    }

    #[test]
    fn an_inverting_one_d_cube_expands_into_an_inverting_cube() {
        let parsed = parse_cube("LUT_1D_SIZE 2\n1 1 1\n0 0 0\n").unwrap();
        let curve = bake_1d_curve_table(&parsed);
        let cube = cube_from_curve_table(&curve);
        let size = CUBE_FROM_CURVE_SIZE;

        assert!((cube[0] - 1.0).abs() < 1e-3);

        let last = size * size * size - 1;
        assert!((cube[last * 4] - 0.0).abs() < 1e-3);
    }

    #[test]
    fn a_three_d_identity_cube_is_the_identity_at_its_corners() {
        let parsed = parse_cube(IDENTITY_2).unwrap();
        let table = bake_3d_table(&parsed);
        for (index, expected) in [
            (0usize, [0.0, 0.0, 0.0]),
            (1, [1.0, 0.0, 0.0]),
            (2, [0.0, 1.0, 0.0]),
            (4, [0.0, 0.0, 1.0]),
        ] {
            assert_eq!(&table[index * 4..index * 4 + 3], &expected);
        }
    }
}
