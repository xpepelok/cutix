use std::collections::HashSet;
use std::path::PathBuf;

use cutix_i18n::{t, t_args};
use cutix_playback::animation::{color_at, scalar_at};
use cutix_project::model::Crop;
use cutix_project::TimelineElement;
use gpui::{
    div, prelude::*, px, relative, svg, App, Context, Div, Entity, FontWeight, ScrollHandle,
    SharedString, Stateful, Window,
};
use time::MediaTime;

use crate::assets::icon;
use crate::components::{
    menu_item, menu_natural_height, menu_surface, overlay_backdrop, overlay_layer, place_anchored,
    Button, ButtonSize, ButtonVariant, MENU_OFFSET_PX,
};
use crate::edit::{self, Field, Setting};
use crate::input::{text_field, FieldStyle, TextEvent, TextField};
use crate::interaction::{mix, Overlay, OverlaySide, Transitions};
use crate::scroll::scrollbar_v;
use crate::state::AppModel;
use crate::theme::{opacity, rem, Palette, RADIUS_MD, RADIUS_SM, TEXT_LG, TEXT_SM, TEXT_XS};

const SECTION_HEADER_HEIGHT_PX: f32 = 44.0;
const SECTION_PAD_PX: f32 = 16.0;
const FIELD_GAP_PX: f32 = 14.0;
const CONTROL_HEIGHT_PX: f32 = 28.0;
const TAB_RAIL_BUTTON_PX: f32 = 32.0;
const LABEL_ROW_HEIGHT_PX: f32 = 16.0;
const KEYFRAME_GLYPH_PX: f32 = 14.0;
const CONTROL_ICON_PX: f32 = 14.0;
const SELECT_MENU_WIDTH_PX: f32 = 168.0;
const SLIDER_TRACK_PX: f32 = 6.0;

const MASK_FEATHER_MAX_PX: f64 = 200.0;
const MASK_STROKE_MAX_PX: f64 = 40.0;
const MASK_STROKE_COLORS: &[&str] = &[
    "#ffffff", "#000000", "#ff3b30", "#ffcc00", "#34c759", "#00c7ff", "#ff2d95",
];
const SLIDER_THUMB_PX: f32 = 14.0;

pub const BLEND_MODES: &[(&str, &str)] = &[
    ("normal", "watermark.blend.normal"),
    ("darken", "watermark.blend.darken"),
    ("multiply", "watermark.blend.multiply"),
    ("color-burn", "properties.blendMode.colorBurn"),
    ("lighten", "watermark.blend.lighten"),
    ("screen", "watermark.blend.screen"),
    ("plus-lighter", "properties.blendMode.plusLighter"),
    ("color-dodge", "properties.blendMode.colorDodge"),
    ("overlay", "watermark.blend.overlay"),
    ("soft-light", "properties.blendMode.softLight"),
    ("hard-light", "properties.blendMode.hardLight"),
    ("difference", "watermark.blend.difference"),
    ("exclusion", "properties.blendMode.exclusion"),
    ("hue", "properties.blendMode.hue"),
    ("saturation", "properties.blendMode.saturation"),
    ("color", "properties.blendMode.color"),
    ("luminosity", "watermark.blend.luminosity"),
];

pub const FONT_FAMILIES: &[&str] = &[
    "Arial",
    "Segoe UI",
    "Times New Roman",
    "Georgia",
    "Courier New",
    "Verdana",
    "Tahoma",
    "Trebuchet MS",
    "Impact",
    "Comic Sans MS",
];

pub const CROP_ASPECTS: &[(&str, f64)] = &[
    ("1:1", 1.0),
    ("4:5", 4.0 / 5.0),
    ("9:16", 9.0 / 16.0),
    ("16:9", 16.0 / 9.0),
];

pub fn tabs_for(element: &TimelineElement) -> Vec<(&'static str, &'static str, &'static str)> {
    match element {
        TimelineElement::Text(_) => vec![
            ("text", "text-font", "editor.panel.text"),
            ("animation", "magic-wand05", "text.animations"),
            ("transform", "arrow-expand", "properties.transform"),
            ("blending", "rain-drop", "properties.blending"),
            ("effects", "magic-wand05", "properties.effects"),
        ],
        TimelineElement::Video(_) => vec![
            ("transform", "arrow-expand", "properties.transform"),
            ("crop", "crop", "properties.crop"),
            ("mask", "layers01", "properties.masks"),
            ("cutout", "checkerboard", "cutout.title"),
            ("tracking", "search01", "tracking.title"),
            ("stabilize", "magnet", "stabilize.title"),
            ("reframe", "full-screen", "reframe.title"),
            ("audio", "music-note03", "properties.audio"),
            ("speed", "dashboard-speed", "properties.speed"),
            ("blending", "rain-drop", "properties.blending"),
            ("transition", "arrow-right-double", "transitions.title"),
            ("effects", "magic-wand05", "properties.effects"),
        ],
        TimelineElement::Image(_) => vec![
            ("transform", "arrow-expand", "properties.transform"),
            ("crop", "crop", "properties.crop"),
            ("mask", "layers01", "properties.masks"),
            ("cutout", "checkerboard", "cutout.title"),
            ("blending", "rain-drop", "properties.blending"),
            ("transition", "arrow-right-double", "transitions.title"),
            ("effects", "magic-wand05", "properties.effects"),
        ],
        TimelineElement::Graphic(_) => vec![
            ("graphic", "happy01", "properties.shape"),
            ("transform", "arrow-expand", "properties.transform"),
            ("crop", "crop", "properties.crop"),
            ("mask", "layers01", "properties.masks"),
            ("blending", "rain-drop", "properties.blending"),
            ("effects", "magic-wand05", "properties.effects"),
        ],
        TimelineElement::Sticker(_) => vec![
            ("transform", "arrow-expand", "properties.transform"),
            ("crop", "crop", "properties.crop"),
            ("blending", "rain-drop", "properties.blending"),
            ("effects", "magic-wand05", "properties.effects"),
        ],
        TimelineElement::Audio(_) => vec![
            ("audio", "music-note03", "properties.audio"),
            ("speed", "dashboard-speed", "properties.speed"),
        ],
        TimelineElement::Effect(_) => vec![("effects", "magic-wand05", "properties.effects")],
    }
}

pub fn element_type_key(element: &TimelineElement) -> &'static str {
    match element {
        TimelineElement::Video(_) => "video",
        TimelineElement::Image(_) => "image",
        TimelineElement::Audio(_) => "audio",
        TimelineElement::Text(_) => "text",
        TimelineElement::Sticker(_) => "sticker",
        TimelineElement::Graphic(_) => "graphic",
        TimelineElement::Effect(_) => "effect",
    }
}

#[derive(Clone, Debug)]
pub struct ScrubDrag {
    key: String,
}

#[derive(Clone, Debug)]
struct SliderDrag {
    key: String,
}

#[derive(Clone, Debug)]
struct EffectDrag {
    element: String,
    index: usize,
}

#[derive(Clone, Debug)]
struct Scrub {
    key: String,
    element: String,
    field: Field,
    start_x: f32,
    start_display: f64,
    scale: f64,
    sensitivity: f64,
}

