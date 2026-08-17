use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cutix_export::{
    backend::AudioSupport, default_backend, ExportPresetId, ExportQuality, ExportRequest, Stage,
    EXPORT_PRESETS, EXPORT_QUALITIES, PACKAGE_EXTENSION,
};
use cutix_i18n::{t, t_args};
use cutix_playback::StoreResolver;
use cutix_project::MediaStore;
use gpui::{div, prelude::*, px, Context, Div, FontWeight, SharedString};

use crate::components::{Button, ButtonVariant};
use crate::state::AppModel;
use crate::theme::{
    opacity, rem, Palette, RADIUS_LG, RADIUS_MD, RADIUS_SM, TEXT_LG, TEXT_SM, TEXT_XS,
};

const DIALOG_WIDTH_PX: f32 = 420.0;
const DIALOG_PADDING_PX: f32 = 20.0;

const POLL_INTERVAL: Duration = Duration::from_millis(33);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportMode {
    Video,

    Project,

    YouTube,
}

pub const EXPORT_MODES: [ExportMode; 3] =
    [ExportMode::Video, ExportMode::Project, ExportMode::YouTube];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportSection {
    Mode,
    Resolution,
    Destination,
    Quality,
    Audio,

    Publish,
}

impl ExportSection {
    pub fn key(self) -> &'static str {
        match self {
            Self::Mode => "mode",
            Self::Resolution => "resolution",
            Self::Destination => "destination",
            Self::Quality => "quality",
            Self::Audio => "audio",
            Self::Publish => "publish",
        }
    }

    pub fn title_key(self) -> &'static str {
        match self {
            Self::Mode => "export.mode",
            Self::Resolution => "export.resolution",
            Self::Destination => "export.destination",
            Self::Quality => "export.quality",
            Self::Audio => "export.audio",
            Self::Publish => "youtube.publish.file",
        }
    }
}

pub const DEFAULT_COLLAPSED: [ExportSection; 2] = [ExportSection::Quality, ExportSection::Audio];

impl ExportMode {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Video => "export.mode.video",
            Self::Project => "export.mode.project",
            Self::YouTube => "export.mode.youtube",
        }
    }

    pub fn hint_key(self) -> &'static str {
        match self {
            Self::Video => "export.mode.video.hint",
            Self::Project => "export.mode.project.hint",
            Self::YouTube => "export.mode.youtube.hint",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Video => "video",
            Self::Project => "project",
            Self::YouTube => "youtube",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Video | Self::YouTube => cutix_export::ExportFormat::Mp4.extension(),
            Self::Project => PACKAGE_EXTENSION,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ExportStatus {
    Idle,
    Running {
        fraction: f32,
        frame: u64,
        total: u64,
        frames_per_second: f32,
        stage: Stage,
    },
    Done {
        mode: ExportMode,
        video: PathBuf,
        audio: Option<PathBuf>,
        has_audio: bool,
        entries: usize,
        bytes: u64,
        frames_per_second: f32,
        outputs: usize,
    },
    Failed(String),
}

enum RunOutcome {
    Video(cutix_export::ExportOutcome),
    Package(cutix_export::PackageOutcome),
}

#[derive(Default)]
struct SharedState {
    progress: Option<cutix_export::Progress>,
    outcome: Option<Result<RunOutcome, String>>,
    outputs: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct YoutubeSummary {
    pub configured: bool,
    pub channel: Option<String>,
}

impl YoutubeSummary {
    pub fn read(_now: i64) -> Self {
        let directory = youtube::data_directory();
        let accounts = youtube::Accounts::load(&directory);
        Self {
            configured: youtube::session::is_supported(),
            channel: accounts.active().map(youtube::Account::display_name),
        }
    }

    pub fn is_ready(&self) -> bool {
        self.configured && self.channel.is_some()
    }
}

pub struct ExportSession {
    pub open: bool,
    pub mode: ExportMode,

    pub youtube_source: Option<PathBuf>,

    pub youtube: YoutubeSummary,
    pub preset: ExportPresetId,

    pub batch: Vec<ExportPresetId>,
    pub quality: ExportQuality,
    pub include_audio: bool,
    pub destination: Option<PathBuf>,
    pub status: ExportStatus,
    pub collapsed: Vec<ExportSection>,

    started_at: Option<Instant>,
    cancel: Arc<AtomicBool>,
    shared: Arc<Mutex<SharedState>>,
}

impl Default for ExportSession {
    fn default() -> Self {
        Self {
            open: false,
            mode: ExportMode::Video,
            youtube_source: None,
            youtube: YoutubeSummary::default(),
            preset: ExportPresetId::Project,
            batch: vec![ExportPresetId::Project],
            quality: ExportQuality::High,
            include_audio: true,
            destination: None,
            status: ExportStatus::Idle,
            collapsed: DEFAULT_COLLAPSED.to_vec(),
            started_at: None,
            cancel: Arc::new(AtomicBool::new(false)),
            shared: Arc::new(Mutex::new(SharedState::default())),
        }
    }
}

impl ExportSession {
    pub fn is_running(&self) -> bool {
        matches!(self.status, ExportStatus::Running { .. })
    }

    pub fn is_collapsed(&self, section: ExportSection) -> bool {
        self.collapsed.contains(&section)
    }

    pub fn toggle_section(&mut self, section: ExportSection) {
        match self.collapsed.iter().position(|entry| *entry == section) {
            Some(index) => {
                self.collapsed.remove(index);
            }
            None => self.collapsed.push(section),
        }
    }

    pub fn toggle_preset(&mut self, id: ExportPresetId) {
        if let Some(index) = self.batch.iter().position(|entry| *entry == id) {
            if self.batch.len() > 1 {
                self.batch.remove(index);
            }
        } else {
            self.batch.push(id);
        }
        self.preset = self.batch[0];
    }

    pub fn set_mode(&mut self, mode: ExportMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        if mode == ExportMode::YouTube {
            self.youtube = YoutubeSummary::read(youtube::now_unix());
        }
        if let Some(destination) = self.destination.take() {
            self.destination = Some(destination.with_extension(mode.extension()));
        }
    }

    pub fn resolved_destination(&self, project_name: &str) -> PathBuf {
        self.destination
            .clone()
            .unwrap_or_else(|| default_destination(project_name))
            .with_extension(self.mode.extension())
    }
}

pub fn batch_destinations(base: &Path, presets: &[ExportPresetId]) -> Vec<PathBuf> {
    if presets.len() <= 1 {
        return vec![base.to_path_buf()];
    }
    let extension = base
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or(cutix_export::ExportFormat::Mp4.extension())
        .to_owned();
    let stem = base
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("export")
        .to_owned();
    presets
        .iter()
        .map(|id| base.with_file_name(format!("{stem}-{}.{extension}", id.slug())))
        .collect()
}

pub fn audio_support() -> AudioSupport {
    default_backend()
        .map(|factory| factory.audio_support())
        .unwrap_or(AudioSupport::None)
}

pub fn output_size(model: &AppModel) -> (u32, u32) {
    let canvas = model
        .project
        .as_ref()
        .map(|project| {
            (
                project.settings.canvas_size.width,
                project.settings.canvas_size.height,
            )
        })
        .unwrap_or((1920, 1080));
    preset_size(canvas, model.export.preset)
}

pub fn preset_size(canvas: (u32, u32), id: ExportPresetId) -> (u32, u32) {
    let (width, height) = cutix_export::preset(id).resolution.unwrap_or(canvas);
    (
        cutix_export::fit::even(width.max(2)),
        cutix_export::fit::even(height.max(2)),
    )
}

pub fn default_destination(project_name: &str) -> PathBuf {
    export_directory().join(format!(
        "{}.{}",
        safe_file_name(project_name),
        cutix_export::ExportFormat::Mp4.extension()
    ))
}

pub fn export_directory() -> PathBuf {
    if let Some(directory) = std::env::var_os("CUTIX_EXPORT_DIR") {
        return PathBuf::from(directory);
    }
    dirs::video_dir()
        .unwrap_or_else(|| std::env::temp_dir())
        .join("cutix")
}

fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, ' ' | '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "export".to_owned()
    } else {
        trimmed
    }
}

