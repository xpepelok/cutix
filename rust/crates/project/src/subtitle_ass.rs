use std::collections::HashMap;

use crate::subtitles::{
    ParseSubtitleResult, SubtitleBackground, SubtitleCue, SubtitlePlacement,
    SubtitleStyleOverrides, SubtitleWarning,
};

const ASS_DEFAULT_PLAY_RES_X: f64 = 384.0;
const ASS_DEFAULT_PLAY_RES_Y: f64 = 288.0;

fn alignment(code: i64) -> (&'static str, &'static str) {
    match code {
        1 => ("left", "bottom"),
        2 => ("center", "bottom"),
        3 => ("right", "bottom"),
        4 => ("left", "middle"),
        5 => ("center", "middle"),
        6 => ("right", "middle"),
        7 => ("left", "top"),
        8 => ("center", "top"),
        9 => ("right", "top"),
        _ => ("center", "bottom"),
    }
}

fn is_style_section(name: &str) -> bool {
    name == "v4 styles" || name == "v4+ styles"
}

pub fn parse_float_prefix(raw: &str) -> Option<f64> {
    let text = raw.trim_start();
    let bytes = text.as_bytes();
    let mut end = 0usize;
    let mut seen_digit = false;
    let mut seen_dot = false;
    let mut seen_exponent = false;

    while end < bytes.len() {
        let byte = bytes[end];
        match byte {
            b'+' | b'-' => {
                let start_of_number = end == 0;
                let after_exponent =
                    end > 0 && matches!(bytes[end - 1], b'e' | b'E') && seen_exponent;
                if !start_of_number && !after_exponent {
                    break;
                }
            }
            b'0'..=b'9' => seen_digit = true,
            b'.' => {
                if seen_dot || seen_exponent {
                    break;
                }
                seen_dot = true;
            }
            b'e' | b'E' => {
                if seen_exponent || !seen_digit {
                    break;
                }
                seen_exponent = true;
            }
            _ => break,
        }
        end += 1;
    }

    if !seen_digit {
        return None;
    }

    let mut candidate = &text[..end];
    while !candidate.is_empty() && candidate.parse::<f64>().is_err() {
        candidate = &candidate[..candidate.len() - 1];
    }
    candidate
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

pub fn parse_ass_timestamp(raw: &str) -> Option<f64> {
    let (clock, fraction) = raw.split_once('.')?;
    if !matches!(fraction.len(), 1 | 2 | 3) || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parts: Vec<&str> = clock.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    if parts[0].is_empty() || !parts[0].bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    if parts[1..]
        .iter()
        .any(|part| part.len() != 2 || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    let hours: f64 = parts[0].parse().ok()?;
    let minutes: f64 = parts[1].parse().ok()?;
    let seconds: f64 = parts[2].parse().ok()?;
    let mut padded = fraction.to_owned();
    while padded.len() < 3 {
        padded.push('0');
    }
    let milliseconds: f64 = padded.parse().ok()?;
    Some(hours * 3600.0 + minutes * 60.0 + seconds + milliseconds / 1000.0)
}

pub fn split_ass_fields(value: &str, expected: usize) -> Vec<String> {
    if expected <= 1 {
        return vec![value.to_owned()];
    }
    let mut result: Vec<String> = Vec::with_capacity(expected);
    let mut current = String::new();
    for character in value.chars() {
        if character == ',' && result.len() < expected - 1 {
            result.push(current.trim().to_owned());
            current.clear();
            continue;
        }
        current.push(character);
    }
    result.push(current.trim().to_owned());
    result
}

fn parse_format(line: &str) -> Vec<String> {
    match line.split_once(':') {
        Some((_, rest)) => rest
            .split(',')
            .map(|field| field.trim().to_lowercase())
            .collect(),
        None => Vec::new(),
    }
}

fn record_from(format: &[String], line: &str) -> Option<HashMap<String, String>> {
    let (_, rest) = line.split_once(':')?;
    let values = split_ass_fields(rest, format.len());
    if values.len() != format.len() {
        return None;
    }
    Some(
        format
            .iter()
            .cloned()
            .zip(values.into_iter())
            .collect::<HashMap<_, _>>(),
    )
}

fn field<'a>(record: &'a HashMap<String, String>, key: &str) -> Option<&'a str> {
    record.get(key).map(String::as_str)
}

fn number(record: &HashMap<String, String>, key: &str) -> Option<f64> {
    field(record, key).and_then(parse_float_prefix)
}

