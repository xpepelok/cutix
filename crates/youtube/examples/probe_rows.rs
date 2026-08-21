use youtube::cdp::Connection;
use youtube::chrome::{Browser, profile_directory};

fn main() {
    let wanted = std::env::args().nth(1);
    let directory = youtube::data_directory();
    let accounts = youtube::Accounts::load(&directory);
    let Some(account) = wanted
        .and_then(|id| {
            accounts
                .accounts
                .iter()
                .find(|account| account.id == id || account.title == id)
                .cloned()
        })
        .or_else(|| accounts.active().cloned())
    else {
        println!("no account");
        return;
    };
    println!("channel: {} ({})", account.title, account.id);

    let profile = profile_directory(&directory, &account.id);
    let mut browser = Browser::launch(&profile, true).expect("browser");
    let mut connection = Connection::connect(&browser.websocket_url).expect("connect");
    let page = connection.open("about:blank").expect("tab");

    if let Ok(agent) = connection.user_agent()
        && youtube::cdp::is_headless_agent(&agent)
    {
        let _ = page.set_user_agent(&mut connection, &youtube::cdp::visible_user_agent(&agent));
    }

    let url = format!(
        "https://studio.youtube.com/channel/{}/videos/upload?hl=en",
        account.id
    );
    page.navigate(&mut connection, &url).expect("navigate");
    std::thread::sleep(std::time::Duration::from_secs(16));

    let rows = page
        .eval_string(
            &mut connection,
            "(() => Array.from(document.querySelectorAll('ytcp-video-row')).slice(0, 12).map(row => { \
             const link = row.querySelector('a#video-title, a[href*=\"/video/\"]'); \
             const id = link ? (link.getAttribute('href') || '').split('/video/')[1] : ''; \
             const cell = row.querySelector('#visibility, .tablecell-visibility, ytcp-video-visibility-select'); \
             const seen = cell ? (cell.innerText || '').trim().split('\\n')[0] : '?'; \
             const text = (row.innerText || '').split('\\n').filter(Boolean).slice(0, 2).join(' | '); \
             return (id ? id.split('/')[0] : '????') + '  [' + seen + ']  ' + text; }).join('\\n'))()",
        )
        .unwrap_or_default();

    println!("--- rows ---\n{rows}");

    connection.close_browser();
    browser.wait_for_exit(std::time::Duration::from_secs(10));
}
