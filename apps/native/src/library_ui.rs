use cutix_i18n::{t, t_args};
use gpui::{
    div, prelude::*, px, relative, svg, App, Context, Div, Entity, FocusHandle, FontWeight, Render,
    SharedString, Window,
};
use std::path::PathBuf;

use crate::assets::icon;
use crate::components::{
    menu_action, menu_natural_height, menu_surface, overlay_backdrop, overlay_layer, overlay_root,
    place_anchored, MENU_ITEM_HEIGHT_PX, MENU_OFFSET_PX,
};
use crate::components::{Button, ButtonVariant};
use crate::interaction::mix;
use crate::interaction::Transitions;
use crate::library::{self, Entry};
use crate::preview::Thumbnails;
use crate::preview_audio::Sound;
use crate::state::{AppModel, Route};
use crate::theme::{
    opacity, rem, Palette, RADIUS_MD, RADIUS_SM, TEXT_BASE, TEXT_LG, TEXT_SM, TEXT_XS,
};

const CARD_GUTTER_PX: f32 = 8.0;
const CARD_THUMBNAIL_RATIO: f32 = 0.5625;
const MENU_WIDTH_PX: f32 = 200.0;

const CARD_BAR_HIT_PX: f32 = 12.0;
const CARD_BAR_LINE_PX: f32 = 4.0;

const CARD_ACTIONS: &[(&str, &str, &str)] = &[
    ("view", "play", "library.action.view"),
    ("edit", "edit03", "library.action.edit"),
    ("upload", "cloud-upload", "library.action.upload"),
    ("delete", "delete02", "common.delete"),
];

const HOVER_FRAME_WIDTH: u32 = 640;

const PLAYER_FRAME_WIDTH: u32 = 1920;

const SEEK_STEPS: usize = 48;

pub const SPEEDS: [f32; 5] = [0.5, 1.0, 1.5, 2.0, 4.0];

struct Player {
    path: PathBuf,
    duration: f64,

    position: f64,
    frame: Option<std::sync::Arc<gpui::RenderImage>>,
    playing: bool,
    speed: f32,

    shown: Option<f64>,

    stepped_at: std::time::Instant,

    generation: u64,
}

impl Player {
    fn fraction(&self) -> f32 {
        if self.duration <= 0.0 {
            return 0.0;
        }
        (self.position / self.duration).clamp(0.0, 1.0) as f32
    }
}

#[derive(Debug)]
struct CardBarDrag;

