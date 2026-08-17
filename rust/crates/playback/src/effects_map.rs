use std::collections::HashMap;

use compositor::{EffectPassDescriptor, EffectUniformValueDescriptor};
use cutix_project::model::ParamValues;

use crate::curve::{bake_curve_table, is_identity_curve_set, parse_curve_set};

const MAX_SINGLE_PASS_SIGMA: f32 = 10.0;
const MAX_STEP: f32 = 4.0;
const MAX_EFFECTIVE_SIGMA: f32 = MAX_SINGLE_PASS_SIGMA * MAX_STEP;
const MAX_ITERATIONS: u32 = 8;
const INTENSITY_TO_SIGMA_DIVISOR: f32 = 5.0;
const MOSAIC_REFERENCE_WIDTH: f32 = 1920.0;
const ZOOM_REFERENCE_WIDTH: f32 = 1920.0;
const ZOOM_REFERENCE_HEIGHT: f32 = 1080.0;

pub const ADJUSTMENT_PARAM_KEYS: [&str; 10] = [
    "brightness",
    "contrast",
    "exposure",
    "saturation",
    "vibrance",
    "temperature",
    "tint",
    "highlights",
    "shadows",
    "sharpness",
];

fn adjustment_scale(key: &str) -> f32 {
    match key {
        "brightness" => 1.0 / 200.0,
        "exposure" => 1.0 / 50.0,
        _ => 1.0 / 100.0,
    }
}

pub const HSL_BANDS: [&str; 8] = [
    "red", "orange", "yellow", "green", "aqua", "blue", "purple", "magenta",
];

pub const HSL_CHANNELS: [&str; 3] = ["hue", "saturation", "luminance"];

fn hsl_scale(channel: &str) -> f32 {
    match channel {
        "hue" => 30.0 / 100.0,
        _ => 1.0 / 100.0,
    }
}

pub const FILTER_PRESETS: &[(&str, &[(&str, f32)])] = &[
    ("none", &[]),
    (
        "warm",
        &[
            ("temperature", 38.0),
            ("saturation", 10.0),
            ("highlights", -6.0),
            ("contrast", 6.0),
        ],
    ),
    (
        "cool",
        &[
            ("temperature", -38.0),
            ("tint", -6.0),
            ("saturation", 6.0),
            ("contrast", 8.0),
        ],
    ),
    (
        "vintage",
        &[
            ("temperature", 20.0),
            ("saturation", -28.0),
            ("contrast", -14.0),
            ("shadows", 16.0),
            ("highlights", -12.0),
        ],
    ),
    (
        "mono",
        &[
            ("saturation", -100.0),
            ("contrast", 18.0),
            ("sharpness", 12.0),
        ],
    ),
    (
        "punch",
        &[
            ("contrast", 42.0),
            ("saturation", 12.0),
            ("shadows", -12.0),
            ("highlights", 8.0),
            ("sharpness", 18.0),
        ],
    ),
    (
        "film",
        &[
            ("contrast", 20.0),
            ("shadows", 14.0),
            ("highlights", -14.0),
            ("saturation", -10.0),
            ("temperature", 10.0),
        ],
    ),
    (
        "fade",
        &[
            ("contrast", -28.0),
            ("shadows", 26.0),
            ("saturation", -16.0),
            ("exposure", 8.0),
        ],
    ),
    (
        "vivid",
        &[
            ("saturation", 26.0),
            ("vibrance", 40.0),
            ("contrast", 16.0),
            ("sharpness", 22.0),
        ],
    ),
    (
        "moody",
        &[
            ("exposure", -14.0),
            ("contrast", 28.0),
            ("shadows", -20.0),
            ("temperature", -14.0),
            ("saturation", -10.0),
        ],
    ),
];

pub fn filter_preset(id: &str) -> &'static [(&'static str, f32)] {
    FILTER_PRESETS
        .iter()
        .find(|(preset, _)| *preset == id)
        .unwrap_or(&FILTER_PRESETS[0])
        .1
}

pub fn resolve_filter_values(preset_id: &str, intensity: f32) -> HashMap<&'static str, f32> {
    let preset = filter_preset(preset_id);
    let strength = intensity.clamp(0.0, 100.0) / 100.0;
    ADJUSTMENT_PARAM_KEYS
        .iter()
        .map(|key| {
            let base = preset
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| *value)
                .unwrap_or(0.0);
            (*key, base * strength)
        })
        .collect()
}

