use cutix_i18n::t;
use gpui::{
    div, prelude::*, px, relative, svg, App, Context, DragMoveEvent, Entity, KeyDownEvent,
    SharedString, Window,
};

use crate::assets::icon;
use crate::components::{
    menu_item, menu_natural_height, menu_surface, overlay_backdrop, overlay_layer, place_anchored,
    tooltipped, Button, ButtonSize, ButtonVariant, MENU_MIN_WIDTH_PX, MENU_OFFSET_PX,
};
use crate::input::{text_field, FieldStyle, TextEvent, TextField};
use crate::interaction::{mix, Overlay, OverlaySide, Tooltips, Transitions, TOOLTIP_DELAY};
use crate::keybindings::{Action, Chord};
use crate::panels::{AssetsPanel, PreviewPanel, TimelinePanel};
use crate::projects::ProjectsView;
use crate::properties::PropertiesPanel;
use crate::shortcuts::ShortcutsState;
use crate::state::{AppModel, Route};
use crate::theme::{
    opacity, rem, COLUMN_GAP, HEADER_HEIGHT, PANEL_MAIN_CONTENT_FRACTION, PANEL_PREVIEW_FRACTION,
    PANEL_PROPERTIES_FRACTION, PANEL_TOOLS_FRACTION, RADIUS_SM, ROW_GAP, TEXT_PROJECT_NAME,
};
use crate::titlebar::Titlebar;

const HEADER_ICON_BUTTON_PX: f32 = 32.0;
const HEADER_ICON_GLYPH_PX: f32 = 17.6;
const LANGUAGE_MENU_ITEM_HEIGHT_PX: f32 = crate::components::MENU_ITEM_HEIGHT_PX;
const LANGUAGE_MENU_WIDTH_PX: f32 = 176.0;
const PROJECT_NAME_FIELD_WIDTH_PX: f32 = 240.0;

const GEOMETRY_WRITE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(400);

const HANDLE_THICKNESS: f32 = 6.0;
const MIN_COLUMN: f32 = 0.15;

pub const TOOLS_MIN_WIDTH_PX: f32 = 240.0;
pub const PROPERTIES_MIN_WIDTH_PX: f32 = 260.0;

pub const PREVIEW_MIN_WIDTH_PX: f32 = 320.0;
const MIN_ROW: f32 = 0.30;
const MAX_ROW: f32 = 0.85;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Split {
    Tools,
    Properties,
}

pub struct Layout {
    pub tools: f32,
    pub preview: f32,
    pub properties: f32,
    pub main_content: f32,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            tools: PANEL_TOOLS_FRACTION,
            preview: PANEL_PREVIEW_FRACTION,
            properties: PANEL_PROPERTIES_FRACTION,
            main_content: PANEL_MAIN_CONTENT_FRACTION,
        }
    }
}

impl Layout {
    pub fn drag_tools(&mut self, fraction: f32) {
        let total = self.tools + self.preview;
        let tools = fraction.clamp(MIN_COLUMN, total - MIN_COLUMN);
        self.preview = total - tools;
        self.tools = tools;
    }

    pub fn drag_properties(&mut self, fraction: f32) {
        let total = self.preview + self.properties;
        let preview = (fraction - self.tools).clamp(MIN_COLUMN, total - MIN_COLUMN);
        self.properties = total - preview;
        self.preview = preview;
    }

    pub fn drag_main_content(&mut self, fraction: f32) {
        self.main_content = fraction.clamp(MIN_ROW, MAX_ROW);
    }
}

pub struct Shell {
    pub app: Entity<AppModel>,
    font: SharedString,
    layout: Layout,
    transitions: Transitions,
    language_menu: Overlay,
    tooltips: Tooltips,
    name_edit: Option<TextField>,
    shortcuts: ShortcutsState,
    route_shown: Option<Route>,
    focus: gpui::FocusHandle,
    titlebar: Entity<Titlebar>,
    home: Entity<crate::home::HomeView>,
    projects: Entity<ProjectsView>,
    library: Entity<crate::library_ui::LibraryView>,
    assets: Entity<AssetsPanel>,
    preview: Entity<PreviewPanel>,
    properties: Entity<PropertiesPanel>,
    timeline: Entity<TimelinePanel>,
    geometry: Option<crate::state::WindowGeometry>,
    geometry_saved: Option<crate::state::WindowGeometry>,
    geometry_written: std::time::Instant,

