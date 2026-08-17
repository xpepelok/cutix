use std::path::PathBuf;
use std::time::{Duration, Instant};

use cutix_i18n::{t, t_args};
use cutix_project::{MediaAssetData, TimelineElement, Track};
use gpui::{
    div, img, linear_color_stop, linear_gradient, prelude::*, px, relative, svg, App, Context, Div,
    Entity, ExternalPaths, FontWeight, ScrollHandle, SharedString, Stateful, Window,
};
use time::MediaTime;

use crate::assets::icon;
use crate::components::{
    menu_item, menu_natural_height, menu_surface, overlay_backdrop, overlay_layer, place_anchored,
    separator_h, separator_v, tooltipped, Button, ButtonSize, ButtonVariant, MENU_OFFSET_PX,
};
use crate::edit::{self, Edge};
use crate::effects_ui;
use crate::gizmos;
use crate::graph_editor::{self, Axis};
use crate::input::TextField;
use crate::interaction::{
    mix, Overlay, OverlaySide, Tooltips, Transitions, TOOLBAR_TOOLTIP_DELAY, TOOLTIP_DELAY,
};
use crate::scroll::{scrollbar_h, scrollbar_v};
use crate::state::{format_seconds, media_glyph, AppModel};
use crate::theme::{
    opacity, rem, ruler_config, ruler_label, track_color, Palette, BASE_TIMELINE_PIXELS_PER_SECOND,
    DEFAULT_TIMELINE_ZOOM, RADIUS_LG, RADIUS_SM, TEXT_LG, TEXT_SM, TEXT_TAB_LABEL, TEXT_XS,
    TIMELINE_BOOKMARK_ROW_HEIGHT_PX, TIMELINE_HEADER_HEIGHT_PX, TIMELINE_INDICATOR_LINE_WIDTH_PX,
    TIMELINE_PLAYHEAD_HANDLE_PX, TIMELINE_PLAYHEAD_HANDLE_TOP_PX, TIMELINE_RULER_HEIGHT_PX,
    TIMELINE_TOOLBAR_HEIGHT_PX, TIMELINE_TRACK_GAP_PX, TIMELINE_TRACK_LABELS_COLUMN_WIDTH_PX,
};

pub const ASSET_TABS: &[(&str, &str)] = &[
    ("media", "folder03"),
    ("sounds", "headphones"),
    ("text", "text"),
    ("stickers", "happy01"),
    ("effects", "magic-wand05"),
    ("transitions", "arrow-right-double"),
    ("captions", "closed-caption"),
    ("speech", "mic01"),
    ("adjustment", "sliders-horizontal"),
    ("templates", "grid-view"),
    ("settings", "more-horizontal"),
];

pub fn tab_label_key(key: &str) -> String {
    match key {
        "settings" => "editor.tab.misc".to_string(),
        other => format!("editor.tab.{other}"),
    }
}

fn tab_scroll_button(colors: Palette, glyph: &'static str, hover: f32) -> Div {
    div()
        .size(px(20.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(rem(RADIUS_SM))
        .border_1()
        .border_color(mix(colors.border, colors.foreground, hover))
        .bg(mix(colors.background, colors.accent, 0.35 + 0.65 * hover))
        .shadow_sm()
        .child(svg().size(px(14.0)).path(icon(glyph)).text_color(mix(
            colors.muted_foreground,
            colors.foreground,
            hover,
        )))
}

pub const TIMELINE_TOOLBAR_LEFT: &[(&str, &str, &str)] = &[
    ("split", "scissor", "timeline.splitElement"),
    ("align-start", "align-left", "timeline.splitLeft"),
    ("align-end", "align-right", "timeline.splitRight"),
    ("link", "link02", "timeline.extractAudio"),
    ("duplicate", "copy01", "timeline.duplicateElement"),
    ("freeze", "snow", "timeline.freezeFrameSoon"),
    ("delete", "delete02", "timeline.deleteElement"),
    ("bookmark", "bookmark02", "timeline.bookmark.add"),
    ("graph", "chart03", "timeline.graph.open"),
];

pub const TIMELINE_TOOLBAR_SEPARATOR_AFTER: usize = 7;

pub const TIMELINE_TOOLBAR_DISABLED: &[&str] = &["freeze"];

const GRAPH_PANE_HEIGHT_PX: f32 = 240.0;
const GRAPH_PANE_HEADER_PX: f32 = 34.0;
const GRAPH_PANE_LIST_WIDTH_PX: f32 = 150.0;
const GRAPH_PLOT_PAD_PX: f32 = 24.0;
const GRAPH_KEY_RADIUS_PX: f32 = 4.0;
const GRAPH_CURVE_SAMPLES: usize = 48;

const GRAPH_PLOT_HEIGHT_PX: f32 = GRAPH_PANE_HEIGHT_PX - GRAPH_PANE_HEADER_PX;

const PANEL_VIEW_HEADER_HEIGHT: f32 = 44.0;
const TAB_SCROLL_ARROW_WIDTH_PX: f32 = 28.0;
const ZOOM_MENU_ITEM_HEIGHT_PX: f32 = crate::components::MENU_ITEM_HEIGHT_PX;
const ZOOM_MENU_WIDTH_PX: f32 = 128.0;
const SCENE_MENU_WIDTH_PX: f32 = 176.0;

const SCENE_MENU_SEPARATOR_PX: f32 = 9.0;
const CONTEXT_MENU_WIDTH_PX: f32 = 184.0;

const SCENE_ACTIONS: &[(&str, &str)] = &[
    ("scene-new", "scenes.add"),
    ("scene-rename", "common.rename"),
    ("scene-delete", "common.delete"),
];

const BOOKMARK_MARKER_WIDTH_PX: f32 = 12.0;
const BOOKMARK_MARKER_HEIGHT_PX: f32 = 15.0;
const DEFAULT_BOOKMARK_COLOR: &str = "#009dff";

pub const PREVIEW_ZOOM_PRESETS: &[u32] = &[25, 50, 75, 100, 150, 200];

const PUBLISH_PREVIEW_WIDTH: u32 = 640;

const YOUTUBE_UPLOAD_SLOTS: usize = 3;

const PREVIEW_CONTROLS_LINGER: Duration = Duration::from_secs(2);

pub const PREVIEW_VOLUME_STEP: f32 = 0.05;
const MONO_FONT: &str = "Consolas";

pub trait Hoverable: 'static {
    fn transitions(&mut self) -> &mut Transitions;
    fn tooltips(&mut self) -> &mut Tooltips;
}

fn hover_listener<V: Hoverable + Render>(
    id: &'static str,
    cx: &mut Context<V>,
) -> Box<dyn Fn(&bool, &mut Window, &mut App) + 'static> {
    Box::new(cx.listener(move |this: &mut V, hovered: &bool, _, cx| {
        this.transitions().set(id, *hovered);
        this.tooltips().hover(id, *hovered);
        cx.notify();
    }))
}

fn press_listener<V: Hoverable + Render>(
    cx: &mut Context<V>,
) -> Box<dyn Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static> {
    Box::new(cx.listener(move |this: &mut V, _, _, cx| {
        this.tooltips().dismiss();
        cx.notify();
    }))
}

fn panel_frame(colors: Palette) -> Div {
    div()
        .flex()
        .flex_col()
        .size_full()
        .overflow_hidden()
        .rounded(rem(RADIUS_SM))
        .border_1()
        .border_color(colors.border)
        .bg(colors.background)
        .text_color(colors.foreground)
}

fn ghost_button(
    id: &'static str,
    name: &'static str,
    colors: Palette,
    progress: f32,
) -> Stateful<Div> {
    Button::new(id, colors)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Icon)
        .hover(progress)
        .icon(name)
        .build()
}

fn text_button(
    id: &'static str,
    name: &'static str,
    colors: Palette,
    progress: f32,
) -> Stateful<Div> {
    Button::new(id, colors)
        .variant(ButtonVariant::Text)
        .hover(progress)
        .icon(name)
        .build()
        .size(px(28.0))
}

pub const ASSET_VIEW_ICONS: &[(&str, &str)] = &[
    ("sounds", "headphones"),
    ("text", "text"),
    ("stickers", "happy01"),
    ("effects", "magic-wand05"),
    ("transitions", "arrow-right-double"),
    ("captions", "closed-caption"),
    ("speech", "mic01"),
    ("adjustment", "sliders-horizontal"),
    ("templates", "grid-view"),
    ("settings", "settings05"),
];

#[derive(Clone, Render)]
pub struct AssetSliderDrag {
    key: String,
}

pub const TRANSCRIPTION_LANGUAGES: &[(&str, &str)] = &[
    ("", "transcription.language.auto"),
    ("en", "transcription.language.en"),
    ("es", "transcription.language.es"),
    ("it", "transcription.language.it"),
    ("fr", "transcription.language.fr"),
    ("de", "transcription.language.de"),
    ("pt", "transcription.language.pt"),
    ("ru", "transcription.language.ru"),
    ("ja", "transcription.language.ja"),
    ("zh", "transcription.language.zh"),
];

pub const CAPTION_STYLES: &[&str] = &[
    "none",
    "subtitle",
    "subtitle-box",
    "bold-outline",
    "drop-shadow",
    "neon-glow",
    "boxed",
    "highlight",
    "gradient",
    "retro",
    "title",
];

fn caption_style_label(id: &str) -> String {
    if id == "none" {
        return t("common.none");
    }
    crate::text::presets()
        .iter()
        .find(|preset| preset.id == id)
        .map(|preset| t(preset.name_key))
        .unwrap_or_else(|| id.to_owned())
}

pub const SETTINGS_MODAL_WIDTH_PX: f32 = 560.0;

pub const MISC_TABS: &[(&str, &str)] = &[
    ("project-info", "settings.tab.projectInfo"),
    ("background", "settings.tab.background"),
    ("watermark", "settings.tab.watermark"),
    ("attributions", "settings.tab.attributions"),
];

pub const APP_SETTINGS_TABS: &[(&str, &str)] = &[
    ("app", "settings.tab.app"),
    ("youtube", "settings.tab.youtube"),
];

pub struct AssetsPanel {
    app: Entity<AppModel>,
    tooltips: Tooltips,
    tabs_scroll: ScrollHandle,

    revealed_tab: Option<usize>,
    body_scroll: ScrollHandle,
    active: usize,
    grid_view: bool,
    sort_descending: bool,
    transitions: Transitions,
    hsl_band: usize,

    misc_tab: usize,

    settings_tab: usize,
    settings_open: bool,
    youtube: crate::youtube_ui::Youtube,

    youtube_refreshed: bool,

    youtube_watching: bool,

    youtube_resumed: bool,
    sticker_catalogue: Vec<crate::stickers_ui::StickerEntry>,
    sticker_catalogue_locale: String,
    sticker_previews: crate::stickers_ui::ShapePreviews,
    sticker_search: crate::input::TextField,
    sticker_category: usize,
    user_templates: Vec<crate::templates_ui::TemplateEntry>,
    template_notice: Option<String>,
    caption_notice: Option<String>,
    caption_style: usize,
    caption_model: usize,
    caption_language: usize,
    speech_text: crate::input::TextField,
    speech_voice: usize,
    speech_notice: Option<String>,
    speech_sample: Option<(Vec<f32>, u32)>,
    sound_search: crate::input::TextField,
    sound_mode: usize,
    sound_results: Vec<sounds::SoundResult>,
    sound_saved: crate::sounds_ui::SavedSounds,
    sound_album: Option<(String, String)>,
    sound_notice: Option<String>,
    sound_loading: bool,
    sound_playing: Option<String>,
    audio_preview: Option<cutix_playback::AudioOutput>,

    publish_preview: Option<crate::preview::FrameWorker>,
    publish_speaker: crate::preview_audio::Speaker,

    publish_bar_bounds: (f32, f32),
    job: Option<crate::ai::Job>,

    rasterizer: Option<cutix_playback::TextRasterizer>,
}

