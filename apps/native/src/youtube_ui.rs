use crate::input::{TextBuffer, TextField};
use crate::theme::Palette;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use youtube::accounts::Accounts;
use youtube::failure::{Failure, FailureNote};
use youtube::history::{History, HistoryEntry, HistoryFilter};
use youtube::publish::{self, Comments, Issue, License, Privacy, PublishSettings, Remix};
use youtube::queue::{Queue, Task, TaskState};
use youtube::session::Session;
use youtube::studio::{Control, Stage};

pub const STEP_ANIMATION_SECONDS: f32 = 0.22;
pub const COPIED_NOTICE_SECONDS: f32 = 2.0;
pub const AVATAR_PX: f32 = 32.0;

const ALLOWED_HOSTS: [&str; 5] = [
    "support.google.com",
    "myaccount.google.com",
    "studio.youtube.com",
    "www.youtube.com",
    "www.google.com",
];

pub fn is_openable_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or("");
    if !ALLOWED_HOSTS.contains(&host) {
        return false;
    }
    !url.contains(['"', '\'', '\n', '\r', '\t', ' ', '&', '|', '^', '<', '>'])
}

pub fn open_url(url: &str) {
    if !is_openable_url(url) {
        return;
    }
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("rundll32.exe")
            .arg("url.dll,FileProtocolHandler")
            .arg(url)
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(url).spawn();
    }
}

pub fn has_browser() -> bool {
    youtube::session::is_supported()
}

pub fn sign_in(
    directory: &std::path::Path,
    now: i64,
    cancel: &Arc<AtomicBool>,
) -> Result<(youtube::Account, PathBuf, Option<Vec<u8>>), Failure> {
    let scratch = format!("pending-{now:x}");
    let cancel = Arc::clone(cancel);
    let (account, avatar) = youtube::session::sign_in(directory, &scratch, now, &mut move || {
        !cancel.load(std::sync::atomic::Ordering::Relaxed)
    })?;
    let from = youtube::chrome::profile_directory(directory, &scratch);
    Ok((account, from, avatar))
}

pub fn adopt_profile(
    directory: &std::path::Path,
    from: &std::path::Path,
    account_id: &str,
) -> std::io::Result<()> {
    let to = youtube::chrome::profile_directory(directory, account_id);
    if from == to {
        return Ok(());
    }
    let _ = std::fs::remove_dir_all(&to);
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut last = None;
    for attempt in 0..PROFILE_MOVE_ATTEMPTS {
        match std::fs::rename(from, &to) {
            Ok(()) => return Ok(()),
            Err(error) => {
                last = Some(error);
                std::thread::sleep(std::time::Duration::from_millis(
                    PROFILE_MOVE_BACKOFF_MS * (attempt as u64 + 1),
                ));
            }
        }
    }
    Err(last.unwrap_or_else(|| std::io::Error::other("the profile could not be moved")))
}

const PROFILE_MOVE_ATTEMPTS: usize = 6;
const PROFILE_MOVE_BACKOFF_MS: u64 = 250;

pub fn run_upload(
    directory: &std::path::Path,
    account_id: &str,
    settings: &PublishSettings,
    source: &std::path::Path,
    job: &Arc<Mutex<UploadJob>>,
    cancel: &Arc<AtomicBool>,
) -> Result<String, Failure> {
    let mut session = Session::open(directory, account_id, true)?;
    if !session.is_signed_in()? {
        return Err(Failure::SignedOut);
    }

    let reporter = Arc::clone(job);
    let cancel = Arc::clone(cancel);
    let outcome = session.upload(settings, source, &mut move |stage, percent| {
        if let Ok(mut guard) = reporter.lock() {
            guard.stage = stage;
            guard.percent = percent;
        }
        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            Control::Cancel
        } else {
            Control::Continue
        }
    });

    session.close();
    outcome
}

