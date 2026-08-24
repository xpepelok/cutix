use crate::http::get_json;
use crate::license::{format_openverse_license, is_edit_safe_openverse};
use crate::mojibake::repair_mojibake;
use crate::{SearchPage, SoundResult, SoundSource, SoundsError};
use serde_json::Value;
use std::time::Duration;

const BASE_URL: &str = "https://api.openverse.org/v1/audio/";
const MAX_PAGE_SIZE: usize = 20;
const DEFAULT_QUERY: &str = "music";
const TIMEOUT: Duration = Duration::from_secs(12);

fn encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

pub const EFFECTS_CATEGORY: &str = "sound_effect";
const DEFAULT_EFFECTS_QUERY: &str = "sound effect";

pub fn search(query: &str, page: usize, page_size: usize) -> Result<SearchPage, SoundsError> {
    search_in(query, page, page_size, None)
}

pub fn search_effects(
    query: &str,
    page: usize,
    page_size: usize,
) -> Result<SearchPage, SoundsError> {
    let query = query.trim();
    let query = if query.is_empty() {
        DEFAULT_EFFECTS_QUERY
    } else {
        query
    };

    match search_in(query, page, page_size, Some(EFFECTS_CATEGORY)) {
        Ok(page) if !page.results.is_empty() => Ok(page),
        Ok(_) | Err(_) => search_in(query, page, page_size, None),
    }
}

pub fn search_url(query: &str, page: usize, page_size: usize, category: Option<&str>) -> String {
    let query = query.trim();
    let query = if query.is_empty() {
        DEFAULT_QUERY
    } else {
        query
    };

    let mut url = format!(
        "{BASE_URL}?q={}&license_type=commercial,modification&page_size={}&page={page}",
        encode(query),
        page_size.min(MAX_PAGE_SIZE)
    );
    if let Some(category) = category {
        url.push_str(&format!("&category={}", encode(category)));
    }
    url
}

fn search_in(
    query: &str,
    page: usize,
    page_size: usize,
    category: Option<&str>,
) -> Result<SearchPage, SoundsError> {
    let url = search_url(query, page, page_size, category);

    let raw = match get_json(&url, TIMEOUT) {
        Ok(raw) => raw,

        Err(SoundsError::Request(message)) if message.contains("pagination depth") => {
            return Ok(SearchPage::default());
        }
        Err(error) => return Err(error),
    };

    Ok(page_from_response(&raw, page))
}

pub fn page_from_response(raw: &Value, page: usize) -> SearchPage {
    let count = raw.get("result_count").and_then(Value::as_u64).unwrap_or(0) as usize;
    let page_count = raw.get("page_count").and_then(Value::as_u64).unwrap_or(0) as usize;

    let results = raw
        .get("results")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(track_from_result)
        .collect();

    SearchPage {
        count,
        has_next_page: page < page_count,
        results,
    }
}

