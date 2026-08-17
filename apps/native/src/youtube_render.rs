use super::*;
use crate::input::{text_field, FieldStyle};
use crate::theme::{
    opacity, RADIUS_LG, RADIUS_MD, RADIUS_SM, TEXT_BASE, TEXT_LG, TEXT_SM, TEXT_XS,
};
use cutix_i18n::{t, t_args};
use gpui::{
    div, prelude::FluentBuilder, px, rems, Context, Div, InteractiveElement, IntoElement,
    ParentElement, Render, SharedString, Stateful, StatefulInteractiveElement, Styled, StyledImage,
    Window,
};

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    StartSetup,

    AddAccount,

    SubmitSignIn,

    Publish,

    CancelTask(String),

    RetryTask(String),

    ForgetTask(String),

    ChooseAccount(String),

    DismissPublished,

    CloseSession,

    OpenVideo(String),

    TogglePreview,
    ToggleSound,
    SetVolume(f32),
    HoverVolume(bool),

    SeekPreview(f32),

    Dismiss,
}

pub trait Host: Render + Sized {
    fn youtube(&mut self) -> &mut Youtube;

    fn youtube_action(&mut self, action: Action, cx: &mut Context<Self>);
}

pub fn action_chip<V: Host>(
    id: impl Into<SharedString>,
    colors: Palette,
    text: String,
    selected: bool,
    action: Action,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let id = id.into();
    div()
        .id(id)
        .px(px(9.0))
        .py(px(4.0))
        .rounded(rems(RADIUS_SM))
        .cursor_pointer()
        .text_size(rems(TEXT_XS))
        .text_color(if selected {
            colors.primary_foreground
        } else {
            colors.foreground
        })
        .bg(if selected {
            colors.primary
        } else {
            opacity(colors.accent, 0.6)
        })
        .child(text)
        .on_click(cx.listener(move |this: &mut V, _, _, cx| {
            this.youtube_action(action.clone(), cx);
            cx.notify();
        }))
}

pub fn overlay(window: &Window, body: Div) -> gpui::AnyElement {
    use gpui::IntoElement;
    let size = window.viewport_size();
    gpui::deferred(
        gpui::anchored()
            .position(gpui::point(px(0.0), px(0.0)))
            .child(div().w(size.width).h(size.height).child(body)),
    )
    .with_priority(3)
    .into_any_element()
}

pub fn label(colors: Palette, text: String) -> Div {
    div()
        .text_size(rems(TEXT_XS))
        .text_color(colors.muted_foreground)
        .child(text)
}

pub fn heading(colors: Palette, text: String) -> Div {
    div()
        .text_size(rems(TEXT_BASE))
        .text_color(colors.foreground)
        .child(text)
}

pub fn field_style(placeholder: String) -> FieldStyle {
    FieldStyle {
        height: 28.0,
        placeholder: SharedString::from(placeholder),
        ..FieldStyle::default()
    }
}

pub fn input<V: Host>(
    id: impl Into<SharedString>,
    field: &TextField,
    colors: Palette,
    style: FieldStyle,
    submit: Option<Action>,
    pick: impl Fn(&mut Youtube) -> Option<&mut TextField> + 'static,
    window: &Window,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let multiline = style.multiline;

    text_field(id, field, colors, style, window).on_key_down(cx.listener(
        move |this: &mut V, event: &gpui::KeyDownEvent, window: &mut Window, cx| {
            let outcome = match pick(this.youtube()) {
                Some(field) => {
                    crate::input::key_down_with_clipboard(&mut field.buffer, event, multiline, cx)
                }
                None => return,
            };
            match outcome {
                crate::input::TextEvent::Submit => {
                    if let Some(action) = submit.clone() {
                        this.youtube_action(action, cx);
                    }
                }
                crate::input::TextEvent::Cancel => window.blur(),
                _ => {}
            }
            cx.notify();
        },
    ))
}

