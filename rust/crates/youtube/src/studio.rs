use crate::cdp::{query, query_all, Connection, Page};
use crate::failure::Failure;
use crate::publish::{Privacy, PublishSettings};
use crate::selectors::studio as css;
use std::path::Path;
use std::time::Duration;

pub const UPLOAD_URL: &str = "https://www.youtube.com/upload?hl=en&persist_hl=1";

pub const DIALOG_TIMEOUT: Duration = Duration::from_secs(60);

pub const SETTLE: Duration = Duration::from_millis(1200);

pub const SAVE_TIMEOUT: Duration = Duration::from_secs(60);

pub const SAVE_GRACE: Duration = Duration::from_secs(6);

const DIALOG_SETTLE: Duration = Duration::from_secs(25);
const DIALOG_ATTEMPTS: usize = 2;
pub const STEP_TIMEOUT: Duration = Duration::from_secs(45);

pub const TRANSFER_TIMEOUT: Duration = Duration::from_secs(6 * 3_600);
const TICK: Duration = Duration::from_millis(500);

pub const PUBLISH_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    #[default]
    Opening,

    Uploading,

    Processing,

    Publishing,
    Done,
}

impl Stage {
    pub fn message_key(&self) -> &'static str {
        match self {
            Self::Opening => "youtube.stage.opening",
            Self::Uploading => "youtube.stage.uploading",
            Self::Processing => "youtube.stage.processing",
            Self::Publishing => "youtube.stage.publishing",
            Self::Done => "youtube.stage.done",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Continue,
    Cancel,
}

pub fn parse_percent(label: &str) -> Option<u32> {
    let bytes = label.as_bytes();
    let percent = label.find('%')?;
    let mut start = percent;
    while start > 0 && bytes[start - 1].is_ascii_digit() {
        start -= 1;
    }
    if start == percent {
        return None;
    }
    label[start..percent]
        .parse::<u32>()
        .ok()
        .map(|value| value.min(100))
}

pub fn is_transfer_done(label: &str) -> bool {
    let lower = label.to_lowercase();
    !lower.contains('%')
        && (lower.contains("process")
            || lower.contains("check")
            || lower.contains("upload complete")
            || lower.contains("обработ")
            || lower.contains("оброб"))
}

pub fn video_id_from_link(link: &str) -> Option<String> {
    let candidate = if let Some(rest) = link.split("youtu.be/").nth(1) {
        rest
    } else if let Some(rest) = link.split("watch?v=").nth(1) {
        rest
    } else if let Some(rest) = link.split("/video/").nth(1) {
        rest
    } else {
        return None;
    };
    let id: String = candidate
        .chars()
        .take_while(|character| {
            character.is_ascii_alphanumeric() || *character == '-' || *character == '_'
        })
        .collect();
    (id.len() >= 8).then_some(id)
}

fn text_of(selector: &str) -> String {
    format!(
        "(() => {{ const node = {}; return node ? (node.innerText || node.textContent || '').trim() : ''; }})()",
        query(selector)
    )
}

fn exists(selector: &str) -> String {
    format!("(() => !!{})()", query(selector))
}

fn visible(selector: &str) -> String {
    format!(
        "(() => {{ const node = {}; return !!node && !!(node.offsetWidth || node.offsetHeight || \
         node.getClientRects().length); }})()",
        query(selector)
    )
}

fn usable(selector: &str) -> String {
    format!(
        "(() => {{ const node = {}; if (!node) return false; \
         if (!(node.offsetWidth || node.offsetHeight || node.getClientRects().length)) return false; \
         return !(node.hasAttribute('disabled') || node.getAttribute('aria-disabled') === 'true'); }})()",
        query(selector)
    )
}

fn chosen(selector: &str) -> String {
    format!(
        "(() => {{ const node = {}; return !!node && (node.getAttribute('aria-checked') === 'true' \
         || node.hasAttribute('checked')); }})()",
        query(selector)
    )
}

fn click_if_present(connection: &mut Connection, page: &Page, selector: &str) -> bool {
    page.eval_bool(
        connection,
        &format!(
            "(() => {{ const node = {}; if (!node) return false; node.click(); return true; }})()",
            query(selector)
        ),
    )
    .unwrap_or(false)
}

pub fn upload(
    connection: &mut Connection,
    page: &Page,
    settings: &PublishSettings,
    source: &Path,
    report: &mut dyn FnMut(Stage, u32) -> Control,
) -> Result<String, Failure> {
    if !source.is_file() {
        return Err(Failure::Io(format!("{} is not a file", source.display())));
    }
    report(Stage::Opening, 0);

    open_dialog(connection, page)?;
    page.choose_file(connection, css::SELECT_FILES, source)?;

    page.wait_until(
        connection,
        &format!("(() => {}.length > 1)()", query_all(css::TEXT_BOXES)),
        DIALOG_TIMEOUT,
        "details form",
    )?;
    report(Stage::Uploading, 0);

    fill_details(connection, page, settings)?;

    let video_id = wait_for_video_id(connection, page)?;

    match watch_transfer(connection, page, report)? {
        Control::Cancel => {
            let _ = click_if_present(connection, page, css::CLOSE_DIALOG);
            return Err(Failure::Cancelled);
        }
        Control::Continue => {}
    }

    restore_details(connection, page, settings);

    report(Stage::Publishing, 0);
    finish(connection, page, settings)?;
    report(Stage::Done, 100);
    Ok(video_id)
}

fn restore_details(connection: &mut Connection, page: &Page, settings: &PublishSettings) {
    let title = settings.title.trim();
    if title.is_empty() {
        return;
    }

    if !text_box_holds(connection, page, 0, title) {
        let _ = fill_text_box(connection, page, 0, title);
    }
    if !settings.description.is_empty()
        && !text_box_holds(connection, page, 1, &settings.description)
    {
        let _ = fill_text_box(connection, page, 1, &settings.description);
    }
    std::thread::sleep(SETTLE);
}

fn fill_details(
    connection: &mut Connection,
    page: &Page,
    settings: &PublishSettings,
) -> Result<(), Failure> {
    page.pretend_to_be_focused(connection);
    fill_text_box(connection, page, 0, settings.title.trim())?;
    if !settings.description.is_empty() {
        fill_text_box(connection, page, 1, &settings.description)?;
    }
    std::thread::sleep(SETTLE);

    let kids = if settings.made_for_kids {
        css::MADE_FOR_KIDS
    } else {
        css::NOT_FOR_KIDS
    };
    if !click_if_present(connection, page, kids) {
        return Err(Failure::PageChanged(kids.to_string()));
    }

    open_advanced(connection, page)?;

    if !settings.tags.is_empty() {
        page.fill(
            connection,
            css::TAGS,
            &format!("{},", settings.tags.join(",")),
        )?;
    }
    if settings.age_restricted {
        click_if_present(connection, page, css::AGE_RESTRICTED);
    }

    click_if_present(
        connection,
        page,
        if settings.altered_content {
            css::ALTERED_CONTENT_YES
        } else {
            css::ALTERED_CONTENT_NO
        },
    );

    set_category(connection, page, settings)?;
    fill_extras(connection, page, settings);
    Ok(())
}

pub fn fill_extras(connection: &mut Connection, page: &Page, settings: &PublishSettings) {
    if let Some(thumbnail) = settings.thumbnail.as_deref().filter(|path| path.is_file()) {
        let _ = page.set_file(connection, css::THUMBNAIL_INPUT, thumbnail);
    }

    set_playlists(connection, page, &settings.playlists);

    if !settings.video_language.trim().is_empty() {
        pick_by_text(
            connection,
            page,
            css::VIDEO_LANGUAGE,
            &settings.video_language,
        );
    }

    if settings.license != crate::publish::License::default() {
        pick_option(
            connection,
            page,
            css::LICENSE_SELECT,
            settings.license.option_index(),
        );
    }
    set_checkbox(connection, page, css::ALLOW_EMBED, settings.allow_embedding);
    set_checkbox(
        connection,
        page,
        css::PAID_PROMOTION,
        settings.paid_promotion,
    );

    if let Some(moderation) = settings.comments.moderation_index() {
        if moderation > 0 {
            pick_option(connection, page, css::MODERATION_SELECT, moderation);
        }
    }
    if !settings.comments.enabled() {
        pick_last_option(connection, page, css::COMMENTS_SELECT);
    }

    set_labelled_checkbox(connection, page, "like", settings.show_like_count);

    if settings.remix != crate::publish::Remix::default() {
        click_if_present(connection, page, settings.remix.selector());
    }
}

fn open_items() -> String {
    format!(
        "{}.filter(node => !!(node.offsetWidth || node.offsetHeight || \
         node.getClientRects().length))",
        query_all(css::DROPDOWN_ITEMS)
    )
}

fn pick_option(connection: &mut Connection, page: &Page, trigger: &str, index: usize) -> bool {
    if !click_if_present(connection, page, trigger) {
        return false;
    }
    if page
        .wait_until(
            connection,
            &format!("(() => {}.length > {index})()", open_items()),
            STEP_TIMEOUT,
            "dropdown",
        )
        .is_err()
    {
        return false;
    }
    page.eval_bool(
        connection,
        &format!(
            "(() => {{ const node = {}[{index}]; if (!node) return false; node.click(); \
             return true; }})()",
            open_items()
        ),
    )
    .unwrap_or(false)
}

fn pick_last_option(connection: &mut Connection, page: &Page, trigger: &str) -> bool {
    if !click_if_present(connection, page, trigger) {
        return false;
    }
    if page
        .wait_until(
            connection,
            &format!("(() => {}.length > 0)()", open_items()),
            STEP_TIMEOUT,
            "dropdown",
        )
        .is_err()
    {
        return false;
    }
    page.eval_bool(
        connection,
        &format!(
            "(() => {{ const items = {}; const node = items[items.length - 1]; \
             if (!node) return false; node.click(); return true; }})()",
            open_items()
        ),
    )
    .unwrap_or(false)
}

fn pick_by_text(connection: &mut Connection, page: &Page, trigger: &str, text: &str) -> bool {
    if !click_if_present(connection, page, trigger) {
        return false;
    }
    if page
        .wait_until(
            connection,
            &format!("(() => {}.length > 0)()", open_items()),
            STEP_TIMEOUT,
            "dropdown",
        )
        .is_err()
    {
        return false;
    }
    let wanted = crate::publish::normalise_label(text);
    page.eval_bool(
        connection,
        &format!(
            "(() => {{ const wanted = {}; const node = {}.find(item => \
             (item.innerText || item.textContent || '').trim().toLowerCase() \
             .split(/\\s+/).join(' ') === wanted); if (!node) return false; node.click(); \
             return true; }})()",
            crate::cdp::js_string(&wanted),
            open_items()
        ),
    )
    .unwrap_or(false)
}

fn set_checkbox(connection: &mut Connection, page: &Page, selector: &str, wanted: bool) {
    let _ = page.eval(
        connection,
        &format!(
            "(() => {{ const outer = {}; if (!outer) return false; \
             const lit = outer.matches('ytcp-checkbox-lit') ? outer \
               : outer.querySelector('ytcp-checkbox-lit') || outer; \
             const on = lit.hasAttribute('checked') || \
               lit.getAttribute('aria-checked') === 'true'; \
             if (on === {wanted}) return true; \
             const target = lit.querySelector('#checkbox-container') || \
               lit.querySelector('#checkbox') || lit; \
             target.click(); return true; }})()",
            query(selector)
        ),
    );
}

fn set_labelled_checkbox(connection: &mut Connection, page: &Page, word: &str, wanted: bool) {
    let _ = page.eval(
        connection,
        &format!(
            "(() => {{ const word = {}; \
             const section = document.querySelector('ytcp-video-metadata-editor-advanced'); \
             if (!section) return false; \
             const node = Array.from(section.querySelectorAll('ytcp-form-checkbox')) \
             .find(item => (item.innerText || '').toLowerCase().includes(word)); \
             if (!node) return false; \
             const lit = node.querySelector('ytcp-checkbox-lit') || node; \
             const on = lit.hasAttribute('checked') || \
               lit.getAttribute('aria-checked') === 'true'; \
             if (on === {wanted}) return true; \
             const target = lit.querySelector('#checkbox-container') || \
               lit.querySelector('#checkbox') || lit; \
             target.click(); return true; }})()",
            crate::cdp::js_string(&word.to_lowercase())
        ),
    );
}

fn set_playlists(connection: &mut Connection, page: &Page, wanted: &[String]) {
    if wanted.is_empty() {
        return;
    }
    if !click_if_present(connection, page, css::PLAYLIST_SELECT) {
        return;
    }
    if page
        .wait_until(
            connection,
            &format!("(() => {}.length > 0)()", query_all(css::PLAYLIST_ITEMS)),
            STEP_TIMEOUT,
            "playlists",
        )
        .is_err()
    {
        return;
    }
    let names: Vec<String> = wanted
        .iter()
        .map(|name| crate::publish::normalise_label(name))
        .collect();
    let _ = page.eval(
        connection,
        &format!(
            "(() => {{ const wanted = {}; \
             for (const item of {}) {{ \
               const text = (item.innerText || item.textContent || '').trim().toLowerCase() \
                 .split(/\\s+/).join(' '); \
               if (!wanted.includes(text)) continue; \
               if (item.hasAttribute('checked') || \
                   item.getAttribute('aria-checked') === 'true') continue; \
               const target = item.querySelector('#checkbox-container') || \
                 item.querySelector('#checkbox') || item; \
               target.click(); \
             }} return true; }})()",
            serde_json::to_string(&names).unwrap_or_else(|_| "[]".to_string()),
            query_all(css::PLAYLIST_ITEMS)
        ),
    );
    click_if_present(connection, page, css::PLAYLIST_DONE);
}

fn open_dialog(connection: &mut Connection, page: &Page) -> Result<(), Failure> {
    for attempt in 0..DIALOG_ATTEMPTS {
        if attempt > 0 {
            let _ = page.eval(
                connection,
                "(() => { window.onbeforeunload = null; return true; })()",
            );
        }
        page.navigate(connection, UPLOAD_URL)?;

        if page
            .wait_until(
                connection,
                &exists(css::SELECT_FILES),
                DIALOG_SETTLE,
                "upload dialog",
            )
            .is_ok()
        {
            return Ok(());
        }
        if click_if_present(connection, page, css::CREATE_BUTTON) {
            std::thread::sleep(TICK);
            click_if_present(connection, page, css::UPLOAD_MENU_ITEM);
            if page
                .wait_until(
                    connection,
                    &exists(css::SELECT_FILES),
                    DIALOG_SETTLE,
                    "upload dialog",
                )
                .is_ok()
            {
                return Ok(());
            }
        }
    }
    Err(Failure::Timeout("upload dialog".to_string()))
}

fn fill_text_box(
    connection: &mut Connection,
    page: &Page,
    index: usize,
    text: &str,
) -> Result<(), Failure> {
    let centre = page
        .eval_string(
            connection,
            &format!(
                "(() => {{ const node = {}[{index}]; if (!node) return ''; \
                 node.scrollIntoView({{ block: 'center' }}); \
                 const box = node.getBoundingClientRect(); \
                 if (!box.width || !box.height) return ''; \
                 return (box.left + box.width / 2) + ',' + (box.top + box.height / 2); }})()",
                query_all(css::TEXT_BOXES)
            ),
        )
        .unwrap_or_default();

    let point = centre
        .split_once(',')
        .and_then(|(x, y)| Some((x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?)));

    if let Some((x, y)) = point {
        let _ = page.click_point(connection, x, y);
        std::thread::sleep(TICK);
    }

    let focused = page.eval_bool(
        connection,
        &format!(
            "(() => {{ const node = {}[{index}]; if (!node) return false; node.focus(); \
             document.execCommand('selectAll', false, null); \
             document.execCommand('delete', false, null); return true; }})()",
            query_all(css::TEXT_BOXES)
        ),
    )?;
    if !focused {
        return Err(Failure::PageChanged(format!(
            "{} [{index}]",
            css::TEXT_BOXES
        )));
    }
    page.insert_text(connection, text)?;
    commit_text_box(connection, page, index);

    let typed = text_box_holds(connection, page, index, text);
    trace_field(connection, page, index, "after typing", typed);
    if typed {
        return Ok(());
    }

    let written = page.eval_bool(
        connection,
        &format!(
            "(() => {{ const node = {}[{index}]; if (!node) return false; \
             node.textContent = {}; \
             node.dispatchEvent(new InputEvent('input', {{ bubbles: true, composed: true }})); \
             node.dispatchEvent(new Event('change', {{ bubbles: true, composed: true }})); \
             return true; }})()",
            query_all(css::TEXT_BOXES),
            crate::cdp::js_string(text)
        ),
    )?;
    if !written {
        return Err(Failure::PageChanged(format!(
            "{} [{index}]",
            css::TEXT_BOXES
        )));
    }

    let written_through = text_box_holds(connection, page, index, text);
    trace_field(connection, page, index, "after writing", written_through);
    if written_through {
        return Ok(());
    }
    Err(Failure::PageChanged(format!(
        "{} [{index}] would not take the text",
        css::TEXT_BOXES
    )))
}

fn trace_dialog(connection: &mut Connection, page: &Page, stage: &str) {
    let Ok(shot) = std::env::var("CUTIX_TRACE_SHOT") else {
        return;
    };
    let text = page
        .eval_string(
            connection,
            "(() => { const node = document.querySelector('ytcp-uploads-dialog');              return node ? (node.innerText || '').replace(/\n+/g, ' | ').slice(0, 400) : 'no dialog'; })()",
        )
        .unwrap_or_default();
    let buttons = page
        .eval_string(
            connection,
            "(() => Array.from(document.querySelectorAll('ytcp-button, tp-yt-paper-button, button'))              .filter(node => { const box = node.getBoundingClientRect(); return box.width > 0 && box.height > 0; })              .map(node => node.tagName + '#' + (node.id || '-') + ':' + (node.innerText || '').trim().slice(0, 24))              .join(' | '))()",
        )
        .unwrap_or_default();
    eprintln!("[dialog {stage}] {text}");
    eprintln!("[dialog {stage}] buttons: {buttons}");
    let path = std::path::PathBuf::from(format!("{shot}-{stage}.png"));
    if page.screenshot(connection, &path) {
        eprintln!("[dialog {stage}] screenshot: {}", path.display());
    }
}

fn trace_field(connection: &mut Connection, page: &Page, index: usize, stage: &str, held: bool) {
    if std::env::var("CUTIX_TRACE_FIELDS").is_err() {
        return;
    }
    let count = page
        .eval_string(
            connection,
            &format!("(() => String({}.length))()", query_all(css::TEXT_BOXES)),
        )
        .unwrap_or_default();
    let seen = page
        .eval_string(
            connection,
            &format!(
                "(() => {}.map((node, at) => at + '=' + JSON.stringify((node.innerText || '').slice(0, 24))                  + (node.offsetParent ? '/shown' : '/hidden')                  + '/' + (node.getBoundingClientRect().width | 0) + 'x' + (node.getBoundingClientRect().height | 0)).join(' '))()",
                query_all(css::TEXT_BOXES)
            ),
        )
        .unwrap_or_default();
    let header = page
        .eval_string(
            connection,
            "(() => { const node = document.querySelector('ytcp-uploads-dialog');              if (!node) return 'no dialog';              const title = node.querySelector('#dialog-title, .dialog-title, h1, h2');              return title ? (title.innerText || '').slice(0, 60) : 'no header'; })()",
        )
        .unwrap_or_default();
    eprintln!("[field {index}] {stage}: held={held} boxes={count} {seen} header={header:?}");
    if let Ok(shot) = std::env::var("CUTIX_TRACE_SHOT") {
        let path =
            std::path::PathBuf::from(format!("{shot}-{index}-{}.png", stage.replace(' ', "-")));
        if page.screenshot(connection, &path) {
            eprintln!("[field {index}] screenshot: {}", path.display());
        }
    }
}

fn real_click(connection: &mut Connection, page: &Page, selector: &str) -> bool {
    let centre = page
        .eval_string(
            connection,
            &format!(
                "(() => {{ const node = {}; if (!node) return '';                  node.scrollIntoView({{ block: 'center' }});                  const box = node.getBoundingClientRect();                  if (!box.width || !box.height) return '';                  return (box.left + box.width / 2) + ',' + (box.top + box.height / 2); }})()",
                query(selector)
            ),
        )
        .unwrap_or_default();

    let Some((x, y)) = centre
        .split_once(',')
        .and_then(|(x, y)| Some((x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?)))
    else {
        return page.click(connection, selector).is_ok();
    };

    if page.click_point(connection, x, y).is_err() {
        return page.click(connection, selector).is_ok();
    }
    std::thread::sleep(TICK);
    true
}

fn commit_text_box(connection: &mut Connection, page: &Page, index: usize) {
    let _ = page.eval_bool(
        connection,
        &format!(
            "(() => {{ const node = {}[{index}]; if (!node) return false;              node.dispatchEvent(new InputEvent('input', {{ bubbles: true, composed: true }}));              node.dispatchEvent(new Event('change', {{ bubbles: true, composed: true }}));              node.blur();              node.dispatchEvent(new FocusEvent('blur', {{ bubbles: false }}));              node.dispatchEvent(new FocusEvent('focusout', {{ bubbles: true, composed: true }}));              return true; }})()",
            query_all(css::TEXT_BOXES)
        ),
    );
    std::thread::sleep(TICK);
}

fn text_box_holds(connection: &mut Connection, page: &Page, index: usize, text: &str) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    let wanted = text.trim();
    loop {
        let seen = page
            .eval_string(
                connection,
                &format!(
                    "(() => {{ const node = {}[{index}]; \
                     return node ? (node.innerText || node.textContent || '') : ''; }})()",
                    query_all(css::TEXT_BOXES)
                ),
            )
            .unwrap_or_default();
        if seen.trim() == wanted {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(TICK);
    }
}

fn open_advanced(connection: &mut Connection, page: &Page) -> Result<(), Failure> {
    let deadline = std::time::Instant::now() + STEP_TIMEOUT;
    loop {
        if page
            .eval_bool(connection, &exists(css::ADVANCED))
            .unwrap_or(false)
        {
            return Ok(());
        }
        if !click_if_present(connection, page, css::SHOW_MORE) {
            return Err(Failure::PageChanged(css::SHOW_MORE.to_string()));
        }
        if std::time::Instant::now() >= deadline {
            return Err(Failure::Timeout("show more".to_string()));
        }
        std::thread::sleep(TICK);
    }
}

fn set_category(
    connection: &mut Connection,
    page: &Page,
    settings: &PublishSettings,
) -> Result<(), Failure> {
    if settings.category_id.trim().is_empty() {
        return Ok(());
    }

    if !click_if_present(connection, page, css::CATEGORY_SELECT) {
        return Err(Failure::PageChanged(css::CATEGORY_SELECT.to_string()));
    }

    page.wait_until(
        connection,
        &format!("(() => {}.length > 0)()", query_all(css::CATEGORY_ITEMS)),
        STEP_TIMEOUT,
        "category list",
    )?;

    let labels: Vec<String> = page
        .eval(
            connection,
            &format!(
                "(() => {}.map(node => (node.innerText || node.textContent || '').trim()))()",
                query_all(css::CATEGORY_ITEMS)
            ),
        )?
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default();

    let Some(index) = labels.iter().position(|text| {
        crate::publish::is_category(text, &settings.category_id, &settings.category_label)
    }) else {
        let _ = page.eval_bool(
            connection,
            "(() => { document.body.click(); return true; })()",
        );
        return Ok(());
    };

    let clicked = page.eval_bool(
        connection,
        &format!(
            "(() => {{ const node = {}[{index}]; if (!node) return false; node.click(); return true; }})()",
            query_all(css::CATEGORY_ITEMS)
        ),
    )?;
    if clicked {
        Ok(())
    } else {
        Err(Failure::PageChanged(css::CATEGORY_ITEMS.to_string()))
    }
}

fn set_notify_subscribers(connection: &mut Connection, page: &Page, wanted: bool) {
    set_checkbox(connection, page, css::NOTIFY_SUBSCRIBERS, wanted);
}

fn wait_for_video_id(connection: &mut Connection, page: &Page) -> Result<String, Failure> {
    let deadline = std::time::Instant::now() + DIALOG_TIMEOUT;
    loop {
        let link = page
            .eval_string(
                connection,
                &format!(
                    "(() => {{ const node = {}; if (!node) return ''; return node.href || node.value || \
                     node.textContent || ''; }})()",
                    query(css::SHARE_URL)
                ),
            )
            .unwrap_or_default();
        if let Some(id) = video_id_from_link(&link) {
            return Ok(id);
        }
        if let Some(failure) = studio_error(connection, page) {
            return Err(failure);
        }
        if std::time::Instant::now() >= deadline {
            return Err(Failure::Timeout("video link".to_string()));
        }
        std::thread::sleep(TICK);
    }
}

fn watch_transfer(
    connection: &mut Connection,
    page: &Page,
    report: &mut dyn FnMut(Stage, u32) -> Control,
) -> Result<Control, Failure> {
    let deadline = std::time::Instant::now() + TRANSFER_TIMEOUT;
    let mut last = (Stage::Uploading, u32::MAX);
    loop {
        let label = page.eval_string(
            connection,
            &format!(
                "(() => {{ const all = {}.map(node => (node.innerText || node.textContent || '') \
                 .trim()); return all.find(text => text.includes('%')) || all[0] || ''; }})()",
                query_all(css::PROGRESS_LABEL)
            ),
        )?;

        if let Some(failure) = daily_limit(connection, page) {
            return Err(failure);
        }

        if let Some(failure) = studio_error(connection, page) {
            return Err(failure);
        }

        let (stage, percent) = match parse_percent(&label) {
            Some(percent) => (Stage::Uploading, percent),
            None if is_transfer_done(&label) => (Stage::Processing, 100),
            None => last,
        };

        if (stage, percent) != last && percent != u32::MAX {
            last = (stage, percent);
            if report(stage, percent) == Control::Cancel {
                return Ok(Control::Cancel);
            }
        }

        if stage == Stage::Processing {
            return Ok(Control::Continue);
        }
        if std::time::Instant::now() >= deadline {
            return Err(Failure::Timeout("upload".to_string()));
        }
        std::thread::sleep(TICK);
    }
}

fn finish(
    connection: &mut Connection,
    page: &Page,
    settings: &PublishSettings,
) -> Result<(), Failure> {
    walk_to_last_step(connection, page)?;

    set_notify_subscribers(connection, page, settings.notify_subscribers);
    apply_visibility(connection, page, settings)?;

    page.wait_until(
        connection,
        &usable(css::DONE),
        STEP_TIMEOUT,
        "publish button",
    )?;
    trace_dialog(connection, page, "before-done");
    if !real_click(connection, page, css::DONE) {
        return Err(Failure::PageChanged(css::DONE.to_string()));
    }
    std::thread::sleep(SETTLE);
    trace_dialog(connection, page, "after-done");
    wait_until_published(connection, page)
}

fn walk_to_last_step(connection: &mut Connection, page: &Page) -> Result<(), Failure> {
    let deadline = std::time::Instant::now() + STEP_TIMEOUT * 4;
    while !page
        .eval_bool(connection, &visible(css::DONE))
        .unwrap_or(false)
    {
        if let Some(failure) = studio_error(connection, page) {
            return Err(failure);
        }
        if std::time::Instant::now() >= deadline {
            return Err(Failure::Timeout("upload steps".to_string()));
        }

        if page
            .eval_bool(connection, &usable(css::NEXT))
            .unwrap_or(false)
        {
            real_click(connection, page, css::NEXT);
        }
        std::thread::sleep(TICK);
    }
    Ok(())
}

fn wait_until_published(connection: &mut Connection, page: &Page) -> Result<(), Failure> {
    let state = format!(
        "(() => {{ const onScreen = (node) => {{ if (!node) return false; \
         const box = node.getBoundingClientRect(); return box.width > 0 && box.height > 0; }}; \
         if (onScreen(document.querySelector({}))) return 'settled'; \
         const dialog = document.querySelector({}); \
         if (!onScreen(dialog)) return 'settled'; \
         if (!onScreen(dialog.querySelector({}))) return 'settled'; \
         return 'waiting'; }})()",
        crate::cdp::js_string(css::PUBLISHED_DIALOG),
        crate::cdp::js_string(css::DIALOG),
        crate::cdp::js_string(css::DONE)
    );

    let deadline = std::time::Instant::now() + PUBLISH_TIMEOUT;
    loop {
        if let Some(failure) = studio_error(connection, page) {
            return Err(failure);
        }
        answer_the_checks_prompt(connection, page);
        if page
            .eval_string(connection, &state)
            .map(|value| value.trim() == "settled")
            .unwrap_or(false)
        {
            wait_until_saved(connection, page);
            trace_dialog(connection, page, "settled");
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            trace_dialog(connection, page, "timed-out");
            return Err(Failure::Timeout("publish".to_string()));
        }
        std::thread::sleep(TICK);
    }
}

fn answer_the_checks_prompt(connection: &mut Connection, page: &Page) -> bool {
    let centre = page
        .eval_string(
            connection,
            &format!(
                "(() => {{ const wanted = {}; \
                 const nodes = Array.from(document.querySelectorAll('ytcp-button, tp-yt-paper-button, button')); \
                 const hit = nodes.find(node => (node.innerText || '').trim().toLowerCase() === wanted \
                 && node.getBoundingClientRect().width > 0 && node.getBoundingClientRect().height > 0); \
                 if (!hit) return ''; \
                 const box = hit.getBoundingClientRect(); \
                 return (box.left + box.width / 2) + ',' + (box.top + box.height / 2); }})()",
                crate::cdp::js_string(css::CONFIRM_LABEL)
            ),
        )
        .unwrap_or_default();

    let Some((x, y)) = centre
        .split_once(',')
        .and_then(|(x, y)| Some((x.trim().parse::<f64>().ok()?, y.trim().parse::<f64>().ok()?)))
    else {
        return false;
    };

    if page.click_point(connection, x, y).is_err() {
        return false;
    }
    std::thread::sleep(SETTLE);
    true
}

fn wait_until_saved(connection: &mut Connection, page: &Page) {
    let saving = "(() => { const marks = Array.from(document.querySelectorAll('*'))          .filter(node => node.children.length === 0)          .map(node => (node.innerText || '').trim().toLowerCase());          return marks.some(text => text.startsWith('saving') || text.startsWith('сохранение'))          ? 'saving' : 'saved'; })()";

    let deadline = std::time::Instant::now() + SAVE_TIMEOUT;
    loop {
        let settled = page
            .eval_string(connection, saving)
            .map(|value| value.trim() == "saved")
            .unwrap_or(true);
        if settled || std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(TICK);
    }

    std::thread::sleep(SAVE_GRACE);
}

fn apply_visibility(
    connection: &mut Connection,
    page: &Page,
    settings: &PublishSettings,
) -> Result<(), Failure> {
    if let Some(publish_at) = settings
        .publish_at
        .as_deref()
        .filter(|at| !at.trim().is_empty())
    {
        if schedule(connection, page, publish_at) {
            return Ok(());
        }

        return Err(Failure::PageChanged(css::SCHEDULE_TOGGLE.to_string()));
    }

    let state = match settings.effective_privacy() {
        Privacy::Public => "public",
        Privacy::Unlisted => "unlisted",
        Privacy::Private => "private",
    };
    let selector = css::visibility(state);
    page.wait_until(
        connection,
        &visible(&selector),
        STEP_TIMEOUT,
        "visibility options",
    )?;
    if !real_click(connection, page, &selector) {
        click_if_present(connection, page, &selector);
    }
    std::thread::sleep(TICK);

    std::thread::sleep(TICK);
    if page
        .eval_bool(connection, &chosen(&selector))
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(Failure::PageChanged(selector))
    }
}

