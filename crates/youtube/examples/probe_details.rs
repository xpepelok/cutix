use youtube::cdp::{Connection, Page, query, query_all};
use youtube::chrome::{Browser, profile_directory};
use youtube::selectors::studio as css;

fn dump(page: &Page, connection: &mut Connection, label: &str, script: &str) {
    match page.eval_string(connection, script) {
        Ok(value) => println!("\n--- {label} ---\n{value}"),
        Err(error) => println!("\n--- {label} ---\n<script failed: {error}>"),
    }
}

const LINK_HOLDERS: &str = "(() => Array.from(document.querySelectorAll('*')).filter(n => { \
     const v = (n.value || '') + ' ' + (n.href || '') + ' ' + (n.textContent || ''); \
     return v.includes('youtu.be') || v.includes('watch?v='); }).slice(-8).map(n => \
     n.tagName.toLowerCase() + '#' + (n.id || '?') + ' cls=' + (n.className || '?') + ' :: ' + \
     ((n.value || n.href || n.textContent || '').trim().slice(0, 80))).join('\\n'))()";

const URL_IDS: &str = "(() => Array.from(document.querySelectorAll('[id]')).filter(n => \
     /url|link|share/i.test(n.id)).map(n => n.tagName.toLowerCase() + '#' + n.id + ' :: ' + \
     ((n.value || n.href || n.textContent || '').trim().slice(0, 60))).join('\\n'))()";

const PROGRESS: &str = "(() => Array.from(document.querySelectorAll('span.progress-label')) \
     .map(n => (n.innerText || n.textContent || '').trim()).join(' | '))()";

fn main() {
    let directory = youtube::data_directory();
    let accounts = youtube::Accounts::load(&directory);
    let Some(account) = accounts.active() else {
        println!("no account");
        return;
    };
    let queue = youtube::queue::Queue::load(&directory);
    let Some(task) = queue.tasks.first().cloned() else {
        println!("nothing queued to take a file from");
        return;
    };

    let profile = profile_directory(&directory, &account.id);
    let mut browser = Browser::launch(&profile, true).expect("browser");
    let mut connection = Connection::connect(&browser.websocket_url).expect("connect");
    let page = connection.open("about:blank").expect("tab");

    if let Ok(agent) = connection.user_agent()
        && youtube::cdp::is_headless_agent(&agent)
    {
        let _ = page.set_user_agent(&mut connection, &youtube::cdp::visible_user_agent(&agent));
    }

    page.navigate(&mut connection, "https://www.youtube.com/upload")
        .expect("navigate");
    page.wait_until(
        &mut connection,
        &format!("(() => !!{})()", query(css::SELECT_FILES)),
        std::time::Duration::from_secs(30),
        "dialog",
    )
    .expect("dialog");

    page.choose_file(&mut connection, css::SELECT_FILES, &task.source)
        .expect("file");
    page.wait_until(
        &mut connection,
        &format!("(() => {}.length > 1)()", query_all(css::TEXT_BOXES)),
        std::time::Duration::from_secs(60),
        "details",
    )
    .expect("details");
    println!("details form reached");

    for wait in [2, 5, 10, 20] {
        std::thread::sleep(std::time::Duration::from_secs(wait));
        println!("\n===== {wait}s after the form =====");
        dump(&page, &mut connection, "progress labels", PROGRESS);
        dump(
            &page,
            &mut connection,
            "anything holding a link",
            LINK_HOLDERS,
        );
        dump(
            &page,
            &mut connection,
            "ids mentioning url, link or share",
            URL_IDS,
        );
    }

    connection.close_browser();
    browser.wait_for_exit(std::time::Duration::from_secs(10));
    println!("\nbrowser closed; a draft was left behind and nothing was published");
}
