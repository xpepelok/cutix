use crate::{SoundsError, USER_AGENT};
use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const MAX_DOWNLOAD_BYTES: u64 = 128 * 1024 * 1024;

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(timeout)
        .build()
}

pub fn get_json(url: &str, timeout: Duration) -> Result<Value, SoundsError> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(timeout)
        .timeout(timeout + Duration::from_secs(10))
        .build();
    let response = agent
        .get(url)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/json")
        .call()
        .map_err(|error| SoundsError::Request(error.to_string()))?;

    let mut body = String::new();
    response
        .into_reader()
        .take(MAX_RESPONSE_BYTES as u64)
        .read_to_string(&mut body)
        .map_err(|error| SoundsError::Request(error.to_string()))?;

    serde_json::from_str(&body).map_err(|error| SoundsError::Request(error.to_string()))
}

pub fn download_audio(url: &str, directory: &Path, stem: &str) -> Result<PathBuf, SoundsError> {
    std::fs::create_dir_all(directory).map_err(|error| SoundsError::Request(error.to_string()))?;

    let response = agent(Duration::from_secs(60))
        .get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| SoundsError::Request(error.to_string()))?;

    let content_type = response.header("Content-Type").unwrap_or_default();
    if is_text_content_type(content_type) {
        return Err(SoundsError::Request(format!(
            "the server answered with a web page ({}) instead of audio",
            mime_of(content_type)
        )));
    }
    let extension = extension_for_content_type(content_type).unwrap_or("mp3");

    let target = directory.join(unique_file_name(stem, url, extension));
    if std::fs::metadata(&target).is_ok_and(|metadata| metadata.len() > 0) {
        if !file_looks_like_a_page(&target) {
            return Ok(target);
        }
        std::fs::remove_file(&target).map_err(request_error)?;
    }

    let mut partial = target.clone().into_os_string();
    partial.push(".part");
    let partial = PathBuf::from(partial);
    let result = write_body(response.into_reader(), &partial, MAX_DOWNLOAD_BYTES)
        .and_then(|()| std::fs::rename(&partial, &target).map_err(request_error));
    if let Err(error) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }

    Ok(target)
}

fn write_body(reader: impl Read, path: &Path, max_bytes: u64) -> Result<(), SoundsError> {
    let mut file =
        std::fs::File::create(path).map_err(|error| SoundsError::Request(error.to_string()))?;

    let mut reader = reader.take(max_bytes + 1);
    let mut buffer = [0u8; 64 * 1024];
    let mut written: u64 = 0;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| SoundsError::Request(error.to_string()))?;
        if read == 0 {
            break;
        }
        written += read as u64;
        if written > max_bytes {
            return Err(SoundsError::Request(format!(
                "download exceeds {} MB",
                max_bytes / (1024 * 1024)
            )));
        }
        file.write_all(&buffer[..read])
            .map_err(|error| SoundsError::Request(error.to_string()))?;
    }

    if written == 0 {
        return Err(SoundsError::Request("empty response body".to_string()));
    }
    file.flush()
        .map_err(|error| SoundsError::Request(error.to_string()))
}

fn request_error(error: std::io::Error) -> SoundsError {
    SoundsError::Request(error.to_string())
}

fn unique_file_name(stem: &str, url: &str, extension: &str) -> String {
    format!("{stem}-{:08x}.{extension}", url_hash(url) as u32)
}

fn url_hash(url: &str) -> u64 {
    url.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn mime_of(value: &str) -> String {
    value
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
}

fn is_text_content_type(value: &str) -> bool {
    mime_of(value).starts_with("text/")
}

fn looks_like_a_page(head: &[u8]) -> bool {
    let head = head.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(head);
    head.iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| *byte == b'<')
}

fn file_looks_like_a_page(path: &Path) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    let mut head = [0u8; 512];
    let mut read = 0;
    while read < head.len() {
        match file.read(&mut head[read..]) {
            Ok(0) | Err(_) => break,
            Ok(count) => read += count,
        }
    }
    looks_like_a_page(&head[..read])
}

fn extension_for_content_type(value: &str) -> Option<&'static str> {
    let value = mime_of(value);
    match value.as_str() {
        "audio/mpeg" | "audio/mp3" => Some("mp3"),
        "audio/ogg" | "application/ogg" => Some("ogg"),
        "audio/wav" | "audio/x-wav" | "audio/wave" => Some("wav"),
        "audio/flac" | "audio/x-flac" => Some("flac"),
        "audio/mp4" | "audio/x-m4a" => Some("m4a"),
        _ => None,
    }
}

