use std::collections::HashMap;
use std::path::Path;

use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Value;

use crate::SpeechError;

const BOS: &str = "^";
const EOS: &str = "$";
const PAD: &str = "_";

#[derive(Clone, Debug)]
pub struct PiperConfig {
    pub sample_rate: u32,
    pub noise_scale: f32,
    pub length_scale: f32,
    pub noise_w: f32,
    pub num_speakers: usize,
    pub language: String,
    pub phoneme_ids: HashMap<String, Vec<i64>>,
}

impl PiperConfig {
    pub fn parse(json: &str) -> Result<Self, SpeechError> {
        let root: serde_json::Value = serde_json::from_str(json)
            .map_err(|error| SpeechError::InvalidVoiceData(format!("bad voice config: {error}")))?;

        let sample_rate = root
            .pointer("/audio/sample_rate")
            .and_then(|value| value.as_u64())
            .ok_or_else(|| {
                SpeechError::InvalidVoiceData("voice config has no audio.sample_rate".into())
            })? as u32;

        let scale = |name: &str, fallback: f32| {
            root.pointer(&format!("/inference/{name}"))
                .and_then(|value| value.as_f64())
                .map(|value| value as f32)
                .unwrap_or(fallback)
        };

        let map = root
            .get("phoneme_id_map")
            .and_then(|value| value.as_object())
            .ok_or_else(|| {
                SpeechError::InvalidVoiceData("voice config has no phoneme_id_map".into())
            })?;

        let mut phoneme_ids = HashMap::with_capacity(map.len());
        for (phoneme, ids) in map {
            let ids: Vec<i64> = ids
                .as_array()
                .map(|list| list.iter().filter_map(|id| id.as_i64()).collect())
                .unwrap_or_default();
            if !ids.is_empty() {
                phoneme_ids.insert(phoneme.clone(), ids);
            }
        }
        if phoneme_ids.is_empty() {
            return Err(SpeechError::InvalidVoiceData(
                "voice config has an empty phoneme_id_map".into(),
            ));
        }
        for required in [BOS, EOS, PAD] {
            if !phoneme_ids.contains_key(required) {
                return Err(SpeechError::InvalidVoiceData(format!(
                    "voice config is missing the {required:?} sentinel"
                )));
            }
        }

        Ok(Self {
            sample_rate,
            noise_scale: scale("noise_scale", 0.667),
            length_scale: scale("length_scale", 1.0),
            noise_w: scale("noise_w", 0.8),
            num_speakers: root
                .get("num_speakers")
                .and_then(|value| value.as_u64())
                .unwrap_or(1) as usize,
            language: root
                .pointer("/language/code")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string(),
            phoneme_ids,
        })
    }

    pub fn load(path: &Path) -> Result<Self, SpeechError> {
        let json = std::fs::read_to_string(path)
            .map_err(|error| SpeechError::Load(format!("{}: {error}", path.display())))?;
        Self::parse(&json)
    }

    pub fn knows(&self, phoneme: &str) -> bool {
        self.phoneme_ids.contains_key(phoneme)
    }

    pub fn unsupported_symbols(&self, ipa: &str) -> Vec<String> {
        let mut missing: Vec<String> = Vec::new();
        for symbol in ipa.chars() {
            let key = symbol.to_string();
            if !self.knows(&key) && !missing.contains(&key) {
                missing.push(key);
            }
        }
        missing
    }

    pub fn encode(&self, ipa: &str) -> Vec<i64> {
        let mut ids = Vec::with_capacity(ipa.chars().count() * 2 + 3);
        ids.extend_from_slice(&self.phoneme_ids[BOS]);
        ids.extend_from_slice(&self.phoneme_ids[PAD]);
        for symbol in ipa.chars() {
            if let Some(mapped) = self.phoneme_ids.get(&symbol.to_string()) {
                ids.extend_from_slice(mapped);
                ids.extend_from_slice(&self.phoneme_ids[PAD]);
            }
        }
        ids.extend_from_slice(&self.phoneme_ids[EOS]);
        ids
    }
}

pub struct PiperSynthesizer {
    session: Session,
    config: PiperConfig,
    speaker: i64,
    length_scale: f32,
}