fn bar_probe(cx: &mut Context<LibraryView>) -> impl IntoElement {
    let handle = cx.entity();

    gpui::canvas(
        move |bounds, _window, cx| {
            let measured = (f32::from(bounds.origin.x), f32::from(bounds.size.width));
            handle.update(cx, |view: &mut LibraryView, _| {
                view.bar_bounds = measured;
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

pub fn advance_by(position: f64, duration: f64, speed: f32, seconds: f64) -> (f64, bool) {
    let step = seconds.max(0.0) * speed.max(0.0) as f64;
    let next = position + step;
    if duration > 0.0 && next >= duration {
        (duration, true)
    } else {
        (next.max(0.0), false)
    }
}

#[derive(Debug)]
pub struct VolumeDrag;

struct Hover {
    path: PathBuf,

    position: f32,
    frame: Option<std::sync::Arc<gpui::RenderImage>>,

    generation: u64,

    scrubbing: bool,

    shown: Option<f64>,

    stepped_at: std::time::Instant,
}

impl Hover {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            position: 0.0,
            frame: None,
            generation: 0,
            scrubbing: false,
            shown: None,
            stepped_at: std::time::Instant::now(),
        }
    }
}

pub struct LibraryView {
    app: Entity<AppModel>,
    focus: FocusHandle,
    transitions: Transitions,
    scroll: gpui::ScrollHandle,

    entries: Vec<Entry>,

    scanned: Option<PathBuf>,

    thumbnails: Thumbnails,

    hover_decoder: Option<crate::preview::FrameWorker>,

    unreadable: std::collections::HashSet<PathBuf>,

    warned: std::collections::HashSet<PathBuf>,

    bar_bounds: (f32, f32),

    hover: Option<Hover>,

    speaker: crate::preview_audio::Speaker,
    sound: Sound,
    volume_open: bool,
    volume_bar: (f32, f32),
    menu: crate::interaction::Overlay,
    hovers: u64,
    menu_for: Option<Entry>,
    menu_at: gpui::Point<gpui::Pixels>,

    confirm_delete: Option<Entry>,
    player: Option<Player>,
    plays: u64,
}

impl LibraryView {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self {
            app,
            focus: cx.focus_handle(),
            transitions: Transitions::new(),
            scroll: gpui::ScrollHandle::new(),
            entries: Vec::new(),
            scanned: None,
            thumbnails: Thumbnails::default(),
            hover_decoder: None,
            unreadable: std::collections::HashSet::new(),
            warned: std::collections::HashSet::new(),
            bar_bounds: (0.0, 0.0),
            hover: None,
            speaker: {
                let settings = crate::state::load_settings();
                let mut speaker = crate::preview_audio::Speaker::default();
                let muted = settings.preview_muted.unwrap_or(true);
                let volume = settings.preview_volume.unwrap_or(1.0);
                speaker.set_volume(if muted { 0.0 } else { volume });
                speaker
            },
            sound: {
                let settings = crate::state::load_settings();
                let volume = settings.preview_volume.unwrap_or(1.0);
                Sound {
                    volume,
                    muted: settings.preview_muted.unwrap_or(true),
                    restore: volume,
                }
            },
            volume_open: false,
            volume_bar: (0.0, 0.0),
            menu: crate::interaction::Overlay::new(crate::interaction::OverlaySide::Bottom),
            hovers: 0,
            menu_for: None,
            menu_at: gpui::point(px(0.0), px(0.0)),
            confirm_delete: None,
            player: None,
            plays: 0,
        }
    }

    fn colors(&self, cx: &App) -> Palette {
        self.app.read(cx).theme.root
    }

    fn directory(&self, cx: &App) -> PathBuf {
        library::directory(self.app.read(cx).video_directory.as_deref())
    }

    fn ensure_scanned(&mut self, cx: &App) {
        let directory = self.directory(cx);
        if self.scanned.as_deref() == Some(directory.as_path()) {
            return;
        }
        self.entries = library::scan(&directory);
        self.scanned = Some(directory);

        self.thumbnails.clear();
    }

    pub fn rescan(&mut self) {
        self.scanned = None;
    }

    fn probe_next(&mut self, cx: &mut Context<Self>) {
        let batch = self
            .thumbnails
            .claim(self.entries.iter().map(|entry| entry.path.as_path()));
        if batch.is_empty() {
            return;
        }

        cx.spawn(async move |this, cx| {
            let probed = cx
                .background_spawn(async move {
                    batch
                        .iter()
                        .map(|path| crate::preview::probe(path))
                        .collect::<Vec<_>>()
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                for one in probed {
                    this.thumbnails.store(one);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) -> bool {
        if self.player.is_some() {
            self.close_player(cx);
            return true;
        }
        if self.confirm_delete.take().is_some() {
            return true;
        }
        if self.menu.is_open() {
            self.menu.dismiss();
            self.menu_for = None;
            return true;
        }
        false
    }

    fn set_hover(&mut self, entry: Option<&Entry>, cx: &mut Context<Self>) {
        match entry {
            Some(entry) => {
                if self.hover.as_ref().map(|hover| &hover.path) == Some(&entry.path) {
                    return;
                }
                self.hovers += 1;
                let mut hover = Hover::new(entry.path.clone());
                hover.generation = self.hovers;
                self.hover = Some(hover);
                self.probe_now(entry.path.clone(), cx);
                self.decode_preview(cx);
            }
            None => {
                if self.hover.as_ref().is_some_and(|hover| hover.scrubbing) {
                    return;
                }
                self.hover = None;
                self.speaker.forget();
            }
        }
        cx.notify();
    }

    fn advance_preview(&mut self, cx: &mut Context<Self>) {
        let Some(hover) = self.hover.as_ref() else {
            return;
        };
        let path = hover.path.clone();
        if self.unreadable.contains(&path) {
            return;
        }
        let duration = self.thumbnails.duration(&path).unwrap_or_default();
        if duration <= 0.0 {
            return;
        }

        let Some(hover) = self.hover.as_mut() else {
            return;
        };
        let elapsed = hover.stepped_at.elapsed().as_secs_f64();
        hover.stepped_at = std::time::Instant::now();
        if !hover.scrubbing {
            hover.position += (elapsed / duration) as f32;
            if hover.position > 1.0 {
                hover.position = 0.0;
            }
        }

        self.decode_preview(cx);
        self.collect_preview(cx);
        self.ensure_sound(cx);
        self.feed_sound();
    }

    fn advance_player(&mut self, cx: &mut Context<Self>) {
        let Some(player) = self.player.as_mut() else {
            return;
        };
        if !player.playing {
            self.collect_player(cx);
            return;
        }

        let elapsed = player.stepped_at.elapsed().as_secs_f64();
        player.stepped_at = std::time::Instant::now();
        let (position, ended) = advance_by(player.position, player.duration, player.speed, elapsed);
        player.position = position;
        if ended {
            player.playing = false;
        }

        self.decode_player(cx);
        self.collect_player(cx);
        self.ensure_player_sound(cx);
        self.feed_player_sound();
    }

    fn feed_player_sound(&mut self) {
        if self.sound.muted || self.hover.is_some() {
            return;
        }
        let Some(player) = self.player.as_ref() else {
            return;
        };
        if !player.playing {
            return;
        }
        self.speaker.feed(player.position);
    }

    fn ensure_player_sound(&mut self, cx: &mut Context<Self>) {
        if self.sound.muted || self.hover.is_some() {
            return;
        }
        let Some(player) = self.player.as_ref() else {
            return;
        };
        if !player.playing {
            return;
        }
        let path = player.path.clone();
        let at = player.position;
        let Some(start) = self.speaker.window_needed(&path, at) else {
            return;
        };

        self.speaker.begin_loading();
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
                        this.speaker.accept_window(&path, pcm, window_start);
                        this.feed_player_sound();
                    }
                    Err(_) => {
                        this.speaker
                            .note_silent(&path, crate::preview_audio::Silent::NoDecoder);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn feed_sound(&mut self) {
        let Some(hover) = self.hover.as_ref() else {
            self.speaker.silence();
            return;
        };
        if self.sound.muted {
            return;
        }
        let Some(duration) = self.thumbnails.duration(&hover.path) else {
            return;
        };
        let at = (hover.position as f64) * duration;
        self.speaker.feed(at);
    }

    fn mute(&mut self, cx: &mut Context<Self>) {
        self.sound.muted = true;
        self.apply_sound(cx);
    }

    fn apply_sound(&mut self, cx: &mut Context<Self>) {
        self.speaker.set_volume(self.sound.effective());
        crate::state::save_preview_audio(self.sound.volume, self.sound.muted);
        if self.sound.muted {
            self.speaker.silence();
        } else {
            self.ensure_sound(cx);
            self.ensure_player_sound(cx);
        }
        cx.notify();
    }

    pub fn nudge_volume(&mut self, step: f32, cx: &mut Context<Self>) {
        self.sound.nudge(step);
        self.volume_open = true;
        self.apply_sound(cx);
    }

    pub fn set_volume(&mut self, volume: f32, cx: &mut Context<Self>) {
        self.sound.set_volume(volume);
        self.apply_sound(cx);
    }

    pub fn hearing_anything(&self) -> bool {
        self.hover.is_some() || self.player.is_some()
    }

    fn toggle_sound(&mut self, cx: &mut Context<Self>) {
        let path = self
            .hover
            .as_ref()
            .map(|hover| hover.path.clone())
            .or_else(|| self.player.as_ref().map(|player| player.path.clone()));

        self.sound.toggle();

        if !self.sound.muted {
            if let Some(why) = path
                .as_ref()
                .and_then(|path| self.speaker.silent_reason(path))
            {
                self.sound.muted = true;
                let reason = t(why.message_key());
                self.app.update(cx, |model, cx| {
                    model.notice = Some(reason);
                    cx.notify();
                });
            }
        }

        self.apply_sound(cx);
    }

    fn ensure_sound(&mut self, cx: &mut Context<Self>) {
        let Some(hover) = self.hover.as_ref() else {
            return;
        };
        if self.sound.muted {
            return;
        }
        let path = hover.path.clone();
        let Some(duration) = self.thumbnails.duration(&path) else {
            return;
        };
        let at = (hover.position as f64) * duration;
        let Some(start) = self.speaker.window_needed(&path, at) else {
            return;
        };

        self.speaker.begin_loading();
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
                        this.speaker.accept_window(&path, pcm, window_start);
                        this.feed_sound();
                    }
                    Err(_) => {
                        this.speaker
                            .note_silent(&path, crate::preview_audio::Silent::NoDecoder);
                        if this.hover.as_ref().map(|hover| &hover.path) == Some(&path) {
                            this.mute(cx);
                            let reason = t(crate::preview_audio::Silent::NoDecoder.message_key());
                            this.app.update(cx, |model, cx| {
                                model.notice = Some(reason);
                                cx.notify();
                            });
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn scrub(&mut self, fraction: f32, cx: &mut Context<Self>) {
        let Some(hover) = self.hover.as_mut() else {
            return;
        };
        hover.position = fraction.clamp(0.0, 1.0);
        self.decode_preview(cx);
        cx.notify();
    }

    fn bar_fraction(&self, x: f32) -> f32 {
        let (left, width) = self.bar_bounds;
        if width <= 0.0 {
            return 0.0;
        }
        ((x - left) / width).clamp(0.0, 1.0)
    }

    fn on_bar_press(
        &mut self,
        event: &gpui::MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        let fraction = self.bar_fraction(f32::from(event.position.x));
        if let Some(hover) = self.hover.as_mut() {
            hover.scrubbing = true;
        }
        self.scrub(fraction, cx);
    }

    fn on_bar_drag(
        &mut self,
        event: &gpui::DragMoveEvent<CardBarDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = event.bounds;
        let width = bounds.right() - bounds.left();
        if width <= px(0.0) {
            return;
        }
        let fraction = ((event.event.position.x - bounds.left()) / width).clamp(0.0, 1.0);
        if let Some(hover) = self.hover.as_mut() {
            hover.scrubbing = true;
        }
        self.scrub(fraction, cx);
    }

    fn on_bar_release(
        &mut self,
        _event: &gpui::MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(hover) = self.hover.as_mut() else {
            return;
        };
        if !hover.scrubbing {
            return;
        }
        hover.scrubbing = false;
        cx.notify();
    }

    fn decode_preview(&mut self, cx: &mut Context<Self>) {
        let Some(hover) = self.hover.as_ref() else {
            return;
        };
        let Some(duration) = self.thumbnails.duration(&hover.path) else {
            return;
        };
        if self.unreadable.contains(&hover.path) {
            return;
        }
        let path = hover.path.clone();
        let generation = hover.generation;
        let at = (hover.position as f64) * duration;

        if self.hover_decoder.is_none() {
            self.hover_decoder = crate::preview::FrameWorker::spawn();
        }
        if let Some(decoder) = self.hover_decoder.as_ref() {
            decoder.request(path, at, generation, Some(HOVER_FRAME_WIDTH));
        }
        self.collect_preview(cx);
    }

    fn collect_preview(&mut self, cx: &mut Context<Self>) {
        self.note_failures(cx);
        let Some(frame) = self
            .hover_decoder
            .as_ref()
            .and_then(crate::preview::FrameWorker::take)
        else {
            return;
        };
        let Some(hover) = self.hover.as_mut() else {
            return;
        };
        if hover.generation != frame.generation {
            return;
        }

        if hover.shown == Some(frame.timestamp) {
            return;
        }
        hover.shown = Some(frame.timestamp);
        let stale = hover.frame.replace(frame.image);
        if let Some(stale) = stale {
            cx.drop_image(stale, None);
        }
        cx.notify();
    }

    fn note_failures(&mut self, cx: &mut Context<Self>) {
        let Some(decoder) = self.hover_decoder.as_ref() else {
            return;
        };
        let failures = decoder.take_failures();
        if failures.is_empty() {
            return;
        }

        for path in failures {
            self.unreadable.insert(path);
        }
        if self
            .hover
            .as_ref()
            .is_some_and(|hover| self.unreadable.contains(&hover.path))
        {
            self.speaker.silence();
            self.sound.muted = true;
            self.speaker.set_volume(0.0);
            if let Some(hover) = self.hover.as_mut() {
                hover.position = 0.0;
            }
            let reason = t("library.preview.unreadable");
            self.app.update(cx, |model, cx| {
                model.notice = Some(reason);
                cx.notify();
            });
        }
        cx.notify();
    }

    fn probe_now(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.thumbnails.duration(&path).is_some() {
            return;
        }

        cx.spawn(async move |this, cx| {
            let probed = cx
                .background_spawn(async move { crate::preview::probe(&path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.thumbnails.store(probed);
                this.decode_preview(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn run_action(&mut self, action: &str, entry: &Entry, cx: &mut Context<Self>) {
        self.menu.dismiss();
        self.menu_for = None;
        match action {
            "view" => self.app.update(cx, |model, cx| {
                model.preview_video = Some(entry.path.clone());
                cx.notify();
            }),
            "edit" => self.app.update(cx, |model, cx| {
                model.open_video_as_project(entry.path.clone(), entry.name.clone(), cx)
            }),
            "upload" => self.app.update(cx, |model, cx| {
                model.youtube_request = Some(entry.path.clone());
                cx.notify();
            }),
            "delete" => self.confirm_delete = Some(entry.clone()),
            _ => {}
        }
        cx.notify();
    }

    fn delete_now(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.confirm_delete.take() else {
            return;
        };
        match std::fs::remove_file(&entry.path) {
            Ok(()) => {
                self.entries.retain(|other| other.path != entry.path);
                if self.hover.as_ref().map(|hover| &hover.path) == Some(&entry.path) {
                    self.hover = None;
                }
            }
            Err(error) => self.app.update(cx, |model, cx| {
                model.notice = Some(error.to_string());
                cx.notify();
            }),
        }
        cx.notify();
    }

    fn card(&mut self, entry: &Entry, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        let key = entry.path.to_string_lossy().to_string();
        let hover_key = format!("video-{key}");
        let lift = self.transitions.eased(&hover_key);
        let hovered = self.hover.as_ref().filter(|hover| hover.path == entry.path);
        let scrub = hovered.map(|hover| hover.position);
        let length = self
            .thumbnails
            .duration(&entry.path)
            .or(entry.duration_seconds);

        let duration = length.map(|seconds| match scrub {
            Some(position) => library::format_duration(seconds * f64::from(1.0 - position)),
            None => library::format_duration(seconds),
        });
        let still = hovered
            .and_then(|hover| hover.frame.clone())
            .or_else(|| self.thumbnails.image(&entry.path));
        let speaker = self.volume_control("card", entry.name.clone(), colors, cx);
        let for_hover = entry.clone();
        let for_leave = entry.clone();
        let for_menu = entry.clone();
        let for_open = entry.clone();

        div()
            .w(relative(0.25))
            .flex_shrink_0()
            .p(px(CARD_GUTTER_PX))
            .child(
                div()
                    .id(SharedString::from(hover_key.clone()))
                    .flex()
                    .flex_col()
                    .w_full()
                    .gap(px(8.0))
                    .on_hover(cx.listener(move |this: &mut Self, is_over: &bool, _, cx| {
                        this.transitions.set(hover_key.clone(), *is_over);
                        if *is_over {
                            this.set_hover(Some(&for_hover), cx);
                        } else if this.hover.as_ref().map(|hover| &hover.path)
                            == Some(&for_leave.path)
                        {
                            this.set_hover(None, cx);
                        }
                        cx.notify();
                    }))
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(
                            move |this: &mut Self, event: &gpui::MouseDownEvent, _, cx| {
                                this.menu_at = event.position;
                                this.menu_for = Some(for_menu.clone());
                                this.menu.set_open(true);
                                cx.notify();
                            },
                        ),
                    )
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.run_action("view", &for_open, cx);
                    }))
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(0.0))
                            .pb(relative(CARD_THUMBNAIL_RATIO))
                            .rounded(rem(RADIUS_MD))
                            .overflow_hidden()
                            .border_1()
                            .border_color(mix(colors.border, colors.primary, lift))
                            .bg(colors.muted)
                            .child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(match still {
                                        Some(image) => gpui::img(image)
                                            .size_full()
                                            .object_fit(gpui::ObjectFit::Cover)
                                            .into_any_element(),

                                        None => svg()
                                            .size(px(34.0))
                                            .path(icon("oc-video"))
                                            .text_color(opacity(colors.muted_foreground, 0.7))
                                            .into_any_element(),
                                    }),
                            )
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
                            })
                            .when_some(scrub, |this, position| {
                                this.child(speaker).child(
                                    div()
                                        .id(SharedString::from(format!("bar-{}", entry.name)))
                                        .absolute()
                                        .bottom_0()
                                        .left_0()
                                        .w_full()
                                        .h(px(CARD_BAR_HIT_PX))
                                        .flex()
                                        .items_end()
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            gpui::MouseButton::Left,
                                            cx.listener(Self::on_bar_press),
                                        )
                                        .on_mouse_up(
                                            gpui::MouseButton::Left,
                                            cx.listener(Self::on_bar_release),
                                        )
                                        .on_drag(CardBarDrag, |_, _, _, cx| cx.new(|_| gpui::Empty))
                                        .on_drag_move::<CardBarDrag>(cx.listener(Self::on_bar_drag))
                                        .child(bar_probe(cx))
                                        .child(
                                            div()
                                                .w_full()
                                                .h(px(CARD_BAR_LINE_PX))
                                                .bg(opacity(gpui::white(), 0.25))
                                                .child(
                                                    div()
                                                        .h_full()
                                                        .w(relative(position))
                                                        .bg(colors.primary),
                                                ),
                                        ),
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
                                    .child(entry.name.clone()),
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
                                        "library.created",
                                        &[(
                                            "date",
                                            &crate::library_ui::format_created(entry.created),
                                        )],
                                    )),
                            ),
                    ),
            )
    }

    fn volume_control(
        &mut self,
        surface: &'static str,
        key: String,
        colors: Palette,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<Div> {
        let muted = self.sound.muted;
        let level = self.sound.effective();
        let open = self.volume_open;
        let glyph = if muted { "volume-mute" } else { "volume-high" };

        let slider = div()
            .id(SharedString::from(format!("volume-bar-{surface}-{key}")))
            .w(px(if open { 74.0 } else { 0.0 }))
            .h(px(20.0))
            .flex()
            .items_center()
            .overflow_hidden()
            .cursor_pointer()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(
                    move |this: &mut Self, event: &gpui::MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        let x = f32::from(event.position.x);
                        this.volume_from_bar(x, cx);
                    },
                ),
            )
            .on_drag(VolumeDrag, |_, _, _, cx| {
                gpui::AppContext::new(cx, |_| gpui::Empty)
            })
            .on_drag_move::<VolumeDrag>(cx.listener(
                move |this: &mut Self, event: &gpui::DragMoveEvent<VolumeDrag>, _, cx| {
                    let x = f32::from(event.event.position.x);
                    this.volume_from_bar(x, cx);
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
                    .child(div().h_full().w(relative(level)).bg(gpui::white()))
                    .child(self.volume_probe(cx)),
            );

        div()
            .id(SharedString::from(format!("volume-{surface}-{key}")))
            .absolute()
            .top(px(8.0))
            .right(px(8.0))
            .h(px(28.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .px(px(6.0))
            .rounded(rem(RADIUS_SM))
            .bg(opacity(gpui::black(), if open { 0.75 } else { 0.55 }))
            .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                this.volume_open = *hovered;
                cx.notify();
            }))
            .child(
                div()
                    .id(SharedString::from(format!("volume-toggle-{surface}-{key}")))
                    .size(px(18.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .child(svg().size(px(15.0)).path(icon(glyph)).text_color(if muted {
                        opacity(gpui::white(), 0.75)
                    } else {
                        colors.primary
                    }))
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx: &mut gpui::App| {
                        cx.stop_propagation()
                    })
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        cx.stop_propagation();
                        this.toggle_sound(cx);
                    })),
            )
            .child(slider)
    }

    fn volume_probe(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let handle = cx.entity();
        gpui::canvas(
            move |bounds, _window, cx| {
                let measured = (f32::from(bounds.origin.x), f32::from(bounds.size.width));
                handle.update(cx, |this: &mut Self, _| {
                    this.volume_bar = measured;
                });
            },
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }

    fn volume_from_bar(&mut self, x: f32, cx: &mut Context<Self>) {
        let (left, width) = self.volume_bar;
        if width <= 1.0 {
            return;
        }
        self.set_volume(((x - left) / width).clamp(0.0, 1.0), cx);
    }

    fn upload_badge(&mut self, cx: &mut Context<Self>) -> Option<gpui::Stateful<Div>> {
        let colors = self.colors(cx);
        let summary = self.app.read(cx).uploads.clone();
        if !summary.is_busy() {
            return None;
        }
        let fraction = summary.fraction();
        let percent = (fraction * 100.0).round() as u32;
        let title = summary
            .title
            .clone()
            .unwrap_or_else(|| t("youtube.queue.title.plain"));

        Some(
            div()
                .id("library-uploads")
                .flex()
                .items_center()
                .gap(px(8.0))
                .px(px(10.0))
                .py(px(6.0))
                .rounded(rem(RADIUS_MD))
                .cursor_pointer()
                .border_1()
                .border_color(opacity(colors.primary, 0.5))
                .bg(opacity(colors.primary, 0.10))
                .child(
                    div()
                        .size(px(22.0))
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
                        .rounded(px(11.0))
                        .bg(colors.primary)
                        .text_size(rem(TEXT_XS))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.primary_foreground)
                        .child(summary.active.to_string()),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(
                            div()
                                .text_size(rem(TEXT_XS))
                                .text_color(colors.foreground)
                                .max_w(px(190.0))
                                .truncate()
                                .child(t_args(
                                    "library.uploads.busy",
                                    &[("title", &title), ("percent", &percent.to_string())],
                                )),
                        )
                        .child(
                            div()
                                .w(px(190.0))
                                .h(px(3.0))
                                .rounded(px(2.0))
                                .overflow_hidden()
                                .bg(opacity(colors.muted, 0.7))
                                .child(div().h_full().w(relative(fraction)).bg(colors.primary)),
                        ),
                )
                .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                    this.app.update(cx, |model, cx| {
                        model.settings_request = Some(String::from("youtube"));
                        cx.notify();
                    });
                })),
        )
    }

    fn header(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let badge = self.upload_badge(cx);
        let colors = self.colors(cx);
        let directory = self.directory(cx);
        let count = self.entries.len();

        div()
            .flex()
            .items_center()
            .gap(px(20.0))
            .w_full()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(px(2.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(rem(TEXT_BASE))
                            .child(
                                div()
                                    .id("library-breadcrumb-home")
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
                                    .child(t("home.library")),
                            ),
                    )
                    .child(
                        div()
                            .text_size(rem(TEXT_XS))
                            .text_color(colors.muted_foreground)
                            .truncate()
                            .child(SharedString::from(directory.display().to_string())),
                    ),
            )
            .children(badge)
            .child(
                div()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(t_args("library.count", &[("count", &count.to_string())])),
            )
            .child(
                Button::new("library-rescan", colors)
                    .variant(ButtonVariant::Outline)
                    .icon("rotate-clockwise")
                    .label(t("library.rescan"))
                    .build()
                    .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                        this.rescan();
                        cx.notify();
                    })),
            )
    }

    fn empty(&self, colors: Palette) -> impl IntoElement {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(8.0))
            .child(
                svg()
                    .size(px(40.0))
                    .path(icon("oc-video"))
                    .text_color(opacity(colors.muted_foreground, 0.6)),
            )
            .child(
                div()
                    .text_size(rem(TEXT_SM))
                    .text_color(colors.muted_foreground)
                    .child(t("library.empty")),
            )
            .child(
                div()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(t("library.empty.hint")),
            )
    }
}

