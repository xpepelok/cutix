use crate::failure::Failure;
use serde_json::Value;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub kind: String,
    pub url: String,
}

impl Target {
    pub fn is_page(&self) -> bool {
        self.kind == "page"
    }
}

pub fn targets(port: u16) -> Result<Vec<Target>, Failure> {
    let body = get(port, "/json/list")?;
    Ok(parse_targets(&body))
}

pub fn parse_targets(body: &str) -> Vec<Target> {
    let Ok(Value::Array(entries)) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(|entry| {
            Some(Target {
                kind: entry.get("type")?.as_str()?.to_string(),
                url: entry.get("url")?.as_str()?.to_string(),
            })
        })
        .collect()
}

fn get(port: u16, path: &str) -> Result<String, Failure> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let mut stream = TcpStream::connect_timeout(&address, HTTP_TIMEOUT)
        .map_err(|error| Failure::Protocol(error.to_string()))?;
    stream
        .set_read_timeout(Some(HTTP_TIMEOUT))
        .and_then(|_| stream.set_write_timeout(Some(HTTP_TIMEOUT)))
        .map_err(|error| Failure::Protocol(error.to_string()))?;

    let request =
        format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|error| Failure::Protocol(error.to_string()))?;

    let mut raw = Vec::new();
    let mut chunk = [0u8; 8 * 1024];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                raw.extend_from_slice(&chunk[..read]);
                if raw.len() >= MAX_RESPONSE_BYTES || is_complete(&raw) {
                    break;
                }
            }

            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                break
            }
            Err(error) if raw.is_empty() => return Err(Failure::Protocol(error.to_string())),
            Err(_) => break,
        }
    }

    let text = String::from_utf8_lossy(&raw).into_owned();
    Ok(split_body(&text).to_string())
}

fn is_complete(raw: &[u8]) -> bool {
    let text = String::from_utf8_lossy(raw);
    let Some(header_end) = text.find("\r\n\r\n").map(|at| at + 4) else {
        return false;
    };
    match content_length(&text[..header_end]) {
        Some(length) => raw.len() >= header_end + length,

        None => false,
    }
}

pub fn content_length(headers: &str) -> Option<usize> {
    headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case("content-length")
            .then(|| value.trim().parse().ok())
            .flatten()
    })
}

pub fn split_body(response: &str) -> &str {
    match response.find("\r\n\r\n") {
        Some(at) => &response[at + 4..],
        None => match response.find("\n\n") {
            Some(at) => &response[at + 2..],
            None => "",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = r#"[
        { "type": "page", "url": "https://accounts.google.com/ServiceLogin", "id": "A" },
        { "type": "background_page", "url": "chrome-extension://abc/bg.html", "id": "B" },
        { "type": "page", "url": "https://studio.youtube.com/channel/UC123/videos", "id": "C" }
    ]"#;

    #[test]
    fn the_tab_list_names_every_page_and_its_address() {
        let targets = parse_targets(LIST);
        assert_eq!(targets.len(), 3);

        let pages: Vec<&Target> = targets.iter().filter(|target| target.is_page()).collect();
        assert_eq!(pages.len(), 2, "the extension page is not a tab");
        assert_eq!(
            pages[1].url,
            "https://studio.youtube.com/channel/UC123/videos"
        );
    }

    #[test]
    fn a_reply_that_is_not_a_tab_list_yields_nothing_rather_than_failing() {
        assert!(parse_targets("").is_empty());
        assert!(parse_targets("not json").is_empty());
        assert!(parse_targets("{}").is_empty());
        assert!(parse_targets("[]").is_empty());
        assert!(
            parse_targets(r#"[{"type":"page"}]"#).is_empty(),
            "no url, no tab"
        );
    }

    #[test]
    fn a_reply_is_complete_once_as_many_body_bytes_as_promised_have_arrived() {
        let head = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n";
        assert!(!is_complete(head), "headers alone are not the reply");
        assert!(!is_complete(
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n["
        ));
        assert!(is_complete(
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n[]"
        ));
        assert!(
            !is_complete(b"HTTP/1.1 200 OK\r\n"),
            "the headers are not finished"
        );
    }

    #[test]
    fn a_reply_with_no_length_is_read_on_rather_than_cut_short() {
        assert!(!is_complete(
            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n[]"
        ));
    }

    #[test]
    fn the_content_length_header_is_found_whatever_its_spelling() {
        assert_eq!(
            content_length("HTTP/1.1 200 OK\r\ncontent-length: 42\r\n"),
            Some(42)
        );
        assert_eq!(
            content_length("HTTP/1.1 200 OK\r\nContent-Length:42\r\n"),
            Some(42)
        );
        assert_eq!(
            content_length("HTTP/1.1 200 OK\r\nCONTENT-LENGTH:  42  \r\n"),
            Some(42)
        );
        assert_eq!(
            content_length("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n"),
            None
        );
        assert_eq!(
            content_length("HTTP/1.1 200 OK\r\nContent-Length: nonsense\r\n"),
            None
        );
    }

    #[test]
    fn the_body_is_taken_from_after_the_headers() {
        let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n[]";
        assert_eq!(split_body(response), "[]");

        assert_eq!(split_body("HTTP/1.1 200 OK\n\n[]"), "[]");
        assert_eq!(split_body("garbage with no header break"), "");
    }

    #[test]
    fn a_body_containing_a_blank_line_is_not_cut_short() {
        let response =
            "HTTP/1.1 200 OK\r\n\r\n[\r\n\r\n{\"type\":\"page\",\"url\":\"https://x/\"}]";
        let targets = parse_targets(split_body(response));
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].url, "https://x/");
    }
}
