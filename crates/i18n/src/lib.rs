use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

include!(concat!(env!("OUT_DIR"), "/locales.rs"));

mod source;

pub use source::{
    BRANCH, MANIFEST_FILE, ManifestEntry, REFRESH_INTERVAL, REPOSITORY, is_safe_file, last_refresh,
    locale_url, manifest_url, mark_refreshed, needs_refresh, parse_manifest, source_base,
    write_overlay,
};

pub const BASE_LOCALE: &str = "en";
pub const DEFAULT_LOCALE: &str = BASE_LOCALE;
pub const FALLBACK_LOCALE: &str = BASE_LOCALE;

type Dictionary = HashMap<String, String>;

fn dictionaries() -> &'static HashMap<&'static str, Dictionary> {
    static DICTIONARIES: OnceLock<HashMap<&'static str, Dictionary>> = OnceLock::new();
    DICTIONARIES.get_or_init(|| {
        EMBEDDED
            .iter()
            .map(|(locale, raw)| (*locale, parse(raw)))
            .collect()
    })
}

fn raw_values() -> &'static HashMap<&'static str, HashMap<String, serde_json::Value>> {
    static RAW: OnceLock<HashMap<&'static str, HashMap<String, serde_json::Value>>> =
        OnceLock::new();
    RAW.get_or_init(|| {
        EMBEDDED
            .iter()
            .map(|(locale, raw)| (*locale, serde_json::from_str(raw).unwrap_or_default()))
            .collect()
    })
}

fn overlay() -> &'static RwLock<HashMap<String, (Dictionary, String)>> {
    static OVERLAY: OnceLock<RwLock<HashMap<String, (Dictionary, String)>>> = OnceLock::new();
    OVERLAY.get_or_init(|| RwLock::new(HashMap::new()))
}

pub fn overlay_directory() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".cache"))
        })
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("cutix").join("lang")
}

pub fn install_locale(code: &str, raw: &str) -> Result<usize, String> {
    let parsed: HashMap<String, serde_json::Value> =
        serde_json::from_str(raw).map_err(|error| error.to_string())?;

    let name = parsed
        .get("$name")
        .and_then(|value| value.as_str())
        .unwrap_or(code)
        .to_string();

    let dictionary: Dictionary = parsed
        .into_iter()
        .filter(|(key, _)| !key.starts_with('$'))
        .filter_map(|(key, value)| value.as_str().map(|text| (key, text.to_string())))
        .collect();

    if dictionary.is_empty() {
        return Err("locale contains no usable strings".to_string());
    }

    let count = dictionary.len();
    overlay()
        .write()
        .map_err(|_| "locale storage is poisoned".to_string())?
        .insert(code.to_string(), (dictionary, name));
    Ok(count)
}

pub fn load_overlay_directory(directory: impl AsRef<Path>) -> Vec<String> {
    let mut loaded = Vec::new();
    let Ok(entries) = fs::read_dir(directory) else {
        return loaded;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
            continue;
        };
        if stem == "index" {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        if install_locale(stem, &raw).is_ok() {
            loaded.push(stem.to_string());
        }
    }

    loaded.sort();
    loaded
}

pub fn bootstrap() -> Vec<String> {
    load_overlay_directory(overlay_directory())
}

fn parse(raw: &str) -> Dictionary {
    serde_json::from_str::<HashMap<String, serde_json::Value>>(raw)
        .unwrap_or_default()
        .into_iter()
        .filter(|(key, _)| !key.starts_with('$'))
        .filter_map(|(key, value)| value.as_str().map(|text| (key, text.to_string())))
        .collect()
}

fn current() -> &'static RwLock<String> {
    static CURRENT: OnceLock<RwLock<String>> = OnceLock::new();
    CURRENT.get_or_init(|| RwLock::new(DEFAULT_LOCALE.to_string()))
}

pub fn available_locales() -> Vec<String> {
    let mut locales: Vec<String> = dictionaries().keys().map(|code| code.to_string()).collect();
    if let Ok(overlay) = overlay().read() {
        for code in overlay.keys() {
            if !locales.contains(code) {
                locales.push(code.clone());
            }
        }
    }
    locales.sort();
    locales
}

pub fn locale_name(locale: &str) -> Option<String> {
    if let Ok(overlay) = overlay().read() {
        if let Some((_, name)) = overlay.get(locale) {
            return Some(name.clone());
        }
    }
    raw_values()
        .get(locale)?
        .get("$name")?
        .as_str()
        .map(str::to_string)
}

pub fn coverage(locale: &str) -> f32 {
    let all = dictionaries();
    let Some(base) = all.get(BASE_LOCALE) else {
        return 0.0;
    };
    let Some(dictionary) = all.get(locale) else {
        return 0.0;
    };
    if base.is_empty() {
        return 0.0;
    }
    let translated = base
        .keys()
        .filter(|key| dictionary.contains_key(*key))
        .count();
    translated as f32 / base.len() as f32
}

pub fn locale() -> String {
    current()
        .read()
        .map(|guard| guard.clone())
        .unwrap_or_else(|_| DEFAULT_LOCALE.to_string())
}

