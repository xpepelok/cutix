use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::SpeechError;

#[derive(Clone, Copy, Debug)]
pub struct ModelSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub license: &'static str,
    pub url: &'static str,
    pub file_name: &'static str,
    pub sample_rate: u32,
    pub style_dimensions: usize,
    pub max_phoneme_tokens: usize,
    pub approximate_size_mb: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Engine {
    Kokoro,
    Piper,
}

#[derive(Clone, Copy, Debug)]
pub struct VoiceSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub language: &'static str,
    pub license: &'static str,
    pub url: &'static str,
    pub file_name: &'static str,
    pub british: bool,
    pub engine: Engine,
    pub config_url: &'static str,
    pub config_file_name: &'static str,
}

pub const SPEECH_MODELS: &[ModelSpec] = &[
    ModelSpec {
        key: "kokoro-v1-q8",
        label: "Kokoro 82M v1.0 (quantised)",
        license: "Apache-2.0",
        url: "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/onnx/model_quantized.onnx",
        file_name: "kokoro-v1.0-quantized.onnx",
        sample_rate: 24_000,
        style_dimensions: 256,
        max_phoneme_tokens: 510,
        approximate_size_mb: 92,
    },
    ModelSpec {
        key: "kokoro-v1",
        label: "Kokoro 82M v1.0",
        license: "Apache-2.0",
        url: "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/onnx/model.onnx",
        file_name: "kokoro-v1.0.onnx",
        sample_rate: 24_000,
        style_dimensions: 256,
        max_phoneme_tokens: 510,
        approximate_size_mb: 326,
    },
];

macro_rules! kokoro_voice {
    ($key:literal, $label:literal, $language:literal, $british:literal) => {
        VoiceSpec {
            key: $key,
            label: $label,
            language: $language,
            license: "Apache-2.0",
            url: concat!(
                "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main/voices/",
                $key,
                ".bin"
            ),
            file_name: concat!("kokoro-voice-", $key, ".bin"),
            british: $british,
            engine: Engine::Kokoro,
            config_url: "",
            config_file_name: "",
        }
    };
}

macro_rules! piper_ru_voice {
    ($key:literal, $name:literal, $label:literal, $license:literal) => {
        VoiceSpec {
            key: $key,
            label: $label,
            language: "ru-RU",
            license: $license,
            url: concat!(
                "https://huggingface.co/rhasspy/piper-voices/resolve/main/ru/ru_RU/",
                $name,
                "/medium/ru_RU-",
                $name,
                "-medium.onnx"
            ),
            file_name: concat!("piper-ru_RU-", $name, "-medium.onnx"),
            british: false,
            engine: Engine::Piper,
            config_url: concat!(
                "https://huggingface.co/rhasspy/piper-voices/resolve/main/ru/ru_RU/",
                $name,
                "/medium/ru_RU-",
                $name,
                "-medium.onnx.json"
            ),
            config_file_name: concat!("piper-ru_RU-", $name, "-medium.onnx.json"),
        }
    };
}

pub const VOICES: &[VoiceSpec] = &[
    kokoro_voice!("af_heart", "Heart (US, female)", "en-US", false),
    kokoro_voice!("af_bella", "Bella (US, female)", "en-US", false),
    kokoro_voice!("af_nicole", "Nicole (US, female)", "en-US", false),
    kokoro_voice!("af_sarah", "Sarah (US, female)", "en-US", false),
    kokoro_voice!("am_michael", "Michael (US, male)", "en-US", false),
    kokoro_voice!("am_adam", "Adam (US, male)", "en-US", false),
    kokoro_voice!("am_puck", "Puck (US, male)", "en-US", false),
    kokoro_voice!("bf_emma", "Emma (UK, female)", "en-GB", true),
    kokoro_voice!("bf_alice", "Alice (UK, female)", "en-GB", true),
    kokoro_voice!("bm_george", "George (UK, male)", "en-GB", true),
    kokoro_voice!("bm_daniel", "Daniel (UK, male)", "en-GB", true),
    piper_ru_voice!("ru_dmitri", "dmitri", "Дмитрий (RU, male)", "CC0-1.0"),
    piper_ru_voice!("ru_denis", "denis", "Денис (RU, male)", "CC0-1.0"),
];

