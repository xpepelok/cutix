#![allow(dead_code)]

use gpui::{px, Hsla, Pixels};

pub const TOKENS_ROOT: &[(&str, &str)] = &[
    ("--background", "hsl(0, 0%, 100%)"),
    ("--foreground", "hsl(0 0% 11%)"),
    ("--card", "hsl(0, 0%, 100%)"),
    ("--card-foreground", "hsl(0 0% 11%)"),
    ("--popover", "hsl(0, 0%, 100%)"),
    ("--popover-hover", "hsl(0, 0%, 96%)"),
    ("--popover-foreground", "hsl(0 0% 2%)"),
    ("--primary", "hsl(200, 90%, 52%)"),
    ("--primary-foreground", "hsl(0, 0%, 100%)"),
    ("--secondary", "hsl(204, 100%, 97%)"),
    ("--secondary-border", "hsl(204, 100%, 94%)"),
    ("--secondary-foreground", "hsl(200, 98%, 39%)"),
    ("--muted", "hsl(0 0% 85.1%)"),
    ("--muted-foreground", "hsl(0 0% 50%)"),
    ("--accent", "hsl(0, 0%, 96%)"),
    ("--accent-foreground", "hsl(0 0% 2%)"),
    ("--destructive", "hsl(0, 83%, 50%)"),
    ("--destructive-foreground", "hsl(0, 0%, 100%)"),
    ("--constructive", "hsl(141, 71%, 48%)"),
    ("--constructive-foreground", "hsl(0, 0%, 100%)"),
    ("--caution", "hsl(38, 92%, 50%)"),
    ("--caution-foreground", "hsl(0, 0%, 10%)"),
    ("--border", "hsl(0 0% 91%)"),
    ("--input", "hsl(0, 0%, 100%)"),
    ("--ring", "hsl(0, 0%, 55%)"),
    ("--sidebar", "hsl(0 0% 98%)"),
    ("--scrollbar-thumb", "hsl(0 0% 78%)"),
    ("--scrollbar-thumb-hover", "hsl(0 0% 65%)"),
    ("--scrollbar-thumb-active", "hsl(0 0% 55%)"),
];

pub const TOKENS_PANEL: &[(&str, &str)] = &[
    ("--background", "hsl(210, 20%, 98%)"),
    ("--foreground", "hsl(0 0% 13%)"),
    ("--card", "hsl(0, 0%, 98%)"),
    ("--card-foreground", "hsl(0 0% 13%)"),
    ("--primary-foreground", "hsl(0, 0%, 98%)"),
    ("--secondary", "hsl(204, 100%, 95%)"),
    ("--secondary-border", "hsl(204, 100%, 92%)"),
    ("--secondary-foreground", "hsl(200, 98%, 37%)"),
    ("--muted", "hsl(0 0% 83.1%)"),
    ("--muted-foreground", "hsl(0 0% 48%)"),
    ("--accent", "hsl(0, 0%, 93%)"),
    ("--accent-foreground", "hsl(0 0% 5%)"),
    ("--destructive-foreground", "hsl(0, 0%, 98%)"),
    ("--constructive-foreground", "hsl(0, 0%, 98%)"),
    ("--border", "hsl(0 0% 87%)"),
    ("--input", "hsl(0 0% 93%)"),
    ("--ring", "hsl(0, 0%, 53%)"),
];

pub const TOKENS_DARK: &[(&str, &str)] = &[
    ("--background", "hsl(0, 0%, 5%)"),
    ("--foreground", "hsl(0 0% 87%)"),
    ("--card", "hsl(0, 0%, 5%)"),
    ("--card-foreground", "hsl(0 0% 87%)"),
    ("--popover", "hsl(0, 0%, 5%)"),
    ("--popover-hover", "hsl(0, 0%, 13%)"),
    ("--popover-foreground", "hsl(0 0% 95%)"),
    ("--secondary", "hsl(204, 100%, 12%)"),
    ("--secondary-border", "hsl(204, 100%, 15%)"),
    ("--secondary-foreground", "hsl(200, 98%, 61%)"),
    ("--muted", "hsl(0 0% 20%)"),
    ("--accent", "hsl(0, 0%, 14%)"),
    ("--accent-foreground", "hsl(0 0% 95%)"),
    ("--border", "hsl(0 0% 16%)"),
    ("--input", "hsl(0 0% 5%)"),
    ("--ring", "hsl(0, 0%, 50%)"),
    ("--caution", "hsl(38, 92%, 60%)"),
    ("--caution-foreground", "hsl(0, 0%, 10%)"),
    ("--sidebar", "hsl(0 0% 6%)"),
    ("--scrollbar-thumb", "hsl(0 0% 26%)"),
    ("--scrollbar-thumb-hover", "hsl(0 0% 38%)"),
    ("--scrollbar-thumb-active", "hsl(0 0% 48%)"),
];