impl AssetsPanel {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self {
            app,
            tooltips: Tooltips::new(TOOLTIP_DELAY),
            tabs_scroll: ScrollHandle::new(),
            revealed_tab: None,
            body_scroll: ScrollHandle::new(),
            active: 0,
            grid_view: true,
            sort_descending: false,
            transitions: Transitions::new(),
            hsl_band: 0,
            misc_tab: 0,
            settings_tab: 0,
            settings_open: false,
            youtube: crate::youtube_ui::Youtube::default(),
            youtube_refreshed: false,
            youtube_watching: false,
            youtube_resumed: false,
            sticker_catalogue: crate::stickers_ui::catalogue(),
            sticker_catalogue_locale: cutix_i18n::locale(),
            sticker_previews: crate::stickers_ui::ShapePreviews::default(),
            sticker_search: crate::input::TextField::new(cx, ""),
            sticker_category: 0,
            user_templates: crate::templates_ui::user_entries(),
            template_notice: None,
            caption_notice: None,
            caption_style: CAPTION_STYLES
                .iter()
                .position(|id| *id == "subtitle")
                .unwrap_or(0),
            caption_model: ml::WHISPER_MODELS
                .iter()
                .position(|model| model.key == ml::DEFAULT_WHISPER_MODEL)
                .unwrap_or(0),
            caption_language: 0,
            speech_text: crate::input::TextField::new(cx, ""),
            speech_voice: 0,
            speech_notice: None,
            speech_sample: None,
            sound_search: crate::input::TextField::new(cx, ""),
            sound_mode: crate::sounds_ui::MODE_MUSIC,
            sound_results: Vec::new(),
            sound_saved: crate::sounds_ui::load_saved(),
            sound_album: None,
            sound_notice: None,
            sound_loading: false,
            sound_playing: None,
            audio_preview: None,
            publish_preview: None,
            publish_speaker: crate::preview_audio::Speaker::default(),
            publish_bar_bounds: (0.0, 0.0),
            job: None,
            rasterizer: None,
        }
    }

    fn colors(&self, cx: &App) -> Palette {
        self.app.read(cx).theme.panel
    }

    fn sorted_media(&self, cx: &App) -> Vec<MediaAssetData> {
        let mut media = self.app.read(cx).media.clone();
        media.sort_by(|left, right| {
            let ordering = left.name.to_lowercase().cmp(&right.name.to_lowercase());
            if self.sort_descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
        media
    }

    fn pick_files(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let paths =
                crate::dialogs::open_files(crate::dialogs::Filter::Media, t("common.import"), true)
                    .await;
            if paths.is_empty() {
                return;
            }
            let _ = this.update(cx, |this, cx| {
                this.app
                    .update(cx, |model, cx| model.import_media(paths, cx));
            });
        })
        .detach();
    }

    fn tab_overflow(&self) -> (bool, bool) {
        let offset = -f32::from(self.tabs_scroll.offset().x);
        let max = f32::from(self.tabs_scroll.max_offset().width);
        (offset > 0.5, offset < max - 1.0)
    }

    fn nudge_tabs(&mut self, direction: f32) {
        let width = f32::from(self.tabs_scroll.bounds().size.width);
        let step = (width * 0.7).max(120.0) * direction;
        let max = f32::from(self.tabs_scroll.max_offset().width);
        let next = (-f32::from(self.tabs_scroll.offset().x) + step).clamp(0.0, max.max(0.0));
        self.tabs_scroll
            .set_offset(gpui::point(px(-next), self.tabs_scroll.offset().y));
    }

    fn reveal_tab(&mut self, index: usize) {
        self.revealed_tab = Some(index);
        self.tabs_scroll.scroll_to_item(index);
    }

    fn keep_active_tab_visible(&mut self) {
        if self.revealed_tab == Some(self.active) {
            return;
        }
        self.tabs_scroll.scroll_to_item(self.active);

        if f32::from(self.tabs_scroll.max_offset().width) > 0.0 {
            self.revealed_tab = Some(self.active);
        }
    }

    fn media_card(&mut self, asset: &MediaAssetData, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        let thumbnail = self.app.read(cx).thumbnail_path(asset);
        let duration = asset.duration.and_then(format_seconds);
        let glyph = media_glyph(asset.media_type);
        let missing = self.app.read(cx).is_media_missing(&asset.id);
        let name_color = if missing {
            colors.destructive
        } else {
            colors.foreground
        };
        let missing_badge = move || {
            div()
                .absolute()
                .top(px(4.0))
                .left(px(4.0))
                .flex()
                .items_center()
                .gap(px(3.0))
                .px(px(4.0))
                .rounded(px(3.0))
                .bg(colors.destructive)
                .text_size(px(9.0))
                .text_color(colors.destructive_foreground)
                .child(
                    svg()
                        .path(icon("alert-circle"))
                        .size(px(9.0))
                        .flex_none()
                        .text_color(colors.destructive_foreground),
                )
                .child(t("media.missing"))
        };

        let art = |size: f32| -> gpui::AnyElement {
            match thumbnail.clone() {
                Some(path) => img(path).size_full().into_any_element(),
                None => svg()
                    .size(px(size))
                    .path(icon(glyph))
                    .text_color(colors.muted_foreground)
                    .into_any_element(),
            }
        };

        let media_id = asset.id.clone();
        let draggable = move |element: Stateful<Div>| {
            element.on_drag(
                MediaDrag {
                    media_id: media_id.clone(),
                },
                |_, _, _, cx| cx.new(|_| gpui::Empty),
            )
        };

        if self.grid_view {
            return div()
                .w(relative(1.0 / 3.0))
                .flex_shrink_0()
                .p(px(4.0))
                .child(draggable(
                    div()
                        .id(SharedString::from(format!("media-{}", asset.id)))
                        .flex()
                        .flex_col()
                        .w_full()
                        .gap(px(4.0))
                        .child(
                            div()
                                .relative()
                                .w_full()
                                .h(px(0.0))
                                .pb(relative(9.0 / 16.0))
                                .rounded(rem(RADIUS_SM))
                                .overflow_hidden()
                                .bg(opacity(colors.foreground, 0.06))
                                .child(
                                    div()
                                        .absolute()
                                        .top_0()
                                        .left_0()
                                        .size_full()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(art(24.0)),
                                )
                                .when_some(duration.clone(), |this, label| {
                                    this.child(
                                        div()
                                            .absolute()
                                            .bottom(px(4.0))
                                            .right(px(4.0))
                                            .px(px(4.0))
                                            .rounded(px(3.0))
                                            .bg(opacity(gpui::black(), 0.65))
                                            .text_size(px(10.0))
                                            .text_color(gpui::white())
                                            .child(label),
                                    )
                                })
                                .when(missing, |this| this.child(missing_badge())),
                        )
                        .child(
                            div()
                                .w_full()
                                .truncate()
                                .text_size(rem(TEXT_XS))
                                .text_color(name_color)
                                .child(asset.name.clone()),
                        ),
                ));
        }

        div().w_full().flex_shrink_0().child(draggable(
            div()
                .id(SharedString::from(format!("media-{}", asset.id)))
                .flex()
                .w_full()
                .h(px(44.0))
                .flex_shrink_0()
                .items_center()
                .gap(px(10.0))
                .px(px(8.0))
                .child(
                    div()
                        .flex()
                        .size(px(32.0))
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
                        .rounded(rem(RADIUS_SM))
                        .overflow_hidden()
                        .bg(opacity(colors.foreground, 0.06))
                        .child(art(16.0)),
                )
                .when(missing, |this| {
                    this.child(
                        svg()
                            .path(icon("alert-circle"))
                            .size(px(13.0))
                            .flex_none()
                            .text_color(colors.destructive),
                    )
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(rem(TEXT_SM))
                        .text_color(name_color)
                        .child(asset.name.clone()),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(rem(TEXT_XS))
                        .text_color(if missing {
                            colors.destructive
                        } else {
                            colors.muted_foreground
                        })
                        .child(if missing {
                            t("media.missing")
                        } else {
                            duration.unwrap_or_else(|| t("common.unknown"))
                        }),
                ),
        ))
    }

    fn dropzone(&mut self, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        let importing = self.app.read(cx).importing;
        let notice = self.app.read(cx).notice.clone();

        div().flex().flex_1().w_full().min_h_0().p(px(8.0)).child(
            div()
                .id("assets-dropzone")
                .flex()
                .flex_col()
                .size_full()
                .items_center()
                .justify_center()
                .gap(px(16.0))
                .p(px(32.0))
                .rounded(rem(RADIUS_LG))
                .cursor_pointer()
                .bg(opacity(
                    colors.foreground,
                    0.05 + 0.05 * self.transitions.eased("assets-dropzone"),
                ))
                .on_hover(hover_listener("assets-dropzone", cx))
                .on_click(cx.listener(|this: &mut Self, _, _, cx| this.pick_files(cx)))
                .on_drop(
                    cx.listener(|this: &mut Self, paths: &ExternalPaths, _, cx| {
                        let paths: Vec<PathBuf> = paths.paths().to_vec();
                        this.app
                            .update(cx, |model, cx| model.import_media(paths, cx));
                    }),
                )
                .child(
                    svg()
                        .size(px(40.0))
                        .path(icon("upload04"))
                        .text_color(colors.foreground),
                )
                .child(
                    div()
                        .text_size(rem(TEXT_XS))
                        .text_color(colors.muted_foreground)
                        .text_center()
                        .child(if importing > 0 {
                            t("common.loading")
                        } else {
                            t("editor.assets.dropzone")
                        }),
                )
                .when_some(notice, |this, message| {
                    this.child(
                        div()
                            .text_size(rem(TEXT_XS))
                            .text_color(colors.destructive)
                            .text_center()
                            .child(message),
                    )
                }),
        )
    }

    fn text_presets(&mut self, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        let bar = scrollbar_v(&self.body_scroll, colors);
        let cards = crate::text::presets()
            .into_iter()
            .map(|preset| {
                let key = format!("text-preset-{}", preset.id);
                let progress = self.transitions.eased(&key);
                let hover_key = key.clone();
                let preset_id = preset.id;
                let name = t(preset.name_key);
                let swatch = crate::theme::parse_hex(preset.color.trim_start_matches('#'));

                div()
                    .w(relative(1.0 / 3.0))
                    .flex_shrink_0()
                    .p(px(4.0))
                    .child(
                        div()
                            .id(SharedString::from(key))
                            .flex()
                            .flex_col()
                            .w_full()
                            .gap(px(4.0))
                            .cursor_pointer()
                            .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                                this.transitions.set(hover_key.clone(), *hovered);
                                cx.notify();
                            }))
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.insert_text_preset(preset_id, cx);
                            }))
                            .on_drag(InsertDrag::TextPreset(preset_id), |_, _, _, cx| {
                                cx.new(|_| gpui::Empty)
                            })
                            .child(
                                div()
                                    .relative()
                                    .w_full()
                                    .h(px(0.0))
                                    .pb(relative(1.0))
                                    .rounded(rem(RADIUS_SM))
                                    .overflow_hidden()
                                    .border_1()
                                    .border_color(mix(colors.border, colors.primary, progress))
                                    .bg(opacity(colors.foreground, 0.06))
                                    .child(
                                        div()
                                            .absolute()
                                            .top_0()
                                            .left_0()
                                            .size_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_size(rem(TEXT_LG))
                                            .text_color(swatch)
                                            .when(preset.bold, |this| {
                                                this.font_weight(FontWeight::BOLD)
                                            })
                                            .child(t("text.sample")),
                                    ),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .truncate()
                                    .text_size(rem(TEXT_XS))
                                    .child(name),
                            ),
                    )
            })
            .collect::<Vec<_>>();

        div()
            .relative()
            .flex()
            .flex_1()
            .w_full()
            .min_h_0()
            .child(
                div()
                    .id("assets-text")
                    .flex()
                    .flex_col()
                    .size_full()
                    .p(px(4.0))
                    .overflow_y_scroll()
                    .track_scroll(&self.body_scroll)
                    .child(div().flex().w_full().flex_wrap().children(cards)),
            )
            .children(bar)
    }

    fn sticker_card(
        &mut self,
        entry: &crate::stickers_ui::StickerEntry,
        cx: &mut Context<Self>,
    ) -> Div {
        let colors = self.colors(cx);
        let key = format!("sticker-{}", entry.sticker_id);
        let progress = self.transitions.eased(&key);
        let hover_key = key.clone();
        let sticker_id = entry.sticker_id.clone();
        let name = entry.name.clone();
        let label = entry.name.clone();
        let drag = InsertDrag::Sticker {
            sticker_id: entry.sticker_id.clone(),
            name: entry.name.clone(),
        };

        let art: gpui::AnyElement = match crate::stickers_ui::flag_asset_path(&entry.sticker_id) {
            Some(path) => img(path).size_full().into_any_element(),
            None => match self.sticker_previews.get(&entry.sticker_id) {
                Some(image) => img(image).size_full().into_any_element(),
                None => div().size_full().into_any_element(),
            },
        };

        div()
            .w(relative(1.0 / 3.0))
            .flex_shrink_0()
            .p(px(4.0))
            .child(
                div()
                    .id(SharedString::from(key))
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap(px(4.0))
                    .cursor_pointer()
                    .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                        this.transitions.set(hover_key.clone(), *hovered);
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.insert_sticker(&sticker_id, &name, cx);
                    }))
                    .on_drag(drag, |_, _, _, cx| cx.new(|_| gpui::Empty))
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(0.0))
                            .pb(relative(1.0))
                            .rounded(rem(RADIUS_SM))
                            .overflow_hidden()
                            .border_1()
                            .border_color(mix(colors.border, colors.primary, progress))
                            .bg(opacity(colors.foreground, 0.06))
                            .child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .size_full()
                                    .p(px(10.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(art),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_size(rem(TEXT_XS))
                            .child(label),
                    ),
            )
    }

    fn run_sound_search(&mut self, cx: &mut Context<Self>) {
        if self.sound_loading {
            return;
        }
        self.sound_album = None;
        self.sound_notice = None;

        let query = self.sound_search.text().to_string();
        let effects = self.sound_mode == crate::sounds_ui::MODE_EFFECTS;
        let key = crate::sounds_ui::freesound_key();

        self.sound_loading = true;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn(async move {
                    if effects {
                        sounds::search_effects(key.as_deref(), &query, 1, sounds::DEFAULT_PAGE_SIZE)
                    } else {
                        sounds::search_songs(&query, 1, sounds::DEFAULT_PAGE_SIZE)
                    }
                    .map_err(|error| error.to_string())
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.sound_loading = false;
                match outcome {
                    Ok(page) if page.results.is_empty() => {
                        this.sound_results.clear();
                        this.sound_notice = Some(t("sounds.search.empty"));
                    }
                    Ok(page) => {
                        this.sound_results = page.results;
                        this.sound_notice = None;
                    }
                    Err(message) => {
                        this.sound_results.clear();
                        this.sound_notice = Some(message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_sound_album(&mut self, result: &sounds::SoundResult, cx: &mut Context<Self>) {
        let Some(identifier) = result.archive_identifier.clone() else {
            return;
        };
        if self.sound_loading {
            return;
        }
        self.sound_loading = true;
        self.sound_notice = None;
        self.sound_album = Some((identifier.clone(), result.name.clone()));
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn(async move {
                    sounds::archive::album_tracks(&identifier).map_err(|error| error.to_string())
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.sound_loading = false;
                match outcome {
                    Ok(tracks) if tracks.is_empty() => {
                        this.sound_results.clear();
                        this.sound_notice = Some(t("sounds.search.empty"));
                    }
                    Ok(tracks) => {
                        this.sound_results = tracks;
                        this.sound_notice = None;
                    }
                    Err(message) => {
                        this.sound_results.clear();
                        this.sound_notice = Some(message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn preview_sound(&mut self, result: &sounds::SoundResult, cx: &mut Context<Self>) {
        if let Some(output) = self.audio_preview.take() {
            let _ = output.stop();
            let was_playing = self.sound_playing.take();
            if was_playing.as_deref() == Some(result.id.as_str()) {
                cx.notify();
                return;
            }
        }

        let Some(url) = result.playable_url().map(str::to_string) else {
            return;
        };
        if self.sound_loading {
            return;
        }

        let stem = sounds::http::sanitise_stem(&result.name);
        let id = result.id.clone();
        self.sound_loading = true;
        self.sound_notice = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn(async move {
                    let directory = std::env::temp_dir().join("cutix-sounds");
                    let path = sounds::http::download_audio(&url, &directory, &stem)
                        .map_err(|error| error.to_string())?;
                    let pcm =
                        cutix_playback::decode_audio(&path).map_err(|error| error.to_string())?;
                    Ok::<_, String>((pcm.channel(0).to_vec(), pcm.sample_rate, path))
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.sound_loading = false;
                match outcome {
                    Ok((samples, rate, _)) => match cutix_playback::AudioOutput::open() {
                        Ok(output) => {
                            let resampled =
                                crate::ai::resample_linear(&samples, rate, output.sample_rate());
                            output.queue_samples(&crate::ai::interleave(
                                &resampled,
                                output.channels(),
                            ));
                            output.start();
                            this.audio_preview = Some(output);
                            this.sound_playing = Some(id);
                        }
                        Err(error) => this.sound_notice = Some(error.to_string()),
                    },
                    Err(message) => this.sound_notice = Some(message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn add_sound_to_timeline(&mut self, result: &sounds::SoundResult, cx: &mut Context<Self>) {
        let Some(url) = result
            .download_url
            .clone()
            .or_else(|| result.preview_url.clone())
        else {
            return;
        };
        if self.sound_loading {
            return;
        }

        let stem = sounds::http::sanitise_stem(&result.name);
        let attribution =
            crate::sounds_ui::attribution_for(result, cutix_project::store::now_iso());

        self.sound_loading = true;
        self.sound_notice = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn(async move {
                    let directory = std::env::temp_dir().join("cutix-sounds");
                    sounds::http::download_audio(&url, &directory, &stem)
                        .map_err(|error| error.to_string())
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.sound_loading = false;
                match outcome {
                    Ok(path) => {
                        this.app.update(cx, |model, cx| {
                            model.record_attribution(attribution, cx);
                            model.import_media_at_playhead(path, cx);
                        });
                        this.sound_notice = Some(t("sounds.addToTimeline"));
                    }
                    Err(message) => this.sound_notice = Some(message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn sound_row(&mut self, index: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let colors = self.colors(cx);
        let result = self.sound_results[index].clone();
        let playing = self.sound_playing.as_deref() == Some(result.id.as_str());
        let saved = self.sound_saved.contains(&result.id);
        let row_id = SharedString::from(format!("sound-{index}"));

        let mut actions = div().flex().items_center().gap(px(4.0));

        if result.is_album {
            actions = actions.child(
                Button::new(format!("{row_id}-open"), colors)
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .icon("folder03")
                    .hover(self.transitions.eased(&format!("{row_id}-open")))
                    .build()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        let result = this.sound_results[index].clone();
                        this.open_sound_album(&result, cx);
                    })),
            );
        } else {
            actions = actions
                .child(
                    Button::new(format!("{row_id}-play"), colors)
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .icon(if playing { "pause" } else { "play" })
                        .hover(self.transitions.eased(&format!("{row_id}-play")))
                        .build()
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            let result = this.sound_results[index].clone();
                            this.preview_sound(&result, cx);
                        })),
                )
                .child(
                    Button::new(format!("{row_id}-save"), colors)
                        .variant(if saved {
                            ButtonVariant::Secondary
                        } else {
                            ButtonVariant::Ghost
                        })
                        .size(ButtonSize::Sm)
                        .icon("bookmark02")
                        .hover(self.transitions.eased(&format!("{row_id}-save")))
                        .build()
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            let result = this.sound_results[index].clone();
                            this.sound_saved.toggle(&result);
                            crate::sounds_ui::save_saved(&this.sound_saved);
                            cx.notify();
                        })),
                )
                .child(
                    Button::new(format!("{row_id}-add"), colors)
                        .variant(ButtonVariant::Ghost)
                        .size(ButtonSize::Sm)
                        .icon("plus-sign")
                        .hover(self.transitions.eased(&format!("{row_id}-add")))
                        .build()
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            let result = this.sound_results[index].clone();
                            this.add_sound_to_timeline(&result, cx);
                        })),
                );
        }

        let mut meta = crate::sounds_ui::credit_line(&result);
        if let Some(duration) = crate::sounds_ui::duration_label(&result) {
            meta = format!("{duration} · {meta}");
        }

        div()
            .id(row_id)
            .flex()
            .w_full()
            .items_center()
            .gap(px(8.0))
            .px(px(8.0))
            .py(px(6.0))
            .rounded(rem(RADIUS_SM))
            .hover(|this| this.bg(opacity(colors.foreground, 0.06)))
            .child(
                svg()
                    .size(px(16.0))
                    .flex_shrink_0()
                    .path(icon(crate::sounds_ui::source_glyph(result.source)))
                    .text_color(opacity(colors.muted_foreground, 0.9)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_size(rem(TEXT_SM))
                            .text_color(if playing {
                                colors.primary
                            } else {
                                colors.foreground
                            })
                            .child(result.name.clone()),
                    )
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .text_size(rem(TEXT_XS))
                            .text_color(colors.muted_foreground)
                            .child(format!("{} · {meta}", result.username)),
                    ),
            )
            .child(actions)
    }

    fn sounds_body(&mut self, window: &Window, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        let saved_mode = self.sound_mode == crate::sounds_ui::MODE_SAVED;

        if saved_mode {
            self.sound_results = self.sound_saved.entries.clone();
        }

        let search = crate::input::text_field(
            "sounds-search",
            &self.sound_search,
            colors,
            crate::input::FieldStyle {
                height: 32.0,
                placeholder: SharedString::from(t("editor.sounds.search")),
                leading: Some(icon("search01")),
                ..Default::default()
            },
            window,
        )
        .on_key_down(
            cx.listener(|this: &mut Self, event: &gpui::KeyDownEvent, _, cx| {
                match this.sound_search.buffer.key_down(event) {
                    crate::input::TextEvent::Cancel => this.sound_search.buffer.set(""),
                    crate::input::TextEvent::Submit => this.run_sound_search(cx),
                    _ => {}
                }
                cx.notify();
            }),
        );

        let tabs = crate::sounds_ui::MODES
            .iter()
            .enumerate()
            .map(|(index, (id, label_key))| {
                let active = index == self.sound_mode;
                div()
                    .id(SharedString::from(format!("sound-mode-{id}")))
                    .px(px(10.0))
                    .h(px(26.0))
                    .flex()
                    .items_center()
                    .rounded(rem(RADIUS_SM))
                    .cursor_pointer()
                    .text_size(rem(TEXT_XS))
                    .when(active, |this| {
                        this.bg(opacity(colors.foreground, 0.1))
                            .text_color(colors.foreground)
                    })
                    .when(!active, |this| this.text_color(colors.muted_foreground))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.sound_mode = index;
                        this.sound_album = None;
                        this.sound_notice = None;
                        if index == crate::sounds_ui::MODE_SAVED {
                            this.sound_results = this.sound_saved.entries.clone();
                        } else {
                            this.sound_results.clear();
                            this.run_sound_search(cx);
                        }
                        cx.notify();
                    }))
                    .child(t(label_key))
            })
            .collect::<Vec<_>>();

        let album = self.sound_album.clone();
        let notice = self.sound_notice.clone();
        let loading = self.sound_loading;
        let rows = (0..self.sound_results.len())
            .map(|index| self.sound_row(index, cx))
            .collect::<Vec<_>>();
        let empty = rows.is_empty();
        let bar = scrollbar_v(&self.body_scroll, colors);

        div()
            .flex()
            .flex_col()
            .flex_1()
            .w_full()
            .min_h_0()
            .when(!saved_mode, |this| {
                this.child(
                    div()
                        .w_full()
                        .flex_shrink_0()
                        .px(px(8.0))
                        .pt(px(8.0))
                        .child(search),
                )
            })
            .child(
                div()
                    .flex()
                    .w_full()
                    .flex_shrink_0()
                    .gap(px(4.0))
                    .px(px(8.0))
                    .py(px(8.0))
                    .children(tabs),
            )
            .when_some(album, |this, (_, name)| {
                this.child(
                    div()
                        .flex()
                        .w_full()
                        .items_center()
                        .gap(px(6.0))
                        .flex_shrink_0()
                        .px(px(8.0))
                        .pb(px(6.0))
                        .child(
                            Button::new("sound-album-back", colors)
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .icon("chevron-left")
                                .hover(self.transitions.eased("sound-album-back"))
                                .build()
                                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                    this.sound_album = None;
                                    this.run_sound_search(cx);
                                })),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(rem(TEXT_XS))
                                .text_color(colors.muted_foreground)
                                .child(name),
                        ),
                )
            })
            .when(loading || notice.is_some(), |this| {
                this.child(
                    div()
                        .w_full()
                        .flex_shrink_0()
                        .px(px(12.0))
                        .pb(px(6.0))
                        .text_size(rem(TEXT_XS))
                        .text_color(colors.muted_foreground)
                        .child(if loading {
                            t("sounds.loading")
                        } else {
                            notice.unwrap_or_default()
                        }),
                )
            })
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .child(
                        div()
                            .id("assets-sounds")
                            .flex()
                            .flex_col()
                            .size_full()
                            .px(px(4.0))
                            .overflow_y_scroll()
                            .track_scroll(&self.body_scroll)
                            .when(empty && !loading, |this| {
                                this.items_center()
                                    .justify_center()
                                    .text_size(rem(TEXT_SM))
                                    .text_color(colors.muted_foreground)
                                    .child(if saved_mode {
                                        t("sounds.empty")
                                    } else {
                                        t("sounds.none")
                                    })
                            })
                            .when(!empty, |this| this.children(rows)),
                    )
                    .children(bar),
            )
    }

    fn stickers_body(&mut self, window: &Window, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        let locale = self.app.read(cx).locale.clone();
        if locale != self.sticker_catalogue_locale {
            self.sticker_catalogue = crate::stickers_ui::catalogue();
            self.sticker_catalogue_locale = locale;
        }
        let category = crate::stickers_ui::CATEGORIES[self.sticker_category].0;
        let entries = crate::stickers_ui::filtered(
            &self.sticker_catalogue,
            category,
            self.sticker_search.text(),
        );

        let search = crate::input::text_field(
            "stickers-search",
            &self.sticker_search,
            colors,
            crate::input::FieldStyle {
                height: 32.0,
                placeholder: SharedString::from(t("editor.stickers.search")),
                leading: Some(icon("search01")),
                ..Default::default()
            },
            window,
        )
        .on_key_down(
            cx.listener(|this: &mut Self, event: &gpui::KeyDownEvent, _, cx| {
                match this.sticker_search.buffer.key_down(event) {
                    crate::input::TextEvent::Cancel => this.sticker_search.buffer.set(""),
                    _ => {}
                }
                cx.notify();
            }),
        );

        let tabs = crate::stickers_ui::CATEGORIES
            .iter()
            .enumerate()
            .map(|(index, (id, label_key))| {
                let active = index == self.sticker_category;
                div()
                    .id(SharedString::from(format!("sticker-category-{id}")))
                    .px(px(10.0))
                    .h(px(26.0))
                    .flex()
                    .items_center()
                    .rounded(rem(RADIUS_SM))
                    .cursor_pointer()
                    .text_size(rem(TEXT_XS))
                    .when(active, |this| {
                        this.bg(opacity(colors.foreground, 0.1))
                            .text_color(colors.foreground)
                    })
                    .when(!active, |this| this.text_color(colors.muted_foreground))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.sticker_category = index;
                        cx.notify();
                    }))
                    .child(t(label_key))
            })
            .collect::<Vec<_>>();

        let empty = entries.is_empty();
        let cards = entries
            .iter()
            .map(|entry| self.sticker_card(entry, cx))
            .collect::<Vec<_>>();
        let bar = scrollbar_v(&self.body_scroll, colors);

        div()
            .flex()
            .flex_col()
            .flex_1()
            .w_full()
            .min_h_0()
            .child(
                div()
                    .w_full()
                    .flex_shrink_0()
                    .px(px(8.0))
                    .pt(px(8.0))
                    .child(search),
            )
            .child(
                div()
                    .flex()
                    .w_full()
                    .flex_shrink_0()
                    .gap(px(4.0))
                    .px(px(8.0))
                    .py(px(8.0))
                    .children(tabs),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .child(
                        div()
                            .id("assets-stickers")
                            .flex()
                            .flex_col()
                            .size_full()
                            .p(px(4.0))
                            .overflow_y_scroll()
                            .track_scroll(&self.body_scroll)
                            .when(empty, |this| {
                                this.items_center()
                                    .justify_center()
                                    .text_size(rem(TEXT_SM))
                                    .text_color(colors.muted_foreground)
                                    .child(t("stickers.empty"))
                            })
                            .when(!empty, |this| {
                                this.child(div().flex().w_full().flex_wrap().children(cards))
                            }),
                    )
                    .children(bar),
            )
    }

    fn chip_row(
        &mut self,
        id: &str,
        label: String,
        options: Vec<(usize, String)>,
        active: usize,
        pick: fn(&mut Self, usize),
        cx: &mut Context<Self>,
    ) -> Div {
        let colors = self.colors(cx);
        let busy = self.job.is_some();
        let chips = options
            .into_iter()
            .map(|(index, text)| {
                let selected = index == active;
                let key = format!("{id}-{index}");
                let progress = self.transitions.eased(&key);
                let hover_key = key.clone();
                div()
                    .id(SharedString::from(key))
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .h(px(24.0))
                    .px(px(9.0))
                    .rounded(rem(RADIUS_SM))
                    .border_1()
                    .border_color(if selected {
                        colors.primary
                    } else {
                        colors.border
                    })
                    .bg(if selected {
                        opacity(colors.primary, 0.16)
                    } else {
                        mix(opacity(colors.accent, 0.0), colors.accent, progress)
                    })
                    .text_size(rem(TEXT_XS))
                    .text_color(if selected {
                        colors.foreground
                    } else {
                        colors.muted_foreground
                    })
                    .when(!busy, |this| this.cursor_pointer())
                    .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                        this.transitions.set(hover_key.clone(), *hovered);
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        if this.job.is_none() {
                            pick(this, index);
                            cx.notify();
                        }
                    }))
                    .child(text)
            })
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .w_full()
            .flex_shrink_0()
            .gap(px(5.0))
            .child(
                div()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(label),
            )
            .child(
                div()
                    .flex()
                    .w_full()
                    .flex_wrap()
                    .gap(px(4.0))
                    .children(chips),
            )
    }

    fn job_row(&mut self, id: &'static str, cx: &mut Context<Self>) -> Option<Div> {
        let colors = self.colors(cx);
        let job = self.job.as_ref()?;
        let status = job.status();
        let cancelled = job.is_cancelled();

        Some(
            div()
                .flex()
                .flex_col()
                .w_full()
                .flex_shrink_0()
                .gap(px(6.0))
                .child(
                    div()
                        .flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(rem(TEXT_XS))
                                .text_color(colors.muted_foreground)
                                .child(status.message),
                        )
                        .child(
                            Button::new(id, colors)
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .hover(self.transitions.eased(id))
                                .label(t(if cancelled {
                                    "common.loading"
                                } else {
                                    "common.cancel"
                                }))
                                .build()
                                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                    if let Some(job) = this.job.as_ref() {
                                        job.request_cancel();
                                    }
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .h(px(4.0))
                        .rounded(px(2.0))
                        .bg(opacity(colors.foreground, 0.1))
                        .child(
                            div()
                                .h_full()
                                .w(relative(status.progress.clamp(0.02, 1.0)))
                                .rounded(px(2.0))
                                .bg(colors.primary),
                        ),
                ),
        )
    }

    fn captions_body(&mut self, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        let notice = self.caption_notice.clone();
        let busy = self.job.is_some();
        let bar = scrollbar_v(&self.body_scroll, colors);

        let models = self.chip_row(
            "caption-model",
            t("captions.model"),
            ml::WHISPER_MODELS
                .iter()
                .enumerate()
                .map(|(index, model)| {
                    (
                        index,
                        format!("{} · {} MB", t(model.name_key), model.approximate_size_mb),
                    )
                })
                .collect(),
            self.caption_model,
            |this, index| this.caption_model = index,
            cx,
        );
        let languages = self.chip_row(
            "caption-language",
            t("captions.language"),
            TRANSCRIPTION_LANGUAGES
                .iter()
                .enumerate()
                .map(|(index, (_, key))| (index, t(key)))
                .collect(),
            self.caption_language,
            |this, index| this.caption_language = index,
            cx,
        );
        let styles = self.chip_row(
            "caption-style",
            t("captions.style"),
            CAPTION_STYLES
                .iter()
                .enumerate()
                .map(|(index, id)| (index, caption_style_label(id)))
                .collect(),
            self.caption_style,
            |this, index| this.caption_style = index,
            cx,
        );
        let progress = self.job_row("captions-cancel", cx);

        div()
            .flex()
            .flex_col()
            .flex_1()
            .w_full()
            .min_h_0()
            .child(
                div()
                    .id("assets-captions")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .gap(px(12.0))
                    .p(px(10.0))
                    .overflow_y_scroll()
                    .track_scroll(&self.body_scroll)
                    .child(models)
                    .child(languages)
                    .child(styles)
                    .child(
                        Button::new("captions-generate", colors)
                            .variant(ButtonVariant::Default)
                            .hover(self.transitions.eased("captions-generate"))
                            .label(t("captions.generate"))
                            .build()
                            .w_full()
                            .flex_shrink_0()
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.generate_transcript(cx);
                            })),
                    )
                    .children(progress)
                    .when_some(notice, |this, message| {
                        this.child(
                            div()
                                .w_full()
                                .text_size(rem(TEXT_XS))
                                .text_color(colors.muted_foreground)
                                .child(message),
                        )
                    })
                    .child(separator_h(colors))
                    .child(self.captions_file_actions(busy, cx))
                    .child(
                        div()
                            .w_full()
                            .text_size(rem(TEXT_XS))
                            .text_color(colors.muted_foreground)
                            .child(t("captions.srt.hint")),
                    ),
            )
            .children(bar)
    }

    fn captions_file_actions(&mut self, _busy: bool, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        div()
            .flex()
            .w_full()
            .flex_shrink_0()
            .gap(px(6.0))
            .child(
                Button::new("captions-import", colors)
                    .variant(ButtonVariant::Secondary)
                    .hover(self.transitions.eased("captions-import"))
                    .label(t("captions.import.action"))
                    .build()
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.import_subtitles(cx);
                    })),
            )
            .child(
                Button::new("captions-export", colors)
                    .variant(ButtonVariant::Secondary)
                    .hover(self.transitions.eased("captions-export"))
                    .label(t("captions.export.srt"))
                    .build()
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.export_subtitles(cx);
                    })),
            )
    }

    fn caption_style(&self) -> crate::edit::CaptionStyle {
        let Some(id) = CAPTION_STYLES
            .get(self.caption_style)
            .filter(|id| **id != "none")
        else {
            return crate::edit::CaptionStyle::default();
        };
        crate::text::presets()
            .iter()
            .find(|preset| preset.id == *id)
            .map(crate::edit::CaptionStyle::from_preset)
            .unwrap_or_default()
    }

    fn import_subtitles(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let paths = crate::dialogs::open_files(
                crate::dialogs::Filter::Subtitles,
                t("dialog.subtitles.title"),
                false,
            )
            .await;
            let Some(path) = paths.first().cloned() else {
                return;
            };
            let Ok(raw) = std::fs::read_to_string(&path) else {
                let _ = this.update(cx, |this, cx| {
                    this.caption_notice = Some(t("captions.error.unexpected"));
                    cx.notify();
                });
                return;
            };
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_owned();
            let _ = this.update(cx, |this, cx| {
                this.insert_subtitles(&raw, &extension, cx);
            });
        })
        .detach();
    }

    fn insert_subtitles(&mut self, raw: &str, extension: &str, cx: &mut Context<Self>) {
        let Some(parsed) = cutix_project::parse_subtitle_file(raw, extension) else {
            self.caption_notice = Some(t("captions.error.unsupportedFormat"));
            cx.notify();
            return;
        };
        if parsed.captions.is_empty() {
            self.caption_notice = Some(t("captions.error.noCues"));
            cx.notify();
            return;
        }
        let skipped = parsed.skipped_cue_count;
        let warnings = parsed.warnings.clone();
        self.insert_caption_cues(parsed.captions, skipped, cx);
        let extra: Vec<String> = warnings
            .iter()
            .map(|warning| match warning.count() {
                Some(count) => t_args(warning.key(), &[("count", &count.to_string())]),
                None => t(warning.key()),
            })
            .collect();
        if !extra.is_empty() {
            let head = self.caption_notice.take().unwrap_or_default();
            self.caption_notice = Some(
                std::iter::once(head)
                    .filter(|line| !line.is_empty())
                    .chain(extra)
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            cx.notify();
        }
    }

    fn insert_caption_cues(
        &mut self,
        cues: Vec<cutix_project::SubtitleCue>,
        skipped: usize,
        cx: &mut Context<Self>,
    ) {
        let canvas = self
            .app
            .read(cx)
            .project
            .as_ref()
            .map(|project| {
                (
                    project.settings.canvas_size.width as f64,
                    project.settings.canvas_size.height as f64,
                )
            })
            .unwrap_or((1920.0, 1080.0));

        let style = self.caption_style();
        let rasterizer = self
            .rasterizer
            .get_or_insert_with(cutix_playback::TextRasterizer::new);
        let elements: Vec<TimelineElement> = cues
            .iter()
            .enumerate()
            .filter_map(|(index, cue)| {
                crate::edit::subtitle_text_element(
                    index, cue, canvas.0, canvas.1, &style, rasterizer,
                )
            })
            .collect();
        let imported = elements.len();
        if imported == 0 {
            self.caption_notice = Some(t("captions.error.none"));
            cx.notify();
            return;
        }

        self.app.update(cx, |model, cx| {
            model.edit(cx, |editor| editor.insert_caption_track(elements));
        });

        self.caption_notice = Some(if skipped > 0 {
            t_args(
                "captions.warn.skipped",
                &[
                    ("imported", &imported.to_string()),
                    ("skipped", &skipped.to_string()),
                ],
            )
        } else {
            t_args("captions.imported", &[("count", &imported.to_string())])
        });
        cx.notify();
    }

    fn generate_transcript(&mut self, cx: &mut Context<Self>) {
        if self.job.is_some() {
            return;
        }
        let model = self.app.read(cx);
        let Some(project) = model.project.clone() else {
            return;
        };
        let scene = model.current_scene().map(|scene| scene.id.clone());
        let store = model.store.clone();

        let spec = ml::WHISPER_MODELS
            .get(self.caption_model)
            .unwrap_or(&ml::WHISPER_MODELS[0]);
        let language = TRANSCRIPTION_LANGUAGES
            .get(self.caption_language)
            .map(|(code, _)| *code)
            .filter(|code| !code.is_empty())
            .map(str::to_owned);

        let job = crate::ai::Job::new(t("captions.step.extractingAudio"));
        self.job = Some(job.clone());
        self.caption_notice = None;
        cx.notify();

        let worker = job.clone();
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn(async move {
                    let samples = crate::ai::timeline_audio(&project, &store, scene.as_deref())?;
                    if worker.is_cancelled() {
                        return Err(t("common.cancel"));
                    }

                    let progress_job = worker.clone();
                    let mut on_progress = move |progress: ml::WhisperProgress| match progress {
                        ml::WhisperProgress::Downloading { file, done } => progress_job.publish(
                            t_args(
                                "captions.step.loadingModel",
                                &[("percent", &format!("{:.0}", done * 100.0))],
                            ) + " · "
                                + &file,
                            done,
                        ),
                        ml::WhisperProgress::Loading => {
                            progress_job.publish(
                                t_args("captions.step.loadingModel", &[("percent", "100")]),
                                1.0,
                            );
                        }
                        ml::WhisperProgress::Transcribing { done } => {
                            progress_job.publish(t("captions.step.transcribing"), done);
                        }
                    };
                    let cancel_job = worker.clone();
                    let should_cancel = move || cancel_job.is_cancelled();

                    let transcript = ml::transcribe(
                        &samples,
                        crate::ai::TRANSCRIPTION_SAMPLE_RATE,
                        spec.key,
                        language.as_deref(),
                        &mut on_progress,
                        &should_cancel,
                    )
                    .map_err(|error| error.to_string())?;

                    let segments: Vec<(String, f64, f64)> = transcript
                        .segments
                        .into_iter()
                        .map(|segment| (segment.text, segment.start, segment.end))
                        .collect();
                    Ok(crate::ai::caption_chunks(
                        &segments,
                        crate::ai::DEFAULT_WORDS_PER_CAPTION,
                        crate::ai::MIN_CAPTION_DURATION_SECONDS,
                    ))
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.job = None;
                match outcome {
                    Ok(cues) if !cues.is_empty() => this.insert_caption_cues(cues, 0, cx),
                    Ok(_) => {
                        this.caption_notice = Some(t("captions.error.none"));
                    }
                    Err(message) => {
                        this.caption_notice = Some(message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn speech_body(&mut self, window: &Window, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        let notice = self.speech_notice.clone();
        let busy = self.job.is_some();
        let bar = scrollbar_v(&self.body_scroll, colors);

        let input = crate::input::text_field(
            "speech-text",
            &self.speech_text,
            colors,
            crate::input::FieldStyle {
                height: 32.0,
                placeholder: SharedString::from(t("speech.placeholder")),
                leading: Some(icon("mic01")),
                ..Default::default()
            },
            window,
        )
        .on_key_down(
            cx.listener(|this: &mut Self, event: &gpui::KeyDownEvent, _, cx| {
                match this.speech_text.buffer.key_down(event) {
                    crate::input::TextEvent::Cancel => this.speech_text.buffer.set(""),
                    crate::input::TextEvent::Submit => this.generate_speech(cx),
                    _ => {}
                }
                cx.notify();
            }),
        );

        let voices = self.chip_row(
            "speech-voice",
            t("speech.voice"),
            speech::models::VOICES
                .iter()
                .enumerate()
                .map(|(index, voice)| (index, voice.label.to_owned()))
                .collect(),
            self.speech_voice,
            |this, index| this.speech_voice = index,
            cx,
        );
        let progress = self.job_row("speech-cancel", cx);
        let sample = self.speech_sample.as_ref().and_then(|(samples, rate)| {
            format_seconds(samples.len() as f64 / (*rate).max(1) as f64)
        });

        div()
            .flex()
            .flex_col()
            .flex_1()
            .w_full()
            .min_h_0()
            .child(
                div()
                    .id("assets-speech")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .gap(px(12.0))
                    .p(px(10.0))
                    .overflow_y_scroll()
                    .track_scroll(&self.body_scroll)
                    .child(div().w_full().flex_shrink_0().child(input))
                    .child(voices)
                    .child(
                        div()
                            .flex()
                            .w_full()
                            .flex_shrink_0()
                            .gap(px(6.0))
                            .child(
                                Button::new("speech-generate", colors)
                                    .variant(ButtonVariant::Default)
                                    .hover(self.transitions.eased("speech-generate"))
                                    .label(t("speech.generate"))
                                    .build()
                                    .flex_1()
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.generate_speech(cx);
                                    })),
                            )
                            .when_some(sample, |this, duration| {
                                this.child(
                                    Button::new("speech-preview", colors)
                                        .variant(ButtonVariant::Secondary)
                                        .hover(self.transitions.eased("speech-preview"))
                                        .label(format!("{} · {duration}", t("common.play")))
                                        .build()
                                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                            this.preview_speech(cx);
                                        })),
                                )
                            }),
                    )
                    .children(progress)
                    .when_some(notice, |this, message| {
                        this.child(
                            div()
                                .w_full()
                                .text_size(rem(TEXT_XS))
                                .text_color(colors.muted_foreground)
                                .child(message),
                        )
                    })
                    .when(!busy, |this| {
                        this.child(
                            div()
                                .w_full()
                                .text_size(rem(TEXT_XS))
                                .text_color(colors.muted_foreground)
                                .child(t("speech.hint")),
                        )
                    }),
            )
            .children(bar)
    }

    fn generate_speech(&mut self, cx: &mut Context<Self>) {
        if self.job.is_some() {
            return;
        }
        let text = self.speech_text.text().trim().to_string();
        if text.is_empty() {
            self.speech_notice = Some(t("speech.emptyText"));
            cx.notify();
            return;
        }
        let Some(voice) = speech::models::VOICES.get(self.speech_voice) else {
            return;
        };
        let voice_key = voice.key;

        let job = crate::ai::Job::new(t("speech.generating"));
        self.job = Some(job.clone());
        self.speech_notice = None;
        cx.notify();

        let worker = job.clone();
        let spoken = text.clone();
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn(async move {
                    let progress_job = worker.clone();
                    let audio = speech::synthesize_with(
                        &spoken,
                        speech::models::DEFAULT_MODEL,
                        voice_key,
                        |file, done| {
                            progress_job.publish(
                                t_args(
                                    "speech.downloading",
                                    &[("percent", &format!("{:.0}", done * 100.0))],
                                ) + " · "
                                    + file,
                                done,
                            );
                        },
                    )
                    .map_err(|error| error.to_string())?;
                    if worker.is_cancelled() {
                        return Err(t("common.cancel"));
                    }
                    worker.publish(t("speech.generating"), 1.0);
                    let path = crate::ai::write_temp_wav(
                        &audio.0,
                        audio.1,
                        &crate::ai::sanitise_asset_stem(&spoken),
                    )?;
                    Ok((audio, path))
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.job = None;
                match outcome {
                    Ok((audio, path)) => {
                        this.speech_sample = Some(audio);
                        this.speech_notice = Some(t("speech.done"));
                        this.app.update(cx, |model, cx| {
                            model.import_media(vec![path], cx);
                        });
                    }
                    Err(message) => {
                        this.speech_notice = Some(format!("{} — {message}", t("speech.failed")));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn preview_speech(&mut self, cx: &mut Context<Self>) {
        if let Some(output) = self.audio_preview.take() {
            let _ = output.stop();
            cx.notify();
            return;
        }
        let Some((samples, rate)) = self.speech_sample.clone() else {
            return;
        };
        let output = match cutix_playback::AudioOutput::open() {
            Ok(output) => output,
            Err(error) => {
                self.speech_notice = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let resampled = crate::ai::resample_linear(&samples, rate, output.sample_rate());
        output.queue_samples(&crate::ai::interleave(&resampled, output.channels()));
        output.start();
        self.audio_preview = Some(output);
        cx.notify();
    }

    fn export_subtitles(&mut self, cx: &mut Context<Self>) {
        let model = self.app.read(cx);
        let cues = model
            .project
            .as_ref()
            .and_then(|project| {
                project
                    .scenes
                    .iter()
                    .find(|scene| scene.id == project.current_scene_id)
                    .or_else(|| project.scenes.first())
            })
            .map(|scene| crate::edit::text_elements_as_cues(&scene.tracks))
            .unwrap_or_default();

        let raw = cutix_project::serialize_srt(&cues);
        if raw.is_empty() {
            self.caption_notice = Some(t("captions.export.empty"));
            cx.notify();
            return;
        }

        let directory = dirs::document_dir().unwrap_or_else(std::env::temp_dir);
        cx.spawn(async move |this, cx| {
            let Some(path) = crate::dialogs::save_file(
                crate::dialogs::Filter::Subtitles,
                t("dialog.captions.title"),
                directory,
                "captions.srt".to_owned(),
            )
            .await
            else {
                return;
            };
            let written = std::fs::write(&path, raw).is_ok();
            let _ = this.update(cx, |this, cx| {
                this.caption_notice = Some(if written {
                    t_args(
                        "captions.exported",
                        &[("path", &path.display().to_string())],
                    )
                } else {
                    t("captions.error.unexpected")
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn template_card(
        &mut self,
        entry: &crate::templates_ui::TemplateEntry,
        cx: &mut Context<Self>,
    ) -> Div {
        let colors = self.colors(cx);
        let key = format!("template-{}", entry.id);
        let progress = self.transitions.eased(&key);
        let hover_key = key.clone();
        let id = entry.id.clone();
        let name = entry.name.clone();
        let description = entry.description.clone();
        let slots = t_args(
            "templates.slots",
            &[("count", &entry.slot_count().to_string())],
        );
        let removable = entry.path.is_some();
        let delete_id = entry.id.clone();

        div().w_full().p(px(4.0)).child(
            div()
                .id(SharedString::from(key))
                .flex()
                .flex_col()
                .w_full()
                .gap(px(4.0))
                .p(px(10.0))
                .rounded(rem(RADIUS_SM))
                .border_1()
                .border_color(mix(colors.border, colors.primary, progress))
                .bg(opacity(colors.foreground, 0.04))
                .cursor_pointer()
                .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                    this.transitions.set(hover_key.clone(), *hovered);
                    cx.notify();
                }))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.apply_template(&id, cx);
                }))
                .child(
                    div()
                        .flex()
                        .w_full()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex_1()
                                .truncate()
                                .text_size(rem(TEXT_SM))
                                .child(name),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_size(rem(TEXT_XS))
                                .text_color(colors.muted_foreground)
                                .child(slots),
                        ),
                )
                .when(!description.is_empty(), |this| {
                    this.child(
                        div()
                            .w_full()
                            .text_size(rem(TEXT_XS))
                            .text_color(colors.muted_foreground)
                            .child(description),
                    )
                })
                .when(removable, |this| {
                    this.child(
                        div()
                            .id(SharedString::from(format!("template-delete-{delete_id}")))
                            .mt(px(2.0))
                            .text_size(rem(TEXT_XS))
                            .text_color(colors.muted_foreground)
                            .cursor_pointer()
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.delete_template(&delete_id, cx);
                            }))
                            .child(t("templates.delete")),
                    )
                }),
        )
    }

    fn templates_body(&mut self, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        let builtins = crate::templates_ui::builtin_entries();
        let mine = self.user_templates.clone();
        let notice = self.template_notice.clone();
        let bar = scrollbar_v(&self.body_scroll, colors);

        let section = |label: String| {
            div()
                .w_full()
                .px(px(8.0))
                .pt(px(10.0))
                .pb(px(4.0))
                .text_size(rem(TEXT_XS))
                .text_color(colors.muted_foreground)
                .child(label)
        };

        let builtin_cards = builtins
            .iter()
            .map(|entry| self.template_card(entry, cx))
            .collect::<Vec<_>>();
        let mine_cards = mine
            .iter()
            .map(|entry| self.template_card(entry, cx))
            .collect::<Vec<_>>();
        let mine_empty = mine.is_empty();

        div()
            .flex()
            .flex_col()
            .flex_1()
            .w_full()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .w_full()
                    .flex_shrink_0()
                    .gap(px(6.0))
                    .p(px(8.0))
                    .child(
                        Button::new("templates-save", colors)
                            .variant(ButtonVariant::Secondary)
                            .hover(self.transitions.eased("templates-save"))
                            .label(t("templates.saveCurrent"))
                            .build()
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.save_current_as_template(cx);
                            })),
                    )
                    .child(
                        Button::new("templates-capcut", colors)
                            .variant(ButtonVariant::Secondary)
                            .hover(self.transitions.eased("templates-capcut"))
                            .label(t("templates.importCapcut"))
                            .build()
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.import_capcut(cx);
                            })),
                    ),
            )
            .when_some(notice, |this, message| {
                this.child(
                    div()
                        .w_full()
                        .px(px(10.0))
                        .pb(px(6.0))
                        .text_size(rem(TEXT_XS))
                        .text_color(colors.muted_foreground)
                        .child(message),
                )
            })
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .child(
                        div()
                            .id("assets-templates")
                            .flex()
                            .flex_col()
                            .size_full()
                            .p(px(4.0))
                            .overflow_y_scroll()
                            .track_scroll(&self.body_scroll)
                            .when(!builtin_cards.is_empty(), |this| {
                                this.child(section(t("templates.builtin")))
                            })
                            .children(builtin_cards)
                            .child(section(t("templates.mine")))
                            .when(mine_empty, |this| {
                                this.child(
                                    div()
                                        .w_full()
                                        .px(px(10.0))
                                        .py(px(6.0))
                                        .text_size(rem(TEXT_XS))
                                        .text_color(colors.muted_foreground)
                                        .child(t("templates.empty.hint")),
                                )
                            })
                            .children(mine_cards),
                    )
                    .children(bar),
            )
    }

    fn find_template(&self, id: &str) -> Option<crate::templates_ui::TemplateEntry> {
        crate::templates_ui::builtin_entries()
            .into_iter()
            .chain(self.user_templates.iter().cloned())
            .find(|entry| entry.id == id)
    }

    fn apply_template(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(entry) = self.find_template(id) else {
            return;
        };
        let media = self.app.read(cx).media.clone();
        let canvas = entry.manifest.canvas;
        let fps = entry.manifest.fps;

        let restored =
            cutix_project::template_project::instantiate_template(&entry.manifest, &media);

        self.app.update(cx, |model, cx| {
            model.update_settings(cx, |settings| {
                settings.canvas_size.width = canvas.width;
                settings.canvas_size.height = canvas.height;
                settings.fps.numerator = fps.numerator;
                settings.fps.denominator = fps.denominator;
                true
            });
            if let Some(restored) = &restored {
                let tracks = restored.tracks.clone();
                model.edit(cx, |editor| editor.replace_tracks(tracks));
            }
        });

        self.template_notice = Some(match &restored {
            Some(restored) if restored.dropped_elements > 0 => t_args(
                "templates.appliedPartial",
                &[("count", &restored.dropped_elements.to_string())],
            ),
            Some(_) => t("templates.applied"),
            None => t("templates.applyUnavailable"),
        });
        cx.notify();
    }

    fn save_current_as_template(&mut self, cx: &mut Context<Self>) {
        let model = self.app.read(cx);
        let Some(project) = model.project.as_ref() else {
            return;
        };
        let Some(scene) = project
            .scenes
            .iter()
            .find(|scene| scene.id == project.current_scene_id)
            .or_else(|| project.scenes.first())
        else {
            return;
        };

        let manifest = cutix_project::template_project::build_template_from_project(
            &project.metadata.name,
            "",
            template::CanvasSpec {
                width: project.settings.canvas_size.width,
                height: project.settings.canvas_size.height,
            },
            template::FpsSpec {
                numerator: project.settings.fps.numerator,
                denominator: project.settings.fps.denominator,
            },
            &scene.tracks,
            &model.media,
        );

        self.template_notice = Some(match manifest {
            Ok(manifest) => match crate::templates_ui::save_user_template(&manifest) {
                Ok(_) => {
                    self.user_templates = crate::templates_ui::user_entries();
                    t("templates.saved")
                }
                Err(_) => t("templates.saveFailed"),
            },
            Err(_) => t("templates.saveFailed"),
        });
        cx.notify();
    }

    fn delete_template(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(path) = self
            .user_templates
            .iter()
            .find(|entry| entry.id == id)
            .and_then(|entry| entry.path.clone())
        else {
            return;
        };
        let _ = crate::templates_ui::delete_user_template(&path);
        self.user_templates = crate::templates_ui::user_entries();
        cx.notify();
    }

    fn import_capcut(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let paths = crate::dialogs::open_files(
                crate::dialogs::Filter::CapCut,
                t("dialog.capcut.title"),
                false,
            )
            .await;
            let Some(path) = paths.first().cloned() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.template_notice =
                    Some(match crate::templates_ui::import_capcut_draft(&path) {
                        Ok(manifest) => match crate::templates_ui::save_user_template(&manifest) {
                            Ok(_) => {
                                this.user_templates = crate::templates_ui::user_entries();
                                t("templates.imported")
                            }
                            Err(_) => t("templates.importFailed"),
                        },
                        Err(reason) => t_args("templates.capcut.failed", &[("reason", &reason)]),
                    });
                cx.notify();
            });
        })
        .detach();
    }

    fn insert_sticker(&mut self, sticker_id: &str, name: &str, cx: &mut Context<Self>) {
        let definition = crate::stickers_ui::graphic_definition_for(sticker_id);
        let element = match definition {
            Some(definition_id) => crate::edit::graphic_element(
                definition_id.to_owned(),
                name.to_owned(),
                cutix_project::model::ParamValues::new(),
            ),
            None => crate::edit::sticker_element(sticker_id.to_owned(), name.to_owned()),
        };
        let playhead = self.app.read(cx).playhead;
        self.app.update(cx, |model, cx| {
            model.edit(cx, |editor| editor.insert_element(element, playhead));
        });
        cx.notify();
    }

    fn insert_text_preset(&mut self, preset_id: &str, cx: &mut Context<Self>) {
        let presets = crate::text::presets();
        let Some(preset) = presets.iter().find(|preset| preset.id == preset_id) else {
            return;
        };
        let name = t(preset.name_key);
        let content = t("text.default");
        let patch = crate::text::patch_for(preset);
        let element = crate::edit::text_element(name, content, patch);
        let playhead = self.app.read(cx).playhead;
        self.app.update(cx, |model, cx| {
            model.edit(cx, |editor| editor.insert_element(element, playhead));
        });
        cx.notify();
    }

    fn selected_visual(&self, cx: &App) -> Option<String> {
        let element = self.app.read(cx).selected_element()?;
        matches!(
            element,
            TimelineElement::Video(_)
                | TimelineElement::Image(_)
                | TimelineElement::Text(_)
                | TimelineElement::Sticker(_)
                | TimelineElement::Graphic(_)
        )
        .then(|| element.base().id.clone())
    }

    fn clip_effect(&self, cx: &App, effect_type: &str) -> Option<(String, String)> {
        let element = self.app.read(cx).selected_element()?;
        let effect = edit::effects_of(element)
            .iter()
            .find(|effect| effect.effect_type == effect_type)?;
        Some((element.base().id.clone(), effect.id.clone()))
    }

    fn ensure_effect(
        &mut self,
        effect_type: &str,
        cx: &mut Context<Self>,
    ) -> Option<(String, String)> {
        if let Some(found) = self.clip_effect(cx, effect_type) {
            return Some(found);
        }
        let element_id = self.selected_visual(cx)?;
        let kind = effect_type.to_owned();
        let created = self.app.update(cx, |model, cx| {
            let mut created = None;
            model.edit(cx, |editor| {
                created = editor.add_clip_effect(&element_id, &kind);
                created.is_some()
            });
            created
        })?;
        Some((element_id, created))
    }

    fn param_value(&self, cx: &App, effect_type: &str, key: &str) -> Option<serde_json::Value> {
        let element = self.app.read(cx).selected_element()?;
        edit::effects_of(element)
            .iter()
            .find(|effect| effect.effect_type == effect_type)
            .and_then(|effect| effect.params.get(key).cloned())
    }

    fn number_param(&self, cx: &App, effect_type: &str, key: &str) -> f64 {
        let fallback = effects_ui::param_definition(effect_type, key)
            .map(|param| param.default_number)
            .unwrap_or(0.0);
        self.param_value(cx, effect_type, key)
            .and_then(|value| value.as_f64())
            .unwrap_or(fallback)
    }

    fn text_param(&self, cx: &App, effect_type: &str, key: &str) -> String {
        let fallback = effects_ui::param_definition(effect_type, key)
            .map(|param| param.default_text.to_owned())
            .unwrap_or_default();
        self.param_value(cx, effect_type, key)
            .and_then(|value| value.as_str().map(|text| text.to_owned()))
            .unwrap_or(fallback)
    }

    fn write_param(
        &mut self,
        effect_type: &str,
        key: &str,
        value: serde_json::Value,
        cx: &mut Context<Self>,
    ) {
        let Some((element_id, effect_id)) = self.ensure_effect(effect_type, cx) else {
            return;
        };
        let patch = vec![(key.to_owned(), value)];
        self.app.update(cx, |model, cx| {
            model.edit(cx, |editor| {
                editor.update_clip_effect_params(&element_id, &effect_id, patch)
            })
        });
        cx.notify();
    }

    fn asset_card(
        &mut self,
        id: String,
        label: String,
        glyph: &'static str,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let colors = self.colors(cx);
        let progress = self.transitions.eased(&format!("card-{id}"));
        let hover_key = id.clone();
        div()
            .id(SharedString::from(format!("asset-card-{id}")))
            .flex()
            .flex_col()
            .w_full()
            .gap(px(4.0))
            .cursor_pointer()
            .on_hover(cx.listener(move |this: &mut Self, hovered, _, cx| {
                this.transitions.set(format!("card-{hover_key}"), *hovered);
                cx.notify();
            }))
            .child(
                div()
                    .flex()
                    .w_full()
                    .h(px(64.0))
                    .items_center()
                    .justify_center()
                    .rounded(rem(RADIUS_SM))
                    .border_1()
                    .border_color(if selected {
                        colors.primary
                    } else {
                        opacity(colors.border, 0.0)
                    })
                    .bg(mix(colors.accent, opacity(colors.primary, 0.25), progress))
                    .child(
                        svg()
                            .size(px(22.0))
                            .path(icon(glyph))
                            .text_color(if selected {
                                colors.primary
                            } else {
                                colors.muted_foreground
                            }),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(label),
            )
    }

    fn grid(&self, cards: Vec<Stateful<Div>>) -> Div {
        div()
            .flex()
            .flex_wrap()
            .w_full()
            .children(cards.into_iter().map(|card| {
                div()
                    .w(relative(1.0 / 3.0))
                    .flex_shrink_0()
                    .p(px(4.0))
                    .child(card)
            }))
    }

    fn scroller(&mut self, id: &'static str, content: Div, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        let bar = scrollbar_v(&self.body_scroll, colors);
        div()
            .relative()
            .flex()
            .flex_1()
            .w_full()
            .min_h_0()
            .child(
                div()
                    .id(id)
                    .flex()
                    .flex_col()
                    .size_full()
                    .p(px(8.0))
                    .gap(px(8.0))
                    .overflow_y_scroll()
                    .track_scroll(&self.body_scroll)
                    .child(content),
            )
            .children(bar)
    }

    fn hint(&self, colors: Palette, text: String) -> Div {
        div()
            .w_full()
            .px(px(4.0))
            .text_size(rem(TEXT_XS))
            .text_color(colors.muted_foreground)
            .child(text)
    }

    fn param_slider(
        &mut self,
        effect_type: &'static str,
        param: &effects_ui::ParamDefinition,
        cx: &mut Context<Self>,
    ) -> Div {
        let colors = self.colors(cx);
        let effects_ui::ParamKind::Number { min, max, step: _ } = param.kind else {
            return div();
        };
        let value = self.number_param(cx, effect_type, param.key);
        let fraction = if max > min {
            ((value - min) / (max - min)) as f32
        } else {
            0.0
        };
        let key = format!("{effect_type}|{}", param.key);
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(4.0))
            .child(
                div()
                    .flex()
                    .w_full()
                    .justify_between()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(t(param.label_key))
                    .child(format!("{value:.0}")),
            )
            .child(
                div()
                    .id(SharedString::from(format!("param-{key}")))
                    .flex()
                    .w_full()
                    .h(px(18.0))
                    .items_center()
                    .cursor_pointer()
                    .on_drag(AssetSliderDrag { key }, |_, _, _, cx| {
                        cx.new(|_| gpui::Empty)
                    })
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(6.0))
                            .rounded(px(3.0))
                            .bg(colors.accent)
                            .child(
                                div()
                                    .w(relative(fraction.clamp(0.0, 1.0)))
                                    .h_full()
                                    .rounded(px(3.0))
                                    .bg(colors.primary),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(-4.0))
                                    .left(relative(fraction.clamp(0.0, 1.0)))
                                    .ml(px(-7.0))
                                    .size(px(14.0))
                                    .rounded_full()
                                    .border_1()
                                    .border_color(colors.border)
                                    .bg(colors.foreground),
                            ),
                    ),
            )
    }

    fn on_param_slider(
        &mut self,
        event: &gpui::DragMoveEvent<AssetSliderDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.drag(cx).key.clone();
        let Some((effect_type, param_key)) = key.split_once('|') else {
            return;
        };
        let Some(param) = effects_ui::param_definition(effect_type, param_key) else {
            return;
        };
        let effects_ui::ParamKind::Number { min, max, step } = param.kind else {
            return;
        };
        let bounds = event.bounds;
        let width = f32::from(bounds.size.width) as f64;
        if width <= 0.0 {
            return;
        }
        let x = f32::from(event.event.position.x - bounds.origin.x) as f64;
        let fraction = (x / width).clamp(0.0, 1.0);
        let raw = min + fraction * (max - min);
        let snapped = if step > 0.0 {
            (raw / step).round() * step
        } else {
            raw
        };
        let effect_type = effect_type.to_owned();
        let param_key = param_key.to_owned();
        self.write_param(
            &effect_type,
            &param_key,
            serde_json::json!(snapped.clamp(min, max)),
            cx,
        );
    }

    fn effects_body(&mut self, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        let selected = self.selected_visual(cx);
        let applied: Vec<String> = self
            .app
            .read(cx)
            .selected_element()
            .map(|element| {
                edit::effects_of(element)
                    .iter()
                    .map(|effect| effect.effect_type.clone())
                    .collect()
            })
            .unwrap_or_default();

        let cards = effects_ui::EFFECT_DEFINITIONS
            .iter()
            .enumerate()
            .map(|(index, definition)| {
                let is_applied = applied.iter().any(|kind| kind == definition.effect_type);
                self.asset_card(
                    format!("effect-{}", definition.effect_type),
                    t(definition.name_key),
                    definition.glyph,
                    is_applied,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    let kind = effects_ui::EFFECT_DEFINITIONS[index].effect_type;
                    this.ensure_effect(kind, cx);
                    cx.notify();
                }))
                .on_drag(
                    ClipTargetDrag::Effect(definition.effect_type),
                    |_, _, _, cx| cx.new(|_| gpui::Empty),
                )
            })
            .collect::<Vec<_>>();

        let hint = if selected.is_some() {
            self.hint(colors, t("properties.effects.emptyHint"))
        } else {
            self.hint(colors, t("editor.effects.selectClip"))
        };
        let grid = self.grid(cards);
        let content = div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(8.0))
            .child(hint)
            .child(grid);
        self.scroller("assets-effects", content, cx)
    }

    fn adjustment_body(&mut self, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        if self.selected_visual(cx).is_none() {
            let hint = self.hint(colors, t("editor.adjustment.selectClip"));
            return self.scroller("assets-adjustment", hint, cx);
        }

        let active_preset = self.text_param(cx, "filter", "preset");
        let has_filter = self.clip_effect(cx, "filter").is_some();
        let preset_cards = effects_ui::FILTER_PRESET_IDS
            .iter()
            .enumerate()
            .map(|(index, id)| {
                let selected = has_filter && active_preset == *id;
                self.asset_card(
                    format!("preset-{id}"),
                    t(&format!("effects.filter.preset.{id}")),
                    "checkerboard",
                    selected,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    let preset = effects_ui::FILTER_PRESET_IDS[index];
                    this.write_param("filter", "preset", serde_json::json!(preset), cx);
                }))
            })
            .collect::<Vec<_>>();
        let presets = self.grid(preset_cards);
        let intensity = match effects_ui::param_definition("filter", "intensity").copied() {
            Some(param) => self.param_slider("filter", &param, cx),
            None => div(),
        };

        let band_index = self.hsl_band.min(effects_ui::HSL_BAND_KEYS.len() - 1);
        let band_cards = effects_ui::HSL_BAND_KEYS
            .iter()
            .enumerate()
            .map(|(index, (id, label))| {
                self.asset_card(
                    format!("band-{id}"),
                    t(label),
                    "layers01",
                    index == band_index,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.hsl_band = index;
                    cx.notify();
                }))
            })
            .collect::<Vec<_>>();
        let bands = self.grid(band_cards);
        let band_id = effects_ui::HSL_BAND_KEYS[band_index].0;
        let hsl_rows = effects_ui::HSL_CHANNEL_KEYS
            .iter()
            .filter_map(|(channel, _)| {
                effects_ui::param_definition("hsl", &format!("{band_id}.{channel}")).copied()
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|param| self.param_slider("hsl", &param, cx))
            .collect::<Vec<_>>();

        let adjust_rows = effects_ui::definition("adjustment")
            .map(|definition| definition.params.to_vec())
            .unwrap_or_default()
            .into_iter()
            .map(|param| self.param_slider("adjustment", &param, cx))
            .collect::<Vec<_>>();

        let content = div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(12.0))
            .child(self.group_title(colors, t("editor.adjustment.filters")))
            .child(presets)
            .child(intensity)
            .child(self.group_title(colors, t("effects.hsl.name")))
            .child(bands)
            .children(hsl_rows)
            .child(self.group_title(colors, t("editor.adjustment.title")))
            .children(adjust_rows);
        self.scroller("assets-adjustment", content, cx)
    }

    fn group_title(&self, colors: Palette, label: String) -> Div {
        div()
            .w_full()
            .px(px(4.0))
            .text_size(rem(TEXT_SM))
            .font_weight(FontWeight::MEDIUM)
            .text_color(colors.foreground)
            .child(label)
    }

    fn transitions_body(&mut self, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        let target = self.app.read(cx).transition_target();
        let current = target.as_ref().and_then(|id| {
            self.app
                .read(cx)
                .element_by_id(id)
                .and_then(edit::transition_of)
                .map(|transition| transition.transition_type.clone())
        });

        let cards = effects_ui::TRANSITION_KEYS
            .iter()
            .enumerate()
            .map(|(index, (id, label))| {
                let selected = current.as_deref() == Some(*id);
                self.asset_card(
                    format!("transition-{id}"),
                    t(label),
                    "arrow-right-double",
                    selected,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    let kind = effects_ui::TRANSITION_KEYS[index].0;
                    this.apply_transition(kind, cx);
                }))
                .on_drag(ClipTargetDrag::Transition(*id), |_, _, _, cx| {
                    cx.new(|_| gpui::Empty)
                })
            })
            .collect::<Vec<_>>();

        let grid = self.grid(cards);
        let notice = if target.is_some() {
            t("transitions.hint")
        } else {
            t("transitions.noCut")
        };
        let content = div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(8.0))
            .child(self.hint(colors, notice))
            .child(grid);
        self.scroller("assets-transitions", content, cx)
    }

    fn transition_of(
        &self,
        element_id: &str,
        cx: &App,
    ) -> Option<cutix_project::model::ElementTransition> {
        let scene = self.app.read(cx).current_scene()?;
        std::iter::once(&scene.tracks.main)
            .chain(scene.tracks.overlay.iter())
            .flat_map(|track| track.elements().iter())
            .find(|element| element.base().id == element_id)
            .and_then(edit::transition_of)
            .cloned()
    }

    fn default_transition_duration(&self, element_id: &str, cx: &App) -> MediaTime {
        use cutix_playback::transitions;
        let default = MediaTime::from_ticks(transitions::DEFAULT_TRANSITION_DURATION_TICKS);
        let Some(scene) = self.app.read(cx).current_scene() else {
            return default;
        };
        for track in std::iter::once(&scene.tracks.main).chain(scene.tracks.overlay.iter()) {
            let Some(previous) = transitions::find_transition_neighbour(track, element_id) else {
                continue;
            };
            let Some(current) = track
                .elements()
                .iter()
                .find(|element| element.base().id == element_id)
            else {
                continue;
            };
            return transitions::clamp_transition_duration(
                default,
                previous.base().duration,
                current.base().duration,
            );
        }
        default
    }

    fn apply_transition(&mut self, transition_type: &str, cx: &mut Context<Self>) {
        let Some(element_id) = self.app.read(cx).transition_target() else {
            return;
        };
        use cutix_playback::transitions;

        let (duration, easing) = match self.transition_of(&element_id, cx) {
            Some(existing) => (existing.duration, existing.easing),
            None => (
                self.default_transition_duration(&element_id, cx),
                Some(transitions::DEFAULT_TRANSITION_EASING.to_owned()),
            ),
        };

        let transition = cutix_project::model::ElementTransition {
            transition_type: transition_type.to_owned(),
            duration,
            easing,
        };
        self.app.update(cx, |model, cx| {
            model.edit(cx, |editor| {
                editor.set_element_transition(&element_id, Some(transition))
            });
            model.select_only(&element_id, cx);
        });
        cx.notify();
    }

    pub fn publish_window_size(&self) -> (f32, f32) {
        if self.youtube.form.is_some() {
            return (1000.0, 560.0);
        }
        if self.youtube.choosing_account {
            return (440.0, 420.0);
        }
        if self.youtube.sign_in_form.is_some() || self.youtube.signing_in {
            return (560.0, 340.0);
        }
        if self.youtube.published.is_some() {
            return (380.0, 290.0);
        }
        let cards = self.youtube.rows().len() + self.youtube.session.len();
        let list = (cards.max(1) as f32) * 96.0;
        let notice = if self.youtube.notice.is_some() {
            26.0
        } else {
            0.0
        };
        (480.0, (140.0 + notice + list.min(430.0)).round())
    }

    pub fn youtube_wants_to_close(&mut self) -> bool {
        let waiting = self
            .youtube
            .queue
            .visible()
            .into_iter()
            .any(|task| task.state.is_active());
        crate::youtube_ui::render::set_uploading(waiting || !self.youtube.running.is_empty());
        let closing = self.youtube.should_close
            && self.youtube.form.is_none()
            && self.youtube.running.is_empty()
            && self.youtube.published.is_none()
            && self.youtube.session.is_empty()
            && !waiting;
        if closing {
            self.youtube.should_close = false;
        }
        closing
    }

    pub fn panel_overlays(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        self.take_requests(cx);
        self.publish_upload_summary(cx);
        if self.youtube.form.is_some() {
            self.advance_publish_preview(cx);
        }
        if self.youtube.form.is_some() || !self.youtube.running.is_empty() {
            window.request_animation_frame();
        }
        let colors = self.colors(cx);
        let now = youtube::now_unix();
        let mut dialogs = Vec::new();
        if let Some(modal) = self.settings_modal(window, cx) {
            dialogs.push(crate::youtube_ui::render::overlay(window, modal));
        }
        if let Some(dialog) =
            crate::youtube_ui::render::published_dialog(&mut self.youtube, colors, cx)
        {
            dialogs.push(crate::youtube_ui::render::overlay(window, dialog));
        } else if let Some(dialog) =
            crate::youtube_ui::render::session_dialog(&mut self.youtube, colors, cx)
        {
            dialogs.push(crate::youtube_ui::render::overlay(window, dialog));
        } else if let Some(dialog) =
            crate::youtube_ui::render::account_picker(&mut self.youtube, colors, cx)
        {
            dialogs.push(crate::youtube_ui::render::overlay(window, dialog));
        } else if let Some(dialog) =
            crate::youtube_ui::render::sign_in_dialog(&mut self.youtube, colors, window, cx)
        {
            dialogs.push(crate::youtube_ui::render::overlay(window, dialog));
        } else if let Some(dialog) =
            crate::youtube_ui::render::publish_dialog(&mut self.youtube, colors, now, window, cx)
        {
            dialogs.push(crate::youtube_ui::render::overlay(window, dialog));
        }
        self.youtube.expire_toasts();
        if !self.youtube.toasts.is_empty() {
            window.request_animation_frame();
            dialogs.push(
                crate::youtube_ui::render::published_toasts(&mut self.youtube, colors, cx)
                    .into_any_element(),
            );
        }
        dialogs
    }

    fn video_directory_row(&mut self, colors: Palette, cx: &mut Context<Self>) -> Div {
        let configured = self.app.read(cx).video_directory.clone();
        let effective = crate::library::directory(configured.as_deref());

        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .w_full()
            .child(
                div()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .truncate()
                    .child(SharedString::from(effective.display().to_string())),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(6.0))
                    .child(
                        self.pill(
                            "video-directory-choose".to_owned(),
                            t("settings.videoDirectory.choose"),
                            false,
                            cx,
                        )
                        .on_click(cx.listener(
                            |this: &mut Self, _, _, cx| {
                                this.choose_video_directory(cx);
                            },
                        )),
                    )
                    .when(configured.is_some(), |row| {
                        row.child(
                            self.pill(
                                "video-directory-reset".to_owned(),
                                t("settings.videoDirectory.reset"),
                                false,
                                cx,
                            )
                            .on_click(cx.listener(
                                |this: &mut Self, _, _, cx| {
                                    this.app.update(cx, |model, cx| {
                                        model.set_video_directory(None, cx)
                                    });
                                    cx.notify();
                                },
                            )),
                        )
                    }),
            )
            .child(self.hint(colors, t("settings.videoDirectory.hint")))
    }

    fn choose_video_directory(&mut self, cx: &mut Context<Self>) {
        let start = self.app.read(cx).video_directory.clone();
        cx.spawn(async move |this, cx| {
            let picked =
                crate::dialogs::open_folder(t("settings.videoDirectory.choose"), start).await;
            let Some(directory) = picked else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.app.update(cx, |model, cx| {
                    model.set_video_directory(Some(directory), cx)
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn settings_modal(&mut self, window: &Window, cx: &mut Context<Self>) -> Option<Div> {
        if !self.settings_open {
            return None;
        }
        let colors = self.colors(cx);
        let active = self.settings_tab.min(APP_SETTINGS_TABS.len() - 1);
        let strip = self.settings_tabs(
            APP_SETTINGS_TABS,
            active,
            colors,
            |this, index| {
                this.settings_tab = index;
            },
            cx,
        );
        let copied = self.youtube.copy_notice_visible();

        let close = div()
            .id("settings-modal-close")
            .size(px(24.0))
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .rounded(rem(RADIUS_SM))
            .cursor_pointer()
            .text_color(colors.muted_foreground)
            .child(
                gpui::svg()
                    .size(px(13.0))
                    .path(crate::assets::icon("win-close")),
            )
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.settings_open = false;
                cx.notify();
            }));

        let header = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .w_full()
            .child(
                div()
                    .flex_1()
                    .text_size(rem(TEXT_LG))
                    .text_color(colors.popover_foreground)
                    .child(t("settings.open")),
            )
            .child(close);

        let body = self.settings_section(
            APP_SETTINGS_TABS[active].0,
            div().flex().flex_col().w_full().gap(px(10.0)),
            colors,
            window,
            cx,
        );

        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(opacity(gpui::black(), 0.55))
                .occlude()
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.settings_open = false;
                        cx.notify();
                    }),
                )
                .child(
                    div()
                        .w(px(SETTINGS_MODAL_WIDTH_PX))
                        .max_w(gpui::relative(0.92))
                        .max_h(gpui::relative(0.86))
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .rounded(rem(RADIUS_LG))
                        .border_1()
                        .border_color(colors.border)
                        .p(px(18.0))
                        .bg(colors.popover)
                        .text_color(colors.popover_foreground)
                        .shadow_lg()
                        .relative()
                        .occlude()
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx: &mut gpui::App| {
                            cx.stop_propagation()
                        })
                        .child(header)
                        .child(strip)
                        .child(
                            div()
                                .id("settings-modal-body")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .child(body),
                        )
                        .when(copied, |card| {
                            card.child(
                                div()
                                    .absolute()
                                    .bottom(px(18.0))
                                    .left_0()
                                    .right_0()
                                    .flex()
                                    .justify_center()
                                    .child(
                                        div()
                                            .px(px(12.0))
                                            .py(px(6.0))
                                            .rounded(rem(RADIUS_SM))
                                            .bg(colors.foreground)
                                            .text_size(rem(TEXT_XS))
                                            .text_color(colors.background)
                                            .child(t("youtube.result.copied")),
                                    ),
                            )
                        }),
                ),
        )
    }

    pub fn dismiss_youtube_overlays(&mut self) -> bool {
        let open = self.youtube.sign_in_form.is_some()
            || self.youtube.form.is_some()
            || self.settings_open;
        if self.youtube.sign_in_form.is_some() {
            if let Some(form) = self.youtube.sign_in_form.as_ref() {
                form.abandon();
            }
            self.youtube.sign_in_form = None;
        } else if self.youtube.form.is_some() {
            self.youtube.form = None;
        } else if self.settings_open {
            self.settings_open = false;
        }
        open
    }

    fn publish_upload_summary(&mut self, cx: &mut Context<Self>) {
        let running = self.youtube.queue.running();
        let live = self.youtube.running.first().map(|job| job.snapshot());
        let summary = crate::state::UploadSummary {
            active: self.youtube.queue.active().len(),
            percent: live
                .map(|(_, percent)| percent)
                .or_else(|| running.map(|task| task.percent))
                .unwrap_or(0),
            title: running.map(|task| task.title().to_string()),
            finished: self.youtube.history.entries.len(),
        };

        if self.app.read(cx).uploads != summary {
            self.app.update(cx, |model, cx| {
                model.uploads = summary;
                cx.notify();
            });
        }
    }

    fn take_requests(&mut self, cx: &mut Context<Self>) {
        if let Some(source) = self.app.update(cx, |model, _| model.youtube_request.take()) {
            self.open_youtube_publish(source, cx);
        }
        if let Some(sub) = self
            .app
            .update(cx, |model, _| model.settings_request.take())
        {
            self.open_settings(Some(sub.as_str()).filter(|sub| !sub.is_empty()));
        }

        if !self.youtube_resumed {
            self.youtube_resumed = true;
            if self.youtube.queue.is_busy() {
                self.pump_youtube_queue(cx);
            }
        }
    }

    fn refresh_youtube_statuses(&mut self, _cx: &mut Context<Self>) {
        self.youtube_refreshed = true;
        self.youtube.refreshing = false;
    }

    pub fn open_settings(&mut self, sub: Option<&str>) {
        if let Some(sub) = sub {
            self.settings_tab = APP_SETTINGS_TABS
                .iter()
                .position(|(key, _)| *key == sub)
                .unwrap_or(self.settings_tab);
        }
        self.settings_open = true;
    }

    fn open_youtube_publish(&mut self, source: std::path::PathBuf, cx: &mut Context<Self>) {
        match self.youtube.accounts.accounts.len() {
            0 => {
                self.youtube.pending_publish = Some(source);
                self.open_youtube_sign_in(cx);
            }

            1 => {
                self.begin_publish(source, cx);
            }

            _ => {
                self.youtube.pending_publish = Some(source);
                self.youtube.choosing_account = true;
                self.ensure_channel_details(cx);
            }
        }
        cx.notify();
    }

    fn apply_publish_sound(&mut self, cx: &mut Context<Self>) {
        let sound = &self.youtube.sound;
        self.publish_speaker.set_volume(sound.effective());
        crate::state::save_preview_audio(sound.volume, sound.muted);
        if sound.muted {
            self.publish_speaker.silence();
        } else {
            self.ensure_publish_sound(cx);
        }
        cx.notify();
    }

    fn feed_publish_sound(&mut self) {
        if self.youtube.sound.muted {
            return;
        }
        let Some(form) = self.youtube.form.as_ref() else {
            return;
        };
        if !form.preview_playing {
            return;
        }
        self.publish_speaker.feed(form.preview_position);
    }

    fn ensure_publish_sound(&mut self, cx: &mut Context<Self>) {
        if self.youtube.sound.muted {
            return;
        }
        let Some(form) = self.youtube.form.as_ref() else {
            return;
        };
        if !form.preview_playing {
            return;
        }
        let path = form.source.clone();
        let at = form.preview_position;
        let Some(start) = self.publish_speaker.window_needed(&path, at) else {
            return;
        };

        self.publish_speaker.begin_loading();
        cx.spawn(async move |this, cx| {
            let decoded = cx
                .background_spawn({
                    let path = path.clone();
                    async move {
                        cutix_playback::decode_audio_window(
                            &path,
                            start,
                            crate::preview_audio::WINDOW_SECONDS,
                        )
                    }
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                match decoded {
                    Ok((pcm, window_start)) => {
                        this.publish_speaker.accept_window(&path, pcm, window_start);
                        this.feed_publish_sound();
                    }
                    Err(_) => {
                        this.publish_speaker
                            .note_silent(&path, crate::preview_audio::Silent::NoDecoder);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn advance_publish_preview(&mut self, cx: &mut Context<Self>) {
        let Some(form) = self.youtube.form.as_mut() else {
            return;
        };
        let duration = form.preview_duration;
        if duration <= 0.0 {
            return;
        }

        let elapsed = form.preview_stepped_at.elapsed().as_secs_f64();
        form.preview_stepped_at = std::time::Instant::now();
        if form.preview_playing {
            form.preview_position += elapsed;
            if form.preview_position >= duration {
                form.preview_position = 0.0;
            }
        }

        let path = form.source.clone();
        let at = form.preview_position;
        if self.publish_preview.is_none() {
            self.publish_preview = crate::preview::FrameWorker::spawn();
        }
        if let Some(worker) = self.publish_preview.as_ref() {
            worker.request(path, at, 0, Some(PUBLISH_PREVIEW_WIDTH));
        }

        let Some(frame) = self
            .publish_preview
            .as_ref()
            .and_then(crate::preview::FrameWorker::take)
        else {
            return;
        };
        let Some(form) = self.youtube.form.as_mut() else {
            return;
        };
        if form.preview_shown == Some(frame.timestamp) {
            return;
        }
        form.preview_shown = Some(frame.timestamp);
        self.ensure_publish_sound(cx);
        self.feed_publish_sound();
        let Some(form) = self.youtube.form.as_mut() else {
            return;
        };
        let stale = form.poster.replace(frame.image);
        if let Some(stale) = stale {
            cx.drop_image(stale, None);
        }
        cx.notify();
    }

    fn begin_publish(&mut self, source: std::path::PathBuf, cx: &mut Context<Self>) {
        self.youtube.choosing_account = false;
        self.youtube.pending_publish = None;
        if !self.youtube.open_publish(source.clone(), cx) {
            return;
        }
        self.load_publish_poster(source, cx);
        self.ensure_channel_details(cx);
    }

    fn ensure_channel_details(&mut self, cx: &mut Context<Self>) {
        if self.youtube.refreshing {
            return;
        }

        let incomplete = |account: &youtube::accounts::Account,
                          avatars: &crate::youtube_ui::Avatars| {
            let named = !account.title.trim().is_empty() && account.title != account.id;
            !named || avatars.get(&account.id).is_none()
        };

        let active = self.youtube.accounts.active().cloned();
        let wanted = active
            .filter(|account| incomplete(account, &self.youtube.avatars))
            .or_else(|| {
                self.youtube
                    .accounts
                    .accounts
                    .iter()
                    .find(|account| incomplete(account, &self.youtube.avatars))
                    .cloned()
            });

        let Some(account) = wanted else {
            return;
        };
        let id = account.id.clone();
        let directory = self.youtube.directory.clone();
        self.youtube.refreshing = true;

        cx.spawn(async move |this, cx| {
            let found = cx
                .background_spawn({
                    let id = id.clone();
                    async move { youtube::session::refresh_decorations(&directory, &id) }
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.youtube.refreshing = false;
                let Ok((title, handle, avatar_url, avatar)) = found else {
                    return;
                };
                if let Some(account) = this
                    .youtube
                    .accounts
                    .accounts
                    .iter_mut()
                    .find(|account| account.id == id)
                {
                    if !title.trim().is_empty() {
                        account.title = title;
                    }
                    if !handle.trim().is_empty() {
                        account.handle = handle;
                    }
                    if !avatar_url.trim().is_empty() {
                        account.avatar_url = avatar_url;
                    }
                }
                let carried = avatar.or_else(|| {
                    this.youtube
                        .accounts
                        .accounts
                        .iter()
                        .find(|account| account.id == id)
                        .map(|account| account.avatar_url.clone())
                        .filter(|url| url.starts_with("https://"))
                        .and_then(|url| crate::youtube_ui::fetch_avatar(&url))
                });
                if let Some(bytes) = carried {
                    this.youtube.store_avatar(&id, &bytes);
                }
                this.youtube.persist();
                this.ensure_channel_details(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn load_publish_poster(&mut self, source: std::path::PathBuf, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let probed = cx
                .background_spawn({
                    let source = source.clone();
                    async move { crate::preview::probe(&source) }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some(form) = this.youtube.form.as_mut() else {
                    return;
                };
                if form.source != source {
                    return;
                }
                form.poster = probed.image;
                form.preview_duration = probed.duration_seconds.unwrap_or_default();
                cx.notify();
            });
        })
        .detach();
    }

    fn choose_publish_account(&mut self, id: String, cx: &mut Context<Self>) {
        self.youtube.accounts.select(&id);
        self.youtube.persist();
        let Some(source) = self.youtube.pending_publish.take() else {
            self.youtube.choosing_account = false;
            cx.notify();
            return;
        };
        self.begin_publish(source, cx);
        cx.notify();
    }

    fn open_youtube_sign_in(&mut self, cx: &mut Context<Self>) {
        if !self.youtube.is_configured() {
            self.youtube.notice = Some(t("youtube.error.noBrowser"));
            cx.notify();
            return;
        }
        if self.youtube.sign_in_form.is_none() {
            self.youtube.sign_in_form = Some(crate::youtube_ui::SignInForm::new());
        }
        cx.notify();
    }

    fn start_youtube_sign_in(&mut self, cx: &mut Context<Self>) {
        if self.youtube.signing_in {
            return;
        }
        let Some(form) = self.youtube.sign_in_form.as_mut() else {
            return;
        };
        if !form.can_submit() {
            cx.notify();
            return;
        }
        form.busy = true;
        form.error = None;
        form.cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cancel = std::sync::Arc::clone(&form.cancel);

        let directory = self.youtube.directory.clone();
        let now = youtube::now_unix();
        self.youtube.signing_in = true;
        self.youtube.notice = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn(
                    async move { crate::youtube_ui::sign_in(&directory, now, &cancel) },
                )
                .await;

            let _ = this.update(cx, |this, cx| {
                this.youtube.signing_in = false;
                if let Some(form) = this.youtube.sign_in_form.as_mut() {
                    form.busy = false;
                }
                match outcome {
                    Ok((account, profile, avatar)) => {
                        let id = account.id.clone();
                        if let Some(bytes) = avatar {
                            this.youtube.store_avatar(&id, &bytes);
                        }

                        if let Err(error) =
                            crate::youtube_ui::adopt_profile(&this.youtube.directory, &profile, &id)
                        {
                            this.youtube.notice = Some(error.to_string());
                            cx.notify();
                            return;
                        }
                        this.youtube.accounts.upsert(account);
                        this.youtube.accounts.select(&id);
                        this.youtube.persist();
                        this.youtube.sign_in_form = None;
                        this.youtube.notice = Some(t("youtube.accounts.added"));

                        if let Some(source) = this.youtube.pending_publish.take() {
                            this.begin_publish(source, cx);
                        }
                    }
                    Err(failure) => match this.youtube.sign_in_form.as_mut() {
                        Some(form) => form.blame(&failure),
                        None => {
                            this.youtube.notice = Some(crate::youtube_ui::failure_message(&failure))
                        }
                    },
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn queue_youtube_upload(&mut self, cx: &mut Context<Self>) {
        let now = youtube::now_unix();
        let Some(form) = self.youtube.form.as_mut() else {
            return;
        };
        if !form.validate(now) {
            cx.notify();
            return;
        }
        let settings = form.settings();
        let source = form.source.clone();

        if self.youtube.enqueue(settings, source, now).is_some() {
            self.youtube.form = None;
            self.pump_youtube_queue(cx);
        }
        cx.notify();
    }

    fn pump_youtube_queue(&mut self, cx: &mut Context<Self>) {
        while self.youtube.running.len() < YOUTUBE_UPLOAD_SLOTS {
            if !self.start_one_upload(cx) {
                break;
            }
        }
    }

    fn start_one_upload(&mut self, cx: &mut Context<Self>) -> bool {
        let running: Vec<String> = self
            .youtube
            .running
            .iter()
            .map(|running| running.task_id.clone())
            .collect();
        let busy_accounts: Vec<String> = self
            .youtube
            .running
            .iter()
            .filter_map(|running| {
                self.youtube
                    .queue
                    .get(&running.task_id)
                    .map(|task| task.account_id.clone())
            })
            .collect();
        let Some(task) = self
            .youtube
            .queue
            .visible()
            .into_iter()
            .find(|task| {
                matches!(task.state, youtube::TaskState::Waiting)
                    && !running.contains(&task.id)
                    && !busy_accounts.contains(&task.account_id)
            })
            .cloned()
        else {
            return false;
        };
        if !self.youtube.is_configured() {
            self.youtube
                .queue
                .fail(&task.id, &youtube::Failure::NoBrowser);
            self.youtube.save_queue();
            cx.notify();
            return true;
        }

        if !task.source_is_ready() {
            self.youtube.queue.fail(
                &task.id,
                &youtube::Failure::Io(task.source.display().to_string()),
            );
            self.youtube.save_queue();
            cx.notify();
            return true;
        }

        let job = std::sync::Arc::new(std::sync::Mutex::new(
            crate::youtube_ui::UploadJob::default(),
        ));
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let directory = self.youtube.directory.clone();

        self.youtube.queue.start(&task.id);
        self.youtube.running.push(crate::youtube_ui::Running {
            task_id: task.id.clone(),
            job: std::sync::Arc::clone(&job),
            cancel: std::sync::Arc::clone(&cancel),
        });
        self.youtube.save_queue();
        self.watch_youtube_queue(cx);
        cx.notify();

        let worker_job = std::sync::Arc::clone(&job);
        let worker_cancel = std::sync::Arc::clone(&cancel);
        let worker = task.clone();
        let task_id = task.id.clone();
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn(async move {
                    crate::youtube_ui::run_upload(
                        &directory,
                        &worker.account_id,
                        &worker.settings,
                        &worker.source,
                        &worker_job,
                        &worker_cancel,
                    )
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.finish_youtube_task(&task_id, outcome, cx);
            });
        })
        .detach();

        true
    }

    fn finish_youtube_task(
        &mut self,
        task_id: &str,
        outcome: Result<String, youtube::Failure>,
        cx: &mut Context<Self>,
    ) {
        self.youtube.sync_running();
        self.youtube
            .running
            .retain(|running| running.task_id != task_id);
        let Some(task) = self.youtube.queue.get(task_id).cloned() else {
            return;
        };
        let now = youtube::now_unix();

        match outcome {
            Ok(video_id) => {
                let bytes = std::fs::metadata(&task.source)
                    .map(|meta| meta.len())
                    .unwrap_or(task.total);
                self.youtube.queue.finish(task_id, &video_id);
                self.youtube.record(
                    youtube::HistoryEntry {
                        video_id,
                        account_id: task.account_id.clone(),
                        title: task.settings.title.clone(),
                        uploaded_at: now,
                        privacy: task.settings.privacy,
                        status: youtube::UploadStatus::Uploaded,
                        source_file: Some(task.source.display().to_string()),
                        scheduled_for: task.settings.publish_at.clone(),
                        bytes,
                    },
                    now,
                );

                self.youtube.queue.remove(task_id);
                self.youtube.notice = None;
                if let Some(entry) = self.youtube.history.entries.first().cloned() {
                    self.youtube.session.insert(0, entry);
                }
                self.youtube.published = Some(task.settings.title.clone());
                if let Some(entry) = self.youtube.session.first() {
                    let _ = crate::notify::published(
                        &entry.title,
                        &youtube::publish::watch_url(&entry.video_id),
                    );
                    self.youtube.toasts.push(crate::youtube_ui::Toast {
                        title: entry.title.clone(),
                        url: youtube::publish::watch_url(&entry.video_id),
                        born: std::time::Instant::now(),
                        copied_at: None,
                    });
                }
            }
            Err(failure) => {
                if failure.is_cancelled() {
                    self.youtube.queue.cancel(task_id);
                } else {
                    self.youtube.queue.fail(task_id, &failure);
                }
                if failure.is_auth() {
                    self.youtube.accounts.mark_needs_reauth(&task.account_id);
                }
            }
        }
        self.youtube.persist();
        self.pump_youtube_queue(cx);
        cx.notify();
    }

    fn watch_youtube_queue(&mut self, cx: &mut Context<Self>) {
        if self.youtube_watching {
            return;
        }
        self.youtube_watching = true;
        cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(120))
                .await;
            let done = this
                .update(cx, |this, cx| {
                    if this.youtube.running.is_empty() && !this.youtube.queue.is_busy() {
                        this.youtube_watching = false;
                        this.youtube.save_queue();
                        cx.notify();
                        return true;
                    }
                    this.youtube.sync_running();
                    cx.notify();
                    false
                })
                .unwrap_or(true);
            if done {
                return;
            }
        })
        .detach();
    }

    fn settings_tabs(
        &self,
        group: &'static [(&'static str, &'static str)],
        active: usize,
        colors: Palette,
        pick: fn(&mut Self, usize),
        cx: &mut Context<Self>,
    ) -> Div {
        let tabs = group
            .iter()
            .enumerate()
            .map(|(index, (id, label))| {
                let selected = index == active;
                div()
                    .id(SharedString::from(format!("settings-tab-{id}")))
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .py(px(5.0))
                    .rounded(rem(RADIUS_SM))
                    .cursor_pointer()
                    .text_size(rem(TEXT_XS))
                    .text_color(if selected {
                        colors.foreground
                    } else {
                        colors.muted_foreground
                    })
                    .bg(if selected {
                        opacity(colors.accent, 0.8)
                    } else {
                        opacity(colors.accent, 0.0)
                    })
                    .child(t(label))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        pick(this, index);
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>();
        div().flex().w_full().gap(px(4.0)).children(tabs)
    }

    fn misc_body(&mut self, window: &Window, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        let active = self.misc_tab.min(MISC_TABS.len() - 1);
        let strip = self.settings_tabs(
            MISC_TABS,
            active,
            colors,
            |this, index| {
                this.misc_tab = index;
            },
            cx,
        );

        let content = div().flex().flex_col().w_full().gap(px(10.0)).child(strip);
        let content = self.settings_section(MISC_TABS[active].0, content, colors, window, cx);
        self.scroller("assets-settings", content, cx)
    }

    fn settings_section(
        &mut self,
        tab: &str,
        content: Div,
        colors: Palette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut content = content;
        match tab {
            "project-info" => {
                let name = self.app.read(cx).project_name();
                let fps = self.app.read(cx).fps() as f64;
                let (width, height) = self.app.read(cx).canvas();
                content = content
                    .child(self.group_title(colors, t("settings.name")))
                    .child(self.hint(colors, name))
                    .child(self.group_title(colors, t("settings.frameRate")))
                    .child(
                        div().flex().flex_wrap().w_full().children(
                            effects_ui::FPS_PRESETS
                                .iter()
                                .enumerate()
                                .map(|(index, preset)| {
                                    let selected = (fps - preset).abs() < 0.5;
                                    self.pill(
                                        format!("fps-{preset}"),
                                        format!("{preset:.0} fps"),
                                        selected,
                                        cx,
                                    )
                                    .on_click(cx.listener(
                                        move |this: &mut Self, _, _, cx| {
                                            let value = effects_ui::FPS_PRESETS[index];
                                            this.app.update(cx, |model, cx| {
                                                model.update_settings(cx, |settings| {
                                                    settings.fps = time::FrameRate::new(
                                                        (value * 1000.0).round() as u32,
                                                        1000,
                                                    );
                                                    true
                                                })
                                            });
                                            cx.notify();
                                        },
                                    ))
                                })
                                .collect::<Vec<_>>(),
                        ),
                    )
                    .child(self.group_title(colors, t("settings.aspectRatio")))
                    .child(
                        div().flex().flex_wrap().w_full().children(
                            effects_ui::CANVAS_PRESETS
                                .iter()
                                .enumerate()
                                .map(|(index, (label, preset_width, preset_height))| {
                                    let selected = width as u32 == *preset_width
                                        && height as u32 == *preset_height;
                                    self.pill(
                                        format!("canvas-{label}"),
                                        (*label).to_owned(),
                                        selected,
                                        cx,
                                    )
                                    .on_click(cx.listener(
                                        move |this: &mut Self, _, _, cx| {
                                            let (_, next_width, next_height) =
                                                effects_ui::CANVAS_PRESETS[index];
                                            this.app.update(cx, |model, cx| {
                                                model.update_settings(cx, |settings| {
                                                    settings.canvas_size =
                                                        cutix_project::model::CanvasSize {
                                                            width: next_width,
                                                            height: next_height,
                                                        };
                                                    settings.canvas_size_mode = Some(
                                                    cutix_project::model::CanvasSizeMode::Preset,
                                                );
                                                    true
                                                })
                                            });
                                            cx.notify();
                                        },
                                    ))
                                })
                                .collect::<Vec<_>>(),
                        ),
                    )
                    .child(self.hint(colors, format!("{} × {}", width as u32, height as u32)));
            }
            "background" => {
                let current = match self
                    .app
                    .read(cx)
                    .project
                    .as_ref()
                    .map(|project| project.settings.background.clone())
                {
                    Some(cutix_project::model::Background::Color { color }) => color,
                    _ => String::new(),
                };
                content = content
                    .child(self.group_title(colors, t("settings.background.colors")))
                    .child(
                        div().flex().flex_wrap().w_full().gap(px(6.0)).children(
                            effects_ui::BACKGROUND_COLORS
                                .iter()
                                .enumerate()
                                .map(|(index, hex)| {
                                    let selected = current.eq_ignore_ascii_case(hex);
                                    div()
                                        .id(SharedString::from(format!("bg-{hex}")))
                                        .size(px(36.0))
                                        .rounded(rem(RADIUS_SM))
                                        .border_2()
                                        .border_color(if selected {
                                            colors.primary
                                        } else {
                                            colors.border
                                        })
                                        .bg(crate::theme::parse_hex(hex))
                                        .cursor_pointer()
                                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                            let color = effects_ui::BACKGROUND_COLORS[index];
                                            this.app.update(cx, |model, cx| {
                                                model.update_settings(cx, |settings| {
                                                    settings.background =
                                                        cutix_project::model::Background::Color {
                                                            color: color.to_owned(),
                                                        };
                                                    true
                                                })
                                            });
                                            cx.notify();
                                        }))
                                })
                                .collect::<Vec<_>>(),
                        ),
                    );
            }
            "watermark" => {
                content = self.watermark_settings(content, colors, cx);
            }
            "attributions" => {
                content = self.attributions_settings(content, colors, cx);
            }
            "youtube" => {
                self.youtube.ensure_filters(cx);
                self.ensure_channel_details(cx);
                self.refresh_youtube_statuses(cx);
                let now = youtube::now_unix();
                let body = crate::youtube_ui::render::settings_body(
                    &mut self.youtube,
                    colors,
                    now,
                    window,
                    cx,
                );
                content = content.child(body);
            }
            _ => {
                let locales = crate::shell::translatable_locales();
                let current = self.app.read(cx).locale.clone();
                let dark = self.app.read(cx).dark;
                content = content
                    .child(self.group_title(colors, t("settings.language")))
                    .child(
                        div().flex().flex_wrap().w_full().children(
                            locales
                                .iter()
                                .enumerate()
                                .map(|(index, (code, name))| {
                                    let selected = *code == current;
                                    let locales = locales.clone();
                                    self.pill(format!("locale-{code}"), name.clone(), selected, cx)
                                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                            let code = locales[index].0.clone();
                                            this.app.update(cx, |model, cx| {
                                                model.set_locale(&code, cx)
                                            });
                                            cx.notify();
                                        }))
                                })
                                .collect::<Vec<_>>(),
                        ),
                    )
                    .child(self.group_title(colors, t("settings.theme")))
                    .child(
                        self.pill(
                            "theme-toggle".to_owned(),
                            t(if dark { "theme.dark" } else { "theme.light" }),
                            false,
                            cx,
                        )
                        .on_click(cx.listener(
                            |this: &mut Self, _, _, cx| {
                                this.app.update(cx, |model, cx| model.toggle_theme(cx));
                                cx.notify();
                            },
                        )),
                    )
                    .child(self.group_title(colors, t("settings.videoDirectory")))
                    .child(self.video_directory_row(colors, cx));
            }
        }

        content
    }

    fn pill(
        &mut self,
        id: String,
        label: String,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let colors = self.colors(cx);
        div()
            .id(SharedString::from(format!("pill-{id}")))
            .m(px(3.0))
            .px(px(10.0))
            .py(px(5.0))
            .rounded(rem(RADIUS_SM))
            .border_1()
            .border_color(if selected {
                colors.primary
            } else {
                colors.border
            })
            .bg(if selected {
                opacity(colors.primary, 0.15)
            } else {
                opacity(colors.accent, 0.0)
            })
            .text_size(rem(TEXT_XS))
            .text_color(if selected {
                colors.foreground
            } else {
                colors.muted_foreground
            })
            .cursor_pointer()
            .child(label)
    }

    fn current_watermark(&self, cx: &App) -> watermark::TWatermark {
        self.app
            .read(cx)
            .project
            .as_ref()
            .map(|project| crate::settings_ui::read_watermark(&project.settings))
            .unwrap_or_else(watermark::create_default_watermark)
    }

    fn mutate_watermark(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut watermark::TWatermark),
    ) {
        self.app.update(cx, |model, cx| {
            model.update_settings(cx, |settings| {
                let mut mark = crate::settings_ui::read_watermark(settings);
                change(&mut mark);
                crate::settings_ui::write_watermark(settings, &mark);
                true
            });
        });
        cx.notify();
    }

    fn watermark_scale_row(
        &mut self,
        colors: Palette,
        label: &str,
        id: &'static str,
        current: f64,
        presets: &'static [(f64, &'static str)],
        cx: &mut Context<Self>,
        apply: impl Fn(&mut watermark::TWatermark, f64) + Copy + 'static,
    ) -> Div {
        let pills = presets
            .iter()
            .map(|(value, text)| {
                let value = *value;
                let selected = (current - value).abs() < 1e-6;
                self.pill(format!("{id}-{text}"), (*text).to_owned(), selected, cx)
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.mutate_watermark(cx, move |mark| apply(mark, value));
                    }))
            })
            .collect::<Vec<_>>();
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(self.group_title(colors, label.to_owned()))
            .child(div().flex().flex_wrap().w_full().children(pills))
    }

    fn watermark_settings(
        &mut self,
        mut content: Div,
        colors: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let mark = self.current_watermark(cx);

        content = content
            .child(self.group_title(colors, t("watermark.title")))
            .child(
                self.pill("wm-enabled".into(), t("watermark.enable"), mark.enabled, cx)
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.mutate_watermark(cx, |mark| mark.enabled = !mark.enabled);
                    })),
            )
            .child(self.hint(colors, t("watermark.hint")));

        let images = self
            .sorted_media(cx)
            .into_iter()
            .filter(|asset| asset.media_type == cutix_project::MediaType::Image)
            .collect::<Vec<_>>();
        let current_image = match &mark.source {
            Some(watermark::TWatermarkSource::Image { media_id }) => Some(media_id.clone()),
            _ => None,
        };
        let is_text = matches!(mark.source, Some(watermark::TWatermarkSource::Text { .. }));

        content = content.child(self.group_title(colors, t("watermark.source")));
        if images.is_empty() {
            content = content.child(self.hint(colors, t("watermark.source.image.empty")));
        }
        let mut source_pills = images
            .iter()
            .map(|asset| {
                let id = asset.id.clone();
                let selected = current_image.as_deref() == Some(asset.id.as_str());
                self.pill(
                    format!("wm-img-{}", asset.id),
                    asset.name.clone(),
                    selected,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    let id = id.clone();
                    this.mutate_watermark(cx, move |mark| {
                        mark.source = Some(watermark::TWatermarkSource::Image { media_id: id });
                    });
                }))
            })
            .collect::<Vec<_>>();
        let project_name = self.app.read(cx).project_name();
        source_pills.push(
            self.pill(
                "wm-src-text".into(),
                t("watermark.source.text"),
                is_text,
                cx,
            )
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                let name = project_name.clone();
                this.mutate_watermark(cx, move |mark| {
                    mark.source = Some(watermark::create_default_watermark_text_source(&name));
                });
            })),
        );
        content = content.child(div().flex().flex_wrap().w_full().children(source_pills));

        content = content.child(self.group_title(colors, t("watermark.position")));
        let anchor_cells = watermark::WATERMARK_ANCHORS
            .iter()
            .map(|anchor| {
                let anchor = *anchor;
                let selected = mark.anchor == anchor;
                div()
                    .id(SharedString::from(format!("wm-anchor-{}", anchor.key())))
                    .flex()
                    .items_center()
                    .justify_center()
                    .h(px(30.0))
                    .rounded(rem(RADIUS_SM))
                    .border_1()
                    .border_color(if selected {
                        colors.primary
                    } else {
                        colors.border
                    })
                    .bg(if selected {
                        opacity(colors.primary, 0.15)
                    } else {
                        opacity(colors.accent, 0.0)
                    })
                    .cursor_pointer()
                    .child(div().size(px(8.0)).rounded_full().bg(if selected {
                        colors.foreground
                    } else {
                        opacity(colors.muted_foreground, 0.4)
                    }))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.mutate_watermark(cx, move |mark| mark.anchor = anchor);
                    }))
            })
            .collect::<Vec<_>>();
        content = content.child(
            div()
                .grid()
                .grid_cols(3)
                .gap(px(4.0))
                .w_full()
                .children(anchor_cells),
        );
        content = content.child(self.watermark_scale_row(
            colors,
            &t("watermark.offset.x"),
            "wm-margin",
            mark.offset.x,
            &[(0.0, "0%"), (0.03, "3%"), (0.05, "5%"), (0.08, "8%")],
            cx,
            |mark, value| {
                mark.offset.x = value;
                mark.offset.y = value;
            },
        ));
        content = content.child(self.hint(colors, t("watermark.offset.hint")));

        content = content
            .child(self.watermark_scale_row(
                colors,
                &t("watermark.size"),
                "wm-size",
                mark.size,
                &[(0.1, "10%"), (0.18, "18%"), (0.25, "25%"), (0.4, "40%")],
                cx,
                |mark, value| mark.size = value,
            ))
            .child(self.watermark_scale_row(
                colors,
                &t("watermark.opacity"),
                "wm-opacity",
                mark.opacity,
                &[(0.3, "30%"), (0.5, "50%"), (0.7, "70%"), (1.0, "100%")],
                cx,
                |mark, value| mark.opacity = value,
            ))
            .child(self.watermark_scale_row(
                colors,
                &t("watermark.rotation"),
                "wm-rotation",
                mark.rotation,
                &[(0.0, "0"), (-15.0, "-15"), (-30.0, "-30"), (-45.0, "-45")],
                cx,
                |mark, value| mark.rotation = value,
            ));

        content = content.child(self.group_title(colors, t("watermark.blend")));
        let blend_pills = watermark::WATERMARK_BLEND_MODES
            .iter()
            .map(|mode| {
                let mode = *mode;
                let selected = mark.blend_mode == mode;
                self.pill(
                    format!("wm-blend-{}", mode.key()),
                    t(&format!("watermark.blend.{}", mode.key())),
                    selected,
                    cx,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.mutate_watermark(cx, move |mark| mark.blend_mode = mode);
                }))
            })
            .collect::<Vec<_>>();
        content = content.child(div().flex().flex_wrap().w_full().children(blend_pills));

        content = content
            .child(self.group_title(colors, t("watermark.tiling")))
            .child(
                self.pill(
                    "wm-tiling".into(),
                    t("watermark.tiling.enable"),
                    mark.tiling.enabled,
                    cx,
                )
                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                    this.mutate_watermark(cx, |mark| {
                        mark.tiling.enabled = !mark.tiling.enabled;
                    });
                })),
            );
        if mark.tiling.enabled {
            content = content
                .child(self.watermark_scale_row(
                    colors,
                    &t("watermark.tiling.spacing"),
                    "wm-tile-spacing",
                    mark.tiling.spacing,
                    &[(0.2, "20%"), (0.6, "60%"), (1.0, "100%"), (2.0, "200%")],
                    cx,
                    |mark, value| mark.tiling.spacing = value,
                ))
                .child(self.watermark_scale_row(
                    colors,
                    &t("watermark.tiling.angle"),
                    "wm-tile-angle",
                    mark.tiling.angle,
                    &[(0.0, "0"), (15.0, "15"), (30.0, "30"), (45.0, "45")],
                    cx,
                    |mark, value| mark.tiling.angle = value,
                ))
                .child(self.hint(colors, t("watermark.tiling.hint")));
        }

        content = content.child(self.group_title(colors, t("watermark.timing")));
        let always = mark.timing.mode == watermark::WatermarkTimingMode::Always;
        content = content.child(
            div()
                .flex()
                .flex_wrap()
                .w_full()
                .child(
                    self.pill(
                        "wm-timing-always".into(),
                        t("watermark.timing.always"),
                        always,
                        cx,
                    )
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.mutate_watermark(cx, |mark| {
                            mark.timing.mode = watermark::WatermarkTimingMode::Always;
                        });
                    })),
                )
                .child(
                    self.pill(
                        "wm-timing-range".into(),
                        t("watermark.timing.range"),
                        !always,
                        cx,
                    )
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.mutate_watermark(cx, |mark| {
                            mark.timing.mode = watermark::WatermarkTimingMode::Range;
                        });
                    })),
                ),
        );

        content
    }

    fn attributions_settings(
        &mut self,
        mut content: Div,
        colors: Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let entries = self
            .app
            .read(cx)
            .project
            .as_ref()
            .map(crate::settings_ui::collect_attributions)
            .unwrap_or_default();

        content = content.child(self.group_title(colors, t("settings.attributions")));
        if entries.is_empty() {
            return content.child(self.hint(colors, t("settings.attributions.empty")));
        }
        content = content.child(self.hint(colors, t("settings.attributions.hint")));

        for entry in entries {
            let credit = crate::settings_ui::attribution_credit_line(&entry);
            let mut card = div()
                .flex()
                .flex_col()
                .w_full()
                .gap(px(2.0))
                .px(px(8.0))
                .py(px(6.0))
                .rounded(rem(RADIUS_SM))
                .border_1()
                .border_color(colors.border)
                .child(
                    div()
                        .text_size(rem(TEXT_SM))
                        .text_color(colors.foreground)
                        .child(entry.title.clone()),
                );
            if !entry.creator.is_empty() {
                card = card.child(
                    div()
                        .text_size(rem(TEXT_XS))
                        .text_color(colors.muted_foreground)
                        .child(entry.creator.clone()),
                );
            }
            card = card.child(
                div()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(credit),
            );
            if let Some(source) = entry.source_url.clone() {
                card = card.child(
                    div()
                        .text_size(rem(TEXT_XS))
                        .text_color(opacity(colors.muted_foreground, 0.8))
                        .child(source),
                );
            }
            content = content.child(card);
        }

        content
    }

    fn body(&mut self, window: &Window, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        let key = ASSET_TABS[self.active].0;

        match key {
            "effects" => return self.effects_body(cx),
            "adjustment" => return self.adjustment_body(cx),
            "transitions" => return self.transitions_body(cx),
            "settings" => return self.misc_body(window, cx),
            "stickers" => return self.stickers_body(window, cx),
            "templates" => return self.templates_body(cx),
            "captions" => return self.captions_body(cx),
            "speech" => return self.speech_body(window, cx),
            "sounds" => return self.sounds_body(window, cx),
            _ => {}
        }

        if key == "media" {
            let media = self.sorted_media(cx);
            if media.is_empty() {
                return self.dropzone(cx);
            }

            let grid = self.grid_view;
            let cards = media
                .iter()
                .map(|asset| self.media_card(asset, cx))
                .collect::<Vec<_>>();
            let importing = self.app.read(cx).importing;
            let notice = self.app.read(cx).notice.clone();
            let bar = scrollbar_v(&self.body_scroll, colors);

            return div()
                .relative()
                .flex()
                .flex_1()
                .w_full()
                .min_h_0()
                .child(
                    div()
                        .id("assets-media")
                        .flex()
                        .flex_col()
                        .size_full()
                        .p(px(4.0))
                        .overflow_y_scroll()
                        .track_scroll(&self.body_scroll)
                        .on_drop(
                            cx.listener(|this: &mut Self, paths: &ExternalPaths, _, cx| {
                                let paths: Vec<PathBuf> = paths.paths().to_vec();
                                this.app
                                    .update(cx, |model, cx| model.import_media(paths, cx));
                            }),
                        )
                        .when(importing > 0, |this| {
                            this.child(
                                div()
                                    .w_full()
                                    .px(px(8.0))
                                    .py(px(6.0))
                                    .text_size(rem(TEXT_XS))
                                    .text_color(colors.muted_foreground)
                                    .child(t("common.loading")),
                            )
                        })
                        .when_some(notice, |this, message| {
                            this.child(
                                div()
                                    .w_full()
                                    .px(px(8.0))
                                    .py(px(6.0))
                                    .text_size(rem(TEXT_XS))
                                    .text_color(colors.destructive)
                                    .child(message),
                            )
                        })
                        .child(
                            div()
                                .flex()
                                .w_full()
                                .when(grid, |this| this.flex_wrap())
                                .when(!grid, |this| this.flex_col())
                                .children(cards),
                        ),
                )
                .children(bar);
        }

        if key == "text" {
            return self.text_presets(cx);
        }

        let glyph = ASSET_VIEW_ICONS
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, glyph)| *glyph)
            .unwrap_or("folder03");

        div()
            .flex()
            .flex_col()
            .flex_1()
            .w_full()
            .min_h_0()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .p(px(16.0))
            .child(
                svg()
                    .size(px(40.0))
                    .path(icon(glyph))
                    .text_color(opacity(colors.muted_foreground, 0.75)),
            )
            .child(
                div()
                    .text_size(rem(TEXT_LG))
                    .font_weight(FontWeight::MEDIUM)
                    .child(t("editor.assets.empty")),
            )
            .child(
                div()
                    .text_size(rem(TEXT_SM))
                    .text_color(colors.muted_foreground)
                    .text_center()
                    .child(t(&format!("editor.tab.{key}"))),
            )
    }
}

