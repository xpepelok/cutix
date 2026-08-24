use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};

macro_rules! icons {
    ($($name:literal),+ $(,)?) => {
        &[$(($name, include_bytes!(concat!("../assets/icons/", $name, ".svg")).as_slice())),+]
    };
}

const ICONS: &[(&str, &[u8])] = icons![
    "alert-circle",
    "arrow-right-double",
    "arrow-up",
    "arrows-vertical",
    "closed-caption",
    "command",
    "copy01",
    "delete02",
    "folder03",
    "full-screen",
    "grid-view",
    "happy01",
    "headphones",
    "layers01",
    "magic-wand05",
    "mic01",
    "play",
    "scissor",
    "align-left",
    "align-right",
    "bookmark02",
    "chart03",
    "cloud-upload",
    "eye",
    "eye-off",
    "left-to-right-list-dash",
    "link02",
    "magnet",
    "search-add",
    "search-minus",
    "settings01",
    "settings05",
    "snow",
    "sorting-one-nine",
    "upload04",
    "video01",
    "volume-high",
    "volume-mute",
    "sliders-horizontal",
    "text",
    "arrow-down",
    "grid-view-alt",
    "pause",
    "sorting-nine-one",
    "chevron-left",
    "chevron-right",
    "languages",
    "music-note03",
    "cutix-logo",
    "oc-ripple",
    "sun03",
    "tick02",
    "win-close",
    "win-maximize",
    "win-minimize",
    "win-restore",
    "search01",
    "plus-sign",
    "more-horizontal",
    "edit03",
    "information-circle",
    "calendar04",
    "moon02",
    "oc-video",
    "keyframe",
    "keyframe-filled",
    "arrow-expand",
    "crop",
    "rain-drop",
    "dashboard-speed",
    "rotate-clockwise",
    "link05",
    "arrow-turn-backward",
    "text-font",
    "align-center",
    "checkerboard",
    "text-width",
    "text-height",
    "flip-horizontal",
    "flip-vertical",
];

pub struct Icons;

pub fn icon(name: &str) -> SharedString {
    SharedString::from(format!("icons/{name}.svg"))
}

impl AssetSource for Icons {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(code) = path
            .strip_prefix("flags/")
            .and_then(|rest| rest.strip_suffix(".svg"))
        {
            return Ok(stickers::flag_svg(code).map(Cow::Borrowed));
        }
        let key = path.trim_start_matches("icons/").trim_end_matches(".svg");
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        if path.starts_with("flags") {
            return Ok(stickers::countries()
                .iter()
                .map(|country| SharedString::from(format!("flags/{}.svg", country.code)))
                .collect());
        }
        Ok(ICONS.iter().map(|(name, _)| icon(name)).collect())
    }
}

/// Only the tests in this file ask this; compiled for them alone so the shipping
/// binary does not carry a function nothing calls.
#[cfg(test)]
pub fn icon_exists(name: &str) -> bool {
    ICONS.iter().any(|(known, _)| *known == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_resolves() {
        for (name, _) in ICONS {
            assert!(Icons.load(&icon(name)).unwrap().is_some(), "{name}");
        }
    }

    #[test]
    fn unknown_icon_is_absent() {
        assert!(Icons.load("icons/not-here.svg").unwrap().is_none());
    }

    #[test]
    fn every_shipped_svg_is_registered() {
        let directory = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icons");
        let mut missing = Vec::new();

        for entry in std::fs::read_dir(directory).expect("assets/icons") {
            let path = entry.expect("dir entry").path();
            if path.extension().and_then(|value| value.to_str()) != Some("svg") {
                continue;
            }
            let name = path
                .file_stem()
                .and_then(|value| value.to_str())
                .expect("file stem")
                .to_string();
            if !ICONS.iter().any(|(known, _)| *known == name) {
                missing.push(name);
            }
        }

        assert!(missing.is_empty(), "unregistered icons: {missing:?}");
    }
}

#[cfg(test)]
mod flag_tests {
    use super::*;

    #[test]
    fn every_country_flag_loads_through_the_asset_source() {
        let mut missing = Vec::new();
        for country in stickers::countries() {
            let path = format!("flags/{}.svg", country.code);
            if Icons.load(&path).unwrap().is_none() {
                missing.push(country.code.clone());
            }
        }
        assert!(
            missing.is_empty(),
            "flags missing from the asset source: {missing:?}"
        );
    }

    #[test]
    fn listing_flags_covers_the_catalogue() {
        let listed = Icons.list("flags").unwrap();
        assert_eq!(listed.len(), stickers::countries().len());
        assert!(listed.iter().any(|path| path.as_ref() == "flags/JP.svg"));
    }

    #[test]
    fn an_unknown_flag_is_absent_rather_than_an_icon() {
        assert!(Icons.load("flags/ZZ.svg").unwrap().is_none());
    }
}