impl LibraryView {
    fn sync_player(&mut self, cx: &mut Context<Self>) {
        let wanted = self.app.read(cx).preview_video.clone();
        match wanted {
            Some(path) if self.player.as_ref().map(|player| &player.path) != Some(&path) => {
                let known = self.thumbnails.duration(&path);
                let duration = known.unwrap_or_default();
                self.plays += 1;
                self.player = Some(Player {
                    path,
                    duration,
                    position: 0.0,
                    frame: None,
                    playing: true,
                    speed: 1.0,
                    shown: None,
                    stepped_at: std::time::Instant::now(),
                    generation: self.plays,
                });
                self.decode_player(cx);
                if known.is_none() {
                    self.probe_player(cx);
                }
            }
            None => self.player = None,
            _ => {}
        }
    }

    fn probe_player(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.player.as_ref().map(|player| player.path.clone()) else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let probed = cx
                .background_spawn({
                    let path = path.clone();
                    async move { crate::preview::probe(&path) }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let (Some(player), Some(duration)) =
                    (this.player.as_mut(), probed.duration_seconds)
                {
                    if player.path == path {
                        player.duration = duration;
                    }
                }
                this.thumbnails.store(probed);
                cx.notify();
            });
        })
        .detach();
    }

    fn close_player(&mut self, cx: &mut Context<Self>) {
        self.player = None;
        self.app.update(cx, |model, cx| {
            model.preview_video = None;
            cx.notify();
        });
        cx.notify();
    }

    fn decode_player(&mut self, cx: &mut Context<Self>) {
        let Some(player) = self.player.as_ref() else {
            return;
        };
        let path = player.path.clone();
        let generation = player.generation;
        let at = player.position;

        if self.hover_decoder.is_none() {
            self.hover_decoder = crate::preview::FrameWorker::spawn();
        }
        if let Some(decoder) = self.hover_decoder.as_ref() {
            decoder.request(path, at, generation, Some(PLAYER_FRAME_WIDTH));
        }
        self.collect_player(cx);
    }

    fn collect_player(&mut self, cx: &mut Context<Self>) {
        let Some(frame) = self
            .hover_decoder
            .as_ref()
            .and_then(crate::preview::FrameWorker::take)
        else {
            return;
        };
        let mut stale = None;
        if let Some(player) = self.player.as_mut() {
            if player.generation == frame.generation && player.shown != Some(frame.timestamp) {
                player.shown = Some(frame.timestamp);
                stale = std::mem::replace(&mut player.frame, Some(frame.image));
            }
        }
        if let Some(stale) = stale {
            cx.drop_image(stale, None);
        }
        cx.notify();
    }

    fn seek_player(&mut self, fraction: f32, cx: &mut Context<Self>) {
        let Some(player) = self.player.as_mut() else {
            return;
        };
        player.position = (fraction.clamp(0.0, 1.0) as f64) * player.duration;
        self.decode_player(cx);
        cx.notify();
    }

    fn player_modal(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let player = self.player.as_ref()?;
        let colors = self.colors(cx);
        let name = player
            .path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();
        let frame = player.frame.clone();
        let fraction = player.fraction();
        let playing = player.playing;
        let speed = player.speed;
        let elapsed = library::format_duration(player.position);
        let total = library::format_duration(player.duration);

        let mut bar = div().id("player-seek").flex().w_full().h(px(16.0));
        for step in 0..SEEK_STEPS {
            let at = step as f32 / (SEEK_STEPS - 1).max(1) as f32;
            bar = bar.child(
                div()
                    .id(SharedString::from(format!("player-seek-{step}")))
                    .flex_1()
                    .h_full()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.seek_player(at, cx);
                    })),
            );
        }

        let volume = self
            .volume_control("player", String::from("modal"), colors, cx)
            .relative()
            .top_0()
            .right_0();

        let speeds: Vec<_> = SPEEDS
            .iter()
            .map(|value| {
                let value = *value;
                let selected = (speed - value).abs() < 0.01;
                div()
                    .id(SharedString::from(format!("player-speed-{value}")))
                    .px(px(9.0))
                    .py(px(3.0))
                    .rounded(rem(RADIUS_SM))
                    .cursor_pointer()
                    .text_size(rem(TEXT_XS))
                    .text_color(if selected {
                        colors.foreground
                    } else {
                        colors.muted_foreground
                    })
                    .bg(opacity(colors.accent, if selected { 0.9 } else { 0.35 }))
                    .child(format!("{value}x"))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        if let Some(player) = this.player.as_mut() {
                            player.speed = value;
                        }
                        cx.notify();
                    }))
            })
            .collect();

        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(opacity(gpui::black(), 0.7))
                .occlude()
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, cx| this.close_player(cx)),
                )
                .child(
                    div()
                        .w(relative(0.78))
                        .max_w(px(1040.0))
                        .flex()
                        .flex_col()
                        .gap(px(10.0))
                        .p(px(16.0))
                        .rounded(rem(RADIUS_MD))
                        .border_1()
                        .border_color(colors.border)
                        .bg(colors.popover)
                        .text_color(colors.popover_foreground)
                        .shadow_lg()
                        .occlude()
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx: &mut gpui::App| {
                            cx.stop_propagation()
                        })
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(rem(TEXT_SM))
                                        .font_weight(FontWeight::MEDIUM)
                                        .child(name),
                                )
                                .child(
                                    div()
                                        .id("player-close")
                                        .size(px(24.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(rem(RADIUS_SM))
                                        .cursor_pointer()
                                        .text_color(colors.muted_foreground)
                                        .child(svg().size(px(13.0)).path(icon("win-close")))
                                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                            this.close_player(cx)
                                        })),
                                ),
                        )
                        .child(
                            div()
                                .relative()
                                .w_full()
                                .h(px(0.0))
                                .pb(relative(CARD_THUMBNAIL_RATIO))
                                .rounded(rem(RADIUS_SM))
                                .overflow_hidden()
                                .bg(gpui::black())
                                .child(
                                    div()
                                        .absolute()
                                        .inset_0()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(match frame {
                                            Some(image) => gpui::img(image)
                                                .size_full()
                                                .object_fit(gpui::ObjectFit::Contain)
                                                .into_any_element(),
                                            None => svg()
                                                .size(px(40.0))
                                                .path(icon("oc-video"))
                                                .text_color(opacity(gpui::white(), 0.5))
                                                .into_any_element(),
                                        }),
                                ),
                        )
                        .child(
                            div()
                                .w_full()
                                .h(px(4.0))
                                .rounded(px(2.0))
                                .overflow_hidden()
                                .bg(opacity(colors.muted, 0.6))
                                .child(div().h_full().w(relative(fraction)).bg(colors.primary)),
                        )
                        .child(bar)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .id("player-play")
                                        .size(px(28.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(rem(RADIUS_SM))
                                        .cursor_pointer()
                                        .bg(opacity(colors.accent, 0.6))
                                        .child(
                                            svg()
                                                .size(px(14.0))
                                                .path(icon(if playing { "pause" } else { "play" }))
                                                .text_color(colors.foreground),
                                        )
                                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                            if let Some(player) = this.player.as_mut() {
                                                if !player.playing
                                                    && player.position >= player.duration
                                                {
                                                    player.position = 0.0;
                                                }
                                                player.playing = !player.playing;
                                            }
                                            if let Some(player) = this.player.as_mut() {
                                                player.stepped_at = std::time::Instant::now();
                                            }
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    div()
                                        .text_size(rem(TEXT_XS))
                                        .text_color(colors.muted_foreground)
                                        .child(format!("{elapsed} / {total}")),
                                )
                                .child(volume)
                                .child(div().flex_1())
                                .children(speeds)
                                .child(
                                    div()
                                        .id("player-upload")
                                        .flex()
                                        .h(px(28.0))
                                        .items_center()
                                        .gap(px(6.0))
                                        .px(px(10.0))
                                        .rounded(rem(RADIUS_SM))
                                        .cursor_pointer()
                                        .bg(opacity(colors.accent, 0.6))
                                        .text_size(rem(TEXT_XS))
                                        .text_color(colors.foreground)
                                        .child(
                                            svg()
                                                .size(px(14.0))
                                                .flex_shrink_0()
                                                .path(icon("cloud-upload"))
                                                .text_color(colors.foreground),
                                        )
                                        .child(t("library.action.upload"))
                                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                            this.upload_from_player(cx);
                                        })),
                                ),
                        ),
                ),
        )
    }

    fn upload_from_player(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.player.as_ref().map(|player| player.path.clone()) else {
            return;
        };
        self.close_player(cx);
        self.app.update(cx, |model, cx| {
            model.youtube_request = Some(path);
            cx.notify();
        });
        cx.notify();
    }

    fn context_menu(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        if !self.menu.is_open() {
            return None;
        }
        let entry = self.menu_for.clone()?;
        let colors = self.colors(cx);

        let items: Vec<_> = CARD_ACTIONS
            .iter()
            .map(|(action, glyph, label)| {
                let key = format!("library-menu-{action}");
                let hovered = self.transitions.eased(&key) > 0.5;
                let for_hover = key.clone();
                let action = *action;
                let entry = entry.clone();
                menu_action(
                    SharedString::from(key),
                    colors,
                    t(label),
                    glyph,
                    hovered,
                    action == "delete",
                )
                .on_hover(cx.listener(move |this: &mut Self, is_over: &bool, _, cx| {
                    this.transitions.set(for_hover.clone(), *is_over);
                    cx.notify();
                }))
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.run_action(action, &entry, cx);
                }))
            })
            .collect();

        let frame = self.menu.frame();
        if !frame.visible {
            return None;
        }
        let natural = (
            MENU_WIDTH_PX,
            menu_natural_height(CARD_ACTIONS.len(), MENU_ITEM_HEIGHT_PX),
        );
        let placement = place_anchored(
            frame,
            crate::interaction::OverlaySide::Bottom,
            natural,
            (
                f32::from(self.menu_at.x),
                f32::from(self.menu_at.y) + MENU_OFFSET_PX,
            ),
        );

        Some(
            overlay_root()
                .child(overlay_backdrop("library-menu-backdrop").on_mouse_up(
                    gpui::MouseButton::Left,
                    cx.listener(|this: &mut Self, _, _, cx| {
                        cx.stop_propagation();
                        this.menu.dismiss();
                        this.menu_for = None;
                        cx.notify();
                    }),
                ))
                .child(overlay_layer(
                    gpui::Corner::TopLeft,
                    placement,
                    menu_surface(colors, placement)
                        .w(px(MENU_WIDTH_PX))
                        .p(px(4.0))
                        .children(items),
                )),
        )
    }

    fn delete_dialog(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let entry = self.confirm_delete.clone()?;
        let colors = self.colors(cx);

        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(opacity(gpui::black(), 0.55))
                .occlude()
                .child(
                    div()
                        .w(px(420.0))
                        .flex()
                        .flex_col()
                        .gap(px(12.0))
                        .p(px(18.0))
                        .rounded(rem(RADIUS_MD))
                        .border_1()
                        .border_color(colors.border)
                        .bg(colors.popover)
                        .text_color(colors.popover_foreground)
                        .shadow_lg()
                        .child(
                            div()
                                .text_size(rem(TEXT_LG))
                                .font_weight(FontWeight::MEDIUM)
                                .child(t("library.delete.title")),
                        )
                        .child(
                            div()
                                .text_size(rem(TEXT_SM))
                                .text_color(colors.muted_foreground)
                                .child(t_args("library.delete.body", &[("name", &entry.name)])),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(div().flex_1())
                                .child(
                                    Button::new("library-delete-cancel", colors)
                                        .variant(ButtonVariant::Outline)
                                        .label(t("common.cancel"))
                                        .build()
                                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                            this.confirm_delete = None;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("library-delete-confirm", colors)
                                        .variant(ButtonVariant::Destructive)
                                        .label(t("common.delete"))
                                        .build()
                                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                            this.delete_now(cx);
                                        })),
                                ),
                        ),
                ),
        )
    }
}

