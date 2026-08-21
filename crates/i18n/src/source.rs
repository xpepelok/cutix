use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const REPOSITORY: &str = "xpepelok/cutix";
pub const BRANCH: &str = "main";
pub const RAW_HOST: &str = "https://raw.githubusercontent.com";
pub const MANIFEST_FILE: &str = "index.json";
pub const STAMP_FILE: &str = ".fetched";
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

pub fn source_base() -> String {
    if let Ok(url) = std::env::var("CUTIX_LANG_URL") {
        let trimmed = url.trim_end_matches('/');
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let branch = std::env::var("CUTIX_LANG_BRANCH")
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| BRANCH.to_string());
    format!("{RAW_HOST}/{REPOSITORY}/{branch}/lang")
}

pub fn manifest_url() -> String {
    format!("{}/{MANIFEST_FILE}", source_base())
}

pub fn locale_url(file: &str) -> String {
    format!("{}/{file}", source_base())
}

#[derive(Clone, Debug, PartialEq)]
pub struct ManifestEntry {
    pub code: String,
    pub name: String,
    pub keys: usize,
    pub file: String,
}

pub fn is_safe_file(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && name.ends_with(".json")
}

fn is_valid_code(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 16
        && code
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

pub fn parse_manifest(raw: &str) -> Option<Vec<ManifestEntry>> {
    let value: serde_json::Value = serde_json::from_str(raw).ok()?;
    let locales = value.get("locales")?.as_array()?;

    let mut entries = Vec::new();
    for item in locales {
        let Some(code) = item.get("code").and_then(|value| value.as_str()) else {
            continue;
        };
        if !is_valid_code(code) {
            continue;
        }
        let file = item
            .get("file")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| format!("{code}.json"));
        if !is_safe_file(&file) {
            continue;
        }
        let name = item
            .get("name")
            .and_then(|value| value.as_str())
            .unwrap_or(code)
            .to_string();
        let keys = item
            .get("keys")
            .and_then(|value| value.as_u64())
            .unwrap_or(0) as usize;
        entries.push(ManifestEntry {
            code: code.to_string(),
            name,
            keys,
            file,
        });
    }

    if entries.is_empty() {
        return None;
    }
    Some(entries)
}

pub fn write_overlay(directory: impl AsRef<Path>, code: &str, raw: &str) -> Result<usize, String> {
    if !is_valid_code(code) {
        return Err(format!("unsafe locale code: {code}"));
    }
    let count = crate::install_locale(code, raw)?;
    let directory = directory.as_ref();
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    fs::write(directory.join(format!("{code}.json")), raw).map_err(|error| error.to_string())?;
    Ok(count)
}

fn stamp_path(directory: impl AsRef<Path>) -> PathBuf {
    directory.as_ref().join(STAMP_FILE)
}

pub fn mark_refreshed(directory: impl AsRef<Path>) {
    let directory = directory.as_ref();
    if fs::create_dir_all(directory).is_err() {
        return;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0);
    let _ = fs::write(stamp_path(directory), now.to_string());
}

pub fn last_refresh(directory: impl AsRef<Path>) -> Option<u64> {
    fs::read_to_string(stamp_path(directory))
        .ok()?
        .trim()
        .parse()
        .ok()
}

pub fn needs_refresh(directory: impl AsRef<Path>) -> bool {
    let Some(stamp) = last_refresh(directory) else {
        return true;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0);
    now.saturating_sub(stamp) >= REFRESH_INTERVAL.as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cutix-i18n-{name}"));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn default_urls_point_at_the_fork() {
        // Mutating the environment is unsound while another thread may be reading it.
        // These tests run single-threaded against a variable only this test touches.
        unsafe { std::env::remove_var("CUTIX_LANG_URL") };
        unsafe { std::env::remove_var("CUTIX_LANG_BRANCH") };
        assert_eq!(
            manifest_url(),
            "https://raw.githubusercontent.com/xpepelok/cutix/main/lang/index.json"
        );
        assert!(locale_url("ru.json").ends_with("/lang/ru.json"));

        unsafe { std::env::set_var("CUTIX_LANG_BRANCH", "ru-localization") };
        assert!(manifest_url().contains("/cutix/ru-localization/lang/"));

        unsafe { std::env::set_var("CUTIX_LANG_URL", "http://127.0.0.1:8321/lang/") };
        assert_eq!(manifest_url(), "http://127.0.0.1:8321/lang/index.json");
        assert_eq!(locale_url("zz.json"), "http://127.0.0.1:8321/lang/zz.json");

        unsafe { std::env::remove_var("CUTIX_LANG_URL") };
        unsafe { std::env::remove_var("CUTIX_LANG_BRANCH") };
    }

    #[test]
    fn parses_the_shared_manifest_format() {
        let raw = include_str!("../../../lang/index.json");
        let entries = parse_manifest(raw).expect("manifest");
        assert!(entries.iter().any(|entry| entry.code == "en"));
        let ru = entries
            .iter()
            .find(|entry| entry.code == "ru")
            .expect("ru entry");
        assert_eq!(ru.file, "ru.json");
        assert!(ru.keys > 0);
        assert!(!ru.name.is_empty());
    }

    #[test]
    fn rejects_traversal_in_manifest_entries() {
        let raw = r#"{"base":"en","locales":[{"code":"zz","file":"../../evil.json"}]}"#;
        assert!(parse_manifest(raw).is_none());
        assert!(!is_safe_file("../evil.json"));
        assert!(!is_safe_file("nested/evil.json"));
        assert!(is_safe_file("ru.json"));
    }

    #[test]
    fn rejects_broken_manifest() {
        assert!(parse_manifest("{ not json").is_none());
        assert!(parse_manifest(r#"{"base":"en"}"#).is_none());
        assert!(parse_manifest(r#"{"base":"en","locales":[]}"#).is_none());
    }

    #[test]
    fn writes_and_reloads_an_overlay_file() {
        let dir = scratch("write");
        let count =
            write_overlay(&dir, "wr", r#"{"$name":"Written","common.cancel":"W"}"#).expect("write");
        assert_eq!(count, 1);
        assert!(dir.join("wr.json").exists());
        assert_eq!(crate::translate("wr", "common.cancel"), "W");

        let loaded = crate::load_overlay_directory(&dir);
        assert_eq!(loaded, vec!["wr".to_string()]);
        assert_eq!(crate::locale_name("wr").as_deref(), Some("Written"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_to_write_a_broken_payload() {
        let dir = scratch("broken");
        assert!(write_overlay(&dir, "bk", "{ not json").is_err());
        assert!(!dir.join("bk.json").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_unsafe_locale_codes() {
        let dir = scratch("unsafe");
        assert!(write_overlay(&dir, "../evil", r#"{"a":"b"}"#).is_err());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn refresh_is_needed_until_stamped() {
        let dir = scratch("stamp");
        assert!(needs_refresh(&dir));
        mark_refreshed(&dir);
        assert!(!needs_refresh(&dir));
        assert!(last_refresh(&dir).is_some());
        let _ = fs::remove_dir_all(&dir);
    }
}
