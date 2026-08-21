pub mod models;
mod phonemes;
mod piper;
mod ru;
mod text;

/// Turns Russian text into the phoneme string the synthesiser reads.
///
/// Exposed because the pronunciation is worth inspecting on its own; the rest of
/// the Russian handling stays internal.
pub use ru::phonemize as phonemize_russian;

use std::path::Path;

use misaki_rs::{G2P, Language};
use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::value::Value;
use thiserror::Error;

use models::{ModelSpec, VoiceSpec};

#[derive(Debug, Error)]
pub enum SpeechError {
    #[error("failed to load model: {0}")]
    Load(String),
    #[error("inference failed: {0}")]
    Inference(String),
    #[error("model returned no audio")]
    EmptyOutput,
    #[error("no pronounceable text was given")]
    EmptyText,
    #[error("phoneme run of {count} tokens exceeds the model limit of {maximum}")]
    TooManyTokens { count: usize, maximum: usize },
    #[error("unknown model: {0}")]
    UnknownModel(String),
    #[error("unknown voice: {0}")]
    UnknownVoice(String),
    #[error("voice data is unusable: {0}")]
    InvalidVoiceData(String),
    #[error("model download failed: {0}")]
    Download(String),
    #[error("grapheme-to-phoneme conversion failed: {0}")]
    Phonemize(String),
}

pub type Audio = (Vec<f32>, u32);

fn language_for(british: bool) -> Language {
    if british {
        Language::EnglishGB
    } else {
        Language::EnglishUS
    }
}

pub const DEFAULT_SPEED: f32 = 1.0;

pub struct Synthesizer {
    session: Session,
    style: Vec<Vec<f32>>,
    spec: &'static ModelSpec,
    british: bool,
    speed: f32,
}

impl Synthesizer {
    pub fn load(
        model_path: impl AsRef<Path>,
        voice_path: impl AsRef<Path>,
        spec: &'static ModelSpec,
        british: bool,
    ) -> Result<Self, SpeechError> {
        let style = models::load_style_matrix(voice_path.as_ref(), spec.style_dimensions)?;
        if style.is_empty() {
            return Err(SpeechError::InvalidVoiceData("voice file is empty".into()));
        }

        let session = Session::builder()
            .map_err(|error| SpeechError::Load(error.to_string()))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|error| SpeechError::Load(error.to_string()))?
            .commit_from_file(model_path.as_ref())
            .map_err(|error| SpeechError::Load(error.to_string()))?;

