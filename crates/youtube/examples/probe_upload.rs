use youtube::cdp::{Connection, query_all};
use youtube::chrome::{Browser, profile_directory};

fn main() {
    let directory = youtube::data_directory();
    let accounts = youtube::Accounts::load(&directory);
    let Some(account) = accounts.active() else {
        println!("no account");
        return;
    };
    let profile = profile_directory(&directory, &account.id);
    let headless = std::env::var("CUTIX_UPLOAD_VISIBLE").is_err();

    let mut browser = match Browser::launch(&profile, headless) {
        Ok(browser) => browser,
        Err(error) => {
            println!("launch failed: {error}");
            return;
        }
    };
    let mut connection = match Connection::connect(&browser.websocket_url) {
        Ok(connection) => connection,
        Err(error) => {
            println!("connect failed: {error}");
            return;
        }
    };
    let page = match connection.open("about:blank") {
        Ok(page) => page,
        Err(error) => {
            println!("open failed: {error}");
            return;
        }
    };

    if let Err(error) = page.navigate(&mut connection, "https://www.youtube.com/upload") {
        println!("navigate failed: {error}");
    }

    for wait in [3, 5, 10, 15] {
        std::thread::sleep(std::time::Duration::from_secs(wait));
        println!("\n===== after {wait}s =====");
        let url = page.url(&mut connection).unwrap_or_default();
        println!("url   : {url}");
        let title = page
            .eval_string(&mut connection, "document.title")
            .unwrap_or_default();
        println!("title : {title}");

        for selector in [
            "#select-files-button",
            "ytcp-uploads-file-picker",
            "input[type=\"file\"]",
            "ytcp-uploads-dialog",
            "#create-icon",
            "#text-item-0",
            "[id=\"textbox\"]",
            "ytcp-button",
            "iframe",
        ] {
            let count = page
                .eval(
                    &mut connection,
                    &format!("(() => {}.length)()", query_all(selector)),
                )
                .ok()
                .and_then(|value| value.as_u64())
                .unwrap_or(0);
            println!("  {count:>3}  {selector}");
        }

        let text = page
            .eval_string(
                &mut connection,
                "(() => (document.body ? document.body.innerText : '').slice(0, 600))()",
            )
            .unwrap_or_default();
        println!("--- body text ---\n{text}");
    }

    connection.close_browser();
    browser.wait_for_exit(std::time::Duration::from_secs(10));
}
