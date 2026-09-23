use crate::clock::Zone;
use crate::failure::{Failure, FailureNote};
use crate::publish::PublishSettings;
use crate::studio::Stage;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const QUEUE_FILE: &str = "youtube-queue.json";
const MAX_TASKS: usize = 100;

/// The shape of the stored queue. A file without the field is from a build whose
/// `publish_at` stamps were local digits with a `Z` on the end (see
/// [`crate::utc_stamp_of_local_digits`]); from this format on they are UTC instants.
const STAMPS_ARE_UTC: u32 = 1;
const CURRENT_FORMAT: u32 = STAMPS_ARE_UTC;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "lowercase")]
pub enum TaskState {
    Waiting,

    Running,

    Done { video_id: String },

    Failed { note: FailureNote },

    Cancelled,
}

impl TaskState {
    pub fn message_key(&self) -> &'static str {
        match self {
            Self::Waiting => "youtube.queue.waiting",
            Self::Running => "youtube.queue.running",
            Self::Done { .. } => "youtube.queue.done",
            Self::Failed { .. } => "youtube.queue.failed",
            Self::Cancelled => "youtube.queue.cancelled",
        }
    }

    pub fn is_active(&self) -> bool {
        matches!(self, Self::Waiting | Self::Running)
    }

    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Failed { .. } | Self::Cancelled)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub account_id: String,
    pub settings: PublishSettings,
    pub source: PathBuf,

    #[serde(default)]
    pub total: u64,

    #[serde(default)]
    pub percent: u32,
    #[serde(default)]
    pub stage: Stage,
    pub queued_at: i64,
    #[serde(flatten)]
    pub state: TaskState,
}

impl Task {
    pub fn new(
        id: String,
        account_id: String,
        settings: PublishSettings,
        source: PathBuf,
        total: u64,
        queued_at: i64,
    ) -> Self {
        Self {
            id,
            account_id,
            settings,
            source,
            total,
            percent: 0,
            stage: Stage::Opening,
            queued_at,
            state: TaskState::Waiting,
        }
    }

    pub fn title(&self) -> &str {
        &self.settings.title
    }

    pub fn fraction(&self) -> f32 {
        (self.percent as f32 / 100.0).clamp(0.0, 1.0)
    }

    pub fn failure_note(&self) -> Option<&FailureNote> {
        match &self.state {
            TaskState::Failed { note } => Some(note),
            _ => None,
        }
    }

    pub fn source_is_ready(&self) -> bool {
        std::fs::metadata(&self.source)
            .map(|meta| meta.is_file() && (self.total == 0 || meta.len() == self.total))
            .unwrap_or(false)
    }
}

