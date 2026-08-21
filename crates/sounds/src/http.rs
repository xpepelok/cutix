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
    let response = agent(timeout)
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

    let extension = response
        .header("Content-Type")
        .and_then(extension_for_content_type)
        .unwrap_or("mp3");

    let target = directory.join(format!("{stem}.{extension}"));
    let mut file =
        std::fs::File::create(&target).map_err(|error| SoundsError::Request(error.to_string()))?;

    let mut reader = response.into_reader().take(MAX_DOWNLOAD_BYTES);
    let mut buffer = [0u8; 64 * 1024];
    let mut written: u64 = 0;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| SoundsError::Request(error.to_string()))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|error| SoundsError::Request(error.to_string()))?;
        written += read as u64;
    }

    if written == 0 {
        let _ = std::fs::remove_file(&target);
        return Err(SoundsError::Request("empty response body".to_string()));
    }

    Ok(target)
}

fn extension_for_content_type(value: &str) -> Option<&'static str> {
    let value = value.split(';').next()?.trim().to_ascii_lowercase();
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
