//! Subtitles: the style they are rendered with, where they sit on the canvas, and
//! the conversion between timeline text elements and subtitle cues.

use super::*;

impl Editor<'_> {
    pub fn insert_caption_track(&mut self, elements: Vec<TimelineElement>) -> bool {
        if elements.is_empty() {
            return false;
        }
        let id = new_id();
        self.commit("captions.import", |tracks, selection| {
            tracks.overlay.insert(
                0,
                Track::Text {
                    id: id.clone(),
                    name: cutix_i18n::t("editor.tab.captions"),
                    elements,
                    hidden: false,
                },
            );
            selection.clear();
            true
        })
    }

    pub fn replace_tracks(&mut self, replacement: SceneTracks) -> bool {
        self.commit("template.apply", |tracks, selection| {
            if *tracks == replacement {
                return false;
            }
            *tracks = replacement;
            selection.clear();
            true
        })
    }
}

pub const SUBTITLE_FONT_SIZE: f64 = 5.0;

pub const SUBTITLE_BOTTOM_MARGIN_RATIO: f64 = 0.05;

pub fn subtitle_position_y(canvas_height: f64, line_count: usize) -> f64 {
    let scaled_font_size =
        SUBTITLE_FONT_SIZE * canvas_height / cutix_playback::text_render::FONT_SIZE_SCALE_REFERENCE;
    let block_height = scaled_font_size
        * cutix_playback::text_render::DEFAULT_LINE_HEIGHT
        * line_count.max(1) as f64;
    canvas_height / 2.0 - canvas_height * SUBTITLE_BOTTOM_MARGIN_RATIO - block_height / 2.0
}

#[derive(Clone)]
pub struct CaptionStyle {
    pub font_family: String,
    pub font_size: f64,
    pub font_weight: String,
    pub color: String,
    pub letter_spacing: f64,
    pub line_height: f64,
    pub background: TextBackground,
    pub stroke: Option<serde_json::Value>,
    pub shadow: Option<serde_json::Value>,
    pub gradient: Option<serde_json::Value>,
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self {
            font_family: String::from("Arial"),
            font_size: SUBTITLE_FONT_SIZE,
            font_weight: String::from("bold"),
            color: String::from("#ffffff"),
            letter_spacing: 0.0,
            line_height: cutix_playback::text_render::DEFAULT_LINE_HEIGHT,
            background: TextBackground {
                enabled: false,
                color: String::from("#000000"),
                corner_radius: Some(0.0),
                padding_x: Some(0.0),
                padding_y: Some(0.0),
                offset_x: Some(0.0),
                offset_y: Some(0.0),
            },
            stroke: None,
            shadow: None,
            gradient: None,
        }
    }
}

impl CaptionStyle {
    pub fn from_preset(preset: &crate::text::TextPreset) -> Self {
        let patch = crate::text::patch_for(preset);
        let font_size = SUBTITLE_FONT_SIZE * preset.font_size_ratio;
        Self {
            font_family: patch.font_family,
            font_size,
            font_weight: patch.font_weight,
            color: patch.color,
            letter_spacing: patch.letter_spacing,
            line_height: patch.line_height,
            background: TextBackground {
                enabled: patch.background_enabled,
                color: patch.background_color,
                corner_radius: Some(patch.corner_radius),
                padding_x: Some(patch.padding_x),
                padding_y: Some(patch.padding_y),
                offset_x: Some(0.0),
                offset_y: Some(0.0),
            },
            stroke: rescaled_span(&patch.stroke, &["width"], font_size),
            shadow: rescaled_span(&patch.shadow, &["blur", "offsetX", "offsetY"], font_size),
            gradient: Some(patch.gradient),
        }
    }
}

pub(crate) fn rescaled_span(
    value: &serde_json::Value,
    keys: &[&str],
    font_size: f64,
) -> Option<serde_json::Value> {
    if !value
        .get("enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return None;
    }
    let mut value = value.clone();
    let scale = font_size / crate::text::DEFAULT_FONT_SIZE;
    for key in keys {
        let Some(number) = value.get(*key).and_then(serde_json::Value::as_f64) else {
            continue;
        };
        value[*key] = serde_json::Value::from(number * scale);
    }
    Some(value)
}

impl CaptionStyle {
    pub fn with_overrides(&self, overrides: &cutix_project::SubtitleStyleOverrides) -> Self {
        let mut merged = self.clone();
        if let Some(family) = &overrides.font_family {
            merged.font_family = family.clone();
        }
        if let Some(ratio) = overrides.font_size_ratio_of_play_height {
            merged.font_size = ratio * cutix_playback::text_render::FONT_SIZE_SCALE_REFERENCE;
        }
        if let Some(color) = &overrides.color {
            merged.color = color.clone();
        }
        if let Some(weight) = &overrides.font_weight {
            merged.font_weight = weight.clone();
        }
        if let Some(spacing) = overrides.letter_spacing {
            merged.letter_spacing = spacing;
        }
        if let Some(background) = &overrides.background {
            merged.background.enabled = background.enabled;
            merged.background.color = background.color.clone();
        }
        merged
    }
}