pub fn new_id(now: i64, counter: u64) -> String {
    format!("q{now:x}-{counter:x}")
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queue {
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    counter: u64,
    /// Missing from a file means format 0; a queue made in memory is always current.
    #[serde(default)]
    format: u32,
}

impl Default for Queue {
    fn default() -> Self {
        Self {
            tasks: Vec::new(),
            counter: 0,
            format: CURRENT_FORMAT,
        }
    }
}

impl Queue {
    pub fn load(directory: &Path) -> Self {
        Self::load_in(directory, Zone::Local)
    }

    /// Loads the stored queue, reading any legacy schedule on the clock in `zone` —
    /// the machine's own outside tests, since that is the clock those digits were
    /// picked on.
    pub fn load_in(directory: &Path, zone: Zone) -> Self {
        let Ok(text) = std::fs::read_to_string(directory.join(QUEUE_FILE)) else {
            return Self::default();
        };
        let mut queue: Self = serde_json::from_str(&text).unwrap_or_default();
        queue.upgrade_format(zone);
        queue.reclaim_interrupted();
        queue
    }

    /// Brings a queue written by an earlier build up to the current format.
    ///
    /// Only a stamp that parses is converted; anything else is left for `validate` to
    /// refuse as it always did.
    fn upgrade_format(&mut self, zone: Zone) {
        if self.format < STAMPS_ARE_UTC {
            for task in &mut self.tasks {
                if let Some(stamp) = task.settings.publish_at.as_deref()
                    && let Some(converted) = crate::utc_stamp_of_local_digits(stamp, zone)
                {
                    task.settings.publish_at = Some(converted);
                }
            }
        }
        self.format = CURRENT_FORMAT;
    }

    pub fn save(&self, directory: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(directory)?;
        let text = serde_json::to_string_pretty(self)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        std::fs::write(directory.join(QUEUE_FILE), text)
    }

    /// Settles a task the last run left marked as uploading.
    ///
    /// It goes to failed-but-retryable rather than straight back to waiting: the app may
    /// have gone down after Studio already had the video, as a draft or even published,
    /// and a silent restart would upload it a second time. The person checks and presses
    /// retry.
    pub fn reclaim_interrupted(&mut self) {
        for task in &mut self.tasks {
            if task.state == TaskState::Running {
                task.state = TaskState::Failed {
                    note: FailureNote::interrupted(),
                };
                task.percent = 0;
                task.stage = Stage::Opening;
            }
        }
    }

    pub fn push(
        &mut self,
        account_id: &str,
        settings: PublishSettings,
        source: PathBuf,
        total: u64,
        now: i64,
    ) -> String {
        self.counter += 1;
        let id = new_id(now, self.counter);
        self.tasks.push(Task::new(
            id.clone(),
            account_id.to_string(),
            settings,
            source,
            total,
            now,
        ));

        while self.tasks.len() > MAX_TASKS {
            match self.tasks.iter().position(|task| !task.state.is_active()) {
                Some(index) => {
                    self.tasks.remove(index);
                }
                None => break,
            }
        }
        id
    }

    pub fn get(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|task| task.id == id)
    }

    pub fn remove(&mut self, id: &str) -> bool {
        let before = self.tasks.len();
        self.tasks.retain(|task| task.id != id);
        before != self.tasks.len()
    }

    pub fn running(&self) -> Option<&Task> {
        self.tasks
            .iter()
            .find(|task| task.state == TaskState::Running)
    }

    pub fn next_waiting(&self) -> Option<&Task> {
        if self.running().is_some() {
            return None;
        }
        self.tasks
            .iter()
            .filter(|task| task.state == TaskState::Waiting)
            .min_by_key(|task| task.queued_at)
    }

    pub fn start(&mut self, id: &str) -> bool {
        match self.get_mut(id) {
            Some(task) => {
                task.state = TaskState::Running;
                true
            }
            None => false,
        }
    }

    pub fn fail(&mut self, id: &str, failure: &Failure) -> bool {
        match self.get_mut(id) {
            Some(task) => {
                task.state = TaskState::Failed {
                    note: failure.note(),
                };
                true
            }
            None => false,
        }
    }

    pub fn finish(&mut self, id: &str, video_id: &str) -> bool {
        match self.get_mut(id) {
            Some(task) => {
                task.percent = 100;
                task.stage = Stage::Done;
                task.state = TaskState::Done {
                    video_id: video_id.to_string(),
                };
                true
            }
            None => false,
        }
    }

    pub fn cancel(&mut self, id: &str) -> bool {
        match self.get_mut(id) {
            Some(task) => {
                task.state = TaskState::Cancelled;
                true
            }
            None => false,
        }
    }

    pub fn retry(&mut self, id: &str) -> bool {
        match self.get_mut(id) {
            Some(task) if task.state.is_retryable() => {
                task.state = TaskState::Waiting;
                task.percent = 0;
                task.stage = Stage::Opening;
                true
            }
            _ => false,
        }
    }

    pub fn progress(&mut self, id: &str, stage: Stage, percent: u32) -> bool {
        match self.get_mut(id) {
            Some(task) => {
                if stage > task.stage {
                    task.stage = stage;
                    task.percent = percent.min(100);
                } else if stage == task.stage {
                    task.percent = task.percent.max(percent.min(100));
                }
                true
            }
            None => false,
        }
    }

    pub fn active(&self) -> Vec<&Task> {
        self.tasks
            .iter()
            .filter(|task| task.state.is_active())
            .collect()
    }

    pub fn visible(&self) -> Vec<&Task> {
        let mut tasks: Vec<&Task> = self
            .tasks
            .iter()
            .filter(|task| !matches!(task.state, TaskState::Done { .. }))
            .collect();
        tasks.sort_by_key(|task| task.queued_at);
        tasks
    }

    pub fn is_busy(&self) -> bool {
        self.tasks.iter().any(|task| task.state.is_active())
    }

    pub fn waiting_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|task| task.state == TaskState::Waiting)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publish::Privacy;

    fn settings(title: &str) -> PublishSettings {
        PublishSettings {
            title: title.to_string(),
            privacy: Privacy::Private,
            ..Default::default()
        }
    }

    fn queue_with(count: usize) -> Queue {
        let mut queue = Queue::default();
        for index in 0..count {
            queue.push(
                "chan-1",
                settings(&format!("clip {index}")),
                PathBuf::from(format!("C:/videos/clip{index}.mp4")),
                1_000,
                1_000 + index as i64,
            );
        }
        queue
    }

    fn temp_dir(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "cutix-yt-queue-{}-{name}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        directory
    }

    #[test]
    fn a_queued_task_starts_waiting_at_zero_percent() {
        let queue = queue_with(1);
        let task = &queue.tasks[0];
        assert_eq!(task.state, TaskState::Waiting);
        assert_eq!(task.percent, 0);
        assert_eq!(task.stage, Stage::Opening);
        assert_eq!(task.fraction(), 0.0);
        assert_eq!(task.title(), "clip 0");
    }

    #[test]
    fn every_task_gets_an_id_of_its_own() {
        let queue = queue_with(3);
        let mut ids: Vec<&str> = queue.tasks.iter().map(|task| task.id.as_str()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn only_one_task_runs_at_a_time_and_the_oldest_goes_first() {
        let mut queue = queue_with(3);
        let first = queue.next_waiting().expect("next").id.clone();
        assert_eq!(queue.get(&first).map(|task| task.queued_at), Some(1_000));

        assert!(queue.start(&first));
        assert_eq!(
            queue.running().map(|task| task.id.clone()),
            Some(first.clone())
        );

        assert!(queue.next_waiting().is_none());
        assert_eq!(queue.waiting_count(), 2);

        queue.finish(&first, "vid-1");
        let second = queue.next_waiting().expect("next").id.clone();
        assert_ne!(second, first);
        assert_eq!(queue.get(&second).map(|task| task.queued_at), Some(1_001));
    }

    #[test]
    fn one_failure_stops_that_task_only_and_the_queue_carries_on() {
        let mut queue = queue_with(2);
        let first = queue.next_waiting().expect("next").id.clone();
        queue.start(&first);
        queue.fail(&first, &Failure::Rejected("duplicate video".to_string()));

        let note = queue
            .get(&first)
            .and_then(Task::failure_note)
            .expect("note");
        assert_eq!(note.key, "youtube.error.rejected");
        assert_eq!(note.detail, "duplicate video");
        assert!(!note.retryable);
        assert!(
            !note.worth_retrying(),
            "youtube will refuse the same file again"
        );

        assert!(queue.next_waiting().is_some());
        assert_eq!(queue.waiting_count(), 1);
    }

    #[test]
    fn a_failed_task_can_be_retried_and_a_finished_one_cannot() {
        let mut queue = queue_with(1);
        let id = queue.tasks[0].id.clone();
        queue.start(&id);
        queue.progress(&id, Stage::Uploading, 60);
        queue.fail(&id, &Failure::Protocol("dropped".to_string()));

        assert!(queue.retry(&id));
        let task = queue.get(&id).expect("task");
        assert_eq!(task.state, TaskState::Waiting);
        assert_eq!(
            task.percent, 0,
            "a browser upload starts over, it does not resume"
        );

        queue.start(&id);
        queue.finish(&id, "vid-9");
        assert!(!queue.retry(&id));
        assert_eq!(
            queue.get(&id).map(|task| task.state.clone()),
            Some(TaskState::Done {
                video_id: "vid-9".to_string()
            })
        );

        assert!(queue.visible().is_empty());
    }

    #[test]
    fn a_cancelled_task_stays_visible_until_it_is_removed_and_can_be_put_back() {
        let mut queue = queue_with(1);
        let id = queue.tasks[0].id.clone();
        queue.start(&id);
        assert!(queue.cancel(&id));
        assert_eq!(queue.visible().len(), 1);
        assert!(!queue.is_busy());

        assert!(queue.retry(&id));
        assert!(queue.is_busy());

        queue.cancel(&id);
        assert!(queue.remove(&id));
        assert!(!queue.remove(&id));
        assert!(queue.tasks.is_empty());
    }

    #[test]
    fn progress_moves_forward_through_the_stages_and_never_back() {
        let mut queue = queue_with(1);
        let id = queue.tasks[0].id.clone();

        assert!(queue.progress(&id, Stage::Uploading, 25));
        assert_eq!(queue.get(&id).map(|task| task.fraction()), Some(0.25));

        queue.progress(&id, Stage::Uploading, 10);
        assert_eq!(queue.get(&id).map(|task| task.percent), Some(25));

        queue.progress(&id, Stage::Processing, 0);
        let task = queue.get(&id).expect("task");
        assert_eq!(task.stage, Stage::Processing);
        assert_eq!(task.percent, 0);

        queue.progress(&id, Stage::Uploading, 99);
        assert_eq!(
            queue.get(&id).map(|task| task.stage),
            Some(Stage::Processing)
        );
        assert_eq!(queue.get(&id).map(|task| task.percent), Some(0));

        queue.finish(&id, "vid");
        assert_eq!(queue.get(&id).map(|task| task.percent), Some(100));
    }

    #[test]
    fn a_percentage_beyond_a_hundred_never_reaches_a_progress_bar() {
        let mut queue = queue_with(1);
        let id = queue.tasks[0].id.clone();
        queue.progress(&id, Stage::Uploading, 4_000);
        assert_eq!(queue.get(&id).map(|task| task.percent), Some(100));
        assert_eq!(queue.get(&id).map(|task| task.fraction()), Some(1.0));
    }

    #[test]
    fn an_upload_interrupted_by_a_restart_waits_for_the_person_before_it_goes_again() {
        let directory = temp_dir("restart");
        let mut queue = queue_with(1);
        let id = queue.tasks[0].id.clone();
        queue.start(&id);
        queue.progress(&id, Stage::Uploading, 64);
        queue.save(&directory).expect("save");

        let text = std::fs::read_to_string(directory.join(QUEUE_FILE)).expect("read");
        assert!(text.contains("running"));

        let mut reloaded = Queue::load(&directory);
        let task = reloaded.get(&id).expect("task");
        let note = task.failure_note().expect("it is marked as stopped");
        assert!(note.worth_retrying(), "one press puts it back");
        assert!(!note.auth && !note.cancelled);
        assert_eq!(task.percent, 0, "the browser that held those bytes is gone");
        assert_eq!(reloaded.running(), None, "no ghost uploading row survives");
        assert!(
            reloaded.next_waiting().is_none(),
            "Studio may already have the video, so nothing starts it again unasked"
        );

        assert!(reloaded.retry(&id));
        assert_eq!(
            reloaded.next_waiting().map(|task| task.id.clone()),
            Some(id)
        );

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_task_whose_file_changed_or_vanished_says_so_before_a_browser_is_started() {
        let directory = temp_dir("source");
        std::fs::create_dir_all(&directory).expect("mkdir");
        let source = directory.join("clip.mp4");
        std::fs::write(&source, vec![7u8; 1_000]).expect("write");

        let task = Task::new(
            "q1".to_string(),
            "chan".to_string(),
            settings("x"),
            source.clone(),
            1_000,
            0,
        );
        assert!(task.source_is_ready());

        std::fs::write(&source, vec![7u8; 2_000]).expect("rewrite");
        assert!(!task.source_is_ready());

        std::fs::remove_file(&source).expect("remove");
        assert!(!task.source_is_ready());

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn the_queue_round_trips_through_disk_and_a_missing_or_broken_file_is_empty() {
        let directory = temp_dir("roundtrip");
        assert_eq!(Queue::load(&directory), Queue::default());

        let queue = queue_with(2);
        queue.save(&directory).expect("save");
        assert_eq!(Queue::load(&directory), queue);

        std::fs::write(directory.join(QUEUE_FILE), "{ not json").expect("corrupt");
        assert_eq!(Queue::load(&directory), Queue::default());

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_queue_written_by_the_old_resumable_build_still_loads() {
        let directory = temp_dir("legacy");
        std::fs::create_dir_all(&directory).expect("mkdir");
        let legacy = r#"{"tasks":[{"id":"q1","account_id":"chan","settings":{"title":"old clip",
            "description":"","tags":[],"category_id":"22","privacy":"private","made_for_kids":false,
            "age_restricted":false,"publish_at":null,"notify_subscribers":true},
            "source":"C:/videos/old.mp4","total":2048,"sent":1024,
            "session":"https://upload.example/s/1","queued_at":42,"state":"waiting"}],"counter":1}"#;
        std::fs::write(directory.join(QUEUE_FILE), legacy).expect("write");

        let queue = Queue::load(&directory);
        let task = queue.get("q1").expect("the row survives");
        assert_eq!(task.title(), "old clip");
        assert_eq!(task.total, 2_048);
        assert_eq!(task.percent, 0, "the old byte offset means nothing now");
        assert_eq!(task.state, TaskState::Waiting);

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_schedule_queued_by_the_previous_build_is_read_on_the_clock_it_was_picked_on() {
        let directory = temp_dir("legacy-schedule");
        std::fs::create_dir_all(&directory).expect("mkdir");
        // 15:00 picked in Moscow, stored by the old build as if it were UTC — and one
        // waiting task with no schedule, plus one already stopped that will be retried.
        let legacy = r#"{"tasks":[
            {"id":"q1","account_id":"chan","settings":{"title":"scheduled","description":"",
             "tags":[],"category_id":"22","privacy":"private","made_for_kids":false,
             "age_restricted":false,"publish_at":"2026-09-23T15:00:00Z","notify_subscribers":true},
             "source":"C:/videos/a.mp4","total":10,"queued_at":1,"state":"waiting"},
            {"id":"q2","account_id":"chan","settings":{"title":"plain","description":"",
             "tags":[],"category_id":"22","privacy":"private","made_for_kids":false,
             "age_restricted":false,"publish_at":null,"notify_subscribers":true},
             "source":"C:/videos/b.mp4","total":10,"queued_at":2,"state":"waiting"},
            {"id":"q3","account_id":"chan","settings":{"title":"interrupted","description":"",
             "tags":[],"category_id":"22","privacy":"private","made_for_kids":false,
             "age_restricted":false,"publish_at":"2026-09-24T01:30:00Z","notify_subscribers":true},
             "source":"C:/videos/c.mp4","total":10,"queued_at":3,"state":"running"}
        ],"counter":3}"#;
        std::fs::write(directory.join(QUEUE_FILE), legacy).expect("write");

        let moscow = Zone::Fixed(3 * 3_600);
        let queue = Queue::load_in(&directory, moscow);
        assert_eq!(
            queue
                .get("q1")
                .and_then(|task| task.settings.publish_at.as_deref()),
            Some("2026-09-23T12:00:00Z"),
            "the instant 15:00 Moscow names, so Studio is typed 3:00 PM again"
        );
        assert_eq!(
            queue
                .get("q2")
                .and_then(|task| task.settings.publish_at.as_deref()),
            None
        );
        assert_eq!(
            queue
                .get("q3")
                .and_then(|task| task.settings.publish_at.as_deref()),
            Some("2026-09-23T22:30:00Z"),
            "a stopped row is converted too: it may be retried"
        );

        // Saved by this build, the file says so, and a reload converts nothing twice.
        queue.save(&directory).expect("save");
        let text = std::fs::read_to_string(directory.join(QUEUE_FILE)).expect("read");
        assert!(text.contains("\"format\": 1"));
        let reloaded = Queue::load_in(&directory, moscow);
        assert_eq!(
            reloaded
                .get("q1")
                .and_then(|task| task.settings.publish_at.as_deref()),
            Some("2026-09-23T12:00:00Z")
        );

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_queue_made_in_this_build_is_current_and_its_schedules_are_kept_as_they_are() {
        let directory = temp_dir("current-schedule");
        let mut queue = Queue::default();
        let mut scheduled = settings("later");
        scheduled.publish_at = Some("2030-01-02T15:04:05Z".to_string());
        let id = queue.push("chan", scheduled, PathBuf::from("C:/x.mp4"), 10, 1);
        queue.save(&directory).expect("save");

        let reloaded = Queue::load_in(&directory, Zone::Fixed(3 * 3_600));
        assert_eq!(
            reloaded
                .get(&id)
                .and_then(|task| task.settings.publish_at.as_deref()),
            Some("2030-01-02T15:04:05Z"),
            "a stamp this build wrote is already the instant"
        );

        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn the_stored_queue_never_grows_without_bound_and_never_drops_a_live_upload() {
        let mut queue = Queue::default();
        for index in 0..MAX_TASKS {
            let id = queue.push(
                "chan",
                settings("old"),
                PathBuf::from("C:/x.mp4"),
                10,
                index as i64,
            );
            queue.start(&id);
            queue.finish(&id, "vid");
        }
        assert_eq!(queue.tasks.len(), MAX_TASKS);
        queue.push(
            "chan",
            settings("new"),
            PathBuf::from("C:/new.mp4"),
            10,
            9_999,
        );
        assert_eq!(queue.tasks.len(), MAX_TASKS);
        assert_eq!(queue.tasks.last().map(|task| task.title()), Some("new"));

        let mut busy = Queue::default();
        for index in 0..(MAX_TASKS + 5) {
            busy.push(
                "chan",
                settings("live"),
                PathBuf::from("C:/x.mp4"),
                10,
                index as i64,
            );
        }
        assert_eq!(busy.tasks.len(), MAX_TASKS + 5);
        assert_eq!(busy.active().len(), MAX_TASKS + 5);
    }

    #[test]
    fn every_task_state_has_a_translation_key_of_its_own() {
        let states = [
            TaskState::Waiting,
            TaskState::Running,
            TaskState::Done {
                video_id: "v".to_string(),
            },
            TaskState::Failed {
                note: Failure::Protocol("x".to_string()).note(),
            },
            TaskState::Cancelled,
        ];
        let mut keys: Vec<&str> = states.iter().map(TaskState::message_key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), states.len());
        assert!(keys.iter().all(|key| key.starts_with("youtube.queue.")));
    }
}
