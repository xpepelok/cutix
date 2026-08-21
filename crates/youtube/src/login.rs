use crate::cdp::{Connection, Page};
use crate::devtools;
use crate::failure::Failure;
use std::time::Duration;

pub const SIGN_IN_URL: &str = "https://accounts.google.com/ServiceLogin?service=youtube&continue=https%3A%2F%2Fstudio.youtube.com%2F";
pub const STUDIO_URL: &str = "https://studio.youtube.com/?hl=en&persist_hl=1";

pub const STEP_TIMEOUT: Duration = Duration::from_secs(45);

pub const WATCH_INTERVAL: Duration = Duration::from_secs(1);

pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(15 * 60);

pub fn is_signed_in(url: &str) -> bool {
    let host = url
        .split("://")
        .nth(1)
        .unwrap_or("")
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("");
    host == "youtube.com" || host.ends_with(".youtube.com")
}

const LANDED: &str = "(() => location.host === 'youtube.com' || \
                      location.host.endsWith('.youtube.com'))()";

pub const SESSION_CHECK_TIMEOUT: Duration = Duration::from_secs(30);

pub fn is_session_live(connection: &mut Connection, page: &Page) -> Result<bool, Failure> {
    page.navigate(connection, STUDIO_URL)?;
    Ok(page
        .wait_until(connection, LANDED, SESSION_CHECK_TIMEOUT, "session check")
        .is_ok())
}

pub fn watch_for_sign_in(
    port: u16,
    still_open: &mut dyn FnMut() -> bool,
) -> Result<String, Failure> {
    let deadline = std::time::Instant::now() + SIGN_IN_TIMEOUT;
    loop {
        if let Ok(targets) = devtools::targets(port) {
            if let Some(url) = landed_url(&targets) {
                return Ok(url);
            }
        }
        if !still_open() {
            return Err(Failure::SignInAbandoned);
        }
        if std::time::Instant::now() >= deadline {
            return Err(Failure::SignInAbandoned);
        }
        std::thread::sleep(WATCH_INTERVAL);
    }
}

pub fn any_tab_signed_in(port: u16) -> bool {
    devtools::targets(port)
        .map(|targets| landed_url(&targets).is_some())
        .unwrap_or(false)
}

pub const CHANNEL_TIMEOUT: Duration = Duration::from_secs(30);

pub fn watch_for_channel_id(port: u16, still_open: &mut dyn FnMut() -> bool) -> Option<String> {
    let deadline = std::time::Instant::now() + CHANNEL_TIMEOUT;
    loop {
        if let Ok(targets) = devtools::targets(port) {
            let found = targets
                .iter()
                .filter(|target| target.is_page())
                .find_map(|target| channel_id_from_url(&target.url));
            if found.is_some() {
                return found;
            }
        }
        if !still_open() || std::time::Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(WATCH_INTERVAL);
    }
}

pub fn landed_url(targets: &[devtools::Target]) -> Option<String> {
    targets
        .iter()
        .find(|target| target.is_page() && is_signed_in(&target.url))
        .map(|target| target.url.clone())
}

pub fn channel_id_from_url(url: &str) -> Option<String> {
    let rest = url.split("/channel/").nth(1)?;
    let id = rest.split(['/', '?', '#']).next()?;
    (id.starts_with("UC") && id.len() >= 20).then(|| id.to_string())
}

pub fn wait_for_channel(connection: &mut Connection, page: &Page) -> Result<String, Failure> {
    page.navigate(connection, STUDIO_URL)?;
    let has_channel = "(() => location.href.includes('/channel/'))()";
    let onboarding = "(() => location.href.includes('/onboarding') || \
                      location.href.includes('create_channel'))()";

    match page.race(
        connection,
        &[has_channel, onboarding],
        STEP_TIMEOUT,
        "studio",
    )? {
        0 => {
            let url = page.url(connection)?;
            channel_id_from_url(&url).ok_or(Failure::NoChannel)
        }
        _ => Err(Failure::NoChannel),
    }
}

