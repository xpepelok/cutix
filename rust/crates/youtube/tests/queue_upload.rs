use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use youtube::failure::Failure;
use youtube::publish::{Privacy, PublishSettings};
use youtube::queue::{Queue, Task, TaskState};
use youtube::studio::{Control, Stage};

fn settings(title: &str) -> PublishSettings {
    PublishSettings {
        title: title.to_string(),
        privacy: Privacy::Private,
        ..Default::default()
    }
}

fn workspace(name: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "cutix-yt-runner-{}-{name}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("workspace");
    directory
}

fn video(directory: &std::path::Path, name: &str, bytes: usize) -> PathBuf {
    let path = directory.join(name);
    std::fs::write(&path, vec![9u8; bytes]).expect("video");
    path
}

enum Script {
    Succeeds(Vec<(Stage, u32)>, &'static str),

    Fails(Vec<(Stage, u32)>, Failure),
}

fn run_next(
    queue: &mut Queue,
    script: Script,
    cancel: &Arc<AtomicBool>,
) -> Option<Result<String, Failure>> {
    let id = queue.next_waiting()?.id.clone();
    queue.start(&id);

    let (steps, outcome) = match script {
        Script::Succeeds(steps, video_id) => (steps, Ok(video_id.to_string())),
        Script::Fails(steps, failure) => (steps, Err(failure)),
    };

    for (stage, percent) in steps {
        let control = if cancel.load(Ordering::Relaxed) {
            Control::Cancel
        } else {
            Control::Continue
        };
        if control == Control::Cancel {
            queue.cancel(&id);
            return Some(Err(Failure::Cancelled));
        }
        queue.progress(&id, stage, percent);
    }

    match &outcome {
        Ok(video_id) => {
            queue.finish(&id, video_id);
        }
        Err(failure) => {
            queue.fail(&id, failure);
        }
    }
    Some(outcome)
}

fn queued(queue: &mut Queue, directory: &std::path::Path, title: &str, at: i64) -> String {
    let source = video(directory, &format!("{title}.mp4"), 4_096);
    queue.push("UCchan", settings(title), source, 4_096, at)
}

#[test]
fn a_finished_upload_reports_its_stages_in_order_and_lands_on_the_video_id() {
    let directory = workspace("happy");
    let mut queue = Queue::default();
    let id = queued(&mut queue, &directory, "clip", 1_000);
    let cancel = Arc::new(AtomicBool::new(false));

    let result = run_next(
        &mut queue,
        Script::Succeeds(
            vec![
                (Stage::Opening, 0),
                (Stage::Uploading, 20),
                (Stage::Uploading, 80),
                (Stage::Processing, 0),
                (Stage::Publishing, 0),
            ],
            "dQw4w9WgXcQ",
        ),
        &cancel,
    )
    .expect("a task was waiting");

    assert_eq!(result.as_deref(), Ok("dQw4w9WgXcQ"));
    let task = queue.get(&id).expect("task");
    assert_eq!(
        task.state,
        TaskState::Done {
            video_id: "dQw4w9WgXcQ".to_string()
        }
    );
    assert_eq!(task.percent, 100);
    assert_eq!(task.stage, Stage::Done);
    assert!(
        queue.visible().is_empty(),
        "a finished row belongs to the history"
    );

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn progress_climbs_through_the_percentages_studio_reports_and_never_slips_back() {
    let directory = workspace("progress");
    let mut queue = Queue::default();
    let id = queued(&mut queue, &directory, "clip", 1_000);
    queue.start(&id);

    let mut seen = Vec::new();
    for percent in [0, 5, 5, 40, 39, 99] {
        queue.progress(&id, Stage::Uploading, percent);
        seen.push(queue.get(&id).expect("task").percent);
    }
    assert_eq!(
        seen,
        vec![0, 5, 5, 40, 40, 99],
        "39 after 40 is a redraw, not a rewind"
    );

    queue.progress(&id, Stage::Processing, 0);
    let task = queue.get(&id).expect("task");
    assert_eq!((task.stage, task.percent), (Stage::Processing, 0));
    assert_eq!(task.fraction(), 0.0);

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_cancelled_upload_stops_where_it_was_and_leaves_a_row_that_can_be_put_back() {
    let directory = workspace("cancel");
    let mut queue = Queue::default();
    let id = queued(&mut queue, &directory, "clip", 1_000);
    let cancel = Arc::new(AtomicBool::new(true));

    let result = run_next(
        &mut queue,
        Script::Succeeds(vec![(Stage::Uploading, 10)], "never-used"),
        &cancel,
    )
    .expect("a task was waiting");

    assert_eq!(result, Err(Failure::Cancelled));
    assert_eq!(
        queue.get(&id).map(|task| task.state.clone()),
        Some(TaskState::Cancelled)
    );
    assert_eq!(
        queue.visible().len(),
        1,
        "the owner can still see what they stopped"
    );
    assert!(!queue.is_busy());

    assert!(queue.retry(&id));
    assert_eq!(queue.get(&id).map(|task| task.percent), Some(0));

    cancel.store(false, Ordering::Relaxed);
    let second = run_next(
        &mut queue,
        Script::Succeeds(vec![(Stage::Uploading, 100)], "abcdefgh12"),
        &cancel,
    )
    .expect("the retry runs");
    assert_eq!(second.as_deref(), Ok("abcdefgh12"));

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_dropped_devtools_connection_is_worth_retrying_and_the_retry_starts_over() {
    let directory = workspace("retry");
    let mut queue = Queue::default();
    let id = queued(&mut queue, &directory, "clip", 1_000);
    let cancel = Arc::new(AtomicBool::new(false));

    run_next(
        &mut queue,
        Script::Fails(
            vec![(Stage::Uploading, 70)],
            Failure::Protocol("the browser closed the connection".to_string()),
        ),
        &cancel,
    )
    .expect("a task was waiting")
    .expect_err("it failed");

    let note = queue
        .get(&id)
        .and_then(Task::failure_note)
        .expect("note")
        .clone();
    assert_eq!(note.key, "youtube.error.protocol");
    assert!(note.retryable && note.worth_retrying());
    assert!(!note.auth);

    assert!(queue.retry(&id));
    assert_eq!(
        queue.get(&id).map(|task| (task.stage, task.percent)),
        Some((Stage::Opening, 0)),
        "the browser held those bytes and the browser is gone"
    );

    let result = run_next(
        &mut queue,
        Script::Succeeds(vec![(Stage::Uploading, 100)], "secondtry1"),
        &cancel,
    )
    .expect("the retry runs");
    assert_eq!(result.as_deref(), Ok("secondtry1"));

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_video_youtube_refuses_says_so_in_its_own_row_and_the_queue_moves_on() {
    let directory = workspace("rejected");
    let mut queue = Queue::default();
    let first = queued(&mut queue, &directory, "duplicate", 1_000);
    let second = queued(&mut queue, &directory, "fine", 1_001);
    let cancel = Arc::new(AtomicBool::new(false));

    run_next(
        &mut queue,
        Script::Fails(
            vec![(Stage::Uploading, 100)],
            Failure::Rejected("This video is a duplicate".to_string()),
        ),
        &cancel,
    )
    .expect("first")
    .expect_err("refused");

    let note = queue
        .get(&first)
        .and_then(Task::failure_note)
        .expect("note");
    assert_eq!(note.detail, "This video is a duplicate");
    assert!(
        !note.worth_retrying(),
        "the same file would be refused again"
    );

    assert_eq!(
        queue.next_waiting().map(|task| task.id.clone()),
        Some(second.clone())
    );
    let result = run_next(
        &mut queue,
        Script::Succeeds(vec![(Stage::Uploading, 100)], "goodclip01"),
        &cancel,
    )
    .expect("second");
    assert_eq!(result.as_deref(), Ok("goodclip01"));

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_session_google_has_ended_is_reported_as_one_and_signing_in_again_is_the_offered_fix() {
    let directory = workspace("signedout");
    let mut queue = Queue::default();
    let id = queued(&mut queue, &directory, "clip", 1_000);
    let cancel = Arc::new(AtomicBool::new(false));

    run_next(
        &mut queue,
        Script::Fails(vec![], Failure::SignedOut),
        &cancel,
    )
    .expect("a task was waiting")
    .expect_err("signed out");

    let note = queue.get(&id).and_then(Task::failure_note).expect("note");
    assert_eq!(note.key, "youtube.error.signedOut");
    assert!(note.auth);
    assert!(note.worth_retrying(), "signing in again is exactly the fix");

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_video_youtube_will_not_take_stops_without_pretending_a_retry_would_help() {
    let directory = workspace("challenge");
    let mut queue = Queue::default();
    let id = queued(&mut queue, &directory, "clip", 1_000);
    let cancel = Arc::new(AtomicBool::new(false));

    run_next(
        &mut queue,
        Script::Fails(vec![], Failure::PageChanged("#next-button".to_string())),
        &cancel,
    )
    .expect("a task was waiting")
    .expect_err("studio moved a control");

    let note = queue.get(&id).and_then(Task::failure_note).expect("note");
    assert_eq!(note.key, "youtube.error.pageChanged");
    assert_eq!(
        note.detail, "#next-button",
        "the row names what went missing"
    );
    assert!(!note.retryable && !note.auth);
    assert!(
        !note.worth_retrying(),
        "the same page would be just as changed"
    );

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn an_upload_killed_by_a_restart_comes_back_waiting_and_runs_from_the_beginning() {
    let directory = workspace("restart");
    let mut queue = Queue::default();
    let id = queued(&mut queue, &directory, "clip", 1_000);

    queue.start(&id);
    queue.progress(&id, Stage::Uploading, 55);
    queue.save(&directory).expect("save");

    let mut reloaded = Queue::load(&directory);
    let task = reloaded.get(&id).expect("task");
    assert_eq!(task.state, TaskState::Waiting);
    assert_eq!((task.stage, task.percent), (Stage::Opening, 0));
    assert_eq!(reloaded.running(), None);

    let cancel = Arc::new(AtomicBool::new(false));
    let result = run_next(
        &mut reloaded,
        Script::Succeeds(vec![(Stage::Uploading, 100)], "afterboot1"),
        &cancel,
    )
    .expect("it runs again");
    assert_eq!(result.as_deref(), Ok("afterboot1"));

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_row_whose_export_was_overwritten_is_caught_before_a_browser_is_started_for_it() {
    let directory = workspace("stale");
    let mut queue = Queue::default();
    let id = queued(&mut queue, &directory, "clip", 1_000);

    assert!(queue.get(&id).expect("task").source_is_ready());
    std::fs::write(directory.join("clip.mp4"), vec![9u8; 8_192]).expect("re-export");
    assert!(!queue.get(&id).expect("task").source_is_ready());

    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn the_queue_runs_its_tasks_one_at_a_time_in_the_order_they_were_added() {
    let directory = workspace("order");
    let mut queue = Queue::default();
    for (index, title) in ["first", "second", "third"].iter().enumerate() {
        queued(&mut queue, &directory, title, 1_000 + index as i64);
    }
    let cancel = Arc::new(AtomicBool::new(false));

    let mut order = Vec::new();
    while let Some(next) = queue.next_waiting().map(|task| task.title().to_string()) {
        order.push(next);
        run_next(
            &mut queue,
            Script::Succeeds(vec![(Stage::Uploading, 100)], "someid1234"),
            &cancel,
        );
    }
    assert_eq!(order, ["first", "second", "third"]);
    assert!(!queue.is_busy());

    let _ = std::fs::remove_dir_all(&directory);
}