impl Hoverable for AssetsPanel {
    fn transitions(&mut self) -> &mut Transitions {
        &mut self.transitions
    }

    fn tooltips(&mut self) -> &mut Tooltips {
        &mut self.tooltips
    }
}

impl crate::youtube_ui::render::Host for AssetsPanel {
    fn youtube(&mut self) -> &mut crate::youtube_ui::Youtube {
        &mut self.youtube
    }

    fn youtube_action(
        &mut self,
        action: crate::youtube_ui::render::Action,
        cx: &mut Context<Self>,
    ) {
        use crate::youtube_ui::render::Action;
        match action {
            Action::StartSetup | Action::AddAccount => {
                self.youtube.notice = None;
                self.open_youtube_sign_in(cx);
            }
            Action::SubmitSignIn => self.start_youtube_sign_in(cx),
            Action::Publish => self.queue_youtube_upload(cx),
            Action::CancelTask(id) => {
                match self
                    .youtube
                    .running
                    .iter()
                    .find(|running| running.task_id == id)
                {
                    Some(running) => running.stop(),
                    _ => {
                        self.youtube.queue.cancel(&id);
                        self.youtube.save_queue();
                    }
                }
            }
            Action::RetryTask(id) => {
                if self.youtube.queue.retry(&id) {
                    self.youtube.notice = None;
                    self.youtube.save_queue();
                    self.pump_youtube_queue(cx);
                }
            }
            Action::ForgetTask(id) => {
                if self.youtube.queue.remove(&id) {
                    self.youtube.save_queue();
                }
            }
            Action::ChooseAccount(id) => self.choose_publish_account(id, cx),
            Action::DismissPublished => self.youtube.published = None,

            Action::CloseSession => {
                self.youtube.should_close = true;
                self.youtube.published = None;
                self.youtube.session.clear();
                self.youtube.notice = None;
                let settled: Vec<String> = self
                    .youtube
                    .queue
                    .visible()
                    .into_iter()
                    .filter(|task| !task.state.is_active())
                    .map(|task| task.id.clone())
                    .collect();
                for id in settled {
                    self.youtube.queue.remove(&id);
                }
                self.youtube.persist();
            }
            Action::OpenVideo(id) => {
                crate::youtube_ui::open_url(&youtube::publish::watch_url(&id));
            }
            Action::TogglePreview => {
                if let Some(form) = self.youtube.form.as_mut() {
                    if !form.preview_playing && form.preview_position >= form.preview_duration {
                        form.preview_position = 0.0;
                    }
                    form.preview_playing = !form.preview_playing;
                    form.preview_stepped_at = std::time::Instant::now();
                }
            }
            Action::SeekPreview(x) => {
                let (left, width) = self.youtube.preview_bar;
                if width > 0.0 {
                    if let Some(form) = self.youtube.form.as_mut() {
                        let fraction = ((x - left) / width).clamp(0.0, 1.0) as f64;
                        form.preview_position = fraction * form.preview_duration;
                        form.preview_shown = None;
                        form.preview_stepped_at = std::time::Instant::now();
                    }
                }
            }
            Action::ToggleSound => {
                self.youtube.sound.toggle();
                self.apply_publish_sound(cx);
            }
            Action::SetVolume(x) => {
                let (left, width) = self.youtube.volume_bar;
                if width > 0.0 {
                    let level = ((x - left) / width).clamp(0.0, 1.0);
                    self.youtube.sound.set_volume(level);
                    self.apply_publish_sound(cx);
                }
            }
            Action::HoverVolume(over) => {
                self.youtube.volume_open = over;
            }
            Action::Dismiss => {
                self.youtube.should_close =
                    self.youtube.running.is_empty() && self.youtube.session.is_empty();
                self.youtube.form = None;
                self.youtube.notice = None;

                self.youtube.choosing_account = false;
                self.youtube.pending_publish = None;
            }
        }
        cx.notify();
    }
}