pub fn unix_from_civil(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> i64 {
    let year = year as i64;
    let month = month as i64;
    let day = day as i64;
    let (year, month) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    days * 86_400 + hour.min(23) as i64 * 3_600 + minute.min(59) as i64 * 60
}

pub fn step_progress(elapsed: f32) -> f32 {
    let linear = (elapsed / STEP_ANIMATION_SECONDS).clamp(0.0, 1.0);
    1.0 - (1.0 - linear).powi(3)
}

pub fn failure_message(failure: &Failure) -> String {
    cutix_i18n::t_args(failure.message_key(), &[("detail", &failure.detail())])
}

pub fn issue_message(issue: &Issue) -> String {
    cutix_i18n::t(issue.message_key())
}

pub fn category_label(id: &str) -> String {
    let key = publish::category_key(id);
    let translated = cutix_i18n::t(&key);
    if translated == key {
        publish::category_label(id).unwrap_or(id).to_string()
    } else {
        translated
    }
}

pub fn account_line(accounts: &Accounts) -> String {
    match accounts.active() {
        Some(account) => cutix_i18n::t_args(
            "youtube.publish.accountIs",
            &[("channel", &account.display_name())],
        ),
        None => cutix_i18n::t("youtube.accounts.none"),
    }
}

#[derive(Debug, Default)]
pub struct UploadJob {
    pub stage: Stage,

    pub percent: u32,
}

pub fn fraction_of(percent: u32) -> f32 {
    (percent as f32 / 100.0).clamp(0.0, 1.0)
}

pub fn stage_label(stage: Stage, fraction: f32) -> String {
    match stage {
        Stage::Uploading => cutix_i18n::t_args(
            stage.message_key(),
            &[("percent", &((fraction * 100.0).round() as u32).to_string())],
        ),
        other => cutix_i18n::t(other.message_key()),
    }
}

pub struct Running {
    pub task_id: String,
    pub job: Arc<Mutex<UploadJob>>,
    pub cancel: Arc<AtomicBool>,
}

impl Running {
    pub fn snapshot(&self) -> (Stage, u32) {
        match self.job.lock() {
            Ok(guard) => (guard.stage, guard.percent),
            Err(_) => (Stage::Opening, 0),
        }
    }

    pub fn stop(&self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

pub struct RowView {
    pub id: String,
    pub account_id: String,
    pub title: String,
    pub status: String,
    pub detail: Option<String>,
    pub fraction: f32,
    pub show_bar: bool,
    pub can_cancel: bool,
    pub can_retry: bool,
    pub failed: bool,
}

pub fn task_status(task: &Task, live: Option<(Stage, f32)>) -> String {
    match (&task.state, live) {
        (TaskState::Running, Some((stage, fraction))) => stage_label(stage, fraction),
        (TaskState::Running, None) => cutix_i18n::t(Stage::Opening.message_key()),
        (state, _) => cutix_i18n::t(state.message_key()),
    }
}

pub fn note_message(note: &FailureNote) -> String {
    cutix_i18n::t_args(&note.key, &[("detail", &note.detail)])
}

pub fn row_view(task: &Task, live: Option<(Stage, f32)>) -> RowView {
    let running = task.state == TaskState::Running;
    let fraction = match (running, live) {
        (true, Some((_, fraction))) => fraction,
        _ => task.fraction(),
    };
    let note = task.failure_note();
    RowView {
        id: task.id.clone(),
        account_id: task.account_id.clone(),
        title: task.title().to_string(),
        status: task_status(task, live),
        detail: note.map(note_message),
        fraction,
        show_bar: task.state.is_active(),
        can_cancel: task.state.is_active(),
        can_retry: task.state.is_retryable()
            && note.map(FailureNote::worth_retrying).unwrap_or(true),
        failed: matches!(task.state, TaskState::Failed { .. }),
    }
}

#[derive(Default)]
pub struct Avatars {
    images: HashMap<String, Arc<gpui::RenderImage>>,

    missing: Vec<String>,
}

impl Avatars {
    pub fn get(&self, account_id: &str) -> Option<Arc<gpui::RenderImage>> {
        self.images.get(account_id).cloned()
    }

    pub fn store(&mut self, account_id: &str, image: Arc<gpui::RenderImage>) {
        self.missing.retain(|id| id != account_id);
        self.images.insert(account_id.to_string(), image);
    }

    pub fn ensure_loaded(&mut self, directory: &std::path::Path, account_id: &str) {
        if self.images.contains_key(account_id) || self.missing.iter().any(|id| id == account_id) {
            return;
        }
        let image = std::fs::read(Self::cache_path(directory, account_id))
            .ok()
            .and_then(|bytes| decode_avatar(&bytes));
        match image {
            Some(image) => {
                self.images.insert(account_id.to_string(), image);
            }
            None => self.missing.push(account_id.to_string()),
        }
    }

    pub fn cache_path(directory: &std::path::Path, account_id: &str) -> PathBuf {
        let stem: String = account_id
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                    character
                } else {
                    '-'
                }
            })
            .collect();
        directory.join("avatars").join(format!("{stem}.img"))
    }
}

use std::io::Read;

pub const MAX_AVATAR_BYTES: usize = 512 * 1024;

pub fn fetch_avatar(url: &str) -> Option<Vec<u8>> {
    let response = ureq::get(url)
        .timeout(std::time::Duration::from_secs(10))
        .call()
        .ok()?;
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_AVATAR_BYTES as u64)
        .read_to_end(&mut bytes)
        .ok()?;
    (!bytes.is_empty()).then_some(bytes)
}

pub fn decode_avatar(bytes: &[u8]) -> Option<Arc<gpui::RenderImage>> {
    let decoded = image::load_from_memory(bytes).ok()?;
    let rgba = decoded.to_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    let mut bgra = rgba.into_raw();
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let buffer = image::ImageBuffer::from_raw(width, height, bgra)?;
    Some(Arc::new(gpui::RenderImage::new(smallvec::smallvec![
        image::Frame::new(buffer)
    ])))
}

pub fn remembered_privacy() -> Privacy {
    let Some(path) = crate::state::settings_path() else {
        return Privacy::Private;
    };
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Privacy::Private;
    };
    let Ok(settings) = serde_json::from_str::<crate::state::Settings>(&raw) else {
        return Privacy::Private;
    };
    match settings.publish_privacy.as_deref() {
        Some("public") => Privacy::Public,
        Some("unlisted") => Privacy::Unlisted,
        _ => Privacy::Private,
    }
}