impl AppModel {
    pub fn toggle_export_dialog(&mut self, cx: &mut Context<Self>) {
        if self.export.open && self.export.is_running() {
            self.export.open = false;
        } else if self.export.open {
            self.export.open = false;
            self.export.status = ExportStatus::Idle;
        } else {
            self.export.open = true;
            if self.export.mode == ExportMode::YouTube {
                self.export.youtube = YoutubeSummary::read(youtube::now_unix());
            }
        }
        cx.notify();
    }

    pub fn close_export_dialog(&mut self, cx: &mut Context<Self>) {
        self.export.open = false;
        if !self.export.is_running() {
            self.export.status = ExportStatus::Idle;
        }
        cx.notify();
    }

    pub fn cancel_export(&mut self, cx: &mut Context<Self>) {
        self.export.cancel.store(true, Ordering::Relaxed);
        cx.notify();
    }

    pub fn start_export(&mut self, cx: &mut Context<Self>) {
        if self.export.is_running() {
            return;
        }
        let Some(project) = self.project.clone() else {
            return;
        };
        if self.export.mode == ExportMode::Project {
            self.start_project_export(project, cx);
            return;
        }

        if self.export.mode == ExportMode::YouTube {
            if let Some(source) = self.export.youtube_source.clone() {
                self.publish_to_youtube(source, cx);
                return;
            }
        }
        let Some(backend) = default_backend() else {
            self.export.status = ExportStatus::Failed(t("export.error.noBackend"));
            cx.notify();
            return;
        };

        let base = self.export.resolved_destination(&project.metadata.name);
        if let Some(directory) = base.parent() {
            if let Err(error) = std::fs::create_dir_all(directory) {
                self.export.status = ExportStatus::Failed(error.to_string());
                cx.notify();
                return;
            }
        }
        let presets = self.export.batch.clone();
        let destinations = batch_destinations(&base, &presets);
        let canvas = self
            .project
            .as_ref()
            .map(|project| {
                (
                    project.settings.canvas_size.width,
                    project.settings.canvas_size.height,
                )
            })
            .unwrap_or((1920, 1080));
        let sizes: Vec<(u32, u32)> = presets.iter().map(|id| preset_size(canvas, *id)).collect();

        let media = MediaStore::for_project(&self.store, &project.metadata.id);
        let matte_root = Some(self.store.project_directory(&project.metadata.id));
        let scene_id = Some(project.current_scene_id.clone());
        let frame_rate = project.settings.fps;
        let quality = self.export.quality;
        let include_audio = self.export.include_audio;

        let cancel = Arc::new(AtomicBool::new(false));
        let shared = Arc::new(Mutex::new(SharedState::default()));
        self.export.cancel = Arc::clone(&cancel);
        self.export.shared = Arc::clone(&shared);
        self.export.started_at = Some(Instant::now());
        self.export.status = ExportStatus::Running {
            fraction: 0.0,
            frame: 0,
            total: 0,
            frames_per_second: 0.0,
            stage: Stage::Rendering,
        };
        cx.notify();

        let worker_shared = Arc::clone(&shared);
        let document = Arc::new(project);

        let spawned = std::thread::Builder::new()
            .name("cutix-export".into())
            .spawn(move || {
                let resolver = StoreResolver::new(media);
                let count = destinations.len().max(1) as f32;
                let mut last: Option<Result<RunOutcome, String>> = None;
                for (index, ((destination, (width, height)), _)) in destinations
                    .into_iter()
                    .zip(sizes)
                    .zip(presets.iter())
                    .enumerate()
                {
                    let request = ExportRequest {
                        project: Arc::clone(&document),
                        scene_id: scene_id.clone(),
                        destination,
                        width,
                        height,
                        frame_rate,
                        quality,
                        include_audio,
                        matte_root: matte_root.clone(),
                        backend,
                    };
                    let done = index as f32;
                    let outcome =
                        cutix_export::run(&request, &resolver, &cancel, &mut |mut progress| {
                            progress.frame += (done * progress.total_frames as f32) as u64;
                            progress.total_frames = (progress.total_frames as f32 * count) as u64;
                            if let Ok(mut guard) = worker_shared.lock() {
                                guard.progress = Some(progress);
                            }
                        })
                        .map_err(|error| error.to_string());
                    let failed = outcome.is_err();
                    last = Some(outcome.map(RunOutcome::Video));
                    if failed || cancel.load(Ordering::Relaxed) {
                        break;
                    }
                }
                if let Ok(mut guard) = worker_shared.lock() {
                    guard.outcome = last;
                    guard.outputs = count as usize;
                }
            });

        if spawned.is_err() {
            self.export.status = ExportStatus::Failed(t("export.error.unknown"));
            cx.notify();
            return;
        }

        self.watch_export(cx);
    }