impl PiperSynthesizer {
    pub fn load(model_path: &Path, config_path: &Path) -> Result<Self, SpeechError> {
        let config = PiperConfig::load(config_path)?;
        let session = Session::builder()
            .map_err(|error| SpeechError::Load(error.to_string()))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|error| SpeechError::Load(error.to_string()))?
            .commit_from_file(model_path)
            .map_err(|error| SpeechError::Load(error.to_string()))?;
        let length_scale = config.length_scale;
        Ok(Self {
            session,
            config,
            speaker: 0,
            length_scale,
        })
    }

    pub fn config(&self) -> &PiperConfig {
        &self.config
    }

    pub fn sample_rate(&self) -> u32 {
        self.config.sample_rate
    }

    pub fn set_speed(&mut self, speed: f32) {
        let speed = speed.clamp(0.5, 2.0);
        self.length_scale = self.config.length_scale / speed;
    }

    pub fn phonemize(&self, text: &str) -> Result<String, SpeechError> {
        let ipa = crate::ru::phonemize(text);
        if ipa.chars().all(|symbol| !symbol.is_alphabetic()) {
            return Err(SpeechError::EmptyText);
        }
        Ok(ipa)
    }

    pub fn synthesize(&mut self, text: &str) -> Result<(Vec<f32>, u32), SpeechError> {
        let ipa = self.phonemize(text)?;
        let ids = self.config.encode(&ipa);
        if ids.len() <= 3 {
            return Err(SpeechError::EmptyText);
        }

        let length = ids.len() as i64;
        let input = Value::from_array((vec![1_i64, length], ids))
            .map_err(|error| SpeechError::Inference(error.to_string()))?;
        let input_lengths = Value::from_array((vec![1_i64], vec![length]))
            .map_err(|error| SpeechError::Inference(error.to_string()))?;
        let scales = Value::from_array((
            vec![3_i64],
            vec![
                self.config.noise_scale,
                self.length_scale,
                self.config.noise_w,
            ],
        ))
        .map_err(|error| SpeechError::Inference(error.to_string()))?;

        let wants_speaker = self
            .session
            .inputs()
            .iter()
            .any(|slot| slot.name() == "sid");

        let outputs = if wants_speaker {
            let sid = Value::from_array((vec![1_i64], vec![self.speaker]))
                .map_err(|error| SpeechError::Inference(error.to_string()))?;
            self.session.run(ort::inputs![
                "input" => input,
                "input_lengths" => input_lengths,
                "scales" => scales,
                "sid" => sid,
            ])
        } else {
            self.session.run(ort::inputs![
                "input" => input,
                "input_lengths" => input_lengths,
                "scales" => scales,
            ])
        }
        .map_err(|error| SpeechError::Inference(error.to_string()))?;

        let first = outputs.iter().next().ok_or(SpeechError::EmptyOutput)?.1;
        let (_shape, data) = first
            .try_extract_tensor::<f32>()
            .map_err(|error| SpeechError::Inference(error.to_string()))?;
        if data.is_empty() {
            return Err(SpeechError::EmptyOutput);
        }
        Ok((data.to_vec(), self.config.sample_rate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "audio": { "sample_rate": 22050, "quality": "medium" },
        "espeak": { "voice": "ru" },
        "inference": { "noise_scale": 0.667, "length_scale": 1, "noise_w": 0.8 },
        "phoneme_id_map": {
            "_": [0], "^": [1], "$": [2], " ": [3], ".": [10],
            "a": [14], "t": [32], "ˈ": [120]
        },
        "num_symbols": 256,
        "num_speakers": 1,
        "language": { "code": "ru_RU", "name_english": "Russian" }
    }"#;

    #[test]
    fn config_reads_audio_and_inference_settings() {
        let config = PiperConfig::parse(SAMPLE).expect("config");
        assert_eq!(config.sample_rate, 22_050);
        assert!((config.noise_scale - 0.667).abs() < 1e-6);
        assert!((config.length_scale - 1.0).abs() < 1e-6);
        assert!((config.noise_w - 0.8).abs() < 1e-6);
        assert_eq!(config.num_speakers, 1);
        assert_eq!(config.language, "ru_RU");
    }

    #[test]
    fn config_reads_the_phoneme_vocabulary() {
        let config = PiperConfig::parse(SAMPLE).expect("config");
        assert!(config.knows("a"));
        assert!(config.knows("\u{02c8}"));
        assert!(!config.knows("ʐ"));
        assert_eq!(config.phoneme_ids["t"], vec![32]);
    }

    #[test]
    fn config_rejects_malformed_input() {
        assert!(PiperConfig::parse("not json").is_err());
        assert!(PiperConfig::parse(r#"{"audio":{}}"#).is_err());
        assert!(PiperConfig::parse(r#"{"audio":{"sample_rate":22050}}"#).is_err());
        assert!(PiperConfig::parse(
            r#"{"audio":{"sample_rate":22050},"phoneme_id_map":{"a":[1]}}"#
        )
        .is_err());
    }

    #[test]
    fn config_defaults_missing_inference_scales() {
        let config = PiperConfig::parse(
            r#"{"audio":{"sample_rate":16000},"phoneme_id_map":{"_":[0],"^":[1],"$":[2],"a":[5]}}"#,
        )
        .expect("config");
        assert_eq!(config.sample_rate, 16_000);
        assert!((config.noise_scale - 0.667).abs() < 1e-6);
        assert!((config.noise_w - 0.8).abs() < 1e-6);
    }

    #[test]
    fn encoding_wraps_the_run_in_sentinels_and_pads() {
        let config = PiperConfig::parse(SAMPLE).expect("config");
        assert_eq!(config.encode("ta"), vec![1, 0, 32, 0, 14, 0, 2]);
        assert_eq!(config.encode(""), vec![1, 0, 2]);
    }

    #[test]
    fn encoding_skips_symbols_the_voice_does_not_know() {
        let config = PiperConfig::parse(SAMPLE).expect("config");
        assert_eq!(config.encode("tʐa"), vec![1, 0, 32, 0, 14, 0, 2]);
        assert_eq!(config.unsupported_symbols("tʐaʂ"), vec!["ʐ", "ʂ"]);
        assert!(config.unsupported_symbols("ta.").is_empty());
    }

    #[test]
    fn russian_phonemes_fit_a_real_russian_voice_vocabulary() {
        let path = crate::models::cached_voice_config_path(
            crate::models::find_voice("ru_dmitri").expect("voice"),
        );
        if !path.is_file() {
            return;
        }
        let config = PiperConfig::load(&path).expect("config");
        let ipa = crate::ru::phonemize(
            "Привет, это тест синтеза речи. Двадцать пять файлов, всё готово!",
        );
        let missing = config.unsupported_symbols(&ipa);
        assert!(
            missing.is_empty(),
            "phonemes outside the voice: {missing:?} in {ipa}"
        );
    }
}