pub fn format_created(unix_seconds: i64) -> String {
    crate::state::format_date(&youtube::iso_date(unix_seconds))
}

impl Render for LibraryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let previewing = self.hover.is_some() || self.player.as_ref().is_some_and(|p| p.playing);
        if previewing {
            let view = cx.entity();
            cx.defer(move |cx| {
                view.update(cx, |this, cx| {
                    this.advance_preview(cx);
                    this.advance_player(cx);
                });
            });
        }
        if self.transitions.animating() || self.menu.animating() || previewing {
            window.request_animation_frame();
        }
        self.ensure_scanned(cx);
        self.probe_next(cx);
        self.sync_player(cx);
        let colors = self.colors(cx);
        let entries = self.entries.clone();
        let header = self.header(cx);
        let menu = self.context_menu(cx);
        let confirm = self.delete_dialog(cx);
        let player = self.player_modal(cx);

        let body: gpui::AnyElement = if entries.is_empty() {
            div()
                .id("library-empty")
                .flex()
                .flex_1()
                .min_h_0()
                .child(self.empty(colors))
                .into_any_element()
        } else {
            let cards: Vec<_> = entries.iter().map(|entry| self.card(entry, cx)).collect();

            div()
                .flex()
                .flex_1()
                .w_full()
                .min_h_0()
                .child(
                    div()
                        .id("library-grid")
                        .flex()
                        .flex_col()
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll)
                        .child(div().flex().flex_wrap().w_full().children(cards)),
                )
                .into_any_element()
        };

        div()
            .track_focus(&self.focus)
            .key_context("Library")
            .on_key_down(
                cx.listener(|this: &mut Self, event: &gpui::KeyDownEvent, _, cx| {
                    if event.keystroke.key != "escape" {
                        return;
                    }

                    if !this.dismiss(cx) {
                        this.app.update(cx, |model, cx| {
                            model.route = Route::Home;
                            cx.notify();
                        });
                    }
                    cx.notify();
                }),
            )
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .p(px(24.0))
            .bg(colors.background)
            .text_color(colors.foreground)
            .child(header)
            .child(body)
            .children(menu)
            .children(confirm)
            .children(player)
    }
}