pub fn channel_name(connection: &mut Connection, page: &Page) -> String {
    page.eval_string(
        connection,
        "(() => { const node = document.querySelector('#channel-title, \
         ytcp-channel-name-and-icon #entity-name, #entity-name'); \
         return node ? node.textContent.trim() : ''; })()",
    )
    .unwrap_or_default()
}

pub fn channel_handle(connection: &mut Connection, page: &Page) -> String {
    page.eval_string(
        connection,
        "(() => { const seen = document.querySelectorAll('ytcp-header, ytcp-channel-name-and-icon,          #channel-handle, #entity-name, tp-yt-paper-tooltip'); for (const node of seen) {          const match = (node.innerText || '').match(/@[A-Za-z0-9._-]{3,30}/);          if (match) return match[0]; } return ''; })()",
    )
    .unwrap_or_default()
}

pub fn channel_avatar(connection: &mut Connection, page: &Page) -> String {
    page.eval_string(
        connection,
        &format!(
            "(() => {{ const node = {}; return node && node.src ? node.src : ''; }})()",
            crate::cdp::query(crate::selectors::channel::AVATAR_IMAGE)
        ),
    )
    .unwrap_or_default()
}

pub fn channel_avatar_bytes(connection: &mut Connection, page: &Page) -> Option<Vec<u8>> {
    let url = channel_avatar(connection, page);
    if !url.starts_with("https://") {
        return None;
    }
    let encoded = page
        .eval_string(
            connection,
            &format!(
                "(async () => {{ try {{ const response = await fetch({}); \
                 if (!response.ok) return ''; const buffer = await response.arrayBuffer(); \
                 if (buffer.byteLength > {MAX_AVATAR_BYTES}) return ''; \
                 let binary = ''; for (const byte of new Uint8Array(buffer)) \
                 binary += String.fromCharCode(byte); return btoa(binary); }} catch (e) {{ return ''; }} }})()",
                crate::cdp::js_string(&url)
            ),
        )
        .ok()?;
    decode_base64(&encoded)
}

const MAX_AVATAR_BYTES: usize = 2 * 1024 * 1024;