pub const TOKENS_DARK_PANEL: &[(&str, &str)] = &[
    ("--background", "hsl(0 0% 10%)"),
    ("--foreground", "hsl(0 0% 85%)"),
    ("--card", "hsl(0, 0%, 10%)"),
    ("--card-foreground", "hsl(0 0% 85%)"),
    ("--secondary", "hsl(204, 67%, 9%)"),
    ("--secondary-border", "hsl(204, 100%, 14%)"),
    ("--secondary-foreground", "hsl(200, 98%, 63%)"),
    ("--muted", "hsl(0 0% 22%)"),
    ("--accent", "hsl(0, 0%, 15%)"),
    ("--accent-foreground", "hsl(0 0% 93%)"),
    ("--border", "hsl(0 0% 18%)"),
    ("--input", "hsl(0 0% 22%)"),
    ("--ring", "hsl(0, 0%, 52%)"),
];

pub const TIMELINE_TRACK_COLORS: &[(&str, &str)] = &[
    ("text", "#5DBAA0"),
    ("audio", "#8F5DBA"),
    ("graphic", "#BA5D7A"),
    ("effect", "#5d93ba"),
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub background: Hsla,
    pub foreground: Hsla,
    pub card: Hsla,
    pub popover: Hsla,
    pub popover_hover: Hsla,
    pub popover_foreground: Hsla,
    pub primary: Hsla,
    pub primary_foreground: Hsla,
    pub secondary: Hsla,
    pub secondary_border: Hsla,
    pub secondary_foreground: Hsla,
    pub muted: Hsla,
    pub muted_foreground: Hsla,
    pub accent: Hsla,
    pub accent_foreground: Hsla,
    pub destructive: Hsla,
    pub destructive_foreground: Hsla,
    pub caution: Hsla,
    pub border: Hsla,
    pub input: Hsla,
    pub ring: Hsla,
    pub scrollbar_thumb: Hsla,
    pub scrollbar_thumb_hover: Hsla,
    pub scrollbar_thumb_active: Hsla,
}