    fn start_project_export(&mut self, project: cutix_project::Project, cx: &mut Context<Self>) {
        let destination = self.export.resolved_destination(&project.metadata.name);
        if let Some(directory) = destination.parent() {
            if let Err(error) = std::fs::create_dir_all(directory) {
                self.export.status = ExportStatus::Failed(error.to_string());
                cx.notify();
                return;
            }
        }

        let store = self.store.clone();
        let shared = Arc::new(Mutex::new(SharedState::default()));
        self.export.cancel = Arc::new(AtomicBool::new(false));
        self.export.shared = Arc::clone(&shared);
        self.export.started_at = Some(Instant::now());
        self.export.status = ExportStatus::Running {
            fraction: 0.0,
            frame: 0,
            total: 0,
            frames_per_second: 0.0,
            stage: Stage::Packaging,
        };
        cx.notify();

        let spawned = std::thread::Builder::new()
            .name("cutix-package".into())
            .spawn(move || {
                let started = Instant::now();
                let outcome =
                    cutix_export::export_package(&store, &project, &destination, &mut |progress| {
                        if let Ok(mut guard) = shared.lock() {
                            guard.progress = Some(cutix_export::Progress {
                                stage: Stage::Packaging,
                                frame: progress.entry as u64,
                                total_frames: progress.total_entries as u64,
                                elapsed: started.elapsed(),
                            });
                        }
                    })
                    .map_err(|error| error.to_string());
                if let Ok(mut guard) = shared.lock() {
                    guard.outcome = Some(outcome.map(RunOutcome::Package));
                    guard.outputs = 1;
                }
            });

        if spawned.is_err() {
            self.export.status = ExportStatus::Failed(t("export.error.unknown"));
            cx.notify();
            return;
        }
        self.watch_export(cx);
    }

    fn watch_export(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(POLL_INTERVAL).await;
            let finished = this
                .update(cx, |this, cx| {
                    let finished = this.drain_export_progress(cx);
                    if !finished && this.export.open {
                        cx.notify();
                    }
                    finished
                })
                .unwrap_or(true);
            if finished {
                return;
            }
        })
        .detach();
    }

    fn publish_to_youtube(&mut self, source: PathBuf, cx: &mut Context<Self>) {
        self.youtube_request = Some(source);
        self.export.status = ExportStatus::Idle;
        self.export.open = false;
        cx.notify();
    }

    fn drain_export_progress(&mut self, cx: &mut Context<Self>) -> bool {
        let (progress, outcome) = {
            let Ok(mut guard) = self.export.shared.lock() else {
                return true;
            };
            (guard.progress.take(), guard.outcome.take())
        };
        let outputs = self
            .export
            .shared
            .lock()
            .map(|guard| guard.outputs.max(1))
            .unwrap_or(1);

        if let Some(outcome) = outcome {
            self.export.started_at = None;
            self.export.status = match outcome {
                Ok(RunOutcome::Video(outcome)) => ExportStatus::Done {
                    mode: self.export.mode,
                    video: outcome
                        .artifacts
                        .video_path
                        .clone()
                        .unwrap_or_else(|| PathBuf::from(".")),
                    audio: outcome.artifacts.audio_path.clone(),
                    has_audio: outcome.artifacts.audio_samples > 0,
                    entries: 0,
                    bytes: outcome.artifacts.bytes,
                    frames_per_second: outcome.frames_per_second(),
                    outputs,
                },
                Ok(RunOutcome::Package(outcome)) => ExportStatus::Done {
                    mode: ExportMode::Project,
                    video: outcome.path.clone(),
                    audio: None,
                    has_audio: false,
                    entries: outcome.entries,
                    bytes: outcome.bytes,
                    frames_per_second: 0.0,
                    outputs: 1,
                },
                Err(error) => ExportStatus::Failed(error),
            };
            if let ExportStatus::Done {
                mode: ExportMode::YouTube,
                video,
                ..
            } = &self.export.status
            {
                let rendered = video.clone();
                self.publish_to_youtube(rendered, cx);
            }
            cx.notify();
            return true;
        }

        if let Some(progress) = progress {
            self.export.status = ExportStatus::Running {
                fraction: progress.fraction(),
                frame: progress.frame,
                total: progress.total_frames,
                frames_per_second: progress.frames_per_second(),
                stage: progress.stage,
            };
            cx.notify();
        }
        false
    }
}

