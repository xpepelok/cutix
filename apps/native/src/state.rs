use std::path::{Path, PathBuf};

use cutix_project::{
    MediaAssetData, MediaStore, MediaType, Project, ProjectStore, ProjectSummary, Scene,
    TimelineElement, Track,
};
use gpui::{App, AppContext, Context, Entity};
use serde::{Deserialize, Serialize};

use crate::edit::{Editor, History};
use crate::export::ExportSession;
use crate::playback::PreviewEngine;
use crate::theme::Theme;
use cutix_playback::WaveformPeaks;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    Home,
    Projects,

    Library,
    Editor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Grid,
    List,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortKey {
    CreatedAt,
    UpdatedAt,
    Name,
    Duration,
}

impl SortKey {
    pub fn label_key(self) -> &'static str {
        match self {
            SortKey::CreatedAt => "projects.sort.created",
            SortKey::UpdatedAt => "projects.sort.modified",
            SortKey::Name => "projects.sort.name",
            SortKey::Duration => "projects.sort.duration",
        }
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub locale: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub dark: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub window: Option<WindowGeometry>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub video_directory: Option<std::path::PathBuf>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub preview_volume: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub preview_muted: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub publish_privacy: Option<String>,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    #[serde(default)]
    pub maximized: bool,
}

impl WindowGeometry {
    pub const MIN_WIDTH: f32 = 960.0;
    pub const MIN_HEIGHT: f32 = 600.0;

    pub fn clamped(self, display: (f32, f32, f32, f32)) -> Self {
        let (left, top, right, bottom) = display;
        let width = self
            .width
            .clamp(Self::MIN_WIDTH, (right - left).max(Self::MIN_WIDTH));
        let height = self
            .height
            .clamp(Self::MIN_HEIGHT, (bottom - top).max(Self::MIN_HEIGHT));
        let x = self.x.clamp(left, (right - width).max(left));
        let y = self.y.clamp(top, (bottom - height).max(top));
        Self {
            x,
            y,
            width,
            height,
            maximized: self.maximized,
        }
    }

    pub fn is_sane(self) -> bool {
        self.width.is_finite()
            && self.height.is_finite()
            && self.x.is_finite()
            && self.y.is_finite()
            && self.width >= 1.0
            && self.height >= 1.0
    }
}

pub fn save_preview_audio(volume: f32, muted: bool) {
    let mut settings = load_settings();
    if settings.preview_volume == Some(volume) && settings.preview_muted == Some(muted) {
        return;
    }
    settings.preview_volume = Some(volume);
    settings.preview_muted = Some(muted);
    save_settings(&settings);
}

pub fn save_window_geometry(geometry: WindowGeometry) {
    let mut settings = load_settings();
    if settings.window == Some(geometry) {
        return;
    }
    settings.window = Some(geometry);
    save_settings(&settings);
}

pub fn settings_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base| base.join("cutix").join("native-settings.json"))
}

pub fn load_settings() -> Settings {
    settings_path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &Settings) {
    let Some(path) = settings_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(settings) {
        let _ = std::fs::write(path, bytes);
    }
}

pub fn format_date(iso: &str) -> String {
    let date = iso.split('T').next().unwrap_or(iso);
    let mut parts = date.split('-');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(year), Some(month), Some(day)) if year.len() == 4 => {
            format!("{day}.{month}.{year}")
        }
        _ => iso.to_string(),
    }
}

pub fn format_duration(time: time::MediaTime) -> Option<String> {
    let seconds = time.to_seconds_f64();
    if seconds <= 0.0 {
        return None;
    }
    let format = if seconds >= 3600.0 {
        time::TimeCodeFormat::HhMmSs
    } else {
        time::TimeCodeFormat::MmSs
    };
    time::format_timecode(time::FormatTimecodeOptions {
        time,
        format: Some(format),
        rate: None,
    })
}

pub fn format_frames(time: time::MediaTime, rate: time::FrameRate) -> String {
    time::format_timecode(time::FormatTimecodeOptions {
        time: time.max(time::MediaTime::ZERO),
        format: Some(time::TimeCodeFormat::HhMmSsFf),
        rate: Some(rate),
    })
    .unwrap_or_else(|| String::from("00:00:00:00"))
}

