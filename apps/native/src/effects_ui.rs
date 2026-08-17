use cutix_project::model::ParamValues;
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamKind {
    Number { min: f64, max: f64, step: f64 },
    Select(&'static [(&'static str, &'static str)]),
    Color,
    Curve,
    Lut,
}

#[derive(Clone, Copy, Debug)]
pub struct ParamDefinition {
    pub key: &'static str,
    pub label_key: &'static str,
    pub kind: ParamKind,
    pub default_number: f64,
    pub default_text: &'static str,
}

impl ParamDefinition {
    pub const fn number(
        key: &'static str,
        label_key: &'static str,
        default: f64,
        min: f64,
        max: f64,
        step: f64,
    ) -> Self {
        Self {
            key,
            label_key,
            kind: ParamKind::Number { min, max, step },
            default_number: default,
            default_text: "",
        }
    }

    pub const fn select(
        key: &'static str,
        label_key: &'static str,
        default: &'static str,
        options: &'static [(&'static str, &'static str)],
    ) -> Self {
        Self {
            key,
            label_key,
            kind: ParamKind::Select(options),
            default_number: 0.0,
            default_text: default,
        }
    }

    pub const fn color(key: &'static str, label_key: &'static str, default: &'static str) -> Self {
        Self {
            key,
            label_key,
            kind: ParamKind::Color,
            default_number: 0.0,
            default_text: default,
        }
    }

    pub fn default_value(&self) -> Value {
        match self.kind {
            ParamKind::Number { .. } => json!(self.default_number),
            ParamKind::Select(_) | ParamKind::Color | ParamKind::Lut => json!(self.default_text),
            ParamKind::Curve => json!(IDENTITY_CURVE_JSON),
        }
    }
}

pub const IDENTITY_CURVE_JSON: &str = "{\"master\":[{\"x\":0,\"y\":0},{\"x\":1,\"y\":1}],\"r\":[{\"x\":0,\"y\":0},{\"x\":1,\"y\":1}],\"g\":[{\"x\":0,\"y\":0},{\"x\":1,\"y\":1}],\"b\":[{\"x\":0,\"y\":0},{\"x\":1,\"y\":1}]}";

pub struct EffectDefinition {
    pub effect_type: &'static str,
    pub name_key: &'static str,
    pub glyph: &'static str,
    pub params: &'static [ParamDefinition],
}

const ADJUSTMENT_PARAMS: &[ParamDefinition] = &[
    ParamDefinition::number(
        "brightness",
        "effects.param.brightness",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number(
        "contrast",
        "effects.param.contrast",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number(
        "exposure",
        "effects.param.exposure",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number(
        "saturation",
        "effects.param.saturation",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number(
        "vibrance",
        "effects.param.vibrance",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number(
        "temperature",
        "effects.param.temperature",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number("tint", "effects.param.tint", 0.0, -100.0, 100.0, 1.0),
    ParamDefinition::number(
        "highlights",
        "effects.param.highlights",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number("shadows", "effects.param.shadows", 0.0, -100.0, 100.0, 1.0),
    ParamDefinition::number("sharpness", "effects.param.sharpness", 0.0, 0.0, 100.0, 1.0),
];

pub const FILTER_PRESET_IDS: &[&str] = &[
    "none", "warm", "cool", "vintage", "mono", "punch", "film", "fade", "vivid", "moody",
];

const FILTER_PRESET_OPTIONS: &[(&str, &str)] = &[
    ("none", "effects.filter.preset.none"),
    ("warm", "effects.filter.preset.warm"),
    ("cool", "effects.filter.preset.cool"),
    ("vintage", "effects.filter.preset.vintage"),
    ("mono", "effects.filter.preset.mono"),
    ("punch", "effects.filter.preset.punch"),
    ("film", "effects.filter.preset.film"),
    ("fade", "effects.filter.preset.fade"),
    ("vivid", "effects.filter.preset.vivid"),
    ("moody", "effects.filter.preset.moody"),
];

const MOSAIC_SHAPE_OPTIONS: &[(&str, &str)] = &[
    ("rect", "effects.mosaic.shape.rect"),
    ("ellipse", "effects.mosaic.shape.ellipse"),
];

const BLUR_PARAMS: &[ParamDefinition] = &[ParamDefinition::number(
    "intensity",
    "effects.param.intensity",
    15.0,
    0.0,
    100.0,
    1.0,
)];

pub const CHROMA_KEY_DEFAULT_COLOR: &str = "#00b140";

const CHROMA_KEY_PARAMS: &[ParamDefinition] = &[
    ParamDefinition::color(
        "keyColor",
        "effects.param.keyColor",
        CHROMA_KEY_DEFAULT_COLOR,
    ),
    ParamDefinition::number(
        "similarity",
        "effects.param.similarity",
        0.3,
        0.0,
        1.0,
        0.01,
    ),
    ParamDefinition::number(
        "smoothness",
        "effects.param.smoothness",
        0.1,
        0.0,
        1.0,
        0.01,
    ),
    ParamDefinition::number("spill", "effects.param.spillRemoval", 0.5, 0.0, 1.0, 0.01),
];

const RETOUCH_PARAMS: &[ParamDefinition] = &[
    ParamDefinition::number(
        "smoothing",
        "effects.param.skinSmoothing",
        0.5,
        0.0,
        1.0,
        0.01,
    ),
    ParamDefinition::number("radius", "effects.param.radius", 1.0, 0.1, 4.0, 0.05),
    ParamDefinition::number(
        "edge",
        "effects.param.detailPreservation",
        0.12,
        0.01,
        0.6,
        0.01,
    ),
    ParamDefinition::number("tone", "effects.param.warmth", 0.3, 0.0, 1.0, 0.01),
    ParamDefinition::number(
        "brightness",
        "effects.param.brightness",
        0.0,
        -0.25,
        0.25,
        0.01,
    ),
];

const FILTER_PARAMS: &[ParamDefinition] = &[
    ParamDefinition::select(
        "preset",
        "effects.param.preset",
        "warm",
        FILTER_PRESET_OPTIONS,
    ),
    ParamDefinition::number(
        "intensity",
        "effects.param.intensity",
        100.0,
        0.0,
        100.0,
        1.0,
    ),
];

const MOSAIC_PARAMS: &[ParamDefinition] = &[
    ParamDefinition::number(
        "blockSize",
        "effects.param.blockSize",
        24.0,
        2.0,
        200.0,
        1.0,
    ),
    ParamDefinition::select("shape", "effects.param.shape", "rect", MOSAIC_SHAPE_OPTIONS),
    ParamDefinition::number("regionX", "effects.param.regionX", 0.0, 0.0, 100.0, 1.0),
    ParamDefinition::number("regionY", "effects.param.regionY", 0.0, 0.0, 100.0, 1.0),
    ParamDefinition::number(
        "regionWidth",
        "effects.param.regionWidth",
        100.0,
        0.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number(
        "regionHeight",
        "effects.param.regionHeight",
        100.0,
        0.0,
        100.0,
        1.0,
    ),
];

const CURVES_PARAMS: &[ParamDefinition] = &[
    ParamDefinition {
        key: "curves",
        label_key: "effects.param.curves",
        kind: ParamKind::Curve,
        default_number: 0.0,
        default_text: IDENTITY_CURVE_JSON,
    },
    ParamDefinition::number("amount", "effects.param.amount", 100.0, 0.0, 100.0, 1.0),
];

const LUT_PARAMS: &[ParamDefinition] = &[
    ParamDefinition {
        key: "lut",
        label_key: "effects.param.lutFile",
        kind: ParamKind::Lut,
        default_number: 0.0,
        default_text: "",
    },
    ParamDefinition::number(
        "intensity",
        "effects.param.intensity",
        100.0,
        0.0,
        100.0,
        1.0,
    ),
];

const BACKGROUND_BLUR_PARAMS: &[ParamDefinition] = &[ParamDefinition::number(
    "strength",
    "effects.param.strength",
    40.0,
    0.0,
    100.0,
    1.0,
)];

pub const HSL_BAND_KEYS: &[(&str, &str)] = &[
    ("red", "effects.hsl.band.red"),
    ("orange", "effects.hsl.band.orange"),
    ("yellow", "effects.hsl.band.yellow"),
    ("green", "effects.hsl.band.green"),
    ("aqua", "effects.hsl.band.aqua"),
    ("blue", "effects.hsl.band.blue"),
    ("purple", "effects.hsl.band.purple"),
    ("magenta", "effects.hsl.band.magenta"),
];

pub const HSL_CHANNEL_KEYS: &[(&str, &str)] = &[
    ("hue", "effects.param.hue"),
    ("saturation", "effects.param.saturation"),
    ("luminance", "effects.param.luminance"),
];

pub const EFFECT_DEFINITIONS: &[EffectDefinition] = &[
    EffectDefinition {
        effect_type: "blur",
        name_key: "effects.blur.name",
        glyph: "rain-drop",
        params: BLUR_PARAMS,
    },
    EffectDefinition {
        effect_type: "chroma-key",
        name_key: "effects.chromaKey.name",
        glyph: "magic-wand05",
        params: CHROMA_KEY_PARAMS,
    },
    EffectDefinition {
        effect_type: "retouch",
        name_key: "effects.retouch.name",
        glyph: "happy01",
        params: RETOUCH_PARAMS,
    },
    EffectDefinition {
        effect_type: "adjustment",
        name_key: "effects.adjustment.name",
        glyph: "sliders-horizontal",
        params: ADJUSTMENT_PARAMS,
    },
    EffectDefinition {
        effect_type: "filter",
        name_key: "effects.filter.name",
        glyph: "checkerboard",
        params: FILTER_PARAMS,
    },
    EffectDefinition {
        effect_type: "mosaic",
        name_key: "effects.mosaic.name",
        glyph: "grid-view",
        params: MOSAIC_PARAMS,
    },
    EffectDefinition {
        effect_type: "curves",
        name_key: "effects.curves.name",
        glyph: "dashboard-speed",
        params: CURVES_PARAMS,
    },
    EffectDefinition {
        effect_type: "hsl",
        name_key: "effects.hsl.name",
        glyph: "layers01",
        params: HSL_PARAMS,
    },
    EffectDefinition {
        effect_type: "lut",
        name_key: "effects.lut.name",
        glyph: "settings05",
        params: LUT_PARAMS,
    },
    EffectDefinition {
        effect_type: "background-blur",
        name_key: "effects.backgroundBlur.name",
        glyph: "snow",
        params: BACKGROUND_BLUR_PARAMS,
    },
];

const HSL_PARAMS: &[ParamDefinition] = &[
    ParamDefinition::number("red.hue", "effects.param.hue", 0.0, -100.0, 100.0, 1.0),
    ParamDefinition::number(
        "red.saturation",
        "effects.param.saturation",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number(
        "red.luminance",
        "effects.param.luminance",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number("orange.hue", "effects.param.hue", 0.0, -100.0, 100.0, 1.0),
    ParamDefinition::number(
        "orange.saturation",
        "effects.param.saturation",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number(
        "orange.luminance",
        "effects.param.luminance",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number("yellow.hue", "effects.param.hue", 0.0, -100.0, 100.0, 1.0),
    ParamDefinition::number(
        "yellow.saturation",
        "effects.param.saturation",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number(
        "yellow.luminance",
        "effects.param.luminance",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number("green.hue", "effects.param.hue", 0.0, -100.0, 100.0, 1.0),
    ParamDefinition::number(
        "green.saturation",
        "effects.param.saturation",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number(
        "green.luminance",
        "effects.param.luminance",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number("aqua.hue", "effects.param.hue", 0.0, -100.0, 100.0, 1.0),
    ParamDefinition::number(
        "aqua.saturation",
        "effects.param.saturation",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number(
        "aqua.luminance",
        "effects.param.luminance",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number("blue.hue", "effects.param.hue", 0.0, -100.0, 100.0, 1.0),
    ParamDefinition::number(
        "blue.saturation",
        "effects.param.saturation",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number(
        "blue.luminance",
        "effects.param.luminance",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number("purple.hue", "effects.param.hue", 0.0, -100.0, 100.0, 1.0),
    ParamDefinition::number(
        "purple.saturation",
        "effects.param.saturation",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number(
        "purple.luminance",
        "effects.param.luminance",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number("magenta.hue", "effects.param.hue", 0.0, -100.0, 100.0, 1.0),
    ParamDefinition::number(
        "magenta.saturation",
        "effects.param.saturation",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
    ParamDefinition::number(
        "magenta.luminance",
        "effects.param.luminance",
        0.0,
        -100.0,
        100.0,
        1.0,
    ),
];

pub fn definition(effect_type: &str) -> Option<&'static EffectDefinition> {
    EFFECT_DEFINITIONS
        .iter()
        .find(|entry| entry.effect_type == effect_type)
}

pub fn default_params(effect_type: &str) -> ParamValues {
    let mut params = ParamValues::new();
    let Some(definition) = definition(effect_type) else {
        return params;
    };
    for param in definition.params {
        params.insert(param.key.to_owned(), param.default_value());
    }
    params
}

pub fn param_definition(effect_type: &str, key: &str) -> Option<&'static ParamDefinition> {
    definition(effect_type)?
        .params
        .iter()
        .find(|param| param.key == key)
}

pub const TRANSITION_KEYS: &[(&str, &str)] = &[
    ("crossfade", "transitions.type.crossfade"),
    ("fadeToBlack", "transitions.type.fadeToBlack"),
    ("slideLeft", "transitions.type.slideLeft"),
    ("slideRight", "transitions.type.slideRight"),
    ("slideUp", "transitions.type.slideUp"),
    ("slideDown", "transitions.type.slideDown"),
    ("wipeLeft", "transitions.type.wipeLeft"),
    ("wipeRight", "transitions.type.wipeRight"),
    ("wipeUp", "transitions.type.wipeUp"),
    ("wipeDown", "transitions.type.wipeDown"),
    ("zoom", "transitions.type.zoom"),
];

pub const TRANSITION_EASING_KEYS: &[(&str, &str)] = &[
    ("linear", "transitions.easing.linear"),
    ("easeIn", "transitions.easing.easeIn"),
    ("easeOut", "transitions.easing.easeOut"),
    ("easeInOut", "transitions.easing.easeInOut"),
];

pub const FPS_PRESETS: &[f64] = &[24.0, 25.0, 30.0, 60.0, 120.0];

pub const CANVAS_PRESETS: &[(&str, u32, u32)] = &[
    ("16:9", 1920, 1080),
    ("9:16", 1080, 1920),
    ("1:1", 1080, 1080),
    ("4:3", 1440, 1080),
];

pub const BACKGROUND_COLORS: &[&str] = &[
    "#000000", "#ffffff", "#1e1e1e", "#f5f5f5", "#1d4ed8", "#047857", "#b91c1c", "#7c3aed",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_has_a_defaultable_param_set() {
        for definition in EFFECT_DEFINITIONS {
            let params = default_params(definition.effect_type);
            assert_eq!(
                params.len(),
                definition.params.len(),
                "{}",
                definition.effect_type
            );
        }
    }

    #[test]
    fn hsl_covers_every_band_and_channel() {
        let params = default_params("hsl");
        assert_eq!(params.len(), 24);
        for (band, _) in HSL_BAND_KEYS {
            for (channel, _) in HSL_CHANNEL_KEYS {
                assert!(params.contains_key(&format!("{band}.{channel}")));
            }
        }
    }

    #[test]
    fn the_curves_default_is_the_identity_set() {
        let params = default_params("curves");
        let curves = params.get("curves").unwrap().as_str().unwrap();
        let parsed = cutix_playback::curve::parse_curve_set(Some(&json!(curves)));
        assert!(cutix_playback::curve::is_identity_curve_set(&parsed));
    }

    #[test]
    fn chroma_key_defaults_match_the_web_effective_values() {
        let params = default_params("chroma-key");
        assert_eq!(params.get("keyColor").unwrap(), CHROMA_KEY_DEFAULT_COLOR);
        assert_eq!(params.get("similarity").unwrap(), 0.3);
        assert_eq!(params.get("smoothness").unwrap(), 0.1);
        assert_eq!(params.get("spill").unwrap(), 0.5);

        assert_eq!(
            param_definition("chroma-key", "spill").unwrap().label_key,
            "effects.param.spillRemoval"
        );
    }

    #[test]
    fn retouch_defaults_and_order_match_the_web() {
        let definition = definition("retouch").unwrap();
        let order: Vec<&str> = definition.params.iter().map(|p| p.key).collect();
        assert_eq!(order, ["smoothing", "radius", "edge", "tone", "brightness"]);

        let params = default_params("retouch");
        assert_eq!(params.get("smoothing").unwrap(), 0.5);
        assert_eq!(params.get("radius").unwrap(), 1.0);
        assert_eq!(params.get("edge").unwrap(), 0.12);
        assert_eq!(params.get("tone").unwrap(), 0.3);
        assert_eq!(params.get("brightness").unwrap(), 0.0);

        let labels: Vec<&str> = definition.params.iter().map(|p| p.label_key).collect();
        assert_eq!(
            labels,
            [
                "effects.param.skinSmoothing",
                "effects.param.radius",
                "effects.param.detailPreservation",
                "effects.param.warmth",
                "effects.param.brightness",
            ]
        );

        if let ParamKind::Number { min, max, .. } =
            param_definition("retouch", "brightness").unwrap().kind
        {
            assert_eq!((min, max), (-0.25, 0.25));
        } else {
            panic!("brightness must be a number param");
        }
    }

    #[test]
    fn filter_defaults_match_the_web() {
        let params = default_params("filter");
        assert_eq!(params.get("preset").unwrap(), "warm");
        assert_eq!(params.get("intensity").unwrap(), 100.0);
    }
}
