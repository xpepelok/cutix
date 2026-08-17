use std::io::Write;
use youtube::queue::{Queue, TaskState};
use youtube::studio::{Control, Stage};

fn accounts_active(directory: &std::path::Path) -> String {
    youtube::Accounts::load(directory)
        .active()
        .map(|account| account.id.clone())
        .unwrap_or_default()
}

fn main() {
    let directory = youtube::data_directory();
    println!("data directory: {}", directory.display());

    let queue = Queue::load(&directory);
    let queued = queue
        .tasks
        .iter()
        .find(|task| !matches!(task.state, TaskState::Done { .. }))
        .cloned();
    let task = match queued {
        Some(task) => task,
        None => {
            println!("nothing in the queue; falling back to a private test upload");
            let source =
                std::path::PathBuf::from(std::env::var("CUTIX_TEST_VIDEO").unwrap_or_else(|_| {
                    String::from(r"C:\Users\ksana\Videos\cutix\New project.mp4")
                }));
            let settings = youtube::PublishSettings {
                title: String::from("cutix publish check"),
                description: String::from("verifying that publish is not a draft"),
                privacy: youtube::Privacy::Private,
                category_label: String::from("Люди и блоги"),
                ..Default::default()
            };
            youtube::queue::Task::new(
                String::from("probe"),
                accounts_active(&directory),
                settings,
                source,
                0,
                youtube::now_unix(),
            )
        }
    };

    println!("task     : {}", task.id);
    println!("title    : {}", task.settings.title);
    println!("file     : {}", task.source.display());
    println!("account  : {}", task.account_id);
    println!("privacy  : {:?}", task.settings.privacy);
    if let Some(note) = task.failure_note() {
        println!("last fail: {} ({})", note.key, note.detail);
    }

    if !task.source.is_file() {
        println!("the file is not there any more, so there is nothing to send");
        return;
    }

    let profile = youtube::chrome::profile_directory(&directory, &task.account_id);
    println!(
        "profile  : {} (exists: {})",
        profile.display(),
        profile.is_dir()
    );
    match youtube::chrome::find() {
        Some(binary) => println!("browser  : {}", binary.display()),
        None => {
            println!("no browser found; nothing can run");
            return;
        }
    }

    let headless = std::env::var("CUTIX_UPLOAD_VISIBLE").is_err();
    println!("headless : {headless}");
    println!("--- opening ---");

    let mut session = match youtube::Session::open(&directory, &task.account_id, headless) {
        Ok(session) => session,
        Err(error) => {
            println!("FAILED to start the browser: {error}");
            return;
        }
    };

    match session.is_signed_in() {
        Ok(true) => println!("session  : alive"),
        Ok(false) => {
            println!("FAILED: the profile is not signed in");
            session.close();
            return;
        }
        Err(error) => {
            println!("FAILED to check the session: {error}");
            session.close();
            return;
        }
    }

    println!("--- uploading ---");
    let mut last = (Stage::Opening, u32::MAX);
    let outcome = session.upload(&task.settings, &task.source, &mut |stage, percent| {
        if (stage, percent) != last {
            last = (stage, percent);
            println!("  {stage:?} {percent}%");
            let _ = std::io::stdout().flush();
        }
        Control::Continue
    });

    match outcome {
        Ok(video_id) => println!("DONE: {}", youtube::publish::watch_url(&video_id)),
        Err(error) => println!("FAILED: {error}"),
    }
    session.close();
}