#[derive(Clone, Debug, PartialEq)]
enum Target {
    Number { field: Field, scale: f64 },
    Text(TextSetting),
    EffectColor { effect_id: String, key: String },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum TextSetting {
    Content,
    TextColor,
    BackgroundColor,
    StrokeColor,
    ShadowColor,
    GraphicColor(&'static str),
}

struct Editing {
    key: String,
    element: String,
    target: Target,
    field: TextField,
}

#[derive(Clone, Debug, PartialEq)]
enum MenuKind {
    BlendMode,
    FontFamily,
    GraphicSelect(&'static str),
}

struct Resolved {
    base: f64,
    value: f64,
    keyed: bool,
}

pub struct PropertiesPanel {
    app: Entity<AppModel>,
    scroll: ScrollHandle,
    transitions: Transitions,
    collapsed: HashSet<String>,
    editing: Option<Editing>,
    scrub: Option<Scrub>,
    slider: Option<String>,
    menu: Overlay,
    menu_kind: Option<MenuKind>,
    scale_locked: bool,
    cutout_job: Option<crate::ai::Job>,
    cutout_notice: Option<String>,
    cutout_rate: u32,
    tracking_job: Option<crate::ai::Job>,
    tracking_notice: Option<String>,
    tracking_threshold: f32,
    tracking_target: Option<String>,
    audio_job: Option<crate::ai::Job>,
    audio_notice: Option<String>,
    eq_gains: Vec<f32>,
    pitch_semitones: f32,
    pitch_formant: f32,
    reverb_preset: usize,
    reverb_wet: f32,
    denoise_strength: f32,
    beat_sensitivity: f32,
    beats: Option<crate::audio_fx::Beats>,
    duck_voice: Option<String>,
    silence_threshold_db: f32,
    silence_min_seconds: f32,
    silence_ranges: Option<(String, Vec<(i64, i64)>)>,
    stabilize_job: Option<crate::ai::Job>,
    stabilize_notice: Option<String>,
    stabilize_strength: f32,
    reframe_job: Option<crate::ai::Job>,
    reframe_notice: Option<String>,
    reframe_aspect: &'static str,
    reframe_zoom: f32,
    motion_intensity: f64,
    lut_notice: Option<String>,
    rail_scroll: ScrollHandle,
}

impl PropertiesPanel {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self {
            app,
            scroll: ScrollHandle::new(),
            transitions: Transitions::new(),
            collapsed: HashSet::new(),
            editing: None,
            scrub: None,
            slider: None,
            menu: Overlay::new(OverlaySide::Bottom),
            menu_kind: None,
            scale_locked: true,
            cutout_job: None,
            cutout_notice: None,
            cutout_rate: ml::DEFAULT_CUTOUT_SAMPLE_RATE,
            tracking_job: None,
            tracking_notice: None,
            tracking_threshold: crate::tracking::DEFAULT_TRACKING_CONFIDENCE,
            tracking_target: None,
            audio_job: None,
            audio_notice: None,
            eq_gains: vec![0.0; dsp::EQUALIZER_BAND_FREQUENCIES.len()],
            pitch_semitones: 0.0,
            pitch_formant: 0.0,
            reverb_preset: 0,
            reverb_wet: 0.3,
            denoise_strength: 0.7,
            beat_sensitivity: 1.3,
            beats: None,
            duck_voice: None,
            silence_threshold_db: audio::DEFAULT_SILENCE_THRESHOLD_DB,
            silence_min_seconds: audio::DEFAULT_MIN_SILENCE_SECONDS,
            silence_ranges: None,
            stabilize_job: None,
            stabilize_notice: None,
            stabilize_strength: crate::stabilize::DEFAULT_STRENGTH,
            reframe_job: None,
            reframe_notice: None,
            reframe_aspect: crate::reframe::REFRAME_ASPECTS[0].0,
            reframe_zoom: crate::reframe::DEFAULT_ZOOM,
            motion_intensity: crate::motion::MOTION_DEFAULT_INTENSITY,
            lut_notice: None,
            rail_scroll: ScrollHandle::new(),
        }
    }

    fn colors(&self, cx: &App) -> Palette {
        self.app.read(cx).theme.panel
    }

    fn selected(&self, cx: &App) -> Option<TimelineElement> {
        self.app.read(cx).selected_element().cloned()
    }

    fn local_time(&self, element: &TimelineElement, cx: &App) -> MediaTime {
        edit::local_time(element, self.app.read(cx).playhead)
    }

    fn within_range(&self, element: &TimelineElement, cx: &App) -> bool {
        edit::playhead_within(element, self.app.read(cx).playhead)
    }

    fn resolve(&self, element: &TimelineElement, field: Field, cx: &App) -> Resolved {
        let base = edit::field_value(element, field);
        let Some(path) = field.path() else {
            return Resolved {
                base,
                value: base,
                keyed: false,
            };
        };
        let local = self.local_time(element, cx);
        let animated = edit::is_animated(element, path);
        let within = self.within_range(element, cx);
        let value = if animated && within {
            scalar_at(element.base().animations.as_ref(), path, base, local)
        } else {
            base
        };
        Resolved {
            base,
            value,
            keyed: within && edit::has_key_at(element, path, local),
        }
    }

    fn commit_number(
        &mut self,
        element_id: &str,
        field: Field,
        value: f64,
        cx: &mut Context<Self>,
    ) {
        self.commit_number_keyed(element_id, field, value, None, cx);
    }

    fn commit_number_keyed(
        &mut self,
        element_id: &str,
        field: Field,
        value: f64,
        coalesce: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(element) = self.selected(cx) else {
            return;
        };
        let local = self.local_time(&element, cx);
        let animated = field
            .path()
            .is_some_and(|path| edit::is_animated(&element, path));
        let within = self.within_range(&element, cx);
        let id = element_id.to_string();
        let locked = self.scale_locked;
        self.app.update(cx, |model, cx| {
            model.edit_coalesced(coalesce, cx, |editor| {
                let mut changed = if animated && within {
                    editor.set_keyframe(&id, field, local, value)
                } else {
                    editor.set_property(&id, field, value)
                };
                if locked && matches!(field, Field::ScaleX | Field::ScaleY) {
                    let partner = if field == Field::ScaleX {
                        Field::ScaleY
                    } else {
                        Field::ScaleX
                    };
                    changed |= if animated && within {
                        editor.set_keyframe(&id, partner, local, value)
                    } else {
                        editor.set_property(&id, partner, value)
                    };
                }
                changed
            })
        });
        cx.notify();
    }

    fn apply_setting(&mut self, element_id: &str, setting: Setting, cx: &mut Context<Self>) {
        let id = element_id.to_string();
        self.app.update(cx, |model, cx| {
            model.edit(cx, |editor| editor.apply_setting(&id, setting))
        });
        cx.notify();
    }

    fn toggle_keyframe(&mut self, element_id: &str, field: Field, cx: &mut Context<Self>) {
        let Some(element) = self.selected(cx) else {
            return;
        };
        if !self.within_range(&element, cx) {
            return;
        }
        let local = self.local_time(&element, cx);
        let value = self.resolve(&element, field, cx).value;
        let id = element_id.to_string();
        self.app.update(cx, |model, cx| {
            model.edit(cx, |editor| {
                editor.toggle_keyframe(&id, field, local, value)
            })
        });
        cx.notify();
    }

    fn on_scrub(
        &mut self,
        event: &gpui::DragMoveEvent<ScrubDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.drag(cx).key.clone();
        let Some(scrub) = self.scrub.clone().filter(|scrub| scrub.key == key) else {
            return;
        };
        let delta = f32::from(event.event.position.x) - scrub.start_x;
        let display = scrub.start_display + delta as f64 * scrub.sensitivity;
        let value = display / scrub.scale;
        self.commit_number_keyed(&scrub.element.clone(), scrub.field, value, Some(key), cx);
    }

    fn end_gesture(&mut self, cx: &mut Context<Self>) {
        if self.scrub.take().is_some() || self.slider.take().is_some() {
            cx.notify();
        }
    }

    fn section(
        &mut self,
        key: String,
        title: String,
        body: Vec<Div>,
        cx: &mut Context<Self>,
    ) -> Div {
        let colors = self.colors(cx);
        let open = !self.collapsed.contains(&key);
        let toggle = key.clone();

        div()
            .flex()
            .flex_col()
            .w_full()
            .flex_shrink_0()
            .border_b_1()
            .border_color(colors.border)
            .child(
                div()
                    .id(SharedString::from(format!("section-{key}")))
                    .flex()
                    .w_full()
                    .h(px(SECTION_HEADER_HEIGHT_PX))
                    .flex_shrink_0()
                    .items_center()
                    .justify_between()
                    .px(px(14.0))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        if !this.collapsed.remove(&toggle) {
                            this.collapsed.insert(toggle.clone());
                        }
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(rem(TEXT_SM))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if open {
                                colors.foreground
                            } else {
                                colors.muted_foreground
                            })
                            .child(title),
                    )
                    .child(
                        svg()
                            .size(px(16.0))
                            .path(icon("arrow-down"))
                            .text_color(if open {
                                colors.foreground
                            } else {
                                colors.muted_foreground
                            }),
                    ),
            )
            .when(open, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .w_full()
                        .gap(px(FIELD_GAP_PX))
                        .px(px(SECTION_PAD_PX))
                        .pb(px(SECTION_PAD_PX))
                        .children(body),
                )
            })
    }

    fn field_label(
        &mut self,
        label: String,
        keyframe: Option<(String, Field, bool, bool)>,
        cx: &mut Context<Self>,
    ) -> Div {
        let colors = self.colors(cx);
        div()
            .flex()
            .w_full()
            .h(px(LABEL_ROW_HEIGHT_PX))
            .items_center()
            .gap(px(6.0))
            .when_some(keyframe, |this, (element_id, field, active, enabled)| {
                this.child(
                    div()
                        .id(SharedString::from(format!(
                            "keyframe-{element_id}-{field:?}"
                        )))
                        .flex()
                        .size(px(KEYFRAME_GLYPH_PX + 4.0))
                        .items_center()
                        .justify_center()
                        .when(enabled, |this| this.cursor_pointer())
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            if enabled {
                                this.toggle_keyframe(&element_id.clone(), field, cx);
                            }
                        }))
                        .child(
                            svg()
                                .size(px(KEYFRAME_GLYPH_PX))
                                .path(icon(if active {
                                    "keyframe-filled"
                                } else {
                                    "keyframe"
                                }))
                                .text_color(if !enabled {
                                    opacity(colors.muted_foreground, 0.35)
                                } else if active {
                                    colors.primary
                                } else {
                                    colors.muted_foreground
                                }),
                        ),
                )
            })
            .child(
                div()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(label),
            )
    }

    #[allow(clippy::too_many_arguments)]
    fn number_field(
        &mut self,
        element_id: &str,
        field: Field,
        icon_glyph: NumberIcon,
        display: String,
        scale: f64,
        is_default: bool,
        sensitivity: f64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let colors = self.colors(cx);
        let key = format!("{element_id}-{field:?}");
        let editing = self
            .editing
            .as_ref()
            .filter(|editing| editing.key == key)
            .is_some();
        let element = element_id.to_string();
        let start_display = display
            .trim_end_matches(|character: char| !character.is_ascii_digit() && character != '.')
            .parse::<f64>()
            .unwrap_or(0.0);
        let drag_key = key.clone();
        let press_key = key.clone();
        let press_element = element.clone();
        let edit_key = key.clone();
        let edit_element = element.clone();
        let reset_element = element.clone();
        let value_text = display.clone();

        div()
            .flex()
            .w_full()
            .h(px(CONTROL_HEIGHT_PX))
            .items_center()
            .rounded(rem(RADIUS_MD))
            .border_1()
            .border_color(if editing {
                colors.primary
            } else {
                colors.border
            })
            .bg(colors.accent)
            .overflow_hidden()
            .child(
                div()
                    .id(SharedString::from(format!("scrub-{key}")))
                    .flex()
                    .flex_shrink_0()
                    .h_full()
                    .items_center()
                    .pl(px(10.0))
                    .pr(px(4.0))
                    .cursor(gpui::CursorStyle::ResizeLeftRight)
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(
                            move |this: &mut Self, event: &gpui::MouseDownEvent, _, cx| {
                                this.scrub = Some(Scrub {
                                    key: press_key.clone(),
                                    element: press_element.clone(),
                                    field,
                                    start_x: f32::from(event.position.x),
                                    start_display,
                                    scale,
                                    sensitivity,
                                });
                                cx.notify();
                            },
                        ),
                    )
                    .on_drag(ScrubDrag { key: drag_key }, |_, _, _, cx| {
                        cx.new(|_| gpui::Empty)
                    })
                    .child(match icon_glyph {
                        NumberIcon::Text(label) => div().child(label).into_any_element(),
                        NumberIcon::Glyph(name) => svg()
                            .size(px(CONTROL_ICON_PX))
                            .flex_shrink_0()
                            .path(icon(name))
                            .text_color(colors.muted_foreground)
                            .into_any_element(),
                    }),
            )
            .child(if editing {
                let field_ref = self.editing.as_ref().expect("editing field");
                div()
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .px(px(6.0))
                    .child(inline_editor(field_ref, colors, window, cx))
                    .into_any_element()
            } else {
                div()
                    .id(SharedString::from(format!("value-{key}")))
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .px(px(6.0))
                    .cursor_text()
                    .text_size(rem(TEXT_SM))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.editing = Some(Editing {
                            key: edit_key.clone(),
                            element: edit_element.clone(),
                            target: Target::Number { field, scale },
                            field: TextField::new(cx, value_text.clone()),
                        });
                        cx.notify();
                    }))
                    .child(display)
                    .into_any_element()
            })
            .when(!is_default, |this| {
                this.child(
                    div()
                        .id(SharedString::from(format!("reset-{key}")))
                        .flex()
                        .flex_shrink_0()
                        .h_full()
                        .items_center()
                        .pr(px(8.0))
                        .cursor_pointer()
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            let default = field.default_value();
                            this.commit_number(&reset_element.clone(), field, default, cx);
                        }))
                        .child(
                            svg()
                                .size(px(CONTROL_ICON_PX))
                                .path(icon("arrow-turn-backward"))
                                .text_color(colors.muted_foreground),
                        ),
                )
            })
    }

    #[allow(clippy::too_many_arguments)]
    fn number_row(
        &mut self,
        element: &TimelineElement,
        field: Field,
        label: String,
        icon_glyph: NumberIcon,
        scale: f64,
        decimals: usize,
        keyframable: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let id = element.base().id.clone();
        let resolved = self.resolve(element, field, cx);
        let within = self.within_range(element, cx);
        let display = format!("{:.*}", decimals, resolved.value * scale);
        let is_default = (resolved.base - field.default_value()).abs() < 1e-9;
        let keyframe = (keyframable && field.path().is_some())
            .then(|| (id.clone(), field, resolved.keyed, within));
        let sensitivity = if matches!(
            field,
            Field::ScaleX | Field::ScaleY | Field::Opacity | Field::Rotate | Field::Volume
        ) {
            0.5
        } else {
            1.0
        };

        let label_row = self.field_label(label, keyframe, cx);
        let control = self.number_field(
            &id,
            field,
            icon_glyph,
            display,
            scale,
            is_default,
            sensitivity,
            window,
            cx,
        );
        div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap(px(8.0))
            .child(label_row)
            .child(control)
    }

    fn switch_row(
        &mut self,
        id: &str,
        label: String,
        checked: bool,
        setting: Setting,
        element_id: &str,
        cx: &mut Context<Self>,
    ) -> Div {
        let colors = self.colors(cx);
        let element = element_id.to_string();
        div()
            .flex()
            .w_full()
            .items_center()
            .justify_between()
            .gap(px(10.0))
            .child(div().text_size(rem(TEXT_SM)).child(label))
            .child(
                div()
                    .id(SharedString::from(format!("switch-{id}")))
                    .flex()
                    .w(px(34.0))
                    .h(px(18.0))
                    .flex_shrink_0()
                    .items_center()
                    .rounded(px(9.0))
                    .px(px(2.0))
                    .cursor_pointer()
                    .bg(if checked {
                        colors.primary
                    } else {
                        colors.muted
                    })
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.apply_setting(&element.clone(), setting.clone(), cx);
                    }))
                    .child(
                        div()
                            .size(px(14.0))
                            .rounded_full()
                            .bg(colors.background)
                            .ml(if checked { px(16.0) } else { px(0.0) }),
                    ),
            )
    }

    fn button_group(
        &mut self,
        id: &str,
        options: Vec<(&'static str, ButtonFace, String)>,
        active: String,
        element_id: &str,
        build: fn(String) -> Setting,
        cx: &mut Context<Self>,
    ) -> Div {
        let colors = self.colors(cx);
        let element = element_id.to_string();
        let children = options
            .into_iter()
            .map(|(value, face, label)| {
                let selected = value == active;
                let element = element.clone();
                let key = format!("{id}-{value}");
                let progress = self.transitions.eased(&key);
                let hover_key = key.clone();
                div()
                    .id(SharedString::from(key))
                    .flex()
                    .h(px(CONTROL_HEIGHT_PX))
                    .min_w(px(CONTROL_HEIGHT_PX))
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .px(px(8.0))
                    .rounded(rem(RADIUS_SM))
                    .cursor_pointer()
                    .text_size(rem(TEXT_XS))
                    .bg(if selected {
                        colors.secondary
                    } else {
                        mix(opacity(colors.accent, 0.0), colors.accent, progress)
                    })
                    .text_color(if selected {
                        colors.secondary_foreground
                    } else {
                        colors.muted_foreground
                    })
                    .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                        this.transitions.set(hover_key.clone(), *hovered);
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.apply_setting(&element.clone(), build(value.to_string()), cx);
                    }))
                    .child(match face {
                        ButtonFace::Glyph(name) => svg()
                            .size(px(16.0))
                            .path(icon(name))
                            .text_color(if selected {
                                colors.secondary_foreground
                            } else {
                                colors.muted_foreground
                            })
                            .into_any_element(),
                        ButtonFace::Label => div().child(label).into_any_element(),
                    })
            })
            .collect::<Vec<_>>();

        div()
            .flex()
            .w_full()
            .items_center()
            .gap(px(4.0))
            .p(px(2.0))
            .rounded(rem(RADIUS_MD))
            .border_1()
            .border_color(colors.border)
            .children(children)
    }

    fn color_row(
        &mut self,
        element: &TimelineElement,
        label: String,
        setting: TextSetting,
        current: String,
        path: Option<&'static str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let colors = self.colors(cx);
        let id = element.base().id.clone();
        let key = format!("{id}-color-{setting:?}");
        let editing = self.editing.as_ref().is_some_and(|edit| edit.key == key);
        let swatch = crate::theme::parse_hex(current.trim_start_matches('#'));
        let within = self.within_range(element, cx);
        let local = self.local_time(element, cx);
        let keyed = path.is_some_and(|path| within && edit::has_color_key_at(element, path, local));

        let label_row = div()
            .flex()
            .w_full()
            .h(px(LABEL_ROW_HEIGHT_PX))
            .items_center()
            .gap(px(6.0))
            .when_some(path, |this, path| {
                let element_id = id.clone();
                let color = current.clone();
                this.child(
                    div()
                        .id(SharedString::from(format!("keyframe-color-{key}")))
                        .flex()
                        .size(px(KEYFRAME_GLYPH_PX + 4.0))
                        .items_center()
                        .justify_center()
                        .when(within, |this| this.cursor_pointer())
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            if !within {
                                return;
                            }
                            let rgba = cutix_project::color::parse_to_srgb_rgba(&color)
                                .unwrap_or([1.0, 1.0, 1.0, 1.0]);
                            let element_id = element_id.clone();
                            this.app.update(cx, |model, cx| {
                                model.edit(cx, |editor| {
                                    editor.toggle_color_keyframe(&element_id, path, local, rgba)
                                })
                            });
                            cx.notify();
                        }))
                        .child(
                            svg()
                                .size(px(KEYFRAME_GLYPH_PX))
                                .path(icon(if keyed { "keyframe-filled" } else { "keyframe" }))
                                .text_color(if !within {
                                    opacity(colors.muted_foreground, 0.35)
                                } else if keyed {
                                    colors.primary
                                } else {
                                    colors.muted_foreground
                                }),
                        ),
                )
            })
            .child(
                div()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(label),
            );

        let edit_key = key.clone();
        let edit_element = id.clone();
        let edit_value = current.clone();

        div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap(px(8.0))
            .child(label_row)
            .child(
                div()
                    .flex()
                    .w_full()
                    .h(px(CONTROL_HEIGHT_PX))
                    .items_center()
                    .gap(px(8.0))
                    .rounded(rem(RADIUS_MD))
                    .border_1()
                    .border_color(if editing {
                        colors.primary
                    } else {
                        colors.border
                    })
                    .bg(colors.accent)
                    .px(px(8.0))
                    .overflow_hidden()
                    .child(
                        div()
                            .size(px(16.0))
                            .flex_shrink_0()
                            .rounded(px(3.0))
                            .border_1()
                            .border_color(colors.border)
                            .bg(swatch),
                    )
                    .child(if editing {
                        let field_ref = self.editing.as_ref().expect("editing field");
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .child(inline_editor(field_ref, colors, window, cx))
                            .into_any_element()
                    } else {
                        div()
                            .id(SharedString::from(format!("color-value-{key}")))
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .items_center()
                            .cursor_text()
                            .text_size(rem(TEXT_SM))
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.editing = Some(Editing {
                                    key: edit_key.clone(),
                                    element: edit_element.clone(),
                                    target: Target::Text(setting),
                                    field: TextField::new(cx, edit_value.clone()),
                                });
                                cx.notify();
                            }))
                            .child(current.to_uppercase())
                            .into_any_element()
                    }),
            )
    }

    fn select_row(
        &mut self,
        id: &str,
        label: String,
        value: String,
        glyph: &'static str,
        kind: MenuKind,
        cx: &mut Context<Self>,
    ) -> Div {
        let colors = self.colors(cx);
        let open_kind = kind.clone();
        let label_row = self.field_label(label, None, cx);
        let menu = self.select_menu(&kind, cx);

        div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap(px(8.0))
            .child(label_row)
            .child(
                div()
                    .relative()
                    .w_full()
                    .child(
                        div()
                            .id(SharedString::from(format!("select-{id}")))
                            .flex()
                            .w_full()
                            .h(px(CONTROL_HEIGHT_PX))
                            .items_center()
                            .gap(px(8.0))
                            .px(px(10.0))
                            .rounded(rem(RADIUS_MD))
                            .border_1()
                            .border_color(colors.border)
                            .bg(colors.accent)
                            .cursor_pointer()
                            .text_size(rem(TEXT_SM))
                            .overflow_hidden()
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                let same = this.menu_kind.as_ref() == Some(&open_kind);
                                this.menu_kind = Some(open_kind.clone());
                                if same {
                                    this.menu.toggle();
                                } else {
                                    this.menu.set_open(true);
                                }
                                cx.notify();
                            }))
                            .child(
                                svg()
                                    .size(px(CONTROL_ICON_PX))
                                    .flex_shrink_0()
                                    .path(icon(glyph))
                                    .text_color(colors.muted_foreground),
                            )
                            .child(div().flex_1().min_w_0().truncate().child(value))
                            .child(
                                svg()
                                    .size(px(14.0))
                                    .flex_shrink_0()
                                    .path(icon("arrow-down"))
                                    .text_color(colors.muted_foreground),
                            ),
                    )
                    .children(menu),
            )
    }

    fn select_menu(&mut self, kind: &MenuKind, cx: &mut Context<Self>) -> Option<Div> {
        if self.menu_kind.as_ref() != Some(kind) {
            return None;
        }
        let frame = self.menu.frame();
        if !frame.visible {
            return None;
        }
        let colors = self.colors(cx);
        let element = self.selected(cx)?;
        let element_id = element.base().id.clone();

        let options: Vec<(String, String)> = match kind {
            MenuKind::BlendMode => BLEND_MODES
                .iter()
                .map(|(value, key)| (value.to_string(), t(key)))
                .collect(),
            MenuKind::FontFamily => FONT_FAMILIES
                .iter()
                .map(|family| (family.to_string(), family.to_string()))
                .collect(),
            MenuKind::GraphicSelect(key) => graphic_param(&element, key)
                .map(|param| {
                    param
                        .options
                        .iter()
                        .map(|(value, label)| (value.to_string(), t(label)))
                        .collect()
                })
                .unwrap_or_default(),
        };
        let current = match kind {
            MenuKind::BlendMode => edit::blend_mode_of(&element).to_string(),
            MenuKind::FontFamily => edit::text_of(&element)
                .map(|text| text.font_family.clone())
                .unwrap_or_default(),
            MenuKind::GraphicSelect(key) => graphic_param(&element, key)
                .map(|param| edit::graphic_param_text(&element, param))
                .unwrap_or_default(),
        };

        let natural = (
            SELECT_MENU_WIDTH_PX,
            menu_natural_height(
                options.len().min(12),
                crate::components::MENU_ITEM_HEIGHT_PX,
            ),
        );
        let placement = place_anchored(
            frame,
            OverlaySide::Bottom,
            natural,
            (0.0, CONTROL_HEIGHT_PX + MENU_OFFSET_PX),
        );

        let kind = kind.clone();
        let items = options
            .into_iter()
            .map(|(value, label)| {
                let key = format!("select-item-{value}");
                let highlighted = self.transitions.eased(&key) > 0.5;
                let selected = value == current;
                let hover_key = key.clone();
                let pick = value.clone();
                let element_id = element_id.clone();
                let kind = kind.clone();
                menu_item(
                    SharedString::from(key),
                    colors,
                    label,
                    highlighted,
                    selected,
                )
                .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                    this.transitions.set(hover_key.clone(), *hovered);
                    cx.notify();
                }))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    let setting = match kind {
                        MenuKind::BlendMode => Setting::BlendMode(pick.clone()),
                        MenuKind::FontFamily => Setting::FontFamily(pick.clone()),
                        MenuKind::GraphicSelect(key) => {
                            Setting::GraphicParamText(key, pick.clone())
                        }
                    };
                    this.apply_setting(&element_id.clone(), setting, cx);
                    this.menu.dismiss();
                    cx.notify();
                }))
            })
            .collect::<Vec<_>>();

        Some(
            crate::components::overlay_root()
                .child(overlay_backdrop("properties-menu-backdrop").on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.menu.dismiss();
                        cx.notify();
                    }),
                ))
                .child(overlay_layer(
                    gpui::Corner::TopLeft,
                    placement,
                    menu_surface(colors, placement)
                        .max_h(px(natural.1))
                        .children(items),
                )),
        )
    }

    fn slider_row(
        &mut self,
        id: &str,
        label: String,
        fraction: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let colors = self.colors(cx);
        let key = format!("slider-{id}");
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(label),
            )
            .child(
                div()
                    .id(SharedString::from(key.clone()))
                    .flex()
                    .w_full()
                    .h(px(20.0))
                    .items_center()
                    .cursor_pointer()
                    .on_drag(SliderDrag { key }, |_, _, _, cx| cx.new(|_| gpui::Empty))
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(SLIDER_TRACK_PX))
                            .rounded(px(SLIDER_TRACK_PX / 2.0))
                            .bg(colors.accent)
                            .child(
                                div()
                                    .w(relative(fraction.clamp(0.0, 1.0)))
                                    .h_full()
                                    .rounded(px(SLIDER_TRACK_PX / 2.0))
                                    .bg(colors.primary),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(-4.0))
                                    .left(relative(fraction.clamp(0.0, 1.0)))
                                    .ml(px(-SLIDER_THUMB_PX / 2.0))
                                    .size(px(SLIDER_THUMB_PX))
                                    .rounded_full()
                                    .border_1()
                                    .border_color(colors.border)
                                    .bg(colors.foreground),
                            ),
                    ),
            )
    }

    fn on_slider(
        &mut self,
        event: &gpui::DragMoveEvent<SliderDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.drag(cx).key.clone();
        let bounds = event.bounds;
        let width = f32::from(bounds.right() - bounds.left());
        if width <= 0.0 {
            return;
        }
        let fraction =
            (f32::from(event.event.position.x - bounds.left()) / width).clamp(0.0, 1.0) as f64;
        let Some(element) = self.selected(cx) else {
            return;
        };
        let id = element.base().id.clone();

        if let Some(suffix) = key
            .rsplit("tracking-")
            .next()
            .filter(|suffix| matches!(*suffix, "region-x" | "region-y" | "region-w" | "region-h"))
        {
            let suffix = suffix.to_string();
            self.app.update(cx, |model, cx| {
                let mut region = model.tracking_region;
                match suffix.as_str() {
                    "region-x" => region.x = fraction as f32,
                    "region-y" => region.y = fraction as f32,
                    "region-w" => region.width = fraction as f32,
                    _ => region.height = fraction as f32,
                }
                model.tracking_region = region.clamped();
                cx.notify();
            });
            self.slider = Some(key);
            cx.notify();
            return;
        }

        if key.ends_with("tracking-threshold") {
            self.tracking_threshold = fraction as f32 * 0.9;
            self.slider = Some(key);
            cx.notify();
            return;
        }

        if key.ends_with("stabilize-strength") {
            self.stabilize_strength = fraction as f32;
            self.slider = Some(key);
            cx.notify();
            return;
        }

        if key.ends_with("reframe-zoom") {
            self.reframe_zoom =
                1.0 + fraction as f32 * (crate::reframe::MAX_ZOOM - crate::reframe::DEFAULT_ZOOM);
            self.slider = Some(key);
            cx.notify();
            return;
        }

        if key.ends_with("motion-intensity") {
            let span = crate::motion::MOTION_MAX_INTENSITY - crate::motion::MOTION_MIN_INTENSITY;
            self.motion_intensity =
                round_to(crate::motion::MOTION_MIN_INTENSITY + fraction * span, 0.01);
            self.slider = Some(key);
            let preset = edit::motion_of(&element).map(|motion| motion.preset_id.clone());
            if let Some(preset) = preset {
                self.apply_motion(&preset, cx);
            }
            cx.notify();
            return;
        }

        if key.ends_with("transition-duration") {
            let Some((previous_duration, own_duration)) = self.transition_bounds(&element, cx)
            else {
                return;
            };
            let Some(mut transition) = edit::transition_of(&element).cloned() else {
                return;
            };
            let ceiling = previous_duration.min(own_duration).as_ticks().max(0);
            let floor = cutix_playback::transitions::MIN_TRANSITION_DURATION_TICKS.min(ceiling);
            let ticks = floor + ((ceiling - floor) as f64 * fraction).round() as i64;
            transition.duration = cutix_playback::transitions::clamp_transition_duration(
                MediaTime::from_ticks(ticks),
                previous_duration,
                own_duration,
            );
            self.slider = Some(key.clone());
            self.app.update(cx, |model, cx| {
                model.edit_coalesced(Some(key), cx, |editor| {
                    editor.set_element_transition(&id, Some(transition))
                })
            });
            cx.notify();
            return;
        }

        if key.ends_with("mask-stroke-width") {
            let width = (fraction * MASK_STROKE_MAX_PX).round();
            self.slider = Some(key.clone());
            self.app.update(cx, |model, cx| {
                model.edit_coalesced(Some(key), cx, |editor| {
                    editor.set_mask_param(&id, "strokeWidth", serde_json::json!(width))
                })
            });
            cx.notify();
            return;
        }

        if key.ends_with("mask-feather") {
            let feather = (fraction * MASK_FEATHER_MAX_PX).round();
            self.slider = Some(key.clone());
            self.app.update(cx, |model, cx| {
                model.edit_coalesced(Some(key), cx, |editor| {
                    editor.set_mask_param(&id, "feather", serde_json::json!(feather))
                })
            });
            cx.notify();
            return;
        }

        if let Some(index) = key
            .rsplit("-eq-")
            .next()
            .and_then(|suffix| suffix.parse::<usize>().ok())
            .filter(|index| key.contains("-eq-") && *index < self.eq_gains.len())
        {
            let limit = dsp::EQUALIZER_GAIN_LIMIT_DB as f64;
            self.eq_gains[index] = (round_to(fraction * 2.0 * limit - limit, 0.5)) as f32;
            self.slider = Some(key);
            cx.notify();
            return;
        }

        if key.ends_with("pitch-semitones") {
            self.pitch_semitones = round_to(fraction * 24.0 - 12.0, 0.5) as f32;
            self.slider = Some(key);
            cx.notify();
            return;
        }

        if key.ends_with("pitch-formant") {
            self.pitch_formant = round_to(fraction * 24.0 - 12.0, 0.5) as f32;
            self.slider = Some(key);
            cx.notify();
            return;
        }

        if key.ends_with("reverb-wet") {
            self.reverb_wet = round_to(fraction, 0.05) as f32;
            self.slider = Some(key);
            cx.notify();
            return;
        }

        if key.ends_with("denoise-strength") {
            self.denoise_strength = round_to(fraction, 0.05) as f32;
            self.slider = Some(key);
            cx.notify();
            return;
        }

        if key.ends_with("beat-sensitivity") {
            self.beat_sensitivity = round_to(0.5 + fraction * 2.5, 0.05) as f32;
            self.slider = Some(key);
            cx.notify();
            return;
        }

        if key.ends_with("silence-threshold") {
            self.silence_threshold_db = round_to(-70.0 + fraction * 60.0, 1.0) as f32;
            self.silence_ranges = None;
            self.slider = Some(key);
            cx.notify();
            return;
        }

        if key.ends_with("silence-minimum") {
            self.silence_min_seconds = round_to(0.1 + fraction * 2.9, 0.05) as f32;
            self.silence_ranges = None;
            self.slider = Some(key);
            cx.notify();
            return;
        }

        let duration = element.base().duration;
        let max = fade_max(duration);
        let (current_in, current_out) = edit::read_audio_fade(&element);
        let seconds = fraction * max;
        let value = MediaTime::from_seconds_f64(seconds).unwrap_or(MediaTime::ZERO);
        let (fade_in, fade_out) = if key.ends_with("fade-in") {
            (value, current_out)
        } else {
            (current_in, value)
        };
        self.slider = Some(key.clone());
        self.app.update(cx, |model, cx| {
            model.edit_coalesced(Some(key), cx, |editor| {
                editor.set_audio_fade(&id, fade_in, fade_out)
            })
        });
        cx.notify();
    }

    fn commit_editing(&mut self, cx: &mut Context<Self>) {
        let Some(editing) = self.editing.take() else {
            return;
        };
        let text = editing.field.text().trim().to_string();
        match editing.target {
            Target::Number { field, scale } => {
                if let Ok(parsed) = text.parse::<f64>() {
                    self.commit_number(&editing.element, field, parsed / scale, cx);
                }
            }
            Target::Text(setting) => {
                let value = if matches!(setting, TextSetting::Content) {
                    text
                } else if text.starts_with('#') {
                    text
                } else {
                    format!("#{text}")
                };
                let setting = match setting {
                    TextSetting::Content => Setting::Content(value),
                    TextSetting::TextColor => Setting::TextColor(value),
                    TextSetting::BackgroundColor => Setting::BackgroundColor(value),
                    TextSetting::StrokeColor => Setting::StrokeColor(value),
                    TextSetting::ShadowColor => Setting::ShadowColor(value),
                    TextSetting::GraphicColor(key) => Setting::GraphicParamText(key, value),
                };
                self.apply_setting(&editing.element, setting, cx);
            }
            Target::EffectColor { effect_id, key } => {
                let value = if text.starts_with('#') {
                    text
                } else {
                    format!("#{text}")
                };
                let element_id = editing.element.clone();
                self.app.update(cx, |model, cx| {
                    model.edit(cx, |editor| {
                        editor.update_clip_effect_params(
                            &element_id,
                            &effect_id,
                            vec![(key.clone(), serde_json::json!(value))],
                        )
                    })
                });
            }
        }
        cx.notify();
    }
}