fn schedule(connection: &mut Connection, page: &Page, publish_at: &str) -> bool {
    if !click_if_present(connection, page, css::SCHEDULE_TOGGLE) {
        return false;
    }
    let (date, time) = match publish_at.split_once('T') {
        Some((date, rest)) => (date, rest.trim_end_matches('Z')),
        None => return false,
    };
    let hour_minute = time.get(0..5).unwrap_or(time);

    page.fill(connection, css::SCHEDULE_DATE, date).is_ok()
        && page
            .fill(connection, css::SCHEDULE_TIME, hour_minute)
            .is_ok()
}

fn daily_limit(connection: &mut Connection, page: &Page) -> Option<Failure> {
    let hit = page
        .eval_bool(
            connection,
            "(() => (document.body.innerText || '').toLowerCase() \
             .includes('daily upload limit'))()",
        )
        .unwrap_or(false);
    hit.then(|| Failure::Rejected("daily upload limit reached".to_string()))
}

fn studio_error(connection: &mut Connection, page: &Page) -> Option<Failure> {
    let message = page
        .eval_string(connection, &text_of(css::ERROR_SHORT))
        .unwrap_or_default();
    (!message.is_empty()).then(|| Failure::Rejected(message))
}

pub fn watch_url(video_id: &str) -> String {
    format!("https://www.youtube.com/watch?v={video_id}")
}

