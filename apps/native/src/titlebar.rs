use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cutix_i18n::{t, t_args};
use gpui::{
    div, prelude::*, px, svg, AnyElement, Context, Entity, FontWeight, MouseButton, MouseDownEvent,
    Window,
};

use crate::assets::icon;
use crate::components::tooltipped;
use crate::interaction::{mix, OverlaySide, Tooltips, Transitions, TOOLBAR_TOOLTIP_DELAY};
use crate::notify;
use crate::state::{AppModel, Route};
use crate::theme::{opacity, rem, Palette, TEXT_XS, TITLEBAR_BUTTON_WIDTH, TITLEBAR_HEIGHT};
use crate::update::{self, Phase};

const UPDATE_PILL_ID: &str = "titlebar-update";
const UPDATE_DISMISS_ID: &str = "titlebar-update-dismiss";
const UPDATE_PILL_HEIGHT: f32 = 22.0;
const UPDATE_PROGRESS_POLL: Duration = Duration::from_millis(120);
/// How often an installed update looks whether the export or upload it waits for
/// has finished.
const UPDATE_IDLE_POLL: Duration = Duration::from_secs(2);
/// How long the pill's tooltip explains a refused click before it goes back to
/// its usual hint.
const UPDATE_BUSY_NOTICE: Duration = Duration::from_secs(4);

pub struct Titlebar {
    app: Entity<AppModel>,
    transitions: Transitions,
    tooltips: Tooltips,
    update_dir: Option<PathBuf>,
    update: update::Shared,
    /// When a click on the pill was refused because something was still running.
    update_refused_at: Option<Instant>,
}

