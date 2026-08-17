use crate::http::get_json;
use crate::license::{format_archive_license, is_edit_safe_license};
use crate::mojibake::repair_mojibake;
use crate::{SearchPage, SoundResult, SoundSource, SoundsError};
use serde_json::Value;
use std::time::Duration;

pub const PROVIDER: &str = "Internet Archive";

const SEARCH_URL: &str = "https://archive.org/advancedsearch.php";
const METADATA_URL: &str = "https://archive.org/metadata";
const DOWNLOAD_URL: &str = "https://archive.org/download";
const DETAILS_URL: &str = "https://archive.org/details";

const PERMISSIVE_QUERY: &str = r"collection:(netlabels) AND mediatype:(audio) AND (licenseurl:(*licenses\/by\/*) OR licenseurl:(*licenses\/by-sa\/*) OR licenseurl:(*publicdomain\/*))";

const SEARCH_TIMEOUT: Duration = Duration::from_secs(8);
const METADATA_TIMEOUT: Duration = Duration::from_secs(12);
const MAX_TRACKS_PER_ALBUM: usize = 60;

pub fn is_valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 150
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '@' | '-'))
}

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

pub fn details_url(identifier: &str) -> String {
    format!("{DETAILS_URL}/{}", encode(identifier))
}

pub fn file_url(identifier: &str, file_name: &str) -> String {
    let path = file_name
        .split('/')
        .map(encode)
        .collect::<Vec<_>>()
        .join("/");
    format!("{DOWNLOAD_URL}/{}/{path}", encode(identifier))
}

fn first_value(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::Array(entries) => entries.iter().find_map(|entry| match entry {
            Value::String(text) if !text.is_empty() => Some(repair_mojibake(text)),
            _ => None,
        }),
        Value::String(text) if !text.is_empty() => Some(repair_mojibake(text)),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(entries)) => entries
            .iter()
            .filter_map(|entry| entry.as_str().map(str::to_string))
            .collect(),
        Some(Value::String(text)) if !text.is_empty() => text
            .split(';')
            .map(|part| part.trim().to_string())
            .filter(|part| !part.is_empty())
            .collect(),
        _ => Vec::new(),
    }
}

pub fn parse_duration_seconds(value: Option<&Value>) -> f64 {
    let Some(raw) = first_value(value) else {
        return 0.0;
    };
    if raw.contains(':') {
        let mut total = 0.0;
        for part in raw.split(':') {
            let Ok(parsed) = part.parse::<f64>() else {
                return 0.0;
            };
            total = total * 60.0 + parsed;
        }
        return total;
    }
    raw.parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}

fn parse_number(value: Option<&Value>) -> u64 {
    first_value(value)
        .and_then(|raw| raw.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value as u64)
        .unwrap_or(0)
}

pub fn search_albums(query: &str, page: usize, rows: usize) -> Result<SearchPage, SoundsError> {
    let escaped: String = query
        .chars()
        .map(|c| if c == '\\' || c == '"' { ' ' } else { c })
        .collect();
    let escaped = escaped.trim();

    let full_query = if escaped.is_empty() {
        PERMISSIVE_QUERY.to_string()
    } else {
        format!("{PERMISSIVE_QUERY} AND ({escaped})")
    };

    let mut url = format!(
        "{SEARCH_URL}?q={}&rows={rows}&page={page}&output=json",
        encode(&full_query)
    );
    for field in ["identifier", "title", "creator", "year", "licenseurl"] {
        url.push_str(&format!("&fl%5B%5D={field}"));
    }

    let raw = get_json(&url, SEARCH_TIMEOUT)?;
    let response = raw
        .get("response")
        .ok_or(SoundsError::Malformed { provider: PROVIDER })?;
    let count = response
        .get("numFound")
        .and_then(Value::as_u64)
        .unwrap_or(0) as usize;
    let docs = response
        .get("docs")
        .and_then(Value::as_array)
        .ok_or(SoundsError::Malformed { provider: PROVIDER })?;

    let results = docs.iter().filter_map(album_from_doc).collect();

    Ok(SearchPage {
        count,
        has_next_page: page * rows < count,
        results,
    })
}

fn album_from_doc(doc: &Value) -> Option<SoundResult> {
    let identifier = doc.get("identifier")?.as_str()?.to_string();
    if !is_valid_identifier(&identifier) {
        return None;
    }
    let license_url = first_value(doc.get("licenseurl"));
    if !is_edit_safe_license(license_url.as_deref()) {
        return None;
    }
    let details = details_url(&identifier);

    Some(SoundResult {
        id: format!("archive:{identifier}"),
        name: first_value(doc.get("title")).unwrap_or_else(|| identifier.clone()),
        description: String::new(),
        url: details.clone(),
        preview_url: None,
        download_url: None,
        duration: 0.0,
        filesize: 0,
        kind: "album".to_string(),
        username: first_value(doc.get("creator")).unwrap_or_else(|| "Unknown".to_string()),
        tags: Vec::new(),
        license: format_archive_license(license_url.as_deref()),
        created: first_value(doc.get("year")).unwrap_or_default(),
        source: SoundSource::Archive,
        license_url,
        landing_url: Some(details),
        provider: Some(PROVIDER.to_string()),
        is_album: true,
        archive_identifier: Some(identifier),
    })
}

