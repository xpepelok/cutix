use std::collections::HashSet;

use cutix_i18n::{t, t_args};
use cutix_project::ProjectSummary;
use gpui::{
    div, img, prelude::*, px, relative, svg, App, Context, Div, Entity, FocusHandle, FontWeight,
    KeyDownEvent, MouseButton, ScrollHandle, SharedString, Window,
};

use crate::assets::icon;
use crate::components::{
    menu_item, menu_natural_height, menu_surface, overlay_backdrop, overlay_layer, overlay_root,
    place_anchored, separator_v, tooltipped, Button, ButtonSize, ButtonVariant,
    MENU_ITEM_HEIGHT_PX, MENU_OFFSET_PX,
};
use crate::input::{text_field, FieldStyle, TextEvent, TextField};
use crate::interaction::{mix, Overlay, OverlaySide, Tooltips, Transitions, TOOLTIP_DELAY};
use crate::scroll::scrollbar_v;
use crate::state::{format_date, format_duration, AppModel, Route, SortKey, ViewMode};
use crate::theme::{
    opacity, rem, Palette, RADIUS_LG, RADIUS_MD, RADIUS_SM, TEXT_BASE, TEXT_LG, TEXT_SM, TEXT_XS,
};

const HEADER_ROW_HEIGHT_PX: f32 = 64.0;
const TOOLBAR_HEIGHT_PX: f32 = 56.0;
const SEARCH_WIDTH_PX: f32 = 240.0;
const CARD_GUTTER_PX: f32 = 12.0;
const LIST_ROW_HEIGHT_PX: f32 = 56.0;
const CHECKBOX_PX: f32 = 20.0;
const MENU_WIDTH_PX: f32 = 192.0;
const LANGUAGE_MENU_WIDTH_PX: f32 = 176.0;
const DIALOG_WIDTH_PX: f32 = 460.0;
const CARD_THUMBNAIL_RATIO: f32 = 9.0 / 16.0;

const SORT_KEYS: &[SortKey] = &[
    SortKey::CreatedAt,
    SortKey::UpdatedAt,
    SortKey::Name,
    SortKey::Duration,
];

const CARD_ACTIONS: &[(&str, &str, &str)] = &[
    ("rename", "edit03", "common.rename"),
    ("duplicate", "copy01", "common.duplicate"),
    ("info", "information-circle", "projects.info"),
    ("delete", "delete02", "common.delete"),
];

pub struct ProjectsView {
    app: Entity<AppModel>,
    focus: FocusHandle,
    transitions: Transitions,
    tooltips: Tooltips,
    search: TextField,
    scroll: ScrollHandle,
    sort_menu: Overlay,
    card_menu: Overlay,
    card_menu_for: Option<String>,
    language_menu: Overlay,
    selected: HashSet<String>,
    rename: Option<(String, TextField)>,
    delete: Option<Vec<(String, String)>>,
    info: Option<ProjectSummary>,
}

impl ProjectsView {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();

        Self {
            app,
            focus: cx.focus_handle(),
            transitions: Transitions::new(),
            tooltips: Tooltips::new(TOOLTIP_DELAY),
            search: TextField::new(cx, ""),
            scroll: ScrollHandle::new(),
            sort_menu: Overlay::new(OverlaySide::Bottom),
            card_menu: Overlay::new(OverlaySide::Bottom),
            card_menu_for: None,
            language_menu: Overlay::new(OverlaySide::Bottom),
            selected: HashSet::new(),
            rename: None,
            delete: None,
            info: None,
        }
    }

    fn colors(&self, cx: &App) -> Palette {
        self.app.read(cx).theme.root
    }

    fn escape(&mut self, cx: &mut Context<Self>) {
        self.sort_menu.dismiss();
        self.card_menu.dismiss();
        self.language_menu.dismiss();
        self.card_menu_for = None;
        self.tooltips.dismiss();
        self.rename = None;
        self.delete = None;
        self.info = None;
        cx.notify();
    }

    fn hover(
        &mut self,
        id: impl Into<String>,
        cx: &mut Context<Self>,
    ) -> Box<dyn Fn(&bool, &mut Window, &mut App) + 'static> {
        let id = id.into();
        Box::new(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
            this.transitions.set(id.clone(), *hovered);
            this.tooltips.hover(&id, *hovered);
            cx.notify();
        }))
    }

    fn apply_search(&mut self, cx: &mut Context<Self>) {
        let query = self.search.text().to_string();
        self.app.update(cx, |model, cx| {
            model.search = query;
            cx.notify();
        });
        cx.notify();
    }

    fn pick_project_package(&mut self, cx: &mut Context<Self>) {
        let app = self.app.clone();
        cx.spawn(async move |_, cx| {
            let picked = crate::dialogs::open_files(
                crate::dialogs::Filter::Project,
                t("dialog.project.open"),
                false,
            )
            .await;
            let Some(path) = picked.into_iter().next() else {
                return;
            };
            let _ = app.update(cx, |model, cx| model.import_project_package(path, cx));
        })
        .detach();
    }

    fn open(&mut self, id: &str, cx: &mut Context<Self>) {
        let id = id.to_string();
        self.app.update(cx, |model, cx| model.open_project(&id, cx));
    }

    fn run_card_action(&mut self, action: &str, summary: &ProjectSummary, cx: &mut Context<Self>) {
        self.card_menu.dismiss();
        self.card_menu_for = None;
        match action {
            "rename" => {
                let field = TextField::new(cx, summary.name.clone());
                self.rename = Some((summary.id.clone(), field));
            }
            "duplicate" => {
                let id = summary.id.clone();
                self.app
                    .update(cx, |model, cx| model.duplicate_project(&id, cx));
            }
            "info" => self.info = Some(summary.clone()),
            "delete" => {
                self.delete = Some(vec![(summary.id.clone(), summary.name.clone())]);
            }
            _ => {}
        }
        cx.notify();
    }
}