fn track_from_result(result: &Value) -> Option<SoundResult> {
    let id = result.get("id")?.as_str()?.to_string();
    let url = result.get("url")?.as_str()?.to_string();
    let license = result.get("license")?.as_str()?;

    if !is_edit_safe_openverse(license) {
        return None;
    }

    let license_version = result.get("license_version").and_then(Value::as_str);
    let landing = result
        .get("foreign_landing_url")
        .and_then(Value::as_str)
        .map(str::to_string);

    let filesize = match result.get("filesize") {
        Some(Value::Number(number)) => number.as_u64().unwrap_or(0),
        Some(Value::String(text)) => text.parse::<u64>().unwrap_or(0),
        _ => 0,
    };

    let duration = result
        .get("duration")
        .and_then(Value::as_f64)
        .map(|milliseconds| milliseconds / 1000.0)
        .unwrap_or(0.0);

    Some(SoundResult {
        id,
        name: repair_mojibake(
            result
                .get("title")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or("Untitled"),
        ),
        description: result
            .get("genres")
            .and_then(Value::as_array)
            .map(|genres| {
                genres
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default(),
        url: landing.clone().unwrap_or_else(|| url.clone()),
        preview_url: Some(url.clone()),
        download_url: Some(url),
        duration,
        filesize,
        kind: result
            .get("filetype")
            .and_then(Value::as_str)
            .unwrap_or("audio")
            .to_string(),
        username: repair_mojibake(
            result
                .get("creator")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .unwrap_or("Unknown"),
        ),
        tags: result
            .get("tags")
            .and_then(Value::as_array)
            .map(|tags| {
                tags.iter()
                    .filter_map(|tag| tag.get("name").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        license: format_openverse_license(license, license_version),
        created: result
            .get("indexed_on")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        source: SoundSource::Openverse,
        license_url: result
            .get("license_url")
            .and_then(Value::as_str)
            .map(str::to_string),
        landing_url: landing,
        provider: result
            .get("provider")
            .and_then(Value::as_str)
            .map(str::to_string),
        is_album: false,
        archive_identifier: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample() -> Value {
        json!({
            "result_count": 120,
            "page_count": 6,
            "results": [
                {
                    "id": "abc",
                    "title": "Ãtude No. 1",
                    "creator": "CafÃ© Trio",
                    "url": "https://example.org/a.mp3",
                    "foreign_landing_url": "https://example.org/a",
                    "duration": 184000,
                    "filesize": "4194304",
                    "filetype": "mp3",
                    "license": "by-sa",
                    "license_version": "4.0",
                    "license_url": "https://creativecommons.org/licenses/by-sa/4.0/",
                    "provider": "jamendo",
                    "genres": ["ambient", "piano"],
                    "tags": [{ "name": "calm" }, { "name": "loop" }],
                    "indexed_on": "2024-01-02"
                },
                {
                    "id": "nope",
                    "title": "Non commercial",
                    "url": "https://example.org/b.mp3",
                    "license": "by-nc-sa"
                }
            ]
        })
    }

    #[test]
    fn noncommercial_results_are_filtered_out_locally() {
        let page = page_from_response(&sample(), 1);
        assert_eq!(page.results.len(), 1);
        assert_eq!(page.results[0].id, "abc");
    }

    #[test]
    fn fields_are_normalised_the_way_the_web_route_does() {
        let page = page_from_response(&sample(), 1);
        let track = &page.results[0];
        assert_eq!(track.name, "Étude No. 1");
        assert_eq!(track.username, "Café Trio");
        assert!((track.duration - 184.0).abs() < 1e-9);
        assert_eq!(track.filesize, 4_194_304);
        assert_eq!(track.license, "CC BY SA 4.0");
        assert_eq!(track.description, "ambient, piano");
        assert_eq!(track.tags, ["calm", "loop"]);
        assert_eq!(track.url, "https://example.org/a");
        assert_eq!(
            track.preview_url.as_deref(),
            Some("https://example.org/a.mp3")
        );
        assert_eq!(track.source, SoundSource::Openverse);
    }

    #[test]
    fn paging_reports_more_pages_until_the_last_one() {
        assert!(page_from_response(&sample(), 1).has_next_page);
        assert!(page_from_response(&sample(), 5).has_next_page);
        assert!(!page_from_response(&sample(), 6).has_next_page);
        assert_eq!(page_from_response(&sample(), 1).count, 120);
    }

    #[test]
    fn the_effects_url_carries_no_key_and_asks_for_the_sound_effect_category() {
        let url = search_url("whoosh", 1, 20, Some(EFFECTS_CATEGORY));
        assert!(url.starts_with("https://api.openverse.org/v1/audio/?q=whoosh"));
        assert!(url.contains("&category=sound_effect"));
        assert!(url.contains("license_type=commercial,modification"));
        assert!(!url.contains("key"), "{url}");
        assert!(!url.contains("token"), "{url}");
    }

    #[test]
    fn a_blank_effects_query_still_asks_for_something() {
        let url = search_url("", 1, 20, Some(EFFECTS_CATEGORY));
        assert!(url.contains("q=music"));
        assert!(!search_url("  ", 2, 20, None).contains("category"));
    }

    #[test]
    fn an_empty_body_degrades_to_an_empty_page() {
        let page = page_from_response(&json!({}), 1);
        assert_eq!(page.count, 0);
        assert!(page.results.is_empty());
        assert!(!page.has_next_page);
    }
}