pub fn edit_url(video_id: &str) -> String {
    format!("https://studio.youtube.com/video/{video_id}/edit")
}

#[cfg(test)]
mod publish_tests {
    use super::*;
    #[test]
    fn the_still_checking_prompt_is_answered_by_publishing_anyway() {
        assert_eq!(css::CONFIRM_LABEL, "publish anyway");
        assert!(css::CONFIRM_BUTTON.contains("secondary-action-button"));
    }

    #[test]
    fn the_follow_up_dialog_studio_shows_after_saving_is_recognised() {
        assert!(css::PUBLISHED_DIALOG.contains("ytcp-uploads-still-processing-dialog"));
        assert!(css::PUBLISHED_DIALOG.contains("ytcp-video-published-dialog"));
    }
}

#[cfg(test)]
mod language_tests {
    use super::*;

    #[test]
    fn studio_is_asked_for_english_so_labels_match_the_table() {
        assert!(UPLOAD_URL.contains("hl=en"));
        assert!(UPLOAD_URL.contains("persist_hl=1"));
        assert!(crate::login::STUDIO_URL.contains("hl=en"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_percentage_is_read_out_of_studios_own_progress_line() {
        assert_eq!(parse_percent("Uploading 43% ... 2 minutes left"), Some(43));
        assert_eq!(parse_percent("Загрузка 7 % завершена"), None);
        assert_eq!(parse_percent("Загружено 7%"), Some(7));
        assert_eq!(parse_percent("100% uploaded"), Some(100));
        assert_eq!(parse_percent("0%"), Some(0));
    }

    #[test]
    fn a_line_with_no_number_is_not_mistaken_for_zero_percent() {
        assert_eq!(parse_percent(""), None);
        assert_eq!(parse_percent("Upload complete"), None);
        assert_eq!(parse_percent("%"), None);
    }

    #[test]
    fn a_percentage_beyond_a_hundred_is_clamped_rather_than_shown() {
        assert_eq!(parse_percent("999%"), Some(100));
    }

    #[test]
    fn the_end_of_the_transfer_is_recognised_in_more_than_one_language() {
        assert!(is_transfer_done(
            "Upload complete ... processing will begin shortly"
        ));
        assert!(is_transfer_done("Обработка видео"));
        assert!(is_transfer_done("Checks complete. No issues found."));
        assert!(!is_transfer_done("Uploading 43% ..."));
        assert!(!is_transfer_done(""));
    }

    #[test]
    fn the_video_id_comes_out_of_whichever_link_studio_is_showing() {
        assert_eq!(
            video_id_from_link("https://youtu.be/dQw4w9WgXcQ"),
            Some("dQw4w9WgXcQ".to_string())
        );
        assert_eq!(
            video_id_from_link("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=1"),
            Some("dQw4w9WgXcQ".to_string())
        );
        assert_eq!(
            video_id_from_link("https://studio.youtube.com/video/dQw4w9WgXcQ/edit"),
            Some("dQw4w9WgXcQ".to_string())
        );
    }

    #[test]
    fn an_empty_or_unfinished_link_yields_no_id_so_the_wait_carries_on() {
        assert_eq!(video_id_from_link(""), None);
        assert_eq!(video_id_from_link("https://youtu.be/"), None);
        assert_eq!(video_id_from_link("https://youtu.be/short"), None);
        assert_eq!(video_id_from_link("https://www.youtube.com/"), None);
    }

    #[test]
    fn the_stages_are_ordered_the_way_an_upload_runs() {
        assert!(Stage::Opening < Stage::Uploading);
        assert!(Stage::Uploading < Stage::Processing);
        assert!(Stage::Processing < Stage::Publishing);
        assert!(Stage::Publishing < Stage::Done);
        assert_eq!(Stage::Uploading.message_key(), "youtube.stage.uploading");
    }

    #[test]
    fn the_links_point_at_the_watch_page_and_the_editor() {
        assert_eq!(
            watch_url("abc12345"),
            "https://www.youtube.com/watch?v=abc12345"
        );
        assert_eq!(
            edit_url("abc12345"),
            "https://studio.youtube.com/video/abc12345/edit"
        );
    }
}