pub fn remember_privacy(privacy: Privacy) {
    let Some(path) = crate::state::settings_path() else {
        return;
    };
    let mut settings = std::fs::read_to_string(&path)
        .ok()
        .and_then(|raw| serde_json::from_str::<crate::state::Settings>(&raw).ok())
        .unwrap_or_default();
    settings.publish_privacy = Some(
        match privacy {
            Privacy::Public => "public",
            Privacy::Unlisted => "unlisted",
            Privacy::Private => "private",
        }
        .to_string(),
    );
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(&settings) {
        let _ = std::fs::write(path, text);
    }
}

#[cfg(test)]
mod privacy_memory_tests {
    use super::*;

    #[test]
    fn every_choice_survives_a_round_trip_through_the_settings_file() {
        for privacy in [Privacy::Public, Privacy::Unlisted, Privacy::Private] {
            let stored = match privacy {
                Privacy::Public => "public",
                Privacy::Unlisted => "unlisted",
                Privacy::Private => "private",
            };
            let settings = crate::state::Settings {
                publish_privacy: Some(stored.to_string()),
                ..Default::default()
            };
            let raw = serde_json::to_string(&settings).expect("write");
            let read: crate::state::Settings = serde_json::from_str(&raw).expect("read");
            assert_eq!(read.publish_privacy.as_deref(), Some(stored));
        }
    }
}

pub struct SignInForm {
    pub error: Option<String>,
    pub entered_at: Instant,

    pub busy: bool,

    pub cancel: Arc<AtomicBool>,
}

