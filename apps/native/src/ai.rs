use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cutix_playback::{mix, AudioCache, MixRequest, StoreResolver};
use cutix_project::{MediaStore, Project, ProjectStore};
use time::MediaTime;

pub const TRANSCRIPTION_SAMPLE_RATE: u32 = 16_000;
pub const DEFAULT_WORDS_PER_CAPTION: usize = 3;
pub const MIN_CAPTION_DURATION_SECONDS: f64 = 0.8;

#[derive(Clone, Debug, Default)]
pub struct JobStatus {
    pub message: String,
    pub progress: f32,
}

#[derive(Clone)]
pub struct Job {
    pub cancel: Arc<AtomicBool>,
    status: Arc<Mutex<JobStatus>>,
}

impl Job {
    pub fn new(message: String) -> Self {
        Self {
            cancel: Arc::new(AtomicBool::new(false)),
            status: Arc::new(Mutex::new(JobStatus {
                message,
                progress: 0.0,
            })),
        }
    }

    pub fn status(&self) -> JobStatus {
        self.status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_default()
    }

    pub fn publish(&self, message: String, progress: f32) {
        if let Ok(mut status) = self.status.lock() {
            status.message = message;
            status.progress = progress.clamp(0.0, 1.0);
        }
    }

    pub fn request_cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

pub fn timeline_audio(
    project: &Project,
    store: &ProjectStore,
    scene_id: Option<&str>,
) -> Result<Vec<f32>, String> {
    let duration = project.metadata.duration;
    if duration.as_ticks() <= 0 {
        return Err(cutix_i18n::t("captions.diagnostic.noAudio"));
    }

    let resolver = StoreResolver::new(MediaStore::for_project(store, &project.metadata.id));
    let mut cache = AudioCache::new();
    let request = MixRequest {
        project,
        scene_id,
        start: MediaTime::ZERO,
        duration,
        sample_rate: TRANSCRIPTION_SAMPLE_RATE,
        channels: 1,
    };
    let (buffer, skipped) =
        mix(&request, &resolver, &mut cache).map_err(|error| error.to_string())?;
    let peak = buffer.peak();
    if peak <= 1e-5 {
        return Err(if skipped.is_empty() {
            cutix_i18n::t("captions.diagnostic.noAudio")
        } else {
            skipped.join(", ")
        });
    }
    Ok(buffer.interleaved)
}

pub fn resample_linear(samples: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || from == 0 || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from as f64 / to as f64;
    let count = ((samples.len() as f64) / ratio).round().max(1.0) as usize;
    (0..count)
        .map(|index| {
            let position = index as f64 * ratio;
            let lower = position.floor() as usize;
            if lower + 1 >= samples.len() {
                return *samples.last().unwrap_or(&0.0);
            }
            let fraction = (position - lower as f64) as f32;
            samples[lower] * (1.0 - fraction) + samples[lower + 1] * fraction
        })
        .collect()
}

pub fn interleave(samples: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return samples.to_vec();
    }
    let mut out = Vec::with_capacity(samples.len() * channels);
    for sample in samples {
        for _ in 0..channels {
            out.push(*sample);
        }
    }
    out
}

pub fn caption_chunks(
    segments: &[(String, f64, f64)],
    words_per_chunk: usize,
    min_duration: f64,
) -> Vec<cutix_project::SubtitleCue> {
    let words_per_chunk = words_per_chunk.max(1);
    let mut captions = Vec::new();
    let mut global_end = 0.0f64;

    for (text, start, end) in segments {
        let words: Vec<&str> = text.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }
        let segment_duration = (end - start).max(1e-6);
        let words_per_second = words.len() as f64 / segment_duration;

        let mut chunk_start = *start;
        for chunk in words.chunks(words_per_chunk) {
            let chunk_duration = min_duration.max(chunk.len() as f64 / words_per_second);
            let adjusted_start = chunk_start.max(global_end);
            captions.push(cutix_project::SubtitleCue::new(
                chunk.join(" "),
                adjusted_start,
                chunk_duration,
            ));
            global_end = adjusted_start + chunk_duration;
            chunk_start += chunk_duration;
        }
    }

    captions
}

pub fn write_temp_wav(samples: &[f32], sample_rate: u32, stem: &str) -> Result<PathBuf, String> {
    let directory = std::env::temp_dir().join("cutix-speech");
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let name = format!("{stem}-{}.wav", uuid::Uuid::new_v4());
    let path = directory.join(name);
    std::fs::write(&path, speech::to_wav(samples, sample_rate))
        .map_err(|error| error.to_string())?;
    Ok(path)
}

pub fn sanitise_asset_stem(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == ' ' || character == '-' {
                character
            } else {
                ' '
            }
        })
        .collect();
    let trimmed: String = cleaned.split_whitespace().collect::<Vec<_>>().join("-");
    let short: String = trimmed.chars().take(32).collect();
    if short.is_empty() {
        cutix_i18n::t("speech.assetName")
    } else {
        short
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_split_on_the_word_budget() {
        let cues = caption_chunks(
            &[(String::from("one two three four five six"), 0.0, 6.0)],
            3,
            0.8,
        );
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].text, "one two three");
        assert_eq!(cues[1].text, "four five six");
        assert!((cues[0].duration - 3.0).abs() < 1e-6);
    }

    #[test]
    fn chunks_never_overlap_across_segments() {
        let cues = caption_chunks(
            &[
                (String::from("first"), 0.0, 2.0),
                (String::from("second"), 1.0, 3.0),
            ],
            3,
            0.8,
        );
        assert_eq!(cues.len(), 2);
        assert!(cues[1].start_time >= cues[0].start_time + cues[0].duration - 1e-9);
    }

    #[test]
    fn short_chunks_get_the_minimum_duration() {
        let cues = caption_chunks(&[(String::from("hi"), 0.0, 0.1)], 3, 0.8);
        assert_eq!(cues.len(), 1);
        assert!((cues[0].duration - 0.8).abs() < 1e-9);
    }

    #[test]
    fn empty_segments_are_dropped() {
        assert!(caption_chunks(&[(String::from("   "), 0.0, 1.0)], 3, 0.8).is_empty());
    }

    #[test]
    fn resampling_halves_the_frame_count() {
        let samples = vec![0.0, 1.0, 0.0, -1.0, 0.0, 1.0, 0.0, -1.0];
        let out = resample_linear(&samples, 32_000, 16_000);
        assert_eq!(out.len(), 4);
        assert_eq!(
            resample_linear(&samples, 16_000, 16_000).len(),
            samples.len()
        );
    }

    #[test]
    fn interleaving_duplicates_the_mono_channel() {
        assert_eq!(interleave(&[1.0, 2.0], 2), vec![1.0, 1.0, 2.0, 2.0]);
        assert_eq!(interleave(&[1.0, 2.0], 1), vec![1.0, 2.0]);
    }

    #[test]
    fn asset_stems_stay_file_safe() {
        assert_eq!(sanitise_asset_stem("Hello, world!"), "Hello-world");
        assert_eq!(
            sanitise_asset_stem("///"),
            cutix_i18n::t("speech.assetName")
        );
    }

    #[test]
    fn a_job_reports_what_it_published() {
        let job = Job::new(String::from("start"));
        assert_eq!(job.status().message, "start");
        job.publish(String::from("half"), 0.5);
        assert_eq!(job.status().progress, 0.5);
        assert!(!job.is_cancelled());
        job.request_cancel();
        assert!(job.is_cancelled());
    }
}