fn derived_source(file: &Value) -> String {
    let name = file
        .get("original")
        .and_then(Value::as_str)
        .or_else(|| file.get("name").and_then(Value::as_str))
        .unwrap_or_default();
    let stem = name
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(name)
        .to_lowercase();
    for suffix in ["_64kb", "_vbr", "_128kb", "_256kb", "_hifi", "_sample"] {
        if let Some(trimmed) = stem.strip_suffix(suffix) {
            return trimmed.to_string();
        }
    }
    stem
}

fn format_rank(file: &Value) -> u8 {
    match file
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "vbr mp3" => 3,
        "mp3" => 2,
        _ => 1,
    }
}

pub fn album_tracks(identifier: &str) -> Result<Vec<SoundResult>, SoundsError> {
    if !is_valid_identifier(identifier) {
        return Err(SoundsError::Malformed { provider: PROVIDER });
    }
    let raw = get_json(
        &format!("{METADATA_URL}/{}", encode(identifier)),
        METADATA_TIMEOUT,
    )?;
    Ok(tracks_from_metadata(identifier, &raw))
}

pub fn tracks_from_metadata(identifier: &str, raw: &Value) -> Vec<SoundResult> {
    let metadata = raw.get("metadata");
    let license_url = first_value(metadata.and_then(|value| value.get("licenseurl")));
    if !is_edit_safe_license(license_url.as_deref()) {
        return Vec::new();
    }

    let license = format_archive_license(license_url.as_deref());
    let album_title = first_value(metadata.and_then(|value| value.get("title")))
        .unwrap_or_else(|| identifier.to_string());
    let album_creator = first_value(metadata.and_then(|value| value.get("creator")));
    let details = details_url(identifier);
    let created = first_value(metadata.and_then(|value| value.get("date")))
        .or_else(|| first_value(metadata.and_then(|value| value.get("year"))))
        .unwrap_or_default();
    let mut tags = string_array(metadata.and_then(|value| value.get("subject")));
    tags.truncate(12);

    let mut best: Vec<&Value> = Vec::new();
    for file in raw
        .get("files")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter(|file| {
            file.get("format")
                .and_then(Value::as_str)
                .is_some_and(|format| format.to_lowercase().contains("mp3"))
        })
    {
        let key = derived_source(file);
        match best
            .iter()
            .position(|existing| derived_source(existing) == key)
        {
            Some(index) if format_rank(file) > format_rank(best[index]) => best[index] = file,
            Some(_) => {}
            None => best.push(file),
        }
    }

    best.into_iter()
        .take(MAX_TRACKS_PER_ALBUM)
        .filter_map(|file| {
            let name = file.get("name")?.as_str()?;
            let url = file_url(identifier, name);
            let title = file
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| {
                    name.strip_suffix(".mp3")
                        .or_else(|| name.strip_suffix(".MP3"))
                        .unwrap_or(name)
                        .to_string()
                });

            Some(SoundResult {
                id: format!("archive:{identifier}:{name}"),
                name: repair_mojibake(&title),
                description: repair_mojibake(
                    file.get("album")
                        .and_then(Value::as_str)
                        .unwrap_or(&album_title),
                ),
                url: details.clone(),
                preview_url: Some(url.clone()),
                download_url: Some(url),
                duration: parse_duration_seconds(file.get("length")),
                filesize: parse_number(file.get("size")),
                kind: "mp3".to_string(),
                username: first_value(file.get("artist"))
                    .or_else(|| first_value(file.get("creator")))
                    .or_else(|| album_creator.clone())
                    .unwrap_or_else(|| "Unknown".to_string()),
                tags: tags.clone(),
                license: license.clone(),
                created: created.clone(),
                source: SoundSource::Archive,
                license_url: license_url.clone(),
                landing_url: Some(details.clone()),
                provider: Some(PROVIDER.to_string()),
                is_album: false,
                archive_identifier: Some(identifier.to_string()),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identifiers_reject_traversal_and_spaces() {
        assert!(is_valid_identifier("netlabel_release-01@x.y"));
        assert!(!is_valid_identifier("../etc/passwd"));
        assert!(!is_valid_identifier("has space"));
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier(&"x".repeat(151)));
    }

    #[test]
    fn file_urls_encode_each_path_segment_separately() {
        assert_eq!(
            file_url("album_01", "disc 1/track 02.mp3"),
            "https://archive.org/download/album_01/disc%201/track%2002.mp3"
        );
    }

    #[test]
    fn albums_with_unsafe_licences_are_dropped() {
        let doc = json!({
            "identifier": "nc_album",
            "title": "Nope",
            "licenseurl": "https://creativecommons.org/licenses/by-nc/4.0/"
        });
        assert!(album_from_doc(&doc).is_none());
    }

    #[test]
    fn albums_with_safe_licences_survive_with_a_formatted_licence() {
        let doc = json!({
            "identifier": "good_album",
            "title": ["CafÃ© Sessions"],
            "creator": "Netlabel",
            "year": 2011,
            "licenseurl": "https://creativecommons.org/licenses/by-sa/3.0/"
        });
        let album = album_from_doc(&doc).expect("album");
        assert_eq!(album.name, "Café Sessions");
        assert_eq!(album.license, "CC BY SA 3.0");
        assert_eq!(album.created, "2011");
        assert!(album.is_album);
        assert_eq!(album.archive_identifier.as_deref(), Some("good_album"));
    }

    #[test]
    fn only_mp3_files_become_tracks_and_the_album_licence_gates_them() {
        let raw = json!({
            "metadata": {
                "identifier": "good_album",
                "title": "Sessions",
                "creator": "Netlabel",
                "licenseurl": "https://creativecommons.org/licenses/by/4.0/",
                "date": "2011-04-02",
                "subject": "ambient;drone"
            },
            "files": [
                { "name": "cover.jpg", "format": "JPEG" },
                { "name": "01 - Dawn.mp3", "format": "VBR MP3", "title": "Dawn",
                  "length": "3:42", "size": "5242880", "artist": "Someone" },
                { "name": "02.mp3", "format": "MP3" }
            ]
        });

        let tracks = tracks_from_metadata("good_album", &raw);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].name, "Dawn");
        assert!((tracks[0].duration - 222.0).abs() < 1e-9);
        assert_eq!(tracks[0].filesize, 5_242_880);
        assert_eq!(tracks[0].license, "CC BY 4.0");
        assert_eq!(tracks[0].tags, ["ambient", "drone"]);
        assert_eq!(
            tracks[0].preview_url.as_deref(),
            Some("https://archive.org/download/good_album/01%20-%20Dawn.mp3")
        );
        assert_eq!(tracks[1].name, "02");
        assert_eq!(tracks[1].username, "Netlabel");
    }

    #[test]
    fn an_unsafe_album_licence_yields_no_tracks() {
        let raw = json!({
            "metadata": { "licenseurl": "https://creativecommons.org/licenses/by-nd/4.0/" },
            "files": [{ "name": "a.mp3", "format": "MP3" }]
        });
        assert!(tracks_from_metadata("x", &raw).is_empty());
    }

    #[test]
    fn bitrate_derivatives_collapse_onto_the_best_encoding() {
        let raw = json!({
            "metadata": { "licenseurl": "https://creativecommons.org/licenses/by/4.0/" },
            "files": [
                { "name": "001_sense.mp3", "format": "VBR MP3", "title": "Sense" },
                { "name": "001_sense_64kb.mp3", "format": "64Kbps MP3", "title": "Sense",
                  "original": "001_sense.mp3" },
                { "name": "001_sense_vbr.mp3", "format": "MP3", "title": "Sense",
                  "original": "001_sense.mp3" },
                { "name": "002_weed_64kb.mp3", "format": "64Kbps MP3", "title": "Weed" }
            ]
        });

        let tracks = tracks_from_metadata("album", &raw);
        assert_eq!(tracks.len(), 2);
        assert_eq!(
            tracks[0].preview_url.as_deref(),
            Some("https://archive.org/download/album/001_sense.mp3")
        );
        assert_eq!(tracks[1].name, "Weed");
    }

    #[test]
    fn derivatives_collapse_by_name_when_original_is_absent() {
        let raw = json!({
            "metadata": { "licenseurl": "https://creativecommons.org/publicdomain/zero/1.0/" },
            "files": [
                { "name": "track.mp3", "format": "VBR MP3" },
                { "name": "track_64kb.mp3", "format": "64Kbps MP3" }
            ]
        });
        assert_eq!(tracks_from_metadata("album", &raw).len(), 1);
    }

    #[test]
    fn durations_parse_from_both_clock_and_decimal_forms() {
        assert!((parse_duration_seconds(Some(&json!("1:02:03"))) - 3723.0).abs() < 1e-9);
        assert!((parse_duration_seconds(Some(&json!("42.5"))) - 42.5).abs() < 1e-9);
        assert_eq!(parse_duration_seconds(Some(&json!("nonsense"))), 0.0);
        assert_eq!(parse_duration_seconds(None), 0.0);
    }
}
