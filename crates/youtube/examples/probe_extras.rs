use std::time::Duration;
use youtube::cdp::{Connection, Page};
use youtube::chrome::{Browser, profile_directory};
use youtube::publish::{Comments, License, Privacy, PublishSettings, Remix};
use youtube::studio::Control;

const READ_BACK: &str = "(() => { \
     const value = selector => { const node = document.querySelector(selector); \
       return node ? (node.innerText || node.textContent || '').trim().split('\\n')[0] : '(gone)'; }; \
     const ticked = selector => { const node = document.querySelector(selector); \
       if (!node) return '(gone)'; \
       const box = node.querySelector('ytcp-checkbox-lit, tp-yt-paper-checkbox') || node; \
       return box.getAttribute('aria-checked') === 'true' ? 'on' : 'off'; }; \
     const radio = selector => { const node = document.querySelector(selector); \
       if (!node) return '(gone)'; \
       return node.getAttribute('aria-checked') === 'true' ? 'chosen' : 'not chosen'; }; \
     return [ \
       'licence         = ' + value('ytcp-form-select#license ytcp-text-dropdown-trigger'), \
       'comments        = ' + value('ytcp-select#enablement-state-select ytcp-text-dropdown-trigger'), \
       'moderation      = ' + value('ytcp-select#moderation-type-select ytcp-text-dropdown-trigger'), \
       'embedding       = ' + ticked('ytcp-form-checkbox#allow-embed'), \
       'paid promotion  = ' + ticked('ytcp-checkbox-lit#has-ppp'), \
       'recording date  = ' + value('ytcp-text-dropdown-trigger#recorded-date'), \
       'video language  = ' + value('ytcp-form-language-input#language-input ytcp-text-dropdown-trigger'), \
       'remix: audio    = ' + radio('#visual-opt-out-radio-button'), \
       'category        = ' + value('ytcp-form-select#category ytcp-text-dropdown-trigger'), \
     ].join('\\n'); })()";

fn main() {
    let directory = youtube::data_directory();
    let accounts = youtube::Accounts::load(&directory);
    let Some(account) = accounts.active() else {
        println!("no account");
        return;
    };
    let account_id = account.id.clone();

    let source = std::path::PathBuf::from(
        std::env::var("CUTIX_VIDEO")
            .unwrap_or_else(|_| String::from(r"C:\Users\ksana\Videos\cutix\New project.mp4")),
    );
    if !source.is_file() {
        println!("no video at {}", source.display());
        return;
    }

    let settings = PublishSettings {
        title: "cutix advanced fields check".to_string(),
        description: "checking that the second half of the form is actually filled in".to_string(),
        tags: vec!["cutix".to_string()],
        category_id: "28".to_string(),
        category_label: "Наука и техника".to_string(),
        privacy: Privacy::Private,
        license: License::CreativeCommons,
        comments: Comments::HoldAll,
        allow_embedding: false,
        paid_promotion: true,
        video_language: "Русский".to_string(),
        remix: Remix::AudioOnly,
        notify_subscribers: false,
        ..Default::default()
    };

    let mut session =
        youtube::session::Session::open(&directory, &account_id, true).expect("session");
    if !session.is_signed_in().unwrap_or(false) {
        println!("that account is signed out");
        return;
    }

    let outcome = session.upload(&settings, &source, &mut |stage, percent| {
        println!("  {stage:?} {percent}%");
        Control::Continue
    });
    session.close();

    let video = match outcome {
        Ok(id) => id,
        Err(error) => {
            println!("upload failed: {error:?}");
            return;
        }
    };
    println!("published {video}, reading it back");

    let profile = profile_directory(&directory, &account_id);
    let mut browser = Browser::launch(&profile, true).expect("browser");
    let mut connection = Connection::connect(&browser.websocket_url).expect("connect");
    let page: Page = connection.open("about:blank").expect("tab");
    if let Ok(agent) = connection.user_agent()
        && youtube::cdp::is_headless_agent(&agent)
    {
        let _ = page.set_user_agent(&mut connection, &youtube::cdp::visible_user_agent(&agent));
    }

    page.navigate(
        &mut connection,
        &format!("https://studio.youtube.com/video/{video}/edit"),
    )
    .expect("navigate");
    std::thread::sleep(Duration::from_secs(14));
    let _ = page.eval(
        &mut connection,
        "(() => { const more = document.querySelector('#toggle-button'); \
         if (more) more.click(); return true; })()",
    );
    std::thread::sleep(Duration::from_secs(4));

    println!(
        "\n=== what the editor says ===\n{}",
        page.eval_string(&mut connection, READ_BACK)
            .unwrap_or_default()
    );
    println!(
        "\nexpected: Creative Commons / held for review / embedding off / promotion on /\n\
              1 Aug 2026 / Russian / audio-only remix / Science & Technology"
    );
    println!("\nthe video is private and is at https://studio.youtube.com/video/{video}/edit");

    connection.close_browser();
    browser.wait_for_exit(Duration::from_secs(10));
}
