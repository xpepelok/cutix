fn srgb_to_linear(value: f64) -> f64 {
    if value <= 0.04045 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn named(color: &str) -> Option<(f64, f64, f64, f64)> {
    let rgb = match color {
        "black" => (0, 0, 0),
        "white" => (255, 255, 255),
        "red" => (255, 0, 0),
        "green" => (0, 128, 0),
        "lime" => (0, 255, 0),
        "blue" => (0, 0, 255),
        "yellow" => (255, 255, 0),
        "cyan" | "aqua" => (0, 255, 255),
        "magenta" | "fuchsia" => (255, 0, 255),
        "gray" | "grey" => (128, 128, 128),
        "silver" => (192, 192, 192),
        "orange" => (255, 165, 0),
        "purple" => (128, 0, 128),
        "transparent" => return Some((0.0, 0.0, 0.0, 0.0)),
        _ => return None,
    };
    Some((
        f64::from(rgb.0) / 255.0,
        f64::from(rgb.1) / 255.0,
        f64::from(rgb.2) / 255.0,
        1.0,
    ))
}

fn parse_hex(text: &str) -> Option<(f64, f64, f64, f64)> {
    let digits = text.strip_prefix('#')?;
    let component = |value: u8| f64::from(value) / 255.0;

    let parse_pair = |slice: &str| u8::from_str_radix(slice, 16).ok();
    let expand = |character: char| {
        let digit = character.to_digit(16)? as u8;
        Some(digit * 16 + digit)
    };

    match digits.len() {
        3 | 4 => {
            let characters: Vec<char> = digits.chars().collect();
            let red = expand(characters[0])?;
            let green = expand(characters[1])?;
            let blue = expand(characters[2])?;
            let alpha = match characters.get(3) {
                Some(character) => expand(*character)?,
                None => 255,
            };
            Some((
                component(red),
                component(green),
                component(blue),
                component(alpha),
            ))
        }
        6 | 8 => {
            let red = parse_pair(&digits[0..2])?;
            let green = parse_pair(&digits[2..4])?;
            let blue = parse_pair(&digits[4..6])?;
            let alpha = if digits.len() == 8 {
                parse_pair(&digits[6..8])?
            } else {
                255
            };
            Some((
                component(red),
                component(green),
                component(blue),
                component(alpha),
            ))
        }
        _ => None,
    }
}

fn parse_functional(text: &str) -> Option<(f64, f64, f64, f64)> {
    let body = text
        .strip_prefix("rgba(")
        .or_else(|| text.strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let parts: Vec<&str> = body
        .split([',', '/', ' '])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.len() < 3 {
        return None;
    }

    let channel = |part: &str| -> Option<f64> {
        if let Some(percent) = part.strip_suffix('%') {
            return percent.parse::<f64>().ok().map(|value| value / 100.0);
        }
        part.parse::<f64>().ok().map(|value| value / 255.0)
    };

    let red = channel(parts[0])?;
    let green = channel(parts[1])?;
    let blue = channel(parts[2])?;
    let alpha = match parts.get(3) {
        Some(part) => {
            if let Some(percent) = part.strip_suffix('%') {
                percent.parse::<f64>().ok()? / 100.0
            } else {
                part.parse::<f64>().ok()?
            }
        }
        None => 1.0,
    };

    Some((red, green, blue, alpha))
}

pub fn parse_to_srgb_rgba(color: &str) -> Option<[f64; 4]> {
    let text = color.trim().to_lowercase();
    let (red, green, blue, alpha) = parse_hex(&text)
        .or_else(|| parse_functional(&text))
        .or_else(|| named(&text))?;
    Some([red, green, blue, alpha.clamp(0.0, 1.0)])
}

pub fn srgb_to_linear_channel(value: f64) -> f64 {
    srgb_to_linear(value)
}

pub fn linear_to_srgb_channel(value: f64) -> f64 {
    let clamped = value.clamp(0.0, 1.0);
    if clamped <= 0.0031308 {
        clamped * 12.92
    } else {
        1.055 * clamped.powf(1.0 / 2.4) - 0.055
    }
}

pub fn format_srgb_hex(rgba: [f64; 4]) -> String {
    let channel = |value: f64| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(rgba[0]),
        channel(rgba[1]),
        channel(rgba[2])
    )
}

pub fn parse_to_linear_rgba(color: &str) -> Option<[f64; 4]> {
    let text = color.trim().to_lowercase();
    let (red, green, blue, alpha) = parse_hex(&text)
        .or_else(|| parse_functional(&text))
        .or_else(|| named(&text))?;

    Some([
        srgb_to_linear(red),
        srgb_to_linear(green),
        srgb_to_linear(blue),
        alpha.clamp(0.0, 1.0),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_and_alpha() {
        let white = parse_to_linear_rgba("#ffffff").expect("white parses");
        assert!((white[0] - 1.0).abs() < 1e-9);
        assert!((white[3] - 1.0).abs() < 1e-9);

        let half = parse_to_linear_rgba("#80808080").expect("grey parses");
        assert!((half[0] - srgb_to_linear(128.0 / 255.0)).abs() < 1e-9);
        assert!((half[3] - 128.0 / 255.0).abs() < 1e-9);
    }

    #[test]
    fn parses_rgb_function_and_names() {
        let red = parse_to_linear_rgba("rgb(255, 0, 0)").expect("rgb parses");
        assert!((red[0] - 1.0).abs() < 1e-9);
        assert!(red[1].abs() < 1e-9);

        let transparent = parse_to_linear_rgba("transparent").expect("named parses");
        assert!(transparent[3].abs() < 1e-9);
    }
}
