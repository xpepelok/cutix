use youtube::cdp::Connection;
use youtube::chrome::{profile_directory, Browser};

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
    let page = connection.open("about:blank").expect("tab");

    if let Ok(agent) = connection.user_agent() {
        if youtube::cdp::is_headless_agent(&agent) {
            let _ = page.set_user_agent(&mut connection, &youtube::cdp::visible_user_agent(&agent));
        }
    }

    let url = format!("https://studio.youtube.com/channel/{}/videos", account.id);
    page.navigate(&mut connection, &url).expect("navigate");
    std::thread::sleep(std::time::Duration::from_secs(12));

    let rows = page
        .eval_string(
            &mut connection,
            "(() => Array.from(document.querySelectorAll('ytcp-video-row')).slice(0, 10) \
             .map(r => (r.innerText || '').split('\\n').filter(Boolean).slice(0, 8).join(' | ')) \
             .join('\\n---\\n'))()",
        )
        .unwrap_or_default();
    println!("--- rows ---\n{rows}");

    connection.close_browser();
    browser.wait_for_exit(std::time::Duration::from_secs(10));
}
