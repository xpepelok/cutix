use std::time::{Duration, Instant};
use youtube::cdp::{Connection, Page, query, query_all};
use youtube::chrome::{Browser, profile_directory};
use youtube::selectors::studio as css;

const BUTTON_STATE: &str = "(() => ['#next-button', '#next-button button', '#done-button', \
     '#done-button button'].map(id => { \
     const n = document.querySelector(id); if (!n) return id + '=absent'; \
     const off = n.hasAttribute('disabled') || n.getAttribute('aria-disabled') === 'true'; \
     const shown = !!(n.offsetWidth || n.offsetHeight); \
     return id + (off ? '=disabled' : '=enabled') + (shown ? ',shown' : ',hidden'); }).join('  '))()";

const RADIOS: &str = "(() => Array.from(document.querySelectorAll('tp-yt-paper-radio-button')) \
     .map(n => (n.getAttribute('name') || '?')).join(', '))()";

const PROGRESS: &str = "(() => Array.from(document.querySelectorAll('span.progress-label')) \
     .map(n => (n.innerText || '').trim()).filter(Boolean).join(' | '))()";

fn say(page: &Page, connection: &mut Connection, label: &str, script: &str) {
    match page.eval_string(connection, script) {
        Ok(value) => println!("  {label}: {value}"),
        Err(error) => println!("  {label}: <failed: {error}>"),
    }
}

fn enabled(page: &Page, connection: &mut Connection, selector: &str) -> bool {
    page.eval_bool(
        connection,
        &format!(
            "(() => {{ const n = {}; if (!n) return false; \
             return !(n.hasAttribute('disabled') || n.getAttribute('aria-disabled') === 'true'); }})()",
            query(selector)
        ),
    )
    .unwrap_or(false)
}

fn wait_enabled(page: &Page, connection: &mut Connection, selector: &str, seconds: u64) -> bool {
    let deadline = Instant::now() + Duration::from_secs(seconds);
    while Instant::now() < deadline {
        if enabled(page, connection, selector) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    false
}

fn main() {
    let directory = youtube::data_directory();
    let accounts = youtube::Accounts::load(&directory);
    let Some(account) = accounts.active() else {
        println!("no account");
        return;
    };
    let source = std::path::PathBuf::from(
        std::env::var("CUTIX_TEST_VIDEO")
            .unwrap_or_else(|_| String::from(r"C:\Users\ksana\Videos\cutix\New project.mp4")),
    );

    let profile = profile_directory(&directory, &account.id);
    let mut browser = Browser::launch(&profile, true).expect("browser");
    let mut connection = Connection::connect(&browser.websocket_url).expect("connect");
    let page = connection.open("about:blank").expect("tab");

    if let Ok(agent) = connection.user_agent() {
        if youtube::cdp::is_headless_agent(&agent) {
            let _ = page.set_user_agent(&mut connection, &youtube::cdp::visible_user_agent(&agent));
        }
    }

    page.navigate(&mut connection, "https://www.youtube.com/upload")
        .expect("navigate");
    page.wait_until(
        &mut connection,
        &format!("(() => !!{})()", query(css::SELECT_FILES)),
        Duration::from_secs(40),
        "dialog",
    )
    .expect("dialog");
    page.choose_file(&mut connection, css::SELECT_FILES, &source)
        .expect("file");
    page.wait_until(
        &mut connection,
        &format!("(() => {}.length > 1)()", query_all(css::TEXT_BOXES)),
        Duration::from_secs(60),
        "details",
    )
    .expect("details");
    println!("details form reached");

    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        let label = page
            .eval_string(&mut connection, PROGRESS)
            .unwrap_or_default();
        if youtube::studio::is_transfer_done(&label) {
            println!("transfer done: {label}");
            break;
        }
        std::thread::sleep(Duration::from_secs(2));
    }

    for step in 1..=4 {
        if enabled(&page, &mut connection, css::DONE) {
            println!("\nDone became usable before Next #{step} — this is the last step");
            break;
        }
        let ready = wait_enabled(&page, &mut connection, css::NEXT, 60);
        let clicked = page.click(&mut connection, "#next-button button").is_ok();
        println!("\n=== Next #{step} (was enabled={ready}, clicked={clicked}) ===");
        std::thread::sleep(Duration::from_secs(2));
        say(&page, &mut connection, "buttons", BUTTON_STATE);
        say(&page, &mut connection, "radios ", RADIOS);
    }

    println!("\n===== the last step =====");
    say(&page, &mut connection, "buttons", BUTTON_STATE);
    say(
        &page,
        &mut connection,
        "radio names and labels",
        "(() => Array.from(document.querySelectorAll('tp-yt-paper-radio-button')).map(n => \
         (n.getAttribute('name') || '?') + '::' + (n.innerText || '').trim().split('\\n')[0]) \
         .join(' | '))()",
    );
    say(
        &page,
        &mut connection,
        "control ids",
        "(() => Array.from(document.querySelectorAll('[id]')).filter(n => \
         /privacy|visib|schedul|public|private|unlist|notify/i.test(n.id)).map(n => \
         n.tagName.toLowerCase() + '#' + n.id).slice(0, 30).join(' | '))()",
    );

    connection.close_browser();
    browser.wait_for_exit(Duration::from_secs(10));
    println!("\nbrowser closed; a draft was left behind and nothing was published");
}