#[derive(Clone, Copy, Debug)]
pub enum NumberIcon {
    Text(&'static str),
    Glyph(&'static str),
}

#[derive(Clone, Copy, Debug)]
enum ButtonFace {
    Glyph(&'static str),
    Label,
}

fn graphic_param(
    element: &TimelineElement,
    key: &str,
) -> Option<&'static stickers::ParamDefinition> {
    edit::graphic_of(element)
        .and_then(|graphic| stickers::definition(&graphic.definition_id))
        .and_then(|definition| definition.params.iter().find(|param| param.key == key))
}

fn graphic_option_label(param: &stickers::ParamDefinition, value: &str) -> String {
    param
        .options
        .iter()
        .find(|(option, _)| *option == value)
        .map(|(_, label)| t(label))
        .unwrap_or_else(|| value.to_owned())
}

fn fade_max(duration: MediaTime) -> f64 {
    duration.to_seconds_f64().clamp(0.1, 10.0)
}

fn inline_editor(
    editing: &Editing,
    colors: Palette,
    window: &mut Window,
    cx: &mut Context<PropertiesPanel>,
) -> impl IntoElement {
    let element = text_field(
        SharedString::from(format!("edit-{}", editing.key)),
        &editing.field,
        colors,
        FieldStyle {
            height: CONTROL_HEIGHT_PX - 6.0,
            placeholder: SharedString::from(""),
            leading: None,
            ..Default::default()
        },
        window,
    )
    .on_key_down(cx.listener(
        |this: &mut PropertiesPanel, event: &gpui::KeyDownEvent, _, cx| {
            let Some(editing) = this.editing.as_mut() else {
                return;
            };
            match editing.field.buffer.key_down(event) {
                TextEvent::Submit => this.commit_editing(cx),
                TextEvent::Cancel => this.editing = None,
                _ => {}
            }
            cx.notify();
        },
    ));
    window.focus(&editing.field.focus);
    element
}

impl PropertiesPanel {
    fn graphic_sections(
        &mut self,
        element: &TimelineElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let Some(definition) = edit::graphic_of(element)
            .and_then(|graphic| stickers::definition(&graphic.definition_id))
        else {
            return Vec::new();
        };

        let mut fill = Vec::new();
        let mut stroke = Vec::new();
        let mut shape = Vec::new();

        for param in definition.params {
            let label = t(param.label_key);
            let row = match param.kind {
                stickers::ParamKind::Color => self.color_row(
                    element,
                    label,
                    TextSetting::GraphicColor(param.key),
                    edit::graphic_param_text(element, param),
                    None,
                    window,
                    cx,
                ),
                stickers::ParamKind::Select => self.select_row(
                    &format!("graphic-{}", param.key),
                    label,
                    graphic_option_label(param, &edit::graphic_param_text(element, param)),
                    "sliders-horizontal",
                    MenuKind::GraphicSelect(param.key),
                    cx,
                ),
                stickers::ParamKind::Number => self.number_row(
                    element,
                    Field::GraphicParam(param),
                    label,
                    NumberIcon::Glyph("sliders-horizontal"),
                    1.0,
                    if param.step >= 1.0 { 0 } else { 2 },
                    true,
                    window,
                    cx,
                ),
            };
            match param.group {
                Some("stroke") => stroke.push(row),
                _ if param.key == "fill" => fill.push(row),
                _ => shape.push(row),
            }
        }

        let mut sections = Vec::new();
        if !fill.is_empty() {
            sections.push(self.section(
                String::from("graphic-fill"),
                t("graphics.param.fill"),
                fill,
                cx,
            ));
        }
        if !stroke.is_empty() {
            sections.push(self.section(
                String::from("graphic-stroke"),
                t("properties.stroke"),
                stroke,
                cx,
            ));
        }
        if !shape.is_empty() {
            sections.push(self.section(
                String::from("graphic-shape"),
                t(definition.name_key),
                shape,
                cx,
            ));
        }
        sections
    }

    fn transition_bounds(
        &self,
        element: &TimelineElement,
        cx: &App,
    ) -> Option<(MediaTime, MediaTime)> {
        let model = self.app.read(cx);
        let scene = model.current_scene()?;
        let id = &element.base().id;
        std::iter::once(&scene.tracks.main)
            .chain(scene.tracks.overlay.iter())
            .find_map(|track| cutix_playback::transitions::find_transition_neighbour(track, id))
            .map(|previous| (previous.base().duration, element.base().duration))
    }

    fn transition_sections(
        &mut self,
        element: &TimelineElement,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let colors = self.colors(cx);
        let element_id = element.base().id.clone();
        let Some((previous_duration, own_duration)) = self.transition_bounds(element, cx) else {
            return vec![self.section(
                format!("{element_id}:transition"),
                t("transitions.title"),
                vec![self.notice_row(t("transitions.noNeighbour"), colors)],
                cx,
            )];
        };

        let current = edit::transition_of(element).cloned();
        let ceiling_ticks = previous_duration.min(own_duration).as_ticks().max(0);
        let floor_ticks =
            cutix_playback::transitions::MIN_TRANSITION_DURATION_TICKS.min(ceiling_ticks);

        let selected_type = current
            .as_ref()
            .map(|transition| transition.transition_type.clone());
        let mut body: Vec<Div> = Vec::new();

        let mut options: Vec<(String, String)> = vec![("none".to_string(), t("transitions.none"))];
        options.extend(
            crate::effects_ui::TRANSITION_KEYS
                .iter()
                .map(|(id, label)| ((*id).to_string(), t(label))),
        );
        body.push(self.field_label(t("transitions.type"), None, cx));
        for (chunk_index, chunk) in options.chunks(3).enumerate() {
            let chunk = chunk.to_vec();
            let id_for_row = element_id.clone();
            body.push(self.chip_row(
                &format!("transition-type-{chunk_index}"),
                chunk,
                selected_type.as_deref().or(Some("none")),
                cx,
                move |this, value, cx| {
                    let element_id = id_for_row.clone();
                    if value == "none" {
                        this.set_transition(&element_id, None, cx);
                    } else {
                        this.set_transition_type(&element_id, &value, cx);
                    }
                },
            ));
        }

        if let Some(transition) = current {
            let duration = cutix_playback::transitions::clamp_transition_duration(
                transition.duration,
                previous_duration,
                own_duration,
            );
            let seconds = duration.as_ticks() as f64 / time::TICKS_PER_SECOND as f64;
            let span = (ceiling_ticks - floor_ticks).max(1) as f64;
            let fraction = ((duration.as_ticks() - floor_ticks) as f64 / span) as f32;
            body.push(self.slider_row(
                "transition-duration",
                format!("{}  {seconds:.2}s", t("transitions.duration")),
                fraction,
                cx,
            ));
            body.push(self.notice_row(
                t_args(
                    "transitions.durationRange",
                    &[
                        (
                            "min",
                            &format!("{:.2}", floor_ticks as f64 / time::TICKS_PER_SECOND as f64),
                        ),
                        (
                            "max",
                            &format!(
                                "{:.2}",
                                ceiling_ticks as f64 / time::TICKS_PER_SECOND as f64
                            ),
                        ),
                    ],
                ),
                colors,
            ));

            let easing = transition.easing.clone().unwrap_or_else(|| {
                cutix_playback::transitions::DEFAULT_TRANSITION_EASING.to_owned()
            });
            body.push(self.field_label(t("transitions.easing"), None, cx));
            let easings: Vec<(String, String)> = crate::effects_ui::TRANSITION_EASING_KEYS
                .iter()
                .map(|(id, label)| ((*id).to_string(), t(label)))
                .collect();
            let id_for_easing = element_id.clone();
            body.push(self.chip_row(
                "transition-easing",
                easings,
                Some(easing.as_str()),
                cx,
                move |this, value, cx| {
                    let element_id = id_for_easing.clone();
                    this.set_transition_easing(&element_id, &value, cx);
                },
            ));

            let id_for_remove = element_id.clone();
            body.push(self.action_row(
                "transition-remove",
                t("transitions.remove"),
                false,
                cx,
                move |this, cx| {
                    let element_id = id_for_remove.clone();
                    this.set_transition(&element_id, None, cx);
                },
            ));
        }

        vec![self.section(
            format!("{element_id}:transition"),
            t("transitions.title"),
            body,
            cx,
        )]
    }

    fn set_transition(
        &mut self,
        element_id: &str,
        transition: Option<cutix_project::model::ElementTransition>,
        cx: &mut Context<Self>,
    ) {
        let element_id = element_id.to_string();
        self.app.update(cx, |model, cx| {
            model.edit(cx, |editor| {
                editor.set_element_transition(&element_id, transition)
            })
        });
        cx.notify();
    }

    fn set_transition_type(&mut self, element_id: &str, kind: &str, cx: &mut Context<Self>) {
        let existing = self
            .selected(cx)
            .as_ref()
            .and_then(edit::transition_of)
            .cloned();
        let transition = cutix_project::model::ElementTransition {
            transition_type: kind.to_owned(),
            duration: existing
                .as_ref()
                .map(|transition| transition.duration)
                .unwrap_or_else(|| {
                    MediaTime::from_ticks(
                        cutix_playback::transitions::DEFAULT_TRANSITION_DURATION_TICKS,
                    )
                }),
            easing: Some(
                existing
                    .and_then(|transition| transition.easing)
                    .unwrap_or_else(|| {
                        cutix_playback::transitions::DEFAULT_TRANSITION_EASING.to_owned()
                    }),
            ),
        };
        self.set_transition(element_id, Some(transition), cx);
    }

    fn set_transition_easing(&mut self, element_id: &str, easing: &str, cx: &mut Context<Self>) {
        let Some(mut transition) = self
            .selected(cx)
            .as_ref()
            .and_then(edit::transition_of)
            .cloned()
        else {
            return;
        };
        transition.easing = Some(easing.to_owned());
        self.set_transition(element_id, Some(transition), cx);
    }

    fn effects_sections(
        &mut self,
        element: &TimelineElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let colors = self.colors(cx);
        let element_id = element.base().id.clone();
        let effects = crate::edit::effects_of(element).to_vec();
        if effects.is_empty() {
            return vec![div()
                .flex()
                .flex_col()
                .w_full()
                .items_center()
                .gap(px(8.0))
                .p(px(SECTION_PAD_PX))
                .child(
                    svg()
                        .size(px(40.0))
                        .path(icon("magic-wand05"))
                        .text_color(opacity(colors.muted_foreground, 0.75)),
                )
                .child(
                    div()
                        .text_size(rem(TEXT_SM))
                        .font_weight(FontWeight::MEDIUM)
                        .child(t("properties.effects.empty")),
                )
                .child(
                    div()
                        .text_size(rem(TEXT_XS))
                        .text_color(colors.muted_foreground)
                        .text_center()
                        .child(t("properties.effects.emptyHint")),
                )];
        }

        let mut cards: Vec<Div> = vec![div()
            .w_full()
            .px(px(SECTION_PAD_PX))
            .pt(px(10.0))
            .child(self.notice_row(t("properties.effects.reorderHint"), colors))];
        for (index, effect) in effects.into_iter().enumerate() {
            cards.push({
                let definition = crate::effects_ui::definition(&effect.effect_type);
                let title = definition
                    .map(|definition| t(definition.name_key))
                    .unwrap_or_else(|| effect.effect_type.clone());
                let mut rows: Vec<Div> = Vec::new();
                if let Some(definition) = definition {
                    for param in definition.params {
                        if matches!(param.kind, crate::effects_ui::ParamKind::Lut) {
                            rows.push(self.lut_row(&element_id, &effect, param, cx));
                            continue;
                        }
                        if matches!(param.kind, crate::effects_ui::ParamKind::Color) {
                            let current = effect
                                .params
                                .get(param.key)
                                .and_then(|value| value.as_str())
                                .map(|value| value.to_string())
                                .unwrap_or_else(|| param.default_text.to_string());
                            rows.push(self.effect_color_row(
                                &element_id,
                                &effect.id,
                                param,
                                current,
                                window,
                                cx,
                            ));
                            continue;
                        }
                        let value = effect
                            .params
                            .get(param.key)
                            .cloned()
                            .unwrap_or_else(|| param.default_value());
                        let display = match &value {
                            serde_json::Value::Number(number) => {
                                format!("{:.2}", number.as_f64().unwrap_or(0.0))
                            }
                            serde_json::Value::String(text) if text.len() <= 24 => text.clone(),
                            _ => String::from("..."),
                        };
                        rows.push(
                            div()
                                .flex()
                                .w_full()
                                .justify_between()
                                .text_size(rem(TEXT_XS))
                                .text_color(colors.muted_foreground)
                                .child(t(param.label_key))
                                .child(display),
                        );
                    }
                }

                let toggle_id = effect.id.clone();
                let remove_id = effect.id.clone();
                let toggle_element = element_id.clone();
                let remove_element = element_id.clone();
                let enabled = effect.enabled;
                let header = div()
                    .flex()
                    .w_full()
                    .items_center()
                    .justify_between()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .id(SharedString::from(format!("effect-grip-{}", effect.id)))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size(px(CONTROL_ICON_PX + 4.0))
                                    .cursor_pointer()
                                    .on_drag(
                                        EffectDrag {
                                            element: element_id.clone(),
                                            index,
                                        },
                                        |_, _, _, cx| cx.new(|_| gpui::Empty),
                                    )
                                    .child(
                                        svg()
                                            .size(px(CONTROL_ICON_PX))
                                            .path(icon("left-to-right-list-dash"))
                                            .text_color(colors.muted_foreground),
                                    ),
                            )
                            .child(
                                div()
                                    .text_size(rem(TEXT_SM))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if enabled {
                                        colors.foreground
                                    } else {
                                        colors.muted_foreground
                                    })
                                    .child(title),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .id(SharedString::from(format!("effect-toggle-{toggle_id}")))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size(px(CONTROL_HEIGHT_PX))
                                    .rounded(rem(RADIUS_SM))
                                    .cursor_pointer()
                                    .bg(if enabled {
                                        opacity(colors.accent, 0.8)
                                    } else {
                                        opacity(colors.accent, 0.0)
                                    })
                                    .child(
                                        svg()
                                            .size(px(CONTROL_ICON_PX))
                                            .path(icon("eye"))
                                            .text_color(if enabled {
                                                colors.foreground
                                            } else {
                                                colors.muted_foreground
                                            }),
                                    )
                                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                        let element_id = toggle_element.clone();
                                        let effect_id = toggle_id.clone();
                                        this.app.update(cx, |model, cx| {
                                            model.edit(cx, |editor| {
                                                editor.toggle_clip_effect(&element_id, &effect_id)
                                            })
                                        });
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("effect-remove-{remove_id}")))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .size(px(CONTROL_HEIGHT_PX))
                                    .rounded(rem(RADIUS_SM))
                                    .cursor_pointer()
                                    .child(
                                        svg()
                                            .size(px(CONTROL_ICON_PX))
                                            .path(icon("delete02"))
                                            .text_color(colors.destructive),
                                    )
                                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                        let element_id = remove_element.clone();
                                        let effect_id = remove_id.clone();
                                        this.app.update(cx, |model, cx| {
                                            model.edit(cx, |editor| {
                                                editor.remove_clip_effect(&element_id, &effect_id)
                                            })
                                        });
                                        cx.notify();
                                    })),
                            ),
                    );

                div().w_full().child(
                    div()
                        .id(SharedString::from(format!("effect-card-{}", effect.id)))
                        .flex()
                        .flex_col()
                        .w_full()
                        .gap(px(6.0))
                        .p(px(SECTION_PAD_PX))
                        .border_b_1()
                        .border_color(colors.border)
                        .on_drop(
                            cx.listener(move |this: &mut Self, drag: &EffectDrag, _, cx| {
                                this.reorder_effect(drag.element.clone(), drag.index, index, cx);
                            }),
                        )
                        .child(header)
                        .children(rows),
                )
            });
        }
        cards
    }

    fn effect_color_row(
        &mut self,
        element_id: &str,
        effect_id: &str,
        param: &'static crate::effects_ui::ParamDefinition,
        current: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let colors = self.colors(cx);
        let key = format!("{element_id}-effect-{effect_id}-{}", param.key);
        let editing = self.editing.as_ref().is_some_and(|edit| edit.key == key);
        let swatch = crate::theme::parse_hex(current.trim_start_matches('#'));

        let label_row = div()
            .flex()
            .w_full()
            .h(px(LABEL_ROW_HEIGHT_PX))
            .items_center()
            .child(
                div()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(t(param.label_key)),
            );

        let edit_key = key.clone();
        let edit_element = element_id.to_string();
        let edit_effect = effect_id.to_string();
        let edit_param = param.key.to_string();
        let edit_value = current.clone();

        div()
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap(px(8.0))
            .child(label_row)
            .child(
                div()
                    .flex()
                    .w_full()
                    .h(px(CONTROL_HEIGHT_PX))
                    .items_center()
                    .gap(px(8.0))
                    .rounded(rem(RADIUS_MD))
                    .border_1()
                    .border_color(if editing {
                        colors.primary
                    } else {
                        colors.border
                    })
                    .bg(colors.accent)
                    .px(px(8.0))
                    .overflow_hidden()
                    .child(
                        div()
                            .size(px(16.0))
                            .flex_shrink_0()
                            .rounded(px(3.0))
                            .border_1()
                            .border_color(colors.border)
                            .bg(swatch),
                    )
                    .child(if editing {
                        let field_ref = self.editing.as_ref().expect("editing field");
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .child(inline_editor(field_ref, colors, window, cx))
                            .into_any_element()
                    } else {
                        div()
                            .id(SharedString::from(format!("effect-color-value-{key}")))
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .items_center()
                            .cursor_text()
                            .text_size(rem(TEXT_SM))
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.editing = Some(Editing {
                                    key: edit_key.clone(),
                                    element: edit_element.clone(),
                                    target: Target::EffectColor {
                                        effect_id: edit_effect.clone(),
                                        key: edit_param.clone(),
                                    },
                                    field: TextField::new(cx, edit_value.clone()),
                                });
                                cx.notify();
                            }))
                            .child(current.to_uppercase())
                            .into_any_element()
                    }),
            )
    }

    fn reorder_effect(
        &mut self,
        element_id: String,
        from: usize,
        to: usize,
        cx: &mut Context<Self>,
    ) {
        if from == to {
            return;
        }
        self.app.update(cx, |model, cx| {
            model.edit(cx, |editor| {
                editor.reorder_clip_effect(&element_id, from, to)
            })
        });
        cx.notify();
    }

    fn lut_row(
        &mut self,
        element_id: &str,
        effect: &cutix_project::model::Effect,
        param: &'static crate::effects_ui::ParamDefinition,
        cx: &mut Context<Self>,
    ) -> Div {
        let colors = self.colors(cx);
        let name = effect
            .params
            .get(param.key)
            .and_then(|value| value.as_str())
            .unwrap_or_default()
            .to_string();
        let size = effect
            .params
            .get("size")
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0) as u32;
        let loaded = !name.is_empty() && size >= 2;

        let status = if loaded {
            t_args("effects.lut.info3d", &[("size", &size.to_string())])
        } else {
            t(param.label_key)
        };

        let import_element = element_id.to_string();
        let import_effect = effect.id.clone();
        let clear_element = element_id.to_string();
        let clear_effect = effect.id.clone();

        let mut row = div()
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
                    .child(status)
                    .child(if loaded { name } else { String::new() }),
            )
            .child(self.action_row(
                "effect-lut-import",
                t("effects.lut.import"),
                false,
                cx,
                move |this, cx| {
                    let element_id = import_element.clone();
                    let effect_id = import_effect.clone();
                    this.import_lut(element_id, effect_id, cx);
                },
            ));
        if loaded {
            row = row.child(self.action_row(
                "effect-lut-clear",
                t("effects.lut.clear"),
                false,
                cx,
                move |this, cx| {
                    let element_id = clear_element.clone();
                    let effect_id = clear_effect.clone();
                    this.app.update(cx, |model, cx| {
                        model.edit(cx, |editor| {
                            editor.update_clip_effect_params(
                                &element_id,
                                &effect_id,
                                vec![
                                    ("lut".to_string(), serde_json::json!("")),
                                    ("size".to_string(), serde_json::json!(0)),
                                    ("table".to_string(), serde_json::json!([])),
                                ],
                            )
                        })
                    });
                    cx.notify();
                },
            ));
        }
        if let Some(message) = self.lut_notice.clone() {
            row = row.child(self.notice_row(message, colors));
        }
        row
    }

    fn import_lut(&mut self, element_id: String, effect_id: String, cx: &mut Context<Self>) {
        let task = cx.background_spawn(crate::dialogs::open_files(
            crate::dialogs::Filter::Lut,
            t("effects.lut.import"),
            false,
        ));
        cx.spawn(async move |this, cx| {
            let paths = task.await;
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let parsed = crate::lut::load_cube(&path);
            let _ = this.update(cx, |this: &mut Self, cx| {
                match parsed {
                    Ok(lut) => {
                        this.lut_notice =
                            Some(t_args("effects.lut.loaded", &[("name", &lut.name)]));
                        this.app.update(cx, |model, cx| {
                            model.edit(cx, |editor| {
                                editor.update_clip_effect_params(
                                    &element_id,
                                    &effect_id,
                                    vec![
                                        ("lut".to_string(), serde_json::json!(lut.name)),
                                        ("size".to_string(), serde_json::json!(lut.size)),
                                        ("table".to_string(), serde_json::json!(lut.table)),
                                    ],
                                )
                            })
                        });
                    }
                    Err(message) => {
                        this.lut_notice =
                            Some(t_args("effects.lut.parseFailed", &[("message", &message)]));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn empty_state(&self, colors: Palette) -> Div {
        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .gap(px(12.0))
            .p(px(16.0))
            .child(
                svg()
                    .size(px(40.0))
                    .path(icon("settings05"))
                    .text_color(opacity(colors.muted_foreground, 0.75)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.0))
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
                            .child(t("editor.inspector.empty")),
                    ),
            )
    }
}

pub fn centered_aspect_crop(source_aspect: f64, target_aspect: f64) -> Crop {
    if !source_aspect.is_finite()
        || source_aspect <= 0.0
        || !target_aspect.is_finite()
        || target_aspect <= 0.0
    {
        return Crop::default();
    }
    let ratio = target_aspect / source_aspect;
    let crop_width = if ratio <= 1.0 { ratio } else { 1.0 };
    let crop_height = if ratio <= 1.0 { 1.0 } else { 1.0 / ratio };
    let horizontal = (1.0 - crop_width) / 2.0;
    let vertical = (1.0 - crop_height) / 2.0;
    Crop {
        left: horizontal,
        right: horizontal,
        top: vertical,
        bottom: vertical,
    }
}

fn panel_frame(colors: Palette) -> Stateful<Div> {
    div()
        .id("properties-panel")
        .flex()
        .size_full()
        .overflow_hidden()
        .rounded(rem(RADIUS_SM))
        .border_1()
        .border_color(colors.border)
        .bg(colors.background)
        .text_color(colors.foreground)
}

fn round_to(value: f64, step: f64) -> f64 {
    if step <= 0.0 {
        return value;
    }
    (value / step).round() * step
}

fn row2(left: Div, right: Div) -> Div {
    div()
        .flex()
        .w_full()
        .items_end()
        .gap(px(8.0))
        .child(left.flex_1().min_w_0())
        .child(right.flex_1().min_w_0())
}

impl PropertiesPanel {
    fn transform_sections(
        &mut self,
        element: &TimelineElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let colors = self.colors(cx);
        let id = element.base().id.clone();
        let locked = self.scale_locked;
        let lock_progress = self.transitions.eased("transform-lock");

        let lock = Button::new("transform-lock", colors)
            .variant(if locked {
                ButtonVariant::Secondary
            } else {
                ButtonVariant::Ghost
            })
            .size(ButtonSize::Icon)
            .hover(lock_progress)
            .icon("link05")
            .build()
            .on_hover(cx.listener(|this: &mut Self, hovered: &bool, _, cx| {
                this.transitions.set("transform-lock", *hovered);
                cx.notify();
            }))
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                this.scale_locked = !this.scale_locked;
                cx.notify();
            }));

        let scale_row = if locked {
            let field = self.number_row(
                element,
                Field::ScaleX,
                t("properties.scale"),
                NumberIcon::Glyph("arrow-expand"),
                100.0,
                0,
                true,
                window,
                cx,
            );
            div()
                .flex()
                .w_full()
                .items_end()
                .gap(px(8.0))
                .child(field.flex_1().min_w_0())
                .child(lock)
        } else {
            let width = self.number_row(
                element,
                Field::ScaleX,
                t("common.width"),
                NumberIcon::Text("W"),
                100.0,
                0,
                true,
                window,
                cx,
            );
            let height = self.number_row(
                element,
                Field::ScaleY,
                t("common.height"),
                NumberIcon::Text("H"),
                100.0,
                0,
                true,
                window,
                cx,
            );
            div()
                .flex()
                .w_full()
                .items_end()
                .gap(px(8.0))
                .child(width.flex_1().min_w_0())
                .child(lock)
                .child(height.flex_1().min_w_0())
        };

        let position_x = self.number_row(
            element,
            Field::PositionX,
            String::from("X"),
            NumberIcon::Text("X"),
            1.0,
            0,
            true,
            window,
            cx,
        );
        let position_y = self.number_row(
            element,
            Field::PositionY,
            String::from("Y"),
            NumberIcon::Text("Y"),
            1.0,
            0,
            true,
            window,
            cx,
        );
        let rotation = self.number_row(
            element,
            Field::Rotate,
            t("properties.rotation"),
            NumberIcon::Glyph("rotate-clockwise"),
            1.0,
            0,
            true,
            window,
            cx,
        );

        let scale_x = edit::field_value(element, Field::ScaleX);
        let scale_y = edit::field_value(element, Field::ScaleY);
        let flip_x_id = id.clone();
        let flip_y_id = id.clone();
        let flip = div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(t("properties.flip")),
            )
            .child(
                div()
                    .flex()
                    .w_full()
                    .gap(px(8.0))
                    .child(
                        Button::new("flip-horizontal", colors)
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .icon("flip-horizontal")
                            .label(t("properties.flipHorizontal"))
                            .build()
                            .flex_1()
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                let id = flip_x_id.clone();
                                this.commit_flip(&id, Field::ScaleX, -scale_x, cx);
                            })),
                    )
                    .child(
                        Button::new("flip-vertical", colors)
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Sm)
                            .icon("flip-vertical")
                            .label(t("properties.flipVertical"))
                            .build()
                            .flex_1()
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                let id = flip_y_id.clone();
                                this.commit_flip(&id, Field::ScaleY, -scale_y, cx);
                            })),
                    ),
            );

        let mut sections = vec![self.section(
            format!("{id}:transform"),
            t("properties.transform"),
            vec![scale_row, row2(position_x, position_y), rotation, flip],
            cx,
        )];
        sections.extend(self.motion_sections(element, cx));
        sections
    }

    fn commit_flip(&mut self, element_id: &str, field: Field, value: f64, cx: &mut Context<Self>) {
        let locked = self.scale_locked;
        self.scale_locked = false;
        self.commit_number(element_id, field, value, cx);
        self.scale_locked = locked;
    }

    fn crop_sections(
        &mut self,
        element: &TimelineElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let colors = self.colors(cx);
        let id = element.base().id.clone();
        let crop = edit::crop_of(element);
        let identity = crop == Crop::default();
        let aspect = self.source_aspect(element, cx);

        let mut presets = vec![{
            let element_id = id.clone();
            Button::new("crop-free", colors)
                .variant(if identity {
                    ButtonVariant::Secondary
                } else {
                    ButtonVariant::Ghost
                })
                .size(ButtonSize::Sm)
                .label(t("properties.crop.aspect.free"))
                .build()
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    let id = element_id.clone();
                    this.apply_setting(&id, Setting::Crop(Crop::default()), cx);
                }))
        }];
        for (label, ratio) in CROP_ASPECTS {
            let element_id = id.clone();
            let ratio = *ratio;
            presets.push(
                Button::new(SharedString::from(format!("crop-{label}")), colors)
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Sm)
                    .label(*label)
                    .build()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        let id = element_id.clone();
                        let crop = centered_aspect_crop(aspect, ratio);
                        this.apply_setting(&id, Setting::Crop(crop), cx);
                    })),
            );
        }

        let aspect_row = div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(t("properties.crop.aspect")),
            )
            .child(div().flex().flex_wrap().gap(px(4.0)).children(presets));

        let left = self.number_row(
            element,
            Field::CropLeft,
            t("properties.crop.left"),
            NumberIcon::Text("L"),
            100.0,
            0,
            true,
            window,
            cx,
        );
        let top = self.number_row(
            element,
            Field::CropTop,
            t("properties.crop.top"),
            NumberIcon::Text("T"),
            100.0,
            0,
            true,
            window,
            cx,
        );
        let right = self.number_row(
            element,
            Field::CropRight,
            t("properties.crop.right"),
            NumberIcon::Text("R"),
            100.0,
            0,
            true,
            window,
            cx,
        );
        let bottom = self.number_row(
            element,
            Field::CropBottom,
            t("properties.crop.bottom"),
            NumberIcon::Text("B"),
            100.0,
            0,
            true,
            window,
            cx,
        );

        vec![self.section(
            format!("{id}:crop"),
            t("properties.crop"),
            vec![aspect_row, row2(left, top), row2(right, bottom)],
            cx,
        )]
    }

    fn source_aspect(&self, element: &TimelineElement, cx: &App) -> f64 {
        let asset = element
            .media_id()
            .and_then(|id| self.app.read(cx).media_by_id(id).cloned());
        match asset {
            Some(asset) => match (asset.width, asset.height) {
                (Some(width), Some(height)) if height > 0 => width as f64 / height as f64,
                _ => 16.0 / 9.0,
            },
            None => 16.0 / 9.0,
        }
    }

    fn animation_chips(
        &mut self,
        slot: &'static str,
        element_id: &str,
        options: Vec<(&'static str, String)>,
        active: Option<String>,
        cx: &mut Context<Self>,
    ) -> Div {
        let colors = self.colors(cx);
        let chips = options
            .into_iter()
            .map(|(value, label)| {
                let selected = active.as_deref() == Some(value);
                let key = format!("text-anim-{slot}-{value}");
                let progress = self.transitions.eased(&key);
                let hover_key = key.clone();
                let element = element_id.to_string();
                div()
                    .id(SharedString::from(key))
                    .flex()
                    .h(px(CONTROL_HEIGHT_PX))
                    .items_center()
                    .justify_center()
                    .px(px(10.0))
                    .rounded(rem(RADIUS_SM))
                    .cursor_pointer()
                    .text_size(rem(TEXT_XS))
                    .bg(if selected {
                        colors.secondary
                    } else {
                        mix(opacity(colors.accent, 0.0), colors.accent, progress)
                    })
                    .text_color(if selected {
                        colors.secondary_foreground
                    } else {
                        colors.muted_foreground
                    })
                    .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                        this.transitions.set(hover_key.clone(), *hovered);
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        let next = if selected { None } else { Some(value) };
                        this.apply_text_animation(&element, slot, next, cx);
                    }))
                    .child(label)
            })
            .collect::<Vec<_>>();

        div()
            .flex()
            .w_full()
            .flex_wrap()
            .gap(px(4.0))
            .children(chips)
    }

    fn apply_text_animation(
        &mut self,
        element_id: &str,
        slot: &str,
        preset_id: Option<&'static str>,
        cx: &mut Context<Self>,
    ) {
        let id = element_id.to_string();
        let slot = slot.to_string();
        self.app.update(cx, |model, cx| {
            let canvas = model
                .project
                .as_ref()
                .map(|project| {
                    (
                        project.settings.canvas_size.width as f64,
                        project.settings.canvas_size.height as f64,
                    )
                })
                .unwrap_or((1920.0, 1080.0));
            model.edit(cx, |editor| match slot.as_str() {
                "reveal" => editor.set_text_reveal(&id, preset_id, None),
                "out" => editor.apply_text_animation(
                    &id,
                    crate::text_anim::Direction::Out,
                    preset_id,
                    None,
                    canvas,
                ),
                _ => editor.apply_text_animation(
                    &id,
                    crate::text_anim::Direction::In,
                    preset_id,
                    None,
                    canvas,
                ),
            })
        });
        cx.notify();
    }

    fn animation_sections(
        &mut self,
        element: &TimelineElement,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let id = element.base().id.clone();
        let mut sections = Vec::new();

        for direction in [
            crate::text_anim::Direction::In,
            crate::text_anim::Direction::Out,
        ] {
            let options = crate::text_anim::presets_for(direction)
                .map(|preset| (preset.id, t(preset.name_key)))
                .collect::<Vec<_>>();
            let active = edit::text_animation_preset(element, direction.key());
            let chips = self.animation_chips(direction.key(), &id, options, active, cx);
            sections.push(self.section(
                format!("{id}:animation-{}", direction.key()),
                t(direction.label_key()),
                vec![chips],
                cx,
            ));
        }

        let reveal_options = crate::text_anim::REVEAL_PRESETS
            .iter()
            .map(|preset| (preset.id, t(preset.name_key)))
            .collect::<Vec<_>>();
        let reveal_active = edit::text_animation_preset(element, "reveal");
        let reveal_chips = self.animation_chips("reveal", &id, reveal_options, reveal_active, cx);
        sections.push(self.section(
            format!("{id}:animation-reveal"),
            t("text.animation.reveal"),
            vec![reveal_chips],
            cx,
        ));

        sections
    }

    fn notice_row(&self, text: String, colors: Palette) -> Div {
        div()
            .w_full()
            .text_size(rem(TEXT_XS))
            .text_color(colors.muted_foreground)
            .child(text)
    }

    fn action_row(
        &mut self,
        id: &'static str,
        label: String,
        disabled: bool,
        cx: &mut Context<Self>,
        run: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Div {
        let colors = self.colors(cx);
        let progress = self.transitions.eased(id);
        let hover_key = id.to_string();
        div().flex().w_full().child(
            Button::new(id, colors)
                .label(label)
                .disabled(disabled)
                .hover(progress)
                .build()
                .flex_1()
                .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                    this.transitions.set(hover_key.clone(), *hovered);
                    cx.notify();
                }))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    if !disabled {
                        run(this, cx);
                    }
                })),
        )
    }

    fn toggle_row(
        &mut self,
        id: &'static str,
        label: String,
        active: bool,
        cx: &mut Context<Self>,
        run: impl Fn(&mut Self, bool, &mut Context<Self>) + 'static,
    ) -> Div {
        let colors = self.colors(cx);
        let progress = self.transitions.eased(id);
        let hover_key = id.to_string();
        div().flex().w_full().child(
            div()
                .id(SharedString::from(id))
                .flex()
                .flex_1()
                .h(px(CONTROL_HEIGHT_PX))
                .items_center()
                .justify_center()
                .rounded(rem(RADIUS_SM))
                .cursor_pointer()
                .text_size(rem(TEXT_XS))
                .bg(if active {
                    colors.secondary
                } else {
                    mix(opacity(colors.accent, 0.0), colors.accent, progress)
                })
                .text_color(if active {
                    colors.secondary_foreground
                } else {
                    colors.muted_foreground
                })
                .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                    this.transitions.set(hover_key.clone(), *hovered);
                    cx.notify();
                }))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| run(this, !active, cx)))
                .child(label),
        )
    }

    fn source_of(&self, element: &TimelineElement, cx: &App) -> Option<(PathBuf, bool)> {
        let model = self.app.read(cx);
        let project = model.project.as_ref()?;
        let asset = model.media_by_id(element.media_id()?)?;
        let store = cutix_project::MediaStore::for_project(&model.store, &project.metadata.id);
        let path = store.source_file(asset);
        path.is_file().then(|| {
            (
                path,
                matches!(asset.media_type, cutix_project::MediaType::Video),
            )
        })
    }

    fn cutout_sections(&mut self, element: &TimelineElement, cx: &mut Context<Self>) -> Vec<Div> {
        let colors = self.colors(cx);
        let id = element.base().id.clone();
        let stored = edit::cutout_of(element);
        let source = self.source_of(element, cx);
        let running = self.cutout_job.is_some();

        let mut body = Vec::new();
        body.push(self.notice_row(t("cutout.hint"), colors));
        if let Some(spec) = ml::models::find_model("modnet") {
            body.push(self.notice_row(
                format!("{}: {}", t("cutout.model"), ml::models::describe(spec)),
                colors,
            ));
        }

        if let Some(job) = self.cutout_job.clone() {
            let status = job.status();
            body.push(self.notice_row(status.message, colors));
            body.push(self.slider_row(
                &format!("{id}-cutout-progress"),
                String::new(),
                status.progress,
                cx,
            ));
            body.push(self.action_row(
                "cutout-cancel",
                t("cutout.cancel"),
                false,
                cx,
                |this, cx| {
                    if let Some(job) = this.cutout_job.as_ref() {
                        job.request_cancel();
                    }
                    cx.notify();
                },
            ));
            return vec![self.section(format!("{id}:cutout"), t("cutout.title"), body, cx)];
        }

        if source.is_none() {
            body.push(self.notice_row(t("cutout.noMedia"), colors));
            return vec![self.section(format!("{id}:cutout"), t("cutout.title"), body, cx)];
        }

        let rate = self.cutout_rate;
        let rate_chips = ml::CUTOUT_SAMPLE_RATES
            .iter()
            .map(|value| {
                let value = *value;
                let selected = value == rate;
                let key = format!("cutout-rate-{value}");
                let progress = self.transitions.eased(&key);
                let hover_key = key.clone();
                div()
                    .id(SharedString::from(key))
                    .flex()
                    .flex_1()
                    .h(px(CONTROL_HEIGHT_PX))
                    .items_center()
                    .justify_center()
                    .rounded(rem(RADIUS_SM))
                    .cursor_pointer()
                    .text_size(rem(TEXT_XS))
                    .bg(if selected {
                        colors.secondary
                    } else {
                        mix(opacity(colors.accent, 0.0), colors.accent, progress)
                    })
                    .text_color(if selected {
                        colors.secondary_foreground
                    } else {
                        colors.muted_foreground
                    })
                    .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                        this.transitions.set(hover_key.clone(), *hovered);
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.cutout_rate = value;
                        cx.notify();
                    }))
                    .child(t_args("cutout.rate", &[("rate", &value.to_string())]))
            })
            .collect::<Vec<_>>();

        let label = if stored.is_some() {
            t("cutout.reapply")
        } else {
            t("cutout.remove")
        };
        body.push(
            self.action_row("cutout-run", label, running, cx, |this, cx| {
                this.run_cutout(crate::cutout::CutoutRequestMode::Static, cx);
            }),
        );
        body.push(div().flex().w_full().gap(px(4.0)).children(rate_chips));
        let rate_label = t_args("cutout.perFrame", &[("frames", &rate.to_string())]);
        body.push(
            self.action_row("cutout-per-frame", rate_label, running, cx, |this, cx| {
                let rate = this.cutout_rate;
                this.run_cutout(crate::cutout::CutoutRequestMode::PerFrame { rate }, cx);
            }),
        );

        if let Some(cutout) = stored {
            let element_id = id.clone();
            body.push(self.toggle_row(
                "cutout-enabled",
                t("cutout.enabled"),
                cutout.enabled,
                cx,
                move |this, next, cx| {
                    let element_id = element_id.clone();
                    this.app.update(cx, |model, cx| {
                        model.edit(cx, |editor| {
                            editor.set_cutout_flag(&element_id, "enabled", next)
                        })
                    });
                    cx.notify();
                },
            ));
            let element_id = id.clone();
            body.push(self.toggle_row(
                "cutout-invert",
                t("cutout.invert"),
                cutout.invert,
                cx,
                move |this, next, cx| {
                    let element_id = element_id.clone();
                    this.app.update(cx, |model, cx| {
                        model.edit(cx, |editor| {
                            editor.set_cutout_flag(&element_id, "invert", next)
                        })
                    });
                    cx.notify();
                },
            ));
            let element_id = id.clone();
            body.push(self.action_row(
                "cutout-clear",
                t("cutout.clear"),
                false,
                cx,
                move |this, cx| {
                    let element_id = element_id.clone();
                    this.app.update(cx, |model, cx| {
                        model.edit(cx, |editor| editor.set_cutout(&element_id, None))
                    });
                    this.cutout_notice = None;
                    cx.notify();
                },
            ));

            body.push(self.notice_row(
                if cutout.mode == ml::CutoutMode::PerFrame {
                    t_args(
                        "cutout.perFrameNotice",
                        &[("frames", &cutout.sample_count().to_string())],
                    )
                } else {
                    t("cutout.staticNotice")
                },
                colors,
            ));
        }

        if let Some(notice) = self.cutout_notice.clone() {
            body.push(self.notice_row(notice, colors));
        }

        vec![self.section(format!("{id}:cutout"), t("cutout.title"), body, cx)]
    }

    fn run_cutout(&mut self, mode: crate::cutout::CutoutRequestMode, cx: &mut Context<Self>) {
        if self.cutout_job.is_some() {
            return;
        }
        let Some(element) = self.selected(cx) else {
            return;
        };
        let Some((source, is_video)) = self.source_of(&element, cx) else {
            self.cutout_notice = Some(t("cutout.noMedia"));
            cx.notify();
            return;
        };
        let id = element.base().id.clone();
        let local = self.local_time(&element, cx);
        let base = element.base();
        let request = crate::cutout::CutoutRequest {
            source,
            is_video,
            reference_seconds: (base.trim_start + local).to_seconds_f64(),
            duration_ticks: base.duration.as_ticks(),
            trim_start_ticks: base.trim_start.as_ticks(),
            mode,
            invert: edit::cutout_of(&element)
                .map(|cutout| cutout.invert)
                .unwrap_or(false),
            model_key: String::from("modnet"),
        };

        let job = crate::ai::Job::new(t("cutout.processing"));
        self.cutout_job = Some(job.clone());
        self.cutout_notice = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let worker = job.clone();
            let outcome = cx
                .background_spawn(async move { crate::cutout::compute(&request, &worker) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.cutout_job = None;
                match outcome {
                    Ok(cutout) => {
                        let percent = (cutout.coverage * 100.0).round() as i64;
                        let samples = cutout.sample_count();
                        let per_frame = cutout.mode == ml::CutoutMode::PerFrame;
                        let value = serde_json::to_value(&cutout).ok();
                        this.app.update(cx, |model, cx| {
                            model.edit(cx, |editor| editor.set_cutout(&id, value.clone()))
                        });
                        this.cutout_notice = Some(if per_frame {
                            t_args("cutout.perFrameDone", &[("frames", &samples.to_string())])
                        } else {
                            t_args("cutout.done", &[("percent", &percent.to_string())])
                        });
                    }
                    Err(error) => {
                        this.cutout_notice = Some(if error == t("cutout.cancel") {
                            error
                        } else {
                            format!("{}: {error}", t("cutout.failed"))
                        });
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn tracking_sections(&mut self, element: &TimelineElement, cx: &mut Context<Self>) -> Vec<Div> {
        let colors = self.colors(cx);
        let id = element.base().id.clone();
        let mut body = Vec::new();
        body.push(self.notice_row(t("tracking.hint"), colors));

        if let Some(job) = self.tracking_job.clone() {
            let status = job.status();
            body.push(self.notice_row(status.message, colors));
            body.push(self.slider_row(
                &format!("{id}-tracking-progress"),
                String::new(),
                status.progress,
                cx,
            ));
            body.push(self.action_row(
                "tracking-cancel",
                t("cutout.cancel"),
                false,
                cx,
                |this, cx| {
                    if let Some(job) = this.tracking_job.as_ref() {
                        job.request_cancel();
                    }
                    cx.notify();
                },
            ));
            return vec![self.section(format!("{id}:tracking"), t("tracking.title"), body, cx)];
        }

        if self.source_of(element, cx).is_none() {
            body.push(self.notice_row(t("tracking.noMedia"), colors));
            return vec![self.section(format!("{id}:tracking"), t("tracking.title"), body, cx)];
        }

        let threshold = self.tracking_threshold;
        body.push(self.slider_row(
            &format!("{id}-tracking-threshold"),
            format!(
                "{} - {}%",
                t("tracking.sensitivity"),
                (threshold * 100.0).round() as i64
            ),
            threshold / 0.9,
            cx,
        ));

        let region = self.app.read(cx).tracking_region;
        for (suffix, label_key, value) in [
            ("region-x", "timeline.property.positionX", region.x),
            ("region-y", "timeline.property.positionY", region.y),
            ("region-w", "common.width", region.width),
            ("region-h", "common.height", region.height),
        ] {
            body.push(self.slider_row(
                &format!("{id}-tracking-{suffix}"),
                format!("{}: {:.0}%", t(label_key), value * 100.0),
                value,
                cx,
            ));
        }

        let bindable = self.bindable_elements(element, cx);
        let target = self.tracking_target.clone();
        let target_rows: Vec<gpui::AnyElement> = if bindable.is_empty() {
            vec![div()
                .text_size(rem(TEXT_XS))
                .text_color(colors.muted_foreground)
                .child(t("tracking.targetPlaceholder"))
                .into_any_element()]
        } else {
            bindable
                .iter()
                .map(|(candidate_id, name)| {
                    let selected = target.as_deref() == Some(candidate_id.as_str());
                    let key = format!("tracking-target-{candidate_id}");
                    let progress = self.transitions.eased(&key);
                    let hover_key = key.clone();
                    let pick = candidate_id.clone();
                    div()
                        .id(SharedString::from(key))
                        .flex()
                        .w_full()
                        .h(px(CONTROL_HEIGHT_PX))
                        .items_center()
                        .px(px(8.0))
                        .rounded(rem(RADIUS_SM))
                        .cursor_pointer()
                        .text_size(rem(TEXT_XS))
                        .bg(if selected {
                            colors.secondary
                        } else {
                            mix(opacity(colors.accent, 0.0), colors.accent, progress)
                        })
                        .text_color(if selected {
                            colors.secondary_foreground
                        } else {
                            colors.muted_foreground
                        })
                        .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                            this.transitions.set(hover_key.clone(), *hovered);
                            cx.notify();
                        }))
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            this.tracking_target = Some(pick.clone());
                            cx.notify();
                        }))
                        .child(name.clone())
                        .into_any_element()
                })
                .collect()
        };

        body.push(
            div()
                .flex()
                .w_full()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(rem(TEXT_XS))
                        .text_color(colors.muted_foreground)
                        .child(t("tracking.target")),
                )
                .children(target_rows),
        );

        body.push(
            self.action_row("tracking-run", t("tracking.run"), false, cx, |this, cx| {
                this.run_tracking(cx);
            }),
        );

        if let Some(notice) = self.tracking_notice.clone() {
            body.push(self.notice_row(notice, colors));
        }

        vec![self.section(format!("{id}:tracking"), t("tracking.title"), body, cx)]
    }

    fn bindable_elements(&self, video: &TimelineElement, cx: &App) -> Vec<(String, String)> {
        let base = video.base();
        let start = base.start_time.as_ticks();
        let end = start + base.duration.as_ticks();
        self.app
            .read(cx)
            .tracks()
            .into_iter()
            .flat_map(cutix_project::Track::elements)
            .filter(|candidate| {
                matches!(
                    candidate,
                    TimelineElement::Text(_)
                        | TimelineElement::Sticker(_)
                        | TimelineElement::Image(_)
                        | TimelineElement::Graphic(_)
                )
            })
            .filter(|candidate| {
                let other = candidate.base();
                other.start_time.as_ticks() < end
                    && other.start_time.as_ticks() + other.duration.as_ticks() > start
            })
            .map(|candidate| (candidate.base().id.clone(), candidate.base().name.clone()))
            .collect()
    }

    fn run_tracking(&mut self, cx: &mut Context<Self>) {
        if self.tracking_job.is_some() {
            return;
        }
        let Some(element) = self.selected(cx) else {
            return;
        };
        let Some((source, is_video)) = self.source_of(&element, cx) else {
            self.tracking_notice = Some(t("tracking.noMedia"));
            cx.notify();
            return;
        };
        if !is_video {
            self.tracking_notice = Some(t("tracking.noMedia"));
            cx.notify();
            return;
        }
        let Some(target_id) = self.tracking_target.clone() else {
            self.tracking_notice = Some(t("tracking.noTarget"));
            cx.notify();
            return;
        };

        let anchor = self.local_time(&element, cx);
        let region = self.app.read(cx).tracking_region;
        let threshold = self.tracking_threshold;
        let model = self.app.read(cx);
        let Some(bound) = model
            .tracks()
            .into_iter()
            .flat_map(cutix_project::Track::elements)
            .find(|candidate| candidate.base().id == target_id)
            .cloned()
        else {
            self.tracking_notice = Some(t("tracking.noTarget"));
            cx.notify();
            return;
        };
        let (canvas_width, canvas_height) = model.canvas();
        let fps = model.fps();
        let source_size = element
            .media_id()
            .and_then(|id| model.media_by_id(id))
            .and_then(|asset| Some((asset.width? as f64, asset.height? as f64)))
            .unwrap_or((canvas_width as f64, canvas_height as f64));

        let base = element.base().clone();
        let start_seconds = (base.trim_start + anchor).to_seconds_f64();
        let duration_seconds = (base.duration.as_ticks() - anchor.as_ticks()).max(0) as f64
            / crate::tracking::TICKS_PER_SECOND as f64;
        let transform = edit::transform_of(&element).cloned().unwrap_or_default();
        let bound_base = bound.base().clone();
        let bound_position = edit::transform_of(&bound)
            .map(|transform| (transform.position.x, transform.position.y))
            .unwrap_or((0.0, 0.0));

        let request = crate::tracking::TrackRequest {
            source,
            region,
            start_seconds,
            duration_seconds,
            fps,
            confidence_threshold: threshold,
        };

        let job = crate::ai::Job::new(t("tracking.sampling"));
        self.tracking_job = Some(job.clone());
        self.tracking_notice = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let worker = job.clone();
            let outcome = cx
                .background_spawn(async move { crate::tracking::analyze(&request, &worker) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.tracking_job = None;
                let analysis = match outcome {
                    Ok(analysis) => analysis,
                    Err(error) => {
                        this.tracking_notice = Some(format!("{}: {error}", t("tracking.failed")));
                        cx.notify();
                        return;
                    }
                };

                let keyframes =
                    crate::tracking::build_tracking_keyframes(&crate::tracking::BindRequest {
                        samples: &analysis.samples,
                        transform: &transform,
                        source_width: source_size.0,
                        source_height: source_size.1,
                        canvas_width: canvas_width as f64,
                        canvas_height: canvas_height as f64,
                        video_start_ticks: base.start_time.as_ticks(),
                        video_trim_start_ticks: base.trim_start.as_ticks(),
                        bound_start_ticks: bound_base.start_time.as_ticks(),
                        bound_duration_ticks: bound_base.duration.as_ticks(),
                        bound_position,
                    });

                if keyframes.is_empty() {
                    this.tracking_notice = Some(t("tracking.noOverlap"));
                    cx.notify();
                    return;
                }

                let count = keyframes.len();
                this.app.update(cx, |model, cx| {
                    model.edit(cx, |editor| {
                        editor.bake_tracking_keyframes(&target_id, &keyframes)
                    })
                });

                this.tracking_notice = Some(match analysis.lost_at_seconds {
                    Some(seconds) => {
                        let clip_seconds = seconds - base.trim_start.to_seconds_f64();
                        t_args(
                            "tracking.lost",
                            &[
                                ("seconds", &format!("{clip_seconds:.2}")),
                                ("count", &count.to_string()),
                            ],
                        )
                    }
                    None => t_args(
                        "tracking.done",
                        &[
                            ("count", &count.to_string()),
                            (
                                "confidence",
                                &((analysis.min_confidence * 100.0).round() as i64).to_string(),
                            ),
                        ],
                    ),
                });
                cx.notify();
            });
        })
        .detach();
    }

    fn media_size(&self, element: &TimelineElement, cx: &App) -> (f64, f64) {
        let model = self.app.read(cx);
        let (canvas_width, canvas_height) = model.canvas();
        element
            .media_id()
            .and_then(|id| model.media_by_id(id))
            .and_then(|asset| Some((asset.width? as f64, asset.height? as f64)))
            .unwrap_or((canvas_width as f64, canvas_height as f64))
    }

    fn stabilize_sections(
        &mut self,
        element: &TimelineElement,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let colors = self.colors(cx);
        let id = element.base().id.clone();
        let mut body = Vec::new();
        body.push(self.notice_row(t("stabilize.hint"), colors));

        if let Some(job) = self.stabilize_job.clone() {
            let status = job.status();
            body.push(self.notice_row(status.message, colors));
            body.push(self.slider_row(
                &format!("{id}-stabilize-progress"),
                String::new(),
                status.progress,
                cx,
            ));
            body.push(self.action_row(
                "stabilize-cancel",
                t("cutout.cancel"),
                false,
                cx,
                |this, cx| {
                    if let Some(job) = this.stabilize_job.as_ref() {
                        job.request_cancel();
                    }
                    cx.notify();
                },
            ));
            return vec![self.section(format!("{id}:stabilize"), t("stabilize.title"), body, cx)];
        }

        if !matches!(self.source_of(element, cx), Some((_, true))) {
            body.push(self.notice_row(t("stabilize.noMedia"), colors));
            return vec![self.section(format!("{id}:stabilize"), t("stabilize.title"), body, cx)];
        }

        let strength = self.stabilize_strength;
        body.push(self.slider_row(
            &format!("{id}-stabilize-strength"),
            format!(
                "{} - {}%",
                t("stabilize.strength"),
                (strength * 100.0).round() as i64
            ),
            strength,
            cx,
        ));

        body.push(self.action_row(
            "stabilize-run",
            t("stabilize.run"),
            false,
            cx,
            |this, cx| {
                this.run_stabilize(cx);
            },
        ));

        if let Some(notice) = self.stabilize_notice.clone() {
            body.push(self.notice_row(notice, colors));
        }

        vec![self.section(format!("{id}:stabilize"), t("stabilize.title"), body, cx)]
    }

    fn run_stabilize(&mut self, cx: &mut Context<Self>) {
        if self.stabilize_job.is_some() {
            return;
        }
        let Some(element) = self.selected(cx) else {
            return;
        };
        let Some((source, true)) = self.source_of(&element, cx) else {
            self.stabilize_notice = Some(t("stabilize.noMedia"));
            cx.notify();
            return;
        };

        let base = element.base().clone();
        let model = self.app.read(cx);
        let (canvas_width, canvas_height) = model.canvas();
        let fps = model.fps();
        let (source_width, source_height) = self.media_size(&element, cx);
        let transform = edit::transform_of(&element).cloned().unwrap_or_default();

        let start_seconds = base.trim_start.to_seconds_f64();
        let duration_ticks = base.duration.as_ticks();
        let duration_seconds = duration_ticks as f64 / crate::tracking::TICKS_PER_SECOND as f64;

        let request = crate::stabilize::StabilizeRequest {
            source,
            start_seconds,
            duration_seconds,
            fps,
            strength: self.stabilize_strength,
        };

        let job = crate::ai::Job::new(t("stabilize.sampling"));
        self.stabilize_job = Some(job.clone());
        self.stabilize_notice = None;
        cx.notify();

        let id = base.id.clone();
        cx.spawn(async move |this, cx| {
            let worker = job.clone();
            let outcome = cx
                .background_spawn(async move { crate::stabilize::analyze(&request, &worker) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.stabilize_job = None;
                let analysis = match outcome {
                    Ok(analysis) => analysis,
                    Err(error) => {
                        this.stabilize_notice = Some(format!("{}: {error}", t("stabilize.failed")));
                        cx.notify();
                        return;
                    }
                };

                let zoom = analysis.required_zoom();
                let keyframes =
                    crate::stabilize::build_stabilize_keyframes(&crate::stabilize::BakeRequest {
                        samples: &analysis.samples,
                        sample_size: analysis.sample_size,
                        transform: &transform,
                        zoom,
                        source_width,
                        source_height,
                        canvas_width: canvas_width as f64,
                        canvas_height: canvas_height as f64,
                        trim_start_ticks: base.trim_start.as_ticks(),
                        duration_ticks,
                    });

                if keyframes.is_empty() {
                    this.stabilize_notice = Some(t("stabilize.failed"));
                    cx.notify();
                    return;
                }

                let count = keyframes.len();
                let scale_x = transform.scale_x * zoom;
                let scale_y = transform.scale_y * zoom;
                this.app.update(cx, |model, cx| {
                    model.edit_coalesced(Some(format!("stabilize-{id}")), cx, |editor| {
                        let zoomed = editor.set_property(&id, Field::ScaleX, scale_x)
                            | editor.set_property(&id, Field::ScaleY, scale_y);
                        zoomed | editor.bake_tracking_keyframes(&id, &keyframes)
                    })
                });

                this.stabilize_notice = Some(t_args(
                    "stabilize.done",
                    &[
                        ("count", &count.to_string()),
                        ("zoom", &((zoom * 100.0).round() as i64).to_string()),
                    ],
                ));
                cx.notify();
            });
        })
        .detach();
    }

    fn reframe_sections(&mut self, element: &TimelineElement, cx: &mut Context<Self>) -> Vec<Div> {
        let colors = self.colors(cx);
        let id = element.base().id.clone();
        let mut body = Vec::new();
        body.push(self.notice_row(t("reframe.hint"), colors));

        if let Some(job) = self.reframe_job.clone() {
            let status = job.status();
            body.push(self.notice_row(status.message, colors));
            body.push(self.slider_row(
                &format!("{id}-reframe-progress"),
                String::new(),
                status.progress,
                cx,
            ));
            body.push(self.action_row(
                "reframe-cancel",
                t("cutout.cancel"),
                false,
                cx,
                |this, cx| {
                    if let Some(job) = this.reframe_job.as_ref() {
                        job.request_cancel();
                    }
                    cx.notify();
                },
            ));
            return vec![self.section(format!("{id}:reframe"), t("reframe.title"), body, cx)];
        }

        if !matches!(self.source_of(element, cx), Some((_, true))) {
            body.push(self.notice_row(t("reframe.noMedia"), colors));
            return vec![self.section(format!("{id}:reframe"), t("reframe.title"), body, cx)];
        }

        body.push(
            div()
                .text_size(rem(TEXT_XS))
                .text_color(colors.muted_foreground)
                .child(t("reframe.aspect")),
        );

        let aspect = self.reframe_aspect;
        body.push(
            self.chip_row(
                &format!("{id}-reframe-aspect"),
                crate::reframe::REFRAME_ASPECTS
                    .iter()
                    .map(|(name, _)| ((*name).to_string(), (*name).to_string()))
                    .collect(),
                Some(aspect),
                cx,
                |this, pick, cx| {
                    if let Some((name, _)) = crate::reframe::REFRAME_ASPECTS
                        .iter()
                        .find(|(name, _)| *name == pick)
                    {
                        this.reframe_aspect = name;
                    }
                    cx.notify();
                },
            ),
        );

        let zoom = self.reframe_zoom;
        body.push(self.slider_row(
            &format!("{id}-reframe-zoom"),
            format!("{} - {}%", t("reframe.zoom"), (zoom * 100.0).round() as i64),
            zoom - 1.0,
            cx,
        ));

        body.push(
            self.action_row("reframe-run", t("reframe.run"), false, cx, |this, cx| {
                this.run_reframe(cx);
            }),
        );

        if let Some(notice) = self.reframe_notice.clone() {
            body.push(self.notice_row(notice, colors));
        }

        vec![self.section(format!("{id}:reframe"), t("reframe.title"), body, cx)]
    }

    fn run_reframe(&mut self, cx: &mut Context<Self>) {
        if self.reframe_job.is_some() {
            return;
        }
        let Some(element) = self.selected(cx) else {
            return;
        };
        let Some((source, is_video)) = self.source_of(&element, cx) else {
            self.reframe_notice = Some(t("reframe.noMedia"));
            cx.notify();
            return;
        };

        let base = element.base().clone();
        let start_seconds = base.trim_start.to_seconds_f64();
        let duration_ticks = base.duration.as_ticks();
        let duration_seconds = duration_ticks as f64 / crate::tracking::TICKS_PER_SECOND as f64;

        let request = crate::reframe::ReframeRequest {
            source,
            is_video,
            start_seconds,
            duration_seconds,
            target_aspect: crate::reframe::aspect_value(self.reframe_aspect),
            zoom: self.reframe_zoom,
            model_key: String::from("modnet"),
        };

        let job = crate::ai::Job::new(t_args(
            "reframe.sampling",
            &[("completed", "0"), ("total", "0")],
        ));
        self.reframe_job = Some(job.clone());
        self.reframe_notice = None;
        cx.notify();

        let id = base.id.clone();
        cx.spawn(async move |this, cx| {
            let worker = job.clone();
            let outcome = cx
                .background_spawn(async move { crate::reframe::analyze(&request, &worker) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.reframe_job = None;
                let analysis = match outcome {
                    Ok(analysis) => analysis,
                    Err(error) => {
                        this.reframe_notice = Some(format!("{}: {error}", t("reframe.failed")));
                        cx.notify();
                        return;
                    }
                };

                let crops = crate::reframe::build_reframe_keyframes(&crate::reframe::BakeRequest {
                    analysis: &analysis,
                    start_seconds,
                    duration_ticks,
                });
                if crops.is_empty() {
                    this.reframe_notice = Some(t("reframe.failed"));
                    cx.notify();
                    return;
                }

                let count = crops.len();
                this.app.update(cx, |model, cx| {
                    model.edit_coalesced(Some(format!("reframe-{id}")), cx, |editor| {
                        let mut changed = false;
                        for crop in &crops {
                            let local = MediaTime::from_ticks(crop.time);
                            for (field, value) in [
                                (Field::CropLeft, crop.left),
                                (Field::CropTop, crop.top),
                                (Field::CropRight, crop.right),
                                (Field::CropBottom, crop.bottom),
                            ] {
                                changed |= editor.set_keyframe(&id, field, local, value);
                            }
                        }
                        changed
                    })
                });

                this.reframe_notice =
                    Some(t_args("reframe.done", &[("count", &count.to_string())]));
                cx.notify();
            });
        })
        .detach();
    }

    fn motion_sections(&mut self, element: &TimelineElement, cx: &mut Context<Self>) -> Vec<Div> {
        if !matches!(
            element,
            TimelineElement::Video(_) | TimelineElement::Image(_) | TimelineElement::Sticker(_)
        ) {
            return Vec::new();
        }

        let colors = self.colors(cx);
        let id = element.base().id.clone();
        let current = edit::motion_of(element);
        let active = current.map(|motion| motion.preset_id.clone());
        let mut body = Vec::new();

        body.push(
            div()
                .text_size(rem(TEXT_XS))
                .text_color(colors.muted_foreground)
                .child(t("motion.preset")),
        );

        let mut options: Vec<(String, String)> = vec![(String::new(), t("common.none"))];
        options.extend(
            crate::motion::MOTION_PRESETS
                .iter()
                .map(|preset| (preset.id.to_string(), t(preset.name_key))),
        );

        for (row, chunk) in options.chunks(3).enumerate() {
            body.push(self.chip_row(
                &format!("{id}-motion-{row}"),
                chunk.to_vec(),
                active.as_deref().or(Some("")),
                cx,
                |this, pick, cx| {
                    this.apply_motion(&pick, cx);
                },
            ));
        }

        if active.is_some() {
            let intensity = current.map_or(self.motion_intensity, |motion| motion.intensity);
            body.push(self.slider_row(
                &format!("{id}-motion-intensity"),
                format!(
                    "{} - {}%",
                    t("motion.intensity"),
                    (intensity * 100.0).round() as i64
                ),
                ((intensity - crate::motion::MOTION_MIN_INTENSITY)
                    / (crate::motion::MOTION_MAX_INTENSITY - crate::motion::MOTION_MIN_INTENSITY))
                    as f32,
                cx,
            ));
        }

        vec![self.section(format!("{id}:motion"), t("motion.title"), body, cx)]
    }

    fn motion_clear_times(element: &TimelineElement) -> Vec<i64> {
        let Some(motion) = edit::motion_of(element) else {
            return Vec::new();
        };
        let Some(preset) = crate::motion::find_preset(&motion.preset_id) else {
            return Vec::new();
        };
        let window = motion.duration.as_ticks() as f64;
        preset
            .spec
            .offsets
            .iter()
            .map(|offset| (offset * window).round() as i64)
            .collect()
    }

    fn apply_motion(&mut self, preset_id: &str, cx: &mut Context<Self>) {
        let Some(element) = self.selected(cx) else {
            return;
        };
        let id = element.base().id.clone();
        let clear_times = Self::motion_clear_times(&element);

        if preset_id.is_empty() {
            self.app.update(cx, |model, cx| {
                model.edit(cx, |editor| {
                    editor.bake_motion(&id, &clear_times, &[], None)
                })
            });
            cx.notify();
            return;
        }

        let base = element.base().clone();
        let model = self.app.read(cx);
        let (canvas_width, canvas_height) = model.canvas();
        let (source_width, source_height) = self.media_size(&element, cx);
        let transform = edit::transform_of(&element).cloned().unwrap_or_default();

        let Some(patch) = crate::motion::build_motion_patch(&crate::motion::PatchRequest {
            preset_id,
            transform: &transform,
            element_duration: base.duration.as_ticks(),
            canvas_width: canvas_width as f64,
            canvas_height: canvas_height as f64,
            source_width,
            source_height,
            duration: None,
            intensity: Some(self.motion_intensity),
        }) else {
            return;
        };

        self.app.update(cx, |model, cx| {
            model.edit_coalesced(Some(format!("motion-{id}")), cx, |editor| {
                editor.bake_motion(
                    &id,
                    &clear_times,
                    &patch.keyframes,
                    Some(patch.settings.clone()),
                )
            })
        });
        cx.notify();
    }

    fn mask_sections(&mut self, element: &TimelineElement, cx: &mut Context<Self>) -> Vec<Div> {
        let colors = self.colors(cx);
        let id = element.base().id.clone();
        let current = edit::mask_of(element);
        let active = current.map(|mask| mask.mask_type.clone());
        let feather = current
            .and_then(|mask| mask.params.get("feather"))
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        let inverted = current
            .and_then(|mask| mask.params.get("inverted"))
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        let chips = masks::ALL_SHAPES
            .iter()
            .map(|shape| {
                let value = shape.key();
                let selected = active.as_deref() == Some(value);
                let key = format!("mask-shape-{value}");
                let progress = self.transitions.eased(&key);
                let hover_key = key.clone();
                let element_id = id.clone();
                div()
                    .id(SharedString::from(key))
                    .flex()
                    .h(px(CONTROL_HEIGHT_PX))
                    .items_center()
                    .justify_center()
                    .px(px(10.0))
                    .rounded(rem(RADIUS_SM))
                    .cursor_pointer()
                    .text_size(rem(TEXT_XS))
                    .bg(if selected {
                        colors.secondary
                    } else {
                        mix(opacity(colors.accent, 0.0), colors.accent, progress)
                    })
                    .text_color(if selected {
                        colors.secondary_foreground
                    } else {
                        colors.muted_foreground
                    })
                    .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                        this.transitions.set(hover_key.clone(), *hovered);
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        let next = if selected { None } else { Some(value) };
                        let element_id = element_id.clone();
                        this.app.update(cx, |model, cx| {
                            model.edit(cx, |editor| editor.set_mask_shape(&element_id, next))
                        });
                        cx.notify();
                    }))
                    .child(t(shape.name_key()))
            })
            .collect::<Vec<_>>();

        let mut body = vec![div()
            .flex()
            .w_full()
            .flex_wrap()
            .gap(px(4.0))
            .children(chips)];

        if active.is_some() {
            body.push(self.slider_row(
                &format!("{id}-mask-feather"),
                format!("{}: {feather:.0}", t("properties.mask.feather")),
                (feather / MASK_FEATHER_MAX_PX) as f32,
                cx,
            ));

            let stroke_width = current
                .and_then(|mask| mask.params.get("strokeWidth"))
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0);
            let stroke_color = current
                .and_then(|mask| mask.params.get("strokeColor"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("#ffffff")
                .to_owned();
            let stroke_align = current
                .and_then(|mask| mask.params.get("strokeAlign"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("center")
                .to_owned();

            body.push(self.slider_row(
                &format!("{id}-mask-stroke-width"),
                format!("{}: {stroke_width:.0}", t("masks.param.strokeWidth")),
                (stroke_width / MASK_STROKE_MAX_PX) as f32,
                cx,
            ));

            if stroke_width > 0.0 {
                let swatches = MASK_STROKE_COLORS
                    .iter()
                    .map(|value| {
                        let selected = stroke_color.eq_ignore_ascii_case(value);
                        let element_id = id.clone();
                        div()
                            .id(SharedString::from(format!("mask-stroke-color-{value}")))
                            .size(px(CONTROL_HEIGHT_PX - 6.0))
                            .rounded(rem(RADIUS_SM))
                            .cursor_pointer()
                            .border_2()
                            .border_color(if selected {
                                colors.primary
                            } else {
                                colors.border
                            })
                            .bg(crate::theme::parse_hex(value.trim_start_matches('#')))
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                let element_id = element_id.clone();
                                this.app.update(cx, |model, cx| {
                                    model.edit(cx, |editor| {
                                        editor.set_mask_param(
                                            &element_id,
                                            "strokeColor",
                                            serde_json::json!(value),
                                        )
                                    })
                                });
                                cx.notify();
                            }))
                    })
                    .collect::<Vec<_>>();
                body.push(
                    div()
                        .flex()
                        .w_full()
                        .flex_wrap()
                        .gap(px(4.0))
                        .children(swatches),
                );

                let aligns = [
                    ("inside", "graphics.strokeAlign.inside"),
                    ("center", "graphics.strokeAlign.center"),
                    ("outside", "graphics.strokeAlign.outside"),
                ]
                .into_iter()
                .map(|(value, label_key)| {
                    let selected = stroke_align == value;
                    let element_id = id.clone();
                    div()
                        .id(SharedString::from(format!("mask-stroke-align-{value}")))
                        .flex()
                        .flex_1()
                        .h(px(CONTROL_HEIGHT_PX))
                        .items_center()
                        .justify_center()
                        .rounded(rem(RADIUS_SM))
                        .cursor_pointer()
                        .text_size(rem(TEXT_XS))
                        .bg(if selected {
                            colors.secondary
                        } else {
                            colors.accent
                        })
                        .text_color(if selected {
                            colors.secondary_foreground
                        } else {
                            colors.muted_foreground
                        })
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            let element_id = element_id.clone();
                            this.app.update(cx, |model, cx| {
                                model.edit(cx, |editor| {
                                    editor.set_mask_param(
                                        &element_id,
                                        "strokeAlign",
                                        serde_json::json!(value),
                                    )
                                })
                            });
                            cx.notify();
                        }))
                        .child(t(label_key))
                })
                .collect::<Vec<_>>();
                body.push(div().flex().w_full().gap(px(4.0)).children(aligns));
            }

            let shape_label = active
                .as_deref()
                .and_then(masks::MaskShape::from_key)
                .map(|shape| t(shape.name_key()))
                .unwrap_or_default();
            let element_id = id.clone();
            body.push(
                div().flex().w_full().gap(px(4.0)).child(
                    div()
                        .id("mask-invert")
                        .flex()
                        .flex_1()
                        .h(px(CONTROL_HEIGHT_PX))
                        .items_center()
                        .justify_center()
                        .rounded(rem(RADIUS_SM))
                        .cursor_pointer()
                        .text_size(rem(TEXT_XS))
                        .bg(if inverted {
                            colors.secondary
                        } else {
                            colors.accent
                        })
                        .text_color(if inverted {
                            colors.secondary_foreground
                        } else {
                            colors.muted_foreground
                        })
                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                            let element_id = element_id.clone();
                            this.app.update(cx, |model, cx| {
                                model.edit(cx, |editor| {
                                    editor.set_mask_param(
                                        &element_id,
                                        "inverted",
                                        serde_json::json!(!inverted),
                                    )
                                })
                            });
                            cx.notify();
                        }))
                        .child(t_args(
                            "properties.mask.invert.aria",
                            &[("name", &shape_label)],
                        )),
                ),
            );
        } else {
            body.push(
                div()
                    .w_full()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(t("properties.mask.empty.hint")),
            );
        }

        vec![self.section(format!("{id}:mask"), t("properties.masks"), body, cx)]
    }

    fn blending_sections(
        &mut self,
        element: &TimelineElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let id = element.base().id.clone();
        let opacity_row = self.number_row(
            element,
            Field::Opacity,
            t("properties.opacity"),
            NumberIcon::Glyph("checkerboard"),
            100.0,
            0,
            true,
            window,
            cx,
        );
        let mode = edit::blend_mode_of(element).to_string();
        let label = BLEND_MODES
            .iter()
            .find(|(value, _)| *value == mode)
            .map(|(_, key)| t(key))
            .unwrap_or_else(|| t("properties.blendMode.placeholder"));
        let select = self.select_row(
            "blend-mode",
            t("properties.blendMode"),
            label,
            "rain-drop",
            MenuKind::BlendMode,
            cx,
        );

        vec![self.section(
            format!("{id}:blending"),
            t("properties.blending"),
            vec![row2(opacity_row, select)],
            cx,
        )]
    }

    fn audio_sections(
        &mut self,
        element: &TimelineElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let colors = self.colors(cx);
        let id = element.base().id.clone();
        let volume = self.number_row(
            element,
            Field::Volume,
            t("properties.volume"),
            NumberIcon::Glyph("volume-high"),
            1.0,
            1,
            true,
            window,
            cx,
        );
        let volume_section = self.section(
            format!("{id}:audio"),
            t("properties.audio"),
            vec![volume],
            cx,
        );

        let (fade_in, fade_out) = edit::read_audio_fade(element);
        let max = fade_max(element.base().duration);
        let in_seconds = fade_in.to_seconds_f64();
        let out_seconds = fade_out.to_seconds_f64();
        let fade_in_row = self.slider_row(
            &format!("{id}-fade-in"),
            format!("{}: {:.1}", t("audioFade.in"), in_seconds),
            (in_seconds / max) as f32,
            cx,
        );
        let fade_out_row = self.slider_row(
            &format!("{id}-fade-out"),
            format!("{}: {:.1}", t("audioFade.out"), out_seconds),
            (out_seconds / max) as f32,
            cx,
        );
        let reset_id = id.clone();
        let reset = div().flex().w_full().child(
            Button::new("fade-reset", colors)
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Sm)
                .label(t("audioFade.clear"))
                .build()
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    let id = reset_id.clone();
                    this.app.update(cx, |model, cx| {
                        model.edit(cx, |editor| {
                            editor.set_audio_fade(&id, MediaTime::ZERO, MediaTime::ZERO)
                        })
                    });
                    cx.notify();
                })),
        );

        let fade_section = self.section(
            format!("{id}:fade"),
            t("audioFade.title"),
            vec![fade_in_row, fade_out_row, reset],
            cx,
        );

        let mut sections = vec![volume_section, fade_section];
        sections.extend(self.audio_effect_sections(element, cx));
        sections.extend(self.audio_enhance_sections(element, cx));
        sections.push(self.audio_silence_section(element, cx));
        sections
    }

    fn chip_row(
        &mut self,
        prefix: &str,
        options: Vec<(String, String)>,
        selected: Option<&str>,
        cx: &mut Context<Self>,
        run: impl Fn(&mut Self, String, &mut Context<Self>) + Clone + 'static,
    ) -> Div {
        let colors = self.colors(cx);
        let chips = options
            .into_iter()
            .map(|(value, label)| {
                let key = format!("{prefix}-{value}");
                let progress = self.transitions.eased(&key);
                let hover_key = key.clone();
                let active = selected == Some(value.as_str());
                let run = run.clone();
                div()
                    .id(SharedString::from(key))
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .h(px(CONTROL_HEIGHT_PX))
                    .items_center()
                    .justify_center()
                    .rounded(rem(RADIUS_SM))
                    .cursor_pointer()
                    .text_size(rem(TEXT_XS))
                    .bg(if active {
                        colors.secondary
                    } else {
                        mix(opacity(colors.accent, 0.0), colors.accent, progress)
                    })
                    .text_color(if active {
                        colors.secondary_foreground
                    } else {
                        colors.muted_foreground
                    })
                    .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                        this.transitions.set(hover_key.clone(), *hovered);
                        cx.notify();
                    }))
                    .on_click(
                        cx.listener(move |this: &mut Self, _, _, cx| run(this, value.clone(), cx)),
                    )
                    .child(label)
            })
            .collect::<Vec<_>>();
        div().flex().w_full().gap(px(4.0)).children(chips)
    }

    fn audio_effect_sections(
        &mut self,
        element: &TimelineElement,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let colors = self.colors(cx);
        let id = element.base().id.clone();
        let busy = self.audio_job.is_some();

        let mut equalizer = Vec::new();
        for (index, frequency) in dsp::EQUALIZER_BAND_FREQUENCIES.iter().enumerate() {
            let gain = self.eq_gains.get(index).copied().unwrap_or(0.0);
            let label = format!(
                "{}: {}{:.1} dB",
                t_args(
                    "audioEffects.equalizer.band",
                    &[("frequency", &format!("{}Hz", *frequency as i64))],
                ),
                if gain > 0.0 { "+" } else { "" },
                gain,
            );
            let fraction =
                (gain + dsp::EQUALIZER_GAIN_LIMIT_DB) / (2.0 * dsp::EQUALIZER_GAIN_LIMIT_DB);
            equalizer.push(self.slider_row(&format!("{id}-eq-{index}"), label, fraction, cx));
        }
        equalizer.push(self.action_row(
            "audio-eq-apply",
            self.apply_label(),
            busy,
            cx,
            |this, cx| {
                let effect = crate::audio_fx::Effect::Equalizer {
                    gains_db: this.eq_gains.clone(),
                };
                this.bake_audio(effect, t("audioEffects.equalizer.suffix"), cx);
            },
        ));
        equalizer.push(self.action_row(
            "audio-eq-reset",
            t("audioEffects.equalizer.reset"),
            busy,
            cx,
            |this, cx| {
                this.eq_gains = vec![0.0; dsp::EQUALIZER_BAND_FREQUENCIES.len()];
                cx.notify();
            },
        ));
        let equalizer_section = self.section(
            format!("{id}:equalizer"),
            t("audioEffects.equalizer.title"),
            equalizer,
            cx,
        );

        let semitones = self.pitch_semitones;
        let formant = self.pitch_formant;
        let pitch_row = self.slider_row(
            &format!("{id}-pitch-semitones"),
            format!(
                "{}: {}{:.1}",
                t("audioEffects.pitch.semitones"),
                if semitones > 0.0 { "+" } else { "" },
                semitones
            ),
            (semitones + 12.0) / 24.0,
            cx,
        );
        let formant_row = self.slider_row(
            &format!("{id}-pitch-formant"),
            format!(
                "{}: {}{:.1}",
                t("audioEffects.pitch.formant"),
                if formant > 0.0 { "+" } else { "" },
                formant
            ),
            (formant + 12.0) / 24.0,
            cx,
        );
        let pitch_apply = self.action_row(
            "audio-pitch-apply",
            self.apply_label(),
            busy,
            cx,
            |this, cx| {
                let effect = crate::audio_fx::Effect::Pitch {
                    semitones: this.pitch_semitones,
                    formant: this.pitch_formant,
                };
                this.bake_audio(effect, t("audioEffects.pitch.suffix"), cx);
            },
        );
        let pitch_hint = self.notice_row(t("audioEffects.pitch.hint"), colors);
        let pitch_section = self.section(
            format!("{id}:voice-changer"),
            t("audioEffects.pitch.title"),
            vec![pitch_row, formant_row, pitch_apply, pitch_hint],
            cx,
        );

        let preset = crate::audio_fx::REVERB_PRESET_IDS[self.reverb_preset.min(2)];
        let preset_chips = self.chip_row(
            "reverb-preset",
            crate::audio_fx::REVERB_PRESET_IDS
                .iter()
                .map(|key| {
                    (
                        (*key).to_string(),
                        t(&format!("audioEffects.reverb.preset.{key}")),
                    )
                })
                .collect(),
            Some(preset),
            cx,
            |this, value, cx| {
                this.reverb_preset = crate::audio_fx::REVERB_PRESET_IDS
                    .iter()
                    .position(|key| *key == value)
                    .unwrap_or(0);
                cx.notify();
            },
        );
        let wet = self.reverb_wet;
        let wet_row = self.slider_row(
            &format!("{id}-reverb-wet"),
            format!(
                "{}: {}%",
                t("audioEffects.reverb.wet"),
                (wet * 100.0).round() as i64
            ),
            wet,
            cx,
        );
        let reverb_apply = self.action_row(
            "audio-reverb-apply",
            self.apply_label(),
            busy,
            cx,
            |this, cx| {
                let preset =
                    crate::audio_fx::REVERB_PRESET_IDS[this.reverb_preset.min(2)].to_string();
                let label = t(&format!("audioEffects.reverb.preset.{preset}"));
                let effect = crate::audio_fx::Effect::Reverb {
                    preset,
                    wet: this.reverb_wet,
                };
                this.bake_audio(effect, label, cx);
            },
        );
        let reverb_hint = self.notice_row(t("audioEffects.reverb.hint"), colors);
        let reverb_section = self.section(
            format!("{id}:reverb"),
            t("audioEffects.reverb.title"),
            vec![preset_chips, wet_row, reverb_apply, reverb_hint],
            cx,
        );

        let voice_chips = self.chip_row(
            "voice-preset",
            dsp::VOICE_PRESETS
                .iter()
                .map(|preset| {
                    (
                        preset.id().to_string(),
                        t(&format!("audioEffects.voice.preset.{}", preset.id())),
                    )
                })
                .collect(),
            None,
            cx,
            |this, value, cx| {
                if this.audio_job.is_some() {
                    return;
                }
                let label = t(&format!("audioEffects.voice.preset.{value}"));
                this.bake_audio(crate::audio_fx::Effect::Voice { preset: value }, label, cx);
            },
        );
        let mut voice_body = vec![voice_chips];
        if let Some(notice) = self.audio_notice.clone() {
            voice_body.push(self.notice_row(notice, colors));
        }
        if let Some(job) = self.audio_job.clone() {
            let status = job.status();
            voice_body.push(self.notice_row(status.message, colors));
            voice_body.push(self.slider_row(
                &format!("{id}-audio-progress"),
                String::new(),
                status.progress,
                cx,
            ));
        }
        let voice_section = self.section(
            format!("{id}:voice-presets"),
            t("audioEffects.voice.title"),
            voice_body,
            cx,
        );

        vec![
            equalizer_section,
            pitch_section,
            reverb_section,
            voice_section,
        ]
    }

    fn apply_label(&self) -> String {
        if self.audio_job.is_some() {
            t("audioEnhance.processing")
        } else {
            t("audioEffects.apply")
        }
    }

    fn audio_enhance_sections(
        &mut self,
        element: &TimelineElement,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let colors = self.colors(cx);
        let id = element.base().id.clone();
        let busy = self.audio_job.is_some();

        let sensitivity = self.beat_sensitivity;
        let mut beats_body = vec![self.slider_row(
            &format!("{id}-beat-sensitivity"),
            format!(
                "{}: {:.2}",
                t("audioEnhance.beats.sensitivity"),
                sensitivity
            ),
            (sensitivity - 0.5) / 2.5,
            cx,
        )];
        beats_body.push(self.action_row(
            "audio-beats-detect",
            if busy {
                t("audioEnhance.analyzing")
            } else {
                t("audioEnhance.beats.detect")
            },
            busy,
            cx,
            |this, cx| this.detect_beats(cx),
        ));
        if let Some(beats) = self.beats.as_ref() {
            let bpm = beats
                .bpm
                .map(|value| format!("{value:.1}"))
                .unwrap_or_else(|| String::from("\u{2014}"));
            let count = beats.times.len().to_string();
            let empty = beats.times.is_empty();
            let result = t_args(
                "audioEnhance.beats.result",
                &[("bpm", &bpm), ("count", &count)],
            );
            beats_body.push(self.notice_row(result, colors));
            beats_body.push(self.action_row(
                "audio-beats-markers",
                t("audioEnhance.beats.addMarkers"),
                empty,
                cx,
                |this, cx| this.add_beat_markers(cx),
            ));
        }
        let beats_section = self.section(
            format!("{id}:beats"),
            t("audioEnhance.beats.title"),
            beats_body,
            cx,
        );

        let strength = self.denoise_strength;
        let strength_row = self.slider_row(
            &format!("{id}-denoise-strength"),
            format!("{}: {:.2}", t("audioEnhance.denoise.strength"), strength),
            strength,
            cx,
        );
        let denoise_run = self.action_row(
            "audio-denoise-run",
            if busy {
                t("audioEnhance.processing")
            } else {
                t("audioEnhance.denoise.run")
            },
            busy,
            cx,
            |this, cx| {
                let effect = crate::audio_fx::Effect::Denoise {
                    strength: this.denoise_strength,
                };
                this.bake_audio(effect, t("audioEnhance.denoise.title"), cx);
            },
        );
        let denoise_hint = self.notice_row(t("audioEnhance.denoise.hint"), colors);
        let denoise_section = self.section(
            format!("{id}:denoise"),
            t("audioEnhance.denoise.title"),
            vec![strength_row, denoise_run, denoise_hint],
            cx,
        );

        let candidates = self.voice_candidates(&id, cx);
        let selected = self.duck_voice.clone();
        let empty = candidates.is_empty();
        let mut ducking_body = Vec::new();
        if empty {
            ducking_body.push(self.notice_row(t("audioEnhance.ducking.voicePlaceholder"), colors));
        } else {
            ducking_body.push(self.chip_row(
                "duck-voice",
                candidates,
                selected.as_deref(),
                cx,
                |this, value, cx| {
                    this.duck_voice = Some(value);
                    cx.notify();
                },
            ));
        }
        ducking_body.push(self.action_row(
            "audio-ducking-apply",
            if busy {
                t("audioEnhance.processing")
            } else {
                t("audioEnhance.ducking.apply")
            },
            busy || empty,
            cx,
            |this, cx| this.apply_ducking(cx),
        ));
        ducking_body.push(self.notice_row(t("audioEnhance.ducking.hint"), colors));
        let ducking_section = self.section(
            format!("{id}:ducking"),
            t("audioEnhance.ducking.title"),
            ducking_body,
            cx,
        );

        vec![beats_section, denoise_section, ducking_section]
    }

    fn audio_silence_section(&mut self, element: &TimelineElement, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        let id = element.base().id.clone();
        let busy = self.audio_job.is_some();
        let threshold = self.silence_threshold_db;
        let minimum = self.silence_min_seconds;

        let threshold_row = self.slider_row(
            &format!("{id}-silence-threshold"),
            format!("{}: {:.0} dB", t("audioSilence.threshold"), threshold),
            (threshold + 70.0) / 60.0,
            cx,
        );
        let minimum_row = self.slider_row(
            &format!("{id}-silence-minimum"),
            format!(
                "{}: {:.2}{}",
                t("audioSilence.minDuration"),
                minimum,
                t("common.secondsShort")
            ),
            (minimum - 0.1) / 2.9,
            cx,
        );
        let detect = self.action_row(
            "audio-silence-detect",
            if busy {
                t("audioEnhance.analyzing")
            } else {
                t("audioSilence.detect")
            },
            busy,
            cx,
            |this, cx| this.detect_silence(cx),
        );
        let mut body = vec![threshold_row, minimum_row, detect];

        if let Some((owner, ranges)) = self.silence_ranges.clone() {
            if owner == id {
                let removed: i64 = ranges.iter().map(|(from, to)| to - from).sum();
                let seconds = format!(
                    "{:.2}",
                    removed as f64 / crate::audio_fx::TICKS_PER_SECOND as f64
                );
                let result = t_args(
                    "audioSilence.result",
                    &[("count", &ranges.len().to_string()), ("seconds", &seconds)],
                );
                body.push(self.notice_row(result, colors));
                body.push(self.action_row(
                    "audio-silence-apply",
                    t("audioSilence.apply"),
                    ranges.is_empty(),
                    cx,
                    |this, cx| this.apply_silence(cx),
                ));
            }
        }
        body.push(self.notice_row(t("audioSilence.hint"), colors));

        self.section(format!("{id}:silence"), t("audioSilence.title"), body, cx)
    }

    fn voice_candidates(&self, exclude: &str, cx: &App) -> Vec<(String, String)> {
        let Some(scene) = self.app.read(cx).current_scene() else {
            return Vec::new();
        };
        scene
            .tracks
            .all()
            .flat_map(|track| track.elements())
            .filter(|element| {
                matches!(
                    element,
                    TimelineElement::Audio(_) | TimelineElement::Video(_)
                )
            })
            .filter(|element| element.base().id != exclude)
            .map(|element| (element.base().id.clone(), element.base().name.clone()))
            .collect()
    }

    fn clip_window(&self, element: &TimelineElement) -> crate::audio_fx::ClipWindow {
        let base = element.base();
        crate::audio_fx::ClipWindow {
            start_time: base.start_time.as_ticks(),
            duration: base.duration.as_ticks(),
            trim_start: base.trim_start.as_ticks(),
            retime: edit::retime_of(element).cloned(),
        }
    }

    fn bake_audio(
        &mut self,
        effect: crate::audio_fx::Effect,
        suffix: String,
        cx: &mut Context<Self>,
    ) {
        if self.audio_job.is_some() {
            return;
        }
        if effect.is_neutral() {
            self.audio_notice = Some(t("audioEffects.nothingToApply"));
            cx.notify();
            return;
        }
        let Some(element) = self.selected(cx) else {
            return;
        };
        let Some((source, _)) = self.source_of(&element, cx) else {
            self.audio_notice = Some(t("audioEnhance.noSource"));
            cx.notify();
            return;
        };
        let name = format!("{} ({suffix})", element.base().name);

        let job = crate::ai::Job::new(t("audioEnhance.processing"));
        self.audio_job = Some(job.clone());
        self.audio_notice = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let worker = job.clone();
            let baked = cx
                .background_spawn(
                    async move { crate::audio_fx::bake(&source, &effect, &name, &worker) },
                )
                .await;
            let _ = this.update(cx, |this, cx| {
                this.audio_job = None;
                match baked {
                    Ok((path, extra_seconds)) => {
                        this.app
                            .update(cx, |model, cx| model.import_media(vec![path], cx));
                        this.audio_notice = Some(if extra_seconds > 0.0 {
                            format!(
                                "{} \u{00b7} {}",
                                t("audioEffects.addedToLibrary"),
                                t_args(
                                    "audioEffects.tailExtended",
                                    &[("seconds", &format!("{extra_seconds:.2}"))],
                                )
                            )
                        } else {
                            t("audioEffects.addedToLibrary")
                        });
                    }
                    Err(error) => {
                        this.audio_notice = Some(format!("{}: {error}", t("audioEffects.failed")));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn analyse<T: Send + 'static>(
        &mut self,
        message: String,
        run: impl FnOnce(Vec<f32>, u32) -> Result<T, String> + Send + 'static,
        done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.audio_job.is_some() {
            return;
        }
        let Some(element) = self.selected(cx) else {
            return;
        };
        let Some((source, _)) = self.source_of(&element, cx) else {
            self.audio_notice = Some(t("audioEnhance.noSource"));
            cx.notify();
            return;
        };

        let job = crate::ai::Job::new(message);
        self.audio_job = Some(job);
        self.audio_notice = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn(async move {
                    let decoded = crate::audio_fx::decode(&source)?;
                    let rate = decoded.sample_rate;
                    run(decoded.mono(), rate)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.audio_job = None;
                match outcome {
                    Ok(value) => done(this, value, cx),
                    Err(error) => this.audio_notice = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn detect_beats(&mut self, cx: &mut Context<Self>) {
        let sensitivity = self.beat_sensitivity;
        self.analyse(
            t("audioEnhance.analyzing"),
            move |samples, rate| Ok(crate::audio_fx::detect_beats(&samples, rate, sensitivity)),
            |this, beats, _| {
                this.audio_notice = Some(if beats.times.is_empty() {
                    t("audioEnhance.beats.none")
                } else {
                    t_args(
                        "audioEnhance.beats.found",
                        &[("count", &beats.times.len().to_string())],
                    )
                });
                this.beats = Some(beats);
            },
            cx,
        );
    }

    fn add_beat_markers(&mut self, cx: &mut Context<Self>) {
        let Some(element) = self.selected(cx) else {
            return;
        };
        let Some(beats) = self.beats.as_ref() else {
            return;
        };
        let window = self.clip_window(&element);
        let times = crate::audio_fx::beat_marker_ticks(&beats.times, &window);
        if times.is_empty() {
            self.audio_notice = Some(t_args("audioEnhance.beats.markersAdded", &[("count", "0")]));
            cx.notify();
            return;
        }
        let count = times.len();
        self.app.update(cx, |model, cx| {
            model.edit(cx, |editor| {
                let mut changed = false;
                for time in &times {
                    changed |= editor.toggle_bookmark(MediaTime::from_ticks(*time));
                }
                changed
            })
        });
        self.audio_notice = Some(t_args(
            "audioEnhance.beats.markersAdded",
            &[("count", &count.to_string())],
        ));
        cx.notify();
    }

    fn apply_ducking(&mut self, cx: &mut Context<Self>) {
        let Some(element) = self.selected(cx) else {
            return;
        };
        let Some(voice_id) = self.duck_voice.clone() else {
            self.audio_notice = Some(t("audioEnhance.ducking.pickVoice"));
            cx.notify();
            return;
        };
        let Some(voice) = self
            .app
            .read(cx)
            .current_scene()
            .and_then(|scene| {
                scene
                    .tracks
                    .all()
                    .flat_map(|track| track.elements())
                    .find(|candidate| candidate.base().id == voice_id)
            })
            .cloned()
        else {
            self.audio_notice = Some(t("audioEnhance.ducking.pickVoice"));
            cx.notify();
            return;
        };
        let Some((source, _)) = self.source_of(&voice, cx) else {
            self.audio_notice = Some(t("audioEnhance.noSource"));
            cx.notify();
            return;
        };

        let voice_window = self.clip_window(&voice);
        let music_window = self.clip_window(&element);
        let music_id = element.base().id.clone();
        let base_volume = edit::field_value(&element, Field::Volume);

        let job = crate::ai::Job::new(t("audioEnhance.processing"));
        self.audio_job = Some(job);
        self.audio_notice = None;
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn(async move {
                    let decoded = crate::audio_fx::decode(&source)?;
                    let points =
                        crate::audio_fx::ducking_points(&decoded.mono(), decoded.sample_rate);
                    Ok::<_, String>(crate::audio_fx::ducking_keyframes(
                        &points,
                        &voice_window,
                        &music_window,
                        base_volume,
                    ))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.audio_job = None;
                match outcome {
                    Ok(keyframes) if keyframes.is_empty() => {
                        this.audio_notice = Some(t("audioEnhance.ducking.noOverlap"));
                    }
                    Ok(keyframes) => {
                        let count = keyframes.len();
                        this.app.update(cx, |model, cx| {
                            model.edit_coalesced(Some(String::from("ducking")), cx, |editor| {
                                let mut changed = false;
                                for (local, value) in &keyframes {
                                    changed |= editor.set_keyframe(
                                        &music_id,
                                        Field::Volume,
                                        *local,
                                        *value,
                                    );
                                }
                                changed
                            })
                        });
                        this.audio_notice = Some(t_args(
                            "audioEnhance.ducking.applied",
                            &[("count", &count.to_string())],
                        ));
                    }
                    Err(error) => {
                        this.audio_notice =
                            Some(format!("{}: {error}", t("audioEnhance.ducking.failed")));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn detect_silence(&mut self, cx: &mut Context<Self>) {
        let Some(element) = self.selected(cx) else {
            return;
        };
        let id = element.base().id.clone();
        let window = self.clip_window(&element);
        let threshold = self.silence_threshold_db;
        let minimum = self.silence_min_seconds;
        self.analyse(
            t("audioEnhance.analyzing"),
            move |samples, rate| {
                Ok(crate::audio_fx::silent_ranges(
                    &samples, rate, threshold, minimum, &window,
                ))
            },
            move |this, ranges, _| {
                this.audio_notice = None;
                this.silence_ranges = Some((id, ranges));
            },
            cx,
        );
    }

    fn apply_silence(&mut self, cx: &mut Context<Self>) {
        let Some((owner, ranges)) = self.silence_ranges.clone() else {
            return;
        };
        if ranges.is_empty() {
            return;
        }
        let Some(track_id) = self.app.read(cx).track_of(&owner) else {
            return;
        };
        let Some(mut tracks) = self
            .app
            .read(cx)
            .current_scene()
            .map(|scene| scene.tracks.clone())
        else {
            return;
        };
        if !crate::audio_fx::remove_silent_ranges(&mut tracks, &track_id, &owner, &ranges) {
            return;
        }
        let count = ranges.len();
        self.app.update(cx, |model, cx| {
            model.edit(cx, |editor| editor.replace_tracks(tracks.clone()))
        });
        self.silence_ranges = None;
        self.audio_notice = Some(t_args(
            "audioSilence.applied",
            &[("count", &count.to_string())],
        ));
        cx.notify();
    }

    fn speed_sections(
        &mut self,
        element: &TimelineElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let id = element.base().id.clone();
        let speed = self.number_row(
            element,
            Field::SpeedRate,
            t("properties.speed"),
            NumberIcon::Glyph("dashboard-speed"),
            1.0,
            2,
            false,
            window,
            cx,
        );
        let retime = edit::retime_of(element);
        let maintain_pitch = retime
            .and_then(|retime| retime.maintain_pitch)
            .unwrap_or(false);
        let blend_frames = retime
            .and_then(|retime| retime.blend_frames)
            .unwrap_or(false);
        let pitch = self.switch_row(
            &format!("{id}-pitch"),
            t("properties.changePitch"),
            !maintain_pitch,
            Setting::MaintainPitch(!maintain_pitch),
            &id,
            cx,
        );
        let blend = self.switch_row(
            &format!("{id}-blend-frames"),
            t("speed.blendFrames"),
            blend_frames,
            Setting::BlendFrames(!blend_frames),
            &id,
            cx,
        );

        vec![self.section(
            format!("{id}:speed"),
            t("speed.title"),
            vec![speed, pitch, blend],
            cx,
        )]
    }

    fn text_sections(
        &mut self,
        element: &TimelineElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let colors = self.colors(cx);
        let id = element.base().id.clone();
        let Some(text) = edit::text_of(element).cloned() else {
            return Vec::new();
        };

        let content_key = format!("{id}-content");
        let editing_content = self
            .editing
            .as_ref()
            .is_some_and(|editing| editing.key == content_key);
        let content_element = id.clone();
        let content_value = text.content.clone();
        let content_control = if editing_content {
            let field_ref = self.editing.as_ref().expect("editing field");
            div()
                .w_full()
                .child(inline_editor(field_ref, colors, window, cx))
                .into_any_element()
        } else {
            div()
                .id(SharedString::from(format!("content-{id}")))
                .flex()
                .w_full()
                .min_h(px(56.0))
                .p(px(8.0))
                .rounded(rem(RADIUS_MD))
                .border_1()
                .border_color(colors.border)
                .bg(colors.accent)
                .cursor_text()
                .text_size(rem(TEXT_SM))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.editing = Some(Editing {
                        key: content_key.clone(),
                        element: content_element.clone(),
                        target: Target::Text(TextSetting::Content),
                        field: TextField::new(cx, content_value.clone()),
                    });
                    cx.notify();
                }))
                .child(text.content.clone())
                .into_any_element()
        };
        let content_section = self.section(
            format!("{id}:content"),
            t("properties.content"),
            vec![div().w_full().child(content_control)],
            cx,
        );

        let presets = crate::text::presets()
            .into_iter()
            .map(|preset| {
                let element_id = id.clone();
                let preset_id = preset.id;
                let swatch = crate::theme::parse_hex(preset.color.trim_start_matches('#'));
                div()
                    .id(SharedString::from(format!("preset-{preset_id}")))
                    .flex()
                    .w(px(62.0))
                    .h(px(46.0))
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .rounded(rem(RADIUS_SM))
                    .border_1()
                    .border_color(colors.border)
                    .bg(opacity(colors.foreground, 0.06))
                    .cursor_pointer()
                    .text_size(rem(TEXT_SM))
                    .text_color(swatch)
                    .when(preset.bold, |this| this.font_weight(FontWeight::BOLD))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        let id = element_id.clone();
                        this.apply_preset(&id, preset_id, cx);
                    }))
                    .child(t("text.sample"))
            })
            .collect::<Vec<_>>();
        let presets_section = self.section(
            format!("{id}:style"),
            t("text.presets"),
            vec![div()
                .flex()
                .flex_wrap()
                .w_full()
                .gap(px(6.0))
                .children(presets)],
            cx,
        );

        let font = self.select_row(
            "font-family",
            t("properties.font"),
            text.font_family.clone(),
            "text-font",
            MenuKind::FontFamily,
            cx,
        );
        let font_size = self.number_row(
            element,
            Field::FontSize,
            t("properties.fontSize"),
            NumberIcon::Glyph("text-font"),
            1.0,
            0,
            false,
            window,
            cx,
        );
        let text_color = self.resolved_color(element, "color", &text.color, cx);
        let color = self.color_row(
            element,
            t("common.color"),
            TextSetting::TextColor,
            text_color,
            Some("color"),
            window,
            cx,
        );
        let align_label = self.field_label(t("properties.textAlign"), None, cx);
        let align = self.button_group(
            "text-align",
            vec![
                ("left", ButtonFace::Glyph("align-left"), String::new()),
                ("center", ButtonFace::Glyph("align-center"), String::new()),
                ("right", ButtonFace::Glyph("align-right"), String::new()),
            ],
            text.text_align.clone(),
            &id,
            Setting::TextAlign,
            cx,
        );
        let weight_label = self.field_label(t("properties.fontWeight"), None, cx);
        let weight = self.button_group(
            "font-weight",
            vec![
                ("normal", ButtonFace::Label, t("properties.weight.normal")),
                ("bold", ButtonFace::Label, t("properties.weight.bold")),
            ],
            text.font_weight.clone(),
            &id,
            Setting::FontWeight,
            cx,
        );
        let typography_section = self.section(
            format!("{id}:typography"),
            t("properties.typography"),
            vec![
                font,
                font_size,
                color,
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap(px(8.0))
                    .child(align_label)
                    .child(align),
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap(px(8.0))
                    .child(weight_label)
                    .child(weight),
            ],
            cx,
        );

        let letter_spacing = self.number_row(
            element,
            Field::LetterSpacing,
            t("properties.letterSpacing"),
            NumberIcon::Glyph("text-width"),
            1.0,
            0,
            false,
            window,
            cx,
        );
        let line_height = self.number_row(
            element,
            Field::LineHeight,
            t("properties.lineHeight"),
            NumberIcon::Glyph("text-height"),
            1.0,
            1,
            false,
            window,
            cx,
        );
        let spacing_section = self.section(
            format!("{id}:spacing"),
            t("properties.spacing"),
            vec![row2(letter_spacing, line_height)],
            cx,
        );

        let stroke_enabled = edit::nested_flag(&text.stroke, "enabled");
        let stroke_switch = self.switch_row(
            &format!("{id}-stroke"),
            t("properties.enabled"),
            stroke_enabled,
            Setting::StrokeEnabled(!stroke_enabled),
            &id,
            cx,
        );
        let stroke_color = self.color_row(
            element,
            t("common.color"),
            TextSetting::StrokeColor,
            edit::nested_color(&text.stroke, "color", "#000000"),
            None,
            window,
            cx,
        );
        let stroke_width = self.number_row(
            element,
            Field::StrokeWidth,
            t("common.width"),
            NumberIcon::Text("W"),
            1.0,
            1,
            false,
            window,
            cx,
        );
        let stroke_section = self.section(
            format!("{id}:stroke"),
            t("properties.stroke"),
            vec![stroke_switch, row2(stroke_color, stroke_width)],
            cx,
        );

        let shadow_enabled = edit::nested_flag(&text.shadow, "enabled");
        let shadow_switch = self.switch_row(
            &format!("{id}-shadow"),
            t("properties.enabled"),
            shadow_enabled,
            Setting::ShadowEnabled(!shadow_enabled),
            &id,
            cx,
        );
        let shadow_color = self.color_row(
            element,
            t("common.color"),
            TextSetting::ShadowColor,
            edit::nested_color(&text.shadow, "color", "#000000"),
            None,
            window,
            cx,
        );
        let shadow_blur = self.number_row(
            element,
            Field::ShadowBlur,
            t("properties.blur"),
            NumberIcon::Glyph("rain-drop"),
            1.0,
            1,
            false,
            window,
            cx,
        );
        let shadow_x = self.number_row(
            element,
            Field::ShadowOffsetX,
            t("properties.offsetX"),
            NumberIcon::Text("X"),
            1.0,
            1,
            false,
            window,
            cx,
        );
        let shadow_y = self.number_row(
            element,
            Field::ShadowOffsetY,
            t("properties.offsetY"),
            NumberIcon::Text("Y"),
            1.0,
            1,
            false,
            window,
            cx,
        );
        let shadow_section = self.section(
            format!("{id}:shadow"),
            t("properties.shadow"),
            vec![
                shadow_switch,
                row2(shadow_color, shadow_blur),
                row2(shadow_x, shadow_y),
            ],
            cx,
        );

        let background_enabled = text.background.enabled;
        let background_switch = self.switch_row(
            &format!("{id}-background"),
            t("properties.enabled"),
            background_enabled,
            Setting::BackgroundEnabled(!background_enabled),
            &id,
            cx,
        );
        let background_hex =
            self.resolved_color(element, "background.color", &text.background.color, cx);
        let background_color = self.color_row(
            element,
            t("common.color"),
            TextSetting::BackgroundColor,
            background_hex,
            Some("background.color"),
            window,
            cx,
        );
        let padding_x = self.number_row(
            element,
            Field::BackgroundPaddingX,
            t("common.width"),
            NumberIcon::Text("W"),
            1.0,
            0,
            true,
            window,
            cx,
        );
        let padding_y = self.number_row(
            element,
            Field::BackgroundPaddingY,
            t("common.height"),
            NumberIcon::Text("H"),
            1.0,
            0,
            true,
            window,
            cx,
        );
        let offset_x = self.number_row(
            element,
            Field::BackgroundOffsetX,
            t("properties.offsetX"),
            NumberIcon::Text("X"),
            1.0,
            0,
            true,
            window,
            cx,
        );
        let offset_y = self.number_row(
            element,
            Field::BackgroundOffsetY,
            t("properties.offsetY"),
            NumberIcon::Text("Y"),
            1.0,
            0,
            true,
            window,
            cx,
        );
        let corner = self.number_row(
            element,
            Field::BackgroundCornerRadius,
            t("properties.cornerRadius"),
            NumberIcon::Glyph("crop"),
            1.0,
            0,
            true,
            window,
            cx,
        );
        let background_section = self.section(
            format!("{id}:background"),
            t("properties.background"),
            vec![
                background_switch,
                background_color,
                row2(padding_x, padding_y),
                row2(offset_x, offset_y),
                corner,
            ],
            cx,
        );

        vec![
            content_section,
            presets_section,
            typography_section,
            spacing_section,
            stroke_section,
            shadow_section,
            background_section,
        ]
    }

    fn resolved_color(
        &self,
        element: &TimelineElement,
        path: &str,
        base: &str,
        cx: &App,
    ) -> String {
        if !self.within_range(element, cx) {
            return base.to_string();
        }
        let local = self.local_time(element, cx);
        let rgba = color_at(element.base().animations.as_ref(), path, base, local);
        cutix_project::color::format_srgb_hex(rgba)
    }

    fn apply_preset(&mut self, element_id: &str, preset_id: &str, cx: &mut Context<Self>) {
        let presets = crate::text::presets();
        let Some(preset) = presets.iter().find(|preset| preset.id == preset_id) else {
            return;
        };
        let patch = crate::text::patch_for(preset);
        let id = element_id.to_string();
        self.app.update(cx, |model, cx| {
            model.edit(cx, |editor| editor.apply_text_preset(&id, patch))
        });
        cx.notify();
    }

    fn tab_rail(&mut self, element: &TimelineElement, cx: &mut Context<Self>) -> Div {
        let colors = self.colors(cx);
        let type_key = element_type_key(element).to_string();
        let active = self.active_tab(element, cx);

        let buttons = tabs_for(element)
            .into_iter()
            .map(|(id, glyph, _)| {
                let selected = id == active;
                let key = format!("tab-{id}");
                let progress = self.transitions.eased(&key);
                let hover_key = key.clone();
                let type_key = type_key.clone();
                Button::new(SharedString::from(key), colors)
                    .variant(if selected {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Ghost
                    })
                    .size(ButtonSize::Icon)
                    .hover(progress)
                    .build()
                    .size(px(TAB_RAIL_BUTTON_PX))
                    .child(
                        svg()
                            .size(px(16.0))
                            .path(icon(glyph))
                            .text_color(if selected {
                                colors.secondary_foreground
                            } else {
                                colors.muted_foreground
                            }),
                    )
                    .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                        this.transitions.set(hover_key.clone(), *hovered);
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        let type_key = type_key.clone();
                        this.app.update(cx, |model, _| {
                            model.properties_tabs.insert(type_key, id.to_string());
                        });
                        cx.notify();
                    }))
            })
            .collect::<Vec<_>>();

        div().flex().flex_shrink_0().h_full().child(
            div()
                .id("properties-tab-rail")
                .flex()
                .flex_col()
                .flex_shrink_0()
                .h_full()
                .gap(px(2.0))
                .p(px(4.0))
                .border_r_1()
                .border_color(colors.border)
                .overflow_y_scroll()
                .track_scroll(&self.rail_scroll)
                .children(buttons),
        )
    }

    fn active_tab(&self, element: &TimelineElement, cx: &App) -> String {
        let tabs = tabs_for(element);
        self.app
            .read(cx)
            .properties_tabs
            .get(element_type_key(element))
            .cloned()
            .filter(|id| tabs.iter().any(|tab| tab.0 == id))
            .unwrap_or_else(|| {
                tabs.first()
                    .map(|tab| tab.0.to_string())
                    .unwrap_or_default()
            })
    }
}

impl Render for PropertiesPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        if self.transitions.animating() || self.menu.animating() {
            window.request_animation_frame();
        }

        let Some(element) = self.selected(cx) else {
            let empty = self.empty_state(colors);
            return panel_frame(colors)
                .items_center()
                .justify_center()
                .child(empty);
        };

        let active = self.active_tab(&element, cx);
        let sections = match active.as_str() {
            "transform" => self.transform_sections(&element, window, cx),
            "crop" => self.crop_sections(&element, window, cx),
            "blending" => self.blending_sections(&element, window, cx),
            "audio" => self.audio_sections(&element, window, cx),
            "speed" => self.speed_sections(&element, window, cx),
            "text" => self.text_sections(&element, window, cx),
            "animation" => self.animation_sections(&element, cx),
            "mask" => self.mask_sections(&element, cx),
            "cutout" => self.cutout_sections(&element, cx),
            "tracking" => self.tracking_sections(&element, cx),
            "stabilize" => self.stabilize_sections(&element, cx),
            "reframe" => self.reframe_sections(&element, cx),
            "graphic" => self.graphic_sections(&element, window, cx),
            "transition" => self.transition_sections(&element, cx),
            "effects" => self.effects_sections(&element, window, cx),
            _ => Vec::new(),
        };
        let rail = self.tab_rail(&element, cx);
        let bar = scrollbar_v(&self.scroll, colors);

        panel_frame(colors)
            .on_drag_move::<ScrubDrag>(cx.listener(Self::on_scrub))
            .on_drag_move::<SliderDrag>(cx.listener(Self::on_slider))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this: &mut Self, _, _, cx| this.end_gesture(cx)),
            )
            .child(rail)
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .child(
                        div()
                            .id("properties-body")
                            .flex()
                            .flex_col()
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .children(sections),
                    )
                    .children(bar),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_blend_mode_label_is_translated() {
        for (value, key) in BLEND_MODES {
            assert_ne!(t(key), *key, "{value}");
        }
    }

    #[test]
    fn a_square_crop_of_a_wide_source_insets_the_sides() {
        let crop = centered_aspect_crop(16.0 / 9.0, 1.0);
        assert!((crop.left - crop.right).abs() < 1e-9);
        assert!(crop.left > 0.0);
        assert_eq!(crop.top, 0.0);
    }

    #[test]
    fn a_tall_target_insets_top_and_bottom() {
        let crop = centered_aspect_crop(1.0, 16.0 / 9.0);
        assert!(crop.top > 0.0);
        assert_eq!(crop.left, 0.0);
    }

    #[test]
    fn text_elements_open_on_their_own_tab() {
        assert_eq!(
            tabs_for(&sample_text()).first().map(|tab| tab.0),
            Some("text")
        );
    }

    fn sample_text() -> TimelineElement {
        edit::text_element(
            String::from("Text"),
            String::from("Sample"),
            crate::text::patch_for(&crate::text::presets()[0]),
        )
    }
}