impl Titlebar {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        let update_dir = update::install_dir();
        let shared: update::Shared = Arc::new(Mutex::new(Phase::Idle));
        if update_dir.is_some() {
            Self::watch_releases(shared.clone(), cx);
        }
        Self {
            app,
            transitions: Transitions::new(),
            tooltips: Tooltips::new(TOOLBAR_TOOLTIP_DELAY),
            update_dir,
            update: shared,
            update_refused_at: None,
        }
    }

    /// Whether restarting now would cut something off: an export writing its file,
    /// an upload, or a sign-in with a browser open. The YouTube state lives in the
    /// assets panel, which publishes what it is doing through the render flags.
    fn work_in_progress(&self, cx: &Context<Self>) -> bool {
        self.app.read(cx).export.is_running() || crate::youtube_ui::render::busy()
    }

    /// Keeps the pill where it is and lets its tooltip say why the click did nothing.
    fn refuse_update(&mut self, cx: &mut Context<Self>) {
        self.update_refused_at = Some(Instant::now());
        cx.notify();
    }

    fn busy_notice_showing(&self) -> bool {
        self.update_refused_at
            .is_some_and(|at| at.elapsed() < UPDATE_BUSY_NOTICE)
    }

    fn phase(&self) -> Phase {
        self.update
            .lock()
            .map(|phase| phase.clone())
            .unwrap_or_default()
    }

    fn set_phase(&self, next: Phase) {
        if let Ok(mut phase) = self.update.lock() {
            *phase = next;
        }
    }

    /// Checks for a newer release shortly after start and then every few hours.
    fn watch_releases(shared: update::Shared, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(update::FIRST_CHECK_DELAY)
                .await;
            loop {
                let found = cx.background_spawn(async { update::check() }).await;
                if let Ok(Some(release)) = found {
                    let changed = match shared.lock() {
                        Ok(mut phase) => {
                            let replace = match &*phase {
                                Phase::Idle => true,
                                Phase::Available(known) | Phase::Failed { release: known, .. } => {
                                    known.tag != release.tag
                                }
                                Phase::Installing { .. }
                                | Phase::Restarting
                                | Phase::ReadyToRestart { .. } => false,
                            };
                            if replace {
                                *phase = Phase::Available(release);
                            }
                            replace
                        }
                        Err(_) => false,
                    };
                    if changed && this.update(cx, |_, cx| cx.notify()).is_err() {
                        return;
                    }
                }
                if this.upgrade().is_none() {
                    return;
                }
                cx.background_executor().timer(update::CHECK_INTERVAL).await;
            }
        })
        .detach();
    }

    /// Downloads and installs the pending release, then restarts into it — once
    /// nothing that a restart would cut short is running.
    fn install_update(&mut self, cx: &mut Context<Self>) {
        let Some(dir) = self.update_dir.clone() else {
            return;
        };
        let release = match self.phase() {
            Phase::Available(release) | Phase::Failed { release, .. } => release,
            Phase::ReadyToRestart { release, exe } => {
                if self.work_in_progress(cx) {
                    self.refuse_update(cx);
                } else {
                    self.restart_into(release, exe, cx);
                }
                return;
            }
            Phase::Idle | Phase::Installing { .. } | Phase::Restarting => return,
        };
        // An export or upload started now would be killed by the restart at the end
        // of the download, so the whole thing waits rather than only the restart.
        if self.work_in_progress(cx) {
            self.refuse_update(cx);
            return;
        }
        self.set_phase(Phase::Installing {
            release: release.clone(),
            progress: 0.0,
        });
        self.update_refused_at = None;
        self.tooltips.dismiss();
        cx.notify();

        let shared = self.update.clone();
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(UPDATE_PROGRESS_POLL).await;
            let installing = shared
                .lock()
                .map(|phase| matches!(*phase, Phase::Installing { .. }))
                .unwrap_or(false);
            if this.update(cx, |_, cx| cx.notify()).is_err() || !installing {
                return;
            }
        })
        .detach();

        let shared = self.update.clone();
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn({
                    let release = release.clone();
                    async move {
                        update::install(&dir, &release, |fraction| {
                            if let Ok(mut phase) = shared.lock() {
                                if let Phase::Installing { progress, .. } = &mut *phase {
                                    *progress = fraction;
                                }
                            }
                        })
                    }
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                match outcome {
                    // The download took a while: an export or upload may have started
                    // meanwhile, and the new files can wait in place for it.
                    Ok(exe) if this.work_in_progress(cx) => {
                        this.set_phase(Phase::ReadyToRestart { release, exe });
                        this.restart_when_idle(cx);
                    }
                    Ok(exe) => this.restart_into(release, exe, cx),
                    Err(error) => this.set_phase(Phase::Failed { release, error }),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Saves and starts the installed executable; this process quits once it is up.
    fn restart_into(&mut self, release: update::Release, exe: PathBuf, cx: &mut Context<Self>) {
        self.set_phase(Phase::Restarting);
        self.tooltips.dismiss();
        self.app.update(cx, |model, _| model.save_now());
        match update::relaunch(&exe) {
            Ok(()) => cx.quit(),
            Err(error) => self.set_phase(Phase::Failed { release, error }),
        }
        cx.notify();
    }

    /// Restarts on its own once the export or upload an installed update waited
    /// for is over — or stops watching when a click got there first.
    fn restart_when_idle(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(UPDATE_IDLE_POLL).await;
            let done = this.update(cx, |this, cx| {
                let Phase::ReadyToRestart { release, exe } = this.phase() else {
                    return true;
                };
                if this.work_in_progress(cx) {
                    return false;
                }
                this.restart_into(release, exe, cx);
                true
            });
            if done.unwrap_or(true) {
                return;
            }
        })
        .detach();
    }

    /// Puts a failed update away for this session; the next check offers it afresh.
    fn dismiss_failed_update(&mut self, cx: &mut Context<Self>) {
        if matches!(self.phase(), Phase::Failed { .. }) {
            self.set_phase(Phase::Idle);
            self.transitions.set(UPDATE_PILL_ID, false);
            self.tooltips.dismiss();
            cx.notify();
        }
    }

    fn update_pill(&mut self, colors: Palette, cx: &mut Context<Self>) -> Option<AnyElement> {
        let phase = self.phase();
        let (label, glyph, tooltip, progress, failed) = match &phase {
            Phase::Idle => return None,
            Phase::Available(release) => (
                t_args("update.available", &[("version", &release.label())]),
                // A tray-and-arrow reads as "download"; a bare chevron reads as a menu.
                Some("download04"),
                Some(t("update.availableHint")),
                None,
                false,
            ),
            Phase::Installing { progress, .. } => {
                let percent = (progress.clamp(0.0, 1.0) * 100.0).round() as u32;
                (
                    t_args("update.installing", &[("percent", &percent.to_string())]),
                    None,
                    None,
                    Some(*progress),
                    false,
                )
            }
            Phase::Restarting => (t("update.restarting"), None, None, Some(1.0), false),
            Phase::ReadyToRestart { .. } => (
                t("update.ready"),
                // The same arrow, now pointing at a restart rather than a download.
                Some("download04"),
                Some(t("update.readyHint")),
                None,
                false,
            ),
            Phase::Failed { error, .. } => (
                t("update.failed"),
                Some("alert-circle"),
                Some(t(error.message_key())),
                None,
                true,
            ),
        };
        // A refused click borrows the tooltip for a moment to say what to finish first.
        let tooltip = if self.busy_notice_showing() && tooltip.is_some() {
            Some(t("update.busy"))
        } else {
            tooltip
        };

        let clickable = !phase.busy();
        let hover = if clickable {
            self.transitions.eased(UPDATE_PILL_ID)
        } else {
            0.0
        };
        let tint = if failed {
            colors.destructive
        } else {
            colors.primary
        };
        // The label takes the palette's blue meant for text on a blue tint: primary
        // itself is too light to read at this size on the light theme.
        let ink = if failed {
            colors.destructive
        } else {
            colors.secondary_foreground
        };

        let mut pill = div()
            .id(UPDATE_PILL_ID)
            .relative()
            .flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(6.0))
            .h(px(UPDATE_PILL_HEIGHT))
            .px(px(10.0))
            .rounded_full()
            .overflow_hidden()
            .border_1()
            .border_color(opacity(tint, 0.35 + 0.3 * hover))
            .bg(mix(opacity(tint, 0.12), opacity(tint, 0.24), hover))
            .text_color(mix(opacity(ink, 0.9), ink, hover))
            .text_size(rem(TEXT_XS))
            .font_weight(FontWeight::MEDIUM)
            .whitespace_nowrap();

        if let Some(progress) = progress {
            pill = pill.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(gpui::relative(progress.clamp(0.0, 1.0)))
                    .bg(opacity(tint, 0.22)),
            );
        }
        if let Some(glyph) = glyph {
            pill = pill.child(
                svg()
                    .size(px(12.0))
                    .flex_shrink_0()
                    .path(icon(glyph))
                    .text_color(ink),
            );
        }
        pill = pill.child(div().relative().child(label));
        if failed {
            // A failure that keeps failing should not sit in the titlebar all session:
            // the cross puts it away, while the pill itself still retries.
            pill = pill.child(
                div()
                    .id(UPDATE_DISMISS_ID)
                    .relative()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(14.0))
                    .rounded_full()
                    .cursor_pointer()
                    .hover(|this| this.bg(opacity(ink, 0.15)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        cx.stop_propagation();
                        this.dismiss_failed_update(cx);
                    }))
                    .child(
                        svg()
                            .size(px(9.0))
                            .flex_shrink_0()
                            .path(icon("win-close"))
                            .text_color(ink),
                    ),
            );
        }

        if clickable {
            pill = pill
                .cursor_pointer()
                .on_hover(cx.listener(|this: &mut Self, hovered, _, cx| {
                    this.transitions.set(UPDATE_PILL_ID, *hovered);
                    this.tooltips.hover(UPDATE_PILL_ID, *hovered);
                    cx.notify();
                }))
                .on_click(cx.listener(|this, _, _, cx| this.install_update(cx)));
        }

        let frame = if tooltip.is_some() {
            self.tooltips.frame_for(UPDATE_PILL_ID)
        } else {
            None
        };
        Some(
            div()
                .flex()
                .h_full()
                .items_center()
                .pr(px(8.0))
                .child(tooltipped(
                    pill,
                    colors,
                    tooltip.unwrap_or_default(),
                    frame,
                    OverlaySide::Bottom,
                ))
                .into_any_element(),
        )
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

        self.tooltips.tick();
        let update_pill = self.update_pill(colors, cx);

        if self.transitions.animating() || self.tooltips.animating() || self.busy_notice_showing() {
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
            .child(
                div()
                    .flex()
                    .h_full()
                    .items_center()
                    .children(update_pill)
                    .children(buttons),
            )
    }
}