pub fn reveal(path: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn();
    }
    #[cfg(not(windows))]
    {
        if let Some(parent) = path.parent() {
            let _ = std::process::Command::new("xdg-open").arg(parent).spawn();
        }
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn section_label(colors: Palette, label: impl Into<SharedString>) -> Div {
    div()
        .text_size(rem(TEXT_XS))
        .font_weight(FontWeight::MEDIUM)
        .text_color(colors.muted_foreground)
        .child(label.into())
}

fn choice_row(
    id: impl Into<SharedString>,
    colors: Palette,
    label: impl Into<SharedString>,
    selected: bool,
    disabled: bool,
) -> gpui::Stateful<Div> {
    div()
        .id(id.into())
        .flex()
        .items_center()
        .gap(px(8.0))
        .h(px(28.0))
        .px(px(8.0))
        .rounded(rem(RADIUS_SM))
        .when(!disabled, |this| this.cursor_pointer())
        .when(selected, |this| this.bg(opacity(colors.accent, 0.6)))
        .when(disabled, |this| this.opacity(0.5))
        .text_size(rem(TEXT_SM))
        .text_color(colors.foreground)
        .child(
            div()
                .size(px(12.0))
                .flex_shrink_0()
                .rounded(px(6.0))
                .border_1()
                .border_color(if selected {
                    colors.primary
                } else {
                    colors.border
                })
                .when(selected, |this| {
                    this.child(
                        div()
                            .absolute()
                            .m(px(2.0))
                            .size(px(6.0))
                            .rounded(px(3.0))
                            .bg(colors.primary),
                    )
                }),
        )
        .child(label.into())
}

const LANE_WIDTH_PX: f32 = DIALOG_WIDTH_PX - 2.0 * DIALOG_PADDING_PX;
const LANE_HEIGHT_PX: f32 = 58.0;
const TILE_WIDTH_PX: f32 = 15.0;
const TILE_HEIGHT_PX: f32 = 19.0;

const TILE_COUNT: usize = 6;

const TILE_TRAVEL_SECONDS: f32 = 1.5;
const TILE_START_X: f32 = 30.0;
const TILE_END_X: f32 = LANE_WIDTH_PX - 74.0;
const STACK_X: f32 = LANE_WIDTH_PX - 56.0;
const STACK_WIDTH_PX: f32 = 40.0;
const STACK_HEIGHT_PX: f32 = 40.0;

const TILE_FADE_SECONDS: f32 = 0.150;

fn ease_out_expo(t: f32) -> f32 {
    if t >= 1.0 {
        1.0
    } else {
        1.0 - (-10.0 * t).exp2()
    }
}

fn file_tile(colors: Palette, x: f32, y: f32, alpha: f32) -> Div {
    div()
        .absolute()
        .left(px(x))
        .top(px(y))
        .w(px(TILE_WIDTH_PX))
        .h(px(TILE_HEIGHT_PX))
        .rounded(px(3.0))
        .bg(opacity(colors.primary, alpha))
        .child(
            div()
                .absolute()
                .right(px(2.0))
                .top(px(2.0))
                .w(px(5.0))
                .h(px(5.0))
                .rounded(px(1.0))
                .bg(opacity(colors.popover, alpha * 0.85)),
        )
        .child(
            div()
                .absolute()
                .left(px(3.0))
                .bottom(px(4.0))
                .w(px(9.0))
                .h(px(2.0))
                .rounded(px(1.0))
                .bg(opacity(colors.popover, alpha * 0.6)),
        )
}

fn assembly_lane(colors: Palette, fraction: f32, phase: f32) -> Div {
    let middle = (LANE_HEIGHT_PX - TILE_HEIGHT_PX) / 2.0;
    let mut lane = div()
        .relative()
        .w_full()
        .h(px(LANE_HEIGHT_PX))
        .overflow_hidden()
        .rounded(rem(RADIUS_MD))
        .border_1()
        .border_color(colors.border)
        .bg(opacity(colors.muted, 0.5));

    lane = lane.child(
        div()
            .absolute()
            .left(px(12.0))
            .top(px(middle - 3.0))
            .w(px(12.0))
            .h(px(TILE_HEIGHT_PX + 6.0))
            .rounded(px(3.0))
            .bg(opacity(colors.foreground, 0.14)),
    );

    for index in 0..TILE_COUNT {
        let offset = index as f32 / TILE_COUNT as f32;
        let travel = ((phase / TILE_TRAVEL_SECONDS) + offset).rem_euclid(1.0);
        let x = TILE_START_X + (TILE_END_X - TILE_START_X) * ease_out_expo(travel);

        let bob = ((travel + offset) * std::f32::consts::TAU).sin() * 5.0;
        let entering = (travel * TILE_TRAVEL_SECONDS / TILE_FADE_SECONDS).clamp(0.0, 1.0);
        let leaving = ((1.0 - travel) / 0.22).clamp(0.0, 1.0);
        let alpha = 0.9 * entering * leaving;
        if alpha <= 0.01 {
            continue;
        }
        lane = lane.child(file_tile(colors, x, middle + bob, alpha));
    }

    let filled = (STACK_HEIGHT_PX * fraction.clamp(0.0, 1.0)).round();
    lane = lane.child(
        div()
            .absolute()
            .left(px(STACK_X))
            .top(px((LANE_HEIGHT_PX - STACK_HEIGHT_PX) / 2.0))
            .w(px(STACK_WIDTH_PX))
            .h(px(STACK_HEIGHT_PX))
            .rounded(px(5.0))
            .border_1()
            .border_color(opacity(colors.primary, 0.55))
            .bg(opacity(colors.primary, 0.10))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .bottom_0()
                    .w(px(STACK_WIDTH_PX - 2.0))
                    .h(px(filled))
                    .rounded(px(4.0))
                    .bg(opacity(colors.primary, 0.75)),
            ),
    );
    lane
}

fn progress_bar(colors: Palette, fraction: f32) -> Div {
    div()
        .w_full()
        .h(px(6.0))
        .rounded(px(3.0))
        .bg(colors.muted)
        .child(
            div()
                .h(px(6.0))
                .w(gpui::relative(fraction.clamp(0.0, 1.0)))
                .rounded(px(3.0))
                .bg(colors.primary),
        )
}

#[derive(Clone, Debug)]
pub struct ExportView {
    colors: Palette,
    mode: ExportMode,
    youtube: YoutubeSummary,
    youtube_source: Option<PathBuf>,
    batch: Vec<ExportPresetId>,
    destination: String,
    quality: ExportQuality,
    include_audio: bool,
    status: ExportStatus,
    collapsed: Vec<ExportSection>,
    width: u32,
    height: u32,

    phase: f32,
}

