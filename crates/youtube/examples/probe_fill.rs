use std::time::Duration;
use youtube::cdp::{Connection, Page};
use youtube::chrome::{Browser, profile_directory};
use youtube::publish::{Comments, License, PublishSettings, Remix};

const FIRST_VIDEO: &str = "(() => { const link = document.querySelector('a#video-title'); \
     if (!link) return ''; const match = (link.href || '').match(/video\\/([^\\/]+)/); \
     return match ? match[1] : ''; })()";

const READ_BACK: &str = "(() => { \
     const value = selector => { const node = document.querySelector(selector); \
       if (!node) return '(gone)'; \
       const lines = (node.innerText || '').split('\\n').map(l => l.trim()).filter(Boolean); \
       return lines.length ? lines[lines.length - 1] : '(blank)'; }; \
     const ticked = selector => { const node = document.querySelector(selector); \
       if (!node) return '(gone)'; \
       const lit = node.matches('ytcp-checkbox-lit') ? node \
         : node.querySelector('ytcp-checkbox-lit') || node; \
       return lit.hasAttribute('checked') ? 'on' : 'off'; }; \
     const radio = selector => { const node = document.querySelector(selector); \
       if (!node) return '(gone)'; \
       return node.getAttribute('aria-checked') === 'true' ? 'chosen' : 'not chosen'; }; \
     return [ \
       'licence        = ' + value('ytcp-form-select#license ytcp-text-dropdown-trigger'), \
       'comments       = ' + value('ytcp-select#enablement-state-select ytcp-text-dropdown-trigger'), \
       'moderation     = ' + value('ytcp-select#moderation-type-select ytcp-text-dropdown-trigger'), \
       'embedding      = ' + ticked('ytcp-form-checkbox#allow-embed'), \
       'paid promotion = ' + ticked('ytcp-checkbox-lit#has-ppp'), \
       'recording date = ' + value('ytcp-text-dropdown-trigger#recorded-date'), \
       'video language = ' + value('ytcp-form-language-input#language-input ytcp-text-dropdown-trigger'), \
       'remix: audio   = ' + radio('#visual-opt-out-radio-button'), \
     ].join('\\n'); })()";

fn main() {
    let directory = youtube::data_directory();
    let accounts = youtube::Accounts::load(&directory);
    let Some(account) = accounts.active() else {
        println!("no account");
        return;
    };

    let profile = profile_directory(&directory, &account.id);
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
        &format!("https://studio.youtube.com/channel/{}/videos", account.id),
    )
    .expect("navigate");
    std::thread::sleep(Duration::from_secs(12));
    let video = page
        .eval_string(&mut connection, FIRST_VIDEO)
        .unwrap_or_default();
    if video.trim().is_empty() {
        println!("no video to try this on");
        connection.close_browser();
        browser.wait_for_exit(Duration::from_secs(10));
        return;
    }

    page.navigate(
        &mut connection,
        &format!("https://studio.youtube.com/video/{video}/edit"),
    )
    .expect("navigate");
    std::thread::sleep(Duration::from_secs(12));
    let _ = page.eval(
        &mut connection,
        "(() => { const more = document.querySelector('#toggle-button'); \
         if (more) more.click(); return true; })()",
    );
    std::thread::sleep(Duration::from_secs(4));

    println!(
        "=== before ===\n{}",
        page.eval_string(&mut connection, READ_BACK)
            .unwrap_or_default()
    );

    let settings = PublishSettings {
        license: License::CreativeCommons,
        comments: Comments::HoldAll,
        allow_embedding: false,
        paid_promotion: true,
        video_language: "Русский".to_string(),
        remix: Remix::AudioOnly,
        ..Default::default()
    };
    youtube::studio::fill_extras(&mut connection, &page, &settings);
    std::thread::sleep(Duration::from_secs(3));

    println!(
        "\n=== after ===\n{}",
        page.eval_string(&mut connection, READ_BACK)
            .unwrap_or_default()
    );
    println!(
        "\nexpected: Creative Commons / held for review / embedding off / promotion on / \
         1 Aug 2026 / Russian / audio-only remix"
    );
    println!("nothing was saved — Studio keeps these until its own Save is pressed.");

    connection.close_browser();
    browser.wait_for_exit(Duration::from_secs(10));
}