fn cell<V: Host>(
    id: String,
    colors: Palette,
    text: String,
    selected: bool,
    press: impl Fn(&mut Youtube) + 'static,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(id))
        .flex_1()
        .h(px(26.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(rems(RADIUS_SM))
        .cursor_pointer()
        .text_size(rems(TEXT_XS))
        .text_color(if selected {
            colors.primary_foreground
        } else {
            colors.foreground
        })
        .bg(if selected {
            colors.primary
        } else {
            opacity(colors.accent, 0.0)
        })
        .child(text)
        .on_click(cx.listener(move |this: &mut V, _, _, cx| {
            press(this.youtube());
            cx.notify();
        }))
}

fn stepper<V: Host>(
    id: &str,
    colors: Palette,
    glyph: &'static str,
    shift: impl Fn(&mut Scheduled) + 'static,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    div()
        .id(SharedString::from(id.to_string()))
        .size(px(22.0))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .rounded(rems(RADIUS_SM))
        .cursor_pointer()
        .bg(opacity(colors.accent, 0.5))
        .child(
            gpui::svg()
                .size(px(11.0))
                .path(crate::assets::icon(glyph))
                .text_color(colors.foreground),
        )
        .on_click(cx.listener(move |this: &mut V, _, _, cx| {
            if let Some(when) = this
                .youtube()
                .form
                .as_mut()
                .and_then(|form| form.schedule.as_mut())
            {
                shift(when);
            }
            cx.notify();
        }))
}

pub fn schedule_picker<V: Host>(
    form: &PublishForm,
    colors: Palette,
    now: i64,
    cx: &mut Context<V>,
) -> Div {
    let chosen = form.schedule;
    let open = form.schedule_open;

    let summary = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .w_full()
        .child(
            div()
                .id("yt-schedule-toggle")
                .flex_1()
                .min_w_0()
                .px(px(10.0))
                .py(px(5.0))
                .rounded(rems(RADIUS_MD))
                .border_1()
                .border_color(if open { colors.ring } else { colors.border })
                .bg(colors.input)
                .cursor_pointer()
                .text_size(rems(TEXT_XS))
                .text_color(if chosen.is_some() {
                    colors.foreground
                } else {
                    colors.muted_foreground
                })
                .child(match chosen {
                    Some(when) => when.label(),
                    None => t("youtube.publish.schedule.none"),
                })
                .on_click(cx.listener(move |this: &mut V, _, _, cx| {
                    if let Some(form) = this.youtube().form.as_mut() {
                        form.schedule_open = !form.schedule_open;
                        if form.schedule_open && form.schedule.is_none() {
                            form.schedule = Some(Scheduled::soon(now));
                        }
                    }
                    cx.notify();
                })),
        )
        .children(chosen.map(|_| {
            chip(
                "yt-schedule-clear",
                colors,
                t("common.cancel"),
                false,
                |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.schedule = None;
                        form.schedule_open = false;
                    }
                },
                cx,
            )
        }));

    let Some(when) = chosen.filter(|_| open) else {
        return div().flex().flex_col().gap(px(6.0)).w_full().child(summary);
    };

    let heading_row = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .w_full()
        .child(stepper(
            "yt-schedule-prev",
            colors,
            "chevron-left",
            move |when| {
                let (year, month) = crate::calendar::previous_month(when.year, when.month);
                *when = when.with_month(year, month);
            },
            cx,
        ))
        .child(
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(rems(TEXT_XS))
                .text_color(colors.foreground)
                .child(format!(
                    "{} {}",
                    t(&crate::calendar::month_key(when.month)),
                    when.year
                )),
        )
        .child(stepper(
            "yt-schedule-next",
            colors,
            "chevron-right",
            move |when| {
                let (year, month) = crate::calendar::next_month(when.year, when.month);
                *when = when.with_month(year, month);
            },
            cx,
        ));

    let weekdays = div().flex().w_full().gap(px(2.0)).children(
        (0..7)
            .map(|index| {
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(rems(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(t(&crate::calendar::weekday_key(index)))
            })
            .collect::<Vec<_>>(),
    );

    let grid = crate::calendar::month_grid(when.year, when.month);
    let mut rows = Vec::new();
    for week in grid.chunks(7) {
        let mut row = div().flex().w_full().gap(px(2.0));
        for slot in week {
            row = match slot {
                Some(day) => {
                    let day = *day;
                    row.child(cell(
                        format!("yt-schedule-day-{day}"),
                        colors,
                        day.to_string(),
                        day == when.day,
                        move |state| {
                            if let Some(when) =
                                state.form.as_mut().and_then(|form| form.schedule.as_mut())
                            {
                                when.day = day;
                            }
                        },
                        cx,
                    ))
                }

                None => row.child(div().flex_1().h(px(26.0))),
            };
        }
        rows.push(row);
    }

    let clock = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .w_full()
        .child(label(colors, t("youtube.publish.schedule.time")))
        .child(div().flex_1())
        .child(stepper(
            "yt-schedule-hour-down",
            colors,
            "arrow-down",
            |when| when.hour = (when.hour + 23) % 24,
            cx,
        ))
        .child(
            div()
                .w(px(46.0))
                .flex()
                .items_center()
                .justify_center()
                .text_size(rems(TEXT_XS))
                .text_color(colors.foreground)
                .child(format!("{:02}:{:02}", when.hour, when.minute)),
        )
        .child(stepper(
            "yt-schedule-hour-up",
            colors,
            "arrow-down",
            |when| when.hour = (when.hour + 1) % 24,
            cx,
        ))
        .child(stepper(
            "yt-schedule-minute",
            colors,
            "plus-sign",
            |when| when.minute = (when.minute + 5) % 60,
            cx,
        ));

    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .w_full()
        .child(summary)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .w_full()
                .p(px(8.0))
                .rounded(rems(RADIUS_MD))
                .border_1()
                .border_color(colors.border)
                .bg(opacity(colors.accent, 0.25))
                .child(heading_row)
                .child(weekdays)
                .children(rows)
                .child(clock),
        )
}

pub fn note(colors: Palette, title: String, body: String, warn: bool) -> Div {
    let tint = if warn { colors.caution } else { colors.border };
    div()
        .flex()
        .flex_col()
        .gap(px(3.0))
        .w_full()
        .p(px(8.0))
        .rounded(rems(RADIUS_MD))
        .border_1()
        .border_color(opacity(tint, 0.6))
        .bg(opacity(tint, 0.08))
        .child(
            div()
                .text_size(rems(TEXT_XS))
                .text_color(colors.foreground)
                .child(title),
        )
        .child(label(colors, body))
}

pub fn link<V: Host>(
    id: impl Into<SharedString>,
    colors: Palette,
    text: String,
    url: String,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    div()
        .id(id.into())
        .cursor_pointer()
        .text_size(rems(TEXT_XS))
        .text_color(colors.primary)
        .child(text)
        .on_click(cx.listener(move |_this: &mut V, _, _, cx| {
            open_url(&url);
            cx.notify();
        }))
}

pub fn checkbox<V: Host>(
    id: impl Into<SharedString>,
    colors: Palette,
    text: String,
    checked: bool,
    toggle: impl Fn(&mut Youtube) + 'static,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    div()
        .id(id.into())
        .flex()
        .items_center()
        .gap(px(6.0))
        .cursor_pointer()
        .child(
            div()
                .size(px(13.0))
                .flex_shrink_0()
                .rounded(rems(RADIUS_SM))
                .border_1()
                .border_color(if checked {
                    colors.primary
                } else {
                    colors.border
                })
                .bg(if checked {
                    colors.primary
                } else {
                    colors.input
                })
                .flex()
                .items_center()
                .justify_center()
                .text_size(rems(0.6))
                .text_color(colors.primary_foreground)
                .child(if checked { "\u{2713}" } else { "" }),
        )
        .child(
            div()
                .text_size(rems(TEXT_XS))
                .text_color(colors.foreground)
                .child(text),
        )
        .on_click(cx.listener(move |this: &mut V, _, _, cx| {
            toggle(this.youtube());
            cx.notify();
        }))
}

pub fn chip<V: Host>(
    id: impl Into<SharedString>,
    colors: Palette,
    text: String,
    selected: bool,
    pick: impl Fn(&mut Youtube) + 'static,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    div()
        .id(id.into())
        .px(px(9.0))
        .py(px(4.0))
        .rounded(rems(RADIUS_SM))
        .cursor_pointer()
        .text_size(rems(TEXT_XS))
        .text_color(if selected {
            colors.foreground
        } else {
            colors.muted_foreground
        })
        .bg(if selected {
            opacity(colors.accent, 0.9)
        } else {
            opacity(colors.accent, 0.35)
        })
        .child(text)
        .on_click(cx.listener(move |this: &mut V, _, _, cx| {
            pick(this.youtube());
            cx.notify();
        }))
}

pub fn progress_bar(colors: Palette, fraction: f32) -> Div {
    div()
        .w_full()
        .h(px(4.0))
        .rounded(px(2.0))
        .overflow_hidden()
        .bg(opacity(colors.muted, 0.5))
        .child(
            div()
                .h_full()
                .w(gpui::relative(fraction.clamp(0.0, 1.0)))
                .bg(colors.primary),
        )
}

pub fn modal(colors: Palette, id: impl Into<SharedString>, body: Div) -> Div {
    modal_sized(colors, id, body, 430.0)
}

static CHROMELESS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_chromeless(value: bool) {
    CHROMELESS.store(value, std::sync::atomic::Ordering::Relaxed);
}

static UPLOADING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_uploading(value: bool) {
    UPLOADING.store(value, std::sync::atomic::Ordering::Relaxed);
}

fn uploading() -> bool {
    UPLOADING.load(std::sync::atomic::Ordering::Relaxed)
}

fn chromeless() -> bool {
    CHROMELESS.load(std::sync::atomic::Ordering::Relaxed)
}

fn window_buttons(colors: Palette) -> Div {
    let button = |id: &'static str, glyph: &'static str, size: f32, destructive: bool| {
        div()
            .id(id)
            .size(px(26.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(rems(RADIUS_SM))
            .cursor_pointer()
            .hover(move |style| {
                style.bg(if destructive {
                    colors.destructive
                } else {
                    colors.accent
                })
            })
            .child(
                gpui::svg()
                    .size(px(size))
                    .path(crate::assets::icon(glyph))
                    .text_color(colors.foreground),
            )
    };

    div()
        .absolute()
        .top(px(8.0))
        .right(px(8.0))
        .flex()
        .items_center()
        .gap(px(2.0))
        .child(
            button("yt-window-minimize", "win-minimize", 13.0, false).on_click(
                |_, window: &mut Window, _: &mut gpui::App| {
                    window.minimize_window();
                    crate::notify::minimize_own_window();
                },
            ),
        )
        .child(button("yt-window-close", "win-close", 13.0, true).on_click(
            |_, window: &mut Window, _: &mut gpui::App| {
                if uploading() {
                    window.minimize_window();
                    crate::notify::minimize_own_window();
                } else {
                    window.remove_window();
                }
            },
        ))
}

pub fn modal_sized(colors: Palette, id: impl Into<SharedString>, body: Div, width: f32) -> Div {
    let id = id.into();
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .when(!chromeless(), |this| this.bg(opacity(gpui::black(), 0.55)))
        .occlude()
        .child(
            div()
                .relative()
                .when(!chromeless(), |this| this.w(px(width)))
                .when(chromeless(), |this| this.w_full().h_full().justify_center())
                .when(!chromeless(), |this| this.max_h(gpui::relative(0.86)))
                .flex()
                .flex_col()
                .gap(px(12.0))
                .when(!chromeless(), |this| {
                    this.rounded(rems(RADIUS_LG))
                        .border_1()
                        .border_color(colors.border)
                })
                .p(px(18.0))
                .bg(colors.popover)
                .text_color(colors.popover_foreground)
                .when(!chromeless(), |this| this.shadow_lg())
                .child(
                    div()
                        .id(id)
                        .w_full()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(body.w_full()),
                )
                .when(chromeless(), |this| {
                    this.child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right(px(78.0))
                            .h(px(46.0))
                            .on_mouse_down(gpui::MouseButton::Left, |_, window: &mut Window, _| {
                                window.start_window_move();
                                crate::notify::begin_window_drag();
                            }),
                    )
                    .child(window_buttons(colors))
                }),
        )
}

pub fn status_row(state: &Youtube, colors: Palette) -> Div {
    let ready = state.can_upload();
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .text_size(rems(TEXT_XS))
                .text_color(if ready {
                    colors.foreground
                } else {
                    colors.caution
                })
                .child(account_line(&state.accounts)),
        )
        .child(label(
            colors,
            t(if state.is_configured() {
                "youtube.publish.limitHint"
            } else {
                "youtube.error.noBrowser"
            }),
        ))
}

pub fn account_row<V: Host>(
    state: &Youtube,
    colors: Palette,
    account: &youtube::Account,
    cx: &mut Context<V>,
) -> Div {
    let id = account.id.clone();
    let selected = state.accounts.active.as_deref() == Some(id.as_str());
    let select_id = id.clone();
    let remove_id = id.clone();

    let mut row = div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .w_full()
        .p(px(6.0))
        .rounded(rems(RADIUS_MD))
        .bg(if selected {
            opacity(colors.accent, 0.6)
        } else {
            opacity(colors.accent, 0.0)
        })
        .child(account_avatar(
            colors,
            state.avatars.get(&id),
            &account.initials(),
            AVATAR_PX,
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(rems(TEXT_XS))
                        .text_color(colors.foreground)
                        .child(account.display_name()),
                )
                .child(label(colors, account.handle.clone())),
        );

    if account.needs_reauth {
        row = row.child(
            div()
                .text_size(rems(TEXT_XS))
                .text_color(colors.caution)
                .child(t("youtube.accounts.reauth")),
        );
    }

    row.child(chip(
        SharedString::from(format!("yt-use-{select_id}")),
        colors,
        t(if selected {
            "youtube.accounts.active"
        } else {
            "youtube.accounts.use"
        }),
        selected,
        move |state| {
            state.accounts.select(&select_id);
            state.persist();
        },
        cx,
    ))
    .child(chip(
        SharedString::from(format!("yt-remove-{remove_id}")),
        colors,
        t("youtube.accounts.remove"),
        false,
        move |state| state.remove_account(&remove_id),
        cx,
    ))
}

pub fn queue_row<V: Host>(
    state: &Youtube,
    colors: Palette,
    row: &crate::youtube_ui::RowView,
    cx: &mut Context<V>,
) -> Div {
    let initials = state
        .accounts
        .get(&row.account_id)
        .map(|account| account.initials())
        .unwrap_or_else(|| "?".to_string());
    let percent = (row.fraction * 100.0).round() as u32;

    let mut lines = div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap(px(2.0))
        .child(
            div()
                .text_size(rems(TEXT_XS))
                .text_color(colors.foreground)
                .child(row.title.clone()),
        )
        .child(
            div()
                .flex()
                .items_baseline()
                .justify_between()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(rems(TEXT_XS))
                        .text_color(if row.failed {
                            colors.destructive
                        } else {
                            colors.muted_foreground
                        })
                        .child(row.status.clone()),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("yt-queue-percent-{}", row.id)))
                        .text_size(rems(TEXT_XS))
                        .text_color(colors.muted_foreground)
                        .child(format!("{percent}%")),
                ),
        );

    if row.show_bar {
        lines = lines.child(progress_bar(colors, row.fraction));
    }
    if let Some(detail) = row.detail.clone() {
        lines = lines.child(
            div()
                .text_size(rems(TEXT_XS))
                .text_color(colors.destructive)
                .child(detail),
        );
    }

    let mut buttons = div().flex().items_center().gap(px(4.0));
    if row.can_cancel {
        buttons = buttons.child(action_chip(
            SharedString::from(format!("yt-queue-cancel-{}", row.id)),
            colors,
            t("common.cancel"),
            false,
            Action::CancelTask(row.id.clone()),
            cx,
        ));
    }
    if row.can_retry {
        buttons = buttons.child(action_chip(
            SharedString::from(format!("yt-queue-retry-{}", row.id)),
            colors,
            t("youtube.queue.retry"),
            true,
            Action::RetryTask(row.id.clone()),
            cx,
        ));
    }
    if !row.show_bar {
        buttons = buttons.child(action_chip(
            SharedString::from(format!("yt-queue-forget-{}", row.id)),
            colors,
            t("youtube.history.remove"),
            false,
            Action::ForgetTask(row.id.clone()),
            cx,
        ));
    }

    let tint = if row.failed {
        colors.destructive
    } else {
        colors.primary
    };
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .w_full()
        .p(px(6.0))
        .rounded(rems(RADIUS_MD))
        .border_1()
        .border_color(opacity(tint, 0.5))
        .bg(opacity(tint, 0.06))
        .child(account_avatar(
            colors,
            state.avatars.get(&row.account_id),
            &initials,
            20.0,
        ))
        .child(lines)
        .child(buttons)
}

