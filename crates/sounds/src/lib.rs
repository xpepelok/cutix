pub mod archive;
mod freesound;
pub mod http;
mod license;
mod mojibake;
mod openverse;

pub use license::{
    format_archive_license, format_openverse_license, is_edit_safe_license, is_edit_safe_openverse,
};
pub use mojibake::repair_mojibake;

use serde::{Deserialize, Serialize};

pub const USER_AGENT: &str = "cutix/0.3 (self-hosted editor)";

#[derive(Debug, thiserror::Error)]
pub enum SoundsError {
    #[error("request failed: {0}")]
    Request(String),
    #[error("unexpected response from {provider}")]
    Malformed { provider: &'static str },
    #[error("no provider returned results")]
    NoProviders,
    #[error("{0} is not configured")]
    NotConfigured(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SoundSource {
    Freesound,
    Openverse,
    Archive,
}

impl SoundSource {
    pub fn label(self) -> &'static str {
        match self {
            SoundSource::Freesound => "Freesound",
            SoundSource::Openverse => "Openverse",
            SoundSource::Archive => archive::PROVIDER,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Songs,
    Effects,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundResult {
    pub id: String,
    pub name: String,
    pub description: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub preview_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub download_url: Option<String>,
    pub duration: f64,
    pub filesize: u64,
    pub kind: String,
    pub username: String,
    pub tags: Vec<String>,
    pub license: String,
    pub created: String,
    pub source: SoundSource,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub license_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub landing_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub is_album: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub archive_identifier: Option<String>,
}

impl SoundResult {
    pub fn playable_url(&self) -> Option<&str> {
        self.preview_url.as_deref().or(self.download_url.as_deref())
    }
}

#[derive(Debug, Clone, Default)]
pub struct SearchPage {
    pub count: usize,
    pub has_next_page: bool,
    pub results: Vec<SoundResult>,
}

pub const DEFAULT_PAGE_SIZE: usize = 20;
const ARCHIVE_MIN_ALBUMS_PER_PAGE: usize = 6;

pub fn search_songs(query: &str, page: usize, page_size: usize) -> Result<SearchPage, SoundsError> {
    let archive_rows = ARCHIVE_MIN_ALBUMS_PER_PAGE.max(page_size.div_ceil(2));

    let openverse = openverse::search(query, page, page_size).ok();
    let archive = archive::search_albums(query, page, archive_rows).ok();

    if openverse.is_none() && archive.is_none() {
        return Err(SoundsError::NoProviders);
    }

    let archive_count = archive.as_ref().map(|page| page.count).unwrap_or(0);
    let has_next_page = openverse
        .as_ref()
        .map(|page| page.has_next_page)
        .unwrap_or(false)
        || page * archive_rows < archive_count;

    Ok(SearchPage {
        count: openverse.as_ref().map(|page| page.count).unwrap_or(0) + archive_count,
        has_next_page,
        results: interleave(
            openverse.map(|page| page.results).unwrap_or_default(),
            archive.map(|page| page.results).unwrap_or_default(),
            2,
        ),
    })
}

pub fn search_effects(
    api_key: Option<&str>,
    query: &str,
    page: usize,
    page_size: usize,
) -> Result<SearchPage, SoundsError> {
    let free = openverse::search_effects(query, page, page_size);

    let Some(key) = api_key.filter(|value| !value.trim().is_empty()) else {
        return free;
    };

    match freesound::search(key, query, page, page_size) {
        Ok(paid) if !paid.results.is_empty() => match free {
            Ok(free) => Ok(SearchPage {
                count: paid.count + free.count,
                has_next_page: paid.has_next_page || free.has_next_page,
                results: interleave(paid.results, free.results, 2),
            }),
            Err(_) => Ok(paid),
        },
        _ => free,
    }
}

pub fn interleave(
    primary: Vec<SoundResult>,
    secondary: Vec<SoundResult>,
    primary_stride: usize,
) -> Vec<SoundResult> {
    let mut merged = Vec::with_capacity(primary.len() + secondary.len());
    let mut primary = primary.into_iter().peekable();
    let mut secondary = secondary.into_iter().peekable();

    while primary.peek().is_some() || secondary.peek().is_some() {
        for _ in 0..primary_stride {
            if let Some(entry) = primary.next() {
                merged.push(entry);
            }
        }
        if let Some(entry) = secondary.next() {
            merged.push(entry);
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub(id: &str, source: SoundSource) -> SoundResult {
        SoundResult {
            id: id.to_string(),
            name: id.to_string(),
            description: String::new(),
            url: String::new(),
            preview_url: None,
            download_url: None,
            duration: 0.0,
            filesize: 0,
            kind: "mp3".to_string(),
            username: String::new(),
            tags: Vec::new(),
            license: String::new(),
            created: String::new(),
            source,
            license_url: None,
            landing_url: None,
            provider: None,
            is_album: false,
            archive_identifier: None,
        }
    }

    #[test]
    fn interleaving_puts_two_openverse_tracks_between_albums() {
        let primary = (0..5)
            .map(|index| stub(&format!("o{index}"), SoundSource::Openverse))
            .collect();
        let secondary = (0..2)
            .map(|index| stub(&format!("a{index}"), SoundSource::Archive))
            .collect();

        let ids: Vec<String> = interleave(primary, secondary, 2)
            .into_iter()
            .map(|entry| entry.id)
            .collect();
        assert_eq!(ids, ["o0", "o1", "a0", "o2", "o3", "a1", "o4"]);
    }

    #[test]
    fn interleaving_keeps_every_entry_when_one_side_is_empty() {
        let primary: Vec<SoundResult> = (0..3)
            .map(|index| stub(&format!("o{index}"), SoundSource::Openverse))
            .collect();
        assert_eq!(interleave(primary.clone(), Vec::new(), 2).len(), 3);
        assert_eq!(interleave(Vec::new(), primary, 2).len(), 3);
    }

    #[test]
    fn effects_are_served_by_a_key_free_provider() {
        let url = openverse::search_url(
            "whoosh",
            1,
            DEFAULT_PAGE_SIZE,
            Some(openverse::EFFECTS_CATEGORY),
        );
        assert!(url.starts_with("https://api.openverse.org/"));
        assert!(url.contains("category=sound_effect"));
        assert!(!url.to_lowercase().contains("api_key"));
        assert!(!url.to_lowercase().contains("token"));
    }

    #[test]
    fn playable_url_prefers_the_preview() {
        let mut result = stub("x", SoundSource::Archive);
        assert_eq!(result.playable_url(), None);
        result.download_url = Some("https://example.com/d.mp3".to_string());
        assert_eq!(result.playable_url(), Some("https://example.com/d.mp3"));
        result.preview_url = Some("https://example.com/p.mp3".to_string());
        assert_eq!(result.playable_url(), Some("https://example.com/p.mp3"));
    }
}
