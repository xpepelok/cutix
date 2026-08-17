use cutix_project::model::Attribution;
use serde::{Deserialize, Serialize};
use sounds::{SoundResult, SoundSource};
use std::path::PathBuf;

pub const MODES: &[(&str, &str)] = &[
    ("music", "sounds.music"),
    ("effects", "sounds.effects"),
    ("saved", "sounds.saved"),
];

pub const MODE_MUSIC: usize = 0;
pub const MODE_EFFECTS: usize = 1;
pub const MODE_SAVED: usize = 2;

pub const FREESOUND_KEY_VARIABLE: &str = "CUTIX_FREESOUND_API_KEY";

pub fn freesound_key() -> Option<String> {
    std::env::var(FREESOUND_KEY_VARIABLE)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedSounds {
    #[serde(default)]
    pub entries: Vec<SoundResult>,
}

pub fn saved_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base| base.join("cutix").join("native-sounds.json"))
}

pub fn load_saved() -> SavedSounds {
    saved_path()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save_saved(saved: &SavedSounds) {
    let Some(path) = saved_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(saved) {
        let _ = std::fs::write(path, bytes);
    }
}

impl SavedSounds {
    pub fn contains(&self, id: &str) -> bool {
        self.entries.iter().any(|entry| entry.id == id)
    }

    pub fn toggle(&mut self, result: &SoundResult) -> bool {
        match self.entries.iter().position(|entry| entry.id == result.id) {
            Some(index) => {
                self.entries.remove(index);
                false
            }
            None => {
                self.entries.push(result.clone());
                true
            }
        }
    }
}

pub fn attribution_for(result: &SoundResult, added_at: String) -> Attribution {
    Attribution {
        id: result.id.clone(),
        title: result.name.clone(),
        creator: result.username.clone(),
        license: result.license.clone(),
        license_url: result.license_url.clone(),
        source_url: result
            .landing_url
            .clone()
            .or_else(|| Some(result.url.clone()))
            .filter(|value| !value.is_empty()),
        provider: result
            .provider
            .clone()
            .or_else(|| Some(result.source.label().to_string())),
        added_at,
    }
}

pub fn credit_line(result: &SoundResult) -> String {
    let provider = result
        .provider
        .clone()
        .unwrap_or_else(|| result.source.label().to_string());
    if result.license.is_empty() {
        return provider;
    }
    format!("{} · {}", result.license, provider)
}

pub fn duration_label(result: &SoundResult) -> Option<String> {
    if result.duration <= 0.0 {
        return None;
    }
    let total = result.duration.round() as u64;
    Some(format!("{}:{:02}", total / 60, total % 60))
}

pub fn source_glyph(source: SoundSource) -> &'static str {
    match source {
        SoundSource::Archive => "folder03",
        SoundSource::Openverse => "music-note03",
        SoundSource::Freesound => "headphones",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub(id: &str) -> SoundResult {
        SoundResult {
            id: id.to_string(),
            name: "Dawn".to_string(),
            description: String::new(),
            url: "https://archive.org/details/album".to_string(),
            preview_url: Some("https://archive.org/download/album/a.mp3".to_string()),
            download_url: Some("https://archive.org/download/album/a.mp3".to_string()),
            duration: 222.0,
            filesize: 0,
            kind: "mp3".to_string(),
            username: "Netlabel".to_string(),
            tags: Vec::new(),
            license: "CC BY 4.0".to_string(),
            created: String::new(),
            source: SoundSource::Archive,
            license_url: Some("https://creativecommons.org/licenses/by/4.0/".to_string()),
            landing_url: Some("https://archive.org/details/album".to_string()),
            provider: Some("Internet Archive".to_string()),
            is_album: false,
            archive_identifier: Some("album".to_string()),
        }
    }

    #[test]
    fn the_mode_tabs_match_the_web_views() {
        let keys: Vec<&str> = MODES.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys, ["music", "effects", "saved"]);
    }

    #[test]
    fn attribution_carries_the_creator_licence_and_source() {
        let attribution = attribution_for(&stub("archive:album:a.mp3"), "2026-08-14".to_string());
        assert_eq!(attribution.id, "archive:album:a.mp3");
        assert_eq!(attribution.title, "Dawn");
        assert_eq!(attribution.creator, "Netlabel");
        assert_eq!(attribution.license, "CC BY 4.0");
        assert_eq!(
            attribution.license_url.as_deref(),
            Some("https://creativecommons.org/licenses/by/4.0/")
        );
        assert_eq!(
            attribution.source_url.as_deref(),
            Some("https://archive.org/details/album")
        );
        assert_eq!(attribution.provider.as_deref(), Some("Internet Archive"));
        assert_eq!(attribution.added_at, "2026-08-14");
    }

    #[test]
    fn attribution_falls_back_to_the_source_label_without_a_provider() {
        let mut result = stub("x");
        result.provider = None;
        result.landing_url = None;
        let attribution = attribution_for(&result, String::new());
        assert_eq!(attribution.provider.as_deref(), Some("Internet Archive"));
        assert_eq!(
            attribution.source_url.as_deref(),
            Some("https://archive.org/details/album")
        );
    }

    #[test]
    fn saving_toggles_and_survives_a_round_trip() {
        let mut saved = SavedSounds::default();
        let entry = stub("a");
        assert!(saved.toggle(&entry));
        assert!(saved.contains("a"));
        assert_eq!(saved.entries.len(), 1);

        let raw = serde_json::to_vec(&saved).expect("serialize");
        let restored: SavedSounds = serde_json::from_slice(&raw).expect("deserialize");
        assert_eq!(restored.entries.len(), 1);
        assert_eq!(restored.entries[0].license, "CC BY 4.0");

        assert!(!saved.toggle(&entry));
        assert!(!saved.contains("a"));
        assert!(saved.entries.is_empty());
    }

    #[test]
    fn credit_lines_join_the_licence_and_provider() {
        assert_eq!(credit_line(&stub("a")), "CC BY 4.0 · Internet Archive");
        let mut bare = stub("a");
        bare.license = String::new();
        assert_eq!(credit_line(&bare), "Internet Archive");
    }

    #[test]
    fn durations_render_as_minutes_and_seconds() {
        assert_eq!(duration_label(&stub("a")).as_deref(), Some("3:42"));
        let mut album = stub("a");
        album.duration = 0.0;
        assert_eq!(duration_label(&album), None);
    }
}
