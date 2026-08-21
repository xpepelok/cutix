use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;

use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::value::Value;

use crate::MlError;
use crate::mel::{self, CHUNK_FRAMES, CHUNK_SAMPLES, MelExtractor, N_MELS, SAMPLE_RATE};
use crate::models::cache_directory;
use crate::tokenizer::Tokenizer;

pub struct WhisperSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub name_key: &'static str,
    pub repo: &'static str,
    pub approximate_size_mb: u32,
}

pub const WHISPER_MODELS: &[WhisperSpec] = &[
    WhisperSpec {
        key: "whisper-tiny",
        label: "Tiny",
        name_key: "transcription.model.tiny",
        repo: "onnx-community/whisper-tiny",
        approximate_size_mb: 145,
    },
    WhisperSpec {
        key: "whisper-base",
        label: "Base",
        name_key: "transcription.model.base",
        repo: "onnx-community/whisper-base",
        approximate_size_mb: 280,
    },
    WhisperSpec {
        key: "whisper-small",
        label: "Small",
        name_key: "transcription.model.small",
        repo: "onnx-community/whisper-small",
        approximate_size_mb: 950,
    },
];

pub const DEFAULT_WHISPER_MODEL: &str = "whisper-base";

const WHISPER_FILES: &[(&str, &str)] = &[
    ("onnx/encoder_model.onnx", "encoder_model.onnx"),
    ("onnx/decoder_model.onnx", "decoder_model.onnx"),
    ("vocab.json", "vocab.json"),
    ("added_tokens.json", "added_tokens.json"),
];

const MAX_TOKENS_PER_CHUNK: usize = 224;

#[derive(Clone, Debug, PartialEq)]
pub struct TranscriptSegment {
    pub text: String,
    pub start: f64,
    pub end: f64,
}

#[derive(Clone, Debug)]
pub struct Transcript {
    pub segments: Vec<TranscriptSegment>,
    pub language: String,
}

pub enum WhisperProgress {
    Downloading { file: String, done: f32 },
    Loading,
    Transcribing { done: f32 },
}

pub fn find_whisper(key: &str) -> Option<&'static WhisperSpec> {
    WHISPER_MODELS.iter().find(|model| model.key == key)
}

pub fn whisper_directory(spec: &WhisperSpec) -> PathBuf {
    cache_directory().join(spec.key)
}

pub fn whisper_is_cached(spec: &WhisperSpec) -> bool {
    let directory = whisper_directory(spec);
    WHISPER_FILES.iter().all(|(_, name)| {
        let path = directory.join(name);
        path.is_file()
            && fs::metadata(&path)
                .map(|meta| meta.len() > 512)
                .unwrap_or(false)
    })
}

pub fn whisper_cached_size_bytes(spec: &WhisperSpec) -> u64 {
    let directory = whisper_directory(spec);
    WHISPER_FILES
        .iter()
        .filter_map(|(_, name)| fs::metadata(directory.join(name)).ok())
        .map(|meta| meta.len())
        .sum()
}

pub fn download_file(
    url: &str,
    target: &PathBuf,
    on_progress: &mut dyn FnMut(f32),
    should_cancel: &dyn Fn() -> bool,
) -> Result<(), MlError> {
    let directory = target.parent().ok_or_else(|| {
        MlError::Download(format!("{} has no parent directory", target.display()))
    })?;
    fs::create_dir_all(directory)
        .map_err(|error| MlError::Download(format!("create {}: {error}", directory.display())))?;

    let response = ureq::get(url)
        .call()
        .map_err(|error| MlError::Download(format!("GET {url}: {error}")))?;
    let total = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);

    let partial = target.with_extension("part");
    let mut file = fs::File::create(&partial)
        .map_err(|error| MlError::Download(format!("create {}: {error}", partial.display())))?;
    let mut reader = response.into_reader();
    let mut buffer = vec![0u8; 256 * 1024];
    let mut written = 0u64;

    loop {
        if should_cancel() {
            drop(file);
            let _ = fs::remove_file(&partial);
            return Err(MlError::Cancelled);
        }
        let read = reader
            .read(&mut buffer)
            .map_err(|error| MlError::Download(format!("read {url}: {error}")))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])
            .map_err(|error| MlError::Download(format!("write {}: {error}", partial.display())))?;
        written += read as u64;
        if total > 0 {
            on_progress((written as f32 / total as f32).clamp(0.0, 1.0));
        }
    }

    drop(file);
    fs::rename(&partial, target)
        .map_err(|error| MlError::Download(format!("rename {}: {error}", partial.display())))?;
    on_progress(1.0);
    Ok(())
}