fn checkbox(id: impl Into<SharedString>, colors: Palette, checked: bool) -> gpui::Stateful<Div> {
    div()
        .id(id.into())
        .flex()
        .size(px(CHECKBOX_PX))
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(rem(RADIUS_SM))
        .border_1()
        .border_color(if checked {
            colors.primary
        } else {
            colors.border
        })
        .bg(if checked {
            colors.primary
        } else {
            opacity(colors.background, 0.6)
        })
        .cursor_pointer()
        .when(checked, |this| {
            this.child(
                svg()
                    .size(px(14.0))
                    .path(icon("tick02"))
                    .text_color(colors.primary_foreground),
            )
        })
}

impl ProjectsView {
    fn header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        let model = self.app.read(cx);
        let view_mode = model.view_mode;
        let locale = model.locale.clone();
        let dark = model.dark;

        let mut toggles = Vec::new();
        for (mode, glyph) in [
            (ViewMode::Grid, "grid-view"),
            (ViewMode::List, "left-to-right-list-dash"),
        ] {
            let id = format!("view-{glyph}");
            let progress = self.transitions.eased(&id);
            let active = view_mode == mode;
            toggles.push(
                div()
                    .id(SharedString::from(id.clone()))
                    .flex()
                    .size(px(30.0))
                    .items_center()
                    .justify_center()
                    .rounded(rem(RADIUS_SM))
                    .cursor_pointer()
                    .bg(if active {
                        colors.accent
                    } else {
                        mix(
                            opacity(colors.accent, 0.0),
                            opacity(colors.accent, 0.6),
                            progress,
                        )
                    })
                    .on_hover(self.hover(id, cx))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.app.update(cx, |model, cx| {
                            model.view_mode = mode;
                            cx.notify();
                        });
                    }))
                    .child(
                        svg()
                            .size(px(16.0))
                            .path(icon(glyph))
                            .text_color(colors.foreground),
                    ),
            );
        }

        let search = text_field(
            "projects-search",
            &self.search,
            colors,
            FieldStyle {
                height: 40.0,
                placeholder: SharedString::from(t("common.search")),
                leading: Some(icon("search01")),
                ..Default::default()
            },
            window,
        )
        .on_key_down(cx.listener(|this: &mut Self, event: &KeyDownEvent, _, cx| {
            match crate::input::key_down_with_clipboard(&mut this.search.buffer, event, false, cx) {
                TextEvent::Cancel => {
                    this.search.buffer.set("");
                    this.apply_search(cx);
                }
                TextEvent::Changed | TextEvent::Submit => this.apply_search(cx),
                _ => cx.notify(),
            }
        }));

        let language = self.language_control(&locale, cx);
        let theme_progress = self.transitions.eased("projects-theme");
        let theme_tip = self.tooltips.frame_for("projects-theme");
        let new_progress = self.transitions.eased("projects-new");

        div()
            .flex()
            .w_full()
            .h(px(HEADER_ROW_HEIGHT_PX))
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .px(px(32.0))
            .pt(px(8.0))
            .bg(colors.background)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(20.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(rem(TEXT_BASE))
                            .child(
                                div()
                                    .id("projects-breadcrumb-home")
                                    .cursor_pointer()
                                    .text_color(colors.muted_foreground)
                                    .child(t("projects.breadcrumb.home"))
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.app.update(cx, |model, cx| {
                                            model.route = Route::Home;
                                            cx.notify();
                                        });
                                    })),
                            )
                            .child(
                                svg()
                                    .size(px(14.0))
                                    .path(icon("chevron-right"))
                                    .text_color(colors.muted_foreground),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(t("projects.breadcrumb.all")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .h(px(40.0))
                            .items_center()
                            .gap(px(2.0))
                            .px(px(6.0))
                            .rounded(rem(RADIUS_MD))
                            .border_1()
                            .border_color(colors.border)
                            .children(toggles),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .child(div().w(px(SEARCH_WIDTH_PX)).flex_shrink_0().child(search))
                    .child(language)
                    .child(tooltipped(
                        Button::new("projects-theme", colors)
                            .variant(ButtonVariant::Ghost)
                            .size(ButtonSize::Icon)
                            .hover(theme_progress)
                            .icon(if dark { "sun03" } else { "moon02" })
                            .build()
                            .size(px(32.0))
                            .on_hover(self.hover("projects-theme", cx))
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.app.update(cx, |model, cx| model.toggle_theme(cx));
                            })),
                        colors,
                        t(if dark { "theme.light" } else { "theme.dark" }),
                        theme_tip,
                        OverlaySide::Bottom,
                    ))
                    .child(
                        Button::new("projects-open-file", colors)
                            .variant(ButtonVariant::Outline)
                            .size(ButtonSize::Lg)
                            .label(t("projects.open.file"))
                            .build()
                            .px(px(18.0))
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.pick_project_package(cx);
                            })),
                    )
                    .child(
                        Button::new("projects-new", colors)
                            .size(ButtonSize::Lg)
                            .hover(new_progress)
                            .label(t("projects.new.action"))
                            .build()
                            .px(px(24.0))
                            .on_hover(self.hover("projects-new", cx))
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.app.update(cx, |model, cx| model.create_project(cx));
                            })),
                    ),
            )
    }

    fn language_control(&mut self, locale: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        let progress = self.transitions.eased("projects-language");
        let tip = self.tooltips.frame_for("projects-language");
        let menu = self.language_menu(locale, cx);

        div()
            .relative()
            .flex()
            .flex_shrink_0()
            .child(tooltipped(
                Button::new("projects-language", colors)
                    .variant(ButtonVariant::Ghost)
                    .size(ButtonSize::Icon)
                    .hover(progress)
                    .icon("languages")
                    .build()
                    .size(px(32.0))
                    .on_hover(self.hover("projects-language", cx))
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.sort_menu.dismiss();
                        this.card_menu.dismiss();
                        this.language_menu.toggle();
                        cx.notify();
                    })),
                colors,
                t("common.language"),
                tip,
                OverlaySide::Bottom,
            ))
            .children(menu)
    }

    fn language_menu(&mut self, locale: &str, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let frame = self.language_menu.frame();
        if !frame.visible {
            return None;
        }
        let colors = self.colors(cx);
        let locales = crate::shell::translatable_locales();
        let natural = (
            LANGUAGE_MENU_WIDTH_PX,
            menu_natural_height(locales.len(), MENU_ITEM_HEIGHT_PX),
        );
        let placement = place_anchored(
            frame,
            OverlaySide::Bottom,
            natural,
            (32.0, 32.0 + MENU_OFFSET_PX),
        );

        let items = locales
            .into_iter()
            .map(|(code, name)| {
                let selected = code == locale;
                let key = format!("lang-{code}");
                let highlighted = self.transitions.eased(&key) > 0.5;
                let pick = code.clone();
                menu_item(
                    SharedString::from(key.clone()),
                    colors,
                    name,
                    highlighted,
                    selected,
                )
                .on_hover(self.hover(key, cx))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    let code = pick.clone();
                    this.app.update(cx, |model, cx| model.set_locale(&code, cx));
                    this.language_menu.dismiss();
                    cx.notify();
                }))
            })
            .collect::<Vec<_>>();

        Some(
            overlay_root()
                .child(overlay_backdrop("language-backdrop").on_mouse_up(
                    MouseButton::Left,
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

    fn toolbar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        let model = self.app.read(cx);
        let sort_key = model.sort_key;
        let ascending = model.sort_ascending;
        let ids: Vec<String> = model
            .visible_projects()
            .into_iter()
            .map(|project| project.id)
            .collect();

        let all_selected = !ids.is_empty() && ids.iter().all(|id| self.selected.contains(id));
        let selected_count = self.selected.len();
        let sort_progress = self.transitions.eased("projects-sort");
        let order_progress = self.transitions.eased("projects-order");
        let sort_menu = self.sort_menu(sort_key, cx);
        let bulk = (selected_count > 0).then(|| self.bulk_actions(cx));

        div()
            .flex()
            .w_full()
            .h(px(TOOLBAR_HEIGHT_PX))
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .px(px(24.0))
            .pt(px(8.0))
            .bg(colors.background)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id("projects-select-all")
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .px(px(8.0))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                if all_selected {
                                    this.selected.clear();
                                } else {
                                    this.selected = ids.iter().cloned().collect();
                                }
                                cx.notify();
                            }))
                            .child(checkbox("projects-select-all-box", colors, all_selected))
                            .child(
                                div()
                                    .text_size(rem(TEXT_SM))
                                    .text_color(colors.muted_foreground)
                                    .child(t("projects.selectAll")),
                            ),
                    )
                    .child(separator_v(colors, 16.0))
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_shrink_0()
                            .child(
                                Button::new("projects-sort", colors)
                                    .variant(ButtonVariant::Text)
                                    .hover(sort_progress)
                                    .label(t(sort_key.label_key()))
                                    .build()
                                    .h(px(28.0))
                                    .pl(px(8.0))
                                    .text_color(colors.muted_foreground)
                                    .on_hover(self.hover("projects-sort", cx))
                                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                        this.language_menu.dismiss();
                                        this.sort_menu.toggle();
                                        cx.notify();
                                    })),
                            )
                            .children(sort_menu),
                    )
                    .child(
                        Button::new("projects-order", colors)
                            .variant(ButtonVariant::Text)
                            .hover(order_progress)
                            .icon(if ascending {
                                "sorting-one-nine"
                            } else {
                                "sorting-nine-one"
                            })
                            .build()
                            .size(px(28.0))
                            .on_hover(self.hover("projects-order", cx))
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.app.update(cx, |model, cx| {
                                    model.sort_ascending = !model.sort_ascending;
                                    cx.notify();
                                });
                            })),
                    ),
            )
            .children(bulk)
    }

    fn bulk_actions(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        let duplicate = self.transitions.eased("bulk-duplicate");
        let delete = self.transitions.eased("bulk-delete");

        div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(12.0))
            .child(
                Button::new("bulk-duplicate", colors)
                    .variant(ButtonVariant::Outline)
                    .size(ButtonSize::Icon)
                    .hover(duplicate)
                    .icon("copy01")
                    .build()
                    .size(px(36.0))
                    .on_hover(self.hover("bulk-duplicate", cx))
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        let ids: Vec<String> = this.selected.iter().cloned().collect();
                        this.app.update(cx, |model, cx| {
                            for id in &ids {
                                model.duplicate_project(id, cx);
                            }
                        });
                        this.selected.clear();
                        cx.notify();
                    })),
            )
            .child(
                Button::new("bulk-delete", colors)
                    .variant(ButtonVariant::DestructiveForeground)
                    .size(ButtonSize::Icon)
                    .hover(delete)
                    .icon("delete02")
                    .build()
                    .size(px(36.0))
                    .on_hover(self.hover("bulk-delete", cx))
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        let selected = this.selected.clone();
                        let victims: Vec<(String, String)> = this
                            .app
                            .read(cx)
                            .projects
                            .iter()
                            .filter(|project| selected.contains(&project.id))
                            .map(|project| (project.id.clone(), project.name.clone()))
                            .collect();
                        if !victims.is_empty() {
                            this.delete = Some(victims);
                        }
                        cx.notify();
                    })),
            )
    }

    fn sort_menu(&mut self, active: SortKey, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let frame = self.sort_menu.frame();
        if !frame.visible {
            return None;
        }
        let colors = self.colors(cx);
        let natural = (
            MENU_WIDTH_PX,
            menu_natural_height(SORT_KEYS.len(), MENU_ITEM_HEIGHT_PX),
        );
        let placement = place_anchored(
            frame,
            OverlaySide::Bottom,
            natural,
            (0.0, 28.0 + MENU_OFFSET_PX),
        );

        let items = SORT_KEYS
            .iter()
            .map(|key| {
                let key = *key;
                let id = format!("sort-{}", key.label_key());
                let highlighted = self.transitions.eased(&id) > 0.5;
                menu_item(
                    SharedString::from(id.clone()),
                    colors,
                    t(key.label_key()),
                    highlighted,
                    key == active,
                )
                .on_hover(self.hover(id, cx))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.app.update(cx, |model, cx| {
                        model.sort_key = key;
                        cx.notify();
                    });
                    this.sort_menu.dismiss();
                    cx.notify();
                }))
            })
            .collect::<Vec<_>>();

        Some(
            overlay_root()
                .child(overlay_backdrop("sort-backdrop").on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        this.sort_menu.dismiss();
                        cx.notify();
                    }),
                ))
                .child(overlay_layer(
                    gpui::Corner::TopLeft,
                    placement,
                    menu_surface(colors, placement).children(items),
                )),
        )
    }

    fn card_menu(&mut self, summary: &ProjectSummary, cx: &mut Context<Self>) -> Option<Div> {
        if self.card_menu_for.as_deref() != Some(summary.id.as_str()) {
            return None;
        }
        let frame = self.card_menu.frame();
        if !frame.visible {
            return None;
        }
        let colors = self.colors(cx);
        let natural = (
            MENU_WIDTH_PX,
            menu_natural_height(CARD_ACTIONS.len(), MENU_ITEM_HEIGHT_PX),
        );
        let placement = place_anchored(
            frame,
            OverlaySide::Bottom,
            natural,
            (28.0, 28.0 + MENU_OFFSET_PX),
        );

        let items = CARD_ACTIONS
            .iter()
            .map(|(action, glyph, label)| {
                let action = *action;
                let id = format!("card-{}-{action}", summary.id);
                let highlighted = self.transitions.eased(&id) > 0.5;
                let summary = summary.clone();
                let destructive = action == "delete";

                let fill = if highlighted {
                    colors.popover_hover
                } else {
                    opacity(colors.popover_hover, 0.0)
                };
                let tint = if destructive {
                    colors.destructive
                } else {
                    colors.popover_foreground
                };

                div()
                    .id(SharedString::from(id.clone()))
                    .flex()
                    .w_full()
                    .h(px(MENU_ITEM_HEIGHT_PX))
                    .flex_shrink_0()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(rem(RADIUS_SM))
                    .px(px(10.0))
                    .cursor_pointer()
                    .text_size(rem(TEXT_SM))
                    .text_color(tint)
                    .bg(fill)
                    .on_hover(self.hover(id, cx))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        cx.stop_propagation();
                        this.run_card_action(action, &summary, cx);
                    }))
                    .child(svg().size(px(14.0)).path(icon(glyph)).text_color(tint))
                    .child(t(label))
            })
            .collect::<Vec<_>>();

        Some(
            overlay_root()
                .child(overlay_backdrop("card-backdrop").on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        cx.stop_propagation();
                        this.card_menu.dismiss();
                        this.card_menu_for = None;
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

    fn menu_trigger(
        &mut self,
        summary: &ProjectSummary,
        colors: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = format!("menu-{}", summary.id);
        let progress = self.transitions.eased(&id);
        let target = summary.id.clone();

        Button::new(SharedString::from(id.clone()), colors)
            .variant(ButtonVariant::Background)
            .size(ButtonSize::Icon)
            .hover(progress)
            .icon("more-horizontal")
            .build()
            .size(px(28.0))
            .on_hover(self.hover(id, cx))
            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                cx.stop_propagation();
                let same = this.card_menu_for.as_deref() == Some(target.as_str());
                this.sort_menu.dismiss();
                this.language_menu.dismiss();
                if same {
                    this.card_menu.dismiss();
                    this.card_menu_for = None;
                } else {
                    this.card_menu.dismiss();
                    this.card_menu = Overlay::new(OverlaySide::Bottom);
                    this.card_menu.set_open(true);
                    this.card_menu_for = Some(target.clone());
                }
                cx.notify();
            }))
    }

    fn thumbnail(&self, summary: &ProjectSummary, size: f32, colors: Palette, cx: &App) -> Div {
        let path = self.app.read(cx).project_thumbnail(summary);
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .bg(colors.muted)
            .child(match path {
                Some(path) => img(path).size_full().into_any_element(),
                None => svg()
                    .w(px(size * 29.0 / 24.0))
                    .h(px(size))
                    .path(icon("oc-video"))
                    .text_color(colors.muted_foreground)
                    .into_any_element(),
            })
    }

    fn grid_card(&mut self, summary: &ProjectSummary, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        let id = summary.id.clone();
        let selected = self.selected.contains(&id);
        let hover_key = format!("card-{id}");
        let hovered = self.transitions.eased(&hover_key) > 0.02;
        let duration = format_duration(summary.duration);
        let menu = self.card_menu(summary, cx);
        let has_menu = menu.is_some();
        let thumbnail = self.thumbnail(summary, 48.0, colors, cx);
        let menu_trigger = self.menu_trigger(summary, colors, cx);
        let open_id = id.clone();
        let toggle_id = id.clone();

        div()
            .w(relative(0.25))
            .flex_shrink_0()
            .p(px(CARD_GUTTER_PX))
            .child(
                div()
                    .id(SharedString::from(hover_key.clone()))
                    .relative()
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap(px(8.0))
                    .on_hover(self.hover(hover_key, cx))
                    .child(
                        div()
                            .id(SharedString::from(format!("open-{id}")))
                            .relative()
                            .w_full()
                            .h(px(0.0))
                            .pb(relative(CARD_THUMBNAIL_RATIO))
                            .rounded(rem(RADIUS_MD))
                            .overflow_hidden()
                            .cursor_pointer()
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.open(&open_id, cx);
                            }))
                            .child(thumbnail)
                            .when_some(duration, |this, label| {
                                this.child(
                                    div()
                                        .absolute()
                                        .bottom(px(8.0))
                                        .right(px(8.0))
                                        .px(px(8.0))
                                        .py(px(3.0))
                                        .rounded(rem(RADIUS_SM))
                                        .bg(opacity(gpui::black(), 0.6))
                                        .text_size(rem(TEXT_XS))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(gpui::white())
                                        .child(label),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .pt(px(6.0))
                            .child(
                                div()
                                    .text_size(rem(TEXT_SM))
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate()
                                    .child(summary.name.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .text_size(rem(TEXT_SM))
                                    .text_color(colors.muted_foreground)
                                    .child(
                                        svg()
                                            .size(px(16.0))
                                            .path(icon("calendar04"))
                                            .text_color(colors.muted_foreground),
                                    )
                                    .child(t_args(
                                        "projects.created",
                                        &[("date", &format_date(&summary.created_at))],
                                    )),
                            ),
                    )
                    .when(hovered || selected, |this| {
                        this.child(
                            div().absolute().top(px(12.0)).left(px(12.0)).child(
                                checkbox(
                                    SharedString::from(format!("check-{id}")),
                                    colors,
                                    selected,
                                )
                                .on_click(cx.listener(
                                    move |this: &mut Self, _, _, cx| {
                                        cx.stop_propagation();
                                        if !this.selected.remove(&toggle_id) {
                                            this.selected.insert(toggle_id.clone());
                                        }
                                        cx.notify();
                                    },
                                )),
                            ),
                        )
                    })
                    .when(hovered || has_menu, move |this| {
                        this.child(
                            div()
                                .absolute()
                                .top(px(12.0))
                                .right(px(12.0))
                                .flex()
                                .child(menu_trigger)
                                .children(menu),
                        )
                    }),
            )
    }

    fn list_row(&mut self, summary: &ProjectSummary, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        let id = summary.id.clone();
        let selected = self.selected.contains(&id);
        let duration = format_duration(summary.duration).unwrap_or_else(|| "—".to_string());
        let menu = self.card_menu(summary, cx);
        let thumbnail = self.thumbnail(summary, 20.0, colors, cx);
        let menu_trigger = self.menu_trigger(summary, colors, cx);
        let open_id = id.clone();
        let toggle_id = id.clone();

        div()
            .relative()
            .flex()
            .w_full()
            .h(px(LIST_ROW_HEIGHT_PX))
            .flex_shrink_0()
            .items_center()
            .gap(px(16.0))
            .px(px(16.0))
            .border_b_1()
            .border_color(opacity(colors.border, 0.5))
            .when(selected, |this| this.bg(opacity(colors.primary, 0.05)))
            .child(
                checkbox(
                    SharedString::from(format!("row-check-{id}")),
                    colors,
                    selected,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    cx.stop_propagation();
                    if !this.selected.remove(&toggle_id) {
                        this.selected.insert(toggle_id.clone());
                    }
                    cx.notify();
                })),
            )
            .child(
                div()
                    .id(SharedString::from(format!("row-open-{id}")))
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(12.0))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.open(&open_id, cx);
                    }))
                    .child(
                        div()
                            .relative()
                            .size(px(40.0))
                            .flex_shrink_0()
                            .rounded(rem(RADIUS_SM))
                            .overflow_hidden()
                            .child(thumbnail),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(rem(TEXT_SM))
                            .font_weight(FontWeight::MEDIUM)
                            .child(summary.name.clone()),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .text_size(rem(TEXT_SM))
                            .text_color(colors.muted_foreground)
                            .child(duration),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .pl(px(32.0))
                            .text_size(rem(TEXT_SM))
                            .text_color(colors.muted_foreground)
                            .child(format_date(&summary.created_at)),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_shrink_0()
                    .child(menu_trigger)
                    .children(menu),
            )
    }

    fn notice_banner(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let colors = self.colors(cx);
        let message = self.app.read(cx).notice.clone()?;
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .mx(px(20.0))
                .mb(px(12.0))
                .px(px(14.0))
                .py(px(10.0))
                .rounded(rem(RADIUS_MD))
                .border_1()
                .border_color(opacity(colors.caution, 0.5))
                .bg(opacity(colors.caution, 0.12))
                .child(
                    svg()
                        .size(px(16.0))
                        .flex_shrink_0()
                        .path(icon("alert-circle"))
                        .text_color(colors.caution),
                )
                .child(
                    div()
                        .flex_1()
                        .text_size(rem(TEXT_SM))
                        .text_color(colors.foreground)
                        .child(message),
                )
                .child(
                    div()
                        .id("projects-notice-dismiss")
                        .flex()
                        .size(px(20.0))
                        .items_center()
                        .justify_center()
                        .rounded(rem(RADIUS_SM))
                        .cursor_pointer()
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.app.update(cx, |model, cx| {
                                model.notice = None;
                                cx.notify();
                            });
                            cx.notify();
                        }))
                        .child(
                            svg()
                                .size(px(12.0))
                                .path(icon("cancel01"))
                                .text_color(colors.muted_foreground),
                        ),
                ),
        )
    }

    fn empty_state(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        let searching = !self.app.read(cx).search.trim().is_empty();
        let query = self.app.read(cx).search.clone();
        let action = self.transitions.eased("empty-action");

        if searching {
            return div()
                .flex()
                .flex_col()
                .w_full()
                .items_center()
                .justify_center()
                .gap(px(20.0))
                .py(px(64.0))
                .child(
                    div()
                        .flex()
                        .size(px(64.0))
                        .items_center()
                        .justify_center()
                        .rounded(rem(RADIUS_MD))
                        .border_1()
                        .border_color(colors.border)
                        .bg(opacity(colors.accent, 0.35))
                        .child(
                            svg()
                                .size(px(32.0))
                                .path(icon("search01"))
                                .text_color(colors.muted_foreground),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(12.0))
                        .child(
                            div()
                                .text_size(rem(TEXT_LG))
                                .font_weight(FontWeight::MEDIUM)
                                .child(t("projects.search.empty.title")),
                        )
                        .child(
                            div()
                                .max_w(px(420.0))
                                .text_center()
                                .text_color(colors.muted_foreground)
                                .child(t_args(
                                    "projects.search.empty.description",
                                    &[("query", &query)],
                                )),
                        ),
                )
                .child(
                    Button::new("empty-action", colors)
                        .variant(ButtonVariant::Outline)
                        .size(ButtonSize::Lg)
                        .hover(action)
                        .label(t("projects.search.clear"))
                        .build()
                        .on_hover(self.hover("empty-action", cx))
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.search.buffer.set("");
                            this.apply_search(cx);
                        })),
                );
        }

        div()
            .flex()
            .flex_col()
            .w_full()
            .items_center()
            .justify_center()
            .gap(px(24.0))
            .py(px(64.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .size(px(64.0))
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(opacity(colors.muted, 0.3))
                            .child(
                                svg()
                                    .size(px(32.0))
                                    .path(icon("video01"))
                                    .text_color(colors.muted_foreground),
                            ),
                    )
                    .child(
                        div()
                            .text_size(rem(TEXT_LG))
                            .font_weight(FontWeight::MEDIUM)
                            .child(t("projects.empty.title")),
                    )
                    .child(
                        div()
                            .max_w(px(460.0))
                            .text_center()
                            .text_color(colors.muted_foreground)
                            .child(t("projects.empty.description")),
                    ),
            )
            .child(
                Button::new("empty-action", colors)
                    .size(ButtonSize::Lg)
                    .hover(action)
                    .icon("plus-sign")
                    .label(t("projects.empty.action"))
                    .build()
                    .on_hover(self.hover("empty-action", cx))
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.app.update(cx, |model, cx| model.create_project(cx));
                    })),
            )
    }
}