impl Render for AssetsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        let active = self.active;

        let tabs = ASSET_TABS
            .iter()
            .enumerate()
            .map(|(index, (key, name))| {
                let selected = index == active;
                let tint = if selected {
                    colors.foreground
                } else {
                    colors.muted_foreground
                };
                let rest = if selected {
                    opacity(colors.accent, 0.4)
                } else {
                    opacity(colors.accent, 0.0)
                };
                let progress = self.transitions.eased(&format!("tab-{key}"));
                let id = *key;

                div()
                    .id(SharedString::from(format!("assets-tab-{key}")))
                    .flex()
                    .flex_col()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .gap(px(2.0))
                    .rounded(rem(RADIUS_SM))
                    .px(px(8.0))
                    .py(px(6.0))
                    .cursor_pointer()
                    .text_color(tint)
                    .bg(mix(rest, colors.accent, progress))
                    .on_hover(cx.listener(move |this: &mut Self, hovered, _, cx| {
                        this.transitions.set(format!("tab-{id}"), *hovered);
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.active = index;
                        this.reveal_tab(index);
                        cx.notify();
                    }))
                    .child(svg().size(rem(1.15)).path(icon(name)).text_color(tint))
                    .child(
                        div()
                            .text_size(rem(TEXT_TAB_LABEL))
                            .line_height(rem(TEXT_TAB_LABEL))
                            .font_weight(FontWeight::MEDIUM)
                            .child(t(&tab_label_key(key))),
                    )
            })
            .collect::<Vec<_>>();

        let list_progress = self.transitions.eased("assets-list");
        let sort_progress = self.transitions.eased("assets-sort");
        let import_progress = self.transitions.eased("assets-import");

        self.tooltips.tick();
        if self.transitions.animating()
            || self.tooltips.animating()
            || self.job.is_some()
            || self.sound_loading
            || self.youtube.needs_repaint()
        {
            window.request_animation_frame();
        }

        let end_fade_progress = self.transitions.eased("assets-tabs-end");
        let start_fade_progress = self.transitions.eased("assets-tabs-start");
        self.keep_active_tab_visible();
        let (show_start_fade, show_end_fade) = self.tab_overflow();
        let list_tip = self.tooltips.frame_for("assets-list");
        let sort_tip = self.tooltips.frame_for("assets-sort");
        let sort_label = cutix_i18n::t_args(
            "assets.sort.tooltip",
            &[
                ("field", &t("common.name")),
                (
                    "order",
                    &t(if self.sort_descending {
                        "common.descending"
                    } else {
                        "common.ascending"
                    }),
                ),
            ],
        );
        let grid_view = self.grid_view;
        let sort_descending = self.sort_descending;
        let body = self.body(window, cx);

        panel_frame(colors)
            .on_drag_move::<AssetSliderDrag>(cx.listener(Self::on_param_slider))
            .child(
                div()
                    .relative()
                    .w_full()
                    .flex_shrink_0()
                    .child(
                        div()
                            .id("assets-tabs")
                            .flex()
                            .w_full()
                            .items_end()
                            .gap(px(4.0))
                            .overflow_x_scroll()
                            .track_scroll(&self.tabs_scroll)
                            .px(px(8.0))
                            .py(px(4.0))
                            .children(tabs),
                    )
                    .when(show_start_fade, |this| {
                        this.child(
                            div()
                                .id("assets-tabs-start")
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left_0()
                                .w(px(TAB_SCROLL_ARROW_WIDTH_PX))
                                .flex()
                                .items_center()
                                .justify_center()
                                .pr(px(4.0))
                                .cursor_pointer()
                                .bg(linear_gradient(
                                    90.0,
                                    linear_color_stop(colors.background, 0.0),
                                    linear_color_stop(opacity(colors.background, 0.0), 1.0),
                                ))
                                .on_hover(hover_listener("assets-tabs-start", cx))
                                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                    this.nudge_tabs(-1.0);
                                    cx.notify();
                                }))
                                .child(tab_scroll_button(
                                    colors,
                                    "chevron-left",
                                    start_fade_progress,
                                )),
                        )
                    })
                    .when(show_end_fade, |this| {
                        this.child(
                            div()
                                .id("assets-tabs-end")
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .right_0()
                                .w(px(TAB_SCROLL_ARROW_WIDTH_PX))
                                .flex()
                                .items_center()
                                .justify_center()
                                .pl(px(4.0))
                                .cursor_pointer()
                                .bg(linear_gradient(
                                    270.0,
                                    linear_color_stop(colors.background, 0.0),
                                    linear_color_stop(opacity(colors.background, 0.0), 1.0),
                                ))
                                .on_hover(hover_listener("assets-tabs-end", cx))
                                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                    this.nudge_tabs(1.0);
                                    cx.notify();
                                }))
                                .child(tab_scroll_button(
                                    colors,
                                    "chevron-right",
                                    end_fade_progress,
                                )),
                        )
                    }),
            )
            .child(div().h(px(1.0)).w_full().flex_shrink_0().bg(colors.border))
            .child(
                div()
                    .flex()
                    .w_full()
                    .h(px(PANEL_VIEW_HEADER_HEIGHT))
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(colors.border)
                    .pl(px(12.0))
                    .pr(px(8.0))
                    .child(
                        div()
                            .text_size(rem(TEXT_SM))
                            .text_color(colors.muted_foreground)
                            .child(t(&tab_label_key(ASSET_TABS[active].0))),
                    )
                    .when(ASSET_TABS[active].0 == "media", |header| {
                        header.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(tooltipped(
                                    ghost_button(
                                        "assets-list",
                                        if grid_view {
                                            "grid-view-alt"
                                        } else {
                                            "left-to-right-list-dash"
                                        },
                                        colors,
                                        list_progress,
                                    )
                                    .on_hover(hover_listener("assets-list", cx))
                                    .on_mouse_down(gpui::MouseButton::Left, press_listener(cx))
                                    .on_click(cx.listener(
                                        |this: &mut Self, _, _, cx| {
                                            this.grid_view = !this.grid_view;
                                            cx.notify();
                                        },
                                    )),
                                    colors,
                                    t(if grid_view {
                                        "assets.view.switchToList"
                                    } else {
                                        "assets.view.switchToGrid"
                                    }),
                                    list_tip,
                                    OverlaySide::Bottom,
                                ))
                                .child(tooltipped(
                                    ghost_button(
                                        "assets-sort",
                                        if sort_descending {
                                            "sorting-nine-one"
                                        } else {
                                            "sorting-one-nine"
                                        },
                                        colors,
                                        sort_progress,
                                    )
                                    .on_hover(hover_listener("assets-sort", cx))
                                    .on_mouse_down(gpui::MouseButton::Left, press_listener(cx))
                                    .on_click(cx.listener(
                                        |this: &mut Self, _, _, cx| {
                                            this.sort_descending = !this.sort_descending;
                                            cx.notify();
                                        },
                                    )),
                                    colors,
                                    sort_label,
                                    sort_tip,
                                    OverlaySide::Bottom,
                                ))
                                .child(
                                    Button::new("assets-import", colors)
                                        .variant(ButtonVariant::Outline)
                                        .size(ButtonSize::Sm)
                                        .hover(import_progress)
                                        .icon("cloud-upload")
                                        .label(t("common.import"))
                                        .build()
                                        .on_hover(hover_listener("assets-import", cx))
                                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                            this.pick_files(cx)
                                        })),
                                ),
                        )
                    }),
            )
            .child(body)
    }
}

#[derive(Clone, Debug)]
pub struct CanvasMove {
    id: String,
}

#[derive(Clone, Debug)]
pub struct CanvasScale {
    id: String,
    corner: (f32, f32),
}

#[derive(Clone, Debug)]
struct CanvasGesture {
    start: gpui::Point<gpui::Pixels>,
    position: (f64, f64),
    scale: (f64, f64),
    size: (f32, f32),
}

#[derive(Clone, Debug)]
pub struct LayerMove {
    id: String,
    target: LayerTarget,
}

#[derive(Clone, Debug)]
pub struct LayerResize {
    id: String,
    target: LayerTarget,
    corner: (f32, f32),
}

#[derive(Clone, Debug)]
pub struct LayerRotate {
    id: String,
}

#[derive(Clone, Debug)]
pub struct CanvasRotate {
    id: String,
}

#[derive(Clone, Debug)]
pub struct CanvasCrop {
    id: String,
    handle: gizmos::CropHandle,
}

#[derive(Clone, Debug)]
pub struct GuideLineDrag {
    index: usize,
}

#[derive(Clone, Copy, Debug)]
struct RotateGesture {
    rotation: f64,
    angle: f64,
}

#[derive(Clone, Debug)]
struct CropGesture {
    crop: cutix_project::Crop,
    full: gizmos::FullBounds,
    rotation_degrees: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayerTarget {
    Mask,
    Tracking,
}

#[derive(Clone, Debug)]
struct LayerGesture {
    start: gpui::Point<gpui::Pixels>,
    rect: cutix_playback::ElementRect,

    scale: f32,

    box_params: (f64, f64, f64, f64, f64),
}

const LAYER_ROTATE_DEGREES_PER_PX: f64 = 0.5;
const LAYER_HANDLE_PX: f32 = 10.0;

pub struct PreviewPanel {
    app: Entity<AppModel>,
    tooltips: Tooltips,
    transitions: Transitions,
    viewport: (f32, f32),
    zoom_menu: Overlay,
    zoom_percent: Option<u32>,
    gesture: Option<CanvasGesture>,

    mask_gesture: Option<LayerGesture>,
    tracking_gesture: Option<LayerGesture>,
    canvas_scale: f32,
    rotate_gesture: Option<RotateGesture>,
    crop_gesture: Option<CropGesture>,
    snap_lines: Vec<gizmos::SnapLine>,
    active_guide: Option<SharedString>,
    grid: gizmos::GridConfig,
    custom_lines: Vec<gizmos::CustomLine>,
    guide_menu: Overlay,

    scene_origin: gpui::Point<gpui::Pixels>,
    scene: (f32, f32),
    frame_scale: f32,

    progress_bounds: (f32, f32),

    progress_hover: Option<f32>,
    scrubbing: bool,

    controls_activity: Option<Instant>,
    controls_hover: bool,
}

impl PreviewPanel {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self {
            app,
            tooltips: Tooltips::new(TOOLBAR_TOOLTIP_DELAY),
            transitions: Transitions::new(),
            viewport: (0.0, 0.0),
            zoom_menu: Overlay::new(OverlaySide::Top),
            zoom_percent: None,
            gesture: None,
            mask_gesture: None,
            tracking_gesture: None,
            canvas_scale: 1.0,
            rotate_gesture: None,
            crop_gesture: None,
            snap_lines: Vec::new(),
            active_guide: None,
            grid: gizmos::GridConfig::default(),
            custom_lines: Vec::new(),
            guide_menu: Overlay::new(OverlaySide::Top),
            scene_origin: gpui::point(px(0.0), px(0.0)),
            scene: (0.0, 0.0),
            frame_scale: 1.0,
            progress_bounds: (0.0, 0.0),
            progress_hover: None,
            scrubbing: false,
            controls_activity: None,
            controls_hover: false,
        }
    }

    fn colors(&self, cx: &App) -> Palette {
        self.app.read(cx).theme.panel
    }

    fn canvas(&self, cx: &App) -> (f32, f32) {
        self.app.read(cx).canvas()
    }

    pub fn dismiss_overlays(&mut self, cx: &mut Context<Self>) {
        let mut dismissed = false;
        if self.zoom_menu.is_open() {
            self.zoom_menu.dismiss();
            dismissed = true;
        }
        if self.guide_menu.is_open() {
            self.guide_menu.dismiss();
            dismissed = true;
        }
        if dismissed {
            cx.notify();
        }
    }

    fn tracking_overlay(
        &mut self,
        scene: (f32, f32),
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let model = self.app.read(cx);
        let element = model.selected_element()?;
        if !matches!(element, cutix_project::TimelineElement::Video(_)) {
            return None;
        }
        if model
            .properties_tabs
            .get(crate::properties::element_type_key(element))
            .map(String::as_str)
            != Some("tracking")
        {
            return None;
        }
        let id = element.base().id.clone();
        let region = model.tracking_region;
        let rect = self.selected_rect(&id, cx)?;
        let colors = self.colors(cx);

        self.layer_box(
            LayerTarget::Tracking,
            &id,
            rect,
            scene,
            (
                (region.x + region.width / 2.0 - 0.5) as f64,
                (region.y + region.height / 2.0 - 0.5) as f64,
                region.width as f64,
                region.height as f64,
                0.0,
            ),
            colors.destructive,
            true,
            cx,
        )
    }

    fn selected_rect(&self, id: &str, cx: &App) -> Option<cutix_playback::ElementRect> {
        let model = self.app.read(cx);
        if model.preview.frame_size.0 == 0 {
            return None;
        }
        model
            .preview
            .rects
            .iter()
            .find(|(candidate, _)| candidate == id)
            .map(|(_, rect)| *rect)
    }

    fn mask_overlay(&mut self, scene: (f32, f32), cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let model = self.app.read(cx);
        let element = model.selected_element()?;
        if model
            .properties_tabs
            .get(crate::properties::element_type_key(element))
            .map(String::as_str)
            != Some("mask")
        {
            return None;
        }
        let mask = edit::mask_of(element)?;
        let id = element.base().id.clone();
        let number = |key: &str, fallback: f64| {
            mask.params
                .get(key)
                .and_then(serde_json::Value::as_f64)
                .filter(|value| value.is_finite())
                .unwrap_or(fallback)
        };
        let params = (
            number("centerX", 0.0),
            number("centerY", 0.0),
            number("width", 0.6),
            number("height", 0.6),
            number("rotation", 0.0),
        );
        let rect = self.selected_rect(&id, cx)?;
        let colors = self.colors(cx);
        self.layer_box(
            LayerTarget::Mask,
            &id,
            rect,
            scene,
            params,
            colors.primary,
            false,
            cx,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn layer_box(
        &mut self,
        target: LayerTarget,
        id: &str,
        rect: cutix_playback::ElementRect,
        scene: (f32, f32),
        params: (f64, f64, f64, f64, f64),
        color: gpui::Hsla,
        dashed: bool,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let frame_width = self.app.read(cx).preview.frame_size.0;
        if frame_width == 0 {
            return None;
        }
        let scale = scene.0 / frame_width as f32;
        let colors = self.colors(cx);
        let (center_x, center_y, box_width, box_height, _) = params;
        let (left, top, width, height) = rect.local_bounds(
            center_x as f32,
            center_y as f32,
            box_width as f32,
            box_height as f32,
        );
        let (left, top, width, height) = (
            left * scale,
            top * scale,
            (width * scale).max(2.0),
            (height * scale).max(2.0),
        );

        let corners = [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)];
        let mut children: Vec<gpui::AnyElement> = corners
            .into_iter()
            .map(|corner| {
                let x = if corner.0 < 0.0 {
                    -LAYER_HANDLE_PX / 2.0
                } else {
                    width - LAYER_HANDLE_PX / 2.0
                };
                let y = if corner.1 < 0.0 {
                    -LAYER_HANDLE_PX / 2.0
                } else {
                    height - LAYER_HANDLE_PX / 2.0
                };
                div()
                    .id(SharedString::from(format!(
                        "layer-handle-{target:?}-{id}-{}-{}",
                        corner.0, corner.1
                    )))
                    .absolute()
                    .left(px(x))
                    .top(px(y))
                    .size(px(LAYER_HANDLE_PX))
                    .rounded(px(2.0))
                    .border_1()
                    .border_color(color)
                    .bg(colors.background)
                    .cursor(gpui::CursorStyle::ResizeUpLeftDownRight)
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(
                            move |this: &mut Self, event: &gpui::MouseDownEvent, _, cx| {
                                this.begin_layer_gesture(
                                    target,
                                    event.position,
                                    rect,
                                    scale,
                                    params,
                                );
                                cx.notify();
                            },
                        ),
                    )
                    .on_drag(
                        LayerResize {
                            id: id.to_owned(),
                            target,
                            corner,
                        },
                        |_, _, _, cx| cx.new(|_| gpui::Empty),
                    )
                    .into_any_element()
            })
            .collect();

        if target == LayerTarget::Mask {
            children.push(
                div()
                    .id(SharedString::from(format!("layer-rotate-{id}")))
                    .absolute()
                    .left(px(width / 2.0 - LAYER_HANDLE_PX / 2.0))
                    .top(px(-LAYER_HANDLE_PX * 2.0))
                    .size(px(LAYER_HANDLE_PX))
                    .rounded_full()
                    .border_1()
                    .border_color(color)
                    .bg(colors.background)
                    .cursor(gpui::CursorStyle::OpenHand)
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(
                            move |this: &mut Self, event: &gpui::MouseDownEvent, _, cx| {
                                this.begin_layer_gesture(
                                    target,
                                    event.position,
                                    rect,
                                    scale,
                                    params,
                                );
                                cx.notify();
                            },
                        ),
                    )
                    .on_drag(LayerRotate { id: id.to_owned() }, |_, _, _, cx| {
                        cx.new(|_| gpui::Empty)
                    })
                    .into_any_element(),
            );
        }