#[cfg(test)]
mod tests {
    use super::Sound;

    #[test]
    fn muting_restores_the_level_it_was_at() {
        let mut sound = Sound::default();
        sound.set_volume(0.6);
        assert!(!sound.muted);

        sound.toggle();
        assert!(sound.muted);
        assert_eq!(sound.effective(), 0.0);

        sound.toggle();
        assert!(!sound.muted);
        assert!((sound.effective() - 0.6).abs() < 1e-6);
    }

    #[test]
    fn unmuting_from_nothing_goes_to_full() {
        let mut sound = Sound::default();
        sound.set_volume(0.0);
        assert!(sound.muted);

        sound.toggle();
        assert!((sound.effective() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn nudging_stays_inside_the_range_and_unmutes() {
        let mut sound = Sound::default();
        sound.set_volume(0.0);
        sound.nudge(0.05);
        assert!(!sound.muted);
        assert!((sound.effective() - 0.05).abs() < 1e-6);

        for _ in 0..40 {
            sound.nudge(0.05);
        }
        assert_eq!(sound.effective(), 1.0);

        for _ in 0..40 {
            sound.nudge(-0.05);
        }
        assert_eq!(sound.effective(), 0.0);
        assert!(sound.muted);
    }

    use super::*;

    #[test]
    fn every_string_the_browser_shows_is_translated() {
        for key in [
            "library.empty",
            "library.empty.hint",
            "library.rescan",
            "home.library",
            "home.title",
        ] {
            assert_ne!(t(key), key, "{key}");
        }
        for key in ["library.created", "library.count"] {
            let filled = t_args(key, &[("date", "2026-08-15"), ("count", "3")]);
            assert_ne!(filled, key, "{key}");
            assert!(!filled.contains('{'), "{filled}");
        }
    }

    #[test]
    fn a_creation_time_reads_as_a_calendar_day() {
        assert_eq!(format_created(1_700_000_000), "14.11.2023");
        assert_eq!(format_created(0), "01.01.1970");
    }

    #[test]
    fn a_tick_walks_the_playhead_forward_at_the_chosen_speed() {
        let (position, ended) = advance_by(0.0, 10.0, 1.0, 0.2);
        assert!((position - 0.2).abs() < 1e-9);
        assert!(!ended);

        let (double, _) = advance_by(0.0, 10.0, 2.0, 0.2);
        assert!(
            (double - 0.4).abs() < 1e-9,
            "twice the speed is twice the step"
        );

        let (half, _) = advance_by(1.0, 10.0, 0.5, 0.2);
        assert!((half - 1.1).abs() < 1e-9);
    }

    #[test]
    fn the_playhead_stops_at_the_end_rather_than_running_past_it() {
        let (position, ended) = advance_by(9.95, 10.0, 1.0, 0.2);
        assert_eq!(position, 10.0);
        assert!(ended, "and the player is told to stop");

        let (open, ended) = advance_by(5.0, 0.0, 1.0, 0.2);
        assert!(open > 5.0);
        assert!(!ended);
    }

    #[test]
    fn a_paused_or_stopped_speed_does_not_walk_backwards() {
        let (position, _) = advance_by(3.0, 10.0, 0.0, 0.2);
        assert_eq!(position, 3.0);
        let (clamped, _) = advance_by(3.0, 10.0, -2.0, 0.2);
        assert_eq!(clamped, 3.0, "a negative speed is not rewind, it is a bug");
    }

    #[test]
    fn every_offered_speed_is_forward_and_they_are_all_different() {
        assert!(SPEEDS.iter().all(|speed| *speed > 0.0));
        let mut sorted = SPEEDS.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).expect("ordered"));
        sorted.dedup();
        assert_eq!(sorted.len(), SPEEDS.len());
        assert!(SPEEDS.contains(&1.0), "normal speed has to be one of them");
    }

    #[test]
    fn every_menu_action_is_translated_and_drawn_with_a_real_icon() {
        for (action, glyph, label) in CARD_ACTIONS {
            assert_ne!(t(label), *label, "{action}");
            assert!(crate::assets::icon_exists(glyph), "{glyph}");
        }
        for key in ["library.delete.title"] {
            assert_ne!(t(key), key, "{key}");
        }
        let body = t_args("library.delete.body", &[("name", "clip")]);
        assert!(body.contains("clip"));
        assert!(!body.contains('{'));
    }

    #[test]
    fn the_icons_the_browser_draws_all_exist() {
        for glyph in ["oc-video", "calendar04", "chevron-left", "rotate-clockwise"] {
            assert!(crate::assets::icon_exists(glyph), "{glyph}");
        }
    }
}