pub const DEFAULT_MODEL: &str = "kokoro-v1-q8";
pub const DEFAULT_VOICE: &str = "af_heart";
pub const DEFAULT_RUSSIAN_VOICE: &str = "ru_dmitri";

pub fn voices_for_language(language: &str) -> impl Iterator<Item = &'static VoiceSpec> + '_ {
    VOICES
        .iter()
        .filter(move |voice| voice.language == language)
}

pub fn find_model(key: &str) -> Option<&'static ModelSpec> {
    SPEECH_MODELS.iter().find(|model| model.key == key)
}

pub fn find_voice(key: &str) -> Option<&'static VoiceSpec> {
    VOICES.iter().find(|voice| voice.key == key)
}

pub fn default_model() -> &'static ModelSpec {
    find_model(DEFAULT_MODEL).expect("default model is registered")
}

pub fn cache_directory() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".cache"))
        })
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("cutix").join("models")
}

pub fn cached_model_path(model: &ModelSpec) -> PathBuf {
    cache_directory().join(model.file_name)
}

pub fn cached_voice_path(voice: &VoiceSpec) -> PathBuf {
    cache_directory().join(voice.file_name)
}

pub fn cached_voice_config_path(voice: &VoiceSpec) -> PathBuf {
    cache_directory().join(voice.config_file_name)
}

fn is_file_cached(path: &Path, minimum_bytes: u64) -> bool {
    path.is_file()
        && fs::metadata(path)
            .map(|meta| meta.len() > minimum_bytes)
            .unwrap_or(false)
}

pub fn is_model_cached(model: &ModelSpec) -> bool {
    is_file_cached(&cached_model_path(model), 1024)
}

pub fn is_voice_cached(voice: &VoiceSpec) -> bool {
    is_file_cached(&cached_voice_path(voice), 1024)
}

fn download(url: &str, target: &Path, mut on_progress: impl FnMut(f32)) -> Result<(), SpeechError> {
    let directory = target
        .parent()
        .ok_or_else(|| SpeechError::Download("cache path has no parent".to_string()))?;
    fs::create_dir_all(directory).map_err(|error| SpeechError::Download(error.to_string()))?;

    let response = ureq::get(url)
        .call()
        .map_err(|error| SpeechError::Download(error.to_string()))?;

    let total = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);

    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    let partial = directory.join(format!("{file_name}.part"));
    let mut file =
        fs::File::create(&partial).map_err(|error| SpeechError::Download(error.to_string()))?;
    let mut reader = response.into_reader();
    let mut buffer = [0u8; 64 * 1024];
    let mut written: u64 = 0;

    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| SpeechError::Download(error.to_string()))?;
        if read == 0 {
            break;
        }
        std::io::Write::write_all(&mut file, &buffer[..read])
            .map_err(|error| SpeechError::Download(error.to_string()))?;
        written += read as u64;
        if total > 0 {
            on_progress((written as f32 / total as f32).clamp(0.0, 1.0));
        }
    }

    drop(file);
    fs::rename(&partial, target).map_err(|error| SpeechError::Download(error.to_string()))?;
    on_progress(1.0);
    Ok(())
}

pub fn ensure_model_downloaded(
    model: &ModelSpec,
    on_progress: impl FnMut(f32),
) -> Result<PathBuf, SpeechError> {
    let target = cached_model_path(model);
    if is_model_cached(model) {
        return Ok(target);
    }
    download(model.url, &target, on_progress)?;
    Ok(target)
}

pub fn ensure_voice_downloaded(
    voice: &VoiceSpec,
    on_progress: impl FnMut(f32),
) -> Result<PathBuf, SpeechError> {
    let target = cached_voice_path(voice);
    if is_voice_cached(voice) {
        return Ok(target);
    }
    download(voice.url, &target, on_progress)?;
    Ok(target)
}

pub fn ensure_voice_config_downloaded(
    voice: &VoiceSpec,
    on_progress: impl FnMut(f32),
) -> Result<PathBuf, SpeechError> {
    if voice.config_url.is_empty() {
        return Err(SpeechError::InvalidVoiceData(format!(
            "{} has no config sidecar",
            voice.key
        )));
    }
    let target = cached_voice_config_path(voice);
    if is_file_cached(&target, 64) {
        return Ok(target);
    }
    download(voice.config_url, &target, on_progress)?;
    Ok(target)
}