        Some(
            div()
                .id(SharedString::from(format!("layer-box-{target:?}-{id}")))
                .absolute()
                .left(px(left))
                .top(px(top))
                .w(px(width))
                .h(px(height))
                .border_2()
                .when(dashed, |this| this.border_dashed())
                .border_color(color)
                .cursor(gpui::CursorStyle::OpenHand)
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(
                        move |this: &mut Self, event: &gpui::MouseDownEvent, _, cx| {
                            this.begin_layer_gesture(target, event.position, rect, scale, params);
                            cx.notify();
                        },
                    ),
                )
                .on_drag(
                    LayerMove {
                        id: id.to_owned(),
                        target,
                    },
                    |_, _, _, cx| cx.new(|_| gpui::Empty),
                )
                .children(children),
        )
    }

    fn gesture_slot(&mut self, target: LayerTarget) -> &mut Option<LayerGesture> {
        match target {
            LayerTarget::Mask => &mut self.mask_gesture,
            LayerTarget::Tracking => &mut self.tracking_gesture,
        }
    }

    fn begin_layer_gesture(
        &mut self,
        target: LayerTarget,
        start: gpui::Point<gpui::Pixels>,
        rect: cutix_playback::ElementRect,
        scale: f32,
        box_params: (f64, f64, f64, f64, f64),
    ) {
        *self.gesture_slot(target) = Some(LayerGesture {
            start,
            rect,
            scale,
            box_params,
        });
    }

    fn layer_delta(gesture: &LayerGesture, position: gpui::Point<gpui::Pixels>) -> (f64, f64) {
        let scale = gesture.scale.max(0.0001);
        let dx = f32::from(position.x - gesture.start.x) / scale;
        let dy = f32::from(position.y - gesture.start.y) / scale;
        let (du, dv) = gesture.rect.local_delta(dx, dy);
        (du as f64, dv as f64)
    }

    fn apply_layer_box(
        &mut self,
        id: &str,
        target: LayerTarget,
        params: (f64, f64, f64, f64, f64),
        cx: &mut Context<Self>,
    ) {
        match target {
            LayerTarget::Mask => {
                let key = format!("mask-box-{id}");
                let id = id.to_owned();
                self.app.update(cx, |model, cx| {
                    model.edit_coalesced(Some(key), cx, |editor| {
                        let mut changed =
                            editor.set_mask_param(&id, "centerX", serde_json::json!(params.0));
                        changed |=
                            editor.set_mask_param(&id, "centerY", serde_json::json!(params.1));
                        changed |= editor.set_mask_param(
                            &id,
                            "width",
                            serde_json::json!(params.2.max(0.01)),
                        );
                        changed |= editor.set_mask_param(
                            &id,
                            "height",
                            serde_json::json!(params.3.max(0.01)),
                        );
                        changed |=
                            editor.set_mask_param(&id, "rotation", serde_json::json!(params.4));
                        changed
                    })
                });
            }
            LayerTarget::Tracking => {
                self.app.update(cx, |model, cx| {
                    model.tracking_region = crate::tracking::TrackingRegion {
                        x: (params.0 + 0.5 - params.2 / 2.0) as f32,
                        y: (params.1 + 0.5 - params.3 / 2.0) as f32,
                        width: params.2 as f32,
                        height: params.3 as f32,
                    }
                    .clamped();
                    cx.notify();
                });
            }
        }
        cx.notify();
    }

    fn on_layer_move(
        &mut self,
        event: &gpui::DragMoveEvent<LayerMove>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let drag = event.drag(cx).clone();
        let Some(gesture) = self.gesture_slot(drag.target).clone() else {
            return;
        };
        let (du, dv) = Self::layer_delta(&gesture, event.event.position);
        let mut params = gesture.box_params;
        params.0 += du;
        params.1 += dv;
        self.apply_layer_box(&drag.id, drag.target, params, cx);
    }

    fn on_layer_resize(
        &mut self,
        event: &gpui::DragMoveEvent<LayerResize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let drag = event.drag(cx).clone();
        let Some(gesture) = self.gesture_slot(drag.target).clone() else {
            return;
        };
        let (du, dv) = Self::layer_delta(&gesture, event.event.position);
        let mut params = gesture.box_params;

        params.2 = (params.2 + du * drag.corner.0 as f64).max(0.01);
        params.3 = (params.3 + dv * drag.corner.1 as f64).max(0.01);
        params.0 += (params.2 - gesture.box_params.2) / 2.0 * drag.corner.0 as f64;
        params.1 += (params.3 - gesture.box_params.3) / 2.0 * drag.corner.1 as f64;
        self.apply_layer_box(&drag.id, drag.target, params, cx);
    }

    fn on_layer_rotate(
        &mut self,
        event: &gpui::DragMoveEvent<LayerRotate>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let drag = event.drag(cx).clone();
        let Some(gesture) = self.mask_gesture.clone() else {
            return;
        };
        let travel = f64::from(f32::from(event.event.position.x - gesture.start.x));
        let mut params = gesture.box_params;
        params.4 = (gesture.box_params.4 + travel * LAYER_ROTATE_DEGREES_PER_PX).rem_euclid(360.0);
        self.apply_layer_box(&drag.id, LayerTarget::Mask, params, cx);
    }

    fn pointer_frame(&self, position: gpui::Point<gpui::Pixels>) -> (f32, f32) {
        let scale = self.frame_scale.max(0.0001);
        (
            f32::from(position.x - self.scene_origin.x) / scale,
            f32::from(position.y - self.scene_origin.y) / scale,
        )
    }

    fn croppable_selection(&self, cx: &App) -> Option<(String, cutix_project::Crop)> {
        let model = self.app.read(cx);
        let element = model.selected_element()?;
        if model
            .properties_tabs
            .get(crate::properties::element_type_key(element))
            .map(String::as_str)
            != Some("crop")
        {
            return None;
        }
        Some((element.base().id.clone(), edit::crop_of(element)))
    }

    fn crop_overlay(&mut self, scene: (f32, f32), cx: &mut Context<Self>) -> Option<Div> {
        let (id, crop) = self.croppable_selection(cx)?;
        let frame_width = self.app.read(cx).preview.frame_size.0;
        if frame_width == 0 {
            return None;
        }
        let scale = scene.0 / frame_width as f32;
        let rect = self.selected_rect(&id, cx)?;
        let full = gizmos::uncropped_bounds(&rect, &crop);
        let full_rect = cutix_playback::ElementRect {
            center_x: full.center_x,
            center_y: full.center_y,
            width: full.width,
            height: full.height,
            rotation_degrees: rect.rotation_degrees,
        };

        let handles = gizmos::CropHandle::ALL.into_iter().map(|handle| {
            let (x, y) = gizmos::handle_point(&rect, handle.offset());
            let (x, y) = (x * scale, y * scale);
            let cursor = cursor_for(gizmos::resize_cursor(gizmos::pointer_angle_degrees(
                (x - rect.center_x * scale) as f64,
                (y - rect.center_y * scale) as f64,
            )));
            let drag_id = id.clone();
            let press_id = id.clone();
            hit_handle(
                format!("crop-handle-{}-{}", id, handle.id()),
                x,
                y,
                cursor,
                handle_bar(handle),
            )
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this: &mut Self, _: &gpui::MouseDownEvent, _, cx| {
                    this.begin_crop_gesture(&press_id, cx);
                }),
            )
            .on_drag(
                CanvasCrop {
                    id: drag_id,
                    handle,
                },
                |_, _, _, cx| cx.new(|_| gpui::Empty),
            )
        });

        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(outline_box(&full_rect, scale, true))
                .child(outline_box(&rect, scale, false))
                .children(handles),
        )
    }

    fn begin_crop_gesture(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(rect) = self.selected_rect(id, cx) else {
            return;
        };
        let crop = self
            .app
            .read(cx)
            .element_by_id(id)
            .map(edit::crop_of)
            .unwrap_or_default();
        self.crop_gesture = Some(CropGesture {
            full: gizmos::uncropped_bounds(&rect, &crop),
            crop,
            rotation_degrees: rect.rotation_degrees,
        });
        cx.notify();
    }

    fn on_canvas_crop(
        &mut self,
        event: &gpui::DragMoveEvent<CanvasCrop>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let drag = event.drag(cx).clone();
        let Some(gesture) = self.crop_gesture.clone() else {
            return;
        };
        let (x, y) = self.pointer_frame(event.event.position);
        let (u, v) = gizmos::crop_display_uv(&gesture.full, gesture.rotation_degrees, x, y);
        let crop = gizmos::apply_crop_handle_drag(
            drag.handle,
            &gesture.crop,
            u,
            v,
            gesture.full.flip_x,
            gesture.full.flip_y,
        );
        let key = format!("canvas-crop-{}", drag.id);
        let id = drag.id.clone();
        self.app.update(cx, |model, cx| {
            model.edit_coalesced(Some(key), cx, |editor| {
                editor.apply_setting(&id, edit::Setting::Crop(crop))
            })
        });
        cx.notify();
    }

    fn begin_rotate_gesture(
        &mut self,
        id: &str,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(rect) = self.selected_rect(id, cx) else {
            return;
        };
        let Some(element) = self.app.read(cx).element_by_id(id).cloned() else {
            return;
        };
        let (x, y) = self.pointer_frame(position);
        self.rotate_gesture = Some(RotateGesture {
            rotation: edit::field_value(&element, edit::Field::Rotate),
            angle: gizmos::pointer_angle_degrees(
                (x - rect.center_x) as f64,
                (y - rect.center_y) as f64,
            ),
        });
        cx.notify();
    }

    fn on_canvas_rotate(
        &mut self,
        event: &gpui::DragMoveEvent<CanvasRotate>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let drag = event.drag(cx).clone();
        let Some(gesture) = self.rotate_gesture else {
            return;
        };
        let Some(rect) = self.selected_rect(&drag.id, cx) else {
            return;
        };
        let (x, y) = self.pointer_frame(event.event.position);
        let angle =
            gizmos::pointer_angle_degrees((x - rect.center_x) as f64, (y - rect.center_y) as f64);
        let rotation = gizmos::rotation_from_pointer(
            gesture.rotation,
            gesture.angle,
            angle,
            !event.event.modifiers.shift,
        );
        let key = format!("canvas-rotate-{}", drag.id);
        let id = drag.id.clone();
        self.app.update(cx, |model, cx| {
            model.edit_coalesced(Some(key), cx, |editor| {
                editor.set_property(&id, edit::Field::Rotate, rotation)
            })
        });
        cx.notify();
    }

    fn snap_overlay(&self, scene: (f32, f32), canvas: (f32, f32)) -> Option<Div> {
        if self.snap_lines.is_empty() || canvas.0 <= 0.0 {
            return None;
        }
        let scale = scene.0 / canvas.0;
        let lines = self.snap_lines.iter().map(|line| match line.axis {
            gizmos::SnapAxis::Vertical => div()
                .absolute()
                .top_0()
                .left(px((line.position as f32 + canvas.0 / 2.0) * scale))
                .w(px(1.0))
                .h(px(scene.1))
                .bg(opacity(gpui::white(), gizmos::SNAP_LINE_OPACITY)),
            gizmos::SnapAxis::Horizontal => div()
                .absolute()
                .left_0()
                .top(px((line.position as f32 + canvas.1 / 2.0) * scale))
                .h(px(1.0))
                .w(px(scene.0))
                .bg(opacity(gpui::white(), gizmos::SNAP_LINE_OPACITY)),
        });
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .children(lines.collect::<Vec<_>>()),
        )
    }

    fn guide_overlay(&mut self, scene: (f32, f32)) -> Option<Div> {
        let id = self.active_guide.clone()?;
        let shapes = gizmos::guide_shapes(&id, self.grid, &self.custom_lines, scene.0, scene.1);
        if shapes.is_empty() {
            return None;
        }
        let stroke = opacity(gpui::white(), gizmos::GUIDE_LINE_OPACITY);
        let children = shapes
            .into_iter()
            .enumerate()
            .map(|(index, shape)| match shape {
                gizmos::GuideShape::VLine { x } => guide_line(
                    index,
                    id.as_ref() == "custom",
                    gizmos::SnapAxis::Vertical,
                    (x, 0.0),
                    scene,
                    stroke,
                ),
                gizmos::GuideShape::HLine { y } => guide_line(
                    index,
                    id.as_ref() == "custom",
                    gizmos::SnapAxis::Horizontal,
                    (0.0, y),
                    scene,
                    stroke,
                ),
                gizmos::GuideShape::Rect {
                    left,
                    top,
                    width,
                    height,
                } => div()
                    .id(SharedString::from(format!("guide-band-{index}")))
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .w(px(width))
                    .h(px(height))
                    .border_1()
                    .border_color(stroke)
                    .bg(opacity(gpui::white(), 0.08)),
            })
            .collect::<Vec<_>>();
        Some(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .children(children),
        )
    }

    fn on_guide_line_drag(
        &mut self,
        event: &gpui::DragMoveEvent<GuideLineDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let index = event.drag(cx).index;
        let scene = self.scene;
        if scene.0 <= 0.0 || scene.1 <= 0.0 {
            return;
        }
        let offset_x = f32::from(event.event.position.x - self.scene_origin.x);
        let offset_y = f32::from(event.event.position.y - self.scene_origin.y);
        if let Some(line) = self.custom_lines.get_mut(index) {
            line.fraction = match line.axis {
                gizmos::SnapAxis::Vertical => (offset_x / scene.0).clamp(0.0, 1.0),
                gizmos::SnapAxis::Horizontal => (offset_y / scene.1).clamp(0.0, 1.0),
            };
            cx.notify();
        }
    }

    fn selection_overlay(
        &mut self,
        scene: (f32, f32),
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        if self.croppable_selection(cx).is_some() {
            return None;
        }
        let colors = self.colors(cx);
        let canvas = self.canvas(cx);
        if canvas.0 <= 0.0 {
            return None;
        }

        let frame_width = self.app.read(cx).preview.frame_size.0;
        if frame_width == 0 {
            return None;
        }
        let scale = scene.0 / frame_width as f32;
        self.canvas_scale = scene.0 / canvas.0;

        let model = self.app.read(cx);
        let id = model.selection.first()?.clone();
        let rect = model
            .preview
            .rects
            .iter()
            .find(|(element, _)| *element == id)
            .map(|(_, rect)| *rect)?;

        let left = (rect.center_x - rect.width / 2.0) * scale;
        let top = (rect.center_y - rect.height / 2.0) * scale;
        let width = rect.width * scale;
        let height = rect.height * scale;

        let corners = [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)];
        let handles = corners
            .into_iter()
            .map(|corner| {
                let (x, y) = gizmos::handle_point(&rect, (corner.0 / 2.0, corner.1 / 2.0));
                let (x, y) = (x * scale, y * scale);
                let cursor = cursor_for(gizmos::resize_cursor(gizmos::pointer_angle_degrees(
                    (x - rect.center_x * scale) as f64,
                    (y - rect.center_y * scale) as f64,
                )));
                let drag_id = id.clone();
                let press_id = id.clone();
                hit_handle(
                    format!("handle-{}-{}-{}", id, corner.0, corner.1),
                    x,
                    y,
                    cursor,
                    (gizmos::HANDLE_SIZE_PX, gizmos::HANDLE_SIZE_PX),
                )
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(
                        move |this: &mut Self, event: &gpui::MouseDownEvent, _, cx| {
                            this.begin_gesture(&press_id, event.position, (width, height), cx);
                        },
                    ),
                )
                .on_drag(
                    CanvasScale {
                        id: drag_id,
                        corner,
                    },
                    |_, _, _, cx| cx.new(|_| gpui::Empty),
                )
            })
            .collect::<Vec<_>>();

        let (rotate_x, rotate_y) = gizmos::rotation_handle_point(
            &rect,
            gizmos::ROTATION_HANDLE_OFFSET_PX / scale.max(0.0001),
        );
        let (rotate_x, rotate_y) = (rotate_x * scale, rotate_y * scale);
        let rotate_press = id.clone();
        let rotate = div()
            .id(SharedString::from(format!("rotate-{id}")))
            .absolute()
            .left(px(rotate_x - gizmos::ICON_HANDLE_RADIUS_PX))
            .top(px(rotate_y - gizmos::ICON_HANDLE_RADIUS_PX))
            .size(px(gizmos::ICON_HANDLE_RADIUS_PX * 2.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(gpui::white())
            .cursor(gpui::CursorStyle::OpenHand)
            .child(
                svg()
                    .size(px(12.0))
                    .flex_shrink_0()
                    .path(icon("rotate-clockwise"))
                    .text_color(gpui::black()),
            )
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(
                    move |this: &mut Self, event: &gpui::MouseDownEvent, _, cx| {
                        this.begin_rotate_gesture(&rotate_press, event.position, cx);
                    },
                ),
            )
            .on_drag(CanvasRotate { id: id.clone() }, |_, _, _, cx| {
                cx.new(|_| gpui::Empty)
            });

        let move_id = id.clone();
        let press_move_id = id.clone();
        let _ = colors;
        Some(
            div()
                .id(SharedString::from(format!("selection-{id}")))
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .child(outline_box(&rect, scale, false))
                .child(
                    div()
                        .id(SharedString::from(format!("selection-body-{id}")))
                        .absolute()
                        .left(px(left))
                        .top(px(top))
                        .w(px(width))
                        .h(px(height))
                        .cursor(gpui::CursorStyle::OpenHand)
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(
                                move |this: &mut Self, event: &gpui::MouseDownEvent, _, cx| {
                                    this.begin_gesture(
                                        &press_move_id,
                                        event.position,
                                        (width, height),
                                        cx,
                                    );
                                },
                            ),
                        )
                        .on_drag(CanvasMove { id: move_id }, |_, _, _, cx| {
                            cx.new(|_| gpui::Empty)
                        }),
                )
                .children(handles)
                .child(rotate),
        )
    }

    fn begin_gesture(
        &mut self,
        element_id: &str,
        start: gpui::Point<gpui::Pixels>,
        size: (f32, f32),
        cx: &mut Context<Self>,
    ) {
        let Some(element) = self.app.read(cx).element_by_id(element_id).cloned() else {
            return;
        };
        self.gesture = Some(CanvasGesture {
            start,
            position: (
                edit::field_value(&element, edit::Field::PositionX),
                edit::field_value(&element, edit::Field::PositionY),
            ),
            scale: (
                edit::field_value(&element, edit::Field::ScaleX),
                edit::field_value(&element, edit::Field::ScaleY),
            ),
            size,
        });
        cx.notify();
    }

    fn on_canvas_move(
        &mut self,
        event: &gpui::DragMoveEvent<CanvasMove>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = event.drag(cx).id.clone();
        let Some(gesture) = self.gesture.clone() else {
            return;
        };
        let scale = self.canvas_scale.max(0.0001);
        let delta_x = f32::from(event.event.position.x - gesture.start.x) / scale;
        let delta_y = f32::from(event.event.position.y - gesture.start.y) / scale;
        let x = gesture.position.0 + delta_x as f64;
        let y = gesture.position.1 + delta_y as f64;
        let (x, y) = if event.event.modifiers.shift {
            self.snap_lines.clear();
            (x, y)
        } else {
            let canvas = self.canvas(cx);
            let rotation = self
                .selected_rect(&id, cx)
                .map_or(0.0, |rect| rect.rotation_degrees as f64);
            let threshold = (gizmos::SNAP_THRESHOLD_SCREEN_PX / scale) as f64;
            let snapped = gizmos::snap_position(
                (x, y),
                (canvas.0 as f64, canvas.1 as f64),
                (
                    (gesture.size.0 / scale) as f64,
                    (gesture.size.1 / scale) as f64,
                ),
                rotation,
                (threshold, threshold),
            );
            self.snap_lines = snapped.lines.clone();
            (snapped.x, snapped.y)
        };
        let key = format!("canvas-move-{id}");
        self.app.update(cx, |model, cx| {
            model.edit_coalesced(Some(key), cx, |editor| {
                let mut changed = editor.set_property(&id, edit::Field::PositionX, x);
                changed |= editor.set_property(&id, edit::Field::PositionY, y);
                changed
            })
        });
        cx.notify();
    }

    fn on_canvas_scale(
        &mut self,
        event: &gpui::DragMoveEvent<CanvasScale>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let drag = event.drag(cx).clone();
        let Some(gesture) = self.gesture.clone() else {
            return;
        };
        if gesture.size.0 <= 1.0 || gesture.size.1 <= 1.0 {
            return;
        }
        let delta_x = f32::from(event.event.position.x - gesture.start.x) * drag.corner.0;
        let delta_y = f32::from(event.event.position.y - gesture.start.y) * drag.corner.1;
        let factor_x = ((gesture.size.0 + delta_x * 2.0) / gesture.size.0).max(0.05) as f64;
        let factor_y = ((gesture.size.1 + delta_y * 2.0) / gesture.size.1).max(0.05) as f64;
        let key = format!("canvas-scale-{}", drag.id);
        let scale_x = gesture.scale.0 * factor_x;
        let scale_y = gesture.scale.1 * factor_y;
        let (scale_x, scale_y) = if event.event.modifiers.shift {
            self.snap_lines.clear();
            (scale_x, scale_y)
        } else {
            let view = self.canvas_scale.max(0.0001);
            let canvas = self.canvas(cx);
            let rotation = self
                .selected_rect(&drag.id, cx)
                .map_or(0.0, |rect| rect.rotation_degrees as f64);
            let base = (
                (gesture.size.0 / view) as f64 / gesture.scale.0.abs().max(1e-6),
                (gesture.size.1 / view) as f64 / gesture.scale.1.abs().max(1e-6),
            );
            let threshold = (gizmos::SNAP_THRESHOLD_SCREEN_PX / view) as f64;
            let (x, y) = gizmos::snap_scale_axes(
                (scale_x, scale_y),
                gesture.position,
                base,
                rotation,
                (canvas.0 as f64, canvas.1 as f64),
                (threshold, threshold),
            );
            let mut lines = x.lines.clone();
            for line in &y.lines {
                if !lines.contains(line) {
                    lines.push(*line);
                }
            }
            self.snap_lines = lines;
            (x.scale, y.scale)
        };
        self.app.update(cx, |model, cx| {
            model.edit_coalesced(Some(key), cx, |editor| {
                let mut changed = editor.set_property(&drag.id, edit::Field::ScaleX, scale_x);
                changed |= editor.set_property(&drag.id, edit::Field::ScaleY, scale_y);
                changed
            })
        });
        cx.notify();
    }

    fn zoom_label(&self) -> String {
        match self.zoom_percent {
            Some(percent) => format!("{percent}%"),
            None => t("editor.preview.fit"),
        }
    }

    fn guide_label(entry: &gizmos::GuideEntry) -> String {
        entry
            .label_key
            .map(t)
            .unwrap_or_else(|| entry.label.to_owned())
    }

    fn guide_stepper(
        &mut self,
        key: &'static str,
        label: String,
        value: u32,
        cx: &mut Context<Self>,
    ) -> Div {
        let colors = self.colors(cx);
        let step = move |this: &mut Self, delta: i32| {
            let mut grid = this.grid;
            let target = if key == "rows" {
                &mut grid.rows
            } else {
                &mut grid.cols
            };
            *target = (*target as i32 + delta)
                .clamp(gizmos::GRID_MIN as i32, gizmos::GRID_MAX as i32)
                as u32;
            this.grid = grid.clamped();
        };
        let button = |id: &'static str, glyph: &'static str, delta: i32, cx: &mut Context<Self>| {
            div()
                .id(id)
                .flex()
                .size(px(20.0))
                .items_center()
                .justify_center()
                .rounded(rem(RADIUS_SM))
                .border_1()
                .border_color(colors.border)
                .cursor_pointer()
                .text_size(rem(TEXT_XS))
                .text_color(colors.foreground)
                .child(glyph)
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    step(this, delta);
                    cx.notify();
                }))
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(8.0))
            .px(px(10.0))
            .py(px(4.0))
            .child(
                div()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(label),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(button(
                        if key == "rows" {
                            "guide-rows-down"
                        } else {
                            "guide-cols-down"
                        },
                        "-",
                        -1,
                        cx,
                    ))
                    .child(
                        div()
                            .w(px(20.0))
                            .text_size(rem(TEXT_XS))
                            .text_center()
                            .text_color(colors.foreground)
                            .child(format!("{value}")),
                    )
                    .child(button(
                        if key == "rows" {
                            "guide-rows-up"
                        } else {
                            "guide-cols-up"
                        },
                        "+",
                        1,
                        cx,
                    )),
            )
    }

    fn guide_menu(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let frame = self.guide_menu.frame();
        if !frame.visible {
            return None;
        }

        let colors = self.colors(cx);
        let side = self.guide_menu.side;
        let active = self.active_guide.clone();
        let active_id: Option<&str> = active.as_ref().map(|value| value.as_ref());
        let extra = match active_id {
            Some("grid") => 3,
            Some("custom") => 4,
            _ => 0,
        };
        let count = gizmos::GUIDE_REGISTRY.len() + 1 + extra;
        let natural = (
            ZOOM_MENU_WIDTH_PX,
            menu_natural_height(count, ZOOM_MENU_ITEM_HEIGHT_PX),
        );
        let placement = place_anchored(frame, side, natural, (0.0, -MENU_OFFSET_PX));

        let mut items: Vec<gpui::AnyElement> = Vec::new();
        items.push(
            menu_item(
                "guide-none",
                colors,
                t("common.none"),
                self.transitions.eased("guide-none") > 0.5,
                active.is_none(),
            )
            .on_hover(hover_listener("guide-none", cx))
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.active_guide = None;
                this.guide_menu.dismiss();
                cx.notify();
            }))
            .into_any_element(),
        );
        for entry in gizmos::GUIDE_REGISTRY {
            let id = entry.id;
            let key = format!("guide-{id}");
            items.push(
                menu_item(
                    SharedString::from(key.clone()),
                    colors,
                    Self::guide_label(entry),
                    self.transitions.eased(&key) > 0.5,
                    active_id == Some(id),
                )
                .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                    this.transitions.set(format!("guide-{id}"), *hovered);
                    cx.notify();
                }))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.active_guide = Some(SharedString::from(id));
                    this.guide_menu.dismiss();
                    cx.notify();
                }))
                .into_any_element(),
            );
        }
        if active_id == Some("grid") {
            let rows = self.grid.rows;
            let cols = self.grid.cols;
            items.push(separator_h(colors).into_any_element());
            items.push(
                self.guide_stepper("rows", t("guides.rows"), rows, cx)
                    .into_any_element(),
            );
            items.push(
                self.guide_stepper("cols", t("guides.columns"), cols, cx)
                    .into_any_element(),
            );
        }
        if active_id == Some("custom") {
            items.push(separator_h(colors).into_any_element());
            for (id, label, axis) in [
                (
                    "guide-add-vertical",
                    t("guides.addLine.vertical"),
                    gizmos::SnapAxis::Vertical,
                ),
                (
                    "guide-add-horizontal",
                    t("guides.addLine.horizontal"),
                    gizmos::SnapAxis::Horizontal,
                ),
            ] {
                items.push(
                    menu_item(id, colors, label, self.transitions.eased(id) > 0.5, false)
                        .on_hover(hover_listener(id, cx))
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.custom_lines.push(gizmos::CustomLine {
                                axis,
                                fraction: 0.5,
                            });
                            cx.notify();
                        }))
                        .into_any_element(),
                );
            }
            items.push(
                menu_item(
                    "guide-clear-lines",
                    colors,
                    t("guides.clearLines"),
                    self.transitions.eased("guide-clear-lines") > 0.5,
                    false,
                )
                .on_hover(hover_listener("guide-clear-lines", cx))
                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                    this.custom_lines.clear();
                    cx.notify();
                }))
                .into_any_element(),
            );
        }

        Some(
            crate::components::overlay_root()
                .child(overlay_backdrop("guide-backdrop").on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.guide_menu.dismiss();
                        cx.notify();
                    }),
                ))
                .child(overlay_layer(
                    gpui::Corner::BottomLeft,
                    placement,
                    menu_surface(colors, placement).children(items),
                )),
        )
    }

    fn zoom_menu(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let frame = self.zoom_menu.frame();
        if !frame.visible {
            return None;
        }

        let colors = self.colors(cx);
        let side = self.zoom_menu.side;
        let count = PREVIEW_ZOOM_PRESETS.len() + 1;
        let natural = (
            ZOOM_MENU_WIDTH_PX,
            menu_natural_height(count, ZOOM_MENU_ITEM_HEIGHT_PX),
        );
        let anchor = (0.0, -MENU_OFFSET_PX);
        let placement = place_anchored(frame, side, natural, anchor);

        let selected = self.zoom_percent;
        let mut items = Vec::with_capacity(count);
        items.push(
            menu_item(
                "zoom-fit",
                colors,
                t("editor.preview.fit"),
                self.transitions.eased("zoom-fit") > 0.5,
                selected.is_none(),
            )
            .on_hover(hover_listener("zoom-fit", cx))
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.zoom_percent = None;
                this.zoom_menu.dismiss();
                cx.notify();
            })),
        );
        for percent in PREVIEW_ZOOM_PRESETS {
            let percent = *percent;
            let key = format!("zoom-{percent}");
            items.push(
                menu_item(
                    SharedString::from(key.clone()),
                    colors,
                    format!("{percent}%"),
                    self.transitions.eased(&key) > 0.5,
                    selected == Some(percent),
                )
                .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                    this.transitions.set(format!("zoom-{percent}"), *hovered);
                    cx.notify();
                }))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.zoom_percent = Some(percent);
                    this.zoom_menu.dismiss();
                    cx.notify();
                })),
            );
        }

        Some(
            crate::components::overlay_root()
                .child(overlay_backdrop("zoom-backdrop").on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.zoom_menu.dismiss();
                        cx.notify();
                    }),
                ))
                .child(overlay_layer(
                    gpui::Corner::BottomLeft,
                    placement,
                    menu_surface(colors, placement).children(items),
                )),
        )
    }
}

fn cursor_for(cursor: gizmos::ResizeCursor) -> gpui::CursorStyle {
    match cursor {
        gizmos::ResizeCursor::EastWest => gpui::CursorStyle::ResizeLeftRight,
        gizmos::ResizeCursor::NorthSouth => gpui::CursorStyle::ResizeUpDown,
        gizmos::ResizeCursor::NorthWestSouthEast => gpui::CursorStyle::ResizeUpLeftDownRight,
        gizmos::ResizeCursor::NorthEastSouthWest => gpui::CursorStyle::ResizeUpRightDownLeft,
    }
}

fn handle_bar(handle: gizmos::CropHandle) -> (f32, f32) {
    if handle.is_corner() {
        return (gizmos::HANDLE_SIZE_PX, gizmos::HANDLE_SIZE_PX);
    }
    match handle {
        gizmos::CropHandle::Left | gizmos::CropHandle::Right => {
            (gizmos::EDGE_HANDLE_THIN_PX, gizmos::EDGE_HANDLE_THICK_PX)
        }
        _ => (gizmos::EDGE_HANDLE_THICK_PX, gizmos::EDGE_HANDLE_THIN_PX),
    }
}

fn hit_handle(
    id: impl Into<SharedString>,
    x: f32,
    y: f32,
    cursor: gpui::CursorStyle,
    bar: (f32, f32),
) -> Stateful<Div> {
    div()
        .id(id.into())
        .absolute()
        .left(px(x - gizmos::HANDLE_HIT_AREA_PX / 2.0))
        .top(px(y - gizmos::HANDLE_HIT_AREA_PX / 2.0))
        .size(px(gizmos::HANDLE_HIT_AREA_PX))
        .flex()
        .items_center()
        .justify_center()
        .cursor(cursor)
        .child(
            div()
                .w(px(bar.0))
                .h(px(bar.1))
                .rounded(px(2.0))
                .bg(gpui::white()),
        )
}

fn guide_line(
    index: usize,
    draggable: bool,
    axis: gizmos::SnapAxis,
    at: (f32, f32),
    scene: (f32, f32),
    stroke: gpui::Hsla,
) -> Stateful<Div> {
    let vertical = matches!(axis, gizmos::SnapAxis::Vertical);
    let band = if draggable {
        gizmos::LINE_HIT_AREA_PX
    } else {
        1.0
    };
    let line = div()
        .id(SharedString::from(format!("guide-line-{index}")))
        .absolute()
        .flex()
        .items_center()
        .justify_center();
    let line = if vertical {
        line.top_0()
            .left(px(at.0 - band / 2.0))
            .w(px(band))
            .h(px(scene.1))
            .child(div().w(px(1.0)).h(px(scene.1)).bg(stroke))
    } else {
        line.left_0()
            .top(px(at.1 - band / 2.0))
            .h(px(band))
            .w(px(scene.0))
            .child(div().h(px(1.0)).w(px(scene.0)).bg(stroke))
    };
    if !draggable {
        return line;
    }
    line.cursor(if vertical {
        gpui::CursorStyle::ResizeLeftRight
    } else {
        gpui::CursorStyle::ResizeUpDown
    })
    .on_drag(GuideLineDrag { index }, |_, _, _, cx| {
        cx.new(|_| gpui::Empty)
    })
}

fn outline_box(rect: &cutix_playback::ElementRect, scale: f32, dashed: bool) -> impl IntoElement {
    let corners = [(-0.5f32, -0.5f32), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)].map(|(u, v)| {
        let (x, y) = rect.canvas_from_local(u, v);
        (x * scale, y * scale)
    });
    gpui::canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let mut builder = gpui::PathBuilder::stroke(px(1.0));
            if dashed {
                builder =
                    builder.dash_array(&[px(gizmos::OUTLINE_DASH_PX), px(gizmos::OUTLINE_DASH_PX)]);
            }
            let point = |index: usize| {
                gpui::point(
                    bounds.origin.x + px(corners[index].0),
                    bounds.origin.y + px(corners[index].1),
                )
            };
            builder.move_to(point(0));
            for index in 1..4 {
                builder.line_to(point(index));
            }
            builder.line_to(point(0));
            if let Ok(path) = builder.build() {
                window.paint_path(path, opacity(gpui::white(), gizmos::OUTLINE_OPACITY));
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

fn scene_probe(cx: &mut Context<PreviewPanel>) -> impl IntoElement {
    let handle = cx.entity();

    gpui::canvas(
        move |bounds, _window, cx| {
            handle.update(cx, |panel: &mut PreviewPanel, cx| {
                if panel.scene_origin != bounds.origin {
                    panel.scene_origin = bounds.origin;
                    cx.notify();
                }
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

fn viewport_probe(cx: &mut Context<PreviewPanel>) -> impl IntoElement {
    let handle = cx.entity();

    gpui::canvas(
        move |bounds, _window, cx| {
            let size = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            handle.update(cx, |panel: &mut PreviewPanel, cx| {
                if panel.viewport != size {
                    panel.viewport = size;
                    cx.notify();
                }
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

#[derive(Debug)]
struct PreviewScrubDrag;

#[derive(Debug)]
struct PreviewVolumeDrag;

fn progress_probe(cx: &mut Context<PreviewPanel>) -> impl IntoElement {
    let handle = cx.entity();

    gpui::canvas(
        move |bounds, _window, cx| {
            let measured = (f32::from(bounds.origin.x), f32::from(bounds.size.width));
            handle.update(cx, |panel: &mut PreviewPanel, _| {
                panel.progress_bounds = measured;
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

impl PreviewPanel {
    fn player_controls(
        &mut self,
        colors: Palette,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let playing = self.app.read(cx).preview.is_playing();
        let visible = self.controls_visible(playing);
        self.transitions.set("preview-controls", visible);
        let shown = self.transitions.eased("preview-controls");
        if shown <= 0.01 {
            self.progress_hover = None;
            return None;
        }

        let rate = self.app.read(cx).frame_rate();
        let total_time = self.app.read(cx).total_duration();
        let playhead = self.app.read(cx).playhead;
        let current = crate::state::format_frames(playhead, rate);
        let total = crate::state::format_frames(total_time, rate);
        let elapsed = if total_time.as_ticks() > 0 {
            (playhead.as_ticks() as f32 / total_time.as_ticks() as f32).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let hover_at = self.progress_hover;
        let hover_label = hover_at.map(|value| {
            crate::state::format_frames(
                MediaTime::from_ticks((total_time.as_ticks() as f64 * f64::from(value)) as i64),
                rate,
            )
        });
        let muted = self.app.read(cx).preview.is_muted();
        let level = if muted {
            0.0
        } else {
            self.app.read(cx).preview.volume()
        };
        let measured = if playing {
            self.app.read(cx).preview.frames_per_second
        } else {
            0.0
        };
        let audio_notice = self.app.read(cx).preview.audio_warning();

        let play_progress = self.transitions.eased("preview-play");
        let mute_progress = self.transitions.eased("preview-mute");
        let full_progress = self.transitions.eased("preview-fullscreen");
        let volume_progress = self.transitions.eased("preview-volume");
        let fit_progress = if self.zoom_menu.is_open() {
            1.0
        } else {
            self.transitions.eased("preview-fit")
        };
        let guide_progress = if self.guide_menu.is_open() {
            1.0
        } else {
            self.transitions.eased("preview-guides")
        };
        let play_tip = self.tooltips.frame_for("preview-play");
        let mute_tip = self.tooltips.frame_for("preview-mute");
        let full_tip = self.tooltips.frame_for("preview-fullscreen");
        let guides_tip = self.tooltips.frame_for("preview-guides");
        let zoom_label = self.zoom_label();
        let zoom_menu = self.zoom_menu(cx);
        let guide_menu = self.guide_menu(cx);

        let progress = div()
            .id("preview-progress")
            .relative()
            .w_full()
            .h(px(14.0))
            .flex()
            .flex_shrink_0()
            .items_center()
            .cursor_pointer()
            .on_hover(cx.listener(|this: &mut Self, hovered: &bool, _, cx| {
                if !*hovered {
                    this.progress_hover = None;
                }
                this.note_activity();
                cx.notify();
            }))
            .on_mouse_move(cx.listener(Self::on_progress_move))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(Self::on_progress_press),
            )
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(Self::on_progress_release),
            )
            .on_drag(PreviewScrubDrag, |_, _, _, cx| cx.new(|_| gpui::Empty))
            .on_drag_move::<PreviewScrubDrag>(cx.listener(Self::on_progress_drag))
            .child(progress_probe(cx))
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(4.0))
                    .rounded(px(2.0))
                    .bg(opacity(colors.foreground, 0.3))
                    .child(
                        div()
                            .w(relative(elapsed))
                            .h_full()
                            .rounded(px(2.0))
                            .bg(colors.primary),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(-3.0))
                            .left(relative(elapsed))
                            .ml(px(-5.0))
                            .size(px(10.0))
                            .rounded_full()
                            .bg(colors.primary),
                    ),
            )
            .when_some(hover_label, |this, label| {
                this.child(
                    div()
                        .absolute()
                        .bottom(px(18.0))
                        .left(relative(hover_at.unwrap_or(0.0)))
                        .child(
                            div()
                                .ml(px(-28.0))
                                .rounded(rem(RADIUS_SM))
                                .border_1()
                                .border_color(colors.border)
                                .bg(colors.popover)
                                .text_color(colors.popover_foreground)
                                .font_family(MONO_FONT)
                                .text_size(rem(TEXT_XS))
                                .px(px(6.0))
                                .py(px(2.0))
                                .child(label),
                        ),
                )
            });

        let volume = div()
            .id("preview-volume")
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(4.0))
            .on_hover(hover_listener("preview-volume", cx))
            .child(tooltipped(
                text_button(
                    "preview-mute",
                    if muted { "volume-mute" } else { "volume-high" },
                    colors,
                    mute_progress,
                )
                .on_hover(hover_listener("preview-mute", cx))
                .on_mouse_down(gpui::MouseButton::Left, press_listener(cx))
                .on_click(cx.listener(|this: &mut Self, _, _, cx| this.toggle_mute(cx))),
                colors,
                if muted {
                    t("preview.unmute")
                } else {
                    t("preview.mute")
                },
                mute_tip,
                OverlaySide::Top,
            ))
            .child(
                div()
                    .id("preview-volume-slider")
                    .flex()
                    .h(px(28.0))
                    .w(px(72.0 * volume_progress))
                    .items_center()
                    .overflow_hidden()
                    .cursor_pointer()
                    .on_drag(PreviewVolumeDrag, |_, _, _, cx| cx.new(|_| gpui::Empty))
                    .on_drag_move::<PreviewVolumeDrag>(cx.listener(Self::on_volume_drag))
                    .on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this: &mut Self, _, _, cx| this.persist_audio(cx)),
                    )
                    .child(
                        div()
                            .relative()
                            .w(px(64.0))
                            .h(px(4.0))
                            .flex_shrink_0()
                            .rounded(px(2.0))
                            .bg(opacity(colors.foreground, 0.3))
                            .child(
                                div()
                                    .w(relative(level))
                                    .h_full()
                                    .rounded(px(2.0))
                                    .bg(colors.foreground),
                            ),
                    ),
            );

        let timecode = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .font_family(MONO_FONT)
            .text_size(rem(TEXT_XS))
            .child(div().text_color(colors.foreground).child(current))
            .child(
                div()
                    .px(px(6.0))
                    .text_color(opacity(colors.foreground, 0.6))
                    .child("/"),
            )
            .child(
                div()
                    .text_color(opacity(colors.foreground, 0.6))
                    .child(total),
            )
            .when(measured > 0.0, |this| {
                this.child(
                    div()
                        .pl(px(10.0))
                        .text_color(opacity(colors.foreground, 0.5))
                        .child(t_args(
                            "preview.renderRate",
                            &[("rate", &format!("{measured:.0}"))],
                        )),
                )
            })
            .when_some(audio_notice, |this, reason| {
                this.child(
                    div()
                        .pl(px(10.0))
                        .min_w_0()
                        .truncate()
                        .text_color(opacity(colors.foreground, 0.5))
                        .child(t_args(
                            "preview.audioUnavailable.reason",
                            &[("reason", &reason)],
                        )),
                )
            });

        let right = div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_shrink_0()
                    .child(
                        div()
                            .id("preview-fit")
                            .flex()
                            .h(px(28.0))
                            .items_center()
                            .gap(px(4.0))
                            .px(px(10.0))
                            .rounded(rem(crate::theme::RADIUS_MD))
                            .border_1()
                            .border_color(opacity(colors.foreground, 0.25))
                            .cursor_pointer()
                            .text_size(rem(TEXT_SM))
                            .text_color(colors.foreground)
                            .bg(mix(
                                opacity(colors.foreground, 0.08),
                                opacity(colors.foreground, 0.2),
                                fit_progress,
                            ))
                            .on_hover(hover_listener("preview-fit", cx))
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.zoom_menu.toggle();
                                this.note_activity();
                                cx.notify();
                            }))
                            .child(zoom_label)
                            .child(
                                svg()
                                    .size(px(16.0))
                                    .flex_shrink_0()
                                    .path(icon("arrow-down"))
                                    .text_color(colors.foreground),
                            ),
                    )
                    .children(zoom_menu),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_shrink_0()
                    .child(tooltipped(
                        text_button("preview-guides", "grid-view", colors, guide_progress)
                            .on_hover(hover_listener("preview-guides", cx))
                            .on_mouse_down(gpui::MouseButton::Left, press_listener(cx))
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.guide_menu.toggle();
                                this.note_activity();
                                cx.notify();
                            })),
                        colors,
                        t("preview.guides"),
                        guides_tip,
                        OverlaySide::Top,
                    ))
                    .children(guide_menu),
            )
            .child(tooltipped(
                text_button("preview-fullscreen", "full-screen", colors, full_progress)
                    .on_hover(hover_listener("preview-fullscreen", cx))
                    .on_mouse_down(gpui::MouseButton::Left, press_listener(cx))
                    .on_click(cx.listener(|this: &mut Self, _, window, cx| {
                        this.toggle_fullscreen(window, cx)
                    })),
                colors,
                t("preview.fullScreen"),
                full_tip,
                OverlaySide::Top,
            ));

        Some(
            div()
                .id("preview-controls")
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx: &mut gpui::App| {
                    cx.stop_propagation()
                })
                .on_mouse_up(gpui::MouseButton::Left, |_, _, cx: &mut gpui::App| {
                    cx.stop_propagation()
                })
                .absolute()
                .bottom_0()
                .left_0()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .px(px(12.0))
                .pb(px(8.0))
                .pt(px(36.0))
                .opacity(shown)
                .bg(linear_gradient(
                    180.0,
                    linear_color_stop(opacity(gpui::black(), 0.0), 0.0),
                    linear_color_stop(opacity(gpui::black(), 0.75), 1.0),
                ))
                .on_hover(cx.listener(|this: &mut Self, hovered: &bool, _, cx| {
                    this.controls_hover = *hovered;
                    this.note_activity();
                    cx.notify();
                }))
                .child(progress)
                .child(
                    div()
                        .flex()
                        .w_full()
                        .items_center()
                        .gap(px(6.0))
                        .child(tooltipped(
                            text_button(
                                "preview-play",
                                if playing { "pause" } else { "play" },
                                colors,
                                play_progress,
                            )
                            .on_hover(hover_listener("preview-play", cx))
                            .on_mouse_down(gpui::MouseButton::Left, press_listener(cx))
                            .on_click(cx.listener(
                                |this: &mut Self, _, _, cx| {
                                    this.app.update(cx, |model, cx| model.toggle_playback(cx));
                                    this.note_activity();
                                    cx.notify();
                                },
                            )),
                            colors,
                            t("shortcuts.action.togglePlay"),
                            play_tip,
                            OverlaySide::Top,
                        ))
                        .child(volume)
                        .child(timecode)
                        .child(div().flex_1())
                        .child(right),
                ),
        )
    }

    fn controls_visible(&self, playing: bool) -> bool {
        if !playing || self.controls_hover || self.scrubbing {
            return true;
        }
        if self.zoom_menu.is_open() || self.guide_menu.is_open() {
            return true;
        }
        self.controls_activity
            .is_some_and(|at| at.elapsed() < PREVIEW_CONTROLS_LINGER)
    }

    fn note_activity(&mut self) {
        self.controls_activity = Some(Instant::now());
    }

    fn fraction_at(bounds: (f32, f32), x: f32) -> f32 {
        let (left, width) = bounds;
        if width <= 0.0 {
            return 0.0;
        }
        ((x - left) / width).clamp(0.0, 1.0)
    }

    fn seek_fraction(&mut self, fraction: f32, cx: &mut Context<Self>) {
        let total = self.app.read(cx).total_duration();
        let fps = self.app.read(cx).fps();
        let ticks = (total.as_ticks() as f64 * f64::from(fraction)) as i64;
        let time = edit::snap_to_frame(MediaTime::from_ticks(ticks), fps);
        let clamped = clamp_playhead(time, total);
        self.app.update(cx, |model, cx| model.seek(clamped, cx));
    }

    fn on_progress_move(
        &mut self,
        event: &gpui::MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let fraction = Self::fraction_at(self.progress_bounds, f32::from(event.position.x));
        self.progress_hover = Some(fraction);
        self.note_activity();
        cx.notify();
    }

    fn on_progress_press(
        &mut self,
        event: &gpui::MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.scrubbing = true;
        self.note_activity();
        let fraction = Self::fraction_at(self.progress_bounds, f32::from(event.position.x));
        self.seek_fraction(fraction, cx);
        cx.notify();
    }

    fn on_progress_drag(
        &mut self,
        event: &gpui::DragMoveEvent<PreviewScrubDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = event.bounds;
        let width = bounds.right() - bounds.left();
        if width <= px(0.0) {
            return;
        }
        let fraction = ((event.event.position.x - bounds.left()) / width).clamp(0.0, 1.0);
        self.scrubbing = true;
        self.progress_hover = Some(fraction);
        self.note_activity();
        self.seek_fraction(fraction, cx);
    }

    fn on_progress_release(
        &mut self,
        _event: &gpui::MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.scrubbing {
            return;
        }
        self.scrubbing = false;
        cx.notify();
    }

    fn on_volume_drag(
        &mut self,
        event: &gpui::DragMoveEvent<PreviewVolumeDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = event.bounds;
        let width = bounds.right() - bounds.left();
        if width <= px(0.0) {
            return;
        }
        let fraction = ((event.event.position.x - bounds.left()) / width).clamp(0.0, 1.0);
        self.set_volume(fraction, cx);
    }

    fn set_volume(&mut self, level: f32, cx: &mut Context<Self>) {
        self.app.update(cx, |model, cx| {
            model.preview.set_volume(level);
            cx.notify();
        });
        self.note_activity();
        cx.notify();
    }

    pub fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        self.app.update(cx, |model, cx| {
            model.preview.toggle_mute();
            cx.notify();
        });
        self.note_activity();
        self.persist_audio(cx);
        cx.notify();
    }

    fn persist_audio(&self, cx: &Context<Self>) {
        let preview = &self.app.read(cx).preview;
        crate::state::save_preview_audio(preview.volume(), preview.is_muted());
    }

    pub fn toggle_fullscreen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.toggle_fullscreen();
        self.note_activity();
        cx.notify();
    }
}