fn parse_ass_boolean(raw: Option<&str>) -> Option<bool> {
    let text = raw?.trim();
    if text.is_empty() {
        return None;
    }
    let value = parse_float_prefix(text)?;
    if !value.is_finite() {
        return None;
    }
    Some(value as i64 != 0)
}

pub struct AssColor {
    pub css_color: String,
    pub alpha: f64,
}

pub fn parse_ass_color(raw: &str) -> Option<AssColor> {
    let trimmed = raw.trim();
    let stripped = if let Some(rest) = trimmed
        .strip_prefix("&H")
        .or_else(|| trimmed.strip_prefix("&h"))
    {
        rest
    } else if let Some(rest) = trimmed
        .strip_prefix('H')
        .or_else(|| trimmed.strip_prefix('h'))
    {
        rest
    } else {
        trimmed
    };
    let mut normalized = String::new();
    for _ in stripped.len()..8 {
        normalized.push('0');
    }
    normalized.push_str(stripped);
    if normalized.len() != 8 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }

    let alpha_hex = &normalized[0..2];
    let blue_hex = &normalized[2..4];
    let green_hex = &normalized[4..6];
    let red_hex = &normalized[6..8];
    let alpha = 1.0 - i64::from_str_radix(alpha_hex, 16).ok()? as f64 / 255.0;

    if alpha >= 1.0 {
        return Some(AssColor {
            css_color: format!("#{red_hex}{green_hex}{blue_hex}").to_lowercase(),
            alpha,
        });
    }
    let red = i64::from_str_radix(red_hex, 16).ok()?;
    let green = i64::from_str_radix(green_hex, 16).ok()?;
    let blue = i64::from_str_radix(blue_hex, 16).ok()?;
    Some(AssColor {
        css_color: format!(
            "rgba({red}, {green}, {blue}, {})",
            (alpha * 1000.0).round() / 1000.0
        ),
        alpha,
    })
}

pub fn strip_ass_text(input: &str) -> (String, bool) {
    let mut without_tags = String::with_capacity(input.len());
    let mut had_tags = false;
    let mut rest = input;
    while let Some(open) = rest.find('{') {
        match rest[open..].find('}') {
            Some(offset) => {
                had_tags = true;
                without_tags.push_str(&rest[..open]);
                rest = &rest[open + offset + 1..];
            }
            None => break,
        }
    }
    without_tags.push_str(rest);

    let mut text = String::with_capacity(without_tags.len());
    let mut characters = without_tags.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\\' {
            text.push(character);
            continue;
        }
        match characters.peek().copied() {
            Some('N') | Some('n') => {
                characters.next();
                text.push('\n');
            }
            Some('h') => {
                characters.next();
                text.push(' ');
            }
            _ => text.push(character),
        }
    }

    (text.trim().to_owned(), had_tags)
}

struct ScriptInfo {
    play_res_x: f64,
    play_res_y: f64,
}

