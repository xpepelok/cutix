use std::fmt;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use video::{VideoStream, first_frame};

use crate::budget::{DECODER_FOOTPRINT, MemoryBudget};
use crate::error::{PlaybackError, Result};

#[derive(Clone)]
pub struct SourceFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,

    pub timestamp: f64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DecodeStats {
    pub opens: u64,
    pub seeks: u64,
    pub requests: u64,
    pub failures: u64,
    pub evictions: u64,
}

struct StreamEntry {
    path: PathBuf,
    stream: VideoStream,
    used: u64,
    generation: u64,
}

struct StillEntry {
    frame: SourceFrame,
    used: u64,
    generation: u64,
}

#[derive(Default)]
pub struct DecodeCache {
    streams: HashMap<String, StreamEntry>,
    stills: HashMap<String, StillEntry>,
    stats: DecodeStats,
    budget: MemoryBudget,
    clock: u64,
    generation: u64,
    still_bytes: usize,
}

impl DecodeCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_budget(budget: MemoryBudget) -> Self {
        Self {
            budget,
            ..Self::default()
        }
    }

    pub fn budget(&self) -> MemoryBudget {
        self.budget
    }

    pub fn set_budget(&mut self, budget: MemoryBudget) {
        self.budget = budget;
        self.enforce_limits();
    }

    pub fn begin_frame(&mut self) {
        self.generation += 1;
    }

    pub fn live_decoders(&self) -> usize {
        self.streams.len()
    }

    pub fn resident_bytes(&self) -> usize {
        self.streams.len() * DECODER_FOOTPRINT + self.still_bytes
    }

    pub fn stats(&self) -> DecodeStats {
        self.stats
    }

    pub fn struggling(&self, media_id: &str) -> bool {
        self.streams
            .get(media_id)
            .is_some_and(|entry| entry.stream.struggling())
    }

    pub fn seek_count(&self, media_id: &str) -> u64 {
        self.streams
            .get(media_id)
            .map(|entry| entry.stream.seek_count())
            .unwrap_or(0)
    }

    pub fn forget(&mut self, media_id: &str) {
        self.streams.remove(media_id);
        if let Some(entry) = self.stills.remove(media_id) {
            self.still_bytes = self.still_bytes.saturating_sub(entry.frame.rgba.len());
        }
    }

    pub fn clear(&mut self) {
        self.streams.clear();
        self.stills.clear();
        self.still_bytes = 0;
    }

    fn tick(&mut self) -> u64 {
        self.clock += 1;
        self.clock
    }

    fn pinned(&self, generation: u64) -> bool {
        self.generation != 0 && generation == self.generation
    }

    fn stream_eviction_candidate(&self) -> Option<String> {
        self.streams
            .iter()
            .filter(|(_, entry)| !self.pinned(entry.generation))
            .min_by_key(|(_, entry)| entry.used)
            .map(|(id, _)| id.clone())
    }

    fn still_eviction_candidate(&self) -> Option<String> {
        self.stills
            .iter()
            .filter(|(_, entry)| !self.pinned(entry.generation))
            .min_by_key(|(_, entry)| entry.used)
            .map(|(id, _)| id.clone())
    }

    fn enforce_limits(&mut self) {
        let max_streams = self.budget.max_decoders();
        while self.streams.len() > max_streams {
            let Some(id) = self.stream_eviction_candidate() else {
                break;
            };
            self.streams.remove(&id);
            self.stats.evictions += 1;
        }

        while self.still_bytes > self.budget.stills && self.stills.len() > 1 {
            let Some(id) = self.still_eviction_candidate() else {
                break;
            };
            if let Some(entry) = self.stills.remove(&id) {
                self.still_bytes = self.still_bytes.saturating_sub(entry.frame.rgba.len());
                self.stats.evictions += 1;
            }
        }
    }

    pub fn video_frame(
        &mut self,
        media_id: &str,
        path: &Path,
        seconds: f64,
    ) -> Result<SourceFrame> {
        self.stats.requests += 1;
        let stale = self
            .streams
            .get(media_id)
            .map(|entry| entry.path != path)
            .unwrap_or(true);
        if stale {
            self.streams.remove(media_id);
            let stream = VideoStream::open(path).map_err(|error| {
                self.stats.failures += 1;
                PlaybackError::Decode(error.to_string())
            })?;
            self.stats.opens += 1;
            let used = self.tick();
            let generation = self.generation;
            self.streams.insert(
                media_id.to_owned(),
                StreamEntry {
                    path: path.to_path_buf(),
                    stream,
                    used,
                    generation,
                },
            );
            self.enforce_limits();
        }

        let used = self.tick();
        let generation = self.generation;
        let entry = self
            .streams
            .get_mut(media_id)
            .expect("stream was just inserted");
        entry.used = used;
        entry.generation = generation;
        let before = entry.stream.seek_count();
        match entry.stream.frame_at(seconds) {
            Ok(frame) => {
                self.stats.seeks += entry.stream.seek_count() - before;
                Ok(SourceFrame {
                    width: frame.width as u32,
                    height: frame.height as u32,
                    rgba: frame.rgba,
                    timestamp: entry.stream.last_timestamp(),
                })
            }
            Err(error) => {
                self.stats.seeks += entry.stream.seek_count() - before;
                self.stats.failures += 1;
                self.streams.remove(media_id);
                Err(PlaybackError::Decode(error.to_string()))
            }
        }
    }

    pub fn still_frame(&mut self, media_id: &str, path: &Path) -> Result<SourceFrame> {
        self.stats.requests += 1;
        let used = self.tick();
        let generation = self.generation;
        if let Some(entry) = self.stills.get_mut(media_id) {
            entry.used = used;
            entry.generation = generation;
            return Ok(entry.frame.clone());
        }
        let frame = load_still(path).inspect_err(|_| {
            self.stats.failures += 1;
        })?;
        self.stats.opens += 1;
        self.still_bytes += frame.rgba.len();
        self.stills.insert(
            media_id.to_owned(),
            StillEntry {
                frame: frame.clone(),
                used,
                generation,
            },
        );
        self.enforce_limits();
        Ok(frame)
    }
}

impl fmt::Debug for SourceFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SourceFrame")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("bytes", &self.rgba.len())
            .finish()
    }
}

fn load_still(path: &Path) -> Result<SourceFrame> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(extension.as_str(), "mp4" | "m4v" | "mov") {
        let frame = first_frame(path).map_err(|error| PlaybackError::Decode(error.to_string()))?;
        return Ok(SourceFrame {
            width: frame.width as u32,
            height: frame.height as u32,
            rgba: frame.rgba,
            timestamp: 0.0,
        });
    }
    let image = image::open(path)
        .map_err(|error| PlaybackError::Decode(error.to_string()))?
        .to_rgba8();
    Ok(SourceFrame {
        width: image.width(),
        height: image.height(),
        rgba: image.into_raw(),
        timestamp: 0.0,
    })
}
