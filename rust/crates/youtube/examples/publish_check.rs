use std::io::Write;
use youtube::studio::{Control, Stage};

fn main() {
    let directory = youtube::data_directory();
    let accounts = youtube::Accounts::load(&directory);
    let Some(account) = std::env::var("CUTIX_ACCOUNT")
        .ok()
        .and_then(|wanted| {
            accounts
                .accounts
                .iter()
                .find(|account| account.id == wanted || account.title == wanted)
                .cloned()
        })
        .or_else(|| accounts.active().cloned())
    else {
        println!("no account is signed in");
        return;
    };

    let source = std::path::PathBuf::from(
        std::env::var("CUTIX_TEST_VIDEO")
            .unwrap_or_else(|_| String::from(r"C:\Users\ksana\Videos\test-clip2.mp4")),
    );
    if !source.is_file() {
        println!("no such file: {}", source.display());
        return;
    }

    let settings = youtube::PublishSettings {
        title: std::env::var("CUTIX_TEST_TITLE")
            .unwrap_or_else(|_| String::from("cutix description check")),
        description: std::env::var("CUTIX_TEST_DESCRIPTION").unwrap_or_else(|_| {
            String::from(
                "first line

third line after a blank one",
            )
        }),
        tags: vec![String::from("cutix"), String::from("field check")],
        category_id: String::from("22"),
        category_label: String::from("People & Blogs"),
        privacy: match std::env::var("CUTIX_TEST_PRIVACY").as_deref() {
            Ok("public") => youtube::Privacy::Public,
            Ok("private") => youtube::Privacy::Private,
            _ => youtube::Privacy::Unlisted,
        },
        made_for_kids: false,
        notify_subscribers: false,
        ..Default::default()
    };

    println!("account : {} ({})", account.title, account.id);
    println!("file    : {}", source.display());
    println!("title   : {}", settings.title);
    println!("privacy : {:?}", settings.privacy);
    println!("--- opening the browser ---");

    let headless = std::env::var("CUTIX_UPLOAD_VISIBLE").is_err();
    let mut session = match youtube::Session::open(&directory, &account.id, headless) {
        Ok(session) => session,
        Err(error) => {
            println!("the browser would not open: {error:?}");
            return;
        }
    };

    match session.is_signed_in() {
        Ok(true) => println!("session : alive"),
        Ok(false) => {
            println!("session : signed out — nothing can be published");
            session.close();
            return;
        }
        Err(error) => println!("session : unreadable ({error:?})"),
    }

    let mut last = (Stage::Opening, 0u32);
    let mut report = move |stage: Stage, percent: u32| -> Control {
        if last.0 != stage || last.1 != percent {
            println!("{stage:?} {percent}%");
            let _ = std::io::stdout().flush();
            last = (stage, percent);
        }
        Control::Continue
    };

    let outcome = session.upload(&settings, &source, &mut report);
    session.close();

    println!("---");
    match outcome {
        Ok(video_id) => println!("published: {}", youtube::publish::watch_url(&video_id)),
        Err(failure) => println!("failed: {failure:?}"),
    }
}