impl Palette {
    fn from_layers(layers: &[&[(&str, &str)]]) -> Self {
        let pick = |name: &str| -> Hsla {
            let mut found = None;
            for layer in layers {
                if let Some((_, value)) = layer.iter().find(|(key, _)| *key == name) {
                    found = Some(*value);
                }
            }
            parse_hsl(found.unwrap_or("hsl(0 0% 50%)"))
        };

        Self {
            background: pick("--background"),
            foreground: pick("--foreground"),
            card: pick("--card"),
            popover: pick("--popover"),
            popover_hover: pick("--popover-hover"),
            popover_foreground: pick("--popover-foreground"),
            primary: pick("--primary"),
            primary_foreground: pick("--primary-foreground"),
            secondary: pick("--secondary"),
            secondary_border: pick("--secondary-border"),
            secondary_foreground: pick("--secondary-foreground"),
            muted: pick("--muted"),
            muted_foreground: pick("--muted-foreground"),
            accent: pick("--accent"),
            accent_foreground: pick("--accent-foreground"),
            destructive: pick("--destructive"),
            destructive_foreground: pick("--destructive-foreground"),
            caution: pick("--caution"),
            border: pick("--border"),
            input: pick("--input"),
            ring: pick("--ring"),
            scrollbar_thumb: pick("--scrollbar-thumb"),
            scrollbar_thumb_hover: pick("--scrollbar-thumb-hover"),
            scrollbar_thumb_active: pick("--scrollbar-thumb-active"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub root: Palette,
    pub panel: Palette,
}

impl Theme {
    pub fn dark() -> Self {
        Self {
            root: Palette::from_layers(&[TOKENS_ROOT, TOKENS_DARK]),
            panel: Palette::from_layers(&[
                TOKENS_ROOT,
                TOKENS_DARK,
                TOKENS_PANEL,
                TOKENS_DARK_PANEL,
            ]),
        }
    }

    pub fn light() -> Self {
        Self {
            root: Palette::from_layers(&[TOKENS_ROOT]),
            panel: Palette::from_layers(&[TOKENS_ROOT, TOKENS_PANEL]),
        }
    }
}

pub const REM: f32 = 16.0;

pub fn rem(value: f32) -> Pixels {
    px(value * REM)
}

pub const TEXT_XS: f32 = 0.72;
pub const TEXT_SM: f32 = 0.79;
pub const TEXT_BASE: f32 = 0.92;
pub const TEXT_LG: f32 = 1.125;
pub const TEXT_TAB_LABEL: f32 = 0.6875;
pub const TEXT_PROJECT_NAME: f32 = 0.9;

pub const RADIUS_SM: f32 = 0.35;
pub const RADIUS_MD: f32 = 0.65;
pub const RADIUS_LG: f32 = 0.82;

pub const TITLEBAR_HEIGHT: f32 = 34.0;
pub const TITLEBAR_BUTTON_WIDTH: f32 = 46.0;
pub const HEADER_HEIGHT: f32 = 3.4 * REM;
pub const SCROLLBAR_SIZE: f32 = 8.0;

pub const TIMELINE_TRACK_LABELS_COLUMN_WIDTH_PX: f32 = 112.0;
pub const TIMELINE_RULER_HEIGHT_PX: f32 = 22.0;
pub const TIMELINE_BOOKMARK_ROW_HEIGHT_PX: f32 = 16.0;
pub const TIMELINE_CONTENT_TOP_PADDING_PX: f32 = 2.0;
pub const TIMELINE_TRACK_GAP_PX: f32 = 6.0;
pub const TIMELINE_TOOLBAR_HEIGHT_PX: f32 = 40.0;
pub const TIMELINE_INDICATOR_LINE_WIDTH_PX: f32 = 2.0;
pub const TIMELINE_PLAYHEAD_HANDLE_PX: f32 = 12.0;
pub const TIMELINE_PLAYHEAD_HANDLE_TOP_PX: f32 = 4.0;

pub const TIMELINE_HEADER_HEIGHT_PX: f32 =
    TIMELINE_RULER_HEIGHT_PX + TIMELINE_BOOKMARK_ROW_HEIGHT_PX + TIMELINE_CONTENT_TOP_PADDING_PX;

pub const BASE_TIMELINE_PIXELS_PER_SECOND: f32 = 50.0;
pub const MIN_LABEL_SPACING_PX: f32 = 120.0;
pub const MIN_TICK_SPACING_PX: f32 = 18.0;
pub const LABEL_FRAME_INTERVALS: &[u32] = &[2, 3, 5, 10, 15];
pub const TICK_FRAME_INTERVALS: &[u32] = &[1, 2, 3, 5, 10, 15];
pub const SECOND_MULTIPLIERS: &[u32] =
    &[1, 2, 3, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600];

pub const DEFAULT_TIMELINE_ZOOM: f32 = 5.75;
pub const DEFAULT_FPS: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RulerConfig {
    pub pixels_per_second: f32,
    pub pixels_per_frame: f32,
    pub label_interval_frames: u32,
    pub tick_interval_frames: u32,
}

impl RulerConfig {
    pub fn label_spacing_px(&self) -> f32 {
        self.label_interval_frames as f32 * self.pixels_per_frame
    }

    pub fn tick_spacing_px(&self) -> f32 {
        self.tick_interval_frames as f32 * self.pixels_per_frame
    }
}

fn optimal_interval(pixels_per_frame: f32, fps: f32, candidates: &[u32], minimum: f32) -> u32 {
    for frames in candidates {
        if *frames as f32 * pixels_per_frame >= minimum {
            return *frames;
        }
    }
    for seconds in SECOND_MULTIPLIERS {
        let frames = (*seconds as f32 * fps).round() as u32;
        if frames as f32 * pixels_per_frame >= minimum {
            return frames;
        }
    }
    (60.0 * fps).round() as u32
}

fn ensure_tick_divides_label(tick: u32, label: u32, fps: f32) -> u32 {
    if label % tick == 0 {
        return tick;
    }
    for frames in TICK_FRAME_INTERVALS.iter().filter(|f| **f >= tick) {
        if label % frames == 0 {
            return *frames;
        }
    }
    for seconds in SECOND_MULTIPLIERS {
        let frames = (*seconds as f32 * fps).round() as u32;
        if frames >= tick && frames > 0 && label % frames == 0 {
            return frames;
        }
    }
    label
}

pub fn ruler_config(zoom_level: f32, fps: f32) -> RulerConfig {
    let fps = if fps > 0.0 { fps } else { DEFAULT_FPS };
    let pixels_per_second = BASE_TIMELINE_PIXELS_PER_SECOND * zoom_level.max(f32::EPSILON);
    let pixels_per_frame = pixels_per_second / fps;

    let label_interval_frames = optimal_interval(
        pixels_per_frame,
        fps,
        LABEL_FRAME_INTERVALS,
        MIN_LABEL_SPACING_PX,
    );
    let tick = optimal_interval(
        pixels_per_frame,
        fps,
        TICK_FRAME_INTERVALS,
        MIN_TICK_SPACING_PX,
    );

    RulerConfig {
        pixels_per_second,
        pixels_per_frame,
        label_interval_frames,
        tick_interval_frames: ensure_tick_divides_label(tick, label_interval_frames, fps),
    }
}

pub fn ruler_label(frame: u32, fps: f32) -> String {
    let fps = if fps > 0.0 { fps } else { DEFAULT_FPS };
    let total = frame as f32 / fps;
    let within = frame % fps.round() as u32;
    if within != 0 {
        return format!("{within}f");
    }
    let seconds = total.round() as u32;
    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes:02}:{secs:02}")
    }
}

pub const DEFAULT_CANVAS_SIZE: (f32, f32) = (1920.0, 1080.0);

pub fn fit_scale(viewport: (f32, f32), canvas: (f32, f32)) -> f32 {
    if viewport.0 <= 0.0 || viewport.1 <= 0.0 || canvas.0 <= 0.0 || canvas.1 <= 0.0 {
        return 1.0;
    }
    (viewport.0 / canvas.0).min(viewport.1 / canvas.1)
}

pub fn scene_size(viewport: (f32, f32), canvas: (f32, f32), zoom: f32) -> (f32, f32) {
    let scale = fit_scale(viewport, canvas) * zoom;
    (canvas.0 * scale, canvas.1 * scale)
}

pub const PANEL_TOOLS_FRACTION: f32 = 0.30;
pub const PANEL_PREVIEW_FRACTION: f32 = 0.44;
pub const PANEL_PROPERTIES_FRACTION: f32 = 0.26;
pub const PANEL_MAIN_CONTENT_FRACTION: f32 = 0.50;

pub const COLUMN_GAP: f32 = 0.19 * REM;
pub const ROW_GAP: f32 = 0.18 * REM;

pub fn opacity(color: Hsla, alpha: f32) -> Hsla {
    Hsla {
        a: color.a * alpha,
        ..color
    }
}

pub fn parse_hsl(value: &str) -> Hsla {
    let inner = value
        .trim()
        .trim_start_matches("hsl(")
        .trim_end_matches(')')
        .replace(',', " ");
    let mut parts = inner.split_whitespace();
    let hue: f32 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
    let saturation: f32 = parts
        .next()
        .and_then(|v| v.trim_end_matches('%').parse().ok())
        .unwrap_or(0.0);
    let lightness: f32 = parts
        .next()
        .and_then(|v| v.trim_end_matches('%').parse().ok())
        .unwrap_or(0.0);

    Hsla {
        h: (hue / 360.0).clamp(0.0, 1.0),
        s: (saturation / 100.0).clamp(0.0, 1.0),
        l: (lightness / 100.0).clamp(0.0, 1.0),
        a: 1.0,
    }
}

pub fn parse_hex(value: &str) -> Hsla {
    let hex = value.trim_start_matches('#');
    let bits = u32::from_str_radix(hex, 16).unwrap_or(0);
    let r = ((bits >> 16) & 0xff) as f32 / 255.0;
    let g = ((bits >> 8) & 0xff) as f32 / 255.0;
    let b = (bits & 0xff) as f32 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let delta = max - min;

    if delta <= f32::EPSILON {
        return Hsla {
            h: 0.0,
            s: 0.0,
            l,
            a: 1.0,
        };
    }

    let s = delta / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    } / 6.0;