impl Hoverable for PreviewPanel {
    fn transitions(&mut self) -> &mut Transitions {
        &mut self.transitions
    }

    fn tooltips(&mut self) -> &mut Tooltips {
        &mut self.tooltips
    }
}

impl Render for PreviewPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        let canvas = self.canvas(cx);
        let rate = self.app.read(cx).frame_rate();
        let playing = self.app.read(cx).preview.is_playing();
        if playing {
            let now = self.app.read(cx).preview.current_time();
            let end = self.app.read(cx).total_duration();
            if end > MediaTime::ZERO && now >= end {
                self.app.update(cx, |model, cx| {
                    model.preview.pause();
                    model.playhead = clamp_playhead(now, end);
                    cx.notify();
                });
            } else {
                self.app
                    .update(cx, |model, _| model.playhead = clamp_playhead(now, end));
            }
        }
        let error = self.app.read(cx).preview.error.clone();
        let zoom = self.zoom_percent.map_or(1.0, |percent| {
            let fit = crate::theme::fit_scale(self.viewport, canvas);
            if fit > 0.0 {
                percent as f32 / 100.0 / fit
            } else {
                1.0
            }
        });
        let scene = crate::theme::scene_size(self.viewport, canvas, zoom);

        let width = (scene.0.round().max(1.0)) as u32;
        let height = (scene.1.round().max(1.0)) as u32;
        let frame = self.app.update(cx, |model, cx| {
            if let Some(stale) = model.preview.tick(width, height, rate) {
                cx.drop_image(stale, None);
            }
            model.preview.image.clone()
        });

        let awaiting = self.app.read(cx).preview.is_pending();
        self.scene = scene;
        self.frame_scale = if self.app.read(cx).preview.frame_size.0 > 0 {
            scene.0 / self.app.read(cx).preview.frame_size.0 as f32
        } else {
            1.0
        };
        let guides = self.guide_overlay(scene);
        let overlay = self.selection_overlay(scene, cx);
        let crop_box = self.crop_overlay(scene, cx);
        let tracking_box = self.tracking_overlay(scene, cx);
        let mask_box = self.mask_overlay(scene, cx);
        let snap = self.snap_overlay(scene, canvas);

        let controls = self.player_controls(colors, cx);

        self.tooltips.tick();
        if playing
            || awaiting
            || self.transitions.animating()
            || self.zoom_menu.animating()
            || self.tooltips.animating()
        {
            window.request_animation_frame();
        }

        panel_frame(colors).child(
            div()
                .flex()
                .flex_1()
                .w_full()
                .min_h_0()
                .items_center()
                .justify_center()
                .overflow_hidden()
                .p(px(8.0))
                .pb(px(0.0))
                .relative()
                .child(viewport_probe(cx))
                .child(
                    div()
                        .id("preview-scene")
                        .relative()
                        .w(px(scene.0))
                        .h(px(scene.1))
                        .flex_shrink_0()
                        .border_1()
                        .border_color(colors.border)
                        .bg(gpui::black())
                        .overflow_hidden()
                        .on_drag_move::<CanvasMove>(cx.listener(Self::on_canvas_move))
                        .on_drag_move::<CanvasScale>(cx.listener(Self::on_canvas_scale))
                        .on_drag_move::<LayerMove>(cx.listener(Self::on_layer_move))
                        .on_drag_move::<LayerResize>(cx.listener(Self::on_layer_resize))
                        .on_drag_move::<LayerRotate>(cx.listener(Self::on_layer_rotate))
                        .on_drag_move::<CanvasRotate>(cx.listener(Self::on_canvas_rotate))
                        .on_drag_move::<CanvasCrop>(cx.listener(Self::on_canvas_crop))
                        .on_drag_move::<GuideLineDrag>(cx.listener(Self::on_guide_line_drag))
                        .on_mouse_move(cx.listener(
                            |this: &mut Self, _: &gpui::MouseMoveEvent, _, cx| {
                                this.note_activity();
                                cx.notify();
                            },
                        ))
                        .on_mouse_up(
                            gpui::MouseButton::Left,
                            cx.listener(
                                |this: &mut Self,
                                 event: &gpui::MouseUpEvent,
                                 window: &mut Window,
                                 cx| {
                                    let ended = this.gesture.take().is_some()
                                        | this.mask_gesture.take().is_some()
                                        | this.tracking_gesture.take().is_some()
                                        | this.rotate_gesture.take().is_some()
                                        | this.crop_gesture.take().is_some();
                                    let snapped = !this.snap_lines.is_empty();
                                    this.snap_lines.clear();
                                    this.note_activity();

                                    if !ended && !snapped {
                                        if event.click_count >= 2 {
                                            this.app
                                                .update(cx, |model, cx| model.toggle_playback(cx));
                                            this.toggle_fullscreen(window, cx);
                                        } else {
                                            this.app
                                                .update(cx, |model, cx| model.toggle_playback(cx));
                                        }
                                    }
                                    cx.notify();
                                },
                            ),
                        )
                        .child(scene_probe(cx))
                        .when_some(frame, |this, image| this.child(img(image).size_full()))
                        .children(guides)
                        .children(overlay)
                        .children(crop_box)
                        .children(mask_box)
                        .children(tracking_box)
                        .children(snap)
                        .when_some(error, |this, message| {
                            this.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .size_full()
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .justify_center()
                                    .gap(px(8.0))
                                    .p(px(16.0))
                                    .bg(opacity(gpui::black(), 0.8))
                                    .child(
                                        div()
                                            .text_size(rem(TEXT_SM))
                                            .text_color(colors.destructive)
                                            .text_center()
                                            .child(t("preview.error")),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.0))
                                            .text_color(colors.muted_foreground)
                                            .text_center()
                                            .child(message),
                                    ),
                            )
                        }),
                )
                .children(controls),
        )
    }
}

pub const TIMELINE_ZOOM_MIN: f32 = 0.1;
pub const TIMELINE_ZOOM_MAX: f32 = 100.0;
pub const TIMELINE_ZOOM_BUTTON_FACTOR: f32 = 1.7;

pub const TIMELINE_ZOOM_WHEEL_EXPONENT: f32 = 0.0106;

pub const TIMELINE_ZOOM_WHEEL_MAX_STEP: f32 = 2.0;

pub fn wheel_zoom_factor(delta_pixels: f32) -> f32 {
    (delta_pixels * TIMELINE_ZOOM_WHEEL_EXPONENT).exp().clamp(
        1.0 / TIMELINE_ZOOM_WHEEL_MAX_STEP,
        TIMELINE_ZOOM_WHEEL_MAX_STEP,
    )
}

pub fn zoom_anchored_offset(
    viewport_left: f32,
    content_left: f32,
    cursor_x: f32,
    scale: f32,
) -> f32 {
    let anchor_x = cursor_x.max(viewport_left);
    let into_content = (anchor_x - content_left).max(0.0);
    (anchor_x - into_content * scale - viewport_left).min(0.0)
}

pub fn zoom_from_slider(slider: f32) -> f32 {
    TIMELINE_ZOOM_MIN * (TIMELINE_ZOOM_MAX / TIMELINE_ZOOM_MIN).powf(slider.clamp(0.0, 1.0))
}

pub fn slider_from_zoom(zoom: f32) -> f32 {
    let zoom = zoom.clamp(TIMELINE_ZOOM_MIN, TIMELINE_ZOOM_MAX);
    (zoom / TIMELINE_ZOOM_MIN).ln() / (TIMELINE_ZOOM_MAX / TIMELINE_ZOOM_MIN).ln()
}

#[derive(Debug)]
struct ZoomDrag;

#[derive(Clone, Debug)]
pub struct ClipDrag {
    id: String,
    grab: f32,
}

#[derive(Clone, Debug)]
pub struct TrimDrag {
    id: String,
    edge: Edge,
}

#[derive(Clone, Debug)]
pub struct MediaDrag {
    pub media_id: String,
}

#[derive(Clone, Debug)]
pub enum InsertDrag {
    TextPreset(&'static str),
    Sticker { sticker_id: String, name: String },
}

#[derive(Clone, Debug)]
pub enum ClipTargetDrag {
    Effect(&'static str),
    Transition(&'static str),
}

fn insert_drag_element(drag: &InsertDrag) -> Option<TimelineElement> {
    match drag {
        InsertDrag::TextPreset(preset_id) => {
            let presets = crate::text::presets();
            let preset = presets.iter().find(|preset| preset.id == *preset_id)?;
            Some(crate::edit::text_element(
                t(preset.name_key),
                t("text.default"),
                crate::text::patch_for(preset),
            ))
        }
        InsertDrag::Sticker { sticker_id, name } => Some(
            match crate::stickers_ui::graphic_definition_for(sticker_id) {
                Some(definition_id) => crate::edit::graphic_element(
                    definition_id.to_owned(),
                    name.clone(),
                    cutix_project::model::ParamValues::new(),
                ),
                None => crate::edit::sticker_element(sticker_id.clone(), name.clone()),
            },
        ),
    }
}

#[derive(Debug)]
struct ScrubDrag;

#[derive(Clone, Debug)]
struct Ghost {
    track_id: String,
    start: MediaTime,
    duration: MediaTime,
    snapped: bool,
}

pub struct TimelinePanel {
    app: Entity<AppModel>,
    tooltips: Tooltips,
    transitions: Transitions,
    scroll: ScrollHandle,
    scene_menu: Overlay,
    add_track_menu: Overlay,
    track_menu: Overlay,
    track_menu_for: Option<String>,
    clip_menu: Overlay,
    clip_menu_for: Option<String>,
    bookmark_menu: Overlay,
    bookmark_menu_for: Option<MediaTime>,
    bookmark_note: Option<TextField>,
    bookmark_drag: Option<(MediaTime, MediaTime)>,
    scene_rename: Option<(String, TextField)>,
    zoom_level: f32,
    snapping: bool,
    ripple: bool,
    ghost: Option<Ghost>,
    drop_target: Option<String>,
    content_left: f32,

    viewport_left: f32,
    scroll_left: f32,
    scroll_top: f32,

    pending_scroll: Option<(f32, f32)>,

    graph_open: bool,
    graph_selected_path: Option<String>,
    graph_key_drag: Option<GraphKeyDragState>,
}

#[derive(Clone, Debug)]
struct GraphKeyDragState {
    element_id: String,
    field: edit::Field,
    key_tick: i64,
    val_min: f64,
    val_max: f64,
    preview: f64,
}

#[derive(Debug)]
struct GraphKeyDrag;

#[derive(Debug)]
struct BookmarkDrag;

impl TimelinePanel {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self {
            app,
            tooltips: Tooltips::new(TOOLBAR_TOOLTIP_DELAY),
            transitions: Transitions::new(),
            scroll: ScrollHandle::new(),
            scene_menu: Overlay::new(OverlaySide::Bottom),
            add_track_menu: Overlay::new(OverlaySide::Bottom),
            track_menu: Overlay::new(OverlaySide::Bottom),
            track_menu_for: None,
            clip_menu: Overlay::new(OverlaySide::Bottom),
            clip_menu_for: None,
            bookmark_menu: Overlay::new(OverlaySide::Bottom),
            bookmark_menu_for: None,
            bookmark_note: None,
            bookmark_drag: None,
            scene_rename: None,
            zoom_level: DEFAULT_TIMELINE_ZOOM,
            snapping: true,
            ripple: false,
            ghost: None,
            drop_target: None,
            content_left: 0.0,
            viewport_left: 0.0,
            scroll_left: 0.0,
            scroll_top: 0.0,
            pending_scroll: None,
            graph_open: false,
            graph_selected_path: None,
            graph_key_drag: None,
        }
    }

    fn colors(&self, cx: &App) -> Palette {
        self.app.read(cx).theme.panel
    }

    fn fps(&self, cx: &App) -> f32 {
        self.app.read(cx).fps()
    }

    fn pixels_per_second(&self) -> f32 {
        BASE_TIMELINE_PIXELS_PER_SECOND * self.zoom_level
    }

    fn time_at(&self, offset_x: f32) -> MediaTime {
        let seconds = (offset_x / self.pixels_per_second()).max(0.0) as f64;
        MediaTime::from_seconds_f64(seconds).unwrap_or(MediaTime::ZERO)
    }

    fn x_of(&self, time: MediaTime) -> f32 {
        time.to_seconds_f64() as f32 * self.pixels_per_second()
    }

    fn snap_threshold(&self) -> MediaTime {
        MediaTime::from_seconds_f64((edit::SNAP_THRESHOLD_PX / self.pixels_per_second()) as f64)
            .unwrap_or(MediaTime::ZERO)
    }

    fn nudge_zoom(&mut self, factor: f32) {
        self.zoom_level = (self.zoom_level * factor).clamp(TIMELINE_ZOOM_MIN, TIMELINE_ZOOM_MAX);
    }

    fn on_timeline_wheel(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.modifiers;
        if modifiers.shift {
            return;
        }

        let delta = f32::from(event.delta.pixel_delta(window.line_height()).y);
        if delta == 0.0 {
            return;
        }

        if modifiers.alt || modifiers.control || modifiers.platform {
            self.pending_scroll = Some((self.scroll_left, (self.scroll_top + delta).min(0.0)));
            cx.notify();
            return;
        }

        let before = self.zoom_level;
        self.nudge_zoom(wheel_zoom_factor(delta));
        if (self.zoom_level - before).abs() < f32::EPSILON {
            return;
        }

        self.pending_scroll = Some((
            zoom_anchored_offset(
                self.viewport_left,
                self.content_left,
                f32::from(event.position.x),
                self.zoom_level / before,
            ),
            self.scroll_top,
        ));
        cx.notify();
    }

    fn track_at(&self, cx: &App, offset_y: f32) -> Option<String> {
        let mut top = TIMELINE_HEADER_HEIGHT_PX;
        for track in self.app.read(cx).tracks() {
            let height = edit::track_height(track);
            if offset_y >= top && offset_y < top + height + TIMELINE_TRACK_GAP_PX {
                return Some(track.id().to_string());
            }
            top += height + TIMELINE_TRACK_GAP_PX;
        }
        self.app
            .read(cx)
            .tracks()
            .last()
            .map(|track| track.id().to_string())
    }

    fn on_zoom_drag(
        &mut self,
        event: &gpui::DragMoveEvent<ZoomDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = event.bounds;
        let width = bounds.right() - bounds.left();
        if width <= px(0.0) {
            return;
        }
        let fraction = (event.event.position.x - bounds.left()) / width;
        self.zoom_level = zoom_from_slider(fraction);
        cx.notify();
    }

    fn scrub(&mut self, offset_x: f32, cx: &mut Context<Self>) {
        let fps = self.fps(cx);
        let total = self.app.read(cx).total_duration();
        let time = clamp_playhead(edit::snap_to_frame(self.time_at(offset_x), fps), total);
        self.app.update(cx, |model, cx| model.seek(time, cx));
    }

    fn on_scrub_drag(
        &mut self,
        event: &gpui::DragMoveEvent<ScrubDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offset = f32::from(event.event.position.x - event.bounds.left());
        self.scrub(offset, cx);
    }

    fn snapped_start(
        &self,
        cx: &App,
        candidate: MediaTime,
        duration: MediaTime,
        exclude: Option<&str>,
    ) -> (MediaTime, bool) {
        let fps = self.app.read(cx).fps();
        let candidate = edit::snap_to_frame(candidate.max(MediaTime::ZERO), fps);
        if !self.snapping {
            return (candidate, false);
        }
        let Some(scene) = self.app.read(cx).current_scene() else {
            return (candidate, false);
        };
        let playhead = self.app.read(cx).playhead;
        let threshold = self.snap_threshold();
        let start = edit::snap_time(&scene.tracks, candidate, playhead, exclude, threshold);
        let end = MediaTime::from_ticks(candidate.as_ticks() + duration.as_ticks());
        let snapped_end = edit::snap_time(&scene.tracks, end, playhead, exclude, threshold);
        let start_delta = (start.as_ticks() - candidate.as_ticks()).abs();
        let end_delta = (snapped_end.as_ticks() - end.as_ticks()).abs();
        if end_delta > 0 && end_delta < start_delta {
            let shifted = MediaTime::from_ticks(snapped_end.as_ticks() - duration.as_ticks());
            return (shifted.max(MediaTime::ZERO), true);
        }
        (start, start_delta > 0)
    }

    fn over_drop_area<T: 'static>(
        &mut self,
        event: &gpui::DragMoveEvent<T>,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.bounds.contains(&event.event.position) {
            return true;
        }
        if self.ghost.is_some() || self.drop_target.is_some() {
            self.ghost = None;
            self.drop_target = None;
            cx.notify();
        }
        false
    }

    fn on_clip_drag(
        &mut self,
        event: &gpui::DragMoveEvent<ClipDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.over_drop_area(event, cx) {
            return;
        }
        let drag = event.drag(cx).clone();
        let x = f32::from(event.event.position.x - event.bounds.left()) - drag.grab;
        let y = f32::from(event.event.position.y - event.bounds.top());
        let Some(track_id) = self.track_at(cx, y) else {
            return;
        };
        let Some(duration) = self
            .app
            .read(cx)
            .element_by_id(&drag.id)
            .map(|element| element.base().duration)
        else {
            return;
        };
        let (start, snapped) = self.snapped_start(cx, self.time_at(x), duration, Some(&drag.id));
        self.ghost = Some(Ghost {
            track_id,
            start,
            duration,
            snapped,
        });
        cx.notify();
    }

    fn on_clip_drop(&mut self, drag: &ClipDrag, cx: &mut Context<Self>) {
        let Some(ghost) = self.ghost.take() else {
            return;
        };
        let id = drag.id.clone();
        let track_id = ghost.track_id.clone();
        let start = ghost.start;
        self.app.update(cx, |model, cx| {
            model.edit(cx, |editor| editor.move_element(&id, &track_id, start));
        });
        cx.notify();
    }

    fn on_trim_drag(
        &mut self,
        event: &gpui::DragMoveEvent<TrimDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.over_drop_area(event, cx) {
            return;
        }
        let drag = event.drag(cx).clone();
        let Some(element) = self.app.read(cx).element_by_id(&drag.id).cloned() else {
            return;
        };
        let base = element.base();
        let pointer = f32::from(event.event.position.x - event.bounds.left());
        let anchor = match drag.edge {
            Edge::Start => base.start_time,
            Edge::End => element.end_time(),
        };
        let (target, snapped) =
            self.snapped_start(cx, self.time_at(pointer), MediaTime::ZERO, Some(&drag.id));
        let delta = MediaTime::from_ticks(target.as_ticks() - anchor.as_ticks());
        let (start, duration) = match drag.edge {
            Edge::Start => (
                target,
                MediaTime::from_ticks(base.duration.as_ticks() - delta.as_ticks()),
            ),
            Edge::End => (
                base.start_time,
                MediaTime::from_ticks(base.duration.as_ticks() + delta.as_ticks()),
            ),
        };
        let track_id = self
            .app
            .read(cx)
            .track_of(&drag.id)
            .unwrap_or_else(|| String::from(""));
        self.ghost = Some(Ghost {
            track_id,
            start,
            duration: duration.max(MediaTime::ZERO),
            snapped,
        });
        cx.notify();
    }

    fn on_trim_drop(&mut self, drag: &TrimDrag, cx: &mut Context<Self>) {
        let Some(ghost) = self.ghost.take() else {
            return;
        };
        let Some(element) = self.app.read(cx).element_by_id(&drag.id).cloned() else {
            return;
        };
        let delta = match drag.edge {
            Edge::Start => {
                MediaTime::from_ticks(ghost.start.as_ticks() - element.base().start_time.as_ticks())
            }
            Edge::End => MediaTime::from_ticks(
                ghost.start.as_ticks() + ghost.duration.as_ticks() - element.end_time().as_ticks(),
            ),
        };
        let id = drag.id.clone();
        let edge = drag.edge;
        self.app.update(cx, |model, cx| {
            model.edit(cx, |editor| editor.trim_element(&id, edge, delta));
        });
        cx.notify();
    }

    fn on_media_drag(
        &mut self,
        event: &gpui::DragMoveEvent<MediaDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.over_drop_area(event, cx) {
            return;
        }
        let drag = event.drag(cx).clone();
        let x = f32::from(event.event.position.x - event.bounds.left());
        let y = f32::from(event.event.position.y - event.bounds.top());
        let Some(asset) = self.app.read(cx).media_by_id(&drag.media_id).cloned() else {
            return;
        };
        let duration = edit::element_for(&asset).base().duration;
        self.place_ghost(x, y, duration, None, cx);
    }

    fn place_ghost(
        &mut self,
        x: f32,
        y: f32,
        duration: MediaTime,
        exclude: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let Some(track_id) = self.track_at(cx, y) else {
            return;
        };
        let (start, snapped) = self.snapped_start(cx, self.time_at(x), duration, exclude);
        self.ghost = Some(Ghost {
            track_id,
            start,
            duration,
            snapped,
        });
        cx.notify();
    }

    fn on_insert_drag(
        &mut self,
        event: &gpui::DragMoveEvent<InsertDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.over_drop_area(event, cx) {
            return;
        }
        let drag = event.drag(cx).clone();
        let x = f32::from(event.event.position.x - event.bounds.left());
        let y = f32::from(event.event.position.y - event.bounds.top());
        let Some(element) = insert_drag_element(&drag) else {
            return;
        };
        let duration = element.base().duration;
        self.place_ghost(x, y, duration, None, cx);
    }

    fn on_insert_drop(&mut self, drag: &InsertDrag, cx: &mut Context<Self>) {
        let Some(ghost) = self.ghost.take() else {
            return;
        };
        let Some(element) = insert_drag_element(drag) else {
            return;
        };
        let element_id = element.base().id.clone();
        let start = ghost.start;
        let track_id = ghost.track_id.clone();
        self.app.update(cx, |model, cx| {
            if model.edit(cx, |editor| editor.insert_element(element, start)) {
                model.edit(cx, |editor| {
                    editor.move_element(&element_id, &track_id, start)
                });
            }
        });
        cx.notify();
    }

    fn element_at(&self, cx: &App, offset_x: f32, offset_y: f32) -> Option<String> {
        let track_id = self.track_at(cx, offset_y)?;
        let time = self.time_at(offset_x).as_ticks();
        self.app
            .read(cx)
            .tracks()
            .into_iter()
            .find(|track| track.id() == track_id)?
            .elements()
            .iter()
            .find(|element| {
                let base = element.base();
                time >= base.start_time.as_ticks()
                    && time < base.start_time.as_ticks() + base.duration.as_ticks()
            })
            .map(|element| element.base().id.clone())
    }

    fn accepts_clip_target(&self, cx: &App, drag: &ClipTargetDrag, element_id: &str) -> bool {
        let Some(element) = self.app.read(cx).element_by_id(element_id) else {
            return false;
        };
        match drag {
            ClipTargetDrag::Effect(_) => matches!(
                element,
                TimelineElement::Video(_) | TimelineElement::Image(_) | TimelineElement::Text(_)
            ),
            ClipTargetDrag::Transition(_) => {
                if !cutix_playback::transitions::can_element_have_transition(element) {
                    return false;
                }
                let Some(track_id) = self.app.read(cx).track_of(element_id) else {
                    return false;
                };
                let start = element.base().start_time.as_ticks();
                self.app
                    .read(cx)
                    .tracks()
                    .into_iter()
                    .find(|track| track.id() == track_id)
                    .is_some_and(|track| {
                        track.elements().iter().any(|other| {
                            let base = other.base();
                            base.id != element_id
                                && cutix_playback::transitions::can_element_have_transition(other)
                                && (base.start_time.as_ticks() + base.duration.as_ticks() - start)
                                    .abs()
                                    <= cutix_playback::transitions::ADJACENCY_TOLERANCE_TICKS
                        })
                    })
            }
        }
    }

    fn on_clip_target_drag(
        &mut self,
        event: &gpui::DragMoveEvent<ClipTargetDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.over_drop_area(event, cx) {
            return;
        }
        let drag = event.drag(cx).clone();
        let x = f32::from(event.event.position.x - event.bounds.left());
        let y = f32::from(event.event.position.y - event.bounds.top());
        self.drop_target = self
            .element_at(cx, x, y)
            .filter(|id| self.accepts_clip_target(cx, &drag, id));
        cx.notify();
    }

    fn on_clip_target_drop(&mut self, drag: &ClipTargetDrag, cx: &mut Context<Self>) {
        let Some(element_id) = self.drop_target.take() else {
            return;
        };
        if !self.accepts_clip_target(cx, drag, &element_id) {
            cx.notify();
            return;
        }
        match drag {
            ClipTargetDrag::Effect(effect_type) => {
                let applied = self
                    .app
                    .read(cx)
                    .element_by_id(&element_id)
                    .is_some_and(|element| {
                        edit::effects_of(element)
                            .iter()
                            .any(|effect| effect.effect_type == *effect_type)
                    });
                if !applied {
                    let kind = (*effect_type).to_owned();
                    self.app.update(cx, |model, cx| {
                        model.edit(cx, |editor| {
                            editor.add_clip_effect(&element_id, &kind).is_some()
                        });
                    });
                }
            }
            ClipTargetDrag::Transition(transition_type) => {
                let transition = cutix_project::model::ElementTransition {
                    transition_type: (*transition_type).to_owned(),
                    duration: MediaTime::from_seconds_f64(1.0).unwrap_or(MediaTime::ZERO),
                    easing: Some(cutix_playback::transitions::DEFAULT_TRANSITION_EASING.to_owned()),
                };
                self.app.update(cx, |model, cx| {
                    model.edit(cx, |editor| {
                        editor.set_element_transition(&element_id, Some(transition))
                    });
                });
            }
        }
        self.app
            .update(cx, |model, cx| model.select_only(&element_id, cx));
        cx.notify();
    }

    fn on_media_drop(&mut self, drag: &MediaDrag, cx: &mut Context<Self>) {
        let Some(ghost) = self.ghost.take() else {
            return;
        };
        let Some(asset) = self.app.read(cx).media_by_id(&drag.media_id).cloned() else {
            return;
        };
        let start = ghost.start;
        let track_id = ghost.track_id.clone();
        self.app.update(cx, |model, cx| {
            model.edit(cx, |editor| {
                editor.insert_media(&asset, start, Some(&track_id))
            });
        });
        cx.notify();
    }

    pub fn toggle_snapping(&mut self, cx: &mut Context<Self>) {
        self.snapping = !self.snapping;
        let snapping = self.snapping;
        self.app.update(cx, |model, cx| {
            model.snapping = snapping;
            cx.notify();
        });
        cx.notify();
    }

    pub fn toggle_ripple(&mut self, cx: &mut Context<Self>) {
        self.ripple = !self.ripple;
        let ripple = self.ripple;
        self.app.update(cx, |model, cx| {
            model.ripple = ripple;
            cx.notify();
        });
        cx.notify();
    }

    fn run_tool(&mut self, id: &str, cx: &mut Context<Self>) {
        if id == "graph" {
            self.graph_open = !self.graph_open;
            cx.notify();
            return;
        }
        let playhead = self.app.read(cx).playhead;
        self.app.update(cx, |model, cx| match id {
            "split" => {
                model.edit(cx, |editor| editor.split_at(playhead));
            }
            "duplicate" => {
                model.edit(cx, |editor| editor.duplicate_selected());
            }
            "delete" => {
                model.edit(cx, |editor| editor.delete_selected());
            }
            "align-start" => {
                model.edit(cx, |editor| {
                    editor.split_retaining(playhead, edit::Retain::Right)
                });
            }
            "align-end" => {
                model.edit(cx, |editor| {
                    editor.split_retaining(playhead, edit::Retain::Left)
                });
            }
            "bookmark" => {
                model.edit(cx, |editor| editor.toggle_bookmark(playhead));
            }
            "link" => {
                let Some((element_id, has_audio)) = model.source_audio_target() else {
                    return;
                };
                model.edit(cx, |editor| {
                    editor.toggle_source_audio(&element_id, has_audio)
                });
            }
            _ => {}
        });
        cx.notify();
    }

    fn graph_field_for_path(path: &str) -> Option<edit::Field> {
        use edit::Field::*;
        Some(match path {
            "transform.positionX" => PositionX,
            "transform.positionY" => PositionY,
            "transform.scaleX" => ScaleX,
            "transform.scaleY" => ScaleY,
            "transform.rotate" => Rotate,
            "opacity" => Opacity,
            "crop.left" => CropLeft,
            "crop.top" => CropTop,
            "crop.right" => CropRight,
            "crop.bottom" => CropBottom,
            "volume" => Volume,
            "background.paddingX" => BackgroundPaddingX,
            "background.paddingY" => BackgroundPaddingY,
            "background.offsetX" => BackgroundOffsetX,
            "background.offsetY" => BackgroundOffsetY,
            "background.cornerRadius" => BackgroundCornerRadius,
            _ => return None,
        })
    }

    fn graph_value_axis(keys: &[graph_editor::PlotKey], size: f32) -> Axis {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for key in keys {
            min = min.min(key.value);
            max = max.max(key.value);
        }
        if !min.is_finite() || !max.is_finite() {
            min = 0.0;
            max = 1.0;
        }
        if (max - min).abs() < 1e-6 {
            min -= 1.0;
            max += 1.0;
        }
        let pad_value = (max - min) * 0.15;
        Axis::new(
            min - pad_value,
            max + pad_value,
            size,
            GRAPH_PLOT_PAD_PX,
            true,
        )
    }

    fn on_graph_key_drag(
        &mut self,
        event: &gpui::DragMoveEvent<GraphKeyDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.graph_key_drag.as_mut() else {
            return;
        };
        let bounds = event.bounds;
        let height = f32::from(bounds.bottom() - bounds.top());
        let axis = Axis::new(
            state.val_min,
            state.val_max,
            height,
            GRAPH_PLOT_PAD_PX,
            true,
        );
        let local_y = f32::from(event.event.position.y - bounds.top());
        state.preview = axis.to_value(local_y);
        cx.notify();
    }

    fn on_graph_key_drop(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.graph_key_drag.take() else {
            return;
        };
        let tick = MediaTime::from_ticks(state.key_tick);
        self.app.update(cx, |model, cx| {
            model.edit(cx, |editor| {
                editor.set_keyframe(&state.element_id, state.field, tick, state.preview)
            });
        });
        cx.notify();
    }