pub fn snapshot(model: &AppModel) -> Option<ExportView> {
    if !model.export.open {
        return None;
    }
    let (width, height) = output_size(model);
    Some(ExportView {
        colors: model.theme.root,
        mode: model.export.mode,
        youtube: model.export.youtube.clone(),
        youtube_source: model.export.youtube_source.clone(),
        batch: model.export.batch.clone(),
        destination: model
            .export
            .resolved_destination(&model.project_name())
            .display()
            .to_string(),
        quality: model.export.quality,
        include_audio: model.export.include_audio,
        status: model.export.status.clone(),
        collapsed: model.export.collapsed.clone(),
        width,
        height,
        phase: model
            .export
            .started_at
            .map(|started| started.elapsed().as_secs_f32())
            .unwrap_or(0.0),
    })
}

impl ExportView {
    fn is_selected(&self, id: ExportPresetId) -> bool {
        self.batch.contains(&id)
    }

    fn is_collapsed(&self, section: ExportSection) -> bool {
        self.collapsed.contains(&section)
    }
}

const DIALOG_MAX_HEIGHT: f32 = 0.86;

fn section(
    colors: Palette,
    which: ExportSection,
    summary: Option<SharedString>,
    collapsed: bool,
    body: Div,
    cx: &mut Context<crate::shell::Shell>,
) -> Div {
    let header = div()
        .id(SharedString::from(format!(
            "export-section-{}",
            which.key()
        )))
        .flex()
        .items_center()
        .gap(px(6.0))
        .cursor_pointer()
        .child(
            div()
                .text_size(rem(TEXT_XS))
                .text_color(colors.muted_foreground)
                .child(if collapsed { "\u{203a}" } else { "\u{2304}" }),
        )
        .child(section_label(colors, t(which.title_key())))
        .when_some(summary, |this, summary| {
            this.child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(summary),
            )
        })
        .on_click(
            cx.listener(move |this: &mut crate::shell::Shell, _, _, cx| {
                this.app.update(cx, |model, cx| {
                    model.export.toggle_section(which);
                    cx.notify();
                });
            }),
        );

    div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .child(header)
        .when(!collapsed, |this| this.child(body))
}

pub fn export_dialog(view: ExportView, cx: &mut Context<crate::shell::Shell>) -> Div {
    let colors = view.colors;
    let (width, height) = (view.width, view.height);
    let running = matches!(view.status, ExportStatus::Running { .. });

    let body = match &view.status {
        ExportStatus::Done { .. } => done_body(&view.status, colors, cx),
        ExportStatus::Failed(error) => failed_body(colors, error.clone(), cx),
        ExportStatus::Running {
            fraction,
            frame,
            total,
            frames_per_second,
            stage,
        } => running_body(
            colors,
            *fraction,
            *frame,
            *total,
            *frames_per_second,
            *stage,
            view.phase,
            cx,
        ),
        ExportStatus::Idle => idle_body(&view, colors, width, height, cx),
    };

    let footer = matches!(&view.status, ExportStatus::Idle).then(|| footer_row(&view, colors, cx));

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
        .child(
            div()
                .w(px(DIALOG_WIDTH_PX))
                .max_h(gpui::relative(DIALOG_MAX_HEIGHT))
                .flex()
                .flex_col()
                .gap(px(14.0))
                .rounded(rem(RADIUS_LG))
                .border_1()
                .border_color(colors.border)
                .p(px(DIALOG_PADDING_PX))
                .bg(colors.popover)
                .text_color(colors.popover_foreground)
                .shadow_lg()
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(rem(TEXT_LG))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(if running {
                            t("export.title.exporting")
                        } else {
                            t("export.title")
                        }),
                )
                .child(
                    div()
                        .id("export-body")
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(body),
                )
                .children(footer),
        )
}

fn footer_row(view: &ExportView, colors: Palette, cx: &mut Context<crate::shell::Shell>) -> Div {
    let youtube = view.mode == ExportMode::YouTube;
    let blocked = youtube && !view.youtube.is_ready();
    div()
        .flex()
        .flex_shrink_0()
        .justify_end()
        .gap(px(8.0))
        .pt(px(4.0))
        .child(
            Button::new("export-cancel", colors)
                .variant(ButtonVariant::Outline)
                .label(t("common.cancel"))
                .build()
                .on_click(cx.listener(|this: &mut crate::shell::Shell, _, _, cx| {
                    this.app
                        .update(cx, |model, cx| model.close_export_dialog(cx));
                })),
        )
        .child(
            Button::new("export-start", colors)
                .label(if youtube {
                    t("youtube.publish.submit")
                } else {
                    t("common.export")
                })
                .build()
                .when(blocked, |this| this.opacity(0.5))
                .on_click(
                    cx.listener(move |this: &mut crate::shell::Shell, _, _, cx| {
                        if blocked {
                            return;
                        }
                        this.app.update(cx, |model, cx| model.start_export(cx));
                    }),
                ),
        )
}

