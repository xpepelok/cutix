use cutix_i18n::{t, t_args};
use gpui::{
    div, prelude::*, px, relative, svg, App, Context, Div, Entity, FocusHandle, FontWeight, Render,
    SharedString, Window,
};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::assets::icon;
use crate::components::{
    menu_action, menu_natural_height, menu_surface, overlay_backdrop, overlay_layer, overlay_root,
    place_anchored, MENU_ITEM_HEIGHT_PX, MENU_OFFSET_PX,
};
use crate::components::{Button, ButtonVariant, MenuPlacement};
use crate::interaction::mix;
use crate::interaction::Transitions;
use crate::library::{self, Entry};
use crate::preview::Thumbnails;
use crate::preview_audio::Sound;
use crate::state::{AppModel, Route, ViewMode};
use crate::theme::{
    opacity, rem, Palette, RADIUS_MD, RADIUS_SM, TEXT_BASE, TEXT_LG, TEXT_SM, TEXT_XS,
};

const CARD_GUTTER_PX: f32 = 8.0;
const METADATA_BATCH: usize = 48;
const RESCAN_NOTICE: Duration = Duration::from_millis(1_600);
const MENU_PAD_PX: f32 = 4.0;
const MENU_OVERLAP_PX: f32 = 10.0;
const GRID_COLUMNS: usize = 4;
const GROUP_HEADER_PX: f32 = 30.0;
const LIST_ROW_PX: f32 = 74.0;
const CARD_TEXT_PX: f32 = 62.0;
const ESTIMATED_GRID_WIDTH: f32 = 1_600.0;
const ESTIMATED_GRID_HEIGHT: f32 = 900.0;
const OVERSCAN: f32 = 0.5;
const SCROLL_FAST_SCALE: f32 = 4.0;
const AUTOSCROLL_DEAD_ZONE: f32 = 14.0;
const AUTOSCROLL_GAIN: f32 = 0.16;
const AUTOSCROLL_MAX_PX: f32 = 340.0;
const AUTOSCROLL_MARKER_PX: f32 = 26.0;
const DELETE_RETRIES: usize = 6;
const DELETE_BACKOFF: Duration = Duration::from_millis(40);

fn y_of(anchor: f32, origin: f32) -> f32 {
    anchor - origin - AUTOSCROLL_MARKER_PX / 2.0
}
const SCROLL_STEP_SCALE: f32 = 2.4;
const SCROLL_GLIDE: f32 = 0.22;
const SCROLL_SNAP_PX: f32 = 0.5;
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

#[derive(Debug)]
struct PlayerBarDrag;

#[derive(Debug)]
struct TrimStartDrag;

#[derive(Debug)]
struct TrimEndDrag;

enum GridRow {
    Header(String),
    Cards(std::ops::Range<usize>),
}