pub fn set_locale(locale: &str) -> bool {
    let known = dictionaries().contains_key(locale)
        || overlay()
            .read()
            .map(|overlay| overlay.contains_key(locale))
            .unwrap_or(false);
    if !known {
        return false;
    }
    match current().write() {
        Ok(mut guard) => {
            *guard = locale.to_string();
            true
        }
        Err(_) => false,
    }
}

pub fn t(key: &str) -> String {
    lookup(&locale(), key)
}

pub fn t_args(key: &str, args: &[(&str, &str)]) -> String {
    interpolate(&t(key), args)
}

pub fn translate(locale: &str, key: &str) -> String {
    lookup(locale, key)
}

fn lookup(locale: &str, key: &str) -> String {
    if let Ok(overlay) = overlay().read() {
        if let Some(value) = overlay
            .get(locale)
            .and_then(|(dictionary, _)| dictionary.get(key))
        {
            return value.clone();
        }
    }

    let all = dictionaries();
    if let Some(value) = all.get(locale).and_then(|dictionary| dictionary.get(key)) {
        return value.clone();
    }

    if let Ok(overlay) = overlay().read() {
        if let Some(value) = overlay
            .get(FALLBACK_LOCALE)
            .and_then(|(dictionary, _)| dictionary.get(key))
        {
            return value.clone();
        }
    }

    all.get(FALLBACK_LOCALE)
        .and_then(|dictionary| dictionary.get(key))
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

fn interpolate(template: &str, args: &[(&str, &str)]) -> String {
    let mut result = template.to_string();
    for (name, value) in args {
        result = result.replace(&format!("{{{name}}}"), value);
    }
    result
}

#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::t($key)
    };
    ($key:expr, $($name:literal => $value:expr),+ $(,)?) => {
        $crate::t_args($key, &[$(($name, &$value.to_string())),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_locales_from_lang_dir() {
        let locales = available_locales();
        assert!(locales.iter().any(|code| code == "en"));
        assert!(locales.len() >= 2);
    }

    #[test]
    fn the_manifest_is_not_a_locale() {
        assert!(!available_locales().iter().any(|code| code == "index"));
        assert!(locale_name("index").is_none());
        assert!(!set_locale("index"));
    }

    #[test]
    fn base_locale_is_present() {
        assert!(dictionaries().contains_key(BASE_LOCALE));
    }

    #[test]
    fn unknown_key_returns_key() {
        assert_eq!(translate(BASE_LOCALE, "nope.nope"), "nope.nope");
    }

    #[test]
    fn falls_back_to_base_locale_per_key() {
        let all = dictionaries();
        let base = &all[BASE_LOCALE];
        for (locale, dictionary) in all.iter() {
            if *locale == BASE_LOCALE {
                continue;
            }
            for key in base.keys() {
                if !dictionary.contains_key(key) {
                    assert_eq!(translate(locale, key), base[key]);
                    return;
                }
            }
        }
    }

    #[test]
    fn coverage_of_base_locale_is_full() {
        assert_eq!(coverage(BASE_LOCALE), 1.0);
    }

    #[test]
    fn installed_locale_appears_in_the_list() {
        install_locale("zz", r#"{"$name":"Test","common.cancel":"Nope"}"#).expect("install");
        assert!(available_locales().iter().any(|code| code == "zz"));
        assert_eq!(locale_name("zz").as_deref(), Some("Test"));
    }

    #[test]
    fn installed_locale_wins_over_embedded() {
        install_locale("en2", r#"{"$name":"Second","common.cancel":"Override"}"#).expect("install");
        assert_eq!(translate("en2", "common.cancel"), "Override");
    }

    #[test]
    fn missing_keys_still_fall_back_to_english() {
        install_locale("partial", r#"{"$name":"Partial","common.cancel":"X"}"#).expect("install");
        let base = translate(BASE_LOCALE, "common.save");
        assert_eq!(translate("partial", "common.save"), base);
    }

    #[test]
    fn broken_payload_is_rejected() {
        assert!(install_locale("bad", "{ not json").is_err());
        assert!(!available_locales().iter().any(|code| code == "bad"));
    }

    #[test]
    fn empty_locale_is_rejected() {
        assert!(install_locale("empty", r#"{"$name":"Empty"}"#).is_err());
    }

    #[test]
    fn installed_locale_can_be_selected() {
        install_locale("sel", r#"{"$name":"Sel","common.cancel":"Y"}"#).expect("install");
        assert!(set_locale("sel"));
        assert_eq!(locale(), "sel");
        assert!(set_locale(BASE_LOCALE));
    }

    #[test]
    fn unknown_locale_cannot_be_selected() {
        assert!(!set_locale("definitely-not-here"));
    }

    #[test]
    fn missing_overlay_directory_loads_nothing() {
        assert!(load_overlay_directory("no-such-directory").is_empty());
    }

    #[test]
    fn overlay_directory_is_under_the_app_folder() {
        assert!(overlay_directory().ends_with("lang"));
    }

    #[test]
    fn interpolates_named_args() {
        assert_eq!(
            interpolate("Created {date}", &[("date", "10.08.2026")]),
            "Created 10.08.2026"
        );
    }
}
