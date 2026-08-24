use std::collections::HashMap;
use std::sync::Arc;

use cutix_i18n::t;
use gpui::{RenderImage, SharedString};

pub const CATEGORIES: &[(&str, &str)] = &[
    ("all", "stickers.category.all"),
    ("flags", "stickers.category.flags"),
    ("shapes", "stickers.category.shapes"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct StickerEntry {
    pub sticker_id: String,
    pub name: String,
    pub category: &'static str,
    pub search: String,
}

fn flag_entries() -> Vec<StickerEntry> {
    stickers::countries()
        .iter()
        .map(|country| {
            let search = format!(
                "{} {} {} {}",
                country.name.to_lowercase(),
                country.code.to_lowercase(),
                country.region.to_lowercase(),
                country.languages.join(" ").to_lowercase()
            );
            StickerEntry {
                sticker_id: stickers::build_sticker_id("flags", &country.code),
                name: country.name.clone(),
                category: "flags",
                search,
            }
        })
        .collect()
}

fn shape_entries() -> Vec<StickerEntry> {
    stickers::DEFINITIONS
        .iter()
        .map(|definition| {
            let name = t(definition.name_key);
            let search = format!(
                "{} {} {}",
                name.to_lowercase(),
                definition.id,
                definition.keywords.join(" ")
            );
            StickerEntry {
                sticker_id: stickers::build_sticker_id("shapes", definition.id),
                name,
                category: "shapes",
                search,
            }
        })
        .collect()
}

pub fn catalogue() -> Vec<StickerEntry> {
    let mut entries = shape_entries();
    entries.extend(flag_entries());
    entries
}

pub fn filtered(entries: &[StickerEntry], category: &str, query: &str) -> Vec<StickerEntry> {
    let needle = query.trim().to_lowercase();
    entries
        .iter()
        .filter(|entry| category == "all" || entry.category == category)
        .filter(|entry| needle.is_empty() || entry.search.contains(&needle))
        .cloned()
        .collect()
}

pub fn flag_asset_path(sticker_id: &str) -> Option<SharedString> {
    let parsed = stickers::parse_sticker_id(sticker_id)?;
    (parsed.provider == "flags").then(|| SharedString::from(format!("flags/{}.svg", parsed.value)))
}

pub fn graphic_definition_for(sticker_id: &str) -> Option<&'static str> {
    match stickers::sticker_source(sticker_id)? {
        stickers::StickerSource::Graphic { definition_id, .. } => Some(definition_id),
        stickers::StickerSource::Svg(_) => None,
    }
}

const PREVIEW_SIZE: u32 = 128;

#[derive(Default)]
pub struct ShapePreviews {
    images: HashMap<String, Arc<RenderImage>>,
}

impl ShapePreviews {
    pub fn get(&mut self, sticker_id: &str) -> Option<Arc<RenderImage>> {
        if let Some(image) = self.images.get(sticker_id) {
            return Some(image.clone());
        }
        let raster = stickers::rasterize_sticker(sticker_id, PREVIEW_SIZE, PREVIEW_SIZE).ok()?;
        let mut bgra = raster.rgba;
        for pixel in bgra.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        let buffer = image::ImageBuffer::from_raw(raster.width, raster.height, bgra)?;
        let frame = image::Frame::new(buffer);
        let render = Arc::new(RenderImage::new(smallvec::smallvec![frame]));
        self.images.insert(sticker_id.to_owned(), render.clone());
        Some(render)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_carries_every_shape_and_flag() {
        let entries = catalogue();
        assert_eq!(
            entries.len(),
            stickers::DEFINITIONS.len() + stickers::countries().len()
        );
        assert!(entries.iter().any(|entry| entry.sticker_id == "flags:DE"));
        assert!(entries
            .iter()
            .any(|entry| entry.sticker_id == "shapes:star"));
    }

    #[test]
    fn filtering_narrows_by_category_and_query() {
        let entries = catalogue();
        let shapes = filtered(&entries, "shapes", "");
        assert_eq!(shapes.len(), stickers::DEFINITIONS.len());

        let germany = filtered(&entries, "all", "germany");
        assert_eq!(germany.len(), 1);
        assert_eq!(germany[0].sticker_id, "flags:DE");
        assert!(filtered(&entries, "all", "german").len() > 1);

        let by_code = filtered(&entries, "flags", "jp");
        assert!(by_code.iter().any(|entry| entry.sticker_id == "flags:JP"));

        assert!(filtered(&entries, "all", "zzzznothing").is_empty());
    }

    #[test]
    fn every_catalogue_entry_resolves_to_a_raster() {
        for entry in catalogue() {
            assert!(
                stickers::sticker_source(&entry.sticker_id).is_some(),
                "{}",
                entry.sticker_id
            );
        }
    }

    #[test]
    fn shape_ids_resolve_to_a_graphics_definition_and_flags_do_not() {
        assert_eq!(graphic_definition_for("shapes:star"), Some("star"));
        assert_eq!(graphic_definition_for("shapes:triangle"), Some("polygon"));
        assert_eq!(graphic_definition_for("flags:JP"), None);
        assert_eq!(graphic_definition_for("nonsense"), None);
    }

    #[test]
    fn only_flags_use_the_asset_source() {
        assert_eq!(
            flag_asset_path("flags:DE").unwrap().as_ref(),
            "flags/DE.svg"
        );
        assert!(flag_asset_path("shapes:star").is_none());
    }

    #[test]
    fn every_category_has_a_translated_label() {
        for (_, key) in CATEGORIES {
            assert_ne!(t(key), *key, "{key}");
        }
    }
}