pub fn sanitise_stem(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character
            } else {
                '-'
            }
        })
        .collect();

    let trimmed: String = cleaned
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");

    let trimmed = if trimmed.len() > 64 {
        trimmed[..64].to_string()
    } else {
        trimmed
    };

    if trimmed.is_empty() {
        "sound".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_types_map_to_extensions() {
        assert_eq!(extension_for_content_type("audio/mpeg"), Some("mp3"));
        assert_eq!(
            extension_for_content_type("audio/ogg; charset=binary"),
            Some("ogg")
        );
        assert_eq!(extension_for_content_type("text/html"), None);
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("cutix-sounds-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn same_titles_from_different_urls_get_different_files() {
        let first = unique_file_name("Whoosh", "https://a.example/1.mp3", "mp3");
        let second = unique_file_name("Whoosh", "https://a.example/2.mp3", "mp3");
        assert_ne!(first, second);
        assert!(first.starts_with("Whoosh-") && first.ends_with(".mp3"));
        assert_eq!(
            first,
            unique_file_name("Whoosh", "https://a.example/1.mp3", "mp3")
        );
    }

    #[test]
    fn oversized_bodies_are_rejected_instead_of_truncated() {
        let directory = scratch_dir("oversized");
        let error = write_body(&[7u8; 100][..], &directory.join("big.part"), 64).unwrap_err();
        assert!(error.to_string().contains("exceeds"));
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn bodies_within_the_cap_are_written_whole() {
        let directory = scratch_dir("within");
        let path = directory.join("ok.part");
        write_body(&[7u8; 64][..], &path, 64).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), vec![7u8; 64]);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn empty_bodies_are_rejected() {
        let directory = scratch_dir("empty");
        assert!(write_body(&[][..], &directory.join("empty.part"), 64).is_err());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_text_content_type_is_never_audio() {
        assert!(is_text_content_type("text/html; charset=utf-8"));
        assert!(is_text_content_type("TEXT/PLAIN"));
        assert!(!is_text_content_type("audio/mpeg"));
        assert!(!is_text_content_type("application/octet-stream"));
        assert!(!is_text_content_type(""));
    }

    #[test]
    fn markup_is_told_apart_from_audio_by_its_first_bytes() {
        assert!(looks_like_a_page(b"<!DOCTYPE html><html>"));
        assert!(looks_like_a_page(b"\n\t <html lang=\"en\">"));
        assert!(looks_like_a_page(b"\xEF\xBB\xBF<html>"));
        assert!(!looks_like_a_page(b"ID3\x04\x00\x00"));
        assert!(!looks_like_a_page(b"\xFF\xFB\x90\x00"));
        assert!(!looks_like_a_page(b"OggS"));
        assert!(!looks_like_a_page(b"RIFF\x24\x00\x00\x00WAVE"));
        assert!(!looks_like_a_page(b""));
        assert!(!looks_like_a_page(b"   "));
    }

    fn serve(responses: Vec<(&'static str, &'static [u8])>) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for (content_type, body) in responses {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut request = [0u8; 4096];
                let _ = stream.read(&mut request);
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(body);
                let _ = stream.flush();
            }
        });
        format!("http://127.0.0.1:{port}/sound.mp3")
    }

    fn serve_once(content_type: &'static str, body: &'static [u8]) -> String {
        serve(vec![(content_type, body)])
    }

    #[test]
    fn a_page_served_as_audio_is_refused_and_nothing_is_cached() {
        let directory = scratch_dir("page");
        let url = serve_once("text/html; charset=utf-8", b"<html>rate limited</html>");
        let error = download_audio(&url, &directory, "Whoosh").unwrap_err();
        assert!(error.to_string().contains("web page"), "{error}");
        let leftovers: Vec<_> = std::fs::read_dir(&directory).unwrap().flatten().collect();
        assert!(
            leftovers.is_empty(),
            "nothing to serve back on the next try"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_page_an_earlier_build_cached_under_an_audio_name_is_replaced() {
        let directory = scratch_dir("stale");
        let url = serve_once("audio/mpeg", b"ID3\x04\x00\x00real audio");
        let stale = directory.join(unique_file_name("Whoosh", &url, "mp3"));
        std::fs::write(&stale, b"<!DOCTYPE html><html>sign in</html>").unwrap();

        let path = download_audio(&url, &directory, "Whoosh").unwrap();
        assert_eq!(path, stale);
        assert_eq!(std::fs::read(&path).unwrap(), b"ID3\x04\x00\x00real audio");
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_complete_audio_file_is_kept_rather_than_rewritten_on_a_repeat_download() {
        let directory = scratch_dir("reuse");
        let url = serve(vec![
            ("audio/mpeg", b"ID3\x04\x00\x00first".as_slice()),
            ("audio/mpeg", b"ID3\x04\x00\x00second".as_slice()),
        ]);
        let first = download_audio(&url, &directory, "Whoosh").unwrap();
        let again = download_audio(&url, &directory, "Whoosh").unwrap();
        assert_eq!(first, again);
        assert_eq!(
            std::fs::read(&again).unwrap(),
            b"ID3\x04\x00\x00first",
            "the file the timeline may already hold open is not touched"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn stems_lose_path_separators_and_unicode() {
        assert_eq!(sanitise_stem("../../etc/passwd"), "etc-passwd");
        assert_eq!(sanitise_stem("Café del Mar!"), "Caf-del-Mar");
        assert_eq!(sanitise_stem(""), "sound");
        assert_eq!(sanitise_stem("///"), "sound");
        assert!(!sanitise_stem(&"x".repeat(500)).contains('/'));
        assert_eq!(sanitise_stem(&"x".repeat(500)).len(), 64);
    }
}