pub fn load_style_matrix(
    path: &Path,
    style_dimensions: usize,
) -> Result<Vec<Vec<f32>>, SpeechError> {
    let bytes = fs::read(path).map_err(|error| SpeechError::Download(error.to_string()))?;
    parse_style_matrix(&bytes, style_dimensions)
}

pub fn parse_style_matrix(
    bytes: &[u8],
    style_dimensions: usize,
) -> Result<Vec<Vec<f32>>, SpeechError> {
    if style_dimensions == 0 {
        return Err(SpeechError::InvalidVoiceData(
            "style dimension must be positive".to_string(),
        ));
    }
    let stride = style_dimensions * 4;
    if bytes.is_empty() || bytes.len() % stride != 0 {
        return Err(SpeechError::InvalidVoiceData(format!(
            "voice file of {} bytes is not a multiple of {stride}",
            bytes.len()
        )));
    }

    Ok(bytes
        .chunks_exact(stride)
        .map(|frame| {
            frame
                .chunks_exact(4)
                .map(|value| f32::from_le_bytes([value[0], value[1], value[2], value[3]]))
                .collect()
        })
        .collect())
}

pub fn remove_cached_model(model: &ModelSpec) -> Result<(), SpeechError> {
    let path = cached_model_path(model);
    if path.exists() {
        fs::remove_file(&path).map_err(|error| SpeechError::Download(error.to_string()))?;
    }
    Ok(())
}

pub fn cached_model_size_mb(model: &ModelSpec) -> Option<u64> {
    fs::metadata(cached_model_path(model))
        .ok()
        .map(|meta| meta.len() / (1024 * 1024))
}

pub fn describe_model(model: &ModelSpec) -> String {
    format!(
        "{} · {} · ~{} MB",
        model.label, model.license, model.approximate_size_mb
    )
}

pub fn describe_voice(voice: &VoiceSpec) -> String {
    format!("{} · {} · {}", voice.label, voice.language, voice.license)
}

pub fn is_inside_cache(path: &Path) -> bool {
    path.starts_with(cache_directory())
}