    pub publish_only: bool,
    publish_size: Option<(f32, f32)>,
}

impl Shell {
    pub fn new(app: Entity<AppModel>, font: SharedString, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();

        Self {
            font,
            publish_only: false,
            publish_size: None,
            layout: Layout::default(),
            transitions: Transitions::new(),
            language_menu: Overlay::new(OverlaySide::Bottom),
            tooltips: Tooltips::new(TOOLTIP_DELAY),
            name_edit: None,
            shortcuts: ShortcutsState::default(),
            route_shown: None,
            focus: cx.focus_handle(),
            titlebar: cx.new(|cx| Titlebar::new(app.clone(), cx)),
            home: cx.new(|cx| crate::home::HomeView::new(app.clone(), cx)),
            projects: cx.new(|cx| ProjectsView::new(app.clone(), cx)),
            library: cx.new(|cx| crate::library_ui::LibraryView::new(app.clone(), cx)),
            assets: cx.new(|cx| AssetsPanel::new(app.clone(), cx)),
            preview: cx.new(|cx| PreviewPanel::new(app.clone(), cx)),
            properties: cx.new(|cx| PropertiesPanel::new(app.clone(), cx)),
            timeline: cx.new(|cx| TimelinePanel::new(app.clone(), cx)),
            geometry: None,
            geometry_saved: None,
            geometry_written: std::time::Instant::now() - GEOMETRY_WRITE_INTERVAL,
            app,
        }
    }

    fn persist_geometry(&mut self, window: &Window) {
        let maximized = window.is_maximized();
        let bounds = match window.window_bounds() {
            gpui::WindowBounds::Windowed(bounds) => bounds,
            gpui::WindowBounds::Maximized(bounds) | gpui::WindowBounds::Fullscreen(bounds) => {
                bounds
            }
        };
        let current = crate::state::WindowGeometry {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
            maximized,
        };
        if !current.is_sane() {
            return;
        }
        self.geometry = Some(current);
        if self.geometry_saved == Some(current)
            || self.geometry_written.elapsed() < GEOMETRY_WRITE_INTERVAL
        {
            return;
        }
        self.geometry_written = std::time::Instant::now();
        self.geometry_saved = Some(current);
        crate::state::save_window_geometry(current);
    }