        Ok(Self {
            session,
            style,
            spec,
            british,
            speed: DEFAULT_SPEED,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.spec.sample_rate
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(0.5, 2.0);
    }

    pub fn phonemize(&self, text: &str) -> Result<String, SpeechError> {
        let cleaned = text::normalise(text);
        if text::is_blank(&cleaned) {
            return Err(SpeechError::EmptyText);
        }
        let engine = G2P::new(language_for(self.british));
        let (phonemes, _tokens) = engine
            .g2p(&cleaned)
            .map_err(|error| SpeechError::Phonemize(error.to_string()))?;
        Ok(phonemes)
    }

    pub fn synthesize(&mut self, text: &str) -> Result<Audio, SpeechError> {
        let phonemes = self.phonemize(text)?;
        let tokens = phonemes::encode(&phonemes);
        if tokens.is_empty() {
            return Err(SpeechError::EmptyText);
        }

        let mut samples = Vec::new();
        for chunk in phonemes::split_tokens(&tokens, self.spec.max_phoneme_tokens) {
            samples.extend(self.run_chunk(&chunk)?);
        }
        if samples.is_empty() {
            return Err(SpeechError::EmptyOutput);
        }
        Ok((samples, self.spec.sample_rate))
    }

    fn run_chunk(&mut self, chunk: &[i64]) -> Result<Vec<f32>, SpeechError> {
        let padded = phonemes::pad(chunk, self.spec.max_phoneme_tokens)?;
        let style_row = self.style[chunk.len().min(self.style.len() - 1)].clone();

        let input_ids = Value::from_array((vec![1_i64, padded.len() as i64], padded))
            .map_err(|error| SpeechError::Inference(error.to_string()))?;
        let style = Value::from_array((vec![1_i64, style_row.len() as i64], style_row))
            .map_err(|error| SpeechError::Inference(error.to_string()))?;
        let speed = Value::from_array((vec![1_i64], vec![self.speed]))
            .map_err(|error| SpeechError::Inference(error.to_string()))?;

        let outputs = self
            .session
            .run(ort::inputs![
                "input_ids" => input_ids,
                "style" => style,
                "speed" => speed,
            ])
            .map_err(|error| SpeechError::Inference(error.to_string()))?;

        let first = outputs.iter().next().ok_or(SpeechError::EmptyOutput)?.1;
        let (_shape, data) = first
            .try_extract_tensor::<f32>()
            .map_err(|error| SpeechError::Inference(error.to_string()))?;
        if data.is_empty() {
            return Err(SpeechError::EmptyOutput);
        }
        Ok(data.to_vec())
    }
}

pub fn synthesize(text: &str, voice: &str) -> Result<Audio, SpeechError> {
    synthesize_with(text, models::DEFAULT_MODEL, voice, |_, _| {})
}

pub fn synthesize_with(
    text: &str,
    model_key: &str,
    voice_key: &str,
    mut on_progress: impl FnMut(&str, f32),
) -> Result<Audio, SpeechError> {
    let model = models::find_model(model_key)
        .ok_or_else(|| SpeechError::UnknownModel(model_key.to_string()))?;
    let voice: &'static VoiceSpec = models::find_voice(voice_key)
        .ok_or_else(|| SpeechError::UnknownVoice(voice_key.to_string()))?;

    match voice.engine {
        models::Engine::Piper => {
            let config_path = models::ensure_voice_config_downloaded(voice, |done| {
                on_progress(voice.config_file_name, done)
            })?;
            let voice_path =
                models::ensure_voice_downloaded(voice, |done| on_progress(voice.file_name, done))?;
            let mut synthesizer = piper::PiperSynthesizer::load(&voice_path, &config_path)?;
            synthesizer.synthesize(text)
        }
        models::Engine::Kokoro => {
            let voice_path =
                models::ensure_voice_downloaded(voice, |done| on_progress(voice.file_name, done))?;
            let model_path =
                models::ensure_model_downloaded(model, |done| on_progress(model.file_name, done))?;
            let mut synthesizer = Synthesizer::load(model_path, voice_path, model, voice.british)?;
            synthesizer.synthesize(text)
        }
    }
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|value| (*value as f64).powi(2)).sum();
    (sum / samples.len() as f64).sqrt() as f32
}

pub fn to_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut wav = Vec::with_capacity(44 + data_len as usize);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * 32767.0) as i16;
        wav.extend_from_slice(&value.to_le_bytes());
    }
    wav
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_model_and_voice_are_reported() {
        let error = synthesize_with("hello", "nope", "af_heart", |_, _| {}).unwrap_err();
        assert!(matches!(error, SpeechError::UnknownModel(_)));
        let error = synthesize_with("hello", models::DEFAULT_MODEL, "nope", |_, _| {}).unwrap_err();
        assert!(matches!(error, SpeechError::UnknownVoice(_)));
    }

    #[test]
    fn rms_of_silence_is_zero() {
        assert_eq!(rms(&[]), 0.0);
        assert_eq!(rms(&[0.0; 16]), 0.0);
        assert!((rms(&[1.0, -1.0, 1.0, -1.0]) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn wav_header_describes_the_payload() {
        let wav = to_wav(&[0.0, 0.5, -0.5], 24_000);
        assert_eq!(&wav[..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(wav.len(), 44 + 6);
        assert_eq!(
            u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]),
            24_000
        );
        assert_eq!(u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]), 6);
    }

    #[test]
    fn english_text_produces_kokoro_tokens() {
        let engine = G2P::new(Language::EnglishUS);
        let (ipa, _) = engine.g2p(&text::normalise("Hello world.")).expect("g2p");
        assert!(!ipa.is_empty(), "g2p produced nothing");
        let tokens = phonemes::encode(&ipa);
        assert!(
            tokens.len() > 4,
            "expected a real token run, got {tokens:?} from {ipa:?}"
        );
        assert!(tokens.iter().all(|token| *token >= 0));
    }

    #[test]
    fn g2p_output_stays_inside_the_kokoro_vocabulary() {
        let engine = G2P::new(Language::EnglishUS);
        let (ipa, _) = engine
            .g2p("The quick brown fox jumps over the lazy dog, 42 times!")
            .expect("g2p");
        assert!(ipa.contains('\u{02c8}'), "expected stress marks in {ipa:?}");
        let missing = phonemes::unsupported_symbols(&ipa);
        assert!(
            missing.is_empty(),
            "phonemes outside the vocabulary: {missing:?} in {ipa:?}"
        );
    }
}
