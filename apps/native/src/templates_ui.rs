use std::path::{Path, PathBuf};

use cutix_i18n::t;
use template::TemplateManifest;

#[derive(Clone, Debug)]
pub struct TemplateEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub path: Option<PathBuf>,
    pub manifest: TemplateManifest,
}

impl TemplateEntry {
    /// Only the tests in this file ask for this; compiled for them alone so the
    /// shipping binary does not carry a method nothing calls.
    #[cfg(test)]
    pub fn has_timeline(&self) -> bool {
        cutix_project::template_project::parse_scenes(&self.manifest.scenes).is_some()
    }

    pub fn slot_count(&self) -> usize {
        self.manifest.slots.len()
    }
}

pub fn user_directory() -> Option<PathBuf> {
    dirs::data_local_dir().map(|base| base.join("cutix").join("templates"))
}

fn localized(key: &str, fallback: &str) -> String {
    let translated = t(key);
    if translated == key {
        fallback.to_owned()
    } else {
        translated
    }
}

fn builtin_key(name: &str) -> String {
    let mut camel = String::new();
    let words = name
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty());
    for (index, word) in words.enumerate() {
        let mut characters = word.chars();
        let Some(first) = characters.next() else {
            continue;
        };
        if index == 0 {
            camel.extend(first.to_lowercase());
        } else {
            camel.extend(first.to_uppercase());
        }
        camel.extend(characters.flat_map(|character| character.to_lowercase()));
    }
    camel
}

pub fn builtin_entries() -> Vec<TemplateEntry> {
    template::builtin_templates()
        .into_iter()
        .map(|manifest| {
            let key = builtin_key(&manifest.name);
            TemplateEntry {
                id: format!("builtin:{key}"),
                name: localized(&format!("templates.builtin.{key}.name"), &manifest.name),
                description: localized(
                    &format!("templates.builtin.{key}.description"),
                    &manifest.description,
                ),
                path: None,
                manifest,
            }
        })
        .collect()
}

pub fn user_entries() -> Vec<TemplateEntry> {
    let Some(directory) = user_directory() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };

    let mut templates: Vec<TemplateEntry> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                return None;
            }
            let raw = std::fs::read_to_string(&path).ok()?;
            let manifest = template::parse(&raw).ok()?;
            Some(TemplateEntry {
                id: format!("user:{}", path.file_stem()?.to_string_lossy()),
                name: manifest.name.clone(),
                description: manifest.description.clone(),
                path: Some(path),
                manifest,
            })
        })
        .collect();
    templates.sort_by_key(|left| left.name.to_lowercase());
    templates
}

fn slugify(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = slug.trim_matches('-').to_owned();
    if trimmed.is_empty() {
        String::from("template")
    } else {
        trimmed
    }
}

pub fn save_user_template(manifest: &TemplateManifest) -> std::io::Result<PathBuf> {
    let directory =
        user_directory().ok_or_else(|| std::io::Error::other("no application data directory"))?;
    std::fs::create_dir_all(&directory)?;

    let stem = slugify(&manifest.name);
    let mut path = directory.join(format!("{stem}.json"));
    let mut suffix = 2;
    while path.exists() {
        path = directory.join(format!("{stem}-{suffix}.json"));
        suffix += 1;
    }

    let raw = template::to_json(manifest).map_err(std::io::Error::other)?;
    std::fs::write(&path, raw)?;
    Ok(path)
}

pub fn delete_user_template(path: &Path) -> std::io::Result<()> {
    std::fs::remove_file(path)
}

pub fn import_capcut_draft(path: &Path) -> Result<TemplateManifest, String> {
    let draft = capcut::load_draft(path).map_err(|error| error.to_string())?;
    capcut::draft_to_template(&draft).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_become_translation_keys() {
        assert_eq!(builtin_key("Beat slideshow"), "beatSlideshow");
        assert_eq!(builtin_key("Green screen overlay"), "greenScreenOverlay");
        assert_eq!(builtin_key("Talking head, vertical"), "talkingHeadVertical");
        assert_eq!(builtin_key("Speed ramp intro"), "speedRampIntro");
    }

    #[test]
    fn every_builtin_template_key_exists_in_the_dictionary() {
        for manifest in template::builtin_templates() {
            let key = format!("templates.builtin.{}.name", builtin_key(&manifest.name));
            assert_ne!(cutix_i18n::t(&key), key, "missing translation for {key}");
        }
    }

    #[test]
    fn every_builtin_template_has_a_translated_name_and_slots() {
        let entries = builtin_entries();
        assert_eq!(entries.len(), template::builtin_names().len());
        for entry in entries {
            assert!(!entry.name.is_empty(), "{}", entry.id);
            assert!(
                !entry.name.starts_with("templates.builtin."),
                "untranslated: {}",
                entry.name
            );
            assert!(entry.slot_count() > 0, "{}", entry.id);
            assert!(!entry.has_timeline(), "{}", entry.id);
        }
    }

    #[test]
    fn slugs_are_safe_file_names() {
        assert_eq!(slugify("My Template"), "my-template");
        assert_eq!(slugify("  ***  "), "template");
        assert_eq!(slugify("Проект"), "template");
    }

    #[test]
    fn the_user_directory_sits_under_the_app_data_folder() {
        let directory = user_directory().expect("data directory");
        assert!(directory.ends_with("cutix/templates") || directory.ends_with("cutix\\templates"));
    }

    #[test]
    fn a_missing_capcut_draft_reports_instead_of_panicking() {
        let error = import_capcut_draft(Path::new("does-not-exist.json")).unwrap_err();
        assert!(!error.is_empty());
    }
}