pub(crate) fn placement_position_y(
    placement: Option<&cutix_project::SubtitlePlacement>,
    canvas_height: f64,
    line_count: usize,
    font_size: f64,
) -> Option<f64> {
    let placement = placement?;
    let margin = placement
        .margin_vertical_ratio
        .filter(|ratio| ratio.is_finite())
        .map(|ratio| ratio * canvas_height);
    let line_height = font_size * canvas_height
        / cutix_playback::text_render::FONT_SIZE_SCALE_REFERENCE
        * cutix_playback::text_render::DEFAULT_LINE_HEIGHT;
    let block = line_height * line_count as f64;
    match placement.vertical_align.as_str() {
        "top" => Some(-canvas_height / 2.0 + margin.unwrap_or(0.0) + block / 2.0),
        "middle" => Some(0.0),
        _ => {
            let margin = margin?;
            Some(canvas_height / 2.0 - margin - block / 2.0)
        }
    }
}

pub fn subtitle_text_element(
    index: usize,
    cue: &cutix_project::SubtitleCue,
    canvas_width: f64,
    canvas_height: f64,
    base_style: &CaptionStyle,
    rasterizer: &mut cutix_playback::TextRasterizer,
) -> Option<TimelineElement> {
    let merged;
    let style = match cue.style.as_ref() {
        Some(overrides) => {
            merged = base_style.with_overrides(overrides);
            &merged
        }
        None => base_style,
    };
    let overrides = cue.style.as_ref();
    let scaled_font_size =
        style.font_size * canvas_height / cutix_playback::text_render::FONT_SIZE_SCALE_REFERENCE;
    let content = rasterizer.wrap_text(
        cue.text.trim(),
        &style.font_family,
        style.font_weight == "bold",
        scaled_font_size,
        canvas_width * cutix_playback::text_render::SUBTITLE_MAX_WIDTH_RATIO,
    );
    if content.is_empty() {
        return None;
    }
    let start_time = MediaTime::from_seconds_f64(cue.start_time.max(0.0))?;
    let duration = MediaTime::from_seconds_f64(cue.duration)?;
    if duration.as_ticks() <= 0 {
        return None;
    }

    let line_count = content.lines().count();
    let mut base = new_base(format!(
        "{} {}",
        cutix_i18n::t("editor.tab.captions"),
        index + 1
    ));
    base.start_time = start_time;
    base.duration = duration;

    Some(TimelineElement::Text(TextElement {
        base,
        content,
        font_size: style.font_size,
        font_family: style.font_family.clone(),
        color: style.color.clone(),
        background: style.background.clone(),
        stroke: style.stroke.clone(),
        shadow: style.shadow.clone(),
        gradient: style.gradient.clone(),
        text_animations: None,
        text_align: overrides
            .and_then(|overrides| overrides.text_align.clone())
            .unwrap_or_else(|| String::from("center")),
        font_weight: style.font_weight.clone(),
        font_style: overrides
            .and_then(|overrides| overrides.font_style.clone())
            .unwrap_or_else(|| String::from("normal")),
        text_decoration: overrides
            .and_then(|overrides| overrides.text_decoration.clone())
            .unwrap_or_else(|| String::from("none")),
        letter_spacing: Some(style.letter_spacing),
        line_height: Some(style.line_height),
        hidden: None,
        transform: Transform {
            scale_x: 1.0,
            scale_y: 1.0,
            position: Vector2 {
                x: 0.0,
                y: placement_position_y(
                    overrides.and_then(|overrides| overrides.placement.as_ref()),
                    canvas_height,
                    line_count,
                    style.font_size,
                )
                .unwrap_or_else(|| subtitle_position_y(canvas_height, line_count)),
            },
            rotate: 0.0,
        },
        opacity: 1.0,
        blend_mode: None,
        effects: None,
        extra: JsonMap::new(),
    }))
}

pub fn text_elements_as_cues(tracks: &SceneTracks) -> Vec<cutix_project::SubtitleCue> {
    let mut cues: Vec<cutix_project::SubtitleCue> = tracks
        .overlay
        .iter()
        .chain(std::iter::once(&tracks.main))
        .flat_map(|track| track.elements())
        .filter_map(|element| match element {
            TimelineElement::Text(text) => Some(cutix_project::SubtitleCue::new(
                text.content.clone(),
                text.base.start_time.to_seconds_f64(),
                text.base.duration.to_seconds_f64(),
            )),
            _ => None,
        })
        .collect();
    cues.sort_by(|left, right| {
        left.start_time
            .partial_cmp(&right.start_time)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    cues
}