pub fn ensure_whisper_downloaded(
    spec: &WhisperSpec,
    on_progress: &mut dyn FnMut(WhisperProgress),
    should_cancel: &dyn Fn() -> bool,
) -> Result<(), MlError> {
    let directory = whisper_directory(spec);
    for (remote, name) in WHISPER_FILES {
        let target = directory.join(name);
        if target.is_file()
            && fs::metadata(&target)
                .map(|meta| meta.len() > 512)
                .unwrap_or(false)
        {
            continue;
        }
        if should_cancel() {
            return Err(MlError::Cancelled);
        }
        let url = format!("https://huggingface.co/{}/resolve/main/{remote}", spec.repo);
        let file = (*name).to_string();
        download_file(
            &url,
            &target,
            &mut |done| {
                on_progress(WhisperProgress::Downloading {
                    file: file.clone(),
                    done,
                })
            },
            should_cancel,
        )?;
    }
    Ok(())
}

struct SpecialTokens {
    start_of_transcript: u32,
    transcribe: u32,
    no_timestamps: u32,
    end_of_text: u32,
    timestamp_begin: u32,
    first_language: u32,
    last_language: u32,
}

impl SpecialTokens {
    fn resolve(tokenizer: &Tokenizer) -> Self {
        let start_of_transcript = tokenizer
            .special_id("<|startoftranscript|>")
            .unwrap_or(50258);
        let timestamp_begin = tokenizer.special_id("<|0.00|>").unwrap_or(50364);
        Self {
            start_of_transcript,
            transcribe: tokenizer.special_id("<|transcribe|>").unwrap_or(50359),
            no_timestamps: tokenizer.special_id("<|notimestamps|>").unwrap_or(50363),
            end_of_text: tokenizer.special_id("<|endoftext|>").unwrap_or(50257),
            timestamp_begin,
            first_language: start_of_transcript + 1,
            last_language: start_of_transcript + 99,
        }
    }
}

pub fn timestamp_token_seconds(token: u32, timestamp_begin: u32) -> Option<f64> {
    if token < timestamp_begin {
        return None;
    }
    Some((token - timestamp_begin) as f64 * 0.02)
}

pub fn assemble_segments(
    tokens: &[u32],
    timestamp_begin: u32,
    offset: f64,
    decode: &dyn Fn(&[u32]) -> String,
) -> Vec<TranscriptSegment> {
    let mut segments = Vec::new();
    let mut start: Option<f64> = None;
    let mut buffer: Vec<u32> = Vec::new();

    for token in tokens {
        match timestamp_token_seconds(*token, timestamp_begin) {
            Some(seconds) => match start {
                None => {
                    start = Some(seconds);
                    buffer.clear();
                }
                Some(begin) => {
                    let text = decode(&buffer).trim().to_string();
                    if !text.is_empty() {
                        segments.push(TranscriptSegment {
                            text,
                            start: offset + begin,
                            end: offset + seconds.max(begin),
                        });
                    }
                    buffer.clear();
                    start = None;
                }
            },
            None => buffer.push(*token),
        }
    }

    if let Some(begin) = start {
        let text = decode(&buffer).trim().to_string();
        if !text.is_empty() {
            segments.push(TranscriptSegment {
                text,
                start: offset + begin,
                end: offset + 30.0,
            });
        }
    }

    segments
}

struct WhisperModel {
    encoder: Session,
    decoder: Session,
    tokenizer: Tokenizer,
    specials: SpecialTokens,
    encoder_input: String,
    decoder_ids_input: String,
    decoder_states_input: String,
}