fn frame_probe(cx: &mut Context<LibraryView>) -> impl IntoElement {
    let handle = cx.entity();
    gpui::canvas(
        move |bounds, _window, cx| {
            let measured = (f32::from(bounds.origin.x), f32::from(bounds.origin.y));
            handle.update(cx, |view: &mut LibraryView, _| {
                view.grid_origin = measured;
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

fn grid_probe(cx: &mut Context<LibraryView>) -> impl IntoElement {
    let handle = cx.entity();
    gpui::canvas(
        move |bounds, _window, cx| {
            let measured = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            handle.update(cx, |view: &mut LibraryView, _| {
                view.grid_view = measured;
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

fn row_probe(cx: &mut Context<LibraryView>) -> impl IntoElement {
    let handle = cx.entity();
    gpui::canvas(
        move |bounds, _window, cx| {
            let measured = f32::from(bounds.size.height);
            handle.update(cx, |view: &mut LibraryView, _| {
                if measured > 0.0 && (view.row_height - measured).abs() > 0.5 {
                    view.row_height = measured;
                }
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortFlyout {
    More,
    Group,
}

impl SortFlyout {
    fn id(self) -> &'static str {
        match self {
            SortFlyout::More => "more",
            SortFlyout::Group => "group",
        }
    }
}

fn player_bar_probe(cx: &mut Context<LibraryView>) -> impl IntoElement {
    let handle = cx.entity();

    gpui::canvas(
        move |bounds, _window, cx| {
            let measured = (f32::from(bounds.origin.x), f32::from(bounds.size.width));
            handle.update(cx, |view: &mut LibraryView, _| {
                view.player_bar = measured;
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

fn trim_bar_probe(cx: &mut Context<LibraryView>) -> impl IntoElement {
    let handle = cx.entity();

    gpui::canvas(
        move |bounds, _window, cx| {
            let measured = (f32::from(bounds.origin.x), f32::from(bounds.size.width));
            handle.update(cx, |view: &mut LibraryView, _| {
                view.trim_bar = measured;
            });
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

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

const CHECKBOX_PX: f32 = 18.0;

fn selection_box(
    id: impl Into<SharedString>,
    colors: Palette,
    checked: bool,
) -> gpui::Stateful<Div> {
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

fn remove_stubborn(path: &std::path::Path) -> std::io::Result<()> {
    let mut outcome = std::fs::remove_file(path);
    for _ in 0..DELETE_RETRIES {
        if outcome.is_ok() || !path.exists() {
            break;
        }
        std::thread::sleep(DELETE_BACKOFF);
        outcome = std::fs::remove_file(path);
    }
    if outcome.is_err() && !path.exists() {
        outcome = Ok(());
    }
    outcome
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
    scroll_target: Option<f32>,
    autoscroll: Option<(f32, f32, f32)>,
    grid_origin: (f32, f32),

    entries: Vec<Entry>,

    scanned: Option<PathBuf>,

    thumbnails: Thumbnails,

    hover_decoder: Option<crate::preview::FrameWorker>,

    unreadable: std::collections::HashSet<PathBuf>,

    bar_bounds: (f32, f32),
    player_bar: (f32, f32),

    hover: Option<Hover>,

    speaker: crate::preview_audio::Speaker,
    sound: Sound,
    volume_open: bool,
    volume_dragging: bool,
    volume_bar: (f32, f32),
    menu: crate::interaction::Overlay,
    hovers: u64,
    menu_for: Option<Entry>,
    menu_at: gpui::Point<gpui::Pixels>,

    confirm_delete: Option<Entry>,
    confirm_bulk: Option<Vec<Entry>>,
    selected: std::collections::HashSet<PathBuf>,
    player: Option<Player>,
    plays: u64,

    trim_range: Option<(f32, f32)>,
    trim_bar: (f32, f32),
    trim_busy: bool,
    temp_file: Option<PathBuf>,
    pending_upload: Option<(PathBuf, bool)>,
    swept: bool,

    query: crate::input::TextField,
    sort: library::SortKey,
    ascending: bool,
    grouping: library::Grouping,
    sort_menu: crate::interaction::Overlay,
    sort_menu_at: gpui::Point<gpui::Pixels>,
    sort_flyout: Option<SortFlyout>,
    grid_view: (f32, f32),
    row_height: f32,
    view: ViewMode,
    metadata_busy: bool,
    visible: Vec<Entry>,
    visible_key: Option<(u64, usize, String, &'static str, bool)>,
    revision: u64,
    rescanned_at: Option<Instant>,
}

impl LibraryView {
    pub fn new(app: Entity<AppModel>, cx: &mut Context<Self>) -> Self {
        cx.observe(&app, |_, _, cx| cx.notify()).detach();
        Self {
            app,
            focus: cx.focus_handle(),
            transitions: Transitions::new(),
            scroll: gpui::ScrollHandle::new(),
            scroll_target: None,
            autoscroll: None,
            grid_origin: (0.0, 0.0),
            entries: Vec::new(),
            scanned: None,
            thumbnails: Thumbnails::default(),
            hover_decoder: None,
            unreadable: std::collections::HashSet::new(),
            bar_bounds: (0.0, 0.0),
            player_bar: (0.0, 0.0),
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
            volume_dragging: false,
            volume_bar: (0.0, 0.0),
            menu: crate::interaction::Overlay::new(crate::interaction::OverlaySide::Bottom),
            hovers: 0,
            menu_for: None,
            menu_at: gpui::point(px(0.0), px(0.0)),
            confirm_delete: None,
            confirm_bulk: None,
            selected: std::collections::HashSet::new(),
            player: None,
            plays: 0,
            trim_range: None,
            trim_bar: (0.0, 0.0),
            trim_busy: false,
            temp_file: None,
            pending_upload: None,
            swept: false,
            query: crate::input::TextField::new(cx, ""),
            sort: library::SortKey::Modified,
            ascending: false,
            grouping: library::Grouping::None,
            sort_menu: crate::interaction::Overlay::new(crate::interaction::OverlaySide::Bottom),
            sort_menu_at: gpui::point(px(0.0), px(0.0)),
            sort_flyout: None,
            grid_view: (0.0, 0.0),
            row_height: 0.0,
            view: ViewMode::Grid,
            metadata_busy: false,
            visible: Vec::new(),
            visible_key: None,
            revision: 0,
            rescanned_at: None,
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
        self.revision += 1;

        self.thumbnails.clear();
    }

    pub fn rescan(&mut self) {
        self.scanned = None;
        self.rescanned_at = Some(Instant::now());
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
        if self.confirm_bulk.take().is_some() {
            return true;
        }
        if self.confirm_delete.take().is_some() {
            return true;
        }
        if self.trim_range.take().is_some() {
            return true;
        }
        if self.player.is_some() {
            self.close_player(cx);
            return true;
        }
        if !self.selected.is_empty() {
            self.selected.clear();
            return true;
        }
        if self.menu.is_open() {
            self.menu.dismiss();
            self.menu_for = None;
            return true;
        }
        false
    }

    fn overlay_open(&self) -> bool {
        self.sort_menu.is_open()
            || self.menu.is_open()
            || self.confirm_delete.is_some()
            || self.confirm_bulk.is_some()
            || self.player.is_some()
    }

    fn selecting(&self) -> bool {
        !self.selected.is_empty()
    }

    fn toggle_selected(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        if !self.selected.remove(path) {
            self.selected.insert(path.to_path_buf());
        }
        cx.notify();
    }

    fn set_hover(&mut self, entry: Option<&Entry>, cx: &mut Context<Self>) {
        let entry = if self.overlay_open() { None } else { entry };
        match entry {
            Some(entry) => {
                if self.hover.as_ref().map(|hover| &hover.path) == Some(&entry.path) {
                    return;
                }
                self.hovers += 1;
                let mut hover = Hover::new(entry.path.clone());
                hover.generation = self.hovers;
                self.hover = Some(hover);
                self.sound.muted = true;
                self.speaker.set_volume(0.0);
                self.speaker.silence();
                self.probe_now(entry.path.clone(), cx);
                self.decode_preview(cx);
            }
            None => {
                if self.volume_dragging || self.hover.as_ref().is_some_and(|hover| hover.scrubbing)
                {
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
            self.speaker.silence();
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
        self.speaker.feed(player.position, player.speed);
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
        self.speaker.feed(at, 1.0);
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

    fn refresh_visible(&mut self) {
        let key = (
            self.revision,
            self.entries.len(),
            self.query.buffer.text.clone(),
            self.sort.id(),
            self.ascending,
        );
        if self.visible_key.as_ref() == Some(&key) {
            return;
        }
        let mut shown: Vec<Entry> = self
            .entries
            .iter()
            .filter(|entry| library::matches(entry, &key.2))
            .cloned()
            .collect();
        library::arrange(&mut shown, self.sort, self.ascending);
        self.visible = shown;
        self.visible_key = Some(key);
    }

    fn probe_metadata(&mut self, cx: &mut Context<Self>) {
        if self.metadata_busy {
            return;
        }
        let batch: Vec<PathBuf> = self
            .entries
            .iter()
            .filter(|entry| entry.duration_seconds.is_none() && entry.width.is_none())
            .map(|entry| entry.path.clone())
            .take(METADATA_BATCH)
            .collect();
        if batch.is_empty() {
            return;
        }
        self.metadata_busy = true;

        cx.spawn(async move |this, cx| {
            let measured = cx
                .background_spawn(async move {
                    batch
                        .into_iter()
                        .map(|path| {
                            let info = video::probe(&path).ok();
                            (path, info)
                        })
                        .collect::<Vec<_>>()
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                for (path, info) in measured {
                    let Some(entry) = this.entries.iter_mut().find(|entry| entry.path == path)
                    else {
                        continue;
                    };
                    match info {
                        Some(info) => {
                            entry.duration_seconds = Some(info.duration_seconds);
                            entry.width = Some(u32::from(info.width));
                            entry.height = Some(u32::from(info.height));
                        }
                        None => {
                            entry.duration_seconds = Some(0.0);
                            entry.width = Some(0);
                            entry.height = Some(0);
                        }
                    }
                }
                this.metadata_busy = false;
                this.revision += 1;
                cx.notify();
            });
        })
        .detach();
    }

    fn on_grid_wheel(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = f32::from(event.delta.pixel_delta(window.line_height()).y);
        if delta == 0.0 {
            return;
        }
        let modifiers = event.modifiers;
        let scale = if modifiers.control || modifiers.platform {
            SCROLL_STEP_SCALE * SCROLL_FAST_SCALE
        } else {
            SCROLL_STEP_SCALE
        };
        let reach = f32::from(self.scroll.max_offset().height);
        let from = self
            .scroll_target
            .unwrap_or_else(|| f32::from(self.scroll.offset().y));
        self.scroll_target = Some((from + delta * scale).clamp(-reach, 0.0));
        cx.stop_propagation();
        cx.notify();
    }

    fn on_grid_middle(
        &mut self,
        event: &gpui::MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        let (x, y) = (f32::from(event.position.x), f32::from(event.position.y));
        self.autoscroll = match self.autoscroll {
            Some(_) => None,
            None => Some((x, y, y)),
        };
        cx.notify();
    }

    fn on_grid_move(
        &mut self,
        event: &gpui::MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((x, anchor, _)) = self.autoscroll else {
            return;
        };
        self.autoscroll = Some((x, anchor, f32::from(event.position.y)));
        cx.notify();
    }

    fn drive_autoscroll(&mut self, window: &mut Window) {
        let Some((_, anchor, pointer)) = self.autoscroll else {
            return;
        };
        window.request_animation_frame();
        let gap = pointer - anchor;
        if gap.abs() <= AUTOSCROLL_DEAD_ZONE {
            return;
        }
        let reach = f32::from(self.scroll.max_offset().height);
        let step = ((gap.abs() - AUTOSCROLL_DEAD_ZONE).min(AUTOSCROLL_MAX_PX) * AUTOSCROLL_GAIN)
            * gap.signum();
        let from = self
            .scroll_target
            .unwrap_or_else(|| f32::from(self.scroll.offset().y));
        self.scroll_target = Some((from - step).clamp(-reach, 0.0));
    }

    fn glide_scroll(&mut self, window: &mut Window) {
        let Some(target) = self.scroll_target else {
            return;
        };
        let current = f32::from(self.scroll.offset().y);
        let gap = target - current;
        if gap.abs() <= SCROLL_SNAP_PX {
            self.scroll
                .set_offset(gpui::point(self.scroll.offset().x, px(target)));
            self.scroll_target = None;
            return;
        }
        let next = current + gap * SCROLL_GLIDE;
        self.scroll
            .set_offset(gpui::point(self.scroll.offset().x, px(next)));
        window.request_animation_frame();
    }

    fn toggle_player(&mut self, cx: &mut Context<Self>) {
        let Some(player) = self.player.as_mut() else {
            return;
        };
        if !player.playing && player.position >= player.duration {
            player.position = 0.0;
        }
        player.playing = !player.playing;
        player.stepped_at = std::time::Instant::now();
        let paused = !player.playing;
        if paused {
            self.speaker.silence();
        }
        cx.notify();
    }

    fn player_seek(&mut self, fraction: f32, cx: &mut Context<Self>) {
        let Some(player) = self.player.as_mut() else {
            return;
        };
        if player.duration <= 0.0 {
            return;
        }
        player.position =
            (f64::from(fraction.clamp(0.0, 1.0)) * player.duration).clamp(0.0, player.duration);
        player.stepped_at = std::time::Instant::now();
        player.shown = None;
        self.speaker.silence();
        self.decode_player(cx);
        self.ensure_player_sound(cx);
        cx.notify();
    }

    fn player_bar_fraction(&self, x: f32) -> f32 {
        let (left, width) = self.player_bar;
        if width <= 0.0 {
            return 0.0;
        }
        ((x - left) / width).clamp(0.0, 1.0)
    }

    fn on_player_bar_press(
        &mut self,
        event: &gpui::MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        let fraction = self.player_bar_fraction(f32::from(event.position.x));
        self.player_seek(fraction, cx);
    }

    fn on_player_bar_drag(
        &mut self,
        event: &gpui::DragMoveEvent<PlayerBarDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let fraction = self.player_bar_fraction(f32::from(event.event.position.x));
        self.player_seek(fraction, cx);
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

    fn release_file(&mut self, path: &std::path::Path, cx: &mut Context<Self>) {
        if self.player.as_ref().map(|player| &player.path) == Some(&path.to_path_buf()) {
            self.close_player(cx);
        }
        if self.hover.as_ref().map(|hover| &hover.path) == Some(&path.to_path_buf()) {
            self.hover = None;
        }
        self.speaker.forget();
        self.thumbnails.forget(path);
        if let Some(decoder) = self.hover_decoder.take() {
            decoder.close();
        }
    }

    fn delete_selected(&mut self, cx: &mut Context<Self>) {
        crate::cues::play(crate::cues::Cue::Remove);
        let Some(victims) = self.confirm_bulk.take() else {
            return;
        };
        let mut refused: Option<String> = None;
        for entry in &victims {
            self.release_file(&entry.path, cx);
            match remove_stubborn(&entry.path) {
                Ok(()) => {
                    self.entries.retain(|other| other.path != entry.path);
                    self.selected.remove(&entry.path);
                }
                Err(error) => {
                    if refused.is_none() {
                        refused = Some(error.to_string());
                    }
                }
            }
        }
        self.revision += 1;
        if let Some(reason) = refused {
            self.app.update(cx, |model, cx| {
                model.notice = Some(reason);
                cx.notify();
            });
        }
        cx.notify();
    }

    fn delete_now(&mut self, cx: &mut Context<Self>) {
        crate::cues::play(crate::cues::Cue::Remove);
        let Some(entry) = self.confirm_delete.take() else {
            return;
        };
        self.release_file(&entry.path, cx);

        let outcome = remove_stubborn(&entry.path);

        match outcome {
            Ok(()) => {
                self.entries.retain(|other| other.path != entry.path);
                self.revision += 1;
                if self.hover.as_ref().map(|hover| &hover.path) == Some(&entry.path) {
                    self.hover = None;
                }
                if self.player.as_ref().map(|player| &player.path) == Some(&entry.path) {
                    self.close_player(cx);
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
        let selected = self.selected.contains(&entry.path);
        let showing_box = selected || lift > 0.02;
        let for_box = entry.path.clone();

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
                    .p(px(4.0))
                    .rounded(rem(RADIUS_MD))
                    .when(selected, |this| this.bg(opacity(colors.primary, 0.06)))
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
                        if this.selecting() {
                            this.toggle_selected(&for_open.path, cx);
                            return;
                        }
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
                                    .rounded(rem(RADIUS_MD))
                                    .overflow_hidden()
                                    .child(match still {
                                        Some(image) => gpui::img(image)
                                            .size_full()
                                            .rounded(rem(RADIUS_MD))
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
                                        "library.modified",
                                        &[(
                                            "date",
                                            &crate::library_ui::format_modified(entry.modified),
                                        )],
                                    )),
                            ),
                    )
                    .when(showing_box, |this| {
                        this.child(
                            div().absolute().top(px(12.0)).left(px(12.0)).child(
                                selection_box(
                                    SharedString::from(format!("library-check-{key}")),
                                    colors,
                                    selected,
                                )
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    |_, _, cx: &mut gpui::App| cx.stop_propagation(),
                                )
                                .on_click(cx.listener(
                                    move |this: &mut Self, _, _, cx| {
                                        cx.stop_propagation();
                                        this.toggle_selected(&for_box, cx);
                                    },
                                )),
                            ),
                        )
                    }),
            )
    }

    fn list_row(&mut self, entry: &Entry, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        let key = entry.path.to_string_lossy().to_string();
        let hover_key = format!("row-{key}");
        let lift = self.transitions.eased(&hover_key);
        let hovered = lift > 0.02;
        let selected = self.selected.contains(&entry.path);
        let still = self.thumbnails.image(&entry.path);
        let length = self
            .thumbnails
            .duration(&entry.path)
            .or(entry.duration_seconds);
        let duration = length.map(library::format_duration);
        let size = crate::export::format_bytes(entry.bytes);
        let modified = format_modified(entry.modified);
        let for_menu = entry.clone();
        let for_open = entry.clone();
        let for_box = entry.path.clone();

        div()
            .w_full()
            .flex_shrink_0()
            .px(px(CARD_GUTTER_PX))
            .py(px(4.0))
            .child(
                div()
                    .id(SharedString::from(hover_key.clone()))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .w_full()
                    .h(px(LIST_ROW_PX - 8.0))
                    .px(px(10.0))
                    .rounded(rem(RADIUS_MD))
                    .border_1()
                    .border_color(mix(colors.border, colors.primary, lift))
                    .when(selected, |this| this.bg(opacity(colors.primary, 0.06)))
                    .cursor_pointer()
                    .on_hover(cx.listener(move |this: &mut Self, is_over: &bool, _, cx| {
                        this.transitions.set(hover_key.clone(), *is_over);
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
                        if this.selecting() {
                            this.toggle_selected(&for_open.path, cx);
                            return;
                        }
                        this.run_action("view", &for_open, cx);
                    }))
                    .when(hovered || selected, |this| {
                        this.child(
                            selection_box(
                                SharedString::from(format!("library-row-check-{key}")),
                                colors,
                                selected,
                            )
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx: &mut gpui::App| {
                                cx.stop_propagation()
                            })
                            .on_click(cx.listener(
                                move |this: &mut Self, _, _, cx| {
                                    cx.stop_propagation();
                                    this.toggle_selected(&for_box, cx);
                                },
                            )),
                        )
                    })
                    .child(
                        div()
                            .w(px(88.0))
                            .h(px(50.0))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(rem(RADIUS_SM))
                            .overflow_hidden()
                            .bg(colors.muted)
                            .child(match still {
                                Some(image) => gpui::img(image)
                                    .size_full()
                                    .rounded(rem(RADIUS_SM))
                                    .object_fit(gpui::ObjectFit::Cover)
                                    .into_any_element(),

                                None => svg()
                                    .size(px(20.0))
                                    .path(icon("oc-video"))
                                    .text_color(opacity(colors.muted_foreground, 0.7))
                                    .into_any_element(),
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(rem(TEXT_SM))
                                    .font_weight(FontWeight::MEDIUM)
                                    .truncate()
                                    .child(entry.name.clone()),
                            )
                            .child(
                                div()
                                    .text_size(rem(TEXT_XS))
                                    .text_color(colors.muted_foreground)
                                    .truncate()
                                    .child(t_args("library.modified", &[("date", &modified)])),
                            ),
                    )
                    .children(duration.map(|text| {
                        div()
                            .flex_shrink_0()
                            .text_size(rem(TEXT_XS))
                            .text_color(colors.muted_foreground)
                            .child(text)
                    }))
                    .child(
                        div()
                            .w(px(90.0))
                            .flex_shrink_0()
                            .text_size(rem(TEXT_XS))
                            .text_color(colors.muted_foreground)
                            .child(size),
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
        let open = self.volume_open || self.volume_dragging;
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
                        this.volume_dragging = true;
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
                    this.volume_dragging = true;
                    let x = f32::from(event.event.position.x);
                    this.volume_from_bar(x, cx);
                },
            ))
            .on_mouse_up(
                gpui::MouseButton::Left,
                cx.listener(|this: &mut Self, _, _, cx| {
                    if this.volume_dragging {
                        this.volume_dragging = false;
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up_out(
                gpui::MouseButton::Left,
                cx.listener(|this: &mut Self, _, _, cx| {
                    if this.volume_dragging {
                        this.volume_dragging = false;
                        cx.notify();
                    }
                }),
            )
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
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|this: &mut Self, _, _, _| {
                    this.volume_dragging = true;
                }),
            )
            .on_hover(cx.listener(move |this: &mut Self, hovered: &bool, _, cx| {
                if !*hovered && this.volume_dragging {
                    return;
                }
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

    fn view_switch(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        let mut toggles = Vec::new();
        for (mode, glyph) in [
            (ViewMode::Grid, "grid-view"),
            (ViewMode::List, "left-to-right-list-dash"),
        ] {
            let id = format!("library-view-{glyph}");
            let progress = self.transitions.eased(&id);
            let active = self.view == mode;
            let hover_id = id.clone();
            toggles.push(
                div()
                    .id(SharedString::from(id))
                    .flex()
                    .size(px(28.0))
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
                    .on_hover(cx.listener(move |this: &mut Self, is_over: &bool, _, cx| {
                        this.transitions.set(hover_id.clone(), *is_over);
                        cx.notify();
                    }))
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        this.view = mode;
                        this.row_height = 0.0;
                        cx.notify();
                    }))
                    .child(
                        svg()
                            .size(px(15.0))
                            .path(icon(glyph))
                            .text_color(colors.foreground),
                    ),
            );
        }

        div()
            .flex()
            .h(px(32.0))
            .flex_shrink_0()
            .items_center()
            .gap(px(2.0))
            .px(px(5.0))
            .rounded(rem(RADIUS_MD))
            .border_1()
            .border_color(colors.border)
            .children(toggles)
    }

    fn bulk_bar(&mut self, shown: &[Entry], cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.colors(cx);
        let chosen = self.selected.len();
        let paths: Vec<PathBuf> = shown.iter().map(|entry| entry.path.clone()).collect();
        let all_chosen = !paths.is_empty() && paths.iter().all(|path| self.selected.contains(path));

        let actions = (chosen > 0).then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .text_size(rem(TEXT_SM))
                        .text_color(colors.muted_foreground)
                        .child(t_args(
                            "library.selected",
                            &[("count", &chosen.to_string())],
                        )),
                )
                .child(
                    Button::new("library-bulk-delete", colors)
                        .variant(ButtonVariant::DestructiveForeground)
                        .icon("delete02")
                        .label(t("common.delete"))
                        .build()
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            let victims: Vec<Entry> = this
                                .entries
                                .iter()
                                .filter(|entry| this.selected.contains(&entry.path))
                                .cloned()
                                .collect();
                            if !victims.is_empty() {
                                this.confirm_bulk = Some(victims);
                            }
                            cx.notify();
                        })),
                )
        });

        div()
            .flex()
            .items_center()
            .justify_between()
            .w_full()
            .child(
                div()
                    .id("library-select-all")
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .px(px(8.0))
                    .cursor_pointer()
                    .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                        if all_chosen {
                            this.selected.clear();
                        } else {
                            this.selected = paths.iter().cloned().collect();
                        }
                        cx.notify();
                    }))
                    .child(selection_box("library-select-all-box", colors, all_chosen))
                    .child(
                        div()
                            .text_size(rem(TEXT_SM))
                            .text_color(colors.muted_foreground)
                            .child(t("library.selectAll")),
                    ),
            )
            .children(actions)
    }

    fn header(
        &mut self,
        shown: &[Entry],
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bulk = self.bulk_bar(shown, cx);
        let view_switch = self.view_switch(cx);
        let announcing = self
            .rescanned_at
            .is_some_and(|at| at.elapsed() < RESCAN_NOTICE);
        let badge = self.upload_badge(cx);
        let colors = self.colors(cx);
        let directory = self.directory(cx);
        let count = shown.len();

        let search = crate::input::text_field(
            "library-search",
            &self.query,
            colors,
            crate::input::FieldStyle {
                height: 32.0,
                placeholder: SharedString::from(t("library.search")),
                leading: Some(icon("search01")),
                ..Default::default()
            },
            window,
        )
        .flex_1()
        .min_w_0()
        .on_key_down(
            cx.listener(|this: &mut Self, event: &gpui::KeyDownEvent, _, cx| {
                if matches!(
                    this.query.buffer.key_down(event),
                    crate::input::TextEvent::Cancel
                ) {
                    this.query.buffer.set("");
                }
                cx.notify();
            }),
        );

        let top = div()
            .flex()
            .items_center()
            .gap(px(20.0))
            .w_full()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_shrink_0()
                    .max_w(px(360.0))
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
            .child(search)
            .child(view_switch)
            .children(badge)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_shrink_0()
                    .items_end()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(t_args("library.count", &[("count", &count.to_string())]))
                    .when(announcing, |this| this.child(t("library.rescanned"))),
            )
            .child(
                Button::new("library-sort", colors)
                    .variant(ButtonVariant::Outline)
                    .icon(if self.ascending {
                        "sorting-one-nine"
                    } else {
                        "sorting-nine-one"
                    })
                    .label(t(self.sort.label_key()))
                    .build()
                    .on_click(
                        cx.listener(|this: &mut Self, event: &gpui::ClickEvent, _, cx| {
                            this.sort_menu_at = event.position();
                            this.sort_flyout = None;
                            this.sort_menu.set_open(true);
                            cx.notify();
                        }),
                    ),
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
            );

        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .w_full()
            .child(top)
            .child(bulk)
    }

    fn sort_row(
        &self,
        colors: Palette,
        key: library::SortKey,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let chosen = self.sort == key;
        menu_action(
            SharedString::from(format!("library-sort-{}", key.id())),
            colors,
            t(key.label_key()),
            if chosen { "tick02" } else { "" },
            chosen,
            false,
        )
        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
            this.sort = key;
            this.sort_flyout = None;
            this.sort_menu.dismiss();
            cx.notify();
        }))
        .into_any_element()
    }

    fn flyout_row(
        &self,
        colors: Palette,
        flyout: SortFlyout,
        label: SharedString,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let open = self.sort_flyout == Some(flyout);
        menu_action(
            SharedString::from(format!("library-flyout-{}", flyout.id())),
            colors,
            label,
            "",
            open,
            false,
        )
        .child(div().flex_1())
        .child(
            svg()
                .size(px(13.0))
                .flex_shrink_0()
                .path(icon("chevron-right"))
                .text_color(opacity(colors.popover_foreground, 0.7)),
        )
        .on_hover(cx.listener(move |this: &mut Self, over: &bool, _, cx| {
            if *over {
                this.sort_flyout = Some(flyout);
                cx.notify();
            }
        }))
        .into_any_element()
    }

    fn sort_menu(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let colors = self.colors(cx);

        let mut rows: Vec<gpui::AnyElement> = Vec::new();
        rows.push(self.sort_row(colors, library::SortKey::Name, cx));
        rows.push(self.sort_row(colors, library::SortKey::Modified, cx));
        rows.push(self.flyout_row(colors, SortFlyout::More, t("library.sort.more").into(), cx));

        for (ascending, label) in [
            (true, "library.sort.ascending"),
            (false, "library.sort.descending"),
        ] {
            let chosen = self.ascending == ascending;
            rows.push(
                menu_action(
                    SharedString::from(format!("library-order-{ascending}")),
                    colors,
                    t(label),
                    if chosen { "tick02" } else { "" },
                    chosen,
                    false,
                )
                .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                    this.ascending = ascending;
                    this.sort_flyout = None;
                    this.sort_menu.dismiss();
                    cx.notify();
                }))
                .into_any_element(),
            );
        }

        rows.push(self.flyout_row(colors, SortFlyout::Group, t("library.group").into(), cx));

        let frame = self.sort_menu.frame();
        if !frame.visible {
            return None;
        }
        let natural = (
            MENU_WIDTH_PX,
            menu_natural_height(rows.len(), MENU_ITEM_HEIGHT_PX),
        );
        let placement = place_anchored(
            frame,
            crate::interaction::OverlaySide::Bottom,
            natural,
            (
                f32::from(self.sort_menu_at.x) - MENU_WIDTH_PX,
                f32::from(self.sort_menu_at.y) + MENU_OFFSET_PX,
            ),
        );

        let flyout = self.sort_flyout.and_then(|flyout| {
            let (anchor, items): (usize, Vec<gpui::AnyElement>) = match flyout {
                SortFlyout::More => (
                    2,
                    library::SORT_KEYS
                        .iter()
                        .filter(|key| {
                            !matches!(key, library::SortKey::Name | library::SortKey::Modified)
                        })
                        .map(|key| self.sort_row(colors, *key, cx))
                        .collect(),
                ),
                SortFlyout::Group => (
                    rows.len() - 1,
                    library::GROUPINGS
                        .iter()
                        .map(|group| {
                            let group = *group;
                            let chosen = self.grouping == group;
                            menu_action(
                                SharedString::from(format!("library-group-{}", group.id())),
                                colors,
                                t(group.label_key()),
                                if chosen { "tick02" } else { "" },
                                chosen,
                                false,
                            )
                            .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                this.grouping = group;
                                this.sort_flyout = None;
                                this.sort_menu.dismiss();
                                cx.notify();
                            }))
                            .into_any_element()
                        })
                        .collect(),
                ),
            };
            if items.is_empty() {
                return None;
            }

            let child = MenuPlacement {
                left: placement.left + placement.width - MENU_OVERLAP_PX,
                top: placement.top + MENU_PAD_PX + anchor as f32 * MENU_ITEM_HEIGHT_PX,
                width: MENU_WIDTH_PX,
                height: menu_natural_height(items.len(), MENU_ITEM_HEIGHT_PX),
                opacity: placement.opacity,
            };

            Some(overlay_layer(
                gpui::Corner::TopLeft,
                child,
                menu_surface(colors, child)
                    .id("library-sort-flyout")
                    .w(px(MENU_WIDTH_PX))
                    .p(px(MENU_PAD_PX))
                    .children(items)
                    .on_hover(cx.listener(move |this: &mut Self, over: &bool, _, cx| {
                        if *over {
                            this.sort_flyout = Some(flyout);
                            cx.notify();
                        }
                    })),
            ))
        });

        Some(
            overlay_root()
                .child(
                    overlay_backdrop("library-sort-backdrop")
                        .on_mouse_up(
                            gpui::MouseButton::Left,
                            cx.listener(|this: &mut Self, _, _, cx| {
                                cx.stop_propagation();
                                this.sort_flyout = None;
                                this.sort_menu.dismiss();
                                cx.notify();
                            }),
                        )
                        .on_mouse_down(
                            gpui::MouseButton::Right,
                            cx.listener(|this: &mut Self, _, _, cx| {
                                cx.stop_propagation();
                                this.sort_flyout = None;
                                this.sort_menu.dismiss();
                                cx.notify();
                            }),
                        ),
                )
                .child(overlay_layer(
                    gpui::Corner::TopLeft,
                    placement,
                    menu_surface(colors, placement)
                        .w(px(MENU_WIDTH_PX))
                        .p(px(MENU_PAD_PX))
                        .children(rows),
                ))
                .children(flyout),
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
                self.hover = None;
                self.speaker.forget();
                self.sound.muted = false;
                if self.sound.volume <= 0.001 {
                    self.sound.volume = 1.0;
                }
                self.speaker.set_volume(self.sound.effective());
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
        self.trim_range = None;
        if self.temp_file.is_some() {
            if let Some(decoder) = self.hover_decoder.take() {
                decoder.close();
            }
            self.speaker.forget();
            self.discard_temp();
        }
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
                stale = player.frame.replace(frame.image);
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

    fn player_entry(&self) -> Option<Entry> {
        let path = self.player.as_ref().map(|player| player.path.clone())?;
        self.entries
            .iter()
            .find(|entry| entry.path == path)
            .cloned()
            .or_else(|| Entry::from_path(path))
    }

    fn edit_from_player(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.player_entry() else {
            return;
        };
        self.run_action("edit", &entry, cx);
    }

    fn toggle_trim(&mut self, cx: &mut Context<Self>) {
        if self.trim_range.take().is_some() {
            cx.notify();
            return;
        }
        let Some(player) = self.player.as_mut() else {
            return;
        };
        player.playing = false;
        self.speaker.silence();
        self.trim_range = Some((0.0, 1.0));
        cx.notify();
    }

    fn trim_fraction(&self, x: f32) -> f32 {
        let (left, width) = self.trim_bar;
        if width <= 0.0 {
            return 0.0;
        }
        ((x - left) / width).clamp(0.0, 1.0)
    }

    fn move_trim_start(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some((_, end)) = self.trim_range else {
            return;
        };
        let start = self.trim_fraction(x);
        self.trim_range = Some((start, end));
        self.player_seek(start, cx);
    }

    fn move_trim_end(&mut self, x: f32, cx: &mut Context<Self>) {
        let Some((start, _)) = self.trim_range else {
            return;
        };
        let end = self.trim_fraction(x);
        self.trim_range = Some((start, end));
        self.player_seek(end, cx);
    }

    fn on_trim_press(
        &mut self,
        event: &gpui::MouseDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        let Some((start, end)) = self.trim_range else {
            return;
        };
        let at = self.trim_fraction(f32::from(event.position.x));
        if (at - start).abs() <= (at - end).abs() {
            self.move_trim_start(event.position.x.into(), cx);
        } else {
            self.move_trim_end(event.position.x.into(), cx);
        }
    }

    fn on_trim_start_drag(
        &mut self,
        event: &gpui::DragMoveEvent<TrimStartDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_trim_start(f32::from(event.event.position.x), cx);
    }

    fn on_trim_end_drag(
        &mut self,
        event: &gpui::DragMoveEvent<TrimEndDrag>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.move_trim_end(f32::from(event.event.position.x), cx);
    }

    fn complain(&mut self, message: String, cx: &mut Context<Self>) {
        self.app.update(cx, |model, cx| {
            model.notice = Some(message);
            cx.notify();
        });
    }

    fn apply_trim(&mut self, cx: &mut Context<Self>) {
        if self.trim_busy {
            return;
        }
        let Some((from, to)) = self.trim_range else {
            return;
        };
        let Some(player) = self.player.as_ref() else {
            return;
        };
        let source = player.path.clone();
        let (start, span) = library::trim_span(from, to, player.duration);
        if span <= 0.05 {
            self.complain(t("library.trim.tooShort"), cx);
            return;
        }
        let Some(destination) = library::temp_file(&source) else {
            self.complain(t("library.trim.noRoom"), cx);
            return;
        };

        self.trim_busy = true;
        if let Some(player) = self.player.as_mut() {
            player.playing = false;
        }
        self.speaker.silence();
        cx.notify();

        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn({
                    let source = source.clone();
                    let destination = destination.clone();
                    async move {
                        if let Some(parent) = destination.parent() {
                            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                        }
                        cutix_export::remux::trim(
                            &source,
                            crate::edit::seconds(start),
                            crate::edit::seconds(span),
                            true,
                            &destination,
                        )
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                    }
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                this.trim_busy = false;
                match outcome {
                    Ok(()) => this.adopt_trimmed(destination, cx),
                    Err(reason) => {
                        let _ = std::fs::remove_file(&destination);
                        this.complain(reason, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn adopt_trimmed(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.discard_temp();
        self.temp_file = Some(path.clone());
        self.trim_range = None;
        self.speaker.forget();
        self.plays += 1;
        let generation = self.plays;
        let mut stale = None;
        if let Some(player) = self.player.as_mut() {
            player.path = path.clone();
            player.position = 0.0;
            player.duration = 0.0;
            player.shown = None;
            player.playing = false;
            player.generation = generation;
            player.stepped_at = std::time::Instant::now();
            stale = player.frame.take();
        }
        if let Some(stale) = stale {
            cx.drop_image(stale, None);
        }
        self.app.update(cx, |model, cx| {
            model.preview_video = Some(path);
            cx.notify();
        });
        self.probe_player(cx);
        self.decode_player(cx);
        cx.notify();
    }

    fn discard_temp(&mut self) {
        let Some(path) = self.temp_file.take() else {
            return;
        };
        self.thumbnails.forget(&path);
        let _ = remove_stubborn(&path);
    }

    fn sweep_temp(&mut self) {
        if self.swept {
            return;
        }
        self.swept = true;
        let Some(directory) = library::temp_directory() else {
            return;
        };
        library::prune_temp(
            &directory,
            std::time::SystemTime::now(),
            library::TEMP_MAX_AGE,
        );
    }

    fn watch_upload(&mut self, cx: &mut Context<Self>) {
        let Some((path, seen)) = self.pending_upload.clone() else {
            return;
        };
        let busy = self.app.read(cx).uploads.is_busy();
        if library::upload_settled(seen, busy) {
            let _ = remove_stubborn(&path);
            self.pending_upload = None;
        } else if busy && !seen {
            self.pending_upload = Some((path, true));
        }
    }

    fn trim_strip(&mut self, colors: Palette, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (start, end) = self.trim_range?;
        let duration = self.player.as_ref().map(|player| player.duration)?;
        let (from, span) = library::trim_span(start, end, duration);
        let low = start.min(end);
        let high = start.max(end);
        let busy = self.trim_busy;

        let handle = |id: &'static str, at: f32| {
            div()
                .id(SharedString::from(id))
                .absolute()
                .left(relative(at))
                .top(px(-6.0))
                .ml(px(-5.0))
                .w(px(10.0))
                .h(px(20.0))
                .rounded(px(3.0))
                .cursor_pointer()
                .bg(colors.primary)
        };

        Some(
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .w_full()
                .child(
                    div()
                        .id("player-trim-bar")
                        .relative()
                        .flex_1()
                        .h(px(8.0))
                        .my(px(8.0))
                        .rounded(px(4.0))
                        .cursor_pointer()
                        .bg(opacity(colors.muted, 0.6))
                        .on_mouse_down(gpui::MouseButton::Left, cx.listener(Self::on_trim_press))
                        .child(trim_bar_probe(cx))
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .bottom_0()
                                .left(relative(low))
                                .w(relative((high - low).max(0.0)))
                                .bg(opacity(colors.primary, 0.45)),
                        )
                        .child(
                            handle("player-trim-start", low)
                                .on_drag(TrimStartDrag, |_, _, _, cx| {
                                    gpui::AppContext::new(cx, |_| gpui::Empty)
                                })
                                .on_drag_move::<TrimStartDrag>(
                                    cx.listener(Self::on_trim_start_drag),
                                ),
                        )
                        .child(
                            handle("player-trim-end", high)
                                .on_drag(TrimEndDrag, |_, _, _, cx| {
                                    gpui::AppContext::new(cx, |_| gpui::Empty)
                                })
                                .on_drag_move::<TrimEndDrag>(cx.listener(Self::on_trim_end_drag)),
                        ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(rem(TEXT_XS))
                        .text_color(colors.muted_foreground)
                        .child(if busy {
                            t("library.trim.busy")
                        } else {
                            t_args(
                                "library.trim.range",
                                &[
                                    ("start", &library::format_duration(from)),
                                    ("length", &library::format_duration(span)),
                                ],
                            )
                        }),
                )
                .child(
                    div()
                        .id("player-trim-apply")
                        .size(px(28.0))
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
                        .rounded(rem(RADIUS_SM))
                        .cursor_pointer()
                        .bg(opacity(colors.primary, if busy { 0.4 } else { 0.9 }))
                        .child(
                            svg()
                                .size(px(14.0))
                                .path(icon("tick02"))
                                .text_color(colors.primary_foreground),
                        )
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.apply_trim(cx);
                        })),
                )
                .child(
                    div()
                        .id("player-trim-cancel")
                        .size(px(28.0))
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
                        .rounded(rem(RADIUS_SM))
                        .cursor_pointer()
                        .bg(opacity(colors.accent, 0.6))
                        .child(
                            svg()
                                .size(px(14.0))
                                .path(icon("win-close"))
                                .text_color(colors.foreground),
                        )
                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                            this.trim_range = None;
                            cx.notify();
                        })),
                ),
        )
    }

    fn player_modal(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let weight = self
            .player
            .as_ref()
            .and_then(|player| self.entry_size(&player.path))
            .map(crate::export::format_bytes);
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

        let trimming = self.trim_range.is_some();
        let strip = self.trim_strip(colors, cx);

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
                        this.speaker.silence();
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
                                        .size(px(28.0))
                                        .flex_shrink_0()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(rem(RADIUS_SM))
                                        .cursor_pointer()
                                        .bg(opacity(colors.accent, 0.6))
                                        .hover(|style| style.bg(colors.destructive))
                                        .child(
                                            svg()
                                                .size(px(14.0))
                                                .path(icon("win-close"))
                                                .text_color(colors.foreground),
                                        )
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
                                        .id("player-stage")
                                        .absolute()
                                        .inset_0()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                            this.toggle_player(cx);
                                        }))
                                        .rounded(rem(RADIUS_SM))
                                        .overflow_hidden()
                                        .child(match frame {
                                            Some(image) => gpui::img(image)
                                                .size_full()
                                                .rounded(rem(RADIUS_SM))
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
                                .id("player-progress")
                                .relative()
                                .w_full()
                                .py(px(6.0))
                                .cursor_pointer()
                                .on_mouse_down(
                                    gpui::MouseButton::Left,
                                    cx.listener(Self::on_player_bar_press),
                                )
                                .on_drag(PlayerBarDrag, |_, _, _, cx| {
                                    gpui::AppContext::new(cx, |_| gpui::Empty)
                                })
                                .on_drag_move::<PlayerBarDrag>(
                                    cx.listener(Self::on_player_bar_drag),
                                )
                                .child(
                                    div()
                                        .w_full()
                                        .h(px(4.0))
                                        .rounded(px(2.0))
                                        .overflow_hidden()
                                        .bg(opacity(colors.muted, 0.6))
                                        .child(
                                            div().h_full().w(relative(fraction)).bg(colors.primary),
                                        ),
                                )
                                .child(player_bar_probe(cx)),
                        )
                        .child(bar)
                        .children(strip)
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
                                            this.toggle_player(cx);
                                        })),
                                )
                                .child(
                                    div()
                                        .text_size(rem(TEXT_XS))
                                        .text_color(colors.muted_foreground)
                                        .child(match weight {
                                            Some(weight) => {
                                                format!("{elapsed} / {total} · {weight}")
                                            }
                                            None => format!("{elapsed} / {total}"),
                                        }),
                                )
                                .child(volume)
                                .child(div().flex_1())
                                .children(speeds)
                                .child(
                                    div()
                                        .id("player-edit")
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
                                                .path(icon("edit03"))
                                                .text_color(colors.foreground),
                                        )
                                        .child(t("library.action.edit"))
                                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                            this.edit_from_player(cx);
                                        })),
                                )
                                .child(
                                    div()
                                        .id("player-trim")
                                        .size(px(28.0))
                                        .flex()
                                        .flex_shrink_0()
                                        .items_center()
                                        .justify_center()
                                        .rounded(rem(RADIUS_SM))
                                        .cursor_pointer()
                                        .bg(if trimming {
                                            opacity(colors.primary, 0.9)
                                        } else {
                                            opacity(colors.accent, 0.6)
                                        })
                                        .child(
                                            svg().size(px(14.0)).path(icon("scissor")).text_color(
                                                if trimming {
                                                    colors.primary_foreground
                                                } else {
                                                    colors.foreground
                                                },
                                            ),
                                        )
                                        .on_click(cx.listener(|this: &mut Self, _, _, cx| {
                                            this.toggle_trim(cx);
                                        })),
                                )
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
        if self.temp_file.as_deref() == Some(path.as_path()) {
            self.temp_file = None;
            self.pending_upload = Some((path.clone(), false));
        }
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
                .child(
                    overlay_backdrop("library-menu-backdrop")
                        .on_mouse_up(
                            gpui::MouseButton::Left,
                            cx.listener(|this: &mut Self, _, _, cx| {
                                cx.stop_propagation();
                                this.menu.dismiss();
                                this.menu_for = None;
                                cx.notify();
                            }),
                        )
                        .on_mouse_down(
                            gpui::MouseButton::Right,
                            cx.listener(|this: &mut Self, _, _, cx| {
                                cx.stop_propagation();
                                this.menu.dismiss();
                                this.menu_for = None;
                                cx.notify();
                            }),
                        ),
                )
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

    fn entry_size(&self, path: &std::path::Path) -> Option<u64> {
        self.entries
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| entry.bytes)
            .or_else(|| std::fs::metadata(path).ok().map(|meta| meta.len()))
    }

    fn entry_length(&self, entry: &Entry) -> Option<f64> {
        entry
            .duration_seconds
            .filter(|seconds| *seconds > 0.0)
            .or_else(|| self.thumbnails.duration(&entry.path))
    }

    fn delete_dialog(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let victims = self.confirm_bulk.clone();
        let one = self.confirm_delete.clone();
        let many = victims.is_some();
        let colors = self.colors(cx);
        let (title, body, details) = match (&victims, &one) {
            (Some(victims), _) => {
                let count = victims.len().to_string();
                let bytes: u64 = victims.iter().map(|entry| entry.bytes).sum();
                let span: f64 = victims
                    .iter()
                    .filter_map(|entry| self.entry_length(entry))
                    .sum();
                (
                    t_args("library.delete.bulk.title", &[("count", &count)]),
                    t_args("library.delete.bulk.body", &[("count", &count)]),
                    t_args(
                        "library.delete.details",
                        &[
                            ("duration", &library::format_duration(span)),
                            ("size", &crate::export::format_bytes(bytes)),
                        ],
                    ),
                )
            }
            (_, Some(entry)) => (
                t("library.delete.title"),
                t_args("library.delete.body", &[("name", &entry.name)]),
                t_args(
                    "library.delete.details",
                    &[
                        (
                            "duration",
                            &match self.entry_length(entry) {
                                Some(seconds) => library::format_duration(seconds),
                                None => t("library.unknown"),
                            },
                        ),
                        ("size", &crate::export::format_bytes(entry.bytes)),
                    ],
                ),
            ),
            _ => return None,
        };

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
                                .child(title),
                        )
                        .child(
                            div()
                                .text_size(rem(TEXT_SM))
                                .text_color(colors.muted_foreground)
                                .child(body),
                        )
                        .child(
                            div()
                                .text_size(rem(TEXT_XS))
                                .text_color(colors.muted_foreground)
                                .child(details),
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
                                            this.confirm_bulk = None;
                                            cx.notify();
                                        })),
                                )
                                .child(
                                    Button::new("library-delete-confirm", colors)
                                        .variant(ButtonVariant::Destructive)
                                        .label(t("common.delete"))
                                        .build()
                                        .on_click(cx.listener(move |this: &mut Self, _, _, cx| {
                                            if many {
                                                this.delete_selected(cx);
                                            } else {
                                                this.delete_now(cx);
                                            }
                                        })),
                                ),
                        ),
                ),
        )
    }
}

