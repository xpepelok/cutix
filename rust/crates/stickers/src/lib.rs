pub mod graphics;

use std::sync::OnceLock;

use serde::Deserialize;
use tiny_skia::{Pixmap, Transform};

pub use graphics::{
    default_params, definition, parse_hex_color, render_graphic, resolve_params, GraphicDefinition,
    ParamDefinition, ParamKind, ParamValues, Raster, DEFAULT_GRAPHIC_SOURCE_SIZE, DEFINITIONS,
    LEGACY_SHAPE_PRESETS,
};

include!(concat!(env!("OUT_DIR"), "/flags.rs"));

pub const DEFAULT_INTRINSIC_SIZE: f64 = 200.0;

#[derive(Clone, Debug, Deserialize)]
pub struct Country {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub languages: Vec<String>,
}

const COUNTRIES_JSON: &str = include_str!("../assets/countries.json");

pub fn countries() -> &'static [Country] {
    static COUNTRIES: OnceLock<Vec<Country>> = OnceLock::new();
    COUNTRIES.get_or_init(|| serde_json::from_str(COUNTRIES_JSON).expect("countries.json"))
}

pub const REGION_GROUPS: &[(&str, &[&str])] = &[
    (
        "europe",
        &[
            "Western Europe",
            "Eastern Europe",
            "Northern Europe",
            "Southern Europe",
        ],
    ),
    (
        "asia",
        &["South Asia", "Southeast Asia", "East Asia", "Central Asia"],
    ),
    ("africa", &["Sub-Saharan Africa", "North Africa"]),
    (
        "america",
        &[
            "North America",
            "South America",
            "Central America",
            "Caribbean",
        ],
    ),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StickerId {
    pub provider: String,
    pub value: String,
}

pub fn parse_sticker_id(raw: &str) -> Option<StickerId> {
    let (provider, value) = raw.split_once(':')?;
    if provider.is_empty() || value.is_empty() {
        return None;
    }
    Some(StickerId {
        provider: provider.to_owned(),
        value: value.to_owned(),
    })
}

pub fn build_sticker_id(provider: &str, value: &str) -> String {
    format!("{provider}:{value}")
}

pub fn flag_svg(code: &str) -> Option<&'static [u8]> {
    let upper = code.to_uppercase();
    FLAG_SVGS
        .iter()
        .find(|(name, _)| *name == upper)
        .map(|(_, bytes)| *bytes)
}

pub fn sticker_svg(sticker_id: &str) -> Option<&'static [u8]> {
    let parsed = parse_sticker_id(sticker_id)?;
    match parsed.provider.as_str() {
        "flags" => flag_svg(&parsed.value),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum StickerSource {
    Svg(&'static [u8]),
    Graphic {
        definition_id: &'static str,
        preset: &'static [(&'static str, f64)],
    },
}

pub fn sticker_source(sticker_id: &str) -> Option<StickerSource> {
    let parsed = parse_sticker_id(sticker_id)?;
    match parsed.provider.as_str() {
        "flags" => flag_svg(&parsed.value).map(StickerSource::Svg),
        "shapes" => {
            if let Some(definition) = definition(&parsed.value) {
                return Some(StickerSource::Graphic {
                    definition_id: definition.id,
                    preset: &[],
                });
            }
            LEGACY_SHAPE_PRESETS
                .iter()
                .find(|(legacy, _, _)| *legacy == parsed.value)
                .map(|(_, definition_id, preset)| StickerSource::Graphic {
                    definition_id,
                    preset,
                })
        }
        _ => None,
    }
}

pub fn sticker_intrinsic_size(sticker_id: &str) -> Option<(f64, f64)> {
    match sticker_source(sticker_id)? {
        StickerSource::Svg(bytes) => {
            let tree = usvg::Tree::from_data(bytes, &usvg::Options::default()).ok()?;
            let size = tree.size();
            Some((size.width() as f64, size.height() as f64))
        }
        StickerSource::Graphic { .. } => Some((DEFAULT_INTRINSIC_SIZE, DEFAULT_INTRINSIC_SIZE)),
    }
}

#[derive(Debug)]
pub enum StickerError {
    UnknownSticker(String),
    Svg(String),
    Raster,
}

impl std::fmt::Display for StickerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSticker(id) => write!(formatter, "unknown sticker '{id}'"),
            Self::Svg(message) => write!(formatter, "sticker svg: {message}"),
            Self::Raster => write!(formatter, "sticker raster allocation failed"),
        }
    }
}

impl std::error::Error for StickerError {}

pub fn rasterize_svg(bytes: &[u8], width: u32, height: u32) -> Result<Raster, StickerError> {
    let width = width.max(1);
    let height = height.max(1);
    let tree = usvg::Tree::from_data(bytes, &usvg::Options::default())
        .map_err(|error| StickerError::Svg(error.to_string()))?;
    let size = tree.size();
    let scale_x = width as f32 / size.width().max(f32::EPSILON);
    let scale_y = height as f32 / size.height().max(f32::EPSILON);
    let mut pixmap = Pixmap::new(width, height).ok_or(StickerError::Raster)?;
    resvg::render(
        &tree,
        Transform::from_scale(scale_x, scale_y),
        &mut pixmap.as_mut(),
    );
    Ok(graphics::raster_from_pixmap(&pixmap))
}