    fn graph_pane(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        if !self.graph_open {
            return None;
        }
        let colors = self.colors(cx);

        let element = self.app.read(cx).selected_element();
        let data = element.and_then(|element| {
            let animations = element.base().animations.as_ref()?;
            let tracks = graph_editor::plot_tracks(animations);
            if tracks.is_empty() {
                return None;
            }
            Some((
                element.base().id.clone(),
                element.base().duration.as_ticks(),
                tracks,
            ))
        });

        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(GRAPH_PANE_HEADER_PX))
            .flex_shrink_0()
            .px(px(12.0))
            .border_b_1()
            .border_color(colors.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        svg()
                            .size(px(14.0))
                            .path(icon("chart03"))
                            .text_color(colors.foreground),
                    )
                    .child(
                        div()
                            .text_size(rem(TEXT_SM))
                            .text_color(colors.foreground)
                            .child(t("timeline.graph.open")),
                    ),
            )
            .child(
                Button::new("graph-close", colors)
                    .variant(ButtonVariant::Text)
                    .size(ButtonSize::Icon)
                    .icon("cancel01")
                    .build()
                    .size(px(24.0))
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.graph_open = false;
                        cx.notify();
                    })),
            );

        let mut pane = div()
            .flex()
            .flex_col()
            .w_full()
            .h(px(GRAPH_PANE_HEIGHT_PX))
            .flex_shrink_0()
            .border_t_1()
            .border_color(colors.border)
            .bg(colors.background)
            .child(header);

        let Some((element_id, duration_ticks, tracks)) = data else {
            pane = pane.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(t("timeline.graph.msg.selectKeyframe")),
            );
            return Some(pane);
        };

        let selected = self
            .graph_selected_path
            .as_ref()
            .and_then(|path| tracks.iter().position(|track| &track.path == path))
            .unwrap_or(0);

        let list = div()
            .flex()
            .flex_col()
            .w(px(GRAPH_PANE_LIST_WIDTH_PX))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(colors.border)
            .py(px(6.0))
            .children(tracks.iter().enumerate().map(|(index, track)| {
                let active = index == selected;
                let path = track.path.clone();
                div()
                    .id(SharedString::from(format!("graph-prop-{}", track.path)))
                    .flex()
                    .items_center()
                    .h(px(26.0))
                    .px(px(10.0))
                    .cursor_pointer()
                    .text_size(rem(TEXT_XS))
                    .text_color(if active {
                        colors.primary
                    } else {
                        colors.muted_foreground
                    })
                    .when(active, |element| element.bg(opacity(colors.primary, 0.08)))
                    .child(graph_property_label(&track.path))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.graph_selected_path = Some(path.clone());
                        cx.notify();
                    }))
            }));

        let track = &tracks[selected];
        let time_axis_min = 0.0;
        let time_axis_max = duration_ticks.max(1) as f64;
        let dragging = self.graph_key_drag.clone();

        let value_axis_static = Self::graph_value_axis(&track.keys, GRAPH_PLOT_HEIGHT_PX);
        let points = track
            .keys
            .iter()
            .map(|key| {
                let time_fraction = ((key.tick as f64 - time_axis_min)
                    / (time_axis_max - time_axis_min).max(1.0))
                    as f32;
                let is_dragged = dragging
                    .as_ref()
                    .is_some_and(|state| state.key_tick == key.tick);
                let value = if is_dragged {
                    dragging
                        .as_ref()
                        .map(|state| state.preview)
                        .unwrap_or(key.value)
                } else {
                    key.value
                };
                let value_fraction = value_axis_static.to_px(value) / GRAPH_PLOT_HEIGHT_PX;
                let field = Self::graph_field_for_path(&track.path);
                let element_id = element_id.clone();
                let key_tick = key.tick;
                let start_value = key.value;
                let val_min = value_axis_static.min;
                let val_max = value_axis_static.max;
                div()
                    .id(SharedString::from(format!(
                        "graph-key-{}-{key_tick}",
                        track.path
                    )))
                    .absolute()
                    .left(relative(time_fraction.clamp(0.0, 1.0)))
                    .top(relative(value_fraction.clamp(0.0, 1.0)))
                    .w(px(GRAPH_KEY_RADIUS_PX * 2.0))
                    .h(px(GRAPH_KEY_RADIUS_PX * 2.0))
                    .ml(px(-GRAPH_KEY_RADIUS_PX))
                    .mt(px(-GRAPH_KEY_RADIUS_PX))
                    .rounded_full()
                    .bg(colors.primary)
                    .border_1()
                    .border_color(colors.background)
                    .when(field.is_some(), |element| {
                        element.cursor(gpui::CursorStyle::ResizeUpDown)
                    })
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this: &mut Self, _, _, cx| {
                            let Some(field) = field else {
                                return;
                            };
                            this.graph_key_drag = Some(GraphKeyDragState {
                                element_id: element_id.clone(),
                                field,
                                key_tick,
                                val_min,
                                val_max,
                                preview: start_value,
                            });
                            cx.notify();
                        }),
                    )
                    .on_drag(GraphKeyDrag, |_, _, _, cx| cx.new(|_| gpui::Empty))
            })
            .collect::<Vec<_>>();

        let plot_keys = track.keys.clone();
        let curve_color = colors.primary;
        let grid_color = opacity(colors.foreground, 0.08);
        let plot = div()
            .id("graph-plot")
            .relative()
            .flex_1()
            .h_full()
            .overflow_hidden()
            .on_drag_move::<GraphKeyDrag>(cx.listener(Self::on_graph_key_drag))
            .on_drop(
                cx.listener(|this: &mut Self, _: &GraphKeyDrag, _, cx| this.on_graph_key_drop(cx)),
            )
            .child(graph_plot_canvas(
                plot_keys,
                time_axis_min,
                time_axis_max,
                value_axis_static,
                curve_color,
                grid_color,
            ))
            .children(points);

        pane = pane.child(
            div()
                .flex()
                .flex_1()
                .w_full()
                .min_h_0()
                .child(list)
                .child(plot),
        );
        Some(pane)
    }

    fn scene_menu(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let frame = self.scene_menu.frame();
        if !frame.visible {
            return None;
        }
        let colors = self.colors(cx);
        let scenes = self.app.read(cx).scene_names();
        let current = self
            .app
            .read(cx)
            .current_scene()
            .map(|scene| scene.id.clone())
            .unwrap_or_default();
        let action_count = if self
            .app
            .read(cx)
            .current_scene()
            .is_some_and(|scene| scene.is_main)
        {
            SCENE_ACTIONS.len() - 1
        } else {
            SCENE_ACTIONS.len()
        };
        let natural = (
            SCENE_MENU_WIDTH_PX,
            menu_natural_height(
                scenes.len().max(1) + action_count,
                crate::components::MENU_ITEM_HEIGHT_PX,
            ) + SCENE_MENU_SEPARATOR_PX,
        );
        let placement = place_anchored(
            frame,
            OverlaySide::Bottom,
            natural,
            (0.0, 28.0 + MENU_OFFSET_PX),
        );

        let items = scenes
            .into_iter()
            .map(|(id, name)| {
                let key = format!("scene-{id}");
                let highlighted = self.transitions.eased(&key) > 0.5;
                let selected = id == current;
                let pick = id.clone();
                menu_item(
                    SharedString::from(key.clone()),
                    colors,
                    name,
                    highlighted,
                    selected,
                )
                .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                    this.transitions.set(key.clone(), *hovered);
                    cx.notify();
                }))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    let id = pick.clone();
                    this.app.update(cx, |model, cx| model.set_scene(&id, cx));
                    this.scene_menu.dismiss();
                    cx.notify();
                }))
            })
            .collect::<Vec<_>>();

        let can_delete = self
            .app
            .read(cx)
            .current_scene()
            .is_some_and(|scene| !scene.is_main);
        let actions = SCENE_ACTIONS
            .iter()
            .filter(|(key, _)| *key != "scene-delete" || can_delete)
            .map(|(key, label)| {
                let action = *key;
                let highlighted = self.transitions.eased(action) > 0.5;
                menu_item(
                    SharedString::from(action),
                    colors,
                    t(label),
                    highlighted,
                    false,
                )
                .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                    this.transitions.set(action.to_string(), *hovered);
                    cx.notify();
                }))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.run_scene_action(action, cx);
                }))
            })
            .collect::<Vec<_>>();

        Some(
            crate::components::overlay_root()
                .child(overlay_backdrop("scene-backdrop").on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.scene_menu.dismiss();
                        cx.notify();
                    }),
                ))
                .child(overlay_layer(
                    gpui::Corner::TopLeft,
                    placement,
                    menu_surface(colors, placement)
                        .children(items)
                        .child(separator_h(colors))
                        .children(actions),
                )),
        )
    }

    fn run_scene_action(&mut self, action: &str, cx: &mut Context<Self>) {
        self.scene_menu.dismiss();
        let current = self
            .app
            .read(cx)
            .current_scene()
            .map(|scene| (scene.id.clone(), scene.name.clone(), scene.is_main));
        match action {
            "scene-new" => {
                self.app.update(cx, |model, cx| {
                    model.edit(cx, |editor| editor.create_scene(t("scenes.new")).is_some());
                });
            }
            "scene-rename" => {
                if let Some((id, name, _)) = current {
                    let field = TextField::new(cx, name);
                    self.scene_rename = Some((id, field));
                }
            }
            "scene-delete" => {
                if let Some((id, _, is_main)) = current {
                    if !is_main {
                        self.app.update(cx, |model, cx| {
                            model.edit(cx, |editor| editor.delete_scene(&id));
                        });
                    }
                }
            }
            _ => {}
        }
        cx.notify();
    }

    fn context_menu(
        &mut self,
        key: &'static str,
        open: bool,
        frame: crate::interaction::OverlayFrame,
        anchor: (f32, f32),
        rows: Vec<(String, SharedString, bool)>,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        if !open || !frame.visible || rows.is_empty() {
            return None;
        }
        let colors = self.colors(cx);
        let natural = (
            CONTEXT_MENU_WIDTH_PX,
            menu_natural_height(rows.len(), crate::components::MENU_ITEM_HEIGHT_PX),
        );
        let placement = place_anchored(frame, OverlaySide::Bottom, natural, anchor);

        let items = rows
            .into_iter()
            .map(|(action, label, checked)| {
                let hover_key = format!("{key}-{action}");
                let highlighted = self.transitions.eased(&hover_key) > 0.5;
                let pick = action.clone();
                menu_item(
                    SharedString::from(hover_key.clone()),
                    colors,
                    label,
                    highlighted,
                    checked,
                )
                .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                    this.transitions.set(hover_key.clone(), *hovered);
                    cx.notify();
                }))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.run_menu_action(&pick, cx);
                }))
            })
            .collect::<Vec<_>>();

        Some(
            crate::components::overlay_root()
                .child(
                    overlay_backdrop(SharedString::from(format!("{key}-backdrop"))).on_mouse_up(
                        gpui::MouseButton::Left,
                        cx.listener(|this: &mut Self, _, _, cx| {
                            this.dismiss_menus();
                            cx.notify();
                        }),
                    ),
                )
                .child(overlay_layer(
                    gpui::Corner::TopLeft,
                    placement,
                    menu_surface(colors, placement).children(items),
                )),
        )
    }

    fn dismiss_menus(&mut self) {
        self.scene_menu.dismiss();
        self.add_track_menu.dismiss();
        self.track_menu.dismiss();
        self.clip_menu.dismiss();
        self.bookmark_menu.dismiss();
        self.track_menu_for = None;
        self.clip_menu_for = None;
        self.bookmark_menu_for = None;
        self.bookmark_note = None;
    }

    pub fn cancel_overlays(&mut self, cx: &mut Context<Self>) {
        self.dismiss_menus();
        self.scene_rename = None;
        cx.notify();
    }

    fn run_menu_action(&mut self, action: &str, cx: &mut Context<Self>) {
        let track_id = self.track_menu_for.clone();
        let clip_id = self.clip_menu_for.clone();
        let bookmark = self.bookmark_menu_for;
        self.dismiss_menus();

        match action {
            "remove-track" => {
                if let Some(id) = track_id {
                    self.app.update(cx, |model, cx| {
                        model.edit(cx, |editor| editor.remove_track(&id));
                    });
                }
            }
            "element-mute" => {
                if let Some(id) = clip_id {
                    self.app.update(cx, |model, cx| {
                        model.edit(cx, |editor| editor.toggle_elements_muted(&[id]));
                    });
                }
            }
            "element-hide" => {
                if let Some(id) = clip_id {
                    self.app.update(cx, |model, cx| {
                        model.edit(cx, |editor| editor.toggle_elements_hidden(&[id]));
                    });
                }
            }
            "element-source-audio" => {
                if let Some(id) = clip_id {
                    self.app.update(cx, |model, cx| {
                        let has_audio = model
                            .element_by_id(&id)
                            .and_then(TimelineElement::media_id)
                            .is_some_and(|media_id| model.media_has_audio(media_id));
                        model.edit(cx, |editor| editor.toggle_source_audio(&id, has_audio));
                    });
                }
            }
            "bookmark-remove" => {
                if let Some(time) = bookmark {
                    self.app.update(cx, |model, cx| {
                        model.edit(cx, |editor| editor.remove_bookmark(time));
                    });
                }
            }
            _ => {
                if let Some(kind) = edit::TrackKind::ALL
                    .into_iter()
                    .find(|kind| action == format!("add-track-{}", kind.id()))
                {
                    self.app.update(cx, |model, cx| {
                        model.edit(cx, |editor| editor.add_track(kind).is_some());
                    });
                }
            }
        }
        cx.notify();
    }

    fn add_track_menu(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let frame = self.add_track_menu.frame();
        let open = self.add_track_menu.is_open() || frame.visible;
        let rows = edit::TrackKind::ALL
            .into_iter()
            .map(|kind| {
                (
                    format!("add-track-{}", kind.id()),
                    SharedString::from(t(kind.label_key())),
                    false,
                )
            })
            .collect();
        self.context_menu(
            "add-track",
            open,
            frame,
            (0.0, 24.0 + MENU_OFFSET_PX),
            rows,
            cx,
        )
    }

    fn track_menu(&mut self, track_id: &str, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if self.track_menu_for.as_deref() != Some(track_id) {
            return None;
        }
        let frame = self.track_menu.frame();
        let rows = vec![(
            String::from("remove-track"),
            SharedString::from(t("timeline.deleteTrack")),
            false,
        )];
        self.context_menu(
            "track",
            self.track_menu.is_open() || frame.visible,
            frame,
            (0.0, 20.0),
            rows,
            cx,
        )
    }

    fn clip_menu(&mut self, element_id: &str, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if self.clip_menu_for.as_deref() != Some(element_id) {
            return None;
        }
        let frame = self.clip_menu.frame();
        let model = self.app.read(cx);
        let element = model.element_by_id(element_id)?.clone();
        let has_audio = element
            .media_id()
            .is_some_and(|media_id| model.media_has_audio(media_id));

        let mut rows = Vec::new();
        if edit::element_can_have_audio(&element) {
            rows.push((
                String::from("element-mute"),
                SharedString::from(t(if edit::element_muted(&element) {
                    "timeline.unmuteElement"
                } else {
                    "timeline.muteElement"
                })),
                edit::element_muted(&element),
            ));
        }
        if edit::element_can_be_hidden(&element) {
            rows.push((
                String::from("element-hide"),
                SharedString::from(t(if edit::element_hidden(&element) {
                    "timeline.showElement"
                } else {
                    "timeline.hideElement"
                })),
                edit::element_hidden(&element),
            ));
        }
        if edit::can_toggle_source_audio(&element, has_audio) {
            let separated = matches!(&element, TimelineElement::Video(video)
                if edit::source_audio_separated(video));
            rows.push((
                String::from("element-source-audio"),
                SharedString::from(t(if separated {
                    "properties.audio.recover"
                } else {
                    "timeline.audio.extract"
                })),
                false,
            ));
        }

        self.context_menu(
            "clip",
            self.clip_menu.is_open() || frame.visible,
            frame,
            (0.0, 20.0),
            rows,
            cx,
        )
    }

    fn bookmark_strip(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let pps = self.pixels_per_second();
        let bookmarks = self.app.read(cx).bookmarks().to_vec();

        let markers = bookmarks
            .iter()
            .map(|bookmark| {
                let time = bookmark.time;
                let display = match self.bookmark_drag {
                    Some((from, to)) if from == time => to,
                    _ => time,
                };
                let fill = crate::theme::parse_hex(
                    bookmark.color.as_deref().unwrap_or(DEFAULT_BOOKMARK_COLOR),
                );
                let span = bookmark
                    .duration
                    .map(|duration| duration.to_seconds_f64() as f32 * pps)
                    .unwrap_or(0.0)
                    .max(0.0);
                let left = display.to_seconds_f64() as f32 * pps - BOOKMARK_MARKER_WIDTH_PX / 2.0;
                let popover = self.bookmark_popover(time, window, cx);

                div()
                    .id(SharedString::from(format!("bookmark-{}", time.as_ticks())))
                    .absolute()
                    .top(px(0.0))
                    .left(px(left.max(0.0)))
                    .h(px(BOOKMARK_MARKER_HEIGHT_PX))
                    .w(px(BOOKMARK_MARKER_WIDTH_PX + span))
                    .cursor_pointer()
                    .when(span > 1.0, |this| {
                        this.child(
                            div()
                                .absolute()
                                .top(px(1.5))
                                .left(px(BOOKMARK_MARKER_WIDTH_PX / 2.0))
                                .w(px(span))
                                .h(px(BOOKMARK_MARKER_HEIGHT_PX - 2.5))
                                .bg(opacity(fill, 0.3)),
                        )
                    })
                    .child(
                        div()
                            .absolute()
                            .top(px(0.0))
                            .left(px(0.0))
                            .size(px(BOOKMARK_MARKER_WIDTH_PX))
                            .child(
                                svg()
                                    .size(px(BOOKMARK_MARKER_WIDTH_PX))
                                    .path(icon("bookmark02"))
                                    .text_color(fill),
                            ),
                    )
                    .on_drag(BookmarkDrag, |_, _, _, cx| cx.new(|_| gpui::Empty))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |this: &mut Self, _, _, cx| {
                            this.bookmark_drag = Some((time, time));
                            this.app.update(cx, |model, cx| model.seek(time, cx));
                            cx.notify();
                        }),
                    )
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(move |this: &mut Self, _, _, cx| {
                            this.dismiss_menus();
                            let note = this
                                .app
                                .read(cx)
                                .bookmarks()
                                .iter()
                                .find(|candidate| candidate.time == time)
                                .and_then(|candidate| candidate.note.clone())
                                .unwrap_or_default();
                            this.bookmark_menu_for = Some(time);
                            this.bookmark_note = Some(TextField::new(cx, note));
                            this.bookmark_menu.toggle();
                            cx.notify();
                        }),
                    )
                    .children(popover)
            })
            .collect::<Vec<_>>();

        div()
            .id("timeline-bookmarks")
            .relative()
            .w_full()
            .h(px(TIMELINE_BOOKMARK_ROW_HEIGHT_PX))
            .flex_shrink_0()
            .on_drag_move::<BookmarkDrag>(cx.listener(Self::on_bookmark_drag))
            .on_drop(
                cx.listener(|this: &mut Self, _: &BookmarkDrag, _, cx| {
                    this.commit_bookmark_drag(cx)
                }),
            )
            .children(markers)
    }

    fn on_bookmark_drag(
        &mut self,
        event: &gpui::DragMoveEvent<BookmarkDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((from, _)) = self.bookmark_drag else {
            return;
        };
        if !event.bounds.contains(&event.event.position) {
            return;
        }
        let offset = f32::from(event.event.position.x - event.bounds.left());
        let fps = self.fps(cx);
        self.bookmark_drag = Some((from, edit::snap_to_frame(self.time_at(offset), fps)));
        cx.notify();
    }

    fn commit_bookmark_drag(&mut self, cx: &mut Context<Self>) {
        let Some((from, to)) = self.bookmark_drag.take() else {
            return;
        };
        if from != to {
            self.app.update(cx, |model, cx| {
                model.edit(cx, |editor| editor.move_bookmark(from, to));
            });
        }
        cx.notify();
    }

    fn bookmark_popover(
        &mut self,
        time: MediaTime,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        if self.bookmark_menu_for != Some(time) {
            return None;
        }
        let frame = self.bookmark_menu.frame();
        if !self.bookmark_menu.is_open() && !frame.visible {
            return None;
        }
        let colors = self.colors(cx);
        let field = self.bookmark_note.as_ref()?;
        window.focus(&field.focus);
        let placement = place_anchored(
            frame,
            OverlaySide::Bottom,
            (CONTEXT_MENU_WIDTH_PX + 48.0, 108.0),
            (0.0, BOOKMARK_MARKER_HEIGHT_PX + MENU_OFFSET_PX),
        );
        let input = crate::input::text_field(
            "bookmark-note-input",
            field,
            colors,
            crate::input::FieldStyle {
                height: 32.0,
                placeholder: SharedString::from(t("timeline.bookmark.note.placeholder")),
                leading: None,
                ..Default::default()
            },
            window,
        )
        .on_key_down(cx.listener(
            move |this: &mut Self, event, window: &mut Window, cx| {
                let Some(field) = this.bookmark_note.as_mut() else {
                    return;
                };
                match crate::input::key_down_with_clipboard(&mut field.buffer, event, false, cx) {
                    crate::input::TextEvent::Submit => {
                        let note = field.text().trim().to_string();
                        this.dismiss_menus();
                        window.blur();
                        this.app.update(cx, |model, cx| {
                            model.edit(cx, |editor| {
                                editor.update_bookmark(time, edit::BookmarkUpdate::Note(Some(note)))
                            });
                        });
                    }
                    crate::input::TextEvent::Cancel => {
                        this.dismiss_menus();
                        window.blur();
                    }
                    crate::input::TextEvent::Changed | crate::input::TextEvent::Ignored => {}
                    _ => {}
                }
                cx.notify();
            },
        ));

        let remove_hover = self.transitions.eased("bookmark-remove");
        let remove = menu_item(
            "bookmark-remove",
            colors,
            t("timeline.bookmark.delete"),
            remove_hover > 0.5,
            false,
        )
        .on_hover(cx.listener(|this: &mut Self, hovered: &bool, _, cx| {
            this.transitions
                .set(String::from("bookmark-remove"), *hovered);
            cx.notify();
        }))
        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
            this.run_menu_action("bookmark-remove", cx);
        }));

        Some(
            crate::components::overlay_root()
                .child(overlay_backdrop("bookmark-backdrop").on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.dismiss_menus();
                        cx.notify();
                    }),
                ))
                .child(overlay_layer(
                    gpui::Corner::TopLeft,
                    placement,
                    menu_surface(colors, placement)
                        .gap(px(6.0))
                        .p(px(8.0))
                        .child(
                            div()
                                .text_size(rem(TEXT_XS))
                                .text_color(opacity(colors.popover_foreground, 0.7))
                                .child(t("common.note")),
                        )
                        .child(input)
                        .child(remove),
                )),
        )
    }

    fn scene_rename_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let (_, field) = self.scene_rename.as_ref()?;
        let colors = self.colors(cx);
        window.focus(&field.focus);
        let input = crate::input::text_field(
            "scene-rename-input",
            field,
            colors,
            crate::input::FieldStyle {
                height: 36.0,
                placeholder: SharedString::from(t("common.name")),
                leading: None,
                ..Default::default()
            },
            window,
        )
        .on_key_down(
            cx.listener(|this: &mut Self, event, window: &mut Window, cx| {
                let Some((id, field)) = this.scene_rename.as_mut() else {
                    return;
                };
                match crate::input::key_down_with_clipboard(&mut field.buffer, event, false, cx) {
                    crate::input::TextEvent::Submit => {
                        let id = id.clone();
                        let name = field.text().trim().to_string();
                        this.scene_rename = None;

                        window.blur();
                        if !name.is_empty() {
                            this.app.update(cx, |model, cx| {
                                model.edit(cx, |editor| editor.rename_scene(&id, name));
                            });
                        }
                    }
                    crate::input::TextEvent::Cancel => {
                        this.scene_rename = None;
                        window.blur();
                    }
                    crate::input::TextEvent::Changed | crate::input::TextEvent::Ignored => {}
                    _ => {}
                }
                cx.notify();
            }),
        );

        let placement = crate::components::MenuPlacement {
            left: 0.0,
            top: 28.0 + MENU_OFFSET_PX,
            width: SCENE_MENU_WIDTH_PX + 64.0,
            height: 84.0,
            opacity: 1.0,
        };

        Some(
            crate::components::overlay_root()
                .child(overlay_backdrop("scene-rename-backdrop").on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.scene_rename = None;
                        cx.notify();
                    }),
                ))
                .child(overlay_layer(
                    gpui::Corner::TopLeft,
                    placement,
                    menu_surface(colors, placement)
                        .gap(px(6.0))
                        .p(px(10.0))
                        .child(
                            div()
                                .text_size(rem(TEXT_XS))
                                .text_color(opacity(colors.popover_foreground, 0.7))
                                .child(t("dialog.rename.label")),
                        )
                        .child(input),
                )),
        )
    }

    fn clip(
        &mut self,
        element: &TimelineElement,
        fill: gpui::Hsla,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let colors = self.colors(cx);
        let pps = self.pixels_per_second();
        let base = element.base();
        let left = base.start_time.to_seconds_f64() as f32 * pps;
        let width = (base.duration.to_seconds_f64() as f32 * pps).max(2.0);
        let selected = self.app.read(cx).is_selected(&base.id);
        let targeted = self.drop_target.as_deref() == Some(base.id.as_str());
        let id = base.id.clone();
        let grab_id = base.id.clone();
        let start_id = base.id.clone();
        let end_id = base.id.clone();

        let media_id = element.media_id().map(str::to_owned);
        let missing = media_id
            .as_deref()
            .is_some_and(|id| self.app.read(cx).is_media_missing(id));
        if let Some(id) = media_id.as_deref() {
            if !missing {
                self.app
                    .update(cx, |model, cx| model.ensure_waveform(id, cx));
            }
        }
        let waveform = media_id
            .as_deref()
            .and_then(|id| self.app.read(cx).waveform(id).cloned())
            .map(|peaks| {
                let from = base.trim_start.to_seconds_f64();
                let to = from + base.duration.to_seconds_f64();
                waveform_bars(&peaks, from, to, width, colors)
            });

        let handle = |edge: Edge, element_id: String| {
            div()
                .id(SharedString::from(format!(
                    "trim-{element_id}-{}",
                    if edge == Edge::Start { "start" } else { "end" }
                )))
                .absolute()
                .top(px(0.0))
                .when(edge == Edge::Start, |this| this.left(px(0.0)))
                .when(edge == Edge::End, |this| this.right(px(0.0)))
                .w(px(TRIM_HANDLE_WIDTH_PX))
                .h_full()
                .cursor(gpui::CursorStyle::ResizeLeftRight)
                .bg(opacity(gpui::white(), if selected { 0.55 } else { 0.2 }))
                .on_drag(
                    TrimDrag {
                        id: element_id,
                        edge,
                    },
                    |_, _, _, cx| cx.new(|_| gpui::Empty),
                )
        };

        let menu_id = base.id.clone();
        let element_dimmed = edit::element_hidden(element) || edit::element_muted(element);
        let clip_menu = self.clip_menu(&base.id, cx);

        div()
            .id(SharedString::from(format!("clip-{id}")))
            .absolute()
            .left(px(left))
            .top(px(0.0))
            .w(px(width))
            .h_full()
            .flex()
            .items_center()
            .overflow_hidden()
            .rounded(rem(RADIUS_SM))
            .border_2()
            .border_color(if targeted {
                colors.caution
            } else if selected {
                colors.foreground
            } else if missing {
                colors.destructive
            } else {
                opacity(fill, 0.0)
            })
            .bg(if targeted {
                mix(fill, colors.caution, 0.35)
            } else if missing {
                gpui::Hsla {
                    s: fill.s * 0.25,
                    l: fill.l * 0.55,
                    ..fill
                }
            } else {
                fill
            })
            .cursor_pointer()
            .px(px(6.0))
            .text_size(px(10.0))
            .text_color(gpui::white())
            .on_click(
                cx.listener(move |this: &mut Self, event: &gpui::ClickEvent, _, cx| {
                    let id = id.clone();
                    let extend = event.modifiers().shift;
                    this.app.update(cx, |model, cx| {
                        if extend {
                            model.extend_selection(&id, cx);
                        } else {
                            model.select_only(&id, cx);
                        }
                    });
                }),
            )
            .on_drag(
                ClipDrag {
                    id: grab_id,
                    grab: 0.0,
                },
                |_, _, _, cx| cx.new(|_| gpui::Empty),
            )
            .children(waveform)
            .when(missing, |this| {
                this.child(
                    svg()
                        .path(crate::assets::icon("alert-circle"))
                        .size(px(11.0))
                        .flex_none()
                        .mr(px(4.0))
                        .text_color(gpui::white()),
                )
            })
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |this: &mut Self, _, _, cx| {
                    let id = menu_id.clone();
                    this.dismiss_menus();
                    this.app.update(cx, |model, cx| model.select_only(&id, cx));
                    this.clip_menu_for = Some(id);
                    this.clip_menu.toggle();
                    cx.notify();
                }),
            )
            .when(element_dimmed, |this| this.opacity(0.45))
            .children(clip_menu)
            .child(div().truncate().child(base.name.clone()))
            .child(handle(Edge::Start, start_id))
            .child(handle(Edge::End, end_id))
    }

    fn transition_markers(
        &mut self,
        track: &Track,
        pixels_per_second: f32,
        cx: &mut Context<Self>,
    ) -> Vec<Stateful<Div>> {
        let colors = self.colors(cx);
        let edges = cutix_playback::transitions::build_track_transition_edges(track);
        let mut markers: Vec<Stateful<Div>> = Vec::new();
        for element in track.elements() {
            let id = element.base().id.clone();
            let Some(edge) = edges.get(&id).and_then(|edges| edges.incoming.as_ref()) else {
                continue;
            };
            let left = edge.start_time.to_seconds_f64() as f32 * pixels_per_second;
            let width = ((edge.end_time.to_seconds_f64() - edge.start_time.to_seconds_f64())
                as f32
                * pixels_per_second)
                .max(14.0);
            let select = id.clone();
            markers.push(
                div()
                    .id(SharedString::from(format!("transition-{id}")))
                    .absolute()
                    .top(relative(0.5))
                    .left(px(left))
                    .w(px(width))
                    .h(px(16.0))
                    .mt(px(-8.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(rem(RADIUS_SM))
                    .border_1()
                    .border_color(opacity(colors.primary, 0.9))
                    .bg(opacity(colors.background, 0.85))
                    .cursor_pointer()
                    .child(
                        svg()
                            .size(px(10.0))
                            .path(icon("arrow-right-double"))
                            .text_color(colors.foreground),
                    )
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        let id = select.clone();
                        this.app.update(cx, |model, cx| model.select_only(&id, cx));
                        cx.notify();
                    })),
            );
        }
        markers
    }

    fn track_row(&mut self, track: &Track, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        let fill = track_fill(track, colors);
        let pps = self.pixels_per_second();
        let dimmed = edit::track_hidden(track);

        let elements: Vec<TimelineElement> = track.elements().to_vec();
        let clips = elements
            .iter()
            .map(|element| self.clip(element, opacity(fill, if dimmed { 0.35 } else { 1.0 }), cx))
            .collect::<Vec<_>>();

        let ghost = self
            .ghost
            .clone()
            .filter(|ghost| ghost.track_id == track.id());
        let markers = self.transition_markers(track, pps, cx);

        div()
            .relative()
            .w_full()
            .h(px(edit::track_height(track)))
            .flex_shrink_0()
            .rounded(rem(RADIUS_SM))
            .when(clips.is_empty(), |this| {
                this.border_2()
                    .border_dashed()
                    .border_color(opacity(colors.muted, 0.3))
            })
            .children(clips)
            .children(markers)
            .when_some(ghost, |this, ghost| {
                this.child(
                    div()
                        .absolute()
                        .top(px(0.0))
                        .left(px(ghost.start.to_seconds_f64() as f32 * pps))
                        .w(px((ghost.duration.to_seconds_f64() as f32 * pps).max(2.0)))
                        .h_full()
                        .rounded(rem(RADIUS_SM))
                        .border_2()
                        .border_color(if ghost.snapped {
                            colors.primary
                        } else {
                            colors.foreground
                        })
                        .bg(opacity(colors.foreground, 0.12)),
                )
            })
    }

    fn track_controls(&mut self, track: &Track, cx: &mut Context<Self>) -> Stateful<Div> {
        let colors = self.colors(cx);
        let glyph = match track {
            Track::Audio { .. } => "volume-high",
            Track::Text { .. } => "text",
            Track::Graphic { .. } => "happy01",
            Track::Effect { .. } => "magic-wand05",
            Track::Video { .. } => "video01",
        };
        let muted = edit::track_muted(track);
        let hidden = edit::track_hidden(track);
        let can_mute = edit::track_can_mute(track);
        let can_hide = edit::track_can_hide(track);
        let mute_id = track.id().to_string();
        let hide_id = track.id().to_string();

        let toggle = |id: SharedString,
                      glyph: &'static str,
                      active: bool,
                      enabled: bool,
                      colors: Palette| {
            div()
                .id(id)
                .flex()
                .size(px(20.0))
                .items_center()
                .justify_center()
                .when(enabled, |this| this.cursor_pointer())
                .child(
                    svg()
                        .size(px(16.0))
                        .path(icon(glyph))
                        .text_color(if !enabled {
                            opacity(colors.muted_foreground, 0.3)
                        } else if active {
                            colors.destructive
                        } else {
                            colors.muted_foreground
                        }),
                )
        };

        let menu_id = track.id().to_string();
        let removable = self
            .app
            .read(cx)
            .current_scene()
            .is_some_and(|scene| scene.tracks.main.id() != track.id());
        let menu = self.track_menu(track.id(), cx);

        div()
            .id(SharedString::from(format!("track-controls-{}", track.id())))
            .relative()
            .flex()
            .w_full()
            .h(px(edit::track_height(track)))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .gap(px(10.0))
            .when(removable, |this| {
                this.on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(move |this: &mut Self, _, _, cx| {
                        this.dismiss_menus();
                        this.track_menu_for = Some(menu_id.clone());
                        this.track_menu.toggle();
                        cx.notify();
                    }),
                )
            })
            .children(menu)
            .child(
                toggle(
                    SharedString::from(format!("mute-{}", track.id())),
                    if muted { "volume-mute" } else { "volume-high" },
                    muted,
                    can_mute,
                    colors,
                )
                .when(can_mute, |this| {
                    this.on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        let id = mute_id.clone();
                        this.app.update(cx, |model, cx| {
                            model.edit(cx, |editor| editor.toggle_track_mute(&id));
                        });
                    }))
                }),
            )
            .child(
                toggle(
                    SharedString::from(format!("hide-{}", track.id())),
                    if hidden { "eye-off" } else { "eye" },
                    hidden,
                    can_hide,
                    colors,
                )
                .when(can_hide, |this| {
                    this.on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        let id = hide_id.clone();
                        this.app.update(cx, |model, cx| {
                            model.edit(cx, |editor| editor.toggle_track_hidden(&id));
                        });
                    }))
                }),
            )
            .child(
                svg()
                    .size(px(16.0))
                    .path(icon(glyph))
                    .text_color(colors.muted_foreground),
            )
    }
}

const WAVEFORM_BAR_PITCH_PX: f32 = 3.0;
const WAVEFORM_MAX_BARS: usize = 512;

fn waveform_bars(
    peaks: &cutix_playback::WaveformPeaks,
    from_seconds: f64,
    to_seconds: f64,
    width: f32,
    colors: Palette,
) -> Div {
    let count = ((width / WAVEFORM_BAR_PITCH_PX).floor() as usize).clamp(1, WAVEFORM_MAX_BARS);
    let values = peaks.slice(from_seconds, to_seconds, count);
    if values.is_empty() {
        return div();
    }
    let tint = opacity(mix(gpui::white(), colors.foreground, 0.15), 0.5);
    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .gap(px(1.0))
        .px(px(2.0))
        .overflow_hidden()
        .children(values.into_iter().map(move |value| {
            div()
                .flex_1()
                .min_w_0()
                .h(relative((value.clamp(0.02, 1.0) * 0.82).max(0.04)))
                .rounded(px(1.0))
                .bg(tint)
        }))
}

fn track_fill(track: &Track, colors: Palette) -> gpui::Hsla {
    match track {
        Track::Video { .. } => colors.primary,
        Track::Text { .. } => track_color("text"),
        Track::Audio { .. } => track_color("audio"),
        Track::Graphic { .. } => track_color("graphic"),
        Track::Effect { .. } => track_color("effect"),
    }
}

const RULER_TICK_TOP_PX: f32 = 6.0;
const RULER_TICK_HEIGHT_PX: f32 = 6.0;
const RULER_LABEL_TOP_PX: f32 = 4.0;
const RULER_SPAN_PX: f32 = 2000.0;

pub fn clamp_playhead(time: MediaTime, total: MediaTime) -> MediaTime {
    let floor = time.max(MediaTime::ZERO);
    if total <= MediaTime::ZERO {
        return floor;
    }
    floor.min(total)
}
const TRIM_HANDLE_WIDTH_PX: f32 = 6.0;

fn ruler(colors: Palette, zoom_level: f32, fps: f32, span: f32) -> Div {
    let config = ruler_config(zoom_level, fps);
    let tick_spacing = config.tick_spacing_px().max(1.0);
    let label_spacing = config.label_spacing_px().max(1.0);

    let ticks = (0..)
        .map(|index| index as f32 * tick_spacing)
        .take_while(|left| *left <= span)
        .map(|left| {
            div()
                .absolute()
                .left(px(left))
                .top(px(RULER_TICK_TOP_PX))
                .w(px(1.0))
                .h(px(RULER_TICK_HEIGHT_PX))
                .bg(opacity(colors.muted_foreground, 0.25))
        })
        .collect::<Vec<_>>();

    let labels = (0..)
        .map(|index| {
            (
                index as f32 * label_spacing,
                index * config.label_interval_frames,
            )
        })
        .take_while(|(left, _)| *left <= span)
        .map(|(left, frame)| {
            div()
                .absolute()
                .left(px(left))
                .top(px(RULER_LABEL_TOP_PX))
                .text_size(px(10.0))
                .line_height(px(10.0))
                .text_color(opacity(colors.muted_foreground, 0.85))
                .child(ruler_label(frame, fps))
        })
        .collect::<Vec<_>>();

    div()
        .relative()
        .w_full()
        .h(px(TIMELINE_RULER_HEIGHT_PX))
        .flex_shrink_0()
        .overflow_hidden()
        .children(ticks)
        .children(labels)
}