fn idle_body(
    view: &ExportView,
    colors: Palette,
    width: u32,
    height: u32,
    cx: &mut Context<crate::shell::Shell>,
) -> Div {
    let support = audio_support();
    let audio_note = match support {
        AudioSupport::Muxed => Some(t("export.audio.muxed")),
        AudioSupport::Sidecar { extension } => Some(t_args(
            "export.audio.sidecar",
            &[("format", &extension.to_uppercase())],
        )),
        AudioSupport::None => Some(t("export.audio.unavailable")),
    };

    let renders = view.mode == ExportMode::Video
        || (view.mode == ExportMode::YouTube && view.youtube_source.is_none());
    let video = renders;

    let mut modes = div().flex().flex_col().gap(px(2.0));
    for mode in EXPORT_MODES {
        modes = modes.child(
            choice_row(
                SharedString::from(format!("export-mode-{}", mode.key())),
                colors,
                t(mode.label_key()),
                view.mode == mode,
                false,
            )
            .on_click(
                cx.listener(move |this: &mut crate::shell::Shell, _, _, cx| {
                    this.app.update(cx, |model, cx| {
                        model.export.set_mode(mode);
                        cx.notify();
                    });
                }),
            ),
        );
    }
    let modes = section(
        colors,
        ExportSection::Mode,
        Some(SharedString::from(t(view.mode.label_key()))),
        view.is_collapsed(ExportSection::Mode),
        div().flex().flex_col().gap(px(4.0)).child(modes).child(
            div()
                .text_size(rem(TEXT_XS))
                .text_color(colors.muted_foreground)
                .child(t(view.mode.hint_key())),
        ),
        cx,
    );

    let mut presets = div().flex().flex_col().gap(px(2.0));
    for preset in EXPORT_PRESETS {
        let id = preset.id;
        presets = presets.child(
            choice_row(
                SharedString::from(format!("export-preset-{}", preset.label_key)),
                colors,
                t(preset.label_key),
                view.is_selected(id),
                false,
            )
            .on_click(
                cx.listener(move |this: &mut crate::shell::Shell, _, _, cx| {
                    this.app.update(cx, |model, cx| {
                        model.export.toggle_preset(id);
                        cx.notify();
                    });
                }),
            ),
        );
    }

    let mut qualities = div().flex().flex_col().gap(px(2.0));
    for quality in EXPORT_QUALITIES {
        qualities = qualities.child(
            choice_row(
                SharedString::from(format!("export-quality-{}", quality.label_key())),
                colors,
                t(quality.label_key()),
                view.quality == quality,
                false,
            )
            .on_click(
                cx.listener(move |this: &mut crate::shell::Shell, _, _, cx| {
                    this.app.update(cx, |model, cx| {
                        model.export.quality = quality;
                        cx.notify();
                    });
                }),
            ),
        );
    }

    let resolution = video.then(|| {
        section(
            colors,
            ExportSection::Resolution,
            Some(SharedString::from(format!("{width}×{height}"))),
            view.is_collapsed(ExportSection::Resolution),
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(rem(TEXT_SM))
                        .text_color(colors.muted_foreground)
                        .child(t(cutix_export::ExportFormat::Mp4.label_key())),
                )
                .child(presets)
                .child(
                    div()
                        .text_size(rem(TEXT_XS))
                        .text_color(colors.muted_foreground)
                        .child(if view.batch.len() > 1 {
                            t_args(
                                "export.batch.count",
                                &[("count", &view.batch.len().to_string())],
                            )
                        } else {
                            t_args(
                                "export.resolution.output",
                                &[
                                    ("width", &width.to_string()),
                                    ("height", &height.to_string()),
                                ],
                            )
                        }),
                ),
            cx,
        )
    });

    let destination = section(
        colors,
        ExportSection::Destination,
        None,
        view.is_collapsed(ExportSection::Destination),
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(rem(TEXT_XS))
                    .text_color(colors.muted_foreground)
                    .child(SharedString::from(view.destination.clone())),
            )
            .child(
                Button::new("export-choose", colors)
                    .variant(ButtonVariant::Outline)
                    .label(t("export.destination.choose"))
                    .build()
                    .on_click(cx.listener(|this: &mut crate::shell::Shell, _, _, cx| {
                        this.choose_export_destination(cx);
                    })),
            ),
        cx,
    );

    let quality = video.then(|| {
        section(
            colors,
            ExportSection::Quality,
            Some(SharedString::from(t(view.quality.label_key()))),
            view.is_collapsed(ExportSection::Quality),
            qualities,
            cx,
        )
    });

    let audio = video.then(|| {
        section(
            colors,
            ExportSection::Audio,
            Some(SharedString::from(t(if view.include_audio {
                "export.audio.on"
            } else {
                "export.audio.off"
            }))),
            view.is_collapsed(ExportSection::Audio),
            div()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    choice_row(
                        "export-audio",
                        colors,
                        t("export.includeAudio"),
                        view.include_audio,
                        support == AudioSupport::None,
                    )
                    .on_click(cx.listener(
                        |this: &mut crate::shell::Shell, _, _, cx| {
                            this.app.update(cx, |model, cx| {
                                model.export.include_audio = !model.export.include_audio;
                                cx.notify();
                            });
                        },
                    )),
                )
                .when_some(audio_note, |this, note| {
                    this.child(
                        div()
                            .text_size(rem(TEXT_XS))
                            .p(px(8.0))
                            .rounded(rem(RADIUS_MD))
                            .bg(opacity(colors.muted, 0.7))
                            .text_color(colors.muted_foreground)
                            .child(note),
                    )
                }),
            cx,
        )
    });

    let publish = (view.mode == ExportMode::YouTube).then(|| {
        section(
            colors,
            ExportSection::Publish,
            Some(SharedString::from(match &view.youtube_source {
                Some(path) => path
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.display().to_string()),
                None => t("export.mode.video"),
            })),
            view.is_collapsed(ExportSection::Publish),
            youtube_body(view, colors),
            cx,
        )
    });

    div()
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(modes)
        .children(publish)
        .children(resolution)
        .when(renders, |this| this.child(destination))
        .children(quality)
        .children(audio)
}

fn youtube_body(view: &ExportView, colors: Palette) -> Div {
    let summary = &view.youtube;

    let mut body = div().flex().flex_col().gap(px(8.0));

    if !summary.configured {
        return body.child(notice(
            colors,
            t("youtube.error.noBrowser"),
            t("youtube.signIn.installChrome"),
        ));
    }

    body = body.child(
        div()
            .flex()
            .flex_col()
            .gap(px(2.0))
            .text_size(rem(TEXT_XS))
            .text_color(colors.muted_foreground)
            .child(match summary.channel.clone() {
                Some(channel) => format!("{}: {channel}", t("youtube.publish.account")),
                None => t("youtube.accounts.none"),
            })
            .child(
                div()
                    .text_color(if summary.configured {
                        colors.foreground
                    } else {
                        colors.caution
                    })
                    .child(t(if summary.configured {
                        "youtube.publish.limitHint"
                    } else {
                        "youtube.error.noBrowser"
                    })),
            ),
    );

    body
}