    Hsla { h, s, l, a: 1.0 }
}

pub fn track_color(kind: &str) -> Hsla {
    TIMELINE_TRACK_COLORS
        .iter()
        .find(|(name, _)| *name == kind)
        .map(|(_, value)| parse_hex(value))
        .unwrap_or(Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.5,
            a: 1.0,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_background_matches_globals_css() {
        let theme = Theme::dark();
        assert_eq!(theme.root.background, parse_hsl("hsl(0, 0%, 5%)"));
        assert_eq!(theme.panel.background, parse_hsl("hsl(0 0% 10%)"));
    }

    #[test]
    fn panel_muted_foreground_comes_from_the_light_panel_layer() {
        assert_eq!(
            Theme::dark().panel.muted_foreground,
            parse_hsl("hsl(0 0% 48%)")
        );
    }

    #[test]
    fn light_theme_uses_root_tokens_only() {
        assert_eq!(
            Theme::light().root.background,
            parse_hsl("hsl(0, 0%, 100%)")
        );
    }

    #[test]
    fn hex_track_colors_round_trip() {
        let text = track_color("text");
        assert!((text.l - 0.55).abs() < 0.05);
        assert!(text.s > 0.2);
    }

    #[test]
    fn unknown_token_layer_falls_back() {
        assert_eq!(
            parse_hsl("nonsense"),
            Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.0,
                a: 1.0
            }
        );
    }

    #[test]
    fn the_default_ruler_reproduces_the_web_reference() {
        let config = ruler_config(DEFAULT_TIMELINE_ZOOM, DEFAULT_FPS);
        assert_eq!(config.label_interval_frames, 15);
        assert_eq!(config.tick_interval_frames, 3);
        assert!(config.label_spacing_px() >= MIN_LABEL_SPACING_PX);
        assert!(config.tick_spacing_px() >= MIN_TICK_SPACING_PX);
    }

    #[test]
    fn ticks_always_divide_the_label_interval() {
        for zoom in [0.2, 0.5, 1.0, 2.0, 5.75, 12.0, 40.0, 100.0] {
            for fps in [24.0, 25.0, 30.0, 60.0] {
                let config = ruler_config(zoom, fps);
                assert!(config.tick_interval_frames > 0, "{zoom} {fps}");
                assert_eq!(
                    config.label_interval_frames % config.tick_interval_frames,
                    0,
                    "zoom {zoom} fps {fps}"
                );
            }
        }
    }

    #[test]
    fn zooming_out_widens_the_interval_in_frames() {
        let near = ruler_config(20.0, DEFAULT_FPS);
        let far = ruler_config(0.5, DEFAULT_FPS);
        assert!(far.label_interval_frames > near.label_interval_frames);
    }

    #[test]
    fn ruler_labels_split_seconds_from_frames() {
        assert_eq!(ruler_label(0, 30.0), "00:00");
        assert_eq!(ruler_label(15, 30.0), "15f");
        assert_eq!(ruler_label(30, 30.0), "00:01");
        assert_eq!(ruler_label(1800, 30.0), "01:00");
        assert_eq!(ruler_label(108_000, 30.0), "1:00:00");
    }

    #[test]
    fn the_timeline_header_offset_matches_the_web_stack() {
        assert_eq!(TIMELINE_HEADER_HEIGHT_PX, 40.0);
    }

    #[test]
    fn the_preview_aspect_fits_inside_the_viewport() {
        let (w, h) = scene_size((800.0, 600.0), DEFAULT_CANVAS_SIZE, 1.0);
        assert!((w - 800.0).abs() < 1e-3);
        assert!((h - 450.0).abs() < 1e-3);

        let (w, h) = scene_size((800.0, 300.0), DEFAULT_CANVAS_SIZE, 1.0);
        assert!((h - 300.0).abs() < 1e-3);
        assert!((w - 533.333).abs() < 1e-2);
    }

    #[test]
    fn a_degenerate_viewport_falls_back_to_unit_scale() {
        assert_eq!(fit_scale((0.0, 100.0), DEFAULT_CANVAS_SIZE), 1.0);
        assert_eq!(fit_scale((100.0, 100.0), (0.0, 0.0)), 1.0);
    }
}
