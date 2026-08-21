#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubtitleStyleOverrides {
    pub font_family: Option<String>,

    pub font_size_ratio_of_play_height: Option<f64>,
    pub color: Option<String>,
    pub font_weight: Option<String>,
    pub font_style: Option<String>,
    pub text_decoration: Option<String>,
    pub letter_spacing: Option<f64>,
    pub text_align: Option<String>,
    pub placement: Option<SubtitlePlacement>,
    pub background: Option<SubtitleBackground>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubtitlePlacement {
    pub vertical_align: String,
    pub margin_left_ratio: Option<f64>,
    pub margin_right_ratio: Option<f64>,
    pub margin_vertical_ratio: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubtitleBackground {
    pub enabled: bool,
    pub color: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubtitleWarning {
    InlineTags(usize),
    Effects(usize),
    MissingStyles(usize),
    NonDialogue(usize),
    UnsupportedStyles,
}

impl SubtitleWarning {
    pub fn key(&self) -> &'static str {
        match self {
            Self::InlineTags(_) => "captions.ass.warn.inlineTags",
            Self::Effects(_) => "captions.ass.warn.effects",
            Self::MissingStyles(_) => "captions.ass.warn.missingStyles",
            Self::NonDialogue(_) => "captions.ass.warn.nonDialogue",
            Self::UnsupportedStyles => "captions.ass.warn.unsupportedStyles",
        }
    }

    pub fn count(&self) -> Option<usize> {
        match self {
            Self::InlineTags(count)
            | Self::Effects(count)
            | Self::MissingStyles(count)
            | Self::NonDialogue(count) => Some(*count),
            Self::UnsupportedStyles => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubtitleFormat {
    Srt,
    Ass,
    Vtt,
}

impl SubtitleFormat {
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.trim_start_matches('.').to_lowercase().as_str() {
            "srt" => Some(Self::Srt),
            "ass" | "ssa" => Some(Self::Ass),
            "vtt" | "webvtt" => Some(Self::Vtt),
            _ => None,
        }
    }
}

pub fn parse_subtitle_file(input: &str, extension: &str) -> Option<ParseSubtitleResult> {
    match SubtitleFormat::from_extension(extension)? {
        SubtitleFormat::Srt => Some(parse_srt(input)),
        SubtitleFormat::Ass => Some(crate::subtitle_ass::parse_ass(input)),
        SubtitleFormat::Vtt => Some(crate::subtitle_vtt::parse_vtt(input)),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SubtitleCue {
    pub text: String,

    pub start_time: f64,
    pub duration: f64,
    pub style: Option<SubtitleStyleOverrides>,
}

impl SubtitleCue {
    pub fn new(text: impl Into<String>, start_time: f64, duration: f64) -> Self {
        Self {
            text: text.into(),
            start_time,
            duration,
            style: None,
        }
    }

    pub fn end_time(&self) -> f64 {
        self.start_time + self.duration
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParseSubtitleResult {
    pub captions: Vec<SubtitleCue>,
    pub skipped_cue_count: usize,
    pub warnings: Vec<SubtitleWarning>,
}

fn parse_timestamp(raw: &str) -> Option<f64> {
    let normalized = raw.trim().replace(',', ".");
    let (clock, fraction) = normalized.split_once('.')?;
    if fraction.is_empty()
        || fraction.len() > 3
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let parts: Vec<&str> = clock.split(':').collect();
    if parts.len() != 3 || parts.iter().any(|part| part.len() != 2) {
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

fn split_arrow(line: &str) -> Option<(&str, &str)> {
    let index = line.find("-->")?;
    Some((&line[..index], &line[index + 3..]))
}

pub fn parse_srt(input: &str) -> ParseSubtitleResult {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let normalized = normalized.trim();
    if normalized.is_empty() {
        return ParseSubtitleResult::default();
    }

    let mut captions = Vec::new();
    let mut skipped = 0usize;

    for block in normalized.split("\n\n") {
        let lines: Vec<&str> = block
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect();
        if lines.len() < 2 {
            if !lines.is_empty() {
                skipped += 1;
            }
            continue;
        }

        let timestamp_index = if lines[0].contains("-->") { 0 } else { 1 };
        let Some(timestamp_line) = lines.get(timestamp_index) else {
            skipped += 1;
            continue;
        };
        let Some((raw_start, raw_end)) = split_arrow(timestamp_line) else {
            skipped += 1;
            continue;
        };
        let (Some(start_time), Some(end_time)) =
            (parse_timestamp(raw_start), parse_timestamp(raw_end))
        else {
            skipped += 1;
            continue;
        };

        let text = lines[timestamp_index + 1..].join("\n").trim().to_owned();
        if text.is_empty() {
            skipped += 1;
            continue;
        }

        let duration = end_time - start_time;
        if !start_time.is_finite() || !end_time.is_finite() || duration <= 0.0 {
            skipped += 1;
            continue;
        }

        captions.push(SubtitleCue {
            text,
            start_time,
            duration,
            style: None,
        });
    }

    ParseSubtitleResult {
        captions,
        skipped_cue_count: skipped,
        warnings: Vec::new(),
    }
}

pub fn format_srt_timestamp(seconds: f64) -> String {
    let clamped = seconds.max(0.0);
    let total_milliseconds = (clamped * 1000.0).round() as i64;
    let milliseconds = total_milliseconds % 1000;
    let total_seconds = (total_milliseconds - milliseconds) / 1000;
    let whole_seconds = total_seconds % 60;
    let total_minutes = (total_seconds - whole_seconds) / 60;
    let minutes = total_minutes % 60;
    let hours = (total_minutes - minutes) / 60;
    format!("{hours:02}:{minutes:02}:{whole_seconds:02},{milliseconds:03}")
}

fn normalize_cue_text(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn serialize_srt(cues: &[SubtitleCue]) -> String {
    let mut blocks = Vec::new();
    for cue in cues {
        let text = normalize_cue_text(&cue.text);
        if text.is_empty() {
            continue;
        }
        let start_time = cue.start_time.max(0.0);
        let end_time = start_time + cue.duration.max(0.0);
        if !start_time.is_finite() || end_time <= start_time {
            continue;
        }
        let index = blocks.len() + 1;
        blocks.push(format!(
            "{index}\n{} --> {}\n{text}",
            format_srt_timestamp(start_time),
            format_srt_timestamp(end_time)
        ));
    }

    if blocks.is_empty() {
        return String::new();
    }
    format!("{}\n", blocks.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "1\n00:00:01,000 --> 00:00:03,500\nHello there\n\n2\n00:00:04,000 --> 00:00:06,000\nSecond line\nover two rows\n";

    #[test]
    fn parses_numbered_cues() {
        let result = parse_srt(SAMPLE);
        assert_eq!(result.skipped_cue_count, 0);
        assert_eq!(result.captions.len(), 2);
        assert_eq!(result.captions[0].text, "Hello there");
        assert!((result.captions[0].start_time - 1.0).abs() < 1e-9);
        assert!((result.captions[0].duration - 2.5).abs() < 1e-9);
        assert_eq!(result.captions[1].text, "Second line\nover two rows");
        assert!((result.captions[1].end_time() - 6.0).abs() < 1e-9);
    }

    #[test]
    fn a_cue_number_is_optional() {
        let result = parse_srt("00:00:00,000 --> 00:00:02,000\nNo index");
        assert_eq!(result.captions.len(), 1);
        assert_eq!(result.captions[0].text, "No index");
    }

    #[test]
    fn dot_separators_and_crlf_are_accepted() {
        let result = parse_srt("1\r\n00:00:01.25 --> 00:00:02.5\r\nDots\r\n");
        assert_eq!(result.captions.len(), 1);
        assert!((result.captions[0].start_time - 1.25).abs() < 1e-9);
        assert!((result.captions[0].duration - 1.25).abs() < 1e-9);
    }

    #[test]
    fn malformed_cues_are_counted_not_dropped_silently() {
        let result = parse_srt(
            "1\nnot a timestamp\nText\n\n2\n00:00:05,000 --> 00:00:04,000\nBackwards\n\n3\n00:00:07,000 --> 00:00:08,000\nGood",
        );
        assert_eq!(result.captions.len(), 1);
        assert_eq!(result.captions[0].text, "Good");
        assert_eq!(result.skipped_cue_count, 2);
    }

    #[test]
    fn an_empty_file_yields_nothing() {
        assert_eq!(parse_srt("   \n\n  "), ParseSubtitleResult::default());
    }

    #[test]
    fn timestamps_format_like_the_web() {
        assert_eq!(format_srt_timestamp(0.0), "00:00:00,000");
        assert_eq!(format_srt_timestamp(1.5), "00:00:01,500");
        assert_eq!(format_srt_timestamp(3661.25), "01:01:01,250");
        assert_eq!(format_srt_timestamp(-5.0), "00:00:00,000");
    }

    #[test]
    fn serialising_round_trips_a_parsed_file() {
        let parsed = parse_srt(SAMPLE);
        let serialized = serialize_srt(&parsed.captions);
        assert_eq!(serialized, SAMPLE);
        assert_eq!(parse_srt(&serialized).captions, parsed.captions);
    }

    #[test]
    fn zero_length_cues_are_dropped_when_serialising() {
        let cues = vec![
            SubtitleCue {
                text: String::from("keep"),
                start_time: 0.0,
                duration: 1.0,
                style: None,
            },
            SubtitleCue {
                text: String::from("drop"),
                start_time: 2.0,
                duration: 0.0,
                style: None,
            },
            SubtitleCue {
                text: String::from("   "),
                start_time: 3.0,
                duration: 1.0,
                style: None,
            },
        ];
        let serialized = serialize_srt(&cues);
        assert_eq!(serialized, "1\n00:00:00,000 --> 00:00:01,000\nkeep\n");
    }
}