pub fn format_seconds(seconds: f64) -> Option<String> {
    time::MediaTime::from_seconds_f64(seconds).and_then(format_duration)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UploadSummary {
    pub active: usize,

    pub percent: u32,

    pub title: Option<String>,

    pub finished: usize,
}

impl UploadSummary {
    pub fn is_busy(&self) -> bool {
        self.active > 0
    }

    pub fn fraction(&self) -> f32 {
        let total = self.active + self.finished;
        if total == 0 {
            return 0.0;
        }
        let done = self.finished as f32 + (self.percent.min(100) as f32 / 100.0);
        (done / total as f32).clamp(0.0, 1.0)
    }
}

pub struct AppModel {
    pub theme: Theme,
    pub dark: bool,
    pub store: ProjectStore,
    pub projects: Vec<ProjectSummary>,
    pub projects_loaded: bool,
    pub route: Route,
    pub project: Option<Project>,
    pub media: Vec<MediaAssetData>,
    pub media_root: Option<PathBuf>,
    pub importing: usize,
    pub pending_import: Vec<PathBuf>,
    pub pending_on_timeline: bool,
    pub editor_origin: Route,
    pub notice: Option<String>,
    pub selection: Vec<String>,

    pub tracking_region: crate::tracking::TrackingRegion,

    pub properties_tabs: std::collections::HashMap<String, String>,
    pub history: History,
    pub preview: PreviewEngine,
    pub export: ExportSession,

    pub youtube_request: Option<PathBuf>,

    pub uploads: UploadSummary,

    pub preview_video: Option<PathBuf>,

    pub settings_request: Option<String>,
    pub playhead: time::MediaTime,
    pub snapping: bool,
    pub ripple: bool,
    pub locale: String,

    pub video_directory: Option<std::path::PathBuf>,
    pub search: String,
    pub view_mode: ViewMode,
    pub sort_key: SortKey,
    pub sort_ascending: bool,
    pub waveforms: std::collections::HashMap<String, WaveformState>,
    pub missing_media: std::collections::HashSet<String>,
    pub clipboard: Vec<TimelineElement>,
    pub keybindings: crate::keybindings::Keybindings,

    dirty: bool,

    pending_epoch: u64,

    debounce_token: u64,
}

pub const AUTOSAVE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(400);

pub const AUTOSAVE_MAX_DEFER: std::time::Duration = std::time::Duration::from_millis(2_000);

#[derive(Clone, Debug, PartialEq)]
pub enum WaveformState {
    Pending,
    Ready(std::sync::Arc<WaveformPeaks>),
    Unavailable,
}

impl AppModel {
    /// Only the tests in this file ask this; compiled for them alone so the shipping
    /// binary does not carry a method nothing calls.
    #[cfg(test)]
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn new(store: ProjectStore, dark: bool) -> Self {
        Self {
            theme: if dark { Theme::dark() } else { Theme::light() },
            dark,
            store,
            projects: Vec::new(),
            projects_loaded: false,
            route: Route::Home,
            project: None,
            media: Vec::new(),
            media_root: None,
            importing: 0,
            pending_import: Vec::new(),
            pending_on_timeline: false,
            editor_origin: Route::Projects,
            notice: None,
            selection: Vec::new(),
            tracking_region: crate::tracking::TrackingRegion::default(),
            properties_tabs: std::collections::HashMap::new(),
            history: History::default(),
            preview: {
                let settings = load_settings();
                let mut preview = PreviewEngine::default();
                preview.restore_audio(
                    settings.preview_volume.unwrap_or(1.0),
                    settings.preview_muted.unwrap_or(false),
                );
                preview
            },
            export: ExportSession::default(),
            youtube_request: None,
            uploads: UploadSummary::default(),
            preview_video: None,
            settings_request: None,
            playhead: time::MediaTime::ZERO,
            snapping: true,
            ripple: false,
            locale: cutix_i18n::locale(),
            video_directory: load_settings().video_directory,
            search: String::new(),
            view_mode: ViewMode::Grid,
            sort_key: SortKey::UpdatedAt,
            sort_ascending: false,
            waveforms: std::collections::HashMap::new(),
            missing_media: std::collections::HashSet::new(),
            clipboard: Vec::new(),
            keybindings: crate::keybindings::load(),
            dirty: false,
            pending_epoch: 0,
            debounce_token: 0,
        }
    }

    pub fn is_open(&self, id: &str) -> bool {
        self.project
            .as_ref()
            .is_some_and(|project| project.metadata.id == id)
    }

    fn discard_pending_save(&mut self) {
        self.pending_epoch = self.pending_epoch.wrapping_add(1);
        self.dirty = false;
    }

    pub fn selected_elements(&self) -> Vec<TimelineElement> {
        self.tracks()
            .into_iter()
            .flat_map(Track::elements)
            .filter(|element| self.is_selected(&element.base().id))
            .cloned()
            .collect()
    }

    pub fn all_element_ids(&self) -> Vec<String> {
        self.tracks()
            .into_iter()
            .flat_map(Track::elements)
            .map(|element| element.base().id.clone())
            .collect()
    }

    pub fn project_name(&self) -> String {
        self.project
            .as_ref()
            .map(|project| project.metadata.name.clone())
            .unwrap_or_else(|| cutix_i18n::t("projects.untitled"))
    }

    pub fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        self.dark = !self.dark;
        self.theme = if self.dark {
            Theme::dark()
        } else {
            Theme::light()
        };
        let mut settings = load_settings();
        settings.dark = Some(self.dark);
        save_settings(&settings);
        cx.notify();
    }

    pub fn set_locale(&mut self, code: &str, cx: &mut Context<Self>) {
        if !cutix_i18n::set_locale(code) {
            return;
        }
        self.locale = code.to_string();
        let mut settings = load_settings();
        settings.locale = Some(self.locale.clone());
        save_settings(&settings);
        cx.notify();
    }

    pub fn browse_videos(&mut self, directory: std::path::PathBuf, cx: &mut Context<Self>) {
        self.video_directory = Some(directory);
        self.route = Route::Library;
        cx.notify();
    }

    pub fn set_video_directory(
        &mut self,
        directory: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) {
        self.video_directory = directory.clone();
        let mut settings = load_settings();
        settings.video_directory = directory;
        save_settings(&settings);
        cx.notify();
    }

    pub fn visible_projects(&self) -> Vec<ProjectSummary> {
        let query = self.search.trim().to_lowercase();
        let mut list: Vec<ProjectSummary> = self
            .projects
            .iter()
            .filter(|project| query.is_empty() || project.name.to_lowercase().contains(&query))
            .cloned()
            .collect();

        list.sort_by(|left, right| {
            let ordering = match self.sort_key {
                SortKey::CreatedAt => left.created_at.cmp(&right.created_at),
                SortKey::UpdatedAt => left.updated_at.cmp(&right.updated_at),
                SortKey::Name => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
                SortKey::Duration => left.duration.as_ticks().cmp(&right.duration.as_ticks()),
            };
            if self.sort_ascending {
                ordering
            } else {
                ordering.reverse()
            }
        });
        list
    }

    pub fn refresh_projects(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        cx.spawn(async move |this, cx| {
            let (listed, unreadable) = cx
                .background_spawn(async move { list_readable_projects(&store) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.projects = listed;
                this.projects_loaded = true;
                if unreadable > 0 {
                    this.notice = Some(format!(
                        "{} ({unreadable})",
                        cutix_i18n::t("toast.project.unreadable")
                    ));
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn create_project(&mut self, cx: &mut Context<Self>) {
        let store = self.store.clone();
        let name = cutix_i18n::t("projects.new");
        cx.spawn(async move |this, cx| {
            let created = cx.background_spawn(async move { store.create(name) }).await;
            let _ = this.update(cx, |this, cx| match created {
                Ok(project) => {
                    let id = project.metadata.id.clone();
                    this.refresh_projects(cx);
                    this.open_project(&id, cx);
                }
                Err(error) => {
                    this.notice = Some(failure("toast.project.createFailed", &error));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn open_video_as_project(&mut self, path: PathBuf, name: String, cx: &mut Context<Self>) {
        if !path.is_file() {
            self.notice = Some(cutix_i18n::t("toast.media.missing"));
            cx.notify();
            return;
        }
        let store = self.store.clone();
        let name = if name.trim().is_empty() {
            cutix_i18n::t("projects.new")
        } else {
            name
        };

        cx.spawn(async move |this, cx| {
            let created = cx.background_spawn(async move { store.create(name) }).await;
            let _ = this.update(cx, |this, cx| match created {
                Ok(project) => {
                    let id = project.metadata.id.clone();
                    this.refresh_projects(cx);
                    this.pending_import = vec![path];
                    this.pending_on_timeline = true;
                    this.open_project(&id, cx);
                }
                Err(error) => {
                    this.notice = Some(failure("toast.project.createFailed", &error));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn import_project_package(&mut self, archive: std::path::PathBuf, cx: &mut Context<Self>) {
        let store = self.store.clone();
        cx.spawn(async move |this, cx| {
            let imported = cx
                .background_spawn(async move { cutix_export::import_package(&store, &archive) })
                .await;
            let _ = this.update(cx, |this, cx| match imported {
                Ok(project) => {
                    let id = project.metadata.id.clone();
                    this.refresh_projects(cx);
                    this.open_project(&id, cx);
                }
                Err(error) => {
                    this.notice = Some(format!(
                        "{}: {error}",
                        cutix_i18n::t("toast.project.importFailed")
                    ));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub fn open_project(&mut self, id: &str, cx: &mut Context<Self>) {
        self.save_now();
        self.editor_origin = if self.route == Route::Library {
            Route::Library
        } else {
            Route::Projects
        };
        let store = self.store.clone();
        let id = id.to_string();
        cx.spawn(async move |this, cx| {
            let loaded = cx
                .background_spawn(async move {
                    let project = store.load(&id)?.project;
                    let media = MediaStore::for_project(&store, &id);
                    let assets = media.list().unwrap_or_default();
                    Ok::<_, cutix_project::ProjectError>((
                        project,
                        assets,
                        media.root().to_path_buf(),
                    ))
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                match loaded {
                    Ok((project, media, root)) => {
                        this.project = Some(project);
                        this.media = media;
                        this.media_root = Some(root);
                        this.waveforms.clear();
                        this.refresh_missing_media();
                        this.selection.clear();
                        this.history.clear();
                        this.playhead = time::MediaTime::ZERO;
                        this.route = Route::Editor;
                        this.start_preview(cx);
                        let waiting = std::mem::take(&mut this.pending_import);
                        let onto_timeline = std::mem::take(&mut this.pending_on_timeline);
                        if !waiting.is_empty() {
                            if onto_timeline && waiting.len() == 1 {
                                this.import_media_at_playhead(waiting[0].clone(), cx);
                            } else {
                                this.import_media(waiting, cx);
                            }
                        }
                    }
                    Err(error) => {
                        this.pending_import.clear();
                        this.pending_on_timeline = false;
                        this.notice = Some(failure("toast.project.notFound", &error));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn close_project(&mut self, cx: &mut Context<Self>) {
        self.save_now();
        self.route = self.editor_origin;
        self.project = None;
        self.media.clear();
        self.media_root = None;
        self.waveforms.clear();
        self.missing_media.clear();
        self.selection.clear();
        self.history.clear();
        self.playhead = time::MediaTime::ZERO;
        self.stop_preview(cx);
        self.refresh_projects(cx);
        cx.notify();
    }

    pub fn rename_project(&mut self, id: &str, name: String, cx: &mut Context<Self>) {
        let store = self.store.clone();
        let id = id.to_string();
        let open = self
            .project
            .as_ref()
            .is_some_and(|project| project.metadata.id == id);

        if open {
            self.save_now();
        }
        cx.spawn(async move |this, cx| {
            let renamed = cx
                .background_spawn(async move { store.rename(&id, name) })
                .await;
            let _ = this.update(cx, |this, cx| {
                match renamed {
                    Ok(project) => {
                        if open {
                            this.project = Some(project);
                        }
                        this.refresh_projects(cx);
                    }
                    Err(error) => this.notice = Some(failure("toast.project.renameFailed", &error)),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn duplicate_project(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.is_open(id) {
            self.save_now();
        }
        let store = self.store.clone();
        let id = id.to_string();
        cx.spawn(async move |this, cx| {
            let duplicated = cx
                .background_spawn(async move { store.duplicate(&id) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Err(error) = duplicated {
                    this.notice = Some(failure("toast.project.duplicateFailed", &error));
                }
                this.refresh_projects(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn delete_project(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.is_open(id) {
            self.discard_pending_save();
        }
        let store = self.store.clone();
        let id = id.to_string();
        cx.spawn(async move |this, cx| {
            let deleted = cx.background_spawn(async move { store.delete(&id) }).await;
            let _ = this.update(cx, |this, cx| {
                if let Err(error) = deleted {
                    this.notice = Some(failure("toast.project.deleteFailed", &error));
                }
                this.refresh_projects(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn import_media_at_playhead(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(project_id) = self
            .project
            .as_ref()
            .map(|project| project.metadata.id.clone())
        else {
            self.notice = Some(cutix_i18n::t("toast.project.noActive"));
            cx.notify();
            return;
        };
        if !path.is_file() {
            return;
        }

        self.importing += 1;
        self.notice = None;
        cx.notify();

        let store = self.store.clone();
        let start = self.playhead;
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_spawn(async move {
                    let media = MediaStore::for_project(&store, &project_id);
                    let root = media.root().to_path_buf();
                    (media.import(&path), file_label(&path), root)
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                let (imported, label, root) = outcome;
                this.importing = this.importing.saturating_sub(1);
                this.media_root = Some(root);
                match imported {
                    Ok(asset) => {
                        let bare = this.timeline_is_empty();
                        this.media.push(asset.clone());
                        this.refresh_missing_media();
                        this.edit(cx, |editor| editor.insert_media(&asset, start, None));
                        if bare {
                            this.adopt_media_frame_rate(&asset);
                        }
                    }
                    Err(error) => {
                        this.notice = Some(cutix_i18n::t_args(
                            "toast.media.processFailedWhy",
                            &[("file", &label), ("why", &error.to_string())],
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn import_media(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let Some(project_id) = self
            .project
            .as_ref()
            .map(|project| project.metadata.id.clone())
        else {
            self.notice = Some(cutix_i18n::t("toast.project.noActive"));
            cx.notify();
            return;
        };
        let paths: Vec<PathBuf> = paths.into_iter().filter(|path| path.is_file()).collect();
        if paths.is_empty() {
            return;
        }

        self.importing += paths.len();
        self.notice = None;
        cx.notify();

        let store = self.store.clone();
        cx.spawn(async move |this, cx| {
            let count = paths.len();
            let outcome = cx
                .background_spawn(async move {
                    let media = MediaStore::for_project(&store, &project_id);
                    let mut imported = Vec::new();
                    let mut failures = Vec::new();
                    for path in paths {
                        match media.import(&path) {
                            Ok(asset) => imported.push(asset),
                            Err(error) => failures.push((file_label(&path), error.to_string())),
                        }
                    }
                    (imported, failures, media.root().to_path_buf())
                })
                .await;

            let _ = this.update(cx, |this, cx| {
                let (imported, failures, root) = outcome;
                this.importing = this.importing.saturating_sub(count);
                this.media_root = Some(root);
                this.media.extend(imported);
                this.refresh_missing_media();
                if let Some((name, why)) = failures.first() {
                    this.notice = Some(cutix_i18n::t_args(
                        "toast.media.processFailedWhy",
                        &[("file", name), ("why", why)],
                    ));
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn start_preview(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else {
            return;
        };
        let scene = self.current_scene().map(|scene| scene.id.clone());
        if let Some(stale) = self.preview.open(&project, scene, &self.store, &self.media) {
            cx.drop_image(stale, None);
        }
    }

    pub fn stop_preview(&mut self, cx: &mut Context<Self>) {
        if let Some(stale) = self.preview.close() {
            cx.drop_image(stale, None);
        }
    }

    pub fn edit<F>(&mut self, cx: &mut Context<Self>, run: F) -> bool
    where
        F: FnOnce(&mut Editor<'_>) -> bool,
    {
        self.edit_coalesced(None, cx, run)
    }

    pub fn edit_coalesced<F>(
        &mut self,
        coalesce: Option<String>,
        cx: &mut Context<Self>,
        run: F,
    ) -> bool
    where
        F: FnOnce(&mut Editor<'_>) -> bool,
    {
        let fps = self.fps();
        let ripple = self.ripple;
        let Some(project) = self.project.as_mut() else {
            return false;
        };
        let mut editor = Editor {
            project,
            history: &mut self.history,
            selection: &mut self.selection,
            ripple,
            fps,
            coalesce,
        };
        if !run(&mut editor) {
            return false;
        }
        self.publish(cx);
        true
    }

    pub fn undo(&mut self, cx: &mut Context<Self>) {
        if self.edit(cx, |editor| editor.undo().is_some()) {
            cx.notify();
        }
    }

    pub fn redo(&mut self, cx: &mut Context<Self>) {
        if self.edit(cx, |editor| editor.redo().is_some()) {
            cx.notify();
        }
    }

    fn publish(&mut self, cx: &mut Context<Self>) {
        if let Some(project) = self.project.as_ref() {
            self.preview.sync_project(project);
        }
        self.save_project(cx);
        cx.notify();
    }

    pub fn record_attribution(
        &mut self,
        attribution: cutix_project::model::Attribution,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(project) = self.project.as_mut() else {
            return false;
        };
        let entries = project.attributions.get_or_insert_with(Vec::new);
        if entries.iter().any(|entry| entry.id == attribution.id) {
            return false;
        }
        entries.push(attribution);
        self.publish(cx);
        true
    }

    pub fn update_settings<F>(&mut self, cx: &mut Context<Self>, change: F) -> bool
    where
        F: FnOnce(&mut cutix_project::model::ProjectSettings) -> bool,
    {
        let Some(project) = self.project.as_mut() else {
            return false;
        };
        if !change(&mut project.settings) {
            return false;
        }
        self.publish(cx);
        true
    }

    pub fn transition_target(&self) -> Option<String> {
        let scene = self.current_scene()?;
        let playhead = self.playhead.as_ticks();
        let mut best: Option<(i64, bool, String)> = None;
        let video_tracks = std::iter::once(&scene.tracks.main).chain(scene.tracks.overlay.iter());
        for track in video_tracks {
            if !matches!(track, Track::Video { .. }) {
                continue;
            }
            let mut elements: Vec<&TimelineElement> = track
                .elements()
                .iter()
                .filter(|element| cutix_playback::transitions::can_element_have_transition(element))
                .collect();
            elements.sort_by_key(|element| element.base().start_time.as_ticks());
            for index in 1..elements.len() {
                let previous = elements[index - 1].base();
                let current = elements[index].base();
                let gap = (previous.start_time.as_ticks() + previous.duration.as_ticks()
                    - current.start_time.as_ticks())
                .abs();
                if gap > cutix_playback::transitions::ADJACENCY_TOLERANCE_TICKS {
                    continue;
                }
                let selected = self.is_selected(&current.id);
                let distance = (current.start_time.as_ticks() - playhead).abs();
                let better = match &best {
                    None => true,
                    Some((best_distance, best_selected, _)) => {
                        (selected, -distance) > (*best_selected, -*best_distance)
                    }
                };
                if better {
                    best = Some((distance, selected, current.id.clone()));
                }
            }
        }
        best.map(|(_, _, id)| id)
    }

    pub fn seek(&mut self, time: time::MediaTime, cx: &mut Context<Self>) {
        let clamped = time.max(time::MediaTime::ZERO);
        self.playhead = clamped;
        self.preview.seek(clamped);
        cx.notify();
    }

    pub fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        if !self.preview.is_open() {
            self.start_preview(cx);
        }
        if !self.preview.is_playing() {
            self.preview.seek(self.playhead);
        }
        self.preview.toggle();
        cx.notify();
    }

    pub fn select_only(&mut self, id: &str, cx: &mut Context<Self>) {
        self.selection = vec![id.to_string()];
        cx.notify();
    }

    pub fn extend_selection(&mut self, id: &str, cx: &mut Context<Self>) {
        let id = id.to_string();
        if let Some(index) = self.selection.iter().position(|value| *value == id) {
            self.selection.remove(index);
        } else {
            self.selection.push(id);
        }
        cx.notify();
    }

    pub fn is_selected(&self, id: &str) -> bool {
        self.selection.iter().any(|value| value == id)
    }

    pub fn timeline_is_empty(&self) -> bool {
        let Some(scene) = self.current_scene() else {
            return true;
        };
        scene.tracks.main.elements().is_empty()
            && scene
                .tracks
                .overlay
                .iter()
                .chain(scene.tracks.audio.iter())
                .all(|track| track.elements().is_empty())
    }

    pub fn adopt_media_frame_rate(&mut self, asset: &cutix_project::MediaAssetData) {
        let Some(rate) = asset.fps.and_then(time::FrameRate::nearest) else {
            return;
        };
        let Some(project) = self.project.as_mut() else {
            return;
        };
        // Compared in reduced form: a project saved before frame rates were reduced holds
        // the same rate written as a different fraction, and adopting it again would mark
        // the project dirty for no change the user made.
        if project.settings.fps.reduced() == rate.reduced() {
            return;
        }
        project.settings.fps = rate;
        self.dirty = true;
    }

    pub fn current_scene(&self) -> Option<&Scene> {
        let project = self.project.as_ref()?;
        project
            .scenes
            .iter()
            .find(|scene| scene.id == project.current_scene_id)
            .or_else(|| project.scenes.first())
    }

    pub fn scene_names(&self) -> Vec<(String, String)> {
        self.project
            .as_ref()
            .map(|project| {
                project
                    .scenes
                    .iter()
                    .map(|scene| (scene.id.clone(), scene.name.clone()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn set_scene(&mut self, scene_id: &str, cx: &mut Context<Self>) {
        if let Some(project) = self.project.as_mut() {
            project.current_scene_id = scene_id.to_string();
        }
        self.selection.clear();
        self.history.clear();
        self.save_project(cx);
        cx.notify();
    }

    pub fn save_project(&mut self, cx: &mut Context<Self>) {
        if self.project.is_none() {
            return;
        }
        let opened_a_new_run = !self.dirty;
        self.dirty = true;
        self.debounce_token = self.debounce_token.wrapping_add(1);

        let token = self.debounce_token;
        let epoch = self.pending_epoch;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(AUTOSAVE_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| {
                if this.debounce_token == token && this.pending_epoch == epoch {
                    this.flush_save(cx);
                }
            });
        })
        .detach();

        if opened_a_new_run {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(AUTOSAVE_MAX_DEFER).await;
                let _ = this.update(cx, |this, cx| {
                    if this.pending_epoch == epoch {
                        this.flush_save(cx);
                    }
                });
            })
            .detach();
        }
    }

    pub fn flush_save(&mut self, cx: &mut Context<Self>) {
        let Some((project, thumbnail)) = self.take_pending_save() else {
            return;
        };
        let store = self.store.clone();
        cx.background_spawn(async move {
            write_project(&store, &project, thumbnail);
        })
        .detach();
    }

    pub fn save_now(&mut self) {
        let Some((project, thumbnail)) = self.take_pending_save() else {
            return;
        };
        write_project(&self.store, &project, thumbnail);
    }

    fn take_pending_save(&mut self) -> Option<(Project, Option<crate::playback::Thumbnail>)> {
        let thumbnail = self.preview.take_thumbnail();
        if !self.dirty && thumbnail.is_none() {
            return None;
        }
        self.pending_epoch = self.pending_epoch.wrapping_add(1);
        self.dirty = false;

        if let Some(project) = self.project.as_mut() {
            let _ = self.store.externalize_mattes(project);
        }

        if thumbnail.is_some() {
            if let Some(project) = self.project.as_mut() {
                project.metadata.thumbnail =
                    Some(cutix_project::store::THUMBNAIL_FILE_NAME.to_string());
            }
        }
        Some((self.project.clone()?, thumbnail))
    }

    pub fn tracks(&self) -> Vec<&Track> {
        let Some(scene) = self.current_scene() else {
            return Vec::new();
        };
        let mut tracks: Vec<&Track> = scene.tracks.overlay.iter().collect();
        tracks.push(&scene.tracks.main);
        tracks.extend(scene.tracks.audio.iter());
        tracks
    }

    pub fn element_by_id(&self, id: &str) -> Option<&TimelineElement> {
        self.tracks()
            .into_iter()
            .flat_map(Track::elements)
            .find(|element| element.base().id == id)
    }

    pub fn track_of(&self, element_id: &str) -> Option<String> {
        self.tracks().into_iter().find_map(|track| {
            track
                .elements()
                .iter()
                .any(|element| element.base().id == element_id)
                .then(|| track.id().to_string())
        })
    }

    pub fn media_by_id(&self, id: &str) -> Option<&MediaAssetData> {
        self.media.iter().find(|asset| asset.id == id)
    }

    pub fn bookmarks(&self) -> &[cutix_project::model::Bookmark] {
        self.current_scene()
            .map(|scene| scene.bookmarks.as_slice())
            .unwrap_or_default()
    }

    pub fn media_has_audio(&self, media_id: &str) -> bool {
        self.media_by_id(media_id)
            .is_some_and(|asset| asset.has_audio != Some(false))
    }

    pub fn source_audio_target(&self) -> Option<(String, bool)> {
        self.selection.iter().find_map(|id| {
            let element = self.element_by_id(id)?;
            let has_audio = element
                .media_id()
                .is_some_and(|media_id| self.media_has_audio(media_id));
            crate::edit::can_toggle_source_audio(element, has_audio)
                .then(|| (id.clone(), has_audio))
        })
    }

    pub fn selected_element(&self) -> Option<&TimelineElement> {
        let id = self.selection.first()?.as_str();
        self.tracks()
            .into_iter()
            .flat_map(Track::elements)
            .find(|element| element.base().id == id)
    }

    pub fn fps(&self) -> f32 {
        self.project
            .as_ref()
            .and_then(|project| project.settings.fps.as_f64())
            .unwrap_or(crate::theme::DEFAULT_FPS as f64) as f32
    }

    pub fn frame_rate(&self) -> time::FrameRate {
        self.project
            .as_ref()
            .map(|project| project.settings.fps)
            .unwrap_or(time::FrameRate::FPS_30)
    }

    pub fn canvas(&self) -> (f32, f32) {
        self.project
            .as_ref()
            .map(|project| {
                (
                    project.settings.canvas_size.width as f32,
                    project.settings.canvas_size.height as f32,
                )
            })
            .unwrap_or(crate::theme::DEFAULT_CANVAS_SIZE)
    }

    pub fn content_duration(&self) -> time::MediaTime {
        let Some(project) = self.project.as_ref() else {
            return time::MediaTime::ZERO;
        };
        let scene = self.current_scene().map(|scene| scene.id.clone());
        let measured = cutix_export::scene_duration(project, scene.as_deref());
        if measured > time::MediaTime::ZERO {
            measured
        } else {
            self.total_duration()
        }
    }

    pub fn total_duration(&self) -> time::MediaTime {
        self.project
            .as_ref()
            .map(|project| project.metadata.duration)
            .unwrap_or(time::MediaTime::ZERO)
    }

    pub fn media_source_path(&self, asset: &MediaAssetData) -> Option<PathBuf> {
        let root = self.media_root.as_ref()?;
        Some(MediaStore::new(root.clone()).source_file(asset))
    }

    pub fn media_asset(&self, media_id: &str) -> Option<&MediaAssetData> {
        self.media.iter().find(|asset| asset.id == media_id)
    }

    pub fn is_media_missing(&self, media_id: &str) -> bool {
        self.missing_media.contains(media_id)
    }

    pub fn refresh_missing_media(&mut self) {
        let missing = self
            .media
            .iter()
            .filter(|asset| {
                self.media_source_path(asset)
                    .map(|path| !path.is_file())
                    .unwrap_or(false)
            })
            .map(|asset| asset.id.clone())
            .collect();
        self.missing_media = missing;
    }

    pub fn waveform(&self, media_id: &str) -> Option<&std::sync::Arc<WaveformPeaks>> {
        match self.waveforms.get(media_id) {
            Some(WaveformState::Ready(peaks)) => Some(peaks),
            _ => None,
        }
    }

    pub fn ensure_waveform(&mut self, media_id: &str, cx: &mut Context<Self>) {
        if self.waveforms.contains_key(media_id) || self.missing_media.contains(media_id) {
            return;
        }
        let Some(asset) = self.media_asset(media_id) else {
            return;
        };
        if asset.has_audio == Some(false) {
            self.waveforms
                .insert(media_id.to_owned(), WaveformState::Unavailable);
            return;
        }
        let Some(path) = self.media_source_path(asset) else {
            return;
        };
        self.waveforms
            .insert(media_id.to_owned(), WaveformState::Pending);
        let key = media_id.to_owned();
        cx.spawn(async move |this, cx| {
            let peaks = cx
                .background_spawn(async move {
                    cutix_playback::peaks_for_file(&path, cutix_playback::DEFAULT_BUCKETS).ok()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let state = match peaks {
                    Some(peaks) if !peaks.is_empty() => {
                        WaveformState::Ready(std::sync::Arc::new(peaks))
                    }
                    _ => WaveformState::Unavailable,
                };
                this.waveforms.insert(key, state);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn thumbnail_path(&self, asset: &MediaAssetData) -> Option<PathBuf> {
        let root = self.media_root.as_ref()?;
        let relative = asset.thumbnail_url.as_ref()?;
        let path = root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR));
        path.is_file().then_some(path)
    }

    pub fn project_thumbnail(&self, summary: &ProjectSummary) -> Option<PathBuf> {
        summary.thumbnail.as_ref()?;
        let path = self.store.thumbnail_file(&summary.id);
        path.is_file().then_some(path)
    }
}

pub fn media_glyph(kind: MediaType) -> &'static str {
    match kind {
        MediaType::Video => "video01",
        MediaType::Audio => "music-note03",
        MediaType::Image => "happy01",
    }
}

fn list_readable_projects(store: &ProjectStore) -> (Vec<ProjectSummary>, usize) {
    let mut summaries = Vec::new();
    let mut unreadable = 0usize;
    for id in store.list_project_ids().unwrap_or_default() {
        match store.summary(&id) {
            Ok(summary) => summaries.push(summary),
            Err(_) => unreadable += 1,
        }
    }
    summaries.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    (summaries, unreadable)
}

fn write_project(
    store: &ProjectStore,
    project: &Project,
    thumbnail: Option<crate::playback::Thumbnail>,
) {
    if let Some(frame) = thumbnail {
        let _ = store.save_thumbnail_from_rgba(
            &project.metadata.id,
            frame.width,
            frame.height,
            &frame.rgba,
        );
    }
    let _ = store.save(project);
}

fn file_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_string()
}

fn failure(key: &str, error: &cutix_project::ProjectError) -> String {
    format!("{}: {error}", cutix_i18n::t(key))
}

pub fn shared(store: ProjectStore, dark: bool, cx: &mut App) -> Entity<AppModel> {
    cx.new(|cx| {
        let mut model = AppModel::new(store, dark);
        model.refresh_projects(cx);
        model
    })
}

#[cfg(test)]
mod autosave_tests {
    use super::*;
    use gpui::TestAppContext;

    fn scratch() -> (tempfile::TempDir, ProjectStore) {
        let directory = tempfile::tempdir().expect("temp dir");
        let store = ProjectStore::new(directory.path().join("cutix"));
        (directory, store)
    }

    fn model(store: &ProjectStore, cx: &mut TestAppContext) -> Entity<AppModel> {
        let store = store.clone();
        cx.update(|cx| cx.new(|_| AppModel::new(store, true)))
    }

    fn with_project(
        model: &Entity<AppModel>,
        store: &ProjectStore,
        cx: &mut TestAppContext,
    ) -> String {
        let project = store.create("Autosave").expect("create");
        let id = project.metadata.id.clone();
        model.update(cx, |this, _| {
            this.project = Some(project);
            this.route = Route::Editor;
        });
        id
    }

    fn rename_on_disk(store: &ProjectStore, id: &str) -> String {
        store.load(id).expect("load").project.metadata.name
    }

    fn edit_name(model: &Entity<AppModel>, name: &str, cx: &mut TestAppContext) {
        model.update(cx, |this, cx| {
            if let Some(project) = this.project.as_mut() {
                project.metadata.name = name.to_string();
            }
            this.save_project(cx);
        });
    }

    #[gpui::test]
    async fn opening_a_video_as_a_project_imports_it_once_the_project_is_open(
        cx: &mut TestAppContext,
    ) {
        let (directory, store) = scratch();
        let model = model(&store, cx);

        let clip = directory.path().join("clip.mp4");
        std::fs::write(&clip, b"not really a video").expect("write");

        model.update(cx, |this, cx| {
            this.open_video_as_project(clip.clone(), "Clip".to_string(), cx)
        });
        cx.run_until_parked();

        model.read_with(cx, |this, _| {
            assert!(this.project.is_some(), "the project has to be open");
            assert!(
                this.pending_import.is_empty(),
                "the waiting import has to be handed over, not left behind"
            );
            assert_ne!(
                this.notice.as_deref(),
                Some(cutix_i18n::t("toast.project.noActive").as_str()),
                "importing must not run before the project is open"
            );
        });
    }

    #[gpui::test]
    async fn a_burst_of_edits_is_written_once_after_the_user_stops(cx: &mut TestAppContext) {
        let (_guard, store) = scratch();
        let model = model(&store, cx);
        let id = with_project(&model, &store, cx);

        for step in 0..25 {
            edit_name(&model, &format!("step-{step}"), cx);
        }

        assert_eq!(rename_on_disk(&store, &id), "Autosave");
        assert!(model.read_with(cx, |this, _| this.is_dirty()));

        cx.executor().advance_clock(AUTOSAVE_DEBOUNCE * 2);
        cx.run_until_parked();

        assert_eq!(rename_on_disk(&store, &id), "step-24");
        assert!(!model.read_with(cx, |this, _| this.is_dirty()));
    }

    #[gpui::test]
    async fn an_uninterrupted_drag_still_reaches_disk(cx: &mut TestAppContext) {
        let (_guard, store) = scratch();
        let model = model(&store, cx);
        let id = with_project(&model, &store, cx);

        for step in 0..10 {
            edit_name(&model, &format!("drag-{step}"), cx);
            cx.executor().advance_clock(AUTOSAVE_DEBOUNCE / 2);
            cx.run_until_parked();
        }

        assert!(
            rename_on_disk(&store, &id).starts_with("drag-"),
            "a gesture longer than the deferral cap never got written"
        );
    }

    #[gpui::test]
    async fn closing_immediately_after_an_edit_keeps_it(cx: &mut TestAppContext) {
        let (_guard, store) = scratch();
        let model = model(&store, cx);
        let id = with_project(&model, &store, cx);

        edit_name(&model, "typed then closed", cx);

        model.update(cx, |this, cx| this.close_project(cx));

        assert_eq!(rename_on_disk(&store, &id), "typed then closed");
        assert!(!model.read_with(cx, |this, _| this.is_dirty()));
    }

    #[gpui::test]
    async fn switching_projects_immediately_after_an_edit_keeps_it(cx: &mut TestAppContext) {
        let (_guard, store) = scratch();
        let model = model(&store, cx);
        let id = with_project(&model, &store, cx);
        let other = store.create("Other").expect("create other");

        edit_name(&model, "typed then switched", cx);
        model.update(cx, |this, cx| this.open_project(&other.metadata.id, cx));
        cx.run_until_parked();

        assert_eq!(rename_on_disk(&store, &id), "typed then switched");
    }

    #[gpui::test]
    async fn renaming_the_open_project_does_not_rename_away_unsaved_edits(cx: &mut TestAppContext) {
        let (_guard, store) = scratch();
        let model = model(&store, cx);
        let id = with_project(&model, &store, cx);

        model.update(cx, |this, cx| {
            if let Some(project) = this.project.as_mut() {
                project.metadata.duration = time::MediaTime::from_ticks(9 * 120_000);
            }
            this.save_project(cx);
        });
        model.update(cx, |this, cx| {
            this.rename_project(&id.clone(), "Renamed".to_string(), cx)
        });
        cx.run_until_parked();

        let on_disk = store.load(&id).expect("load").project;
        assert_eq!(on_disk.metadata.name, "Renamed");
        assert_eq!(
            on_disk.metadata.duration.as_ticks(),
            9 * 120_000,
            "the rename read a stale document"
        );
    }

    #[gpui::test]
    async fn an_explicit_flush_leaves_nothing_owed(cx: &mut TestAppContext) {
        let (_guard, store) = scratch();
        let model = model(&store, cx);
        let id = with_project(&model, &store, cx);

        edit_name(&model, "flushed", cx);
        model.update(cx, |this, _| this.save_now());
        assert_eq!(rename_on_disk(&store, &id), "flushed");

        model.update(cx, |this, _| {
            if let Some(project) = this.project.as_mut() {
                project.metadata.name = "later, unsaved".to_string();
            }
        });
        cx.executor().advance_clock(AUTOSAVE_DEBOUNCE * 4);
        cx.run_until_parked();
        assert_eq!(rename_on_disk(&store, &id), "flushed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_dates_render_day_first() {
        assert_eq!(format_date("2026-08-13T09:12:44.123Z"), "13.08.2026");
        assert_eq!(format_date("nonsense"), "nonsense");
    }

    #[test]
    fn a_zero_duration_has_no_badge() {
        assert!(format_duration(time::MediaTime::from_ticks(0)).is_none());
    }

    #[test]
    fn short_clips_use_minutes_and_seconds() {
        assert_eq!(format_seconds(65.0).as_deref(), Some("01:05"));
        assert_eq!(format_seconds(3725.0).as_deref(), Some("01:02:05"));
    }

    #[test]
    fn a_saved_rect_inside_the_display_is_kept() {
        let geometry = WindowGeometry {
            x: 100.0,
            y: 80.0,
            width: 1280.0,
            height: 800.0,
            maximized: false,
        };
        assert_eq!(geometry.clamped((0.0, 0.0, 1920.0, 1080.0)), geometry);
        assert!(geometry.is_sane());
    }

    #[test]
    fn an_off_screen_rect_is_pulled_back_into_view() {
        let geometry = WindowGeometry {
            x: 5000.0,
            y: -900.0,
            width: 1280.0,
            height: 800.0,
            maximized: true,
        };
        let clamped = geometry.clamped((0.0, 0.0, 1920.0, 1080.0));
        assert_eq!(clamped.x, 640.0);
        assert_eq!(clamped.y, 0.0);
        assert_eq!((clamped.width, clamped.height), (1280.0, 800.0));
        assert!(clamped.maximized, "the maximised flag survives clamping");
    }

    #[test]
    fn a_rect_larger_than_the_display_is_shrunk_to_it() {
        let geometry = WindowGeometry {
            x: 0.0,
            y: 0.0,
            width: 4000.0,
            height: 3000.0,
            maximized: false,
        };
        let clamped = geometry.clamped((0.0, 0.0, 1920.0, 1080.0));
        assert_eq!((clamped.width, clamped.height), (1920.0, 1080.0));
    }

    #[test]
    fn a_tiny_rect_is_raised_to_the_minimum_window_size() {
        let geometry = WindowGeometry {
            x: 10.0,
            y: 10.0,
            width: 20.0,
            height: 20.0,
            maximized: false,
        };
        let clamped = geometry.clamped((0.0, 0.0, 1920.0, 1080.0));
        assert_eq!(
            (clamped.width, clamped.height),
            (WindowGeometry::MIN_WIDTH, WindowGeometry::MIN_HEIGHT)
        );
    }

    #[test]
    fn a_garbage_rect_is_rejected_before_it_is_used() {
        let geometry = WindowGeometry {
            x: f32::NAN,
            y: 0.0,
            width: 1280.0,
            height: 800.0,
            maximized: false,
        };
        assert!(!geometry.is_sane());
        assert!(!WindowGeometry::default().is_sane());
    }

    #[test]
    fn an_idle_queue_has_nothing_to_show() {
        let idle = UploadSummary::default();
        assert!(!idle.is_busy());
        assert_eq!(idle.fraction(), 0.0);
    }

    #[test]
    fn the_bar_counts_the_whole_batch_rather_than_only_the_file_in_front() {
        let summary = UploadSummary {
            active: 2,
            percent: 50,
            title: None,
            finished: 2,
        };
        assert!(summary.is_busy());
        assert!((summary.fraction() - 0.625).abs() < 1e-6);
    }

    #[test]
    fn one_upload_on_its_own_reads_as_its_own_percentage() {
        let summary = UploadSummary {
            active: 1,
            percent: 40,
            title: Some(String::from("clip")),
            finished: 0,
        };
        assert!((summary.fraction() - 0.4).abs() < 1e-6);
    }

    #[test]
    fn a_percentage_beyond_a_hundred_cannot_overfill_the_bar() {
        let summary = UploadSummary {
            active: 1,
            percent: 4_000,
            title: None,
            finished: 0,
        };
        assert_eq!(summary.fraction(), 1.0);
    }

    #[test]
    fn geometry_round_trips_through_the_settings_document() {
        let settings = Settings {
            locale: Some("ru".into()),
            dark: Some(false),
            window: Some(WindowGeometry {
                x: 12.0,
                y: 34.0,
                width: 1000.0,
                height: 700.0,
                maximized: true,
            }),
            video_directory: Some(std::path::PathBuf::from("/videos")),
            preview_volume: Some(0.5),
            preview_muted: Some(true),
            publish_privacy: Some("unlisted".into()),
        };
        let json = serde_json::to_string(&settings).expect("serialise");
        assert!(json.contains("\"maximized\":true"), "{json}");
        let back: Settings = serde_json::from_str(&json).expect("parse");
        assert_eq!(back.window, settings.window);
        assert_eq!(back.video_directory, settings.video_directory);

        let legacy: Settings = serde_json::from_str("{\"locale\":\"en\"}").expect("older document");
        assert!(legacy.window.is_none());
        assert!(legacy.video_directory.is_none());
    }

    #[test]
    fn every_sort_key_has_a_translated_label() {
        for key in [
            SortKey::CreatedAt,
            SortKey::UpdatedAt,
            SortKey::Name,
            SortKey::Duration,
        ] {
            let label = key.label_key();
            assert_ne!(cutix_i18n::t(label), label, "{label}");
        }
    }
}