fn number(params: &ParamValues, name: &str, fallback: f32) -> f32 {
    let Some(value) = params
        .get(name)
        .or_else(|| params.get(&format!("u_{name}")))
    else {
        return fallback;
    };
    if let Some(parsed) = value.as_f64() {
        return parsed as f32;
    }
    value
        .as_str()
        .and_then(|text| text.parse::<f32>().ok())
        .filter(|parsed| parsed.is_finite())
        .unwrap_or(fallback)
}

fn text<'a>(params: &'a ParamValues, name: &str, fallback: &'a str) -> &'a str {
    params
        .get(name)
        .and_then(|value| value.as_str())
        .unwrap_or(fallback)
}

fn vector(params: &ParamValues, name: &str, fallback: &[f32]) -> Vec<f32> {
    let Some(value) = params
        .get(name)
        .or_else(|| params.get(&format!("u_{name}")))
    else {
        return fallback.to_vec();
    };
    if let Some(array) = value.as_array() {
        let mapped: Vec<f32> = array
            .iter()
            .filter_map(|entry| entry.as_f64())
            .map(|entry| entry as f32)
            .collect();
        if mapped.len() == fallback.len() {
            return mapped;
        }
    }
    if let Some(object) = value.as_object() {
        let keys = ["r", "g", "b", "a"];
        let mapped: Vec<f32> = keys
            .iter()
            .take(fallback.len())
            .filter_map(|key| object.get(*key).and_then(|entry| entry.as_f64()))
            .map(|entry| entry as f32)
            .collect();
        if mapped.len() == fallback.len() {
            return mapped;
        }
    }
    fallback.to_vec()
}

const CHROMA_KEY_DEFAULT_COLOR: [f32; 3] = [0.0, 177.0 / 255.0, 64.0 / 255.0];

fn key_color(params: &ParamValues, fallback: [f32; 3]) -> [f32; 3] {
    let Some(value) = params.get("keyColor") else {
        return fallback;
    };
    if let Some(hex) = value.as_str() {
        return parse_hex_color(hex).unwrap_or(fallback);
    }
    let mapped = vector(params, "keyColor", &fallback);
    [mapped[0], mapped[1], mapped[2]]
}

fn parse_hex_color(value: &str) -> Option<[f32; 3]> {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() != 6 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let packed = u32::from_str_radix(hex, 16).ok()?;
    Some([
        ((packed >> 16) & 0xff) as f32 / 255.0,
        ((packed >> 8) & 0xff) as f32 / 255.0,
        (packed & 0xff) as f32 / 255.0,
    ])
}

fn pass(
    shader: &str,
    uniforms: HashMap<String, EffectUniformValueDescriptor>,
) -> EffectPassDescriptor {
    EffectPassDescriptor {
        shader: shader.to_owned(),
        uniforms,
        data_id: None,
    }
}

pub fn intensity_to_sigma(intensity: f32, resolution: f32, reference: f32) -> f32 {
    (intensity / INTENSITY_TO_SIGMA_DIVISOR) * (resolution / reference)
}

pub fn gaussian_blur_passes(sigma_x: f32, sigma_y: f32) -> Vec<EffectPassDescriptor> {
    let max_sigma = sigma_x.max(sigma_y);
    if max_sigma < 0.001 {
        return Vec::new();
    }
    let iterations = ((max_sigma * max_sigma) / (MAX_EFFECTIVE_SIGMA * MAX_EFFECTIVE_SIGMA))
        .ceil()
        .max(1.0)
        .min(MAX_ITERATIONS as f32) as u32;
    let root = (iterations as f32).sqrt();
    let per_pass_x = sigma_x / root;
    let per_pass_y = sigma_y / root;
    let step_x = (per_pass_x / MAX_SINGLE_PASS_SIGMA).max(1.0);
    let step_y = (per_pass_y / MAX_SINGLE_PASS_SIGMA).max(1.0);

    let mut passes = Vec::with_capacity(iterations as usize * 2);
    for _ in 0..iterations {
        for (sigma, step, direction) in [
            (per_pass_x, step_x, [1.0, 0.0]),
            (per_pass_y, step_y, [0.0, 1.0]),
        ] {
            let mut uniforms = HashMap::new();
            uniforms.insert(
                "u_sigma".into(),
                EffectUniformValueDescriptor::Number(sigma),
            );
            uniforms.insert("u_step".into(), EffectUniformValueDescriptor::Number(step));
            uniforms.insert(
                "u_direction".into(),
                EffectUniformValueDescriptor::Vector(direction.to_vec()),
            );
            passes.push(pass("gaussian-blur", uniforms));
        }
    }
    passes
}