fn style_overrides(
    style: &HashMap<String, String>,
    info: &ScriptInfo,
) -> (SubtitleStyleOverrides, bool) {
    let mut overrides = SubtitleStyleOverrides::default();

    if let Some(name) = field(style, "fontname")
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        overrides.font_family = Some(name.to_owned());
    }
    if let Some(size) = number(style, "fontsize") {
        let ratio = (size / info.play_res_y * 1000.0).round() / 1000.0;
        if ratio.is_finite() && ratio > 0.0 {
            overrides.font_size_ratio_of_play_height = Some(ratio);
        }
    }
    if let Some(color) = field(style, "primarycolour").and_then(parse_ass_color) {
        overrides.color = Some(color.css_color);
    }

    let bold = parse_ass_boolean(field(style, "bold"));
    let italic = parse_ass_boolean(field(style, "italic"));
    let underline = parse_ass_boolean(field(style, "underline"));
    let strike_out = parse_ass_boolean(field(style, "strikeout"));

    if let Some(bold) = bold {
        overrides.font_weight = Some(String::from(if bold { "bold" } else { "normal" }));
    }
    if let Some(italic) = italic {
        overrides.font_style = Some(String::from(if italic { "italic" } else { "normal" }));
    }
    if underline == Some(true) {
        overrides.text_decoration = Some(String::from("underline"));
    } else if strike_out == Some(true) {
        overrides.text_decoration = Some(String::from("line-through"));
    }
    if let Some(spacing) = number(style, "spacing") {
        overrides.letter_spacing = Some(spacing);
    }

    let align_code = number(style, "alignment")
        .filter(|value| value.is_finite())
        .map(|value| value.round() as i64)
        .unwrap_or(2);
    let (text_align, vertical_align) = alignment(align_code);
    overrides.text_align = Some(text_align.to_owned());

    let margin_left = number(style, "marginl").map(|value| value / info.play_res_x);
    let margin_right = number(style, "marginr").map(|value| value / info.play_res_x);
    let margin_vertical = number(style, "marginv").map(|value| value / info.play_res_y);
    if margin_left.is_some()
        || margin_right.is_some()
        || margin_vertical.is_some()
        || vertical_align != "bottom"
    {
        overrides.placement = Some(SubtitlePlacement {
            vertical_align: vertical_align.to_owned(),
            margin_left_ratio: margin_left,
            margin_right_ratio: margin_right,
            margin_vertical_ratio: margin_vertical,
        });
    }

    let border_style = number(style, "borderstyle");
    if let Some(back) = field(style, "backcolour").and_then(parse_ass_color) {
        if border_style.map(|value| value.round() as i64) == Some(3) {
            overrides.background = Some(SubtitleBackground {
                enabled: back.alpha > 0.0,
                color: if back.alpha > 0.0 {
                    back.css_color
                } else {
                    String::from("transparent")
                },
            });
        }
    }

    let border_code = border_style.map(|value| value.round() as i64);
    let unsupported = if border_code != Some(1) && border_code != Some(3) {
        true
    } else {
        number(style, "outline").unwrap_or(0.0) > 0.0
            || number(style, "shadow").unwrap_or(0.0) > 0.0
            || number(style, "angle").unwrap_or(0.0) != 0.0
            || number(style, "scalex").is_some_and(|value| value != 100.0)
            || number(style, "scaley").is_some_and(|value| value != 100.0)
            || (underline == Some(true) && strike_out == Some(true))
    };

    (overrides, unsupported)
}