pub fn rasterize_sticker(
    sticker_id: &str,
    width: u32,
    height: u32,
) -> Result<Raster, StickerError> {
    match sticker_source(sticker_id) {
        Some(StickerSource::Svg(bytes)) => rasterize_svg(bytes, width, height),
        Some(StickerSource::Graphic {
            definition_id,
            preset,
        }) => {
            let mut params = ParamValues::new();
            for (key, value) in preset {
                params.insert((*key).to_owned(), serde_json::Value::from(*value));
            }
            render_graphic(definition_id, &params, width, height).ok_or(StickerError::Raster)
        }
        None => Err(StickerError::UnknownSticker(sticker_id.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(raster: &Raster, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * raster.width + x) * 4) as usize;
        [
            raster.rgba[offset],
            raster.rgba[offset + 1],
            raster.rgba[offset + 2],
            raster.rgba[offset + 3],
        ]
    }

    #[test]
    fn every_country_has_a_flag_asset() {
        let missing: Vec<&str> = countries()
            .iter()
            .filter(|country| flag_svg(&country.code).is_none())
            .map(|country| country.code.as_str())
            .collect();
        assert!(missing.is_empty(), "flags without assets: {missing:?}");
    }

    #[test]
    fn the_catalogue_is_the_full_country_list() {
        assert_eq!(countries().len(), 254);
        assert!(FLAG_SVGS.len() >= 254);
    }

    #[test]
    fn sticker_ids_round_trip() {
        let id = build_sticker_id("flags", "DE");
        assert_eq!(id, "flags:DE");
        let parsed = parse_sticker_id(&id).expect("parsed");
        assert_eq!(parsed.provider, "flags");
        assert_eq!(parsed.value, "DE");
        assert!(parse_sticker_id("flags").is_none());
        assert!(parse_sticker_id(":DE").is_none());
    }

    #[test]
    fn the_japanese_flag_rasterises_to_its_real_colours() {
        let raster = rasterize_sticker("flags:JP", 120, 80).expect("flag");
        assert_eq!(raster.width, 120);
        assert_eq!(raster.height, 80);
        let centre = pixel(&raster, 60, 40);
        assert!(centre[0] > 180, "centre red channel {centre:?}");
        assert!(centre[1] < 90, "centre green channel {centre:?}");
        assert!(centre[2] < 90, "centre blue channel {centre:?}");
        assert_eq!(centre[3], 255);
        let corner = pixel(&raster, 3, 3);
        assert!(
            corner[0] > 230 && corner[1] > 230 && corner[2] > 230,
            "{corner:?}"
        );
    }

    #[test]
    fn the_french_flag_has_three_bands() {
        let raster = rasterize_sticker("flags:FR", 90, 60).expect("flag");
        let blue = pixel(&raster, 10, 30);
        let white = pixel(&raster, 45, 30);
        let red = pixel(&raster, 80, 30);
        assert!(blue[2] > blue[0] + 40, "left band should be blue {blue:?}");
        assert!(
            white[0] > 200 && white[1] > 200 && white[2] > 200,
            "middle band should be white {white:?}"
        );
        assert!(red[0] > red[2] + 40, "right band should be red {red:?}");
    }

    #[test]
    fn shape_stickers_resolve_through_the_graphics_definitions() {
        let raster = rasterize_sticker("shapes:ellipse", 64, 64).expect("shape");
        assert_eq!(pixel(&raster, 32, 32)[3], 255);
        assert_eq!(pixel(&raster, 0, 0)[3], 0);
    }

    #[test]
    fn legacy_shape_ids_still_resolve() {
        let raster = rasterize_sticker("shapes:triangle", 64, 64).expect("legacy shape");
        assert_eq!(pixel(&raster, 32, 40)[3], 255);
        assert_eq!(pixel(&raster, 1, 62)[3], 0);
    }

    #[test]
    fn unknown_stickers_report_instead_of_panicking() {
        let error = rasterize_sticker("flags:ZZZZ", 32, 32).unwrap_err();
        assert!(matches!(error, StickerError::UnknownSticker(_)));
    }

    #[test]
    fn intrinsic_size_comes_from_the_svg() {
        let (width, height) = sticker_intrinsic_size("flags:JP").expect("size");
        assert!(width > 0.0 && height > 0.0);
        assert!((width / height - 1.5).abs() < 0.5, "{width}x{height}");
    }

    #[test]
    fn every_flag_asset_parses_as_svg() {
        let mut broken = Vec::new();
        for (code, bytes) in FLAG_SVGS {
            if usvg::Tree::from_data(bytes, &usvg::Options::default()).is_err() {
                broken.push(*code);
            }
        }
        assert!(broken.is_empty(), "unparsable flags: {broken:?}");
    }
}
