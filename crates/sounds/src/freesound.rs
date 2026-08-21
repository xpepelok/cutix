use crate::http::get_json;
use crate::{SearchPage, SoundResult, SoundSource, SoundsError};
use serde_json::Value;
use std::time::Duration;

pub const PROVIDER: &str = "Freesound";

const BASE_URL: &str = "https://freesound.org/apiv2/search/text/";
const TIMEOUT: Duration = Duration::from_secs(15);
const FIELDS: &str = "id,name,description,url,previews,download,duration,filesize,type,channels,bitrate,bitdepth,samplerate,username,tags,license,created,num_downloads,avg_rating,num_ratings";

const COMMERCIAL_LICENSE_FILTER: &str =
    "license:(\"Attribution\" OR \"Creative Commons 0\" OR \"Attribution Commercial\")";
const CATEGORY_FILTER: &str = "tag:sound-effect OR tag:sfx OR tag:foley OR tag:ambient OR tag:nature OR tag:mechanical OR tag:electronic OR tag:impact OR tag:whoosh OR tag:explosion";

pub const DEFAULT_MIN_RATING: f32 = 3.0;

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

pub fn build_url(api_key: &str, query: &str, page: usize, page_size: usize) -> String {
    let sort = if query.trim().is_empty() {
        "downloads_desc"
    } else {
        "score"
    };

    let mut url = format!(
        "{BASE_URL}?query={}&token={}&page={page}&page_size={page_size}&sort={sort}&fields={FIELDS}",
        encode(query.trim()),
        encode(api_key)
    );
    url.push_str(&format!("&filter={}", encode("duration:[* TO 30.0]")));
    url.push_str(&format!(
        "&filter={}",
        encode(&format!("avg_rating:[{DEFAULT_MIN_RATING} TO *]"))
    ));
    url.push_str(&format!("&filter={}", encode(COMMERCIAL_LICENSE_FILTER)));
    url.push_str(&format!("&filter={}", encode(CATEGORY_FILTER)));
    url
}

pub fn search(
    api_key: &str,
    query: &str,
    page: usize,
    page_size: usize,
) -> Result<SearchPage, SoundsError> {
    let raw = get_json(&build_url(api_key, query, page, page_size), TIMEOUT)?;
    Ok(page_from_response(&raw))
}

pub fn page_from_response(raw: &Value) -> SearchPage {
    SearchPage {
        count: raw.get("count").and_then(Value::as_u64).unwrap_or(0) as usize,
        has_next_page: raw.get("next").is_some_and(|value| !value.is_null()),
        results: raw
            .get("results")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .filter_map(effect_from_result)
            .collect(),
    }
}

fn effect_from_result(result: &Value) -> Option<SoundResult> {
    let id = result.get("id")?.as_u64()?;
    let previews = result.get("previews");
    let preview_url = previews
        .and_then(|value| value.get("preview-hq-mp3"))
        .or_else(|| previews.and_then(|value| value.get("preview-lq-mp3")))
        .and_then(Value::as_str)
        .map(str::to_string);

    Some(SoundResult {
        id: format!("freesound:{id}"),
        name: result
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("Untitled")
            .to_string(),
        description: result
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        url: result
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        download_url: result
            .get("download")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| preview_url.clone()),
        preview_url,
        duration: result
            .get("duration")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
        filesize: result.get("filesize").and_then(Value::as_u64).unwrap_or(0),
        kind: result
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("audio")
            .to_string(),
        username: result
            .get("username")
            .and_then(Value::as_str)
            .unwrap_or("Unknown")
            .to_string(),
        tags: result
            .get("tags")
            .and_then(Value::as_array)
            .map(|tags| {
                tags.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        license: result
            .get("license")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        created: result
            .get("created")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        source: SoundSource::Freesound,
        license_url: result
            .get("license")
            .and_then(Value::as_str)
            .map(str::to_string),
        landing_url: result
            .get("url")
            .and_then(Value::as_str)
            .map(str::to_string),
        provider: Some(PROVIDER.to_string()),
        is_album: false,
        archive_identifier: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_url_carries_every_effects_filter_and_never_leaks_a_raw_key() {
        let url = build_url("k e y", "glass break", 2, 20);
        assert!(url.contains("token=k%20e%20y"));
        assert!(url.contains("query=glass%20break"));
        assert!(url.contains("page=2"));
        assert!(url.contains("sort=score"));
        assert_eq!(url.matches("&filter=").count(), 4);
        assert!(url.contains("duration%3A%5B%2A%20TO%2030.0%5D"));
        assert!(url.contains("avg_rating"));
    }

    #[test]
    fn an_empty_query_sorts_by_downloads() {
        assert!(build_url("k", "  ", 1, 20).contains("sort=downloads_desc"));
    }

    #[test]
    fn results_map_onto_the_shared_shape() {
        let raw = json!({
            "count": 42,
            "next": "https://freesound.org/apiv2/search/text/?page=2",
            "results": [{
                "id": 1234,
                "name": "Whoosh.wav",
                "description": "a whoosh",
                "url": "https://freesound.org/s/1234/",
                "previews": {
                    "preview-hq-mp3": "https://freesound.org/data/previews/1234-hq.mp3",
                    "preview-lq-mp3": "https://freesound.org/data/previews/1234-lq.mp3"
                },
                "duration": 1.25,
                "filesize": 4096,
                "type": "wav",
                "username": "someone",
                "tags": ["whoosh", "sfx"],
                "license": "http://creativecommons.org/publicdomain/zero/1.0/",
                "created": "2019-05-01"
            }]
        });

        let page = page_from_response(&raw);
        assert_eq!(page.count, 42);
        assert!(page.has_next_page);
        let effect = &page.results[0];
        assert_eq!(effect.id, "freesound:1234");
        assert_eq!(
            effect.preview_url.as_deref(),
            Some("https://freesound.org/data/previews/1234-hq.mp3")
        );
        assert_eq!(effect.download_url, effect.preview_url);
        assert_eq!(effect.source, SoundSource::Freesound);
        assert!((effect.duration - 1.25).abs() < 1e-9);
    }

    #[test]
    fn a_null_next_means_the_last_page() {
        let page = page_from_response(&json!({ "count": 3, "next": null, "results": [] }));
        assert!(!page.has_next_page);
    }
}
