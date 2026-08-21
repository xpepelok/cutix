use gpui::{
    div, prelude::*, px, svg, Context, Entity, FontWeight, MouseButton, MouseDownEvent, Window,
};

use crate::assets::icon;
use crate::interaction::{mix, Transitions};
use crate::notify;
use crate::state::{AppModel, Route};
use crate::theme::{opacity, rem, TEXT_XS, TITLEBAR_BUTTON_WIDTH, TITLEBAR_HEIGHT};

pub struct Titlebar {
    app: Entity<AppModel>,
    transitions: Transitions,
}

impl Titlebar {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self {
            app,
            transitions: Transitions::new(),
        }
    }
}

impl Render for Titlebar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let model = self.app.read(cx);
        let colors = model.theme.root;
        let title = match model.route {
            Route::Editor => format!("cutix — {}", model.project_name()),
            Route::Home | Route::Projects | Route::Library => String::from("cutix"),
        };
        let maximized = window.is_maximized();

        let buttons = [
            ("titlebar-minimize", "win-minimize", 14.0, false),
            (
                "titlebar-maximize",
                if maximized {
                    "win-restore"
                } else {
                    "win-maximize"
                },
                12.0,
                false,
            ),
            ("titlebar-close", "win-close", 14.0, true),
        ]
        .into_iter()
        .map(|(id, glyph, size, destructive)| {
            let progress = self.transitions.eased(id);
            let target_bg = if destructive {
                colors.destructive
            } else {
                colors.accent
            };
            let target_fg = if destructive {
                colors.destructive_foreground
            } else {
                colors.foreground
            };

            div()
                .id(id)
                .flex()
                .h_full()
                .w(px(TITLEBAR_BUTTON_WIDTH))
                .items_center()
                .justify_center()
                .cursor_pointer()
                .bg(mix(opacity(target_bg, 0.0), target_bg, progress))
                .on_hover(cx.listener(move |this: &mut Self, hovered, _, cx| {
                    this.transitions.set(id, *hovered);
                    cx.notify();
                }))
                .on_click(cx.listener(move |_, _, window: &mut Window, cx| {
                    match id {
                        "titlebar-minimize" => {
                            if !notify::minimize_own_window() {
                                window.minimize_window();
                            }
                        }
                        "titlebar-maximize" => {
                            if !notify::toggle_window_maximized() {
                                window.zoom_window();
                            }
                        }
                        _ => window.remove_window(),
                    }
                    cx.notify();
                }))
                .child(svg().size(px(size)).path(icon(glyph)).text_color(mix(
                    colors.muted_foreground,
                    target_fg,
                    progress,
                )))
        })
        .collect::<Vec<_>>();

        if self.transitions.animating() {
            window.request_animation_frame();
        }

        div()
            .flex()
            .h(px(TITLEBAR_HEIGHT))
            .w_full()
            .flex_shrink_0()
            .items_center()
            .justify_between()
            .bg(colors.background)
            .text_color(colors.foreground)
            .border_b_1()
            .border_color(colors.border)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .h_full()
                    .items_center()
                    .gap(px(8.0))
                    .pl(px(12.0))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, event: &MouseDownEvent, window: &mut Window, cx| {
                            if event.click_count >= 2 {
                                if !notify::toggle_window_maximized() {
                                    window.zoom_window();
                                }
                            } else if !notify::begin_window_drag() {
                                window.start_window_move();
                            }
                            cx.notify();
                        }),
                    )
                    .child(
                        svg()
                            .size(px(16.0))
                            .flex_shrink_0()
                            .path(icon("cutix-logo"))
                            .text_color(colors.foreground),
                    )
                    .child(
                        div()
                            .text_size(rem(TEXT_XS))
                            .font_weight(FontWeight::MEDIUM)
                            .child(title),
                    ),
            )
            .child(div().flex().h_full().children(buttons))
    }
}