fn row_action<V: Host>(
    id: impl Into<SharedString>,
    colors: Palette,
    glyph: &'static str,
    hovered: bool,
    destructive: bool,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let id = id.into();
    let tint = match (destructive, hovered) {
        (true, true) => colors.destructive,
        (false, true) => colors.foreground,
        _ => colors.muted_foreground,
    };
    div()
        .id(id.clone())
        .size(px(26.0))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(rems(RADIUS_SM))
        .cursor_pointer()
        .bg(opacity(
            if destructive {
                colors.destructive
            } else {
                colors.accent
            },
            if hovered { 0.16 } else { 0.0 },
        ))
        .child(
            gpui::svg()
                .size(px(14.0))
                .path(crate::assets::icon(glyph))
                .text_color(tint),
        )
        .on_hover(cx.listener({
            let id = id.clone();
            move |this: &mut V, is_over: &bool, _, cx| {
                this.youtube().hovered_action = is_over.then(|| id.to_string());
                cx.notify();
            }
        }))
}

pub fn history_row<V: Host>(
    state: &Youtube,
    colors: Palette,
    entry: &HistoryEntry,
    cx: &mut Context<V>,
) -> Div {
    let url = entry.url();
    let video_id = entry.video_id.clone();
    let forget_id = entry.video_id.clone();
    let studio_id = entry.video_id.clone();
    let initials = state
        .accounts
        .get(&entry.account_id)
        .map(|account| account.initials())
        .unwrap_or_else(|| "?".to_string());

    let link_id = format!("yt-link-{video_id}");
    let studio_button = format!("yt-studio-{studio_id}");
    let forget_button = format!("yt-forget-{forget_id}");
    let hovered = |id: &str| state.hovered_action.as_deref() == Some(id);
    let watch_url = url.clone();

    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .w_full()
        .p(px(8.0))
        .rounded(rems(RADIUS_MD))
        .border_1()
        .border_color(opacity(colors.border, 0.7))
        .bg(opacity(colors.card, 0.5))
        .child(account_avatar(
            colors,
            state.avatars.get(&entry.account_id),
            &initials,
            22.0,
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(rems(TEXT_SM))
                        .text_color(colors.foreground)
                        .truncate()
                        .child(entry.title.clone()),
                )
                .child(label(
                    colors,
                    format!(
                        "{} \u{00b7} {} \u{00b7} {}",
                        youtube::iso_date(entry.uploaded_at),
                        t(entry.privacy.message_key()),
                        t(entry.status.message_key())
                    ),
                )),
        )
        .child(
            row_action(
                SharedString::from(link_id.clone()),
                colors,
                "link02",
                hovered(&link_id),
                false,
                cx,
            )
            .on_click(cx.listener(
                move |this: &mut V, event: &gpui::ClickEvent, _, cx| {
                    let held = event.modifiers();
                    if held.control || held.platform {
                        open_url(&watch_url);
                        return;
                    }
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(url.clone()));
                    this.youtube().copied_at = Some(Instant::now());
                    cx.notify();
                },
            )),
        )
        .child(
            row_action(
                SharedString::from(studio_button.clone()),
                colors,
                "edit03",
                hovered(&studio_button),
                false,
                cx,
            )
            .on_click(cx.listener(move |_this: &mut V, _, _, cx| {
                open_url(&publish::studio_url(&studio_id));
                cx.notify();
            })),
        )
        .child(
            row_action(
                SharedString::from(forget_button.clone()),
                colors,
                "win-close",
                hovered(&forget_button),
                true,
                cx,
            )
            .on_click(cx.listener(move |this: &mut V, _, _, cx| {
                let state = this.youtube();
                state.history.remove(&forget_id);
                state.persist();
                cx.notify();
            })),
        )
}

