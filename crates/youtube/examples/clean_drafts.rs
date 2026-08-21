use std::time::Duration;
use youtube::cdp::{Connection, Page, query};
use youtube::chrome::{Browser, profile_directory};

const ROWS: &str = "(() => Array.from(document.querySelectorAll('ytcp-video-row')).map((row, i) => \
     i + ') ' + (row.innerText || '').split('\\n').filter(Boolean).slice(0, 6).join(' | ')) \
     .join('\\n'))()";

const ROW_COUNT: &str = "(() => document.querySelectorAll('ytcp-video-row').length)()";

fn is_draft(page: &Page, connection: &mut Connection, index: usize) -> bool {
    page.eval_bool(
        connection,
        &format!(
            "(() => {{ const row = document.querySelectorAll('ytcp-video-row')[{index}]; \
             if (!row) return false; \
             const cell = row.querySelector('#visibility, .tablecell-visibility, \
             ytcp-video-visibility-select'); if (!cell) return false; \
             const text = (cell.innerText || cell.textContent || '').trim().toLowerCase(); \
             return text.includes('черновик') || text.includes('draft'); }})()"
        ),
    )
    .unwrap_or(false)
}

fn main() {
    let directory = youtube::data_directory();
    let accounts = youtube::Accounts::load(&directory);
    let Some(account) = accounts.active() else {
        println!("no account");
        return;
    };
    let for_real = std::env::var("CUTIX_DELETE")
        .map(|v| v == "yes")
        .unwrap_or(false);

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
    std::thread::sleep(Duration::from_secs(12));

    println!("=== what is on the channel ===");
    println!(
        "{}",
        page.eval_string(&mut connection, ROWS).unwrap_or_default()
    );

    let total = page
        .eval(&mut connection, ROW_COUNT)
        .ok()
        .and_then(|value| value.as_u64())
        .unwrap_or(0) as usize;
    let drafts: Vec<usize> = (0..total)
        .filter(|index| is_draft(&page, &mut connection, *index))
        .collect();

    println!(
        "\n{} rows, {} of them drafts: {drafts:?}",
        total,
        drafts.len()
    );

    if !for_real {
        println!("\nDRY RUN — nothing was deleted. Set CUTIX_DELETE=yes to remove these.");
        connection.close_browser();
        browser.wait_for_exit(Duration::from_secs(10));
        return;
    }

    let mut removed = 0;
    for _ in 0..drafts.len() {
        let total = page
            .eval(&mut connection, ROW_COUNT)
            .ok()
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize;
        let Some(index) = (0..total).find(|index| is_draft(&page, &mut connection, *index)) else {
            break;
        };

        let title = page
            .eval_string(
                &mut connection,
                &format!(
                    "(() => {{ const row = document.querySelectorAll('ytcp-video-row')[{index}]; \
                     return row ? (row.innerText || '').split('\\n').filter(Boolean)[1] || '?' : '?'; }})()"
                ),
            )
            .unwrap_or_default();
        println!("\nremoving draft #{index}: {title}");

        let picked = page.eval_bool(
            &mut connection,
            &format!(
                "(() => {{ const row = document.querySelectorAll('ytcp-video-row')[{index}]; \
                 if (!row) return false; \
                 const wanted = Array.from(row.querySelectorAll('button, ytcp-button')) \
                 .find(n => /удалить видео|delete video/i.test(n.getAttribute('aria-label') || '')); \
                 if (!wanted) return false; wanted.click(); return true; }})()"
            ),
        ).unwrap_or(false);
        if !picked {
            println!("  no delete button on that row; stopping rather than guessing");
            break;
        }
        std::thread::sleep(Duration::from_secs(2));

        let _ = page.eval(
            &mut connection,
            "(() => { const dialog = Array.from(document.querySelectorAll('tp-yt-paper-dialog, \
             ytcp-confirmation-dialog')).find(n => !!(n.offsetWidth || n.offsetHeight)); \
             if (!dialog) return false; \
             for (const box of dialog.querySelectorAll('ytcp-checkbox-lit, tp-yt-paper-checkbox')) { \
             if (box.getAttribute('aria-checked') !== 'true') box.click(); } return true; })()",
        );
        std::thread::sleep(Duration::from_millis(600));

        let confirmed = page
            .eval_bool(
                &mut connection,
                "(() => { const dialog = Array.from(document.querySelectorAll('tp-yt-paper-dialog, \
             ytcp-confirmation-dialog')).find(n => !!(n.offsetWidth || n.offsetHeight)); \
             if (!dialog) return false; \
             const wanted = Array.from(dialog.querySelectorAll('ytcp-button, button')) \
             .filter(n => !!(n.offsetWidth || n.offsetHeight)) \
             .find(n => /удалить|delete/i.test(n.innerText || '')); \
             if (!wanted || wanted.hasAttribute('disabled')) return false; \
             wanted.click(); return true; })()",
            )
            .unwrap_or(false);
        if !confirmed {
            println!("  the confirmation would not accept; stopping");
            break;
        }
        removed += 1;
        std::thread::sleep(Duration::from_secs(4));
    }

    println!("\nremoved {removed} draft(s)");
    println!("--- what is left ---");
    std::thread::sleep(Duration::from_secs(3));
    println!(
        "{}",
        page.eval_string(&mut connection, ROWS).unwrap_or_default()
    );

    connection.close_browser();
    browser.wait_for_exit(Duration::from_secs(10));
    let _ = query("");
}