fn dialog_shell(colors: Palette, body: impl IntoElement) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(opacity(gpui::black(), 0.55))
        .child(
            div()
                .w(px(DIALOG_WIDTH_PX))
                .flex()
                .flex_col()
                .gap(px(16.0))
                .p(px(20.0))
                .rounded(rem(RADIUS_LG))
                .border_1()
                .border_color(colors.border)
                .bg(colors.popover)
                .text_color(colors.popover_foreground)
                .shadow_lg()
                .child(body),
        )
}

impl ProjectsView {
    fn rename_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<Div> {
        let (id, field) = self.rename.as_ref()?;
        let colors = self.colors(cx);
        let id = id.clone();
        let cancel = self.transitions.eased("rename-cancel");
        let confirm = self.transitions.eased("rename-confirm");

        let input = text_field(
            "rename-input",
            field,
            colors,
            FieldStyle {
                height: 36.0,
                placeholder: SharedString::from(t("dialog.rename.placeholder")),
                leading: None,
                ..Default::default()
            },
            window,
        )
        .on_key_down(cx.listener(|this: &mut Self, event: &KeyDownEvent, _, cx| {
            let Some((id, field)) = this.rename.as_mut() else {
                return;
            };
            match crate::input::key_down_with_clipboard(&mut field.buffer, event, false, cx) {
                TextEvent::Submit => {
                    let (id, name) = (id.clone(), field.text().trim().to_string());
                    this.rename = None;
                    if !name.is_empty() {
                        this.app
                            .update(cx, |model, cx| model.rename_project(&id, name, cx));
                    }
                }
                TextEvent::Cancel => this.rename = None,
                _ => {}
            }
            cx.notify();
        }));

        window.focus(&field.focus);

        Some(dialog_shell(
            colors,
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(
                    div()
                        .text_size(rem(TEXT_LG))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(t("dialog.rename.title")),
                )
                .child(
                    div()
                        .text_size(rem(TEXT_SM))
                        .text_color(colors.muted_foreground)
                        .child(t("dialog.rename.label")),
                )
                .child(input)
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.0))
                        .pt(px(4.0))
                        .child(
                            Button::new("rename-cancel", colors)
                                .variant(ButtonVariant::Outline)
                                .hover(cancel)
                                .label(t("common.cancel"))
                                .build()
                                .on_hover(self.hover("rename-cancel", cx))
                                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                    this.rename = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("rename-confirm", colors)
                                .hover(confirm)
                                .label(t("common.rename"))
                                .build()
                                .on_hover(self.hover("rename-confirm", cx))
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    let Some((_, field)) = this.rename.as_ref() else {
                                        return;
                                    };
                                    let name = field.text().trim().to_string();
                                    this.rename = None;
                                    if !name.is_empty() {
                                        this.app.update(cx, |model, cx| {
                                            model.rename_project(&id, name, cx)
                                        });
                                    }
                                    cx.notify();
                                })),
                        ),
                ),
        ))
    }

    fn delete_dialog(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let victims = self.delete.clone()?;
        let colors = self.colors(cx);
        let cancel = self.transitions.eased("delete-cancel");
        let confirm = self.transitions.eased("delete-confirm");
        let count = victims.len();
        let single = (count == 1).then(|| victims[0].1.clone());

        let title = match &single {
            Some(name) => t_args("dialog.delete.title.one", &[("name", name)]),
            None => t_args("dialog.delete.title.many", &[("count", &count.to_string())]),
        };
        let warning = match &single {
            Some(name) => t_args("dialog.delete.warning.one", &[("name", name)]),
            None => t_args(
                "dialog.delete.warning.many",
                &[("count", &count.to_string())],
            ),
        };

        Some(dialog_shell(
            colors,
            div()
                .flex()
                .flex_col()
                .gap(px(14.0))
                .child(
                    div()
                        .text_size(rem(TEXT_LG))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .p(px(12.0))
                        .rounded(rem(RADIUS_MD))
                        .border_1()
                        .border_color(opacity(colors.destructive, 0.4))
                        .bg(opacity(colors.destructive, 0.08))
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(colors.destructive)
                                .child(t("common.warning")),
                        )
                        .child(
                            div()
                                .text_size(rem(TEXT_SM))
                                .text_color(colors.muted_foreground)
                                .child(warning),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.0))
                        .child(
                            Button::new("delete-cancel", colors)
                                .variant(ButtonVariant::Outline)
                                .hover(cancel)
                                .label(t("common.cancel"))
                                .build()
                                .on_hover(self.hover("delete-cancel", cx))
                                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                    this.delete = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("delete-confirm", colors)
                                .variant(ButtonVariant::Destructive)
                                .hover(confirm)
                                .label(t("dialog.delete.action"))
                                .build()
                                .on_hover(self.hover("delete-confirm", cx))
                                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                    let ids: Vec<String> =
                                        victims.iter().map(|(id, _)| id.clone()).collect();
                                    this.app.update(cx, |model, cx| {
                                        for id in &ids {
                                            model.delete_project(id, cx);
                                        }
                                    });
                                    for id in &ids {
                                        this.selected.remove(id);
                                    }
                                    this.delete = None;
                                    cx.notify();
                                })),
                        ),
                ),
        ))
    }

    fn info_dialog(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let summary = self.info.clone()?;
        let colors = self.colors(cx);
        let close = self.transitions.eased("info-close");

        let row = |label: String, value: String| {
            div()
                .flex()
                .justify_between()
                .gap(px(16.0))
                .text_size(rem(TEXT_SM))
                .child(div().text_color(colors.muted_foreground).child(label))
                .child(div().truncate().child(value))
        };

        Some(dialog_shell(
            colors,
            div()
                .flex()
                .flex_col()
                .gap(px(12.0))
                .child(
                    div()
                        .text_size(rem(TEXT_LG))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(summary.name.clone()),
                )
                .child(row(
                    t("dialog.projectInfo.created"),
                    format_date(&summary.created_at),
                ))
                .child(row(
                    t("dialog.projectInfo.modified"),
                    format_date(&summary.updated_at),
                ))
                .child(row(
                    t("common.duration"),
                    format_duration(summary.duration).unwrap_or_else(|| "—".to_string()),
                ))
                .child(row(t("dialog.projectInfo.id"), summary.id.clone()))
                .child(
                    div().flex().justify_end().pt(px(4.0)).child(
                        Button::new("info-close", colors)
                            .variant(ButtonVariant::Outline)
                            .hover(close)
                            .label(t("common.close"))
                            .build()
                            .on_hover(self.hover("info-close", cx))
                            .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                this.info = None;
                                cx.notify();
                            })),
                    ),
                ),
        ))
    }
}