fn notice(colors: Palette, title: String, body: String) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .p(px(8.0))
        .rounded(rem(RADIUS_MD))
        .border_1()
        .border_color(opacity(colors.caution, 0.6))
        .bg(opacity(colors.caution, 0.08))
        .child(
            div()
                .text_size(rem(TEXT_XS))
                .text_color(colors.foreground)
                .child(title),
        )
        .child(
            div()
                .text_size(rem(TEXT_XS))
                .text_color(colors.muted_foreground)
                .child(body),
        )
}

fn running_body(
    colors: Palette,
    fraction: f32,
    frame: u64,
    total: u64,
    frames_per_second: f32,
    stage: Stage,
    phase: f32,
    cx: &mut Context<crate::shell::Shell>,
) -> Div {
    let packaging = stage == Stage::Packaging;
    let stage_label = match stage {
        Stage::Rendering => t("export.stage.rendering"),
        Stage::Audio => t("export.stage.audio"),
        Stage::Packaging => t("export.stage.packaging"),
        Stage::Finishing => t("export.stage.finishing"),
    };
    let counter = if packaging {
        t_args(
            "export.progress.files",
            &[("done", &frame.to_string()), ("total", &total.to_string())],
        )
    } else {
        t_args(
            "export.progress.frames",
            &[("frame", &frame.to_string()), ("total", &total.to_string())],
        )
    };

    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .flex()
                .items_baseline()
                .justify_between()
                .gap(px(8.0))
                .child(div().text_size(rem(TEXT_SM)).child(stage_label))
                .child(
                    div()
                        .text_size(rem(TEXT_LG))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(colors.primary)
                        .child(t_args(
                            "export.progress.percent",
                            &[(
                                "percent",
                                &format!("{:.0}", fraction.clamp(0.0, 1.0) * 100.0),
                            )],
                        )),
                ),
        )
        .child(progress_bar(colors, fraction))
        .child(assembly_lane(colors, fraction, phase))
        .child(
            div()
                .flex()
                .justify_between()
                .text_size(rem(TEXT_XS))
                .text_color(colors.muted_foreground)
                .child(counter)
                .when(!packaging, |this| {
                    this.child(t_args(
                        "export.progress.rate",
                        &[("fps", &format!("{frames_per_second:.1}"))],
                    ))
                }),
        )
        .child(
            div().flex().justify_end().pt(px(4.0)).child(
                Button::new("export-abort", colors)
                    .variant(ButtonVariant::Outline)
                    .label(t("common.cancel"))
                    .build()
                    .on_click(cx.listener(|this: &mut crate::shell::Shell, _, _, cx| {
                        this.app.update(cx, |model, cx| model.cancel_export(cx));
                    })),
            ),
        )
}

fn done_body(status: &ExportStatus, colors: Palette, cx: &mut Context<crate::shell::Shell>) -> Div {
    let ExportStatus::Done {
        mode,
        video,
        audio,
        has_audio,
        entries,
        bytes,
        frames_per_second,
        outputs,
    } = status
    else {
        return div();
    };
    let (mode, entries, outputs) = (*mode, *entries, *outputs);
    let reveal_target = video.clone();

    let mut notes = div()
        .flex()
        .flex_col()
        .gap(px(4.0))
        .text_size(rem(TEXT_XS))
        .text_color(colors.muted_foreground)
        .child(SharedString::from(video.display().to_string()));
    if outputs > 1 {
        notes = notes.child(t_args(
            "export.done.batch",
            &[("count", &outputs.to_string())],
        ));
    }
    if let Some(path) = audio.clone() {
        notes = notes.child(SharedString::from(path.display().to_string()));
    }
    if *has_audio {
        notes = notes.child(t("export.done.audio"));
    }
    notes = match mode {
        ExportMode::Project => notes.child(t_args(
            "export.done.entries",
            &[("count", &entries.to_string())],
        )),
        ExportMode::Video | ExportMode::YouTube => notes.child(t_args(
            "export.done.summary",
            &[
                ("size", &format_bytes(*bytes)),
                ("fps", &format!("{frames_per_second:.1}")),
            ],
        )),
    };

    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .text_size(rem(TEXT_SM))
                .font_weight(FontWeight::MEDIUM)
                .child(match mode {
                    ExportMode::Project => t("export.done.project"),
                    ExportMode::Video | ExportMode::YouTube => t("export.done.title"),
                }),
        )
        .child(notes)
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .pt(px(4.0))
                .child(
                    Button::new("export-reveal", colors)
                        .variant(ButtonVariant::Outline)
                        .label(t("export.done.reveal"))
                        .build()
                        .on_click(cx.listener(move |_: &mut crate::shell::Shell, _, _, _| {
                            reveal(&reveal_target);
                        })),
                )
                .child(
                    Button::new("export-close", colors)
                        .label(t("common.close"))
                        .build()
                        .on_click(cx.listener(|this: &mut crate::shell::Shell, _, _, cx| {
                            this.app
                                .update(cx, |model, cx| model.close_export_dialog(cx));
                        })),
                ),
        )
}