fn adjustment_pass(values: &HashMap<&'static str, f32>) -> Option<EffectPassDescriptor> {
    if ADJUSTMENT_PARAM_KEYS
        .iter()
        .all(|key| values.get(key).copied().unwrap_or(0.0).abs() < 0.0001)
    {
        return None;
    }
    let mut uniforms = HashMap::new();
    for key in ADJUSTMENT_PARAM_KEYS {
        let value = values.get(key).copied().unwrap_or(0.0) * adjustment_scale(key);
        uniforms.insert(
            format!("u_{key}"),
            EffectUniformValueDescriptor::Number(value),
        );
    }
    Some(pass("adjustment", uniforms))
}

pub fn build_hsl_table(params: &ParamValues) -> Vec<f32> {
    let mut table = vec![0.0f32; HSL_BANDS.len() * 4];
    for (band_index, band) in HSL_BANDS.iter().enumerate() {
        for (channel_index, channel) in HSL_CHANNELS.iter().enumerate() {
            let raw = number(params, &format!("{band}.{channel}"), 0.0);
            table[band_index * 4 + channel_index] = raw * hsl_scale(channel);
        }
    }
    table
}

pub fn is_hsl_identity(params: &ParamValues) -> bool {
    HSL_BANDS.iter().all(|band| {
        HSL_CHANNELS
            .iter()
            .all(|channel| number(params, &format!("{band}.{channel}"), 0.0).abs() < 0.0001)
    })
}