pub fn format_modified(unix_seconds: i64) -> String {
    crate::state::format_date(&youtube::iso_date(unix_seconds))
}

impl Render for LibraryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let awake = window.is_window_active();
        if !awake && self.autoscroll.take().is_some() {
            cx.notify();
        }
        if self.overlay_open() && self.hover.is_some() {
            self.hover = None;
            self.speaker.silence();
        }
        if !awake {
            if self.hover.is_some() {
                self.hover = None;
                self.speaker.silence();
            }
            if let Some(player) = self.player.as_mut() {
                if player.playing {
                    player.playing = false;
                    self.speaker.silence();
                }
            }
        }
        let previewing =
            awake && (self.hover.is_some() || self.player.as_ref().is_some_and(|p| p.playing));
        if previewing {
            let view = cx.entity();
            cx.defer(move |cx| {
                view.update(cx, |this, cx| {
                    this.advance_preview(cx);
                    this.advance_player(cx);
                });
            });
        }
        let announcing = self
            .rescanned_at
            .is_some_and(|at| at.elapsed() < RESCAN_NOTICE);
        if self.transitions.animating()
            || self.menu.animating()
            || self.sort_menu.animating()
            || announcing
            || previewing
        {
            window.request_animation_frame();
        }
        self.sweep_temp();
        self.watch_upload(cx);
        self.ensure_scanned(cx);
        self.drive_autoscroll(window);
        self.glide_scroll(window);
        self.probe_next(cx);
        self.probe_metadata(cx);
        self.sync_player(cx);
        self.refresh_visible();
        let colors = self.colors(cx);
        let entries = std::mem::take(&mut self.visible);
        let root = self.directory(cx);
        let grouping = self.grouping;
        let header = self.header(&entries, window, cx);
        let sorting = self.sort_menu(cx);
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
            let listing = self.view == ViewMode::List;
            let columns = if listing { 1 } else { GRID_COLUMNS };
            let mut plan: Vec<GridRow> = Vec::new();
            let mut cursor = 0usize;
            while cursor < entries.len() {
                if grouping != library::Grouping::None {
                    let label = match grouping {
                        library::Grouping::Folder => library::folder_of(&entries[cursor], &root),
                        _ => format_modified(entries[cursor].modified),
                    };
                    let same = |entry: &Entry| match grouping {
                        library::Grouping::Folder => library::folder_of(entry, &root) == label,
                        _ => format_modified(entry.modified) == label,
                    };
                    let end = entries[cursor..]
                        .iter()
                        .position(|entry| !same(entry))
                        .map(|offset| cursor + offset)
                        .unwrap_or(entries.len());
                    plan.push(GridRow::Header(label));
                    while cursor < end {
                        let stop = (cursor + columns).min(end);
                        plan.push(GridRow::Cards(cursor..stop));
                        cursor = stop;
                    }
                } else {
                    let stop = (cursor + columns).min(entries.len());
                    plan.push(GridRow::Cards(cursor..stop));
                    cursor = stop;
                }
            }

            let card_height = if listing {
                LIST_ROW_PX
            } else if self.row_height > 0.0 {
                self.row_height
            } else {
                let width = if self.grid_view.0 > 0.0 {
                    self.grid_view.0
                } else {
                    ESTIMATED_GRID_WIDTH
                };
                let inner = (width / GRID_COLUMNS as f32) - CARD_GUTTER_PX * 2.0;
                inner * CARD_THUMBNAIL_RATIO + CARD_TEXT_PX + CARD_GUTTER_PX * 2.0
            };
            let height_of = |row: &GridRow| match row {
                GridRow::Header(_) => GROUP_HEADER_PX,
                GridRow::Cards(_) => card_height,
            };

            let viewport = if self.grid_view.1 > 0.0 {
                self.grid_view.1
            } else {
                ESTIMATED_GRID_HEIGHT
            };
            let scrolled = -f32::from(self.scroll.offset().y);

            let mut above = 0.0f32;
            let mut first = 0usize;
            while first < plan.len() {
                let height = height_of(&plan[first]);
                if above + height > scrolled - viewport * OVERSCAN {
                    break;
                }
                above += height;
                first += 1;
            }

            let mut drawn = 0.0f32;
            let mut last = first;
            let reach = viewport * (1.0 + OVERSCAN * 2.0);
            while last < plan.len() && drawn < reach {
                drawn += height_of(&plan[last]);
                last += 1;
            }

            let below: f32 = plan[last..].iter().map(height_of).sum();

            let mut painted: Vec<gpui::AnyElement> = Vec::new();
            let mut measured = false;
            for row in &plan[first..last] {
                match row {
                    GridRow::Header(label) => painted.push(
                        div()
                            .h(px(GROUP_HEADER_PX))
                            .flex()
                            .items_center()
                            .px(px(CARD_GUTTER_PX))
                            .text_size(rem(TEXT_XS))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.muted_foreground)
                            .child(SharedString::from(label.clone()))
                            .into_any_element(),
                    ),
                    GridRow::Cards(span) => {
                        if listing {
                            let rows: Vec<_> = entries[span.clone()]
                                .iter()
                                .map(|entry| self.list_row(entry, cx))
                                .collect();
                            painted.push(
                                div()
                                    .flex()
                                    .flex_col()
                                    .w_full()
                                    .children(rows)
                                    .into_any_element(),
                            );
                            continue;
                        }
                        let cards: Vec<_> = entries[span.clone()]
                            .iter()
                            .map(|entry| self.card(entry, cx))
                            .collect();
                        let mut strip = div().flex().w_full().children(cards);
                        if !measured {
                            measured = true;
                            strip = strip.child(row_probe(cx));
                        }
                        painted.push(strip.relative().into_any_element());
                    }
                }
            }

            let marker = self.autoscroll.map(|(x, anchor, pointer)| {
                let gap = pointer - anchor;
                let glyph = if gap.abs() <= AUTOSCROLL_DEAD_ZONE {
                    "arrows-vertical"
                } else if gap > 0.0 {
                    "arrow-down"
                } else {
                    "arrow-up"
                };
                gpui::deferred(
                    div()
                        .absolute()
                        .left(px(x - self.grid_origin.0 - AUTOSCROLL_MARKER_PX / 2.0))
                        .top(px(y_of(anchor, self.grid_origin.1)))
                        .size(px(AUTOSCROLL_MARKER_PX))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(AUTOSCROLL_MARKER_PX / 2.0))
                        .border_1()
                        .border_color(opacity(colors.primary, 0.9))
                        .bg(opacity(colors.popover, 0.92))
                        .shadow_lg()
                        .child(
                            svg()
                                .size(px(16.0))
                                .path(icon(glyph))
                                .text_color(colors.primary),
                        ),
                )
                .with_priority(3)
            });

            div()
                .relative()
                .flex()
                .flex_1()
                .w_full()
                .min_h_0()
                .child(frame_probe(cx))
                .child(
                    div()
                        .id("library-grid")
                        .relative()
                        .flex()
                        .flex_col()
                        .size_full()
                        .overflow_y_scroll()
                        .track_scroll(&self.scroll)
                        .on_scroll_wheel(cx.listener(Self::on_grid_wheel))
                        .on_mouse_down(gpui::MouseButton::Middle, cx.listener(Self::on_grid_middle))
                        .on_mouse_move(cx.listener(Self::on_grid_move))
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(|this: &mut Self, _, _, cx| {
                                if this.autoscroll.take().is_some() {
                                    cx.notify();
                                }
                            }),
                        )
                        .child(grid_probe(cx))
                        .child(div().h(px(above)).flex_shrink_0())
                        .children(painted)
                        .child(div().h(px(below.max(0.0))).flex_shrink_0()),
                )
                .children(marker)
                .into_any_element()
        };
        self.visible = entries;

        div()
            .track_focus(&self.focus)
            .key_context("Library")
            .on_key_down(cx.listener(
                |this: &mut Self, event: &gpui::KeyDownEvent, window: &mut Window, cx| {
                    if event.keystroke.key == "space"
                        && this.player.is_some()
                        && !this.query.focus.is_focused(window)
                    {
                        cx.stop_propagation();
                        this.toggle_player(cx);
                        return;
                    }
                    if event.keystroke.key != "escape" {
                        return;
                    }

                    if this.autoscroll.take().is_some() {
                        cx.notify();
                        return;
                    }
                    if !this.dismiss(cx) {
                        this.app.update(cx, |model, cx| {
                            model.route = Route::Home;
                            cx.notify();
                        });
                    }
                    cx.notify();
                },
            ))
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .p(px(24.0))
            .bg(colors.background)
            .text_color(colors.foreground)
            .child(header)
            .child(body)
            .children(sorting)
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
        for key in ["library.modified", "library.count"] {
            let filled = t_args(key, &[("date", "2026-08-15"), ("count", "3")]);
            assert_ne!(filled, key, "{key}");
            assert!(!filled.contains('{'), "{filled}");
        }
    }

    #[test]
    fn a_write_time_reads_as_a_calendar_day() {
        assert_eq!(format_modified(1_700_000_000), "14.11.2023");
        assert_eq!(format_modified(0), "01.01.1970");
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
        {
            let key = "library.delete.title";
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

    #[test]
    fn picking_videos_out_and_cutting_one_up_is_all_spelled_out_for_the_reader() {
        for key in [
            "library.selectAll",
            "library.trim.busy",
            "library.trim.noRoom",
            "library.trim.tooShort",
        ] {
            assert_ne!(t(key), key, "{key}");
        }
        for key in [
            "library.selected",
            "library.delete.bulk.title",
            "library.delete.bulk.body",
        ] {
            let filled = t_args(key, &[("count", "3")]);
            assert_ne!(filled, key, "{key}");
            assert!(filled.contains('3'), "{filled}");
            assert!(!filled.contains('{'), "{filled}");
        }
        let range = t_args(
            "library.trim.range",
            &[("start", "0:05"), ("length", "0:12")],
        );
        assert!(range.contains("0:05") && range.contains("0:12"), "{range}");
        assert!(!range.contains('{'), "{range}");
    }

    #[test]
    fn the_icons_the_picking_and_cutting_controls_draw_all_exist() {
        for glyph in ["tick02", "scissor", "edit03", "win-close", "delete02"] {
            assert!(crate::assets::icon_exists(glyph), "{glyph}");
        }
    }

    #[test]
    fn a_file_that_is_already_gone_counts_as_deleted() {
        let directory = std::env::temp_dir().join(format!(
            "cutix-library-ui-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).expect("workspace");
        let clip = directory.join("clip.mp4");
        std::fs::write(&clip, b"clip").expect("write");

        assert!(remove_stubborn(&clip).is_ok());
        assert!(!clip.exists());
        assert!(
            remove_stubborn(&clip).is_ok(),
            "asking twice is not a failure"
        );

        let _ = std::fs::remove_dir_all(&directory);
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    use gpui::TestAppContext;

    fn scratch() -> (tempfile::TempDir, Vec<Entry>) {
        let directory = tempfile::tempdir().expect("temp dir");
        let mut entries = Vec::new();
        for name in ["one.mp4", "two.mp4", "three.mp4"] {
            let path = directory.path().join(name);
            std::fs::write(&path, b"not really a video").expect("write");
            entries.push(Entry::from_path(path).expect("an entry"));
        }
        (directory, entries)
    }

    fn view(entries: Vec<Entry>, cx: &mut TestAppContext) -> Entity<LibraryView> {
        let store = cutix_project::ProjectStore::new(
            tempfile::tempdir().expect("temp dir").keep().join("cutix"),
        );
        cx.update(|cx| {
            let app = cx.new(|_| AppModel::new(store, true));
            cx.new(|cx| {
                let mut view = LibraryView::new(app, cx);
                view.entries = entries;
                view
            })
        })
    }

    #[gpui::test]
    fn a_click_opens_the_player_until_the_first_video_is_picked(cx: &mut TestAppContext) {
        let (_guard, entries) = scratch();
        let first = entries[0].path.clone();
        let view = view(entries, cx);

        view.update(cx, |this, _| {
            assert!(!this.selecting(), "nothing is picked to begin with");
        });

        view.update(cx, |this, cx| {
            this.toggle_selected(&first, cx);
            assert!(
                this.selecting(),
                "one pick puts the browser in picking mode"
            );
        });

        view.update(cx, |this, cx| {
            this.toggle_selected(&first, cx);
            assert!(
                !this.selecting(),
                "unpicking the last one hands clicks back to the player"
            );
        });
    }

    #[gpui::test]
    fn escape_drops_the_picks_before_it_leaves_the_browser(cx: &mut TestAppContext) {
        let (_guard, entries) = scratch();
        let first = entries[0].path.clone();
        let view = view(entries, cx);

        view.update(cx, |this, cx| {
            this.toggle_selected(&first, cx);
            assert!(this.dismiss(cx), "the picks are what escape takes away");
            assert!(!this.selecting());
            assert!(!this.dismiss(cx), "and then there is nothing left to undo");
        });
    }

    #[gpui::test]
    fn deleting_a_batch_takes_every_picked_file_off_the_disk(cx: &mut TestAppContext) {
        let (_guard, entries) = scratch();
        let victims = vec![entries[0].clone(), entries[2].clone()];
        let survivor = entries[1].path.clone();
        let view = view(entries, cx);

        view.update(cx, |this, cx| {
            for entry in &victims {
                this.toggle_selected(&entry.path, cx);
            }
            this.confirm_bulk = Some(victims.clone());
            this.delete_selected(cx);
        });

        for entry in &victims {
            assert!(!entry.path.exists(), "{}", entry.path.display());
        }
        assert!(survivor.exists(), "an unpicked video is left alone");

        view.update(cx, |this, _| {
            assert_eq!(this.entries.len(), 1);
            assert_eq!(this.entries[0].path, survivor);
            assert!(!this.selecting(), "the picks go away with the files");
            assert!(this.confirm_bulk.is_none());
        });
    }

    #[gpui::test]
    fn the_scissors_open_and_close_a_range_over_the_whole_clip(cx: &mut TestAppContext) {
        let (_guard, entries) = scratch();
        let path = entries[0].path.clone();
        let view = view(entries, cx);

        view.update(cx, |this, cx| {
            this.player = Some(Player {
                path,
                duration: 20.0,
                position: 0.0,
                frame: None,
                playing: true,
                speed: 1.0,
                shown: None,
                stepped_at: std::time::Instant::now(),
                generation: 1,
            });

            this.toggle_trim(cx);
            assert_eq!(this.trim_range, Some((0.0, 1.0)), "the whole clip is kept");
            assert!(
                !this.player.as_ref().expect("a player").playing,
                "cutting pauses the picture"
            );

            this.trim_bar = (0.0, 100.0);
            this.move_trim_start(25.0, cx);
            this.move_trim_end(75.0, cx);
            assert_eq!(this.trim_range, Some((0.25, 0.75)));
            let (start, span) = library::trim_span(0.25, 0.75, 20.0);
            assert!((start - 5.0).abs() < 1e-9);
            assert!((span - 10.0).abs() < 1e-9);

            this.toggle_trim(cx);
            assert!(
                this.trim_range.is_none(),
                "the scissors put themselves away"
            );
        });
    }

    #[gpui::test]
    fn closing_the_viewer_sweeps_the_cut_piece_away(cx: &mut TestAppContext) {
        let (_guard, entries) = scratch();
        let path = entries[0].path.clone();
        let view = view(entries, cx);

        view.update(cx, |this, cx| {
            this.player = Some(Player {
                path: path.clone(),
                duration: 20.0,
                position: 0.0,
                frame: None,
                playing: false,
                speed: 1.0,
                shown: None,
                stepped_at: std::time::Instant::now(),
                generation: 1,
            });
            this.temp_file = Some(path.clone());
            this.close_player(cx);
        });

        assert!(
            !path.exists(),
            "the working copy does not outlive the viewer"
        );
        view.update(cx, |this, _| assert!(this.temp_file.is_none()));
    }

    #[gpui::test]
    fn a_cut_piece_handed_to_youtube_waits_for_the_upload_to_finish(cx: &mut TestAppContext) {
        let (_guard, entries) = scratch();
        let path = entries[0].path.clone();
        let view = view(entries, cx);

        view.update(cx, |this, cx| {
            this.player = Some(Player {
                path: path.clone(),
                duration: 20.0,
                position: 0.0,
                frame: None,
                playing: false,
                speed: 1.0,
                shown: None,
                stepped_at: std::time::Instant::now(),
                generation: 1,
            });
            this.temp_file = Some(path.clone());
            this.upload_from_player(cx);
        });

        assert!(path.exists(), "the upload still needs the file");

        view.update(cx, |this, cx| {
            this.app.update(cx, |model, _| {
                model.uploads.active = 1;
            });
            this.watch_upload(cx);
            assert!(this.pending_upload.is_some(), "the upload is under way");

            this.app.update(cx, |model, _| {
                model.uploads.active = 0;
                model.uploads.finished = 1;
            });
            this.watch_upload(cx);
            assert!(this.pending_upload.is_none());
        });

        assert!(!path.exists(), "and then the working copy is swept away");
    }
}