    pub fn choose_export_destination(&mut self, cx: &mut Context<Self>) {
        let current = self
            .app
            .read(cx)
            .export
            .destination
            .clone()
            .unwrap_or_else(|| {
                crate::export::default_destination(&self.app.read(cx).project_name())
            });
        let directory = current
            .parent()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(crate::export::export_directory);
        let file_name = current
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("export.mp4")
            .to_owned();
        cx.spawn(async move |this, cx| {
            let Some(path) = crate::dialogs::save_file(
                crate::dialogs::Filter::Video,
                t("dialog.export.title"),
                directory,
                file_name,
            )
            .await
            else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                this.app.update(cx, |model, cx| {
                    model.export.destination = Some(path);
                    cx.notify();
                });
            });
        })
        .detach();
    }

    pub fn flush_geometry(&self) {
        if let Some(geometry) = self.geometry {
            if self.geometry_saved != Some(geometry) {
                crate::state::save_window_geometry(geometry);
            }
        }
    }

    fn on_column_drag(
        &mut self,
        event: &DragMoveEvent<Split>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = event.bounds;
        let width = bounds.right() - bounds.left();
        if width <= px(0.0) {
            return;
        }
        let fraction = (event.event.position.x - bounds.left()) / width;

        match event.drag(cx) {
            Split::Tools => self.layout.drag_tools(fraction),
            Split::Properties => self.layout.drag_properties(fraction),
        }
        cx.notify();
    }

    fn on_row_drag(
        &mut self,
        event: &DragMoveEvent<RowSplit>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = event.bounds;
        let height = bounds.bottom() - bounds.top();
        if height <= px(0.0) {
            return;
        }
        self.layout
            .drag_main_content((event.event.position.y - bounds.top()) / height);
        cx.notify();
    }

    fn run_chord(&mut self, chord: &Chord, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(action) = self.app.read(cx).keybindings.action_for(chord) else {
            return false;
        };
        self.run_action(action, window, cx)
    }

    pub fn run_action(
        &mut self,
        action: Action,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match action {
            Action::ToggleSnapping => {
                self.timeline
                    .update(cx, |timeline, cx| timeline.toggle_snapping(cx));
                return true;
            }
            Action::ToggleRippleEditing => {
                self.timeline
                    .update(cx, |timeline, cx| timeline.toggle_ripple(cx));
                return true;
            }
            Action::CancelInteraction => {
                self.cancel_interaction(window, cx);
                return true;
            }
            Action::PreviewToggleFullscreen => {
                window.toggle_fullscreen();
                return true;
            }
            Action::PreviewVolumeUp | Action::PreviewVolumeDown => {
                let step = if matches!(action, Action::PreviewVolumeUp) {
                    crate::panels::PREVIEW_VOLUME_STEP
                } else {
                    -crate::panels::PREVIEW_VOLUME_STEP
                };
                self.app.update(cx, |model, cx| {
                    let level = if model.preview.is_muted() {
                        0.0
                    } else {
                        model.preview.volume()
                    };
                    model.preview.set_volume((level + step).clamp(0.0, 1.0));
                    crate::state::save_preview_audio(
                        model.preview.volume(),
                        model.preview.is_muted(),
                    );
                    cx.notify();
                });
                return true;
            }
            Action::PreviewToggleMute => {
                self.app.update(cx, |model, cx| {
                    model.preview.toggle_mute();
                    crate::state::save_preview_audio(
                        model.preview.volume(),
                        model.preview.is_muted(),
                    );
                    cx.notify();
                });
                return true;
            }
            _ => {}
        }

        let handled = self.app.update(cx, |model, cx| {
            let playhead = model.playhead;
            let total = model.total_duration();
            let rate = model.frame_rate();

            let seek_by = |model: &mut AppModel, cx: &mut Context<AppModel>, seconds: f64| {
                let target = if seconds >= 0.0 {
                    (playhead + crate::edit::seconds(seconds)).min(total)
                } else {
                    playhead - crate::edit::seconds(-seconds)
                };
                model.seek(target.max(time::MediaTime::ZERO), cx);
            };
            let step_frame = |model: &mut AppModel, cx: &mut Context<AppModel>, delta: i64| {
                let Some(frame) = playhead.to_frame_round(rate) else {
                    return;
                };
                let Some(target) = time::MediaTime::from_frame((frame + delta).max(0), rate) else {
                    return;
                };
                model.seek(target.min(total), cx);
            };

            match action {
                Action::TogglePlay => model.toggle_playback(cx),
                Action::StopPlayback => {
                    if model.preview.is_playing() {
                        model.preview.pause();
                    }
                    model.seek(time::MediaTime::ZERO, cx);
                }
                Action::SeekForward => seek_by(model, cx, 1.0),
                Action::SeekBackward => seek_by(model, cx, -1.0),
                Action::JumpForward => seek_by(model, cx, 5.0),
                Action::JumpBackward => seek_by(model, cx, -5.0),
                Action::FrameStepForward => step_frame(model, cx, 1),
                Action::FrameStepBackward => step_frame(model, cx, -1),
                Action::GotoStart => model.seek(time::MediaTime::ZERO, cx),
                Action::GotoEnd => model.seek(total, cx),
                Action::Split => {
                    model.edit(cx, |editor| editor.split_at(playhead));
                }
                Action::SplitLeft => {
                    model.edit(cx, |editor| {
                        editor.split_retaining(playhead, crate::edit::Retain::Right)
                    });
                }
                Action::SplitRight => {
                    model.edit(cx, |editor| {
                        editor.split_retaining(playhead, crate::edit::Retain::Left)
                    });
                }
                Action::DeleteSelected => {
                    model.edit(cx, |editor| editor.delete_selected());
                }
                Action::DuplicateSelected => {
                    model.edit(cx, |editor| editor.duplicate_selected());
                }
                Action::CopySelected => {
                    let selected = model.selected_elements();
                    if selected.is_empty() {
                        return false;
                    }
                    model.clipboard = selected;
                }
                Action::PasteCopied => {
                    let clipboard = model.clipboard.clone();
                    if clipboard.is_empty() {
                        return false;
                    }
                    model.edit(cx, |editor| editor.paste_elements(clipboard, playhead));
                }
                Action::SelectAll => model.selection = model.all_element_ids(),
                Action::DeselectAll => model.selection.clear(),
                Action::Undo => model.undo(cx),
                Action::Redo => model.redo(cx),
                Action::ToggleBookmark => {
                    model.edit(cx, |editor| editor.toggle_bookmark(playhead));
                }
                Action::ToggleSourceAudio => {
                    let Some((element_id, has_audio)) = model.source_audio_target() else {
                        return false;
                    };
                    model.edit(cx, |editor| {
                        editor.toggle_source_audio(&element_id, has_audio)
                    });
                }
                Action::ToggleElementsMuted => {
                    let selection = model.selection.clone();
                    if selection.is_empty() {
                        return false;
                    }
                    model.edit(cx, |editor| editor.toggle_elements_muted(&selection));
                }
                Action::ToggleElementsVisibility => {
                    let selection = model.selection.clone();
                    if selection.is_empty() {
                        return false;
                    }
                    model.edit(cx, |editor| editor.toggle_elements_hidden(&selection));
                }
                Action::RemoveMediaAsset | Action::RemoveMediaAssets => return false,
                Action::PreviewVolumeUp
                | Action::PreviewVolumeDown
                | Action::PreviewToggleMute
                | Action::PreviewToggleFullscreen => return false,
                Action::ToggleSnapping
                | Action::ToggleRippleEditing
                | Action::CancelInteraction => return false,
            }
            cx.notify();
            true
        });

        if handled {
            cx.notify();
        }
        handled
    }

    fn cancel_interaction(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.language_menu.dismiss();
        self.tooltips.dismiss();
        self.name_edit = None;
        if self.shortcuts.recording.is_some() {
            self.shortcuts.recording = None;
            self.shortcuts.notice = None;
        } else if self.shortcuts.open {
            self.shortcuts.close();
        }
        self.preview
            .update(cx, |preview, cx| preview.dismiss_overlays(cx));
        self.timeline
            .update(cx, |timeline, cx| timeline.cancel_overlays(cx));
        self.app.update(cx, |model, cx| {
            if !model.selection.is_empty() {
                model.selection.clear();
                cx.notify();
            }
        });
        window.focus(&self.focus);
        cx.notify();
    }

    pub fn open_shortcuts(&mut self, cx: &mut Context<Self>) {
        self.shortcuts.toggle();
        self.tooltips.dismiss();
        cx.notify();
    }

    pub fn close_shortcuts(&mut self, cx: &mut Context<Self>) {
        self.shortcuts.close();
        cx.notify();
    }

    pub fn record_shortcut(&mut self, action: Action, cx: &mut Context<Self>) {
        self.shortcuts.recording = Some(action);
        self.shortcuts.notice = None;
        cx.notify();
    }

    pub fn reset_keybindings(&mut self, cx: &mut Context<Self>) {
        self.shortcuts.recording = None;
        self.shortcuts.notice = None;
        self.app.update(cx, |model, cx| {
            model.keybindings.reset();
            crate::keybindings::save(&model.keybindings);
            cx.notify();
        });
        cx.notify();
    }

    fn capture_rebind(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(action) = self.shortcuts.recording else {
            return;
        };
        if event.keystroke.key == "escape" {
            self.shortcuts.recording = None;
            self.shortcuts.notice = None;
            cx.notify();
            return;
        }
        let Some(chord) = Chord::from_keystroke(&event.keystroke) else {
            return;
        };

        let conflict = self.app.read(cx).keybindings.conflict(&chord, action);
        self.shortcuts.recording = None;
        match conflict {
            Some(conflict) => {
                self.shortcuts.notice = Some(SharedString::from(
                    crate::shortcuts::conflict_message(&chord, conflict.existing),
                ));
            }
            None => {
                self.shortcuts.notice = None;
                self.app.update(cx, |model, cx| {
                    model.keybindings.rebind(action, chord);
                    crate::keybindings::save(&model.keybindings);
                    cx.notify();
                });
            }
        }
        cx.notify();
    }

    pub fn focus(&mut self, window: &mut Window, _cx: &mut Context<Self>) {
        window.focus(&self.focus);
    }

    fn language_menu(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let frame = self.language_menu.frame();
        if !frame.visible {
            return None;
        }

        let colors = self.app.read(cx).theme.root;
        let locale = self.app.read(cx).locale.clone();
        let locales = translatable_locales();
        let side = self.language_menu.side;
        let natural = (
            LANGUAGE_MENU_WIDTH_PX.max(MENU_MIN_WIDTH_PX),
            menu_natural_height(locales.len(), LANGUAGE_MENU_ITEM_HEIGHT_PX),
        );
        let anchor = (
            HEADER_ICON_BUTTON_PX,
            HEADER_ICON_BUTTON_PX + MENU_OFFSET_PX,
        );
        let placement = place_anchored(frame, side, natural, anchor);

        let items = locales
            .into_iter()
            .map(|(code, name)| {
                let selected = code == locale;
                let highlight = self.transitions.eased(&format!("lang-{code}")) > 0.5;
                let pick = code.clone();

                menu_item(
                    SharedString::from(format!("lang-item-{code}")),
                    colors,
                    name,
                    highlight,
                    selected,
                )
                .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                    this.transitions.set(format!("lang-{code}"), *hovered);
                    cx.notify();
                }))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    let code = pick.clone();
                    this.app.update(cx, |model, cx| model.set_locale(&code, cx));
                    this.language_menu.dismiss();
                    cx.notify();
                }))
            })
            .collect::<Vec<_>>();

        Some(
            crate::components::overlay_root()
                .child(overlay_backdrop("language-backdrop").on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.language_menu.dismiss();
                        cx.notify();
                    }),
                ))
                .child(overlay_layer(
                    gpui::Corner::TopRight,
                    placement,
                    menu_surface(colors, placement).children(items),
                )),
        )
    }

    fn project_name(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.app.read(cx).theme.root;

        if let Some(field) = self.name_edit.as_ref() {
            let editor = text_field(
                "header-name-input",
                field,
                colors,
                FieldStyle {
                    height: HEADER_ICON_BUTTON_PX,
                    placeholder: SharedString::from(t("dialog.rename.placeholder")),
                    leading: None,
                    ..Default::default()
                },
                window,
            )
            .on_key_down(cx.listener(
                |this: &mut Self, event: &KeyDownEvent, _, cx| {
                    let Some(field) = this.name_edit.as_mut() else {
                        return;
                    };
                    match field.buffer.key_down(event) {
                        TextEvent::Submit => {
                            let name = field.text().trim().to_string();
                            this.name_edit = None;
                            let id = this
                                .app
                                .read(cx)
                                .project
                                .as_ref()
                                .map(|project| project.metadata.id.clone());
                            if let (Some(id), false) = (id, name.is_empty()) {
                                this.app
                                    .update(cx, |model, cx| model.rename_project(&id, name, cx));
                            }
                        }
                        TextEvent::Cancel => this.name_edit = None,
                        _ => {}
                    }
                    cx.notify();
                },
            ));
            window.focus(&field.focus);

            return div()
                .w(px(PROJECT_NAME_FIELD_WIDTH_PX))
                .flex_shrink_0()
                .child(editor)
                .into_any_element();
        }

        let name = self.app.read(cx).project_name();
        let progress = self.transitions.eased("header-name");

        div()
            .id("header-name")
            .flex()
            .h(px(HEADER_ICON_BUTTON_PX))
            .items_center()
            .px(px(8.0))
            .rounded(rem(RADIUS_SM))
            .cursor_pointer()
            .text_size(rem(TEXT_PROJECT_NAME))
            .bg(mix(opacity(colors.accent, 0.0), colors.accent, progress))
            .text_color(mix(colors.foreground, colors.accent_foreground, progress))
            .on_hover(header_hover("header-name", cx))
            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                let name = this.app.read(cx).project_name();
                this.name_edit = Some(TextField::new(cx, name));
                cx.notify();
            }))
            .child(name)
            .into_any_element()
    }

    fn header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.app.read(cx).theme.root;
        let dark = self.app.read(cx).dark;
        let logo = self.transitions.eased("header-logo");
        let export = self.transitions.eased("header-export");
        let language = if self.language_menu.is_open() {
            1.0
        } else {
            self.transitions.eased("header-language")
        };
        let theme_toggle = self.transitions.eased("header-theme");
        let shortcuts_hover = if self.shortcuts.open {
            1.0
        } else {
            self.transitions.eased("header-shortcuts")
        };
        let language_menu = self.language_menu(cx);
        let name = self.project_name(window, cx);

        let icon_button = |id: &'static str, glyph: &'static str, progress: f32| {
            Button::new(id, colors)
                .variant(ButtonVariant::Ghost)
                .size(ButtonSize::Icon)
                .hover(progress)
                .build()
                .size(px(HEADER_ICON_BUTTON_PX))
                .child(
                    svg()
                        .size(px(HEADER_ICON_GLYPH_PX))
                        .flex_shrink_0()
                        .path(icon(glyph))
                        .text_color(colors.foreground),
                )
        };
        let settings_hover = self.transitions.eased("header-settings");
        let settings_tip = self.tooltips.frame_for("header-settings");
        let language_tip = self.tooltips.frame_for("header-language");
        let theme_tip = self.tooltips.frame_for("header-theme");
        let export_tip = self.tooltips.frame_for("header-export");
        let logo_tip = self.tooltips.frame_for("header-logo");
        let shortcuts_tip = self.tooltips.frame_for("header-shortcuts");

        div()
            .flex()
            .w_full()
            .h(px(HEADER_HEIGHT))
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .bg(colors.background)
            .px(px(12.0))
            .pt(px(2.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .child(tooltipped(
                        div()
                            .id("header-logo")
                            .flex()
                            .size(px(HEADER_ICON_BUTTON_PX))
                            .items_center()
                            .justify_center()
                            .rounded(rem(RADIUS_SM))
                            .cursor_pointer()
                            .bg(mix(opacity(colors.accent, 0.0), colors.accent, logo))
                            .on_hover(header_hover("header-logo", cx))
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.name_edit = None;
                                this.app.update(cx, |model, cx| model.close_project(cx));
                            }))
                            .child(
                                svg()
                                    .size(px(20.0))
                                    .path(icon("cutix-logo"))
                                    .text_color(colors.foreground),
                            ),
                        colors,
                        t("projects.title"),
                        logo_tip,
                        OverlaySide::Bottom,
                    ))
                    .child(name),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(tooltipped(
                        Button::new("header-export", colors)
                            .variant(ButtonVariant::Outline)
                            .hover(export)
                            .icon("upload04")
                            .label(t("common.export"))
                            .build()
                            .h(px(HEADER_ICON_BUTTON_PX))
                            .on_hover(header_hover("header-export", cx))
                            .on_mouse_down(gpui::MouseButton::Left, header_press(cx))
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.tooltips.dismiss();
                                this.app
                                    .update(cx, |model, cx| model.toggle_export_dialog(cx));
                            })),
                        colors,
                        t("export.title"),
                        export_tip,
                        OverlaySide::Bottom,
                    ))
                    .child(tooltipped(
                        icon_button("header-settings", "settings01", settings_hover)
                            .on_hover(header_hover("header-settings", cx))
                            .on_mouse_down(gpui::MouseButton::Left, header_press(cx))
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.tooltips.dismiss();
                                this.app.update(cx, |model, cx| {
                                    model.settings_request = Some(String::new());
                                    cx.notify();
                                });
                            })),
                        colors,
                        t("settings.open"),
                        settings_tip,
                        OverlaySide::Bottom,
                    ))
                    .child(tooltipped(
                        icon_button("header-shortcuts", "command", shortcuts_hover)
                            .on_hover(header_hover("header-shortcuts", cx))
                            .on_mouse_down(gpui::MouseButton::Left, header_press(cx))
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.open_shortcuts(cx);
                            })),
                        colors,
                        t("editor.shortcuts"),
                        shortcuts_tip,
                        OverlaySide::Bottom,
                    ))
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_shrink_0()
                            .child(tooltipped(
                                icon_button("header-language", "languages", language)
                                    .on_hover(header_hover("header-language", cx))
                                    .on_mouse_down(gpui::MouseButton::Left, header_press(cx))
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.language_menu.toggle();
                                        cx.notify();
                                    })),
                                colors,
                                t("common.language"),
                                language_tip,
                                OverlaySide::Bottom,
                            ))
                            .children(language_menu),
                    )
                    .child(tooltipped(
                        icon_button(
                            "header-theme",
                            if dark { "sun03" } else { "moon02" },
                            theme_toggle,
                        )
                        .on_hover(header_hover("header-theme", cx))
                        .on_mouse_down(gpui::MouseButton::Left, header_press(cx))
                        .on_click(cx.listener(
                            |this: &mut Self, _, _, cx| {
                                this.app.update(cx, |model, cx| model.toggle_theme(cx));
                            },
                        )),
                        colors,
                        t(if dark { "theme.light" } else { "theme.dark" }),
                        theme_tip,
                        OverlaySide::Bottom,
                    )),
            )
    }

    fn editor(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = Layout {
            tools: self.layout.tools,
            preview: self.layout.preview,
            properties: self.layout.properties,
            main_content: self.layout.main_content,
        };
        let header = self.header(window, cx);

        let column_handle = |id: &'static str, split: Split| {
            div()
                .id(id)
                .flex()
                .flex_shrink_0()
                .w(px(COLUMN_GAP))
                .h_full()
                .items_center()
                .justify_center()
                .cursor_col_resize()
                .on_drag(split, |_, _, _, cx| cx.new(|_| gpui::Empty))
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(header)
            .child(
                div()
                    .id("editor-rows")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .on_drag_move::<RowSplit>(cx.listener(Self::on_row_drag))
                    .on_drag_move::<Split>(cx.listener(Self::on_column_drag))
                    .child(
                        div()
                            .id("editor-columns")
                            .flex()
                            .flex_shrink_0()
                            .h(relative(layout.main_content))
                            .w_full()
                            .min_h_0()
                            .overflow_x_scroll()
                            .px(px(12.0))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .w(relative(layout.tools))
                                    .min_w(px(TOOLS_MIN_WIDTH_PX))
                                    .h_full()
                                    .child(self.assets.clone()),
                            )
                            .child(column_handle("split-tools", Split::Tools))
                            .child(
                                div()
                                    .flex_1()
                                    .h_full()
                                    .min_w(px(PREVIEW_MIN_WIDTH_PX))
                                    .child(self.preview.clone()),
                            )
                            .child(column_handle("split-properties", Split::Properties))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .w(relative(layout.properties))
                                    .min_w(px(PROPERTIES_MIN_WIDTH_PX))
                                    .h_full()
                                    .child(self.properties.clone()),
                            ),
                    )
                    .child(
                        div()
                            .id("split-main-content")
                            .flex_shrink_0()
                            .w_full()
                            .h(px(ROW_GAP.max(HANDLE_THICKNESS)))
                            .cursor_row_resize()
                            .on_drag(RowSplit, |_, _, _, cx| cx.new(|_| gpui::Empty)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .w_full()
                            .min_h_0()
                            .px(px(12.0))
                            .pb(px(12.0))
                            .child(self.timeline.clone()),
                    ),
            )
    }
}