fn failed_body(colors: Palette, error: String, cx: &mut Context<crate::shell::Shell>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(10.0))
        .child(
            div()
                .text_size(rem(TEXT_SM))
                .font_weight(FontWeight::MEDIUM)
                .text_color(colors.destructive)
                .child(t("export.failed")),
        )
        .child(
            div()
                .text_size(rem(TEXT_XS))
                .text_color(colors.muted_foreground)
                .child(SharedString::from(error)),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .pt(px(4.0))
                .child(
                    Button::new("export-retry", colors)
                        .variant(ButtonVariant::Outline)
                        .label(t("export.retry"))
                        .build()
                        .on_click(cx.listener(|this: &mut crate::shell::Shell, _, _, cx| {
                            this.app.update(cx, |model, cx| {
                                model.export.status = ExportStatus::Idle;
                                model.start_export(cx);
                            });
                        })),
                )
                .child(
                    Button::new("export-failed-close", colors)
                        .label(t("common.close"))
                        .build()
                        .on_click(cx.listener(|this: &mut crate::shell::Shell, _, _, cx| {
                            this.app
                                .update(cx, |model, cx| model.close_export_dialog(cx));
                        })),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_name_becomes_a_safe_file_name() {
        assert_eq!(safe_file_name("My Clip"), "My Clip");
        assert_eq!(safe_file_name("a/b:c*d"), "a-b-c-d");
        assert_eq!(safe_file_name("   "), "export");
        assert_eq!(safe_file_name("..."), "export");
    }

    #[test]
    fn byte_sizes_read_in_the_largest_sensible_unit() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn the_shipped_backend_carries_audio_inside_the_video_file() {
        if !cutix_export::aac::is_available() {
            eprintln!(
                "SKIPPED: no aac encoder ({}); audio falls back to a sidecar",
                cutix_export::aac::unavailable_reason()
            );
            assert_eq!(audio_support(), AudioSupport::Sidecar { extension: "wav" });
            return;
        }
        assert_eq!(audio_support(), AudioSupport::Muxed);
    }

    #[test]
    fn switching_mode_re_points_the_destination_at_the_other_kind_of_file() {
        let mut session = ExportSession::default();
        assert_eq!(session.mode, ExportMode::Video);
        session.destination = Some(PathBuf::from("C:/out/My Clip.mp4"));

        session.set_mode(ExportMode::Project);
        assert_eq!(
            session.resolved_destination("ignored"),
            PathBuf::from("C:/out/My Clip.ocut")
        );

        session.set_mode(ExportMode::Video);
        assert_eq!(
            session.resolved_destination("ignored"),
            PathBuf::from("C:/out/My Clip.mp4")
        );
    }

    #[test]
    fn a_destination_the_picker_returned_still_gets_the_mode_extension() {
        let mut session = ExportSession::default();
        session.mode = ExportMode::Project;

        session.destination = Some(PathBuf::from("C:/out/Scene.mp4"));
        assert_eq!(
            session.resolved_destination("ignored"),
            PathBuf::from("C:/out/Scene.ocut")
        );
    }

    #[test]
    fn the_three_modes_write_the_file_type_they_promise() {
        assert_eq!(ExportMode::Video.extension(), "mp4");
        assert_eq!(ExportMode::Project.extension(), "ocut");

        assert_eq!(ExportMode::YouTube.extension(), "mp4");
        assert_eq!(EXPORT_MODES.len(), 3);

        let mut keys: Vec<&str> = EXPORT_MODES.iter().map(|mode| mode.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), EXPORT_MODES.len());
        for mode in EXPORT_MODES {
            assert_ne!(t(mode.label_key()), mode.label_key());
            assert_ne!(t(mode.hint_key()), mode.hint_key());
        }
    }

    #[test]
    fn youtube_mode_only_offers_to_publish_once_the_setup_and_a_channel_are_there() {
        let mut summary = YoutubeSummary::default();
        assert!(!summary.is_ready());
        summary.configured = true;
        assert!(
            !summary.is_ready(),
            "a client id without a channel is not enough"
        );
        summary.channel = Some("Some Channel".to_string());
        assert!(summary.is_ready());
    }

    #[test]
    fn switching_into_youtube_mode_keeps_the_destination_a_video() {
        let mut session = ExportSession::default();
        session.destination = Some(PathBuf::from("C:/out/My Clip.mp4"));
        session.set_mode(ExportMode::YouTube);
        assert_eq!(
            session.resolved_destination("ignored"),
            PathBuf::from("C:/out/My Clip.mp4")
        );
        assert!(session.youtube_source.is_none());
    }

    #[test]
    fn the_travel_curve_starts_and_ends_where_the_lane_does() {
        assert!(ease_out_expo(0.0).abs() < 0.001);
        assert_eq!(ease_out_expo(1.0), 1.0);

        assert!(ease_out_expo(0.5) > 0.9);
        for step in 0..20 {
            let t = step as f32 / 20.0;
            assert!(ease_out_expo(t) <= ease_out_expo(t + 0.05));
        }
    }

    #[test]
    fn a_single_preset_keeps_the_chosen_file_name() {
        let base = PathBuf::from("C:/out/My Clip.mp4");
        let paths = batch_destinations(&base, &[ExportPresetId::Youtube1080p]);
        assert_eq!(paths, vec![base]);
    }

    #[test]
    fn a_batch_tags_each_file_with_its_preset() {
        let base = PathBuf::from("C:/out/My Clip.mp4");
        let paths = batch_destinations(
            &base,
            &[
                ExportPresetId::Youtube1080p,
                ExportPresetId::Vertical1080p,
                ExportPresetId::Square1080,
            ],
        );
        assert_eq!(
            paths,
            vec![
                PathBuf::from("C:/out/My Clip-1920x1080.mp4"),
                PathBuf::from("C:/out/My Clip-1080x1920.mp4"),
                PathBuf::from("C:/out/My Clip-1080x1080.mp4"),
            ]
        );
    }

    #[test]
    fn toggling_never_empties_the_batch() {
        let mut session = ExportSession::default();
        assert_eq!(session.batch, vec![ExportPresetId::Project]);
        session.toggle_preset(ExportPresetId::Project);
        assert_eq!(session.batch, vec![ExportPresetId::Project]);

        session.toggle_preset(ExportPresetId::Square1080);
        assert!(session.batch.contains(&ExportPresetId::Square1080));
        assert_eq!(session.batch.len(), 2);

        session.toggle_preset(ExportPresetId::Project);
        assert_eq!(session.batch, vec![ExportPresetId::Square1080]);
        assert_eq!(session.preset, ExportPresetId::Square1080);
    }

    #[test]
    fn preset_sizes_are_even_and_fall_back_to_the_canvas() {
        assert_eq!(
            preset_size((1921, 1081), ExportPresetId::Project),
            (1920, 1080)
        );
        assert_eq!(
            preset_size((640, 480), ExportPresetId::Portrait4x5),
            (1080, 1350)
        );
    }
}
