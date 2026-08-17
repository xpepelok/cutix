use crate::subtitles::{ParseSubtitleResult, SubtitleCue};

pub fn parse_vtt_timestamp(raw: &str) -> Option<f64> {
    let text = raw.trim();
    let (clock, fraction) = text.split_once('.')?;
    if fraction.len() != 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let parts: Vec<&str> = clock.split(':').collect();
    if !matches!(parts.len(), 2 | 3) {
        return None;
    }
    if parts
        .iter()
        .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return None;
    }
    let (hours, minutes, seconds) = if parts.len() == 3 {
        (parts[0], parts[1], parts[2])
    } else {
        ("0", parts[0], parts[1])
    };
    if minutes.len() != 2 || seconds.len() != 2 {
        return None;
    }
    let hours: f64 = hours.parse().ok()?;
    let minutes: f64 = minutes.parse().ok()?;
    let seconds: f64 = seconds.parse().ok()?;
    let milliseconds: f64 = fraction.parse().ok()?;
    Some(hours * 3600.0 + minutes * 60.0 + seconds + milliseconds / 1000.0)
}

pub fn strip_vtt_tags(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(open) = rest.find('<') {
        match rest[open..].find('>') {
            Some(offset) => {
                out.push_str(&rest[..open]);
                rest = &rest[open + offset + 1..];
            }
            None => break,
        }
    }
    out.push_str(rest);
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", "\u{a0}")
        .replace("&lrm;", "")
        .replace("&rlm;", "")
}

fn strip_cue_settings(after_arrow: &str) -> &str {
    after_arrow
        .trim()
        .split_whitespace()
        .next()
        .unwrap_or_default()
}

pub fn parse_vtt(input: &str) -> ParseSubtitleResult {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");

    let normalized = normalized.trim_start_matches('\u{feff}');
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return ParseSubtitleResult::default();
    }

    let mut captions = Vec::new();
    let mut skipped = 0usize;

    for block in normalized.split("\n\n") {
        let lines: Vec<&str> = block
            .split('\n')
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        if lines.is_empty() {
            continue;
        }
        let head = lines[0];
        if head.starts_with("WEBVTT")
            || head.starts_with("NOTE")
            || head.starts_with("STYLE")
            || head.starts_with("REGION")
        {
            continue;
        }

        let timing_index = lines.iter().position(|line| line.contains("-->"));
        let Some(timing_index) = timing_index else {
            skipped += 1;
            continue;
        };

        let timing = lines[timing_index];
        let arrow = timing.find("-->").expect("checked above");
        let start = parse_vtt_timestamp(&timing[..arrow]);
        let end = parse_vtt_timestamp(strip_cue_settings(&timing[arrow + 3..]));
        let (Some(start), Some(end)) = (start, end) else {
            skipped += 1;
            continue;
        };
        if end <= start {
            skipped += 1;
            continue;
        }

        let text = lines[timing_index + 1..]
            .iter()
            .map(|line| strip_vtt_tags(line))
            .collect::<Vec<_>>()
            .join("\n");
        let text = text.trim().to_owned();
        if text.is_empty() {
            skipped += 1;
            continue;
        }

        captions.push(SubtitleCue {
            text,
            start_time: start,
            duration: end - start,
            style: None,
        });
    }

    ParseSubtitleResult {
        captions,
        skipped_cue_count: skipped,
        warnings: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "WEBVTT - Some title\n\
\n\
NOTE this block is a comment\n\
and continues here\n\
\n\
intro\n\
00:00:01.000 --> 00:00:03.500 line:0 position:20% align:start\n\
<v Narrator>Hello, <i>world</i>\n\
\n\
02:00.000 --> 02:01.250\n\
Short form timestamps\n\
\n\
00:00:09.000 --> 00:00:08.000\n\
Backwards\n";

    #[test]
    fn cue_identifiers_settings_and_tags_are_dropped() {
        let parsed = parse_vtt(SAMPLE);
        assert_eq!(parsed.captions.len(), 2);
        assert_eq!(parsed.captions[0].text, "Hello, world");
        assert!((parsed.captions[0].start_time - 1.0).abs() < 1e-9);
        assert!((parsed.captions[0].duration - 2.5).abs() < 1e-9);
    }

    #[test]
    fn the_header_and_note_blocks_are_not_cues() {
        let parsed = parse_vtt(SAMPLE);
        assert!(parsed
            .captions
            .iter()
            .all(|cue| !cue.text.contains("comment")));
    }

    #[test]
    fn minute_form_timestamps_are_accepted() {
        let parsed = parse_vtt(SAMPLE);
        assert!((parsed.captions[1].start_time - 120.0).abs() < 1e-9);
        assert!((parsed.captions[1].duration - 1.25).abs() < 1e-9);
    }

    #[test]
    fn a_backwards_cue_counts_as_skipped() {
        assert_eq!(parse_vtt(SAMPLE).skipped_cue_count, 1);
    }

    #[test]
    fn timestamps_need_three_fraction_digits() {
        assert_eq!(parse_vtt_timestamp("00:00:01.000"), Some(1.0));
        assert_eq!(parse_vtt_timestamp("01:02.500"), Some(62.5));
        assert_eq!(parse_vtt_timestamp("00:00:01.50"), None);
        assert_eq!(parse_vtt_timestamp("00:00:01,500"), None);
    }

    #[test]
    fn entities_are_decoded() {
        assert_eq!(strip_vtt_tags("a &amp; b &lt;c&gt;"), "a & b <c>");
    }

    #[test]
    fn an_empty_file_yields_nothing() {
        assert_eq!(parse_vtt("  "), ParseSubtitleResult::default());
        assert_eq!(parse_vtt("WEBVTT\n"), ParseSubtitleResult::default());
    }
}