impl WhisperModel {
    fn load(spec: &WhisperSpec) -> Result<Self, MlError> {
        let directory = whisper_directory(spec);
        let encoder = build_session(&directory.join("encoder_model.onnx"))?;
        let decoder = build_session(&directory.join("decoder_model.onnx"))?;

        let vocab = fs::read_to_string(directory.join("vocab.json"))
            .map_err(|error| MlError::Load(format!("vocab.json: {error}")))?;
        let added = fs::read_to_string(directory.join("added_tokens.json"))
            .map_err(|error| MlError::Load(format!("added_tokens.json: {error}")))?;
        let tokenizer = Tokenizer::from_json(&vocab, &added)?;
        let specials = SpecialTokens::resolve(&tokenizer);

        let encoder_input = encoder
            .inputs()
            .first()
            .map(|input| input.name().to_string())
            .ok_or_else(|| MlError::Load("encoder has no inputs".to_string()))?;

        let decoder_names: Vec<String> = decoder
            .inputs()
            .iter()
            .map(|input| input.name().to_string())
            .collect();
        if decoder_names.len() < 2 {
            return Err(MlError::Load(format!(
                "decoder expects {} inputs, need input_ids and encoder_hidden_states",
                decoder_names.len()
            )));
        }
        let decoder_ids_input = decoder_names
            .iter()
            .find(|name| name.contains("input_ids"))
            .cloned()
            .unwrap_or_else(|| decoder_names[0].clone());
        let decoder_states_input = decoder_names
            .iter()
            .find(|name| name.contains("encoder_hidden_states") || name.contains("encoder"))
            .cloned()
            .unwrap_or_else(|| decoder_names[1].clone());

        Ok(Self {
            encoder,
            decoder,
            tokenizer,
            specials,
            encoder_input,
            decoder_ids_input,
            decoder_states_input,
        })
    }

    fn encode(&mut self, mel_data: Vec<f32>) -> Result<(Vec<i64>, Vec<f32>), MlError> {
        let input = Value::from_array((vec![1i64, N_MELS as i64, CHUNK_FRAMES as i64], mel_data))
            .map_err(|error| MlError::Inference(format!("mel tensor: {error}")))?;
        let name = self.encoder_input.clone();
        let outputs = self
            .encoder
            .run(ort::inputs![name => input])
            .map_err(|error| MlError::Inference(format!("encoder: {error}")))?;
        let output = outputs.iter().next().ok_or(MlError::EmptyOutput)?.1;
        let (shape, data) = output
            .try_extract_tensor::<f32>()
            .map_err(|error| MlError::Inference(format!("encoder output: {error}")))?;
        Ok((shape.to_vec(), data.to_vec()))
    }

    fn decode_step(
        &mut self,
        tokens: &[u32],
        states_shape: &[i64],
        states: &[f32],
    ) -> Result<Vec<f32>, MlError> {
        let ids: Vec<i64> = tokens.iter().map(|token| *token as i64).collect();
        let ids_value = Value::from_array((vec![1i64, ids.len() as i64], ids))
            .map_err(|error| MlError::Inference(format!("input_ids tensor: {error}")))?;
        let states_value = Value::from_array((states_shape.to_vec(), states.to_vec()))
            .map_err(|error| MlError::Inference(format!("encoder states tensor: {error}")))?;

        let ids_name = self.decoder_ids_input.clone();
        let states_name = self.decoder_states_input.clone();
        let outputs = self
            .decoder
            .run(ort::inputs![
                ids_name => ids_value,
                states_name => states_value
            ])
            .map_err(|error| MlError::Inference(format!("decoder: {error}")))?;

        let output = outputs.iter().next().ok_or(MlError::EmptyOutput)?.1;
        let (shape, data) = output
            .try_extract_tensor::<f32>()
            .map_err(|error| MlError::Inference(format!("decoder output: {error}")))?;

        let vocab = *shape.last().unwrap_or(&0) as usize;
        if vocab == 0 || data.len() < vocab {
            return Err(MlError::EmptyOutput);
        }
        Ok(data[data.len() - vocab..].to_vec())
    }
}

