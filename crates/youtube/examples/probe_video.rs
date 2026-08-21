use youtube::cdp::{Connection, query_all};
use youtube::chrome::{Browser, profile_directory};
use youtube::selectors::studio as css;

fn main() {
    let id = std::env::args().nth(1).unwrap_or_default();
    if id.is_empty() {
        println!("usage: probe_video <video id>");
        return;
    }

    let directory = youtube::data_directory();
    let accounts = youtube::Accounts::load(&directory);
    let wanted = std::env::args().nth(2);
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
    println!("reading as: {} ({})", account.title, account.id);

    let profile = profile_directory(&directory, &account.id);
    let mut browser = Browser::launch(&profile, true).expect("browser");
    let mut connection = Connection::connect(&browser.websocket_url).expect("connect");
    let page = connection.open("about:blank").expect("tab");

    if let Ok(agent) = connection.user_agent()
        && youtube::cdp::is_headless_agent(&agent)
    {
        let _ = page.set_user_agent(&mut connection, &youtube::cdp::visible_user_agent(&agent));
    }

    let url = format!("https://studio.youtube.com/video/{id}/edit?hl=en");
    page.navigate(&mut connection, &url).expect("navigate");
    std::thread::sleep(std::time::Duration::from_secs(14));

    println!(
        "final url: {}",
        page.eval_string(&mut connection, "(() => location.href)()")
            .unwrap_or_default()
    );
    println!(
        "page title: {}",
        page.eval_string(&mut connection, "(() => document.title)()")
            .unwrap_or_default()
    );

    let boxes = page
        .eval_string(
            &mut connection,
            &format!(
                "(() => {}.map((node, at) => at + '=' + JSON.stringify((node.innerText || '').slice(0, 60))).join('\\n'))()",
                query_all(css::TEXT_BOXES)
            ),
        )
        .unwrap_or_default();
    println!("--- text boxes ---\n{boxes}");

    let visibility = page
        .eval_string(
            &mut connection,
            "(() => { const node = document.querySelector('#privacy-dropdown, ytcp-video-visibility-select'); \
             return node ? (node.innerText || '').slice(0, 120) : 'not found'; })()",
        )
        .unwrap_or_default();
    println!("--- visibility ---\n{visibility}");

    let tags = page
        .eval_string(
            &mut connection,
            &format!(
                "(() => {{ const node = {}[0]; return node ? (node.value || node.innerText || '') : 'no tag field'; }})()",
                query_all(css::TAGS)
            ),
        )
        .unwrap_or_default();
    println!("--- tags ---\n{tags}");

    connection.close_browser();
    browser.wait_for_exit(std::time::Duration::from_secs(10));
}