pub fn history_filters<V: Host>(
    state: &Youtube,
    colors: Palette,
    window: &Window,
    cx: &mut Context<V>,
) -> Div {
    let Some(filters) = state.filters.as_ref() else {
        return div();
    };

    let mut boxes = Vec::new();
    for account in &state.accounts.accounts {
        let id = account.id.clone();
        let toggle_id = id.clone();
        let ticked = filters.has(&id);
        boxes.push(
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .child(account_avatar(
                    colors,
                    state.avatars.get(&id),
                    &account.initials(),
                    18.0,
                ))
                .child(checkbox(
                    SharedString::from(format!("yt-filter-{id}")),
                    colors,
                    account.display_name(),
                    ticked,
                    move |state| {
                        if let Some(filters) = state.filters.as_mut() {
                            filters.toggle(&toggle_id);
                        }
                    },
                    cx,
                )),
        );
    }

    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .w_full()
        .child(input(
            "yt-history-search",
            &filters.search,
            colors,
            field_style(t("youtube.history.search")),
            None,
            |state| state.filters.as_mut().map(|filters| &mut filters.search),
            window,
            cx,
        ))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(label(colors, t("youtube.history.from")))
                .child(div().flex_1().child(input(
                    "yt-history-from",
                    &filters.from,
                    colors,
                    field_style(t("youtube.history.date.placeholder")),
                    None,
                    |state| state.filters.as_mut().map(|filters| &mut filters.from),
                    window,
                    cx,
                )))
                .child(label(colors, t("youtube.history.to")))
                .child(div().flex_1().child(input(
                    "yt-history-to",
                    &filters.to,
                    colors,
                    field_style(t("youtube.history.date.placeholder")),
                    None,
                    |state| state.filters.as_mut().map(|filters| &mut filters.to),
                    window,
                    cx,
                ))),
        )
        .child(label(colors, t("youtube.history.accounts")))
        .child(div().flex().flex_wrap().gap(px(8.0)).children(boxes))
        .child(chip(
            "yt-history-clear",
            colors,
            t("youtube.history.clear"),
            filters.is_active(),
            |state| {
                if let Some(filters) = state.filters.as_mut() {
                    filters.clear();
                }
            },
            cx,
        ))
}

pub fn settings_body<V: Host>(
    state: &mut Youtube,
    colors: Palette,
    _now: i64,
    window: &Window,
    cx: &mut Context<V>,
) -> Div {
    let configured = state.is_configured();

    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .child(heading(colors, t("youtube.title")))
        .child(label(colors, t("youtube.subtitle")));

    if !configured {
        return body
            .child(note(
                colors,
                t("youtube.error.noBrowser"),
                t("youtube.signIn.installChrome"),
                true,
            ))
            .child(link(
                "yt-panel-get-chrome",
                colors,
                t("youtube.signIn.getChrome"),
                youtube::CHROME_DOWNLOAD_URL.to_string(),
                cx,
            ));
    }

    body = body
        .child(status_row(state, colors))
        .child(heading(colors, t("youtube.accounts.title")));

    if state.accounts.is_empty() {
        body = body.child(label(colors, t("youtube.accounts.none")));
    } else {
        let mut rows = Vec::new();
        for account in &state.accounts.accounts {
            rows.push(account_row(state, colors, account, cx));
        }
        body = body.child(div().flex().flex_col().gap(px(2.0)).children(rows));
    }

    body = body.child(action_chip(
        "yt-account-add",
        colors,
        t(if state.signing_in {
            "youtube.accounts.signingIn"
        } else {
            "youtube.accounts.add"
        }),
        !state.signing_in,
        Action::AddAccount,
        cx,
    ));

    if let Some(notice) = state.notice.clone() {
        body = body.child(
            div()
                .text_size(rems(TEXT_XS))
                .text_color(colors.caution)
                .child(notice),
        );
    }

    body = body
        .child(heading(colors, t("youtube.history.title")))
        .child(label(colors, t("youtube.history.localOnly")));

    let rows = state.rows();
    if !rows.is_empty() {
        let waiting = state.queue.waiting_count();
        let mut queued = Vec::new();
        for row in &rows {
            queued.push(queue_row(state, colors, row, cx));
        }
        body = body
            .child(label(
                colors,
                t_args("youtube.queue.title", &[("count", &rows.len().to_string())]),
            ))
            .child(div().flex().flex_col().gap(px(4.0)).children(queued));
        if waiting > 1 {
            body = body.child(label(colors, t("youtube.queue.sequential")));
        }
    }

    let total = state.history.entries.len();
    if total == 0 {
        return body.child(label(colors, t("youtube.history.empty")));
    }

    let entries: Vec<HistoryEntry> = state.filtered().into_iter().cloned().collect();
    body = body
        .child(history_filters(state, colors, window, cx))
        .child(label(
            colors,
            t_args(
                "youtube.history.count",
                &[
                    ("shown", &entries.len().to_string()),
                    ("total", &total.to_string()),
                ],
            ),
        ));

    if entries.is_empty() {
        return body.child(label(colors, t("youtube.history.noMatches")));
    }

    let mut rows = Vec::new();
    for entry in &entries {
        rows.push(history_row(state, colors, entry, cx));
    }
    body.child(div().flex().flex_col().gap(px(4.0)).children(rows))
}