pub fn parse_ass(input: &str) -> ParseSubtitleResult {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return ParseSubtitleResult::default();
    }

    let mut info = ScriptInfo {
        play_res_x: ASS_DEFAULT_PLAY_RES_X,
        play_res_y: ASS_DEFAULT_PLAY_RES_Y,
    };
    let mut styles: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut style_format: Vec<String> = Vec::new();
    let mut event_format: Vec<String> = Vec::new();
    let mut section = String::new();

    let mut captions: Vec<SubtitleCue> = Vec::new();
    let mut skipped = 0usize;
    let mut inline_tag_cues = 0usize;
    let mut effect_cues = 0usize;
    let mut missing_style_cues = 0usize;
    let mut non_dialogue_events = 0usize;
    let mut unsupported_styles = false;

    for raw_line in normalized.split('\n') {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            section = name.trim().to_lowercase();
            continue;
        }

        if section == "script info" {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let key = key.trim().to_lowercase();
            let Some(number) = parse_float_prefix(value) else {
                continue;
            };
            if !number.is_finite() || number <= 0.0 {
                continue;
            }
            if key == "playresx" {
                info.play_res_x = number;
            } else if key == "playresy" {
                info.play_res_y = number;
            }
            continue;
        }

        if is_style_section(&section) {
            if line.to_lowercase().starts_with("format:") {
                style_format = parse_format(line);
                continue;
            }
            if !line.to_lowercase().starts_with("style:") || style_format.is_empty() {
                continue;
            }
            let Some(record) = record_from(&style_format, line) else {
                continue;
            };
            let Some(name) = field(&record, "name")
                .map(str::to_owned)
                .filter(|name| !name.is_empty())
            else {
                continue;
            };
            styles.insert(name.to_lowercase(), record);
            continue;
        }

        if section != "events" {
            continue;
        }

        if line.to_lowercase().starts_with("format:") {
            event_format = parse_format(line);
            continue;
        }
        if event_format.is_empty() {
            continue;
        }
        if !line.to_lowercase().starts_with("dialogue:") {
            if line.contains(':') {
                non_dialogue_events += 1;
            }
            continue;
        }

        let Some(record) = record_from(&event_format, line) else {
            skipped += 1;
            continue;
        };

        let start = field(&record, "start").and_then(parse_ass_timestamp);
        let end = field(&record, "end").and_then(parse_ass_timestamp);
        let (Some(start), Some(end)) = (start, end) else {
            skipped += 1;
            continue;
        };
        let duration = end - start;
        if duration <= 0.0 {
            skipped += 1;
            continue;
        }

        let (text, had_tags) = strip_ass_text(field(&record, "text").unwrap_or_default());
        if had_tags {
            inline_tag_cues += 1;
        }
        if text.is_empty() {
            skipped += 1;
            continue;
        }
        if field(&record, "effect")
            .map(str::trim)
            .is_some_and(|effect| !effect.is_empty())
        {
            effect_cues += 1;
        }

        let referenced = field(&record, "style")
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_lowercase);
        let resolved = match referenced {
            Some(name) => {
                let found = styles.get(&name);
                if found.is_none() {
                    missing_style_cues += 1;
                }
                found.or_else(|| styles.get("default"))
            }
            None => styles.get("default"),
        };

        let style = resolved.map(|style| {
            let (overrides, unsupported) = style_overrides(style, &info);
            unsupported_styles |= unsupported;
            overrides
        });

        captions.push(SubtitleCue {
            text,
            start_time: start,
            duration,
            style,
        });
    }

    let mut warnings = Vec::new();
    if inline_tag_cues > 0 {
        warnings.push(SubtitleWarning::InlineTags(inline_tag_cues));
    }
    if effect_cues > 0 {
        warnings.push(SubtitleWarning::Effects(effect_cues));
    }
    if missing_style_cues > 0 {
        warnings.push(SubtitleWarning::MissingStyles(missing_style_cues));
    }
    if non_dialogue_events > 0 {
        warnings.push(SubtitleWarning::NonDialogue(non_dialogue_events));
    }
    if unsupported_styles {
        warnings.push(SubtitleWarning::UnsupportedStyles);
    }

    ParseSubtitleResult {
        captions,
        skipped_cue_count: skipped,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "[Script Info]\n\
PlayResX: 1280\n\
PlayResY: 720\n\
\n\
[V4+ Styles]\n\
Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, Alignment, MarginL, MarginR, MarginV, Encoding\n\
Style: Default,Verdana,36,&H00FFFFFF,&H000000FF,&H00000000,&H80000000,-1,0,0,0,100,100,0,0,3,0,0,2,64,64,36,1\n\
\n\
[Events]\n\
Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text\n\
Dialogue: 0,0:00:01.00,0:00:03.50,Default,,0,0,0,,Hello, world\n\
Dialogue: 0,0:00:04.00,0:00:05.00,Default,,0,0,0,karaoke,{\\i1}Styled{\\i0} line\n\
Comment: 0,0:00:06.00,0:00:07.00,Default,,0,0,0,,ignored\n";

    #[test]
    fn dialogue_timings_and_text_survive_the_comma_split() {
        let parsed = parse_ass(SAMPLE);
        assert_eq!(parsed.captions.len(), 2);
        assert!((parsed.captions[0].start_time - 1.0).abs() < 1e-9);
        assert!((parsed.captions[0].duration - 2.5).abs() < 1e-9);
        assert_eq!(parsed.captions[0].text, "Hello, world");
    }

    #[test]
    fn inline_override_tags_are_stripped_and_counted() {
        let parsed = parse_ass(SAMPLE);
        assert_eq!(parsed.captions[1].text, "Styled line");
        assert!(parsed.warnings.contains(&SubtitleWarning::InlineTags(1)));
        assert!(parsed.warnings.contains(&SubtitleWarning::Effects(1)));
        assert!(parsed.warnings.contains(&SubtitleWarning::NonDialogue(1)));
    }

    #[test]
    fn the_font_size_becomes_a_fraction_of_the_play_height() {
        let parsed = parse_ass(SAMPLE);
        let style = parsed.captions[0].style.as_ref().expect("style");
        assert_eq!(style.font_size_ratio_of_play_height, Some(0.05));
        assert_eq!(style.font_family.as_deref(), Some("Verdana"));
        assert_eq!(style.font_weight.as_deref(), Some("bold"));
    }

    #[test]
    fn margins_scale_by_the_matching_play_resolution() {
        let parsed = parse_ass(SAMPLE);
        let placement = parsed.captions[0]
            .style
            .as_ref()
            .and_then(|style| style.placement.as_ref())
            .expect("placement");
        assert_eq!(placement.vertical_align, "bottom");
        assert!((placement.margin_left_ratio.expect("left") - 64.0 / 1280.0).abs() < 1e-9);
        assert!((placement.margin_vertical_ratio.expect("vertical") - 36.0 / 720.0).abs() < 1e-9);
    }

    #[test]
    fn an_opaque_primary_colour_is_written_as_hex() {
        let color = parse_ass_color("&H00FFFFFF").expect("colour");
        assert_eq!(color.css_color, "#ffffff");
        assert!((color.alpha - 1.0).abs() < 1e-9);
    }

    #[test]
    fn ass_stores_colours_as_bgr_with_inverted_alpha() {
        let color = parse_ass_color("&H8000FF00").expect("colour");
        assert_eq!(color.css_color, "rgba(0, 255, 0, 0.498)");
    }

    #[test]
    fn a_translucent_backcolour_only_becomes_a_background_for_border_style_three() {
        let parsed = parse_ass(SAMPLE);
        let background = parsed.captions[0]
            .style
            .as_ref()
            .and_then(|style| style.background.as_ref())
            .expect("background");
        assert!(background.enabled);
        assert_eq!(background.color, "rgba(0, 0, 0, 0.498)");
    }

    #[test]
    fn timestamps_pad_the_fraction_on_the_right() {
        assert_eq!(parse_ass_timestamp("0:00:01.5"), Some(1.5));
        assert_eq!(parse_ass_timestamp("0:00:01.50"), Some(1.5));
        assert_eq!(parse_ass_timestamp("0:00:01.005"), Some(1.005));
        assert_eq!(parse_ass_timestamp("0:00:01.0050"), None);
        assert_eq!(parse_ass_timestamp("0:00:01,50"), None);
    }

    #[test]
    fn only_the_last_field_keeps_its_commas() {
        assert_eq!(
            split_ass_fields("a, b, c, d, e", 3),
            vec![
                String::from("a"),
                String::from("b"),
                String::from("c, d, e")
            ]
        );
    }

    #[test]
    fn escapes_become_newlines_and_hard_spaces() {
        let (text, had_tags) = strip_ass_text("{\\an8}one\\Ntwo\\nthree\\hfour");
        assert_eq!(text, "one\ntwo\nthree four");
        assert!(had_tags);
    }

    #[test]
    fn dialogue_before_a_format_line_is_ignored_without_counting_as_skipped() {
        let parsed =
            parse_ass("[Events]\nDialogue: 0,0:00:01.00,0:00:02.00,Default,,0,0,0,,orphan\n");
        assert!(parsed.captions.is_empty());
        assert_eq!(parsed.skipped_cue_count, 0);
    }

    #[test]
    fn backwards_and_empty_cues_count_as_skipped() {
        let parsed = parse_ass(
            "[Events]\nFormat: Start, End, Text\n\
Dialogue: 0:00:05.00,0:00:04.00,backwards\n\
Dialogue: 0:00:01.00,0:00:02.00,{\\p1}\n",
        );
        assert!(parsed.captions.is_empty());
        assert_eq!(parsed.skipped_cue_count, 2);
    }

    #[test]
    fn a_missing_style_reference_falls_back_to_default() {
        let parsed = parse_ass(&SAMPLE.replace("Default,,0,0,0,,Hello", "Ghost,,0,0,0,,Hello"));
        assert!(parsed.warnings.contains(&SubtitleWarning::MissingStyles(1)));
        assert!(parsed.captions[0].style.is_some());
    }

    #[test]
    fn play_resolution_defaults_when_the_header_is_absent() {
        let parsed = parse_ass(
            "[V4+ Styles]\nFormat: Name, Fontsize, BorderStyle\nStyle: Default,28.8,1\n\
[Events]\nFormat: Start, End, Style, Text\nDialogue: 0:00:00.00,0:00:01.00,Default,hi\n",
        );
        let style = parsed.captions[0].style.as_ref().expect("style");
        assert_eq!(style.font_size_ratio_of_play_height, Some(0.1));
    }

    #[test]
    fn a_numeric_prefix_parses_the_way_javascript_does() {
        assert_eq!(parse_float_prefix("384abc"), Some(384.0));
        assert_eq!(parse_float_prefix("  -1 "), Some(-1.0));
        assert_eq!(parse_float_prefix(""), None);
        assert_eq!(parse_float_prefix("abc"), None);
    }

    #[test]
    fn outline_and_shadow_raise_the_unsupported_warning() {
        let parsed = parse_ass(&SAMPLE.replace(",0,0,2,64,64,36,1", ",2,1,2,64,64,36,1"));
        assert!(parsed
            .warnings
            .contains(&SubtitleWarning::UnsupportedStyles));
    }
}