fn build_session(path: &PathBuf) -> Result<Session, MlError> {
    Session::builder()
        .map_err(|error| MlError::Load(error.to_string()))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|error| MlError::Load(error.to_string()))?
        .commit_from_file(path)
        .map_err(|error| MlError::Load(format!("{}: {error}", path.display())))
}

fn argmax_range(logits: &[f32], from: usize, to: usize) -> u32 {
    let end = to.min(logits.len());
    let mut best = from as u32;
    let mut best_value = f32::MIN;
    for (offset, value) in logits[from.min(end)..end].iter().enumerate() {
        if *value > best_value {
            best_value = *value;
            best = (from + offset) as u32;
        }
    }
    best
}

pub fn transcribe(
    samples: &[f32],
    sample_rate: u32,
    model_key: &str,
    language: Option<&str>,
    on_progress: &mut dyn FnMut(WhisperProgress),
    should_cancel: &dyn Fn() -> bool,
) -> Result<Transcript, MlError> {
    let spec =
        find_whisper(model_key).ok_or_else(|| MlError::UnknownModel(model_key.to_string()))?;
    if samples.is_empty() {
        return Err(MlError::EmptyAudio);
    }
    if sample_rate == 0 {
        return Err(MlError::EmptyAudio);
    }

    ensure_whisper_downloaded(spec, on_progress, should_cancel)?;
    if should_cancel() {
        return Err(MlError::Cancelled);
    }

    on_progress(WhisperProgress::Loading);
    let mut model = WhisperModel::load(spec)?;

    let audio = mel::resample_to_16k(samples, sample_rate);
    if audio.is_empty() {
        return Err(MlError::EmptyAudio);
    }
    let extractor = MelExtractor::new();
    let chunks = audio.len().div_ceil(CHUNK_SAMPLES).max(1);

    let mut language_code = language.unwrap_or("").to_string();
    let mut segments: Vec<TranscriptSegment> = Vec::new();

    for chunk in 0..chunks {
        if should_cancel() {
            return Err(MlError::Cancelled);
        }
        let start = chunk * CHUNK_SAMPLES;
        let end = (start + CHUNK_SAMPLES).min(audio.len());
        let mut window = vec![0.0f32; CHUNK_SAMPLES];
        window[..end - start].copy_from_slice(&audio[start..end]);

        let mel_data = extractor.log_mel(&window);
        let (states_shape, states) = model.encode(mel_data)?;

        let language_token = if language_code.is_empty() {
            let logits = model.decode_step(
                &[model.specials.start_of_transcript],
                &states_shape,
                &states,
            )?;
            let token = argmax_range(
                &logits,
                model.specials.first_language as usize,
                model.specials.last_language as usize + 1,
            );
            language_code = model
                .tokenizer
                .language_code(token)
                .unwrap_or_else(|| "en".to_string());
            token
        } else {
            model
                .tokenizer
                .special_id(&format!("<|{language_code}|>"))
                .ok_or_else(|| MlError::UnknownLanguage(language_code.clone()))?
        };

        let mut tokens = vec![
            model.specials.start_of_transcript,
            language_token,
            model.specials.transcribe,
        ];
        let prompt_length = tokens.len();
        let mut repeats = 0usize;

        for _ in 0..MAX_TOKENS_PER_CHUNK {
            if should_cancel() {
                return Err(MlError::Cancelled);
            }
            let mut logits = model.decode_step(&tokens, &states_shape, &states)?;
            let suppress_from = model.specials.start_of_transcript as usize;
            let suppress_to = (model.specials.no_timestamps as usize + 1).min(logits.len());
            for value in logits[suppress_from..suppress_to].iter_mut() {
                *value = f32::MIN;
            }

            let next = argmax_range(&logits, 0, logits.len());
            if next == model.specials.end_of_text {
                break;
            }
            if tokens.last() == Some(&next) {
                repeats += 1;
                if repeats >= 8 {
                    break;
                }
            } else {
                repeats = 0;
            }
            tokens.push(next);
        }

        let decode = |ids: &[u32]| model.tokenizer.decode(ids);
        segments.extend(assemble_segments(
            &tokens[prompt_length..],
            model.specials.timestamp_begin,
            chunk as f64 * 30.0,
            &decode,
        ));

        on_progress(WhisperProgress::Transcribing {
            done: ((chunk + 1) as f32 / chunks as f32).clamp(0.0, 1.0),
        });
    }

    let duration = audio.len() as f64 / SAMPLE_RATE as f64;
    for segment in segments.iter_mut() {
        segment.start = segment.start.min(duration);
        segment.end = segment.end.min(duration).max(segment.start);
    }

    Ok(Transcript {
        segments,
        language: if language_code.is_empty() {
            "en".to_string()
        } else {
            language_code
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_the_default_model() {
        assert!(find_whisper(DEFAULT_WHISPER_MODEL).is_some());
        assert!(find_whisper("whisper-tiny").is_some());
        assert!(find_whisper("nope").is_none());
    }

    #[test]
    fn cache_directories_are_per_model() {
        let tiny = find_whisper("whisper-tiny").expect("model");
        let base = find_whisper("whisper-base").expect("model");
        assert_ne!(whisper_directory(tiny), whisper_directory(base));
        assert!(whisper_directory(tiny).ends_with("whisper-tiny"));
    }

    #[test]
    fn timestamp_tokens_convert_to_seconds() {
        assert_eq!(timestamp_token_seconds(50364, 50364), Some(0.0));
        assert_eq!(timestamp_token_seconds(50365, 50364), Some(0.02));
        assert_eq!(timestamp_token_seconds(50364 + 100, 50364), Some(2.0));
        assert_eq!(timestamp_token_seconds(50363, 50364), None);
    }

    fn fake_decode(ids: &[u32]) -> String {
        ids.iter()
            .map(|id| match id {
                1 => " hello",
                2 => " world",
                3 => " again",
                _ => " ?",
            })
            .collect()
    }

    #[test]
    fn segments_are_built_from_timestamp_pairs() {
        let tokens = [50364, 1, 2, 50414, 50414, 3, 50464];
        let segments = assemble_segments(&tokens, 50364, 0.0, &fake_decode);
        assert_eq!(
            segments,
            vec![
                TranscriptSegment {
                    text: "hello world".to_string(),
                    start: 0.0,
                    end: 1.0,
                },
                TranscriptSegment {
                    text: "again".to_string(),
                    start: 1.0,
                    end: 2.0,
                },
            ]
        );
    }

    #[test]
    fn segments_are_offset_by_the_chunk_start() {
        let tokens = [50364, 1, 50414];
        let segments = assemble_segments(&tokens, 50364, 30.0, &fake_decode);
        assert_eq!(segments[0].start, 30.0);
        assert_eq!(segments[0].end, 31.0);
    }

    #[test]
    fn an_unterminated_segment_still_closes() {
        let tokens = [50364, 1, 2];
        let segments = assemble_segments(&tokens, 50364, 0.0, &fake_decode);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].end, 30.0);
    }

    #[test]
    fn empty_segments_are_dropped() {
        let tokens = [50364, 50414, 50414, 1, 50464];
        let segments = assemble_segments(&tokens, 50364, 0.0, &fake_decode);
        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "hello");
    }

    #[test]
    fn argmax_respects_the_restricted_range() {
        let logits = vec![9.0, 1.0, 5.0, 3.0];
        assert_eq!(argmax_range(&logits, 0, 4), 0);
        assert_eq!(argmax_range(&logits, 1, 4), 2);
    }

    #[test]
    fn results_can_cross_thread_boundaries() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Transcript>();
        assert_send_sync::<TranscriptSegment>();
        assert_send_sync::<WhisperProgress>();
        assert_send_sync::<MlError>();
        assert_send_sync::<&'static WhisperSpec>();
    }

    #[test]
    fn transcribe_rejects_empty_audio() {
        let error = transcribe(&[], 16_000, "whisper-tiny", None, &mut |_| {}, &|| false)
            .expect_err("should fail");
        assert!(matches!(error, MlError::EmptyAudio));
    }

    #[test]
    fn transcribe_rejects_unknown_models() {
        let error = transcribe(&[0.0], 16_000, "nope", None, &mut |_| {}, &|| false)
            .expect_err("should fail");
        assert!(error.to_string().contains("nope"));
    }
}