pub fn is_permissive_licence(license: &str) -> bool {
    const ALLOWED: &[&str] = &[
        "Apache-2.0",
        "MIT",
        "BSD-3-Clause",
        "BSD-2-Clause",
        "CC0-1.0",
    ];
    ALLOWED.contains(&license)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registries_are_not_empty() {
        assert!(!SPEECH_MODELS.is_empty());
        assert!(!VOICES.is_empty());
    }

    #[test]
    fn every_model_has_a_permissive_licence() {
        for model in SPEECH_MODELS {
            assert!(
                is_permissive_licence(model.license),
                "{} has licence {}",
                model.key,
                model.license
            );
        }
    }

    #[test]
    fn every_voice_has_a_permissive_licence() {
        for voice in VOICES {
            assert!(
                is_permissive_licence(voice.license),
                "{} has licence {}",
                voice.key,
                voice.license
            );
        }
    }

    #[test]
    fn non_commercial_licences_are_rejected() {
        assert!(!is_permissive_licence("cc-by-nc-4.0"));
        assert!(!is_permissive_licence("CC-BY-NC-SA-4.0"));
        assert!(!is_permissive_licence("other"));
        assert!(!is_permissive_licence(""));
    }

    #[test]
    fn models_and_voices_are_looked_up_by_key() {
        assert!(find_model("kokoro-v1-q8").is_some());
        assert!(find_model("nope").is_none());
        assert!(find_voice("af_heart").is_some());
        assert!(find_voice("nope").is_none());
    }

    #[test]
    fn default_model_and_voice_exist() {
        assert_eq!(default_model().key, DEFAULT_MODEL);
        assert!(find_voice(DEFAULT_VOICE).is_some());
    }

    #[test]
    fn keys_are_unique() {
        for (index, model) in SPEECH_MODELS.iter().enumerate() {
            assert!(
                SPEECH_MODELS[..index]
                    .iter()
                    .all(|other| other.key != model.key),
                "duplicate model key {}",
                model.key
            );
        }
        for (index, voice) in VOICES.iter().enumerate() {
            assert!(
                VOICES[..index].iter().all(|other| other.key != voice.key),
                "duplicate voice key {}",
                voice.key
            );
        }
    }

    #[test]
    fn urls_point_at_the_declared_repository() {
        for model in SPEECH_MODELS {
            assert!(model
                .url
                .starts_with("https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/"));
        }
        for voice in VOICES {
            match voice.engine {
                Engine::Kokoro => {
                    assert!(voice.url.starts_with(
                        "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/"
                    ));
                    assert!(voice.url.ends_with(&format!("/{}.bin", voice.key)));
                    assert!(voice.config_url.is_empty());
                }
                Engine::Piper => {
                    assert!(voice
                        .url
                        .starts_with("https://huggingface.co/rhasspy/piper-voices/"));
                    assert!(voice.url.ends_with(".onnx"), "{}", voice.url);
                    assert_eq!(voice.config_url, format!("{}.json", voice.url));
                    assert_eq!(voice.config_file_name, format!("{}.json", voice.file_name));
                }
            }
        }
    }

    #[test]
    fn russian_voices_are_registered_and_permissive() {
        let russian: Vec<_> = voices_for_language("ru-RU").collect();
        assert_eq!(russian.len(), 2, "expected dmitri and denis");
        for voice in &russian {
            assert_eq!(voice.engine, Engine::Piper);
            assert_eq!(voice.license, "CC0-1.0");
            assert!(is_permissive_licence(voice.license));
            assert!(!voice.british);
        }
        assert!(find_voice(DEFAULT_RUSSIAN_VOICE).is_some());
    }

    #[test]
    fn non_permissive_russian_voices_stay_out_of_the_registry() {
        assert!(find_voice("ru_ruslan").is_none());
        assert!(find_voice("ru_irina").is_none());
        assert!(!is_permissive_licence("CC-BY-NC-SA-4.0"));
        assert!(!is_permissive_licence("Unknown"));
    }

    #[test]
    fn only_piper_voices_carry_a_config_sidecar() {
        for voice in VOICES {
            assert_eq!(
                voice.engine == Engine::Piper,
                !voice.config_file_name.is_empty(),
                "{}",
                voice.key
            );
        }
        let kokoro = find_voice(DEFAULT_VOICE).expect("voice");
        assert!(ensure_voice_config_downloaded(kokoro, |_| {}).is_err());
    }

    #[test]
    fn piper_config_is_cached_next_to_its_model() {
        let voice = find_voice(DEFAULT_RUSSIAN_VOICE).expect("voice");
        let model = cached_voice_path(voice);
        let config = cached_voice_config_path(voice);
        assert!(is_inside_cache(&config));
        assert_eq!(config, model.with_extension("onnx.json"));
    }

    #[test]
    fn cached_paths_sit_in_the_cache_directory() {
        assert!(is_inside_cache(&cached_model_path(default_model())));
        assert!(is_inside_cache(&cached_voice_path(
            find_voice(DEFAULT_VOICE).expect("voice")
        )));
    }

    #[test]
    fn descriptions_mention_the_licence() {
        assert!(describe_model(default_model()).contains("Apache-2.0"));
        assert!(describe_voice(find_voice("bm_george").expect("voice")).contains("Apache-2.0"));
    }

    #[test]
    fn british_voices_are_flagged() {
        assert!(find_voice("bm_george").expect("voice").british);
        assert!(!find_voice("af_heart").expect("voice").british);
    }

    #[test]
    fn style_matrix_is_parsed_row_by_row() {
        let mut bytes = Vec::new();
        for value in [1.0f32, 2.0, 3.0, 4.0] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        let matrix = parse_style_matrix(&bytes, 2).expect("matrix");
        assert_eq!(matrix.len(), 2);
        assert_eq!(matrix[0], vec![1.0, 2.0]);
        assert_eq!(matrix[1], vec![3.0, 4.0]);
    }

    #[test]
    fn style_matrix_rejects_ragged_input() {
        assert!(parse_style_matrix(&[0u8; 6], 2).is_err());
        assert!(parse_style_matrix(&[], 2).is_err());
        assert!(parse_style_matrix(&[0u8; 8], 0).is_err());
    }
}