pub fn effect_passes(
    effect_type: &str,
    params: &ParamValues,
    width: u32,
    height: u32,
) -> Vec<EffectPassDescriptor> {
    let width_f = width.max(1) as f32;
    let height_f = height.max(1) as f32;

    match effect_type {
        "blur" => {
            let intensity = number(params, "intensity", 15.0);
            gaussian_blur_passes(
                intensity_to_sigma(intensity, width_f, ZOOM_REFERENCE_WIDTH),
                intensity_to_sigma(intensity, height_f, ZOOM_REFERENCE_HEIGHT),
            )
        }
        "gaussian-blur" => {
            let mut uniforms = HashMap::new();
            uniforms.insert(
                "u_sigma".into(),
                EffectUniformValueDescriptor::Number(number(params, "sigma", 4.0)),
            );
            uniforms.insert(
                "u_step".into(),
                EffectUniformValueDescriptor::Number(number(params, "step", 1.0)),
            );
            uniforms.insert(
                "u_direction".into(),
                EffectUniformValueDescriptor::Vector(vector(params, "direction", &[1.0, 0.0])),
            );
            vec![pass("gaussian-blur", uniforms)]
        }
        "chroma-key" => {
            let mut uniforms = HashMap::new();
            uniforms.insert(
                "u_key_color".into(),
                EffectUniformValueDescriptor::Vector(
                    key_color(params, CHROMA_KEY_DEFAULT_COLOR).to_vec(),
                ),
            );
            for (name, key, fallback) in [
                ("u_similarity", "similarity", 0.3),
                ("u_smoothness", "smoothness", 0.1),
                ("u_spill", "spill", 0.5),
            ] {
                uniforms.insert(
                    name.into(),
                    EffectUniformValueDescriptor::Number(number(params, key, fallback)),
                );
            }
            vec![pass("chroma-key", uniforms)]
        }
        "retouch" => {
            let mut uniforms = HashMap::new();
            for (name, key, fallback) in [
                ("u_smoothing", "smoothing", 0.5),
                ("u_radius", "radius", 1.0),
                ("u_tone", "tone", 0.3),
                ("u_brightness", "brightness", 0.0),
                ("u_edge", "edge", 0.12),
            ] {
                uniforms.insert(
                    name.into(),
                    EffectUniformValueDescriptor::Number(number(params, key, fallback)),
                );
            }
            vec![pass("retouch", uniforms)]
        }
        "adjustment" => {
            let values: HashMap<&'static str, f32> = ADJUSTMENT_PARAM_KEYS
                .iter()
                .map(|key| (*key, number(params, key, 0.0)))
                .collect();
            adjustment_pass(&values).into_iter().collect()
        }
        "filter" => {
            let preset = text(params, "preset", "warm");
            let intensity = number(params, "intensity", 100.0);
            adjustment_pass(&resolve_filter_values(preset, intensity))
                .into_iter()
                .collect()
        }
        "mosaic" => {
            let block_size = number(params, "blockSize", 24.0);
            let scaled = ((block_size * width_f) / MOSAIC_REFERENCE_WIDTH).max(1.0);
            let region = [
                (number(params, "regionX", 0.0) / 100.0).clamp(0.0, 1.0),
                (number(params, "regionY", 0.0) / 100.0).clamp(0.0, 1.0),
                (number(params, "regionWidth", 100.0) / 100.0).clamp(0.0, 1.0),
                (number(params, "regionHeight", 100.0) / 100.0).clamp(0.0, 1.0),
            ];
            let shape = if text(params, "shape", "rect") == "ellipse" {
                1.0
            } else {
                0.0
            };
            let mut uniforms = HashMap::new();
            uniforms.insert(
                "u_block_size".into(),
                EffectUniformValueDescriptor::Number(scaled),
            );
            uniforms.insert(
                "u_region".into(),
                EffectUniformValueDescriptor::Vector(region.to_vec()),
            );
            uniforms.insert(
                "u_shape".into(),
                EffectUniformValueDescriptor::Number(shape),
            );
            vec![pass("mosaic", uniforms)]
        }
        "curves" => {
            let curves = parse_curve_set(params.get("curves"));
            let amount = number(params, "amount", 100.0).clamp(0.0, 100.0);
            if is_identity_curve_set(&curves) || amount <= 0.0 {
                return Vec::new();
            }
            let mut uniforms = HashMap::new();
            uniforms.insert(
                "u_amount".into(),
                EffectUniformValueDescriptor::Number(amount / 100.0),
            );
            uniforms.insert(
                "u_table".into(),
                EffectUniformValueDescriptor::Vector(bake_curve_table(&curves)),
            );
            vec![pass("curves", uniforms)]
        }
        "hsl" => {
            if is_hsl_identity(params) {
                return Vec::new();
            }
            let mut uniforms = HashMap::new();
            uniforms.insert(
                "u_table".into(),
                EffectUniformValueDescriptor::Vector(build_hsl_table(params)),
            );
            vec![pass("hsl-qualifier", uniforms)]
        }
        "lut" => {
            let Some(table) = params.get("table").and_then(|value| value.as_array()) else {
                return Vec::new();
            };
            let entries: Vec<f32> = table
                .iter()
                .filter_map(|value| value.as_f64())
                .map(|value| value as f32)
                .collect();
            let size = number(params, "size", 0.0);
            if size < 2.0 || entries.len() < (size as usize).pow(3) * 4 {
                return Vec::new();
            }

            let amount =
                number(params, "intensity", number(params, "amount", 100.0)).clamp(0.0, 100.0);
            if amount <= 0.0 {
                return Vec::new();
            }
            let mut uniforms = HashMap::new();
            uniforms.insert("u_size".into(), EffectUniformValueDescriptor::Number(size));

            uniforms.insert(
                "u_intensity".into(),
                EffectUniformValueDescriptor::Number(amount / 100.0),
            );
            uniforms.insert(
                "u_table".into(),
                EffectUniformValueDescriptor::Vector(entries),
            );
            vec![pass("lut3d", uniforms)]
        }
        "background-blur" => Vec::new(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn params(value: serde_json::Value) -> ParamValues {
        value.as_object().cloned().unwrap()
    }

    fn scalar(pass: &EffectPassDescriptor, name: &str) -> f32 {
        match pass.uniforms.get(name) {
            Some(EffectUniformValueDescriptor::Number(value)) => *value,
            other => panic!("expected a scalar for {name}, got {other:?}"),
        }
    }

    fn table(pass: &EffectPassDescriptor, name: &str) -> Vec<f32> {
        match pass.uniforms.get(name) {
            Some(EffectUniformValueDescriptor::Vector(values)) => values.clone(),
            other => panic!("expected a vector for {name}, got {other:?}"),
        }
    }

    #[test]
    fn blur_scales_sigma_with_the_canvas_and_splits_into_separable_passes() {
        let passes = effect_passes("blur", &params(json!({ "intensity": 50 })), 1920, 1080);
        assert_eq!(passes.len(), 2);
        assert!((scalar(&passes[0], "u_sigma") - 10.0).abs() < 1e-5);
        assert_eq!(table(&passes[0], "u_direction"), vec![1.0, 0.0]);
        assert_eq!(table(&passes[1], "u_direction"), vec![0.0, 1.0]);

        let half = effect_passes("blur", &params(json!({ "intensity": 50 })), 960, 540);
        assert!((scalar(&half[0], "u_sigma") - 5.0).abs() < 1e-5);
    }

    #[test]
    fn a_huge_blur_iterates_and_keeps_per_pass_sigma_bounded() {
        let passes = effect_passes("blur", &params(json!({ "intensity": 100 })), 7680, 4320);
        assert!(passes.len() > 2, "expected iteration, got {}", passes.len());
        assert_eq!(passes.len() % 2, 0);
        for entry in &passes {
            let sigma = scalar(entry, "u_sigma");
            let step = scalar(entry, "u_step");
            assert!(sigma / step <= MAX_SINGLE_PASS_SIGMA + 1e-4);
        }
    }

    #[test]
    fn a_zero_blur_produces_no_passes() {
        assert!(effect_passes("blur", &params(json!({ "intensity": 0 })), 1920, 1080).is_empty());
    }

    #[test]
    fn an_identity_adjustment_produces_no_passes() {
        assert!(
            effect_passes("adjustment", &params(json!({ "contrast": 0 })), 1920, 1080).is_empty()
        );
    }

    #[test]
    fn adjustment_applies_the_web_uniform_scales() {
        let passes = effect_passes(
            "adjustment",
            &params(json!({ "brightness": 100, "exposure": 50, "contrast": 40 })),
            1920,
            1080,
        );
        assert_eq!(passes.len(), 1);
        assert!((scalar(&passes[0], "u_brightness") - 0.5).abs() < 1e-6);
        assert!((scalar(&passes[0], "u_exposure") - 1.0).abs() < 1e-6);
        assert!((scalar(&passes[0], "u_contrast") - 0.4).abs() < 1e-6);
        assert!((scalar(&passes[0], "u_tint") - 0.0).abs() < 1e-6);
    }

    #[test]
    fn a_filter_preset_scales_by_intensity() {
        let full = effect_passes(
            "filter",
            &params(json!({ "preset": "mono", "intensity": 100 })),
            1920,
            1080,
        );
        assert!((scalar(&full[0], "u_saturation") + 1.0).abs() < 1e-6);
        let half = effect_passes(
            "filter",
            &params(json!({ "preset": "mono", "intensity": 50 })),
            1920,
            1080,
        );
        assert!((scalar(&half[0], "u_saturation") + 0.5).abs() < 1e-6);
        assert!(effect_passes(
            "filter",
            &params(json!({ "preset": "none", "intensity": 100 })),
            1920,
            1080
        )
        .is_empty());
    }

    #[test]
    fn mosaic_scales_the_block_with_the_canvas_width() {
        let passes = effect_passes(
            "mosaic",
            &params(json!({ "blockSize": 48, "shape": "ellipse", "regionWidth": 50 })),
            960,
            540,
        );
        assert!((scalar(&passes[0], "u_block_size") - 24.0).abs() < 1e-5);
        assert!((scalar(&passes[0], "u_shape") - 1.0).abs() < 1e-6);
        assert_eq!(table(&passes[0], "u_region"), vec![0.0, 0.0, 0.5, 1.0]);
    }

    #[test]
    fn curves_bakes_a_256_entry_table_and_skips_the_identity() {
        assert!(effect_passes("curves", &params(json!({})), 1920, 1080).is_empty());
        let lifted = json!({
            "curves": {
                "master": [{ "x": 0, "y": 0 }, { "x": 0.5, "y": 0.75 }, { "x": 1, "y": 1 }],
                "r": [{ "x": 0, "y": 0 }, { "x": 1, "y": 1 }],
                "g": [{ "x": 0, "y": 0 }, { "x": 1, "y": 1 }],
                "b": [{ "x": 0, "y": 0 }, { "x": 1, "y": 1 }]
            },
            "amount": 100
        });
        let passes = effect_passes("curves", &params(lifted), 1920, 1080);
        assert_eq!(passes.len(), 1);
        assert_eq!(passes[0].shader, "curves");
        let baked = table(&passes[0], "u_table");
        assert_eq!(baked.len(), 1024);
        assert!(baked[128 * 4] > 0.7);
        assert!((scalar(&passes[0], "u_amount") - 1.0).abs() < 1e-6);
    }

    #[test]
    fn hsl_builds_a_32_float_band_table() {
        assert!(effect_passes("hsl", &params(json!({})), 1920, 1080).is_empty());
        let passes = effect_passes(
            "hsl",
            &params(json!({ "red.hue": 100, "blue.saturation": -50 })),
            1920,
            1080,
        );
        assert_eq!(passes[0].shader, "hsl-qualifier");
        let built = table(&passes[0], "u_table");
        assert_eq!(built.len(), 32);
        assert!((built[0] - 30.0).abs() < 1e-5);
        assert!((built[5 * 4 + 1] + 0.5).abs() < 1e-6);
        assert!((built[1] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn lut_needs_a_cube_sized_table() {
        assert!(
            effect_passes("lut", &params(json!({ "size": 2 })), 1920, 1080).is_empty(),
            "a declared size with no table must not reach the shader"
        );
        let entries: Vec<f32> = (0..2 * 2 * 2 * 4).map(|index| index as f32).collect();
        let passes = effect_passes(
            "lut",
            &params(json!({ "size": 2, "amount": 100, "table": entries })),
            1920,
            1080,
        );
        assert_eq!(passes.len(), 1);
        assert_eq!(passes[0].shader, "lut3d");
        assert_eq!(table(&passes[0], "u_table").len(), 32);
        assert!((scalar(&passes[0], "u_size") - 2.0).abs() < 1e-6);
    }

    #[test]
    fn lut_emits_the_intensity_uniform_the_shader_asks_for() {
        let entries: Vec<f32> = (0..2 * 2 * 2 * 4).map(|_| 0.0).collect();
        let passes = effect_passes(
            "lut",
            &params(json!({ "size": 2, "intensity": 50, "table": entries.clone() })),
            1920,
            1080,
        );
        assert!((scalar(&passes[0], "u_intensity") - 0.5).abs() < 1e-6);
        assert!(passes[0].uniforms.get("u_amount").is_none());

        let legacy = effect_passes(
            "lut",
            &params(json!({ "size": 2, "amount": 25, "table": entries })),
            1920,
            1080,
        );
        assert!((scalar(&legacy[0], "u_intensity") - 0.25).abs() < 1e-6);
    }

    #[test]
    fn chroma_key_parses_a_hex_key_color_into_normalized_rgb() {
        let passes = effect_passes(
            "chroma-key",
            &params(json!({ "keyColor": "#00b140" })),
            1920,
            1080,
        );
        let rgb = table(&passes[0], "u_key_color");
        assert!((rgb[0] - 0.0).abs() < 1e-6);
        assert!((rgb[1] - 177.0 / 255.0).abs() < 1e-6);
        assert!((rgb[2] - 64.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn chroma_key_falls_back_to_web_default_when_absent() {
        let passes = effect_passes("chroma-key", &params(json!({})), 1920, 1080);
        let rgb = table(&passes[0], "u_key_color");
        assert!((rgb[1] - 177.0 / 255.0).abs() < 1e-6);
        assert!((scalar(&passes[0], "u_similarity") - 0.3).abs() < 1e-6);
        assert!((scalar(&passes[0], "u_spill") - 0.5).abs() < 1e-6);
    }

    #[test]
    fn chroma_key_still_reads_a_legacy_rgb_array() {
        let passes = effect_passes(
            "chroma-key",
            &params(json!({ "keyColor": [0.2, 0.4, 0.6] })),
            1920,
            1080,
        );
        assert_eq!(table(&passes[0], "u_key_color"), vec![0.2, 0.4, 0.6]);
    }

    #[test]
    fn retouch_round_trips_a_stored_native_scale_value() {
        let passes = effect_passes(
            "retouch",
            &params(json!({ "radius": 4.0, "tone": 0.0, "edge": 0.1 })),
            1920,
            1080,
        );
        assert!((scalar(&passes[0], "u_radius") - 4.0).abs() < 1e-6);
        assert!((scalar(&passes[0], "u_tone") - 0.0).abs() < 1e-6);
        assert!((scalar(&passes[0], "u_edge") - 0.1).abs() < 1e-6);
    }

    #[test]
    fn background_blur_is_not_renderable_without_a_matte() {
        assert!(effect_passes(
            "background-blur",
            &params(json!({ "strength": 40 })),
            1920,
            1080
        )
        .is_empty());
    }
}
