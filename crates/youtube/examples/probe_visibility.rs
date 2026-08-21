use youtube::cdp::{Connection, Page, query, query_all};
use youtube::chrome::{Browser, profile_directory};
use youtube::selectors::studio as css;

fn dump(page: &Page, connection: &mut Connection, label: &str, script: &str) {
    match page.eval_string(connection, script) {
        Ok(value) => println!("\n--- {label} ---\n{value}"),
        Err(error) => println!("\n--- {label} ---\n<script failed: {error}>"),
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
        std::time::Duration::from_secs(40),
        "dialog",
    )
    .expect("dialog");
    page.choose_file(&mut connection, css::SELECT_FILES, &source)
        .expect("file");
    page.wait_until(
        &mut connection,
        &format!("(() => {}.length > 1)()", query_all(css::TEXT_BOXES)),
        std::time::Duration::from_secs(60),
        "details",
    )
    .expect("details");
    println!("details form reached");

    for _ in 0..12 {
        if page
            .eval_bool(
                &mut connection,
                &format!("(() => !!{})()", query(css::DONE)),
            )
            .unwrap_or(false)
        {
            break;
        }
        let _ = page.click(&mut connection, css::NEXT);
        std::thread::sleep(std::time::Duration::from_millis(900));
    }
    println!("last step reached");

    dump(
        &page,
        &mut connection,
        "every radio on this step, with its name",
        "(() => Array.from(document.querySelectorAll('tp-yt-paper-radio-button')).map(n => \
         (n.getAttribute('name') || '?') + ' :: ' + (n.innerText || '').trim().split('\\n')[0]) \
         .join('\\n'))()",
    );

    dump(
        &page,
        &mut connection,
        "ids mentioning privacy, visibility or schedule",
        "(() => Array.from(document.querySelectorAll('[id]')).filter(n => \
         /privacy|visib|schedul|public|private|unlist/i.test(n.id)).map(n => \
         n.tagName.toLowerCase() + '#' + n.id).join('\\n'))()",
    );

    dump(
        &page,
        &mut connection,
        "what #privacy-radios holds",
        "(() => { const box = document.querySelector('#privacy-radios'); if (!box) \
         return 'no #privacy-radios'; return Array.from(box.querySelectorAll('*')).slice(0, 20) \
         .map(n => n.tagName.toLowerCase() + '#' + (n.id || '?') + ' name=' + \
         (n.getAttribute('name') || '-')).join('\\n'); })()",
    );

    dump(
        &page,
        &mut connection,
        "the notify checkbox",
        "(() => { const n = document.querySelector('#notify-subscribers'); return n ? \
         n.tagName.toLowerCase() + ' checked=' + n.getAttribute('aria-checked') : 'absent'; })()",
    );

    connection.close_browser();
    browser.wait_for_exit(std::time::Duration::from_secs(10));
    println!("\nbrowser closed; a draft was left behind and nothing was published");
}