impl Hoverable for TimelinePanel {
    fn transitions(&mut self) -> &mut Transitions {
        &mut self.transitions
    }

    fn tooltips(&mut self) -> &mut Tooltips {
        &mut self.tooltips
    }
}

impl Render for TimelinePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        let fps = self.fps(cx);

        let (left, right) = TIMELINE_TOOLBAR_LEFT.split_at(TIMELINE_TOOLBAR_SEPARATOR_AFTER);
        let build_tools = |group: &'static [(&'static str, &'static str, &'static str)],
                           this: &mut Self,
                           cx: &mut Context<Self>| {
            group
                .iter()
                .map(|(id, name, label)| {
                    (
                        *id,
                        *name,
                        *label,
                        this.transitions.eased(id),
                        this.tooltips.frame_for(id),
                    )
                })
                .collect::<Vec<_>>()
                .into_iter()
                .map(|(id, name, label, progress, tip)| {
                    let disabled = TIMELINE_TOOLBAR_DISABLED.contains(&id);
                    let button = Button::new(id, colors)
                        .variant(ButtonVariant::Text)
                        .size(ButtonSize::Icon)
                        .hover(if disabled { 0.0 } else { progress })
                        .disabled(disabled)
                        .icon(name)
                        .build()
                        .size(px(28.0))
                        .on_hover(hover_listener(id, cx))
                        .on_mouse_down(gpui::MouseButton::Left, press_listener(cx))
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            if !disabled {
                                this.run_tool(id, cx);
                            }
                        }));
                    tooltipped(button, colors, t(label), tip, OverlaySide::Bottom)
                })
                .collect::<Vec<_>>()
        };
        let left_tools = build_tools(left, self, cx);
        let right_tools = build_tools(right, self, cx);

        let snapping = self.snapping;
        let snapping_progress = self.transitions.eased("timeline-snapping");
        let snapping_tip = self.tooltips.frame_for("timeline-snapping");
        let ripple_tip = self.tooltips.frame_for("timeline-ripple");
        let ripple = self.ripple;
        let slider = slider_from_zoom(self.zoom_level);
        let scene_progress = self.transitions.eased("timeline-scene");
        let add_track_progress = self.transitions.eased("timeline-add-track");
        let ripple_progress = self.transitions.eased("timeline-ripple");
        let zoom_out_progress = self.transitions.eased("timeline-zoom-out");
        let zoom_in_progress = self.transitions.eased("timeline-zoom-in");

        let scene_label = self
            .app
            .read(cx)
            .current_scene()
            .map(|scene| scene.name.clone())
            .unwrap_or_else(|| t("editor.timeline.scene.none"));
        let scene_menu = self.scene_menu(cx);
        let scene_rename = self.scene_rename_dialog(window, cx);
        let add_track_menu = self.add_track_menu(cx);
        let bookmarks = self.bookmark_strip(window, cx);
        let graph_pane = self.graph_pane(cx);

        let pixels_per_second = self.pixels_per_second();
        let tracks: Vec<Track> = self.app.read(cx).tracks().into_iter().cloned().collect();
        let content_width = (self.app.read(cx).total_duration().to_seconds_f64() as f32
            * pixels_per_second)
            .max(RULER_SPAN_PX);
        let viewport = self.scroll.bounds();
        if let Some((target_x, target_y)) = self.pending_scroll.take() {
            let furthest = (content_width - f32::from(viewport.size.width)).max(0.0);
            self.scroll.set_offset(gpui::point(
                px(target_x.clamp(-furthest, 0.0)),
                px(target_y),
            ));
        }
        self.viewport_left = f32::from(viewport.origin.x);
        let settled = self.scroll.offset();
        self.scroll_left = f32::from(settled.x);
        self.scroll_top = f32::from(settled.y);
        let playhead_x = self.x_of(self.app.read(cx).playhead);
        let label_offset = f32::from(self.scroll.offset().y);

        let labels = tracks
            .iter()
            .map(|track| self.track_controls(track, cx))
            .collect::<Vec<_>>();
        let rows = tracks
            .iter()
            .map(|track| self.track_row(track, cx))
            .collect::<Vec<_>>();

        self.tooltips.tick();
        if self.transitions.animating() || self.tooltips.animating() || self.scene_menu.animating()
        {
            window.request_animation_frame();
        }

        let horizontal_bar = scrollbar_h(&self.scroll, colors);
        let vertical_bar = scrollbar_v(&self.scroll, colors);

        panel_frame(colors)
            .child(
                div()
                    .flex()
                    .w_full()
                    .h(px(TIMELINE_TOOLBAR_HEIGHT_PX))
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(colors.border)
                    .px(px(8.0))
                    .py(px(4.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .children(left_tools)
                            .child(div().mx(px(4.0)).child(separator_v(colors, 24.0)))
                            .children(right_tools),
                    )
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_shrink_0()
                            .child(
                                div()
                                    .id("timeline-scene")
                                    .flex()
                                    .h(px(28.0))
                                    .items_center()
                                    .overflow_hidden()
                                    .rounded(rem(0.82))
                                    .border_1()
                                    .border_color(opacity(colors.foreground, 0.1))
                                    .cursor_pointer()
                                    .text_size(rem(TEXT_SM))
                                    .bg(mix(colors.accent, colors.muted, scene_progress))
                                    .on_hover(hover_listener("timeline-scene", cx))
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.scene_menu.toggle();
                                        cx.notify();
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .h_full()
                                            .items_center()
                                            .px(px(12.0))
                                            .child(scene_label),
                                    )
                                    .child(
                                        div()
                                            .w(px(1.0))
                                            .h_full()
                                            .bg(opacity(colors.foreground, 0.15)),
                                    )
                                    .child(
                                        div().flex().h_full().items_center().px(px(8.0)).child(
                                            svg()
                                                .size(px(16.0))
                                                .path(icon("layers01"))
                                                .text_color(colors.foreground),
                                        ),
                                    ),
                            )
                            .children(scene_menu)
                            .children(scene_rename),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(tooltipped(
                                Button::new("timeline-snapping", colors)
                                    .variant(if snapping {
                                        ButtonVariant::Secondary
                                    } else {
                                        ButtonVariant::Text
                                    })
                                    .size(ButtonSize::Icon)
                                    .hover(snapping_progress)
                                    .icon("magnet")
                                    .build()
                                    .size(px(28.0))
                                    .on_hover(hover_listener("timeline-snapping", cx))
                                    .on_mouse_down(gpui::MouseButton::Left, press_listener(cx))
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.snapping = !this.snapping;
                                        let snapping = this.snapping;
                                        this.app.update(cx, |model, cx| {
                                            model.snapping = snapping;
                                            cx.notify();
                                        });
                                        cx.notify();
                                    })),
                                colors,
                                t("timeline.autoSnapping"),
                                snapping_tip,
                                OverlaySide::Bottom,
                            ))
                            .child(tooltipped(
                                Button::new("timeline-ripple", colors)
                                    .variant(if ripple {
                                        ButtonVariant::Secondary
                                    } else {
                                        ButtonVariant::Text
                                    })
                                    .size(ButtonSize::Icon)
                                    .hover(ripple_progress)
                                    .icon("oc-ripple")
                                    .build()
                                    .size(px(28.0))
                                    .on_hover(hover_listener("timeline-ripple", cx))
                                    .on_mouse_down(gpui::MouseButton::Left, press_listener(cx))
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.ripple = !this.ripple;
                                        let ripple = this.ripple;
                                        this.app.update(cx, |model, cx| {
                                            model.ripple = ripple;
                                            cx.notify();
                                        });
                                        cx.notify();
                                    })),
                                colors,
                                t("timeline.rippleEditing"),
                                ripple_tip,
                                OverlaySide::Bottom,
                            ))
                            .child(div().mx(px(4.0)).child(separator_v(colors, 24.0)))
                            .child(
                                text_button(
                                    "timeline-zoom-out",
                                    "search-minus",
                                    colors,
                                    zoom_out_progress,
                                )
                                .on_hover(hover_listener("timeline-zoom-out", cx))
                                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                    this.nudge_zoom(1.0 / TIMELINE_ZOOM_BUTTON_FACTOR);
                                    cx.notify();
                                })),
                            )
                            .child(
                                div()
                                    .id("timeline-zoom-slider")
                                    .flex()
                                    .w(px(112.0))
                                    .h(px(28.0))
                                    .items_center()
                                    .cursor_pointer()
                                    .on_drag(ZoomDrag, |_, _, _, cx| cx.new(|_| gpui::Empty))
                                    .on_drag_move::<ZoomDrag>(cx.listener(Self::on_zoom_drag))
                                    .child(
                                        div()
                                            .relative()
                                            .w_full()
                                            .h(px(6.0))
                                            .rounded(px(3.0))
                                            .bg(colors.accent)
                                            .child(
                                                div()
                                                    .w(relative(slider))
                                                    .h_full()
                                                    .rounded(px(3.0))
                                                    .bg(colors.foreground),
                                            )
                                            .child(
                                                div()
                                                    .absolute()
                                                    .top(px(-4.0))
                                                    .left(relative(slider))
                                                    .ml(px(-7.0))
                                                    .size(px(14.0))
                                                    .rounded_full()
                                                    .border_1()
                                                    .border_color(colors.border)
                                                    .bg(colors.foreground),
                                            ),
                                    ),
                            )
                            .child(
                                text_button(
                                    "timeline-zoom-in",
                                    "search-add",
                                    colors,
                                    zoom_in_progress,
                                )
                                .on_hover(hover_listener("timeline-zoom-in", cx))
                                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                    this.nudge_zoom(TIMELINE_ZOOM_BUTTON_FACTOR);
                                    cx.notify();
                                })),
                            ),
                    ),
            )
            .child(
                div()
                    .id("timeline-body")
                    .flex()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .overflow_hidden()
                    .on_scroll_wheel(cx.listener(Self::on_timeline_wheel))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_shrink_0()
                            .w(px(TIMELINE_TRACK_LABELS_COLUMN_WIDTH_PX))
                            .h_full()
                            .border_r_1()
                            .border_color(colors.border)
                            .overflow_hidden()
                            .child(
                                div()
                                    .relative()
                                    .flex()
                                    .w_full()
                                    .h(px(TIMELINE_HEADER_HEIGHT_PX))
                                    .flex_shrink_0()
                                    .items_center()
                                    .justify_center()
                                    .child(
                                        Button::new("timeline-add-track", colors)
                                            .variant(ButtonVariant::Text)
                                            .size(ButtonSize::Icon)
                                            .hover(add_track_progress)
                                            .icon("plus-sign")
                                            .build()
                                            .size(px(24.0))
                                            .on_hover(hover_listener("timeline-add-track", cx))
                                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                                this.add_track_menu.toggle();
                                                cx.notify();
                                            })),
                                    )
                                    .children(add_track_menu),
                            )
                            .child(
                                div()
                                    .relative()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .w_full()
                                    .min_h_0()
                                    .overflow_hidden()
                                    .child(
                                        div()
                                            .relative()
                                            .top(px(label_offset))
                                            .flex()
                                            .flex_col()
                                            .flex_shrink_0()
                                            .gap(px(TIMELINE_TRACK_GAP_PX))
                                            .children(labels),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_1()
                            .h_full()
                            .min_w_0()
                            .child(
                                div()
                                    .id("timeline-scroll")
                                    .flex()
                                    .flex_col()
                                    .size_full()
                                    .overflow_scroll()
                                    .track_scroll(&self.scroll)
                                    .child(
                                        div()
                                            .id("timeline-content")
                                            .relative()
                                            .flex()
                                            .flex_col()
                                            .w(px(content_width))
                                            .flex_shrink_0()
                                            .child(content_probe(cx))
                                            .on_drag_move::<ClipDrag>(cx.listener(Self::on_clip_drag))
                                            .on_drag_move::<TrimDrag>(cx.listener(Self::on_trim_drag))
                                            .on_drag_move::<MediaDrag>(cx.listener(Self::on_media_drag))
                                            .on_drag_move::<InsertDrag>(cx.listener(Self::on_insert_drag))
                                            .on_drag_move::<ClipTargetDrag>(cx.listener(Self::on_clip_target_drag))
                                            .on_drop(cx.listener(|this: &mut Self, drag: &ClipDrag, _, cx| {
                                                this.on_clip_drop(drag, cx)
                                            }))
                                            .on_drop(cx.listener(|this: &mut Self, drag: &TrimDrag, _, cx| {
                                                this.on_trim_drop(drag, cx)
                                            }))
                                            .on_drop(cx.listener(|this: &mut Self, drag: &MediaDrag, _, cx| {
                                                this.on_media_drop(drag, cx)
                                            }))
                                            .on_drop(cx.listener(|this: &mut Self, drag: &InsertDrag, _, cx| {
                                                this.on_insert_drop(drag, cx)
                                            }))
                                            .on_drop(cx.listener(|this: &mut Self, drag: &ClipTargetDrag, _, cx| {
                                                this.on_clip_target_drop(drag, cx)
                                            }))
                                            .child(
                                                div()
                                                    .id("timeline-ruler")
                                                    .relative()
                                                    .w_full()
                                                    .flex_shrink_0()
                                                    .cursor(gpui::CursorStyle::ResizeLeftRight)
                                                    .on_mouse_down(
                                                        gpui::MouseButton::Left,
                                                        cx.listener(move |this: &mut Self, event: &gpui::MouseDownEvent, _, cx| {
                                                            let offset = f32::from(event.position.x) - this.content_left;
                                                            this.scrub(offset, cx);
                                                        }),
                                                    )
                                                    .on_drag(ScrubDrag, |_, _, _, cx| cx.new(|_| gpui::Empty))
                                                    .on_drag_move::<ScrubDrag>(cx.listener(Self::on_scrub_drag))
                                                    .child(ruler(colors, self.zoom_level, fps, content_width)),
                                            )
                                            .child(bookmarks)
                                            .child(
                                                div()
                                                    .w_full()
                                                    .h(px(
                                                        crate::theme::TIMELINE_CONTENT_TOP_PADDING_PX,
                                                    ))
                                                    .flex_shrink_0(),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .w_full()
                                                    .gap(px(TIMELINE_TRACK_GAP_PX))
                                                    .children(rows),
                                            )
                                            .child(playhead(colors, playhead_x)),
                                    ),
                            )
                            .children(horizontal_bar)
                            .children(vertical_bar),
                    ),
            )
            .children(graph_pane)
    }
}

fn content_probe(cx: &mut Context<TimelinePanel>) -> impl IntoElement {
    let handle = cx.entity();
    gpui::canvas(
        move |bounds, _window, cx| {
            let left = f32::from(bounds.origin.x);
            handle.update(cx, |panel: &mut TimelinePanel, cx| {
                if (panel.content_left - left).abs() > 0.5 {
                    panel.content_left = left;
                    cx.notify();
                }
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .w_full()
    .h(px(TIMELINE_RULER_HEIGHT_PX))
}

fn graph_property_label(path: &str) -> SharedString {
    let mapped = match path {
        "transform.positionX" => Some(("properties.position", " X")),
        "transform.positionY" => Some(("properties.position", " Y")),
        "transform.scaleX" => Some(("properties.scale", " X")),
        "transform.scaleY" => Some(("properties.scale", " Y")),
        "transform.rotate" => Some(("properties.rotation", "")),
        "opacity" => Some(("properties.opacity", "")),
        "volume" => Some(("properties.volume", "")),
        _ => None,
    };
    match mapped {
        Some((key, suffix)) => {
            let translated = t(key);
            if translated == key {
                SharedString::from(path.to_string())
            } else {
                SharedString::from(format!("{translated}{suffix}"))
            }
        }
        None => SharedString::from(path.to_string()),
    }
}

fn graph_plot_canvas(
    keys: Vec<graph_editor::PlotKey>,
    time_min: f64,
    time_max: f64,
    value_axis: Axis,
    curve_color: gpui::Hsla,
    grid_color: gpui::Hsla,
) -> impl IntoElement {
    gpui::canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let width = f32::from(bounds.size.width);
            let height = f32::from(bounds.size.height);

            let time_axis = Axis::new(time_min, time_max, width, 0.0, false);
            let value_axis = Axis::new(
                value_axis.min,
                value_axis.max,
                height,
                GRAPH_PLOT_PAD_PX,
                true,
            );
            let origin_x = bounds.origin.x;
            let origin_y = bounds.origin.y;
            let point = |tick: f64, value: f64| {
                gpui::point(
                    origin_x + px(time_axis.to_px(tick)),
                    origin_y + px(value_axis.to_px(value)),
                )
            };

            let mut grid = gpui::PathBuilder::stroke(px(1.0));
            for step in 0..=4 {
                let value = value_axis.min + (value_axis.max - value_axis.min) * step as f64 / 4.0;
                let y = origin_y + px(value_axis.to_px(value));
                grid.move_to(gpui::point(origin_x + px(GRAPH_PLOT_PAD_PX), y));
                grid.line_to(gpui::point(origin_x + px(width - GRAPH_PLOT_PAD_PX), y));
            }
            if let Ok(path) = grid.build() {
                window.paint_path(path, grid_color);
            }

            if keys.len() >= 2 {
                let mut curve = gpui::PathBuilder::stroke(px(2.0));
                curve.move_to(point(keys[0].tick as f64, keys[0].value));
                for window_keys in keys.windows(2) {
                    let left = &window_keys[0];
                    let right = &window_keys[1];
                    for sample in 1..=GRAPH_CURVE_SAMPLES {
                        let fraction = sample as f64 / GRAPH_CURVE_SAMPLES as f64;
                        let tick = left.tick as f64 + (right.tick - left.tick) as f64 * fraction;
                        let value = graph_editor::eval_segment(
                            left.tick,
                            left.value,
                            right.tick,
                            right.value,
                            &left.segment_to_next,
                            left.right_handle,
                            right.left_handle,
                            tick.round() as i64,
                        );
                        curve.line_to(point(tick, value));
                    }
                }
                if let Ok(path) = curve.build() {
                    window.paint_path(path, curve_color);
                }
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

fn playhead(colors: Palette, x: f32) -> Div {
    div()
        .absolute()
        .top(px(0.0))
        .left(px(x - TIMELINE_INDICATOR_LINE_WIDTH_PX / 2.0))
        .w(px(TIMELINE_INDICATOR_LINE_WIDTH_PX))
        .h_full()
        .child(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .w(px(TIMELINE_INDICATOR_LINE_WIDTH_PX))
                .h_full()
                .bg(colors.primary),
        )
        .child(
            div()
                .absolute()
                .top(px(TIMELINE_PLAYHEAD_HANDLE_TOP_PX))
                .left(px((TIMELINE_INDICATOR_LINE_WIDTH_PX
                    - TIMELINE_PLAYHEAD_HANDLE_PX)
                    / 2.0))
                .size(px(TIMELINE_PLAYHEAD_HANDLE_PX))
                .rounded_full()
                .border_2()
                .border_color(opacity(colors.primary, 0.5))
                .bg(colors.primary),
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Theme, TEXT_BASE};

    #[test]
    fn every_effect_and_param_label_is_translated() {
        for definition in effects_ui::EFFECT_DEFINITIONS {
            assert_ne!(t(definition.name_key), definition.name_key);
            for param in definition.params {
                assert_ne!(t(param.label_key), param.label_key, "{}", param.key);
                if let effects_ui::ParamKind::Select(options) = param.kind {
                    for (_, label) in options {
                        assert_ne!(t(label), *label);
                    }
                }
            }
        }
    }

    #[test]
    fn every_transition_and_settings_label_is_translated() {
        for (_, key) in effects_ui::TRANSITION_KEYS
            .iter()
            .chain(effects_ui::TRANSITION_EASING_KEYS)
            .chain(MISC_TABS)
            .chain(APP_SETTINGS_TABS)
            .chain(effects_ui::HSL_BAND_KEYS)
            .chain(effects_ui::HSL_CHANNEL_KEYS)
        {
            assert_ne!(t(key), *key, "{key}");
        }
        for key in [
            "transitions.hint",
            "transitions.noCut",
            "editor.adjustment.filters",
            "editor.adjustment.title",
            "editor.adjustment.selectClip",
            "editor.effects.selectClip",
            "properties.effects.emptyHint",
            "settings.name",
            "settings.frameRate",
            "settings.aspectRatio",
            "settings.background.colors",
            "settings.language",
            "settings.theme",
            "theme.dark",
            "theme.light",
        ] {
            assert_ne!(t(key), key, "{key}");
        }
        for id in effects_ui::FILTER_PRESET_IDS {
            let key = format!("effects.filter.preset.{id}");
            assert_ne!(t(&key), key, "{key}");
        }
    }

    #[test]
    fn asset_tabs_match_the_web_tab_keys() {
        let keys: Vec<&str> = ASSET_TABS.iter().map(|(key, _)| *key).collect();
        assert_eq!(
            keys,
            vec![
                "media",
                "sounds",
                "text",
                "stickers",
                "effects",
                "transitions",
                "captions",
                "speech",
                "adjustment",
                "templates",
                "settings",
            ]
        );
    }

    #[test]
    fn every_tab_label_is_translated() {
        for (key, _) in ASSET_TABS {
            let value = t(&tab_label_key(key));
            assert!(!value.starts_with("editor.tab."), "{key}");
        }
    }

    #[test]
    fn the_odds_and_ends_tab_is_not_called_settings_any_more() {
        assert_eq!(tab_label_key("settings"), "editor.tab.misc");
        assert_eq!(tab_label_key("media"), "editor.tab.media");
        assert_ne!(t("editor.tab.misc"), t("settings.open"));
    }

    #[test]
    fn the_two_groups_of_sections_do_not_overlap_and_cover_the_old_list() {
        let mut all: Vec<&str> = MISC_TABS
            .iter()
            .chain(APP_SETTINGS_TABS)
            .map(|(key, _)| *key)
            .collect();
        let before = all.len();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), before, "a section cannot be in both places");
        assert_eq!(
            all,
            vec![
                "app",
                "attributions",
                "background",
                "project-info",
                "watermark",
                "youtube"
            ]
        );
    }

    #[test]
    fn text_scale_tokens_stay_in_order() {
        assert!(TEXT_XS < TEXT_SM);
        assert!(TEXT_SM < TEXT_BASE);
        assert!(TEXT_BASE < TEXT_LG);
    }

    #[test]
    fn the_playhead_paints_with_the_primary_token() {
        let primary = Theme::dark().panel.primary;
        assert_eq!(primary, crate::theme::parse_hsl("hsl(200, 90%, 52%)"));
        assert!((primary.h - crate::theme::parse_hex("16a8f3").h).abs() < 0.01);
    }

    #[test]
    fn the_preview_zoom_presets_match_the_web() {
        assert_eq!(PREVIEW_ZOOM_PRESETS, &[25, 50, 75, 100, 150, 200]);
    }

    #[test]
    fn the_tab_scroll_affordance_is_a_full_height_28px_column() {
        assert_eq!(TAB_SCROLL_ARROW_WIDTH_PX, 28.0);
    }

    #[test]
    fn the_zoom_slider_is_exponential_and_round_trips() {
        assert!((zoom_from_slider(0.0) - TIMELINE_ZOOM_MIN).abs() < 1e-4);
        assert!((zoom_from_slider(1.0) - TIMELINE_ZOOM_MAX).abs() < 1e-2);
        for zoom in [0.1, 0.5, 1.0, 5.75, 20.0, 100.0] {
            let back = zoom_from_slider(slider_from_zoom(zoom));
            assert!((back - zoom).abs() < 1e-2, "{zoom} -> {back}");
        }
    }

    #[test]
    fn the_slider_position_is_monotonic_in_zoom() {
        assert!(slider_from_zoom(1.0) < slider_from_zoom(10.0));
        assert_eq!(slider_from_zoom(0.001), 0.0);
        assert_eq!(slider_from_zoom(1000.0), 1.0);
    }

    #[test]
    fn zoom_buttons_step_by_the_web_factor() {
        assert_eq!(TIMELINE_ZOOM_BUTTON_FACTOR, 1.7);
    }

    #[test]
    fn every_guide_menu_string_is_translated() {
        for key in [
            "preview.guides",
            "common.none",
            "guides.rows",
            "guides.columns",
            "guides.clearLines",
            "guides.addLine.vertical",
            "guides.addLine.horizontal",
        ] {
            let value = t(key);
            assert_ne!(&value, key, "{key} has no translation");
            assert!(!value.is_empty(), "{key}");
        }
        for entry in crate::gizmos::GUIDE_REGISTRY {
            let label = PreviewPanel::guide_label(entry);
            assert!(!label.is_empty(), "{} has no label", entry.id);
            if let Some(key) = entry.label_key {
                assert_ne!(label.as_str(), key, "{} label key is unresolved", entry.id);
            }
        }
    }

    #[test]
    fn the_preview_handles_match_the_web_sizes() {
        assert_eq!(crate::gizmos::HANDLE_SIZE_PX, 10.0);
        assert_eq!(crate::gizmos::HANDLE_HIT_AREA_PX, 18.0);
        assert_eq!(crate::gizmos::ICON_HANDLE_RADIUS_PX, 10.0);
        assert_eq!(crate::gizmos::EDGE_HANDLE_THIN_PX, 6.0);
        assert_eq!(crate::gizmos::EDGE_HANDLE_THICK_PX, 14.0);
        assert_eq!(crate::gizmos::LINE_HIT_AREA_PX, 48.0);
        assert_eq!(crate::gizmos::ROTATION_HANDLE_OFFSET_PX, 24.0);
        assert_eq!(crate::gizmos::SNAP_THRESHOLD_SCREEN_PX, 8.0);
        assert_eq!(handle_bar(crate::gizmos::CropHandle::TopLeft), (10.0, 10.0));
        assert_eq!(handle_bar(crate::gizmos::CropHandle::Left), (6.0, 14.0));
        assert_eq!(handle_bar(crate::gizmos::CropHandle::Top), (14.0, 6.0));
        assert_eq!(handle_bar(crate::gizmos::CropHandle::Bottom), (14.0, 6.0));
    }

    #[test]
    fn the_timecode_frames_field_never_reaches_the_frame_rate() {
        let rates = [
            (time::FrameRate::FPS_24, 24u64),
            (time::FrameRate::FPS_30, 30),
            (time::FrameRate::FPS_60, 60),
            (
                time::FrameRate {
                    numerator: 120_000,
                    denominator: 1_000,
                },
                120,
            ),
        ];

        for (rate, fps) in rates {
            for ticks in [0i64, 1, 3_999, 120_000, 1_048_000, 1_234_567, 359_999_999] {
                let text = crate::state::format_frames(MediaTime::from_ticks(ticks), rate);
                let fields: Vec<u64> = text
                    .split(':')
                    .map(|field| field.parse().expect("numeric field"))
                    .collect();
                assert_eq!(fields.len(), 4, "{text}");
                assert!(fields[1] < 60 && fields[2] < 60, "{text}");
                assert!(fields[3] < fps, "{text} at {fps} fps");
            }
        }
    }

    #[test]
    fn the_position_and_the_total_format_identically() {
        let rate = time::FrameRate {
            numerator: 120_000,
            denominator: 1_000,
        };
        let total = MediaTime::from_ticks(1_048_000);

        assert_eq!(
            crate::state::format_frames(total, rate),
            "00:00:08:88",
            "8.7333s at 120 fps is 8 seconds and 88 frames"
        );
        assert_eq!(
            crate::state::format_frames(total, rate),
            crate::state::format_frames(clamp_playhead(total, total), rate)
        );
        assert_eq!(
            crate::state::format_frames(MediaTime::from_ticks(1_048_000), time::FrameRate::FPS_30),
            "00:00:08:22"
        );
    }

    #[test]
    fn wheel_zoom_keeps_the_time_under_the_cursor() {
        let viewport_left = 160.0f32;

        for offset_x in [0.0f32, -240.0, -1875.5] {
            let content_left = viewport_left + offset_x;
            for cursor_x in [200.0f32, 400.0, 1204.5, 1900.0] {
                for zoom_before in [0.1f32, 1.0, 5.75, 40.0] {
                    for factor in [1.25f32, 1.0 / 1.25, 2.0, 0.5, 1.01] {
                        let zoom_after =
                            (zoom_before * factor).clamp(TIMELINE_ZOOM_MIN, TIMELINE_ZOOM_MAX);
                        let before = BASE_TIMELINE_PIXELS_PER_SECOND * zoom_before;
                        let after = BASE_TIMELINE_PIXELS_PER_SECOND * zoom_after;
                        let scale = zoom_after / zoom_before;

                        let time_before = (cursor_x - content_left).max(0.0) / before;
                        let next =
                            zoom_anchored_offset(viewport_left, content_left, cursor_x, scale);

                        if cursor_x - (cursor_x - content_left).max(0.0) * scale - viewport_left
                            > 0.0
                        {
                            assert_eq!(next, 0.0);
                            continue;
                        }

                        let time_after = (cursor_x - (viewport_left + next)) / after;
                        assert!(
                            (time_after - time_before).abs() < 1e-3,
                            "cursor {cursor_x} moved from {time_before}s to {time_after}s                              (offset {offset_x}, zoom {zoom_before} -> {zoom_after})"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_pointer_over_the_label_column_holds_the_leftmost_visible_time() {
        let viewport_left = 160.0f32;
        let content_left = -840.0f32;
        let pixels_per_second = 100.0f32;
        let leftmost = (viewport_left - content_left) / pixels_per_second;

        for cursor_x in [20.0f32, 95.0, 159.0] {
            for scale in [2.0f32, 0.5, 1.25] {
                let next = zoom_anchored_offset(viewport_left, content_left, cursor_x, scale);
                let leftmost_after =
                    (viewport_left - (viewport_left + next)) / (pixels_per_second * scale);
                assert!(
                    (leftmost_after - leftmost).abs() < 1e-3,
                    "leftmost time moved to {leftmost_after} from {leftmost}"
                );
            }
        }
    }

    #[test]
    fn wheel_zoom_never_scrolls_past_the_left_edge() {
        for (viewport_left, content_left, cursor_x, scale) in [
            (0.0f32, 0.0f32, 0.0f32, 0.5f32),
            (160.0, -1000.0, 400.0, 0.1),
            (160.0, 160.0, 900.0, 2.0),
            (160.0, -40.0, 165.0, 0.25),
        ] {
            assert!(zoom_anchored_offset(viewport_left, content_left, cursor_x, scale) <= 0.0);
        }
    }

    #[test]
    fn the_wheel_zooms_by_a_ratio_and_never_runs_away() {
        assert!(wheel_zoom_factor(0.0) == 1.0);
        assert!(wheel_zoom_factor(21.0) > 1.2 && wheel_zoom_factor(21.0) < 1.3);
        assert!(wheel_zoom_factor(-21.0) > 0.77 && wheel_zoom_factor(-21.0) < 0.84);
        assert!((wheel_zoom_factor(2.0) - 1.0).abs() < 0.05);
        assert_eq!(wheel_zoom_factor(100_000.0), TIMELINE_ZOOM_WHEEL_MAX_STEP);
        assert_eq!(
            wheel_zoom_factor(-100_000.0),
            1.0 / TIMELINE_ZOOM_WHEEL_MAX_STEP
        );
        assert!((wheel_zoom_factor(21.0) * wheel_zoom_factor(-21.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn the_wheel_shares_the_magnifier_limits() {
        let mut zoom = TIMELINE_ZOOM_MAX;
        for _ in 0..50 {
            zoom = (zoom * wheel_zoom_factor(120.0)).clamp(TIMELINE_ZOOM_MIN, TIMELINE_ZOOM_MAX);
        }
        assert_eq!(zoom, TIMELINE_ZOOM_MAX);

        let mut zoom = TIMELINE_ZOOM_MIN;
        for _ in 0..50 {
            zoom = (zoom * wheel_zoom_factor(-120.0)).clamp(TIMELINE_ZOOM_MIN, TIMELINE_ZOOM_MAX);
        }
        assert_eq!(zoom, TIMELINE_ZOOM_MIN);
    }

    #[test]
    fn the_playhead_never_reads_past_the_project_duration() {
        let total = MediaTime::from_seconds_f64(8.7333).expect("total");
        assert!(total.as_ticks() > 0);

        for offset_px in [0.0f32, 1.0, 500.0, 1130.0, RULER_SPAN_PX, 100_000.0] {
            let pixels_per_second = BASE_TIMELINE_PIXELS_PER_SECOND * DEFAULT_TIMELINE_ZOOM;
            let seconds = (offset_px / pixels_per_second).max(0.0) as f64;
            let raw = MediaTime::from_seconds_f64(seconds).unwrap_or(MediaTime::ZERO);
            let clamped = clamp_playhead(raw, total);
            assert!(
                clamped.as_ticks() <= total.as_ticks(),
                "{offset_px}px scrubbed to {} ticks past {} ticks",
                clamped.as_ticks(),
                total.as_ticks()
            );
            assert!(clamped.as_ticks() >= 0);
        }

        assert_eq!(clamp_playhead(total, total).as_ticks(), total.as_ticks());
        let inside = MediaTime::from_seconds_f64(4.0).expect("inside");
        assert_eq!(clamp_playhead(inside, total).as_ticks(), inside.as_ticks());
    }

    #[test]
    fn an_empty_project_leaves_the_playhead_where_it_is() {
        let somewhere = MediaTime::from_seconds_f64(3.0).expect("time");
        assert_eq!(
            clamp_playhead(somewhere, MediaTime::ZERO).as_ticks(),
            somewhere.as_ticks()
        );
        assert_eq!(
            clamp_playhead(MediaTime::from_ticks(-500), MediaTime::ZERO).as_ticks(),
            0
        );
    }

    #[test]
    fn every_toolbar_tooltip_key_is_translated() {
        for (id, _, key) in TIMELINE_TOOLBAR_LEFT {
            let value = t(key);
            assert_ne!(&value, key, "{id} has no translation for {key}");
            assert!(!value.is_empty(), "{id}");
        }
    }

    #[test]
    fn the_shared_tooltip_keys_resolve() {
        for key in [
            "timeline.autoSnapping",
            "timeline.rippleEditing",
            "assets.view.switchToList",
            "assets.view.switchToGrid",
            "assets.sort.tooltip",
            "shortcuts.action.togglePlay",
            "preview.fullScreen",
            "common.language",
            "editor.export",
            "theme.light",
        ] {
            assert_ne!(t(key), key, "{key}");
        }
    }

    #[test]
    fn every_enabled_toolbar_tool_has_a_run_tool_arm() {
        const HANDLED: &[&str] = &[
            "split",
            "align-start",
            "align-end",
            "link",
            "duplicate",
            "delete",
            "bookmark",
            "graph",
        ];
        for (id, _, _) in TIMELINE_TOOLBAR_LEFT {
            if TIMELINE_TOOLBAR_DISABLED.contains(id) {
                continue;
            }
            assert!(HANDLED.contains(id), "{id} would be a no-op button");
        }
    }

    #[test]
    fn the_bookmark_and_link_tools_are_no_longer_disabled() {
        assert!(!TIMELINE_TOOLBAR_DISABLED.contains(&"bookmark"));
        assert!(!TIMELINE_TOOLBAR_DISABLED.contains(&"link"));
    }

    #[test]
    fn the_graph_tool_is_enabled() {
        assert!(!TIMELINE_TOOLBAR_DISABLED.contains(&"graph"));
    }

    #[test]
    fn every_scene_action_label_is_translated() {
        for (id, key) in SCENE_ACTIONS {
            assert_ne!(&t(key), key, "{id}");
        }
    }

    #[test]
    fn every_track_kind_has_a_translated_menu_label_and_an_icon() {
        for kind in edit::TrackKind::ALL {
            let key = kind.label_key();
            assert_ne!(t(key), key, "{}", kind.id());
            assert!(crate::assets::icon_exists(kind.glyph()), "{}", kind.id());
        }
        assert_ne!(t("timeline.addTrack"), "timeline.addTrack");
        assert_ne!(t("scenes.new"), "scenes.new");
    }

    #[test]
    fn every_toolbar_and_timeline_glyph_is_a_registered_icon() {
        for (id, glyph, _) in TIMELINE_TOOLBAR_LEFT {
            assert!(crate::assets::icon_exists(glyph), "{id} -> {glyph}");
        }
        for glyph in [
            "plus-sign",
            "bookmark02",
            "layers01",
            "magnet",
            "oc-ripple",
            "eye",
            "eye-off",
            "volume-high",
            "volume-mute",
        ] {
            assert!(crate::assets::icon_exists(glyph), "{glyph}");
        }
    }

    #[test]
    fn the_context_menu_labels_resolve() {
        for key in [
            "timeline.deleteTrack",
            "timeline.muteElement",
            "timeline.unmuteElement",
            "timeline.hideElement",
            "timeline.showElement",
            "timeline.audio.extract",
            "properties.audio.recover",
            "timeline.bookmark.delete",
            "timeline.bookmark.note.placeholder",
            "common.note",
            "dialog.rename.label",
        ] {
            assert_ne!(t(key), key, "{key}");
        }
    }

    #[test]
    fn every_disabled_tool_is_a_real_toolbar_entry() {
        for id in TIMELINE_TOOLBAR_DISABLED {
            assert!(
                TIMELINE_TOOLBAR_LEFT.iter().any(|(name, _, _)| name == id),
                "{id}"
            );
        }
    }

    #[test]
    fn the_toolbar_separator_splits_after_delete() {
        assert_eq!(
            TIMELINE_TOOLBAR_LEFT[TIMELINE_TOOLBAR_SEPARATOR_AFTER - 1].0,
            "delete"
        );
        assert_eq!(
            TIMELINE_TOOLBAR_LEFT[TIMELINE_TOOLBAR_SEPARATOR_AFTER].0,
            "bookmark"
        );
    }

    #[test]
    fn every_non_media_tab_has_empty_state_art() {
        for (key, _) in ASSET_TABS.iter().filter(|(key, _)| *key != "media") {
            assert!(
                ASSET_VIEW_ICONS.iter().any(|(name, _)| name == key),
                "{key}"
            );
        }
    }

    #[test]
    fn timeline_toolbar_ids_are_unique() {
        let mut ids: Vec<&str> = TIMELINE_TOOLBAR_LEFT.iter().map(|(id, _, _)| *id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count);
    }

    #[test]
    fn text_variant_hover_matches_the_web_opacity() {
        assert_eq!(crate::components::TEXT_HOVER_OPACITY, 0.75);
    }
}