pub fn sign_in_dialog<V: Host>(
    state: &Youtube,
    colors: Palette,
    window: &Window,
    cx: &mut Context<V>,
) -> Option<Div> {
    let _ = window;
    let form = state.sign_in_form.as_ref()?;
    let fade = step_progress(form.elapsed());
    let ready = state.is_configured();

    let mut panel = div().flex().flex_col().gap(px(10.0)).w_full().child(
        div()
            .text_size(rems(TEXT_XS))
            .text_color(opacity(colors.foreground, 0.4 + 0.6 * fade))
            .child(t(if form.busy {
                "youtube.signIn.waiting"
            } else {
                "youtube.signIn.body"
            })),
    );

    if form.busy {
        panel = panel
            .child(progress_bar(colors, 0.5))
            .child(label(colors, t("youtube.signIn.waitingHint")));
    } else {
        panel = panel.child(note(
            colors,
            t("youtube.signIn.privacy.title"),
            t("youtube.signIn.privacy.body"),
            false,
        ));
    }

    if !ready {
        panel = panel
            .child(note(
                colors,
                t("youtube.error.noBrowser"),
                t("youtube.signIn.installChrome"),
                true,
            ))
            .child(link(
                "yt-sign-in-get-chrome",
                colors,
                t("youtube.signIn.getChrome"),
                youtube::CHROME_DOWNLOAD_URL.to_string(),
                cx,
            ));
    }

    if let Some(error) = form.error.as_ref() {
        panel = panel.child(
            div()
                .text_size(rems(TEXT_XS))
                .text_color(colors.destructive)
                .child(error.clone()),
        );
    }

    let controls = div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(chip(
            "yt-sign-in-close",
            colors,
            t("common.close"),
            false,
            |state| {
                if let Some(form) = state.sign_in_form.as_ref() {
                    form.abandon();
                }
                state.sign_in_form = None;
            },
            cx,
        ))
        .child(div().flex_1())
        .child(action_chip(
            "yt-sign-in-submit",
            colors,
            t(if form.busy {
                "youtube.signIn.working"
            } else {
                "youtube.signIn.submit"
            }),
            ready && form.can_submit(),
            Action::SubmitSignIn,
            cx,
        ));

    Some(modal(
        colors,
        "yt-sign-in-body",
        div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(rems(TEXT_LG))
                    .child(t("youtube.signIn.title")),
            )
            .child(panel)
            .child(controls),
    ))
}

fn choice_row<V: Host, T: PartialEq + Copy + 'static>(
    prefix: &str,
    colors: Palette,
    options: &[T],
    current: T,
    text: impl Fn(T) -> String,
    set: impl Fn(&mut Youtube, T) + Copy + 'static,
    cx: &mut Context<V>,
) -> Div {
    let mut chips = Vec::new();
    for (index, option) in options.iter().enumerate() {
        let option = *option;
        chips.push(chip(
            SharedString::from(format!("{prefix}-{index}")),
            colors,
            text(option),
            current == option,
            move |state| set(state, option),
            cx,
        ));
    }
    div().flex().flex_wrap().gap(px(4.0)).children(chips)
}

fn advanced_fields<V: Host>(
    form: &PublishForm,
    colors: Palette,
    _now: i64,
    window: &Window,
    cx: &mut Context<V>,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(label(colors, t("youtube.publish.playlists")))
        .child(input(
            "yt-form-playlists",
            &form.playlists,
            colors,
            field_style(t("youtube.publish.playlists.placeholder")),
            None,
            |state| state.form.as_mut().map(|form| &mut form.playlists),
            window,
            cx,
        ))
        .child(label(colors, t("youtube.publish.language")))
        .child(input(
            "yt-form-language",
            &form.video_language,
            colors,
            field_style(t("youtube.publish.language.placeholder")),
            None,
            |state| state.form.as_mut().map(|form| &mut form.video_language),
            window,
            cx,
        ))
        .child(label(colors, t("youtube.publish.license")))
        .child(choice_row(
            "yt-form-license",
            colors,
            &License::ALL,
            form.license,
            |license| t(license.message_key()),
            |state, license| {
                if let Some(form) = state.form.as_mut() {
                    form.license = license;
                }
            },
            cx,
        ))
        .child(label(colors, t("youtube.publish.comments")))
        .child(choice_row(
            "yt-form-comments",
            colors,
            &Comments::ALL,
            form.comments,
            |comments| t(comments.message_key()),
            |state, comments| {
                if let Some(form) = state.form.as_mut() {
                    form.comments = comments;
                }
            },
            cx,
        ))
        .child(label(colors, t("youtube.publish.remix")))
        .child(choice_row(
            "yt-form-remix",
            colors,
            &Remix::ALL,
            form.remix,
            |remix| t(remix.message_key()),
            |state, remix| {
                if let Some(form) = state.form.as_mut() {
                    form.remix = remix;
                }
            },
            cx,
        ))
        .child(checkbox(
            "yt-form-embed",
            colors,
            t("youtube.publish.allowEmbedding"),
            form.allow_embedding,
            |state| {
                if let Some(form) = state.form.as_mut() {
                    form.allow_embedding = !form.allow_embedding;
                }
            },
            cx,
        ))
        .child(checkbox(
            "yt-form-likes",
            colors,
            t("youtube.publish.showLikeCount"),
            form.show_like_count,
            |state| {
                if let Some(form) = state.form.as_mut() {
                    form.show_like_count = !form.show_like_count;
                }
            },
            cx,
        ))
        .child(checkbox(
            "yt-form-promotion",
            colors,
            t("youtube.publish.paidPromotion"),
            form.paid_promotion,
            |state| {
                if let Some(form) = state.form.as_mut() {
                    form.paid_promotion = !form.paid_promotion;
                }
            },
            cx,
        ))
        .child(checkbox(
            "yt-form-altered",
            colors,
            t("youtube.publish.alteredContent"),
            form.altered_content,
            |state| {
                if let Some(form) = state.form.as_mut() {
                    form.altered_content = !form.altered_content;
                }
            },
            cx,
        ))
        .child(label(colors, t("youtube.publish.alteredContent.hint")))
}