impl SignInForm {
    pub fn new() -> Self {
        Self {
            error: None,
            entered_at: Instant::now(),
            busy: false,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn abandon(&self) {
        self.cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn can_submit(&self) -> bool {
        !self.busy
    }

    pub fn elapsed(&self) -> f32 {
        self.entered_at.elapsed().as_secs_f32()
    }

    pub fn is_animating(&self) -> bool {
        self.elapsed() < STEP_ANIMATION_SECONDS
    }

    pub fn blame(&mut self, failure: &Failure) {
        self.busy = false;
        self.error = Some(failure_message(failure));
    }
}

impl Default for SignInForm {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Scheduled {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

impl Scheduled {
    pub fn soon(now: i64) -> Self {
        let (year, month, day) = youtube::civil_from_unix(now + 3_600);
        let hour = ((now + 3_600).rem_euclid(86_400) / 3_600) as u32;
        Self {
            year: year as i32,
            month,
            day,
            hour,
            minute: 0,
        }
    }

    pub fn seconds(&self) -> i64 {
        unix_from_civil(self.year, self.month, self.day, self.hour, self.minute)
    }

    pub fn stamp(&self) -> String {
        crate::calendar::to_stamp(self.year, self.month, self.day, self.hour, self.minute)
    }

    pub fn label(&self) -> String {
        format!(
            "{:02}.{:02}.{:04} {:02}:{:02}",
            self.day, self.month, self.year, self.hour, self.minute
        )
    }

    pub fn with_month(self, year: i32, month: u32) -> Self {
        Self {
            year,
            month,
            day: crate::calendar::clamp_day(year, month, self.day),
            ..self
        }
    }
}

pub struct PublishForm {
    pub poster: Option<std::sync::Arc<gpui::RenderImage>>,
    pub preview_playing: bool,
    pub preview_position: f64,
    pub preview_duration: f64,

    pub preview_shown: Option<f64>,

    pub preview_stepped_at: std::time::Instant,
    pub source: PathBuf,
    pub title: TextField,
    pub description: TextField,
    pub tags: TextField,

    pub schedule: Option<Scheduled>,
    pub schedule_open: bool,
    pub category_id: String,
    pub privacy: Privacy,
    pub made_for_kids: bool,
    pub age_restricted: bool,
    pub notify_subscribers: bool,

    pub playlists: TextField,
    pub video_language: TextField,
    pub license: License,
    pub allow_embedding: bool,
    pub comments: Comments,
    pub show_like_count: bool,
    pub paid_promotion: bool,
    pub altered_content: bool,
    pub remix: Remix,

    pub more_open: bool,
    pub issues: Vec<Issue>,
}

impl PublishForm {
    pub fn new(cx: &mut gpui::App, source: PathBuf) -> Self {
        let stem = source
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_string();
        Self {
            poster: None,
            preview_playing: false,
            preview_position: 0.0,
            preview_duration: 0.0,
            preview_shown: None,
            preview_stepped_at: std::time::Instant::now(),
            source,
            title: TextField::new(cx, stem),
            description: TextField::new(cx, ""),
            tags: TextField::new(cx, ""),
            schedule: None,
            schedule_open: false,
            category_id: publish::DEFAULT_CATEGORY.to_string(),
            privacy: remembered_privacy(),
            made_for_kids: false,
            age_restricted: false,
            notify_subscribers: true,
            playlists: TextField::new(cx, ""),
            video_language: TextField::new(cx, ""),
            license: License::Standard,
            allow_embedding: true,
            comments: Comments::On,
            show_like_count: true,
            paid_promotion: false,
            altered_content: false,
            remix: Remix::VideoAndAudio,
            more_open: false,
            issues: Vec::new(),
        }
    }

    pub fn settings(&self) -> PublishSettings {
        let schedule = self
            .schedule
            .as_ref()
            .map(Scheduled::stamp)
            .unwrap_or_default();
        PublishSettings {
            title: self.title.buffer.text.clone(),
            description: self.description.buffer.text.clone(),
            tags: publish::parse_tags(&self.tags.buffer.text),
            category_id: self.category_id.clone(),
            privacy: self.privacy,
            made_for_kids: self.made_for_kids,
            age_restricted: self.age_restricted,
            publish_at: (!schedule.is_empty()).then_some(schedule),
            notify_subscribers: self.notify_subscribers,

            category_label: category_label(&self.category_id),
            thumbnail: None,

            playlists: publish::parse_tags(&self.playlists.buffer.text),
            video_language: self.video_language.buffer.text.trim().to_string(),
            license: self.license,
            allow_embedding: self.allow_embedding,
            comments: self.comments,
            show_like_count: self.show_like_count,
            paid_promotion: self.paid_promotion,
            altered_content: self.altered_content,
            remix: self.remix,
        }
    }

    pub fn validate(&mut self, now: i64) -> bool {
        self.issues = self.settings().validate(&youtube::iso_timestamp(now));
        self.issues.is_empty()
    }

    pub fn tags_used(&self) -> usize {
        publish::tags_length(&publish::parse_tags(&self.tags.buffer.text))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    From,
    To,
}

pub struct Filters {
    pub search: TextField,
    pub from: Option<Scheduled>,
    pub to: Option<Scheduled>,
    pub open: Option<Edge>,
    pub accounts: Vec<String>,
}

impl Filters {
    pub fn new(cx: &mut gpui::App) -> Self {
        Self {
            search: TextField::new(cx, ""),
            from: None,
            to: None,
            open: None,
            accounts: Vec::new(),
        }
    }

    pub fn edge(&self, edge: Edge) -> Option<Scheduled> {
        match edge {
            Edge::From => self.from,
            Edge::To => self.to,
        }
    }

    pub fn edge_mut(&mut self, edge: Edge) -> &mut Option<Scheduled> {
        match edge {
            Edge::From => &mut self.from,
            Edge::To => &mut self.to,
        }
    }

    pub fn toggle_picker(&mut self, edge: Edge, now: i64) {
        if self.open == Some(edge) {
            self.open = None;
            return;
        }
        self.open = Some(edge);
        if self.edge(edge).is_none() {
            let mut when = Scheduled::soon(now);
            when.minute = 0;
            if edge == Edge::From {
                when.hour = 0;
            } else {
                when.hour = 23;
                when.minute = 59;
            }
            *self.edge_mut(edge) = Some(when);
        }
    }

    pub fn as_filter(&self) -> HistoryFilter {
        HistoryFilter {
            search: self.search.buffer.text.clone(),
            from: self.from.map(|when| when.seconds()),
            to: self.to.map(|when| when.seconds()),
            accounts: self.accounts.clone(),
        }
    }

    pub fn toggle(&mut self, account_id: &str) {
        match self.accounts.iter().position(|id| id == account_id) {
            Some(index) => {
                self.accounts.remove(index);
            }
            None => self.accounts.push(account_id.to_string()),
        }
    }

    pub fn has(&self, account_id: &str) -> bool {
        self.accounts.iter().any(|id| id == account_id)
    }

    pub fn clear(&mut self) {
        self.search.buffer = TextBuffer::new("");
        self.from = None;
        self.to = None;
        self.open = None;
        self.accounts.clear();
    }

    pub fn is_active(&self) -> bool {
        !self.as_filter().is_empty()
    }
}

pub struct Youtube {
    pub directory: PathBuf,
    pub accounts: Accounts,
    pub history: History,
    pub queue: Queue,
    pub avatars: Avatars,
    pub filters: Option<Filters>,
    pub sign_in_form: Option<SignInForm>,
    pub form: Option<PublishForm>,

    pub pending_publish: Option<PathBuf>,

    pub choosing_account: bool,

    pub preview_bar: (f32, f32),
    pub sound: crate::preview_audio::Sound,
    pub volume_open: bool,
    pub volume_bar: (f32, f32),
    pub should_close: bool,
    pub dismissed: bool,

    pub running: Vec<Running>,

    pub session: Vec<youtube::HistoryEntry>,

    pub published: Option<String>,

    pub toasts: Vec<Toast>,
    pub copied_at: Option<Instant>,

    pub hovered_action: Option<String>,
    pub notice: Option<String>,
    pub signing_in: bool,
    pub refreshing: bool,
}

impl Default for Youtube {
    fn default() -> Self {
        let directory = youtube::data_directory();
        let accounts = Accounts::load(&directory);

        let mut avatars = Avatars::default();
        for account in &accounts.accounts {
            avatars.ensure_loaded(&directory, &account.id);
        }
        Self {
            accounts,
            history: History::load(&directory),

            queue: Queue::load(&directory),
            directory,
            avatars,
            filters: None,
            sign_in_form: None,
            form: None,
            pending_publish: None,
            choosing_account: false,
            preview_bar: (0.0, 0.0),
            sound: {
                let settings = crate::state::load_settings();
                let volume = settings.preview_volume.unwrap_or(1.0);
                crate::preview_audio::Sound {
                    volume,
                    muted: settings.preview_muted.unwrap_or(true),
                    restore: volume,
                }
            },
            volume_open: false,
            volume_bar: (0.0, 0.0),
            should_close: false,
            dismissed: false,
            running: Vec::new(),
            session: Vec::new(),
            published: None,
            toasts: Vec::new(),
            copied_at: None,
            hovered_action: None,
            notice: None,
            signing_in: false,
            refreshing: false,
        }
    }
}

fn expire_toasts(toasts: &mut Vec<Toast>) {
    toasts.retain(|toast| toast.born.elapsed() < TOAST_LIFE);
}

pub const TOAST_LIFE: std::time::Duration = std::time::Duration::from_secs(10);

pub struct Toast {
    pub title: String,
    pub url: String,
    pub born: Instant,

    pub copied_at: Option<Instant>,
}

impl Youtube {
    pub fn expire_toasts(&mut self) {
        expire_toasts(&mut self.toasts);
    }

    pub fn is_configured(&self) -> bool {
        has_browser()
    }

    pub fn forget(&mut self, account_id: &str) -> bool {
        let _ = youtube::session::forget_profile(&self.directory, account_id);
        let removed = self.accounts.remove(account_id);
        if removed {
            let _ = self.accounts.save(&self.directory);
        }
        removed
    }

    pub fn persist(&self) {
        let _ = self.accounts.save(&self.directory);
        let _ = self.history.save(&self.directory);
        let _ = self.queue.save(&self.directory);
    }

    pub fn save_queue(&self) {
        let _ = self.queue.save(&self.directory);
    }

    pub fn enqueue(
        &mut self,
        settings: PublishSettings,
        source: PathBuf,
        now: i64,
    ) -> Option<String> {
        if !self.is_configured() {
            self.notice = Some(cutix_i18n::t("youtube.error.noBrowser"));
            return None;
        }
        let Some(account_id) = self.accounts.active.clone() else {
            self.notice = Some(cutix_i18n::t("youtube.accounts.none"));
            return None;
        };
        let total = std::fs::metadata(&source)
            .map(|meta| meta.len())
            .unwrap_or(0);
        if total == 0 {
            self.notice = Some(cutix_i18n::t_args(
                "youtube.error.io",
                &[("detail", &source.display().to_string())],
            ));
            return None;
        }

        let id = self.queue.push(&account_id, settings, source, total, now);
        self.notice = Some(cutix_i18n::t("youtube.queue.added"));
        self.save_queue();
        Some(id)
    }

    pub fn sync_running(&mut self) -> bool {
        let mut changed = false;
        let snapshots: Vec<(String, Stage, u32)> = self
            .running
            .iter()
            .map(|running| {
                let (stage, percent) = running.snapshot();
                (running.task_id.clone(), stage, percent)
            })
            .collect();

        for (id, stage, percent) in snapshots {
            let before = self.queue.get(&id).map(|task| task.stage);
            self.queue.progress(&id, stage, percent);
            changed |= before != self.queue.get(&id).map(|task| task.stage);
        }

        if changed {
            self.save_queue();
        }
        changed
    }

    pub fn rows(&self) -> Vec<RowView> {
        let live: Vec<(String, Stage, f32)> = self
            .running
            .iter()
            .map(|running| {
                let (stage, percent) = running.snapshot();
                (running.task_id.clone(), stage, fraction_of(percent))
            })
            .collect();
        self.queue
            .visible()
            .into_iter()
            .map(|task| {
                let live = live
                    .iter()
                    .find(|(id, _, _)| *id == task.id)
                    .map(|(_, stage, fraction)| (*stage, *fraction));
                row_view(task, live)
            })
            .collect()
    }

    pub fn ensure_filters(&mut self, cx: &mut gpui::App) {
        if self.filters.is_none() {
            self.filters = Some(Filters::new(cx));
        }
    }

    pub fn open_publish(&mut self, source: PathBuf, cx: &mut gpui::App) -> bool {
        if !self.is_configured() {
            self.notice = Some(cutix_i18n::t("youtube.error.noBrowser"));
            return false;
        }
        if self.accounts.active().is_none() {
            self.notice = Some(cutix_i18n::t("youtube.accounts.none"));
            return false;
        }
        self.form = Some(PublishForm::new(cx, source));
        true
    }

    pub fn store_avatar(&mut self, account_id: &str, bytes: &[u8]) {
        let Some(image) = decode_avatar(bytes) else {
            return;
        };
        let path = Avatars::cache_path(&self.directory, account_id);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, bytes);
        self.avatars.store(account_id, image);
    }

    pub fn can_upload(&self) -> bool {
        self.is_configured() && self.accounts.active().is_some()
    }

    pub fn record(&mut self, entry: HistoryEntry, _now: i64) {
        self.history.upsert(entry);
        self.persist();
    }

    pub fn remove_account(&mut self, account_id: &str) {
        self.forget(account_id);
        self.persist();
        self.notice = Some(cutix_i18n::t("youtube.accounts.removed"));
    }

    pub fn filtered(&self) -> Vec<&HistoryEntry> {
        match self.filters.as_ref() {
            Some(filters) => self.history.filtered(&filters.as_filter()),
            None => self.history.entries.iter().collect(),
        }
    }

    pub fn copy_notice_visible(&self) -> bool {
        self.copied_at
            .is_some_and(|at| at.elapsed().as_secs_f32() < COPIED_NOTICE_SECONDS)
    }

    pub fn needs_repaint(&self) -> bool {
        self.queue.is_busy()
            || self.signing_in
            || self.copy_notice_visible()
            || self
                .sign_in_form
                .as_ref()
                .is_some_and(SignInForm::is_animating)
    }
}

pub fn account_avatar(
    colors: Palette,
    image: Option<Arc<gpui::RenderImage>>,
    initials: &str,
    size: f32,
) -> gpui::Div {
    use gpui::{div, img, px, rems, ParentElement, Styled};

    let base = div()
        .size(px(size))
        .flex_shrink_0()
        .rounded(px(size / 2.0))
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .bg(colors.accent);

    match image {
        Some(image) => base.child(img(image).size(px(size))),
        None => base
            .text_size(rems(0.7))
            .text_color(colors.muted_foreground)
            .child(initials.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use youtube::history::UploadStatus;

    #[test]
    fn the_pages_the_youtube_tab_links_to_can_all_be_opened() {
        assert!(is_openable_url(&publish::watch_url("abc")));
        assert!(is_openable_url(&publish::studio_url("abc")));
        assert!(is_openable_url(youtube::CHROME_DOWNLOAD_URL));
    }

    #[test]
    fn anything_that_is_not_an_allowed_https_google_page_is_refused() {
        assert!(!is_openable_url("http://console.cloud.google.com/"));
        assert!(!is_openable_url("https://evil.example/"));
        assert!(!is_openable_url("file:///C:/Windows/System32/calc.exe"));
        assert!(!is_openable_url(""));
        assert!(!is_openable_url(
            "https://console.cloud.google.com.evil.example/"
        ));
    }

    #[test]
    fn a_url_carrying_shell_metacharacters_is_refused_even_on_an_allowed_host() {
        assert!(!is_openable_url("https://www.youtube.com/watch?v=a&calc"));
        assert!(!is_openable_url("https://www.youtube.com/a b"));
        assert!(!is_openable_url("https://www.youtube.com/a\"b"));
        assert!(!is_openable_url("https://www.youtube.com/a|b"));
    }

    #[test]
    fn an_unfinished_sign_in_leaves_the_panel_ready_to_try_again() {
        let mut form = SignInForm::new();
        assert!(
            form.can_submit(),
            "nothing to fill in, so it is always ready"
        );

        form.busy = true;
        assert!(!form.can_submit(), "the window is already open");

        form.blame(&Failure::SignInAbandoned);
        assert!(!form.busy, "the wait is over either way");
        assert!(form.can_submit(), "and the button comes back");
        let message = form.error.clone().expect("a reason");
        assert!(!message.contains('{'), "{message}");
    }

    #[test]
    fn a_moment_on_the_calendar_maps_onto_the_same_second_the_crate_would_name() {
        assert_eq!(unix_from_civil(1970, 1, 1, 0, 0), 0);
        assert_eq!(unix_from_civil(1970, 1, 1, 0, 1), 60);
        assert_eq!(unix_from_civil(2023, 11, 14, 0, 0), 1_699_920_000);
        assert_eq!(unix_from_civil(2000, 2, 29, 12, 30), 951_782_400 + 45_000);
        for day in ["1970-01-01", "2001-09-09", "2024-02-29"] {
            let mut parts = day.split('-');
            let year: i32 = parts.next().unwrap().parse().unwrap();
            let month: u32 = parts.next().unwrap().parse().unwrap();
            let date: u32 = parts.next().unwrap().parse().unwrap();
            assert_eq!(
                youtube::iso_date(unix_from_civil(year, month, date, 13, 45)),
                day
            );
        }
    }

    #[test]
    fn the_step_transition_eases_out_and_settles() {
        assert_eq!(step_progress(0.0), 0.0);
        assert_eq!(step_progress(STEP_ANIMATION_SECONDS), 1.0);
        assert_eq!(step_progress(10.0), 1.0);
        assert!(step_progress(STEP_ANIMATION_SECONDS / 2.0) > 0.5);
    }

    #[test]
    fn an_announcement_goes_by_itself_once_its_ten_seconds_are_up() {
        let mut toasts = vec![
            Toast {
                title: "fresh".into(),
                url: "https://example.test/a".into(),
                born: Instant::now(),
                copied_at: None,
            },
            Toast {
                title: "stale".into(),
                url: "https://example.test/b".into(),
                born: Instant::now() - TOAST_LIFE - std::time::Duration::from_secs(1),
                copied_at: None,
            },
        ];

        expire_toasts(&mut toasts);

        assert_eq!(toasts.len(), 1);
        assert_eq!(toasts[0].title, "fresh");
    }

    #[test]
    fn the_upload_progress_fraction_is_bounded() {
        let mut job = UploadJob::default();
        assert_eq!(fraction_of(job.percent), 0.0);
        assert_eq!(job.stage, Stage::Opening);
        job.percent = 50;
        assert_eq!(fraction_of(job.percent), 0.5);

        job.percent = 500;
        assert_eq!(fraction_of(job.percent), 1.0);
    }

    #[test]
    fn an_avatar_is_read_from_its_cache_file_once_and_a_missing_one_is_not_retried() {
        let directory = std::env::temp_dir().join(format!(
            "cutix-yt-avatars-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);

        let mut avatars = Avatars::default();
        avatars.ensure_loaded(&directory, "chan-1");
        assert!(avatars.get("chan-1").is_none(), "nothing cached yet");
        assert_eq!(
            avatars.missing,
            vec!["chan-1".to_string()],
            "and it is not asked for twice"
        );

        let path = Avatars::cache_path(&directory, "chan-2");
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, b"not an image").expect("write");
        avatars.ensure_loaded(&directory, "chan-2");
        assert!(avatars.get("chan-2").is_none());
        assert!(avatars.missing.contains(&"chan-2".to_string()));

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_cached_avatar_path_never_escapes_its_directory() {
        let path = Avatars::cache_path(std::path::Path::new("C:/data"), "../../etc/passwd");
        assert_eq!(path.parent(), Some(std::path::Path::new("C:/data/avatars")));
        assert!(!path.to_string_lossy().contains(".."));
    }

    #[test]
    fn the_status_line_names_the_channel_and_says_so_when_there_is_none() {
        let mut accounts = Accounts::default();
        assert_eq!(
            account_line(&accounts),
            cutix_i18n::t("youtube.accounts.none")
        );

        accounts.upsert(youtube::Account {
            id: "UC_x5XG1OV2P6uZZ5FSM9Ttw".to_string(),
            title: "First Channel".to_string(),
            ..Default::default()
        });
        let line = account_line(&accounts);
        assert!(line.contains("First Channel"), "{line}");
        assert!(!line.contains('{'), "{line}");
    }

    #[test]
    fn every_failure_and_issue_resolves_to_a_real_translated_string() {
        let failures = [
            Failure::NoBrowser,
            Failure::BrowserLaunch("m".into()),
            Failure::Protocol("m".into()),
            Failure::SignInAbandoned,
            Failure::NoChannel,
            Failure::SignedOut,
            Failure::Timeout("m".into()),
            Failure::PageChanged("m".into()),
            Failure::Rejected("m".into()),
            Failure::Cancelled,
            Failure::Io("m".into()),
        ];
        for failure in failures {
            let message = failure_message(&failure);
            assert_ne!(message, failure.message_key(), "{failure:?}");
            assert!(!message.contains('{'), "{message}");
        }

        for issue in [
            Issue::TitleEmpty,
            Issue::TitleTooLong,
            Issue::TitleHasAngleBrackets,
            Issue::DescriptionTooLong,
            Issue::TagsTooLong,
            Issue::TooManyTags,
            Issue::UnknownCategory,
            Issue::ScheduleInThePast,
            Issue::ScheduleNotIso,
            Issue::KidsAndAgeRestricted,
        ] {
            assert_ne!(issue_message(&issue), issue.message_key(), "{issue:?}");
        }
    }

    #[test]
    fn every_stage_privacy_status_and_category_has_a_translation() {
        for stage in [
            Stage::Opening,
            Stage::Uploading,
            Stage::Processing,
            Stage::Publishing,
            Stage::Done,
        ] {
            assert_ne!(
                cutix_i18n::t(stage.message_key()),
                stage.message_key(),
                "{stage:?}"
            );
        }
        for privacy in Privacy::ALL {
            assert_ne!(cutix_i18n::t(privacy.message_key()), privacy.message_key());
        }
        for (id, _) in publish::CATEGORIES {
            assert_ne!(category_label(id), publish::category_key(id), "{id}");
        }
        for key in [
            "youtube.signIn.title",
            "youtube.signIn.body",
            "youtube.signIn.submit",
            "youtube.signIn.waiting",
            "youtube.signIn.waitingHint",
            "youtube.signIn.working",
            "youtube.signIn.installChrome",
            "youtube.signIn.getChrome",
            "youtube.accounts.added",
            "youtube.publish.limitHint",
        ] {
            assert_ne!(cutix_i18n::t(key), key, "{key}");
        }
        for status in [
            UploadStatus::Uploaded,
            UploadStatus::Processing,
            UploadStatus::Processed,
            UploadStatus::Rejected {
                reason: String::new(),
            },
            UploadStatus::Failed {
                reason: String::new(),
            },
            UploadStatus::Deleted,
        ] {
            assert_ne!(cutix_i18n::t(status.message_key()), status.message_key());
        }
    }

    fn task(title: &str) -> Task {
        Task::new(
            "q1".to_string(),
            "chan-1".to_string(),
            PublishSettings {
                title: title.to_string(),
                ..Default::default()
            },
            PathBuf::from("C:/videos/clip.mp4"),
            1_000,
            0,
        )
    }

    #[test]
    fn a_waiting_row_says_so_and_a_running_one_counts_up_in_whole_percent() {
        let mut task = task("Sunset drive");
        let waiting = row_view(&task, None);
        assert_eq!(waiting.status, cutix_i18n::t("youtube.queue.waiting"));
        assert_eq!(waiting.fraction, 0.0);
        assert!(waiting.show_bar && waiting.can_cancel && !waiting.can_retry);
        assert!(waiting.detail.is_none());

        task.state = TaskState::Running;
        let running = row_view(&task, Some((Stage::Uploading, 0.43)));
        assert!(running.status.contains("43"), "{}", running.status);
        assert!(!running.status.contains('{'));
        assert_eq!(running.fraction, 0.43);
        assert!(running.show_bar && running.can_cancel);

        let opening = row_view(&task, Some((Stage::Opening, 0.0)));
        assert_eq!(opening.status, cutix_i18n::t("youtube.stage.opening"));

        assert_eq!(
            row_view(&task, None).status,
            cutix_i18n::t("youtube.stage.opening")
        );

        let processing = row_view(&task, Some((Stage::Processing, 1.0)));
        assert_eq!(processing.status, cutix_i18n::t("youtube.stage.processing"));
    }

    #[test]
    fn a_failed_row_shows_the_real_reason_and_offers_a_retry_unless_youtube_refused_the_video() {
        let mut task = task("Boss fight");
        task.state = TaskState::Failed {
            note: Failure::Protocol("connection reset".to_string()).note(),
        };
        let dropped = row_view(&task, None);
        assert!(dropped.failed);
        assert!(!dropped.show_bar, "a stopped upload has no bar to fill");
        assert!(
            dropped.can_retry,
            "a dropped connection is worth another go"
        );
        let detail = dropped.detail.clone().expect("detail");
        assert!(detail.contains("connection reset"), "{detail}");
        assert!(!detail.contains('{'));

        task.state = TaskState::Failed {
            note: Failure::Rejected("This video is a duplicate".to_string()).note(),
        };
        let walled = row_view(&task, None);
        assert!(walled.failed);
        assert!(
            !walled.can_retry,
            "sending the same file again would only be refused again"
        );
        let detail = walled.detail.expect("detail");
        assert!(detail.contains("This video is a duplicate"), "{detail}");

        task.state = TaskState::Failed {
            note: Failure::SignedOut.note(),
        };
        assert!(
            row_view(&task, None).can_retry,
            "signing in again is the fix"
        );
    }

    #[test]
    fn a_cancelled_row_can_be_put_back_and_a_row_with_no_bar_can_be_dismissed() {
        let mut task = task("Clip");
        task.state = TaskState::Cancelled;
        let row = row_view(&task, None);
        assert_eq!(row.status, cutix_i18n::t("youtube.queue.cancelled"));
        assert!(!row.failed);
        assert!(row.can_retry);
        assert!(!row.show_bar && !row.can_cancel);
    }

    #[test]
    fn the_percentage_a_finished_row_shows_is_a_hundred() {
        let mut task = task("Clip");
        task.percent = 100;
        task.state = TaskState::Running;
        assert_eq!(row_view(&task, Some((Stage::Uploading, 1.0))).fraction, 1.0);
        assert_eq!(task.fraction(), 1.0);
    }

    #[test]
    fn every_string_the_queue_can_show_has_a_translation() {
        for key in [
            "youtube.queue.title",
            "youtube.queue.title.plain",
            "youtube.queue.background",
            "youtube.queue.behind",
            "youtube.queue.sequential",
            "youtube.queue.waiting",
            "youtube.queue.running",
            "youtube.queue.done",
            "youtube.queue.failed",
            "youtube.queue.cancelled",
            "youtube.queue.retry",
            "youtube.queue.added",
            "youtube.queue.finished",
            "settings.open",
        ] {
            assert_ne!(cutix_i18n::t(key), key, "{key}");
        }

        assert!(!cutix_i18n::t_args("youtube.queue.title", &[("count", "2")]).contains('{'));
        assert!(!cutix_i18n::t_args("youtube.queue.behind", &[("count", "2")]).contains('{'));
    }

    #[test]
    fn the_progress_label_interpolates_a_whole_percentage() {
        let job = UploadJob {
            stage: Stage::Uploading,
            percent: 51,
        };
        let label = stage_label(job.stage, fraction_of(job.percent));
        assert!(label.contains("51"), "{label}");
        assert!(!label.contains('{'), "{label}");

        let processing = stage_label(Stage::Processing, 0.0);
        assert!(!processing.contains('%'), "{processing}");
        assert!(!processing.contains('{'), "{processing}");
    }
}

#[path = "youtube_render.rs"]
pub mod render;