pub fn translatable_locales() -> Vec<(String, String)> {
    cutix_i18n::available_locales()
        .into_iter()
        .filter_map(|code| cutix_i18n::locale_name(&code).map(|name| (code, name)))
        .collect()
}

#[derive(Debug)]
struct RowSplit;

fn header_hover(
    id: &'static str,
    cx: &mut Context<Shell>,
) -> Box<dyn Fn(&bool, &mut Window, &mut App) + 'static> {
    Box::new(cx.listener(move |this: &mut Shell, hovered: &bool, _, cx| {
        this.transitions.set(id, *hovered);
        this.tooltips.hover(id, *hovered);
        cx.notify();
    }))
}

fn header_press(
    cx: &mut Context<Shell>,
) -> Box<dyn Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static> {
    Box::new(cx.listener(move |this: &mut Shell, _, _, cx| {
        this.tooltips.dismiss();
        cx.notify();
    }))
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.app.read(cx).theme.root;
        let route = self.app.read(cx).route;

        if self.route_shown != Some(route) {
            self.route_shown = Some(route);
            window.focus(&self.focus);
        }
        self.persist_geometry(window);
        self.tooltips.tick();
        if self.transitions.animating()
            || self.language_menu.animating()
            || self.tooltips.animating()
        {
            window.request_animation_frame();
        }

        let content = if self.publish_only {
            div().size_full().into_any_element()
        } else {
            match route {
                Route::Home => self.home.clone().into_any_element(),
                Route::Projects => self.projects.clone().into_any_element(),
                Route::Library => self.library.clone().into_any_element(),
                Route::Editor => self.editor(window, cx).into_any_element(),
            }
        };

        let youtube = self
            .assets
            .update(cx, |panel, cx| panel.panel_overlays(window, cx));

        if self.publish_only {
            let closing = self
                .assets
                .update(cx, |panel, _| panel.youtube_wants_to_close());
            if closing {
                cx.quit();
            }
            let wanted = self
                .assets
                .update(cx, |panel, _| panel.publish_window_size());
            if self.publish_size != Some(wanted) {
                self.publish_size = Some(wanted);
                let scale = crate::notify::desktop_scale();
                window.resize(gpui::size(px(wanted.0 * scale), px(wanted.1 * scale)));
                cx.defer(|_| crate::notify::centre_own_window(0.0, 0.0));
            }
        }

        let export_view = (route == Route::Editor && !self.publish_only)
            .then(|| crate::export::snapshot(self.app.read(cx)))
            .flatten();
        let export = export_view.map(|view| crate::export::export_dialog(view, cx));
        if self.app.read(cx).export.is_running() {
            window.request_animation_frame();
        }
        let shortcuts = self.shortcuts.open.then(|| {
            let bindings = self.app.read(cx).keybindings.clone();
            crate::shortcuts::dialog(colors, &bindings, &self.shortcuts, cx)
        });

        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(
                |this: &mut Self, event: &KeyDownEvent, window: &mut Window, cx| {
                    if this.shortcuts.recording.is_some() {
                        this.capture_rebind(event, cx);
                        return;
                    }
                    if event.keystroke.key == "escape" {
                        this.cancel_interaction(window, cx);
                        return;
                    }

                    if event.keystroke.key == "escape" {
                        let closed = this
                            .assets
                            .update(cx, |panel, _| panel.dismiss_youtube_overlays());
                        if closed {
                            cx.notify();
                            return;
                        }
                    }
                    if this.app.read(cx).route == Route::Library && this.name_edit.is_none() {
                        let step = match event.keystroke.key.as_str() {
                            "+" | "=" | "add" => Some(0.05),
                            "-" | "subtract" => Some(-0.05),
                            _ => None,
                        };
                        if let Some(step) = step {
                            let heard = this.library.update(cx, |library, cx| {
                                if library.hearing_anything() {
                                    library.nudge_volume(step, cx);
                                    true
                                } else {
                                    false
                                }
                            });
                            if heard {
                                cx.notify();
                                return;
                            }
                        }
                    }
                    if this.name_edit.is_some() || this.app.read(cx).route != Route::Editor {
                        return;
                    }
                    let Some(chord) = Chord::from_keystroke(&event.keystroke) else {
                        return;
                    };

                    let typing = window
                        .focused(cx)
                        .is_some_and(|handle| handle != this.focus);
                    if !chord.control && !chord.alt && typing {
                        return;
                    }
                    this.run_chord(&chord, window, cx);
                },
            ))
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .font_family(self.font.clone())
            .text_size(rem(crate::theme::TEXT_BASE))
            .when(self.publish_only, |this| this.bg(colors.popover))
            .when(!self.publish_only, |this| {
                this.bg(colors.background).child(self.titlebar.clone())
            })
            .text_color(colors.foreground)
            .child(div().flex().flex_col().flex_1().min_h_0().child(content))
            .children(youtube)
            .children(export)
            .children(shortcuts)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_columns_sum_to_one() {
        let layout = Layout::default();
        assert!((layout.tools + layout.preview + layout.properties - 1.0).abs() < 1e-6);
    }

    #[test]
    fn dragging_the_tools_split_preserves_the_pair_total() {
        let mut layout = Layout::default();
        let total = layout.tools + layout.preview;
        layout.drag_tools(0.45);
        assert!((layout.tools - 0.45).abs() < 1e-6);
        assert!((layout.tools + layout.preview - total).abs() < 1e-6);
    }

    #[test]
    fn column_drags_respect_the_minimum() {
        let mut layout = Layout::default();
        layout.drag_tools(0.0);
        assert!(layout.tools >= MIN_COLUMN);
    }

    #[test]
    fn row_drag_is_clamped_to_the_web_limits() {
        let mut layout = Layout::default();
        layout.drag_main_content(0.99);
        assert!((layout.main_content - MAX_ROW).abs() < 1e-6);
        layout.drag_main_content(0.0);
        assert!((layout.main_content - MIN_ROW).abs() < 1e-6);
    }

    #[test]
    fn the_bundled_locales_are_all_translatable() {
        let locales = translatable_locales();
        assert!(locales.iter().any(|(code, _)| code == "ru"));
        assert!(locales.iter().all(|(_, name)| !name.is_empty()));
    }
}
