use std::time::{Duration, Instant};
use youtube::cdp::{query, query_all, Connection, Page};
use youtube::chrome::{profile_directory, Browser};
use youtube::selectors::studio as css;

const VISIBLE_RADIOS: &str =
    "(() => Array.from(document.querySelectorAll('tp-yt-paper-radio-button')) \
     .filter(n => !!(n.offsetWidth || n.offsetHeight)).map(n => (n.getAttribute('name') || '?') + \
     '=' + (n.getAttribute('aria-checked') || n.getAttribute('checked') || 'no')).join('  '))()";

const COMPLAINTS: &str = "(() => Array.from(document.querySelectorAll('*')).filter(n => { \
     if (!(n.offsetWidth || n.offsetHeight)) return false; \
     const c = (n.className || '') + ' ' + (n.id || ''); \
     return /error|invalid|required|warning/i.test(c) && (n.innerText || '').trim().length > 0; }) \
     .slice(0, 6).map(n => (n.id || n.className) + ' :: ' + (n.innerText || '').trim() \
     .split('\\n')[0].slice(0, 90)).join('\\n'))()";

const VISIBLE_HEADINGS: &str = "(() => Array.from(document.querySelectorAll('ytcp-uploads-dialog h1, \
     ytcp-uploads-dialog h2, ytcp-uploads-dialog h3')).filter(n => !!(n.offsetWidth || n.offsetHeight)) \
     .map(n => (n.innerText || '').trim()).filter(Boolean).slice(0, 8).join(' / '))()";

fn say(page: &Page, connection: &mut Connection, label: &str, script: &str) {
    match page.eval_string(connection, script) {
        Ok(value) => println!("  {label}:\n    {}", value.replace('\n', "\n    ")),
        Err(error) => println!("  {label}: <failed: {error}>"),
    }
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

    println!("=== as the form opens ===");
    say(&page, &mut connection, "step", VISIBLE_HEADINGS);
    say(&page, &mut connection, "radios", VISIBLE_RADIOS);

    let _ = page.click(&mut connection, css::NOT_FOR_KIDS);
    std::thread::sleep(Duration::from_millis(800));
    println!("\n=== after answering \"not for kids\" ===");
    say(&page, &mut connection, "radios", VISIBLE_RADIOS);

    let altered = page.click(&mut connection, css::ALTERED_CONTENT_NO).is_ok();
    std::thread::sleep(Duration::from_millis(800));
    println!("\n=== after answering altered content (clicked={altered}) ===");
    say(&page, &mut connection, "radios", VISIBLE_RADIOS);

    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        let label = page
            .eval_string(
                &mut connection,
                &format!(
                    "(() => {}.map(n => (n.innerText || '').trim()).join(' | '))()",
                    query_all(css::PROGRESS_LABEL)
                ),
            )
            .unwrap_or_default();
        if youtube::studio::is_transfer_done(&label) {
            println!("\ntransfer done: {label}");
            break;
        }
        std::thread::sleep(Duration::from_secs(2));
    }

    println!("\n=== before pressing Next ===");
    say(&page, &mut connection, "radios", VISIBLE_RADIOS);
    say(&page, &mut connection, "complaints", COMPLAINTS);

    for step in 1..=4 {
        let _ = page.click(&mut connection, css::NEXT);
        std::thread::sleep(Duration::from_secs(3));
        println!("\n=== after Next #{step} ===");
        say(&page, &mut connection, "step", VISIBLE_HEADINGS);
        say(&page, &mut connection, "radios", VISIBLE_RADIOS);
        say(
            &page,
            &mut connection,
            "done button",
            "(() => { const n = document.querySelector('#done-button'); if (!n) return 'absent'; \
             const off = n.hasAttribute('disabled') || n.getAttribute('aria-disabled') === 'true'; \
             return (off ? 'disabled' : 'enabled') + ((n.offsetWidth || n.offsetHeight) ? \
             ',visible' : ',hidden'); })()",
        );
        say(
            &page,
            &mut connection,
            "visible checkboxes",
            "(() => Array.from(document.querySelectorAll('ytcp-checkbox-lit, tp-yt-paper-checkbox')) \
             .filter(n => !!(n.offsetWidth || n.offsetHeight)).map(n => (n.id || '?') + '=' + \
             (n.getAttribute('aria-checked') || '-')).join('  '))()",
        );
    }
    say(&page, &mut connection, "complaints", COMPLAINTS);

    connection.close_browser();
    browser.wait_for_exit(Duration::from_secs(10));
    println!("\nbrowser closed; a draft was left behind and nothing was published");
}