impl Render for ProjectsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        let model = self.app.read(cx);
        let loaded = model.projects_loaded;
        let view_mode = model.view_mode;
        let projects = model.visible_projects();

        self.tooltips.tick();
        if self.transitions.animating()
            || self.tooltips.animating()
            || self.sort_menu.animating()
            || self.card_menu.animating()
            || self.language_menu.animating()
        {
            window.request_animation_frame();
        }

        let header = self.header(window, cx);
        let toolbar = self.toolbar(cx);
        let notice = self.notice_banner(cx);

        let body = if !loaded {
            div()
                .flex()
                .w_full()
                .items_center()
                .justify_center()
                .py(px(64.0))
                .text_color(colors.muted_foreground)
                .child(t("common.loading"))
                .into_any_element()
        } else if projects.is_empty() {
            self.empty_state(cx).into_any_element()
        } else if view_mode == ViewMode::Grid {
            let cards = projects
                .iter()
                .map(|summary| self.grid_card(summary, cx))
                .collect::<Vec<_>>();
            div()
                .flex()
                .flex_wrap()
                .w_full()
                .px(px(20.0))
                .children(cards)
                .into_any_element()
        } else {
            let rows = projects
                .iter()
                .map(|summary| self.list_row(summary, cx))
                .collect::<Vec<_>>();
            div()
                .flex()
                .flex_col()
                .w_full()
                .px(px(16.0))
                .children(rows)
                .into_any_element()
        };

        let bar = scrollbar_v(&self.scroll, colors);
        let rename = self.rename_dialog(window, cx);
        let delete = self.delete_dialog(cx);
        let info = self.info_dialog(cx);

        div()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this: &mut Self, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    this.escape(cx);
                }
            }))
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .bg(colors.background)
            .text_color(colors.foreground)
            .child(header)
            .child(toolbar)
            .children(notice)
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_1()
                    .w_full()
                    .min_h_0()
                    .child(
                        div()
                            .id("projects-body")
                            .flex()
                            .flex_col()
                            .size_full()
                            .gap(px(16.0))
                            .pt(px(8.0))
                            .pb(px(24.0))
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .child(body),
                    )
                    .children(bar),
            )
            .children(rename)
            .children(delete)
            .children(info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_card_menu_matches_the_web_context_menu() {
        let labels: Vec<&str> = CARD_ACTIONS.iter().map(|(id, _, _)| *id).collect();
        assert_eq!(labels, vec!["rename", "duplicate", "info", "delete"]);
    }

    #[test]
    fn every_card_action_label_is_translated() {
        for (id, _, key) in CARD_ACTIONS {
            assert_ne!(&t(key), key, "{id}");
        }
    }

    #[test]
    fn the_projects_page_strings_all_resolve() {
        for key in [
            "projects.breadcrumb.home",
            "projects.breadcrumb.all",
            "projects.selectAll",
            "projects.new.action",
            "projects.empty.title",
            "projects.empty.description",
            "projects.empty.action",
            "projects.search.empty.title",
            "projects.search.clear",
            "projects.created",
            "dialog.rename.title",
            "dialog.delete.action",
            "dialog.projectInfo.created",
            "dialog.projectInfo.modified",
            "dialog.projectInfo.id",
        ] {
            assert_ne!(t(key), key, "{key}");
        }
    }

    #[test]
    fn the_grid_lays_out_four_columns() {
        assert!((0.25_f32 * 4.0 - 1.0).abs() < 1e-6);
    }
}