pub fn account_picker<V: Host>(
    state: &mut Youtube,
    colors: Palette,
    cx: &mut Context<V>,
) -> Option<Div> {
    if !state.choosing_account {
        return None;
    }
    let file = state
        .pending_publish
        .as_ref()?
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();

    let mut rows = Vec::new();
    for account in &state.accounts.accounts {
        let id = account.id.clone();
        let name = account.display_name();
        let initials = account.initials();
        rows.push(
            div()
                .id(SharedString::from(format!("yt-pick-{id}")))
                .flex()
                .w_full()
                .items_center()
                .gap(px(10.0))
                .h(px(44.0))
                .px(px(10.0))
                .rounded(rems(RADIUS_MD))
                .cursor_pointer()
                .bg(opacity(colors.accent, 0.35))
                .child(
                    div()
                        .size(px(28.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(colors.muted)
                        .text_size(rems(TEXT_XS))
                        .text_color(colors.foreground)
                        .child(initials),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(rems(TEXT_SM))
                        .child(name),
                )
                .on_click(cx.listener(move |this: &mut V, _, _, cx| {
                    this.youtube_action(Action::ChooseAccount(id.clone()), cx);
                })),
        );
    }

    let body = div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(10.0))
        .child(heading(colors, t("youtube.publish.pickAccount")))
        .child(label(colors, file))
        .child(div().flex().flex_col().gap(px(6.0)).children(rows))
        .child(div().flex().justify_end().pt(px(4.0)).child(action_chip(
            "yt-pick-cancel",
            colors,
            t("common.cancel"),
            false,
            Action::Dismiss,
            cx,
        )));

    Some(modal_sized(colors, "yt-pick-body", body, 420.0))
}

#[derive(Debug)]
pub struct PreviewVolumeDrag;

fn preview_volume<V: Host>(
    muted: bool,
    level: f32,
    open: bool,
    cx: &mut Context<V>,
) -> impl IntoElement {
    let slider = div()
        .id("yt-volume-bar")
        .w(px(if open { 70.0 } else { 0.0 }))
        .h(px(16.0))
        .flex()
        .items_center()
        .overflow_hidden()
        .cursor_pointer()
        .on_mouse_down(
            gpui::MouseButton::Left,
            cx.listener(move |this: &mut V, event: &gpui::MouseDownEvent, _, cx| {
                this.youtube_action(Action::SetVolume(event.position.x.into()), cx);
            }),
        )
        .on_drag(PreviewVolumeDrag, |_, _, _, cx| {
            gpui::AppContext::new(cx, |_| gpui::Empty)
        })
        .on_drag_move::<PreviewVolumeDrag>(cx.listener(
            move |this: &mut V, event: &gpui::DragMoveEvent<PreviewVolumeDrag>, _, cx| {
                this.youtube_action(Action::SetVolume(event.event.position.x.into()), cx);
            },
        ))
        .child(
            div()
                .relative()
                .w_full()
                .h(px(4.0))
                .rounded(px(2.0))
                .overflow_hidden()
                .bg(opacity(gpui::white(), 0.3))
                .child(div().h_full().w(gpui::relative(level)).bg(gpui::white()))
                .child(volume_bar_probe(cx)),
        );

    div()
        .id("yt-volume")
        .flex()
        .items_center()
        .gap(px(6.0))
        .on_hover(cx.listener(move |this: &mut V, hovered: &bool, _, cx| {
            this.youtube_action(Action::HoverVolume(*hovered), cx);
        }))
        .child(
            div()
                .id("yt-volume-toggle")
                .size(px(22.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded(rems(RADIUS_SM))
                .cursor_pointer()
                .bg(opacity(gpui::white(), 0.15))
                .child(
                    gpui::svg()
                        .size(px(12.0))
                        .path(crate::assets::icon(if muted {
                            "volume-mute"
                        } else {
                            "volume-high"
                        }))
                        .text_color(gpui::white()),
                )
                .on_click(cx.listener(move |this: &mut V, _, _, cx| {
                    this.youtube_action(Action::ToggleSound, cx);
                })),
        )
        .child(slider)
}

fn volume_bar_probe<V: Host>(cx: &mut Context<V>) -> impl IntoElement {
    let handle = cx.entity();

    gpui::canvas(
        move |bounds, _window, cx| {
            let measured = (f32::from(bounds.origin.x), f32::from(bounds.size.width));
            handle.update(cx, |host: &mut V, _| {
                host.youtube().volume_bar = measured;
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
pub struct PreviewSeekDrag;

fn preview_bar_probe<V: Host>(cx: &mut Context<V>) -> impl IntoElement {
    let handle = cx.entity();

    gpui::canvas(
        move |bounds, _window, cx| {
            let measured = (f32::from(bounds.origin.x), f32::from(bounds.size.width));
            handle.update(cx, |host: &mut V, _| {
                host.youtube().preview_bar = measured;
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

pub fn published_dialog<V: Host>(
    state: &mut Youtube,
    colors: Palette,
    cx: &mut Context<V>,
) -> Option<Div> {
    let title = state.published.clone()?;

    let body = div()
        .flex()
        .flex_col()
        .w_full()
        .items_center()
        .gap(px(10.0))
        .child(heading(colors, t("youtube.published.title")))
        .child(
            div()
                .size(px(56.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(opacity(colors.primary, 0.15))
                .child(
                    gpui::svg()
                        .size(px(30.0))
                        .path(crate::assets::icon("tick02"))
                        .text_color(colors.primary),
                ),
        )
        .child(
            div()
                .text_size(rems(TEXT_BASE))
                .text_color(colors.foreground)
                .child(t("youtube.published.done")),
        )
        .child(
            div()
                .max_w_full()
                .truncate()
                .text_size(rems(TEXT_XS))
                .text_color(colors.muted_foreground)
                .child(title),
        )
        .child(div().pt(px(6.0)).child(action_chip(
            "yt-published-ok",
            colors,
            t("youtube.session.close"),
            true,
            Action::DismissPublished,
            cx,
        )));

    Some(modal_sized(colors, "yt-published-body", body, 360.0))
}

pub fn published_toasts<V: Host>(state: &mut Youtube, colors: Palette, cx: &mut Context<V>) -> Div {
    let cards = state
        .toasts
        .iter()
        .enumerate()
        .map(|(index, toast)| {
            let copied = toast
                .copied_at
                .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(2));
            let url = toast.url.clone();

            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .w(px(320.0))
                .p(px(12.0))
                .rounded(rems(RADIUS_LG))
                .border_1()
                .border_color(colors.border)
                .bg(colors.popover)
                .text_color(colors.popover_foreground)
                .shadow_lg()
                .occlude()
                .child(
                    div()
                        .size(px(28.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(opacity(colors.primary, 0.15))
                        .child(
                            gpui::svg()
                                .size(px(16.0))
                                .path(crate::assets::icon("tick02"))
                                .text_color(colors.primary),
                        ),
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
                                .text_size(rems(TEXT_XS))
                                .child(t("youtube.toast.title")),
                        )
                        .child(
                            div()
                                .truncate()
                                .text_size(rems(TEXT_XS))
                                .text_color(colors.muted_foreground)
                                .child(SharedString::from(toast.title.clone())),
                        ),
                )
                .child(
                    row_action(
                        SharedString::from(format!("yt-toast-copy-{index}")),
                        colors,
                        if copied { "tick02" } else { "copy01" },
                        copied,
                        false,
                        cx,
                    )
                    .on_click(cx.listener(move |this: &mut V, _, _, cx| {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(url.clone()));
                        if let Some(toast) = this.youtube().toasts.get_mut(index) {
                            toast.copied_at = Some(Instant::now());

                            toast.born = Instant::now();
                        }
                        cx.notify();
                    })),
                )
                .child(
                    row_action(
                        SharedString::from(format!("yt-toast-close-{index}")),
                        colors,
                        "cancel01",
                        false,
                        false,
                        cx,
                    )
                    .on_click(cx.listener(move |this: &mut V, _, _, cx| {
                        let toasts = &mut this.youtube().toasts;
                        if index < toasts.len() {
                            toasts.remove(index);
                        }
                        cx.notify();
                    })),
                )
        })
        .collect::<Vec<_>>();

    div()
        .absolute()
        .bottom(px(18.0))
        .right(px(18.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .children(cards)
}

pub fn session_dialog<V: Host>(
    state: &mut Youtube,
    colors: Palette,
    cx: &mut Context<V>,
) -> Option<Div> {
    let waiting = !state.queue.visible().is_empty();
    if state.session.is_empty() && state.running.is_empty() && !waiting && state.notice.is_none() {
        return None;
    }
    if state.form.is_some() || state.published.is_some() || state.choosing_account {
        return None;
    }

    let card = |colors: Palette| {
        div()
            .flex()
            .flex_col()
            .w_full()
            .gap(px(6.0))
            .p(px(10.0))
            .rounded(rems(RADIUS_MD))
            .border_1()
            .border_color(colors.border)
            .bg(opacity(colors.muted, 0.35))
    };

    let mut rows = Vec::new();

    for view in state.rows() {
        let failed = view.failed;
        let id = view.id.clone();
        let mut card = card(colors).child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .size(px(22.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(opacity(
                            if failed {
                                colors.destructive
                            } else {
                                colors.primary
                            },
                            0.15,
                        ))
                        .child(
                            gpui::svg()
                                .size(px(12.0))
                                .path(crate::assets::icon(if failed {
                                    "alert-circle"
                                } else {
                                    "upload04"
                                }))
                                .text_color(if failed {
                                    colors.destructive
                                } else {
                                    colors.primary
                                }),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(rems(TEXT_SM))
                        .child(view.title.clone()),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(rems(TEXT_XS))
                        .text_color(colors.muted_foreground)
                        .child(if view.show_bar && view.fraction > 0.0 {
                            format!("{}%", (view.fraction * 100.0).round() as u32)
                        } else {
                            view.status.clone()
                        }),
                ),
        );

        if view.show_bar {
            card = card.child(
                div()
                    .w_full()
                    .h(px(4.0))
                    .rounded(px(2.0))
                    .overflow_hidden()
                    .bg(opacity(colors.muted, 0.6))
                    .child(
                        div()
                            .h_full()
                            .w(gpui::relative(view.fraction))
                            .bg(colors.primary),
                    ),
            );
        }

        let detail = view.detail.clone().unwrap_or_else(|| view.status.clone());
        card = card.child(
            div()
                .text_size(rems(TEXT_XS))
                .text_color(if failed {
                    colors.destructive
                } else {
                    colors.muted_foreground
                })
                .child(detail),
        );

        if view.can_retry || view.can_cancel {
            let mut actions = div().flex().justify_end().gap(px(6.0));
            if view.can_retry {
                actions = actions.child(action_chip(
                    SharedString::from(format!("yt-retry-{id}")),
                    colors,
                    t("common.retry"),
                    true,
                    Action::RetryTask(id.clone()),
                    cx,
                ));
            }
            if view.can_cancel {
                actions = actions.child(action_chip(
                    SharedString::from(format!("yt-cancel-{id}")),
                    colors,
                    t("common.cancel"),
                    false,
                    Action::CancelTask(id.clone()),
                    cx,
                ));
            }
            card = card.child(actions);
        }

        rows.push(card);
    }

    for entry in &state.session {
        let id = entry.video_id.clone();
        let link = youtube::publish::watch_url(&entry.video_id);
        rows.push(
            card(colors).child(
                div()
                    .flex()
                    .items_center()
                    .w_full()
                    .gap(px(8.0))
                    .child(
                        div()
                            .size(px(22.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(opacity(colors.primary, 0.15))
                            .child(
                                gpui::svg()
                                    .size(px(12.0))
                                    .path(crate::assets::icon("tick02"))
                                    .text_color(colors.primary),
                            ),
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
                                    .truncate()
                                    .text_size(rems(TEXT_SM))
                                    .child(entry.title.clone()),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(rems(TEXT_XS))
                                    .text_color(colors.muted_foreground)
                                    .child(youtube::publish::watch_url(&entry.video_id)),
                            ),
                    )
                    .child(
                        row_action(
                            SharedString::from(format!("yt-copy-{id}")),
                            colors,
                            "copy01",
                            false,
                            false,
                            cx,
                        )
                        .on_click(cx.listener(
                            move |_: &mut V, _, _, cx| {
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                    link.clone(),
                                ));
                                cx.notify();
                            },
                        )),
                    )
                    .child(action_chip(
                        SharedString::from(format!("yt-open-{id}")),
                        colors,
                        t("youtube.session.open"),
                        false,
                        Action::OpenVideo(id),
                        cx,
                    )),
            ),
        );
    }

    let body = div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(8.0))
        .child(heading(colors, t("youtube.session.title")))
        .when_some(state.notice.clone(), |this, message| {
            this.child(
                div()
                    .w_full()
                    .text_size(rems(TEXT_XS))
                    .text_color(colors.caution)
                    .child(message),
            )
        })
        .child(
            div()
                .id("yt-session-list")
                .flex()
                .flex_col()
                .w_full()
                .gap(px(8.0))
                .max_h(px(430.0))
                .overflow_y_scroll()
                .children(rows),
        )
        .child(div().flex().justify_end().pt(px(4.0)).child(action_chip(
            "yt-session-close",
            colors,
            t("youtube.session.close"),
            true,
            Action::CloseSession,
            cx,
        )));

    Some(modal_sized(colors, "yt-session-body", body, 460.0))
}

pub fn publish_dialog<V: Host>(
    state: &mut Youtube,
    colors: Palette,
    now: i64,
    window: &Window,
    cx: &mut Context<V>,
) -> Option<Div> {
    let status_text = account_line(&state.accounts);
    let blocked = !state.can_upload();

    let _queued = state.queue.active().len();
    let state = &*state;
    let form = state.form.as_ref()?;

    let mut categories = Vec::new();
    for (id, _) in publish::CATEGORIES {
        let id = id.to_string();
        let pick = id.clone();
        categories.push(chip(
            SharedString::from(format!("yt-cat-{id}")),
            colors,
            category_label(&id),
            form.category_id == id,
            move |state| {
                if let Some(form) = state.form.as_mut() {
                    form.category_id = pick.clone();
                }
            },
            cx,
        ));
    }

    let mut privacies = Vec::new();
    for privacy in Privacy::ALL {
        privacies.push(chip(
            SharedString::from(format!("yt-privacy-{}", privacy.as_api())),
            colors,
            t(privacy.message_key()),
            form.privacy == privacy,
            move |state| {
                if let Some(form) = state.form.as_mut() {
                    form.privacy = privacy;
                    crate::youtube_ui::remember_privacy(privacy);
                }
            },
            cx,
        ));
    }

    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(label(colors, t("youtube.publish.title")))
        .child(input(
            "yt-form-title",
            &form.title,
            colors,
            field_style(t("youtube.publish.title.placeholder")),
            None,
            |state| state.form.as_mut().map(|form| &mut form.title),
            window,
            cx,
        ))
        .child(label(colors, t("youtube.publish.description")))
        .child(input(
            "yt-form-description",
            &form.description,
            colors,
            FieldStyle {
                height: 132.0,
                multiline: true,
                placeholder: SharedString::from(t("youtube.publish.description.placeholder")),
                ..FieldStyle::default()
            },
            None,
            |state| state.form.as_mut().map(|form| &mut form.description),
            window,
            cx,
        ))
        .child(label(colors, t("youtube.publish.tags")))
        .child(input(
            "yt-form-tags",
            &form.tags,
            colors,
            field_style(t("youtube.publish.tags.placeholder")),
            None,
            |state| state.form.as_mut().map(|form| &mut form.tags),
            window,
            cx,
        ))
        .child(label(
            colors,
            t_args(
                "youtube.publish.tags.hint",
                &[
                    ("used", &form.tags_used().to_string()),
                    ("limit", &publish::MAX_TAG_CHARS.to_string()),
                ],
            ),
        ))
        .child(label(colors, t("youtube.publish.privacy")))
        .child(div().flex().gap(px(4.0)).children(privacies))
        .child(chip(
            SharedString::from("yt-form-more"),
            colors,
            if form.more_open {
                t("youtube.publish.less")
            } else {
                t("youtube.publish.more")
            },
            form.more_open,
            |state| {
                if let Some(form) = state.form.as_mut() {
                    form.more_open = !form.more_open;
                }
            },
            cx,
        ));

    if form.more_open {
        body = body
            .child(checkbox(
                "yt-form-kids",
                colors,
                t("youtube.publish.madeForKids"),
                form.made_for_kids,
                |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.made_for_kids = !form.made_for_kids;
                        if form.made_for_kids {
                            form.age_restricted = false;
                        }
                    }
                },
                cx,
            ))
            .child(checkbox(
                "yt-form-age",
                colors,
                t("youtube.publish.ageRestricted"),
                form.age_restricted,
                |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.age_restricted = !form.age_restricted;
                        if form.age_restricted {
                            form.made_for_kids = false;
                        }
                    }
                },
                cx,
            ))
            .child(checkbox(
                "yt-form-notify",
                colors,
                t("youtube.publish.notify"),
                form.notify_subscribers,
                |state| {
                    if let Some(form) = state.form.as_mut() {
                        form.notify_subscribers = !form.notify_subscribers;
                    }
                },
                cx,
            ))
            .child(label(colors, t("youtube.publish.schedule")))
            .child(schedule_picker(form, colors, now, cx))
            .child(advanced_fields(form, colors, now, window, cx));
    }

    for issue in &form.issues {
        body = body.child(
            div()
                .text_size(rems(TEXT_XS))
                .text_color(colors.destructive)
                .child(issue_message(issue)),
        );
    }

    if let Some(notice) = state.notice.clone() {
        body = body.child(
            div()
                .text_size(rems(TEXT_XS))
                .text_color(colors.caution)
                .child(notice),
        );
    }

    let file_name = form
        .source
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();

    let account = state.accounts.active();
    let channel_name = account
        .map(youtube::Account::display_name)
        .unwrap_or_else(|| t("youtube.accounts.none"));
    let channel_initials = account.map(youtube::Account::initials).unwrap_or_default();
    let avatar = account.and_then(|account| state.avatars.get(&account.id));
    let position = form.preview_position;
    let muted = state.sound.muted;
    let level = state.sound.effective();
    let volume_open = state.volume_open;
    let duration = form.preview_duration.max(0.001);
    let played = (position / duration).clamp(0.0, 1.0) as f32;

    let aside = div()
        .flex()
        .flex_col()
        .w(px(300.0))
        .flex_shrink_0()
        .gap(px(8.0))
        .child(
            div()
                .relative()
                .w_full()
                .h(px(0.0))
                .pb(gpui::relative(0.5625))
                .rounded(rems(RADIUS_SM))
                .overflow_hidden()
                .bg(gpui::black())
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(match form.poster.clone() {
                            Some(image) => gpui::AnyElement::from(
                                gpui::img(image)
                                    .size_full()
                                    .object_fit(gpui::ObjectFit::Contain)
                                    .into_any_element(),
                            ),
                            None => gpui::AnyElement::from(
                                gpui::svg()
                                    .size(px(32.0))
                                    .path(crate::assets::icon("oc-video"))
                                    .text_color(opacity(gpui::white(), 0.4))
                                    .into_any_element(),
                            ),
                        }),
                )
                .child(
                    div()
                        .absolute()
                        .bottom_0()
                        .left_0()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .px(px(8.0))
                        .pb(px(8.0))
                        .pt(px(20.0))
                        .bg(opacity(gpui::black(), 0.45))
                        .child(
                            div()
                                .id("yt-preview-bar")
                                .w_full()
                                .h(px(12.0))
                                .flex()
                                .items_center()
                                .cursor_pointer()
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(
                                        move |this: &mut V, event: &gpui::MouseDownEvent, _, cx| {
                                            this.youtube_action(
                                                Action::SeekPreview(event.position.x.into()),
                                                cx,
                                            );
                                        },
                                    ),
                                )
                                .on_drag(PreviewSeekDrag, |_, _, _, cx| {
                                    gpui::AppContext::new(cx, |_| gpui::Empty)
                                })
                                .on_drag_move::<PreviewSeekDrag>(cx.listener(
                                    move |this: &mut V,
                                          event: &gpui::DragMoveEvent<PreviewSeekDrag>,
                                          _,
                                          cx| {
                                        this.youtube_action(
                                            Action::SeekPreview(event.event.position.x.into()),
                                            cx,
                                        );
                                    },
                                ))
                                .child(
                                    div()
                                        .w_full()
                                        .h(px(4.0))
                                        .rounded(px(2.0))
                                        .overflow_hidden()
                                        .bg(opacity(gpui::white(), 0.3))
                                        .child(
                                            div()
                                                .h_full()
                                                .w(gpui::relative(played))
                                                .bg(colors.primary),
                                        ),
                                )
                                .child(preview_bar_probe(cx)),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .id("yt-preview-play")
                                        .size(px(22.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(rems(RADIUS_SM))
                                        .cursor_pointer()
                                        .bg(opacity(gpui::white(), 0.15))
                                        .child(
                                            gpui::svg()
                                                .size(px(12.0))
                                                .path(crate::assets::icon(
                                                    if form.preview_playing {
                                                        "pause"
                                                    } else {
                                                        "play"
                                                    },
                                                ))
                                                .text_color(gpui::white()),
                                        )
                                        .on_click(cx.listener(move |this: &mut V, _, _, cx| {
                                            this.youtube_action(Action::TogglePreview, cx);
                                        })),
                                )
                                .child(
                                    div()
                                        .text_size(rems(TEXT_XS))
                                        .text_color(gpui::white())
                                        .child(format!(
                                            "{} / {}",
                                            crate::library::format_duration(position),
                                            crate::library::format_duration(form.preview_duration)
                                        )),
                                )
                                .child(div().flex_1())
                                .child(preview_volume(muted, level, volume_open, cx)),
                        ),
                ),
        )
        .child(
            div()
                .text_size(rems(TEXT_SM))
                .text_color(colors.foreground)
                .truncate()
                .child(file_name),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(match avatar {
                    Some(image) => gpui::AnyElement::from(
                        gpui::img(image)
                            .size(px(24.0))
                            .rounded_full()
                            .into_any_element(),
                    ),
                    None => gpui::AnyElement::from(
                        div()
                            .size(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(colors.muted)
                            .text_size(rems(TEXT_XS))
                            .text_color(colors.foreground)
                            .child(channel_initials)
                            .into_any_element(),
                    ),
                })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(rems(TEXT_XS))
                        .text_color(colors.muted_foreground)
                        .child(channel_name),
                ),
        )
        .when(blocked, |this| {
            this.child(
                div()
                    .text_size(rems(TEXT_XS))
                    .text_color(colors.caution)
                    .child(status_text),
            )
        })
        .child(label(colors, t("youtube.publish.category")))
        .child(div().flex().flex_wrap().gap(px(4.0)).children(categories));

    let columns = div()
        .flex()
        .w_full()
        .min_w_0()
        .overflow_hidden()
        .gap(px(16.0))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .max_w(px(560.0))
                .overflow_hidden()
                .child(body),
        )
        .child(aside);

    let dialog = div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(12.0))
        .child(heading(colors, t("youtube.publish.heading")))
        .child(columns)
        .child(
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap(px(6.0))
                .pt(px(4.0))
                .child(action_chip(
                    "yt-form-cancel",
                    colors,
                    t("common.cancel"),
                    false,
                    Action::Dismiss,
                    cx,
                ))
                .child(action_chip(
                    "yt-form-submit",
                    colors,
                    t("youtube.publish.submit"),
                    !blocked,
                    Action::Publish,
                    cx,
                )),
        );

    Some(modal_sized(colors, "yt-form-body", dialog, 920.0))
}