pub fn decode_base64(text: &str) -> Option<Vec<u8>> {
    if text.is_empty() {
        return None;
    }
    let value = |byte: u8| -> Option<u32> {
        Some(match byte {
            b'A'..=b'Z' => (byte - b'A') as u32,
            b'a'..=b'z' => (byte - b'a') as u32 + 26,
            b'0'..=b'9' => (byte - b'0') as u32 + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    };

    let body = text.trim_end_matches('=');
    let padding = text.len() - body.len();
    if padding > 2 || text.len() % 4 != 0 {
        return None;
    }

    let mut out = Vec::with_capacity(body.len() / 4 * 3);
    let mut accumulator: u32 = 0;
    let mut bits = 0;
    for byte in body.bytes() {
        accumulator = (accumulator << 6) | value(byte)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((accumulator >> bits) as u8);
        }
    }
    Some(out)
}

pub fn sign_out(connection: &mut Connection, page: &Page) -> Result<(), Failure> {
    page.navigate(connection, "https://accounts.google.com/Logout")?;
    page.wait_until(
        connection,
        "(() => !location.href.includes('/Logout'))()",
        Duration::from_secs(20),
        "sign-out",
    )
    .or(Ok(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landing_on_youtube_is_what_counts_as_signed_in() {
        assert!(is_signed_in(
            "https://studio.youtube.com/channel/UC123/videos"
        ));
        assert!(is_signed_in("https://www.youtube.com/"));
        assert!(is_signed_in("https://youtube.com/upload"));
        assert!(!is_signed_in("https://accounts.google.com/ServiceLogin"));
        assert!(!is_signed_in(
            "https://accounts.google.com/signin/challenge/totp/2"
        ));
    }

    #[test]
    fn a_lookalike_host_is_not_mistaken_for_youtube() {
        assert!(!is_signed_in("https://youtube.com.evil.example/"));
        assert!(!is_signed_in("https://notyoutube.com/"));
        assert!(!is_signed_in(
            "https://evil.example/?next=https://youtube.com/"
        ));
    }

    #[test]
    fn the_waiting_expression_and_the_rust_check_agree_on_what_counts() {
        assert!(LANDED.contains("location.host === 'youtube.com'"));
        assert!(LANDED.contains(".youtube.com"));
    }

    fn target(kind: &str, url: &str) -> devtools::Target {
        devtools::Target {
            kind: kind.to_string(),
            url: url.to_string(),
        }
    }

    #[test]
    fn the_wait_ends_on_the_first_ordinary_tab_that_reached_youtube() {
        let targets = vec![
            target("page", "https://accounts.google.com/ServiceLogin"),
            target(
                "page",
                "https://studio.youtube.com/channel/UC_x5XG1OV2P6uZZ5FSM9Ttw/videos",
            ),
        ];
        let landed = landed_url(&targets).expect("landed");
        assert_eq!(
            channel_id_from_url(&landed),
            Some("UC_x5XG1OV2P6uZZ5FSM9Ttw".to_string()),
            "the url the wait returns already carries the channel"
        );
    }

    #[test]
    fn a_browser_still_on_googles_page_keeps_the_wait_going() {
        assert_eq!(landed_url(&[]), None);
        assert_eq!(
            landed_url(&[target(
                "page",
                "https://accounts.google.com/signin/challenge/totp/2"
            )]),
            None
        );
    }

    #[test]
    fn only_a_real_tab_counts_not_a_worker_or_an_extension() {
        for kind in ["service_worker", "background_page", "iframe", "other"] {
            assert_eq!(
                landed_url(&[target(kind, "https://www.youtube.com/")]),
                None,
                "{kind}"
            );
        }
        assert!(landed_url(&[target("page", "https://www.youtube.com/")]).is_some());
    }

    #[test]
    fn the_channel_id_comes_out_of_the_studio_url() {
        assert_eq!(
            channel_id_from_url(
                "https://studio.youtube.com/channel/UC_x5XG1OV2P6uZZ5FSM9Ttw/videos"
            ),
            Some("UC_x5XG1OV2P6uZZ5FSM9Ttw".to_string())
        );
        assert_eq!(
            channel_id_from_url("https://studio.youtube.com/channel/UC_x5XG1OV2P6uZZ5FSM9Ttw?d=1"),
            Some("UC_x5XG1OV2P6uZZ5FSM9Ttw".to_string())
        );
        assert_eq!(channel_id_from_url("https://studio.youtube.com/"), None);
        assert_eq!(
            channel_id_from_url("https://studio.youtube.com/channel/short"),
            None
        );
    }

    #[test]
    fn the_person_gets_long_enough_to_find_a_phone_and_read_a_code() {
        assert!(SIGN_IN_TIMEOUT >= Duration::from_secs(10 * 60));
        assert!(SIGN_IN_TIMEOUT > STEP_TIMEOUT * 10);
    }

    #[test]
    fn the_sign_in_url_lands_on_studio_once_google_is_done() {
        assert!(SIGN_IN_URL.starts_with("https://accounts.google.com/"));
        assert!(SIGN_IN_URL.contains("studio.youtube.com"));
    }

    #[test]
    fn base64_from_the_page_decodes_back_to_the_bytes_that_went_in() {
        assert_eq!(decode_base64("TWFu"), Some(b"Man".to_vec()));
        assert_eq!(decode_base64("TWE="), Some(b"Ma".to_vec()));
        assert_eq!(decode_base64("TQ=="), Some(b"M".to_vec()));

        assert_eq!(
            decode_base64("iVBORw0KGgo="),
            Some(vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
        );
    }

    #[test]
    fn a_failed_fetch_or_a_mangled_reply_yields_no_avatar_rather_than_junk() {
        assert_eq!(
            decode_base64(""),
            None,
            "the page says '' when the fetch failed"
        );
        assert_eq!(decode_base64("TWFuX"), None, "not a multiple of four");
        assert_eq!(decode_base64("TW!u"), None, "not a base64 alphabet");
        assert_eq!(decode_base64("TQ==="), None, "over-padded");
    }
}
