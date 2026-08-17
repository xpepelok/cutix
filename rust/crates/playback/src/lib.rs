pub mod animation;
pub mod budget;
pub mod curve;
pub mod effects_map;
pub mod error;
pub mod media;
pub mod raster_cache;
pub mod resolve;
pub mod retime;
pub mod text_render;
pub mod transitions;

#[cfg(not(target_arch = "wasm32"))]
pub mod audio_decode;
#[cfg(not(target_arch = "wasm32"))]
pub mod controller;
#[cfg(not(target_arch = "wasm32"))]
pub mod decode_cache;
#[cfg(not(target_arch = "wasm32"))]
pub mod mix;
#[cfg(not(target_arch = "wasm32"))]
pub mod output;
#[cfg(not(target_arch = "wasm32"))]
pub mod render;
#[cfg(not(target_arch = "wasm32"))]
pub mod waveform;

pub use budget::MemoryBudget;
pub use error::{PlaybackError, Result};
pub use media::{MediaMap, MediaResolver, StoreResolver};
pub use raster_cache::RasterCache;

#[cfg(not(target_arch = "wasm32"))]
pub use audio_decode::{decode_audio, decode_audio_window, AudioCache, PcmBuffer};
#[cfg(not(target_arch = "wasm32"))]
pub use controller::{FrameSlot, PlaybackController};
#[cfg(not(target_arch = "wasm32"))]
pub use decode_cache::{DecodeCache, DecodeStats, SourceFrame};
#[cfg(not(target_arch = "wasm32"))]
pub use mix::{db_to_linear, mix, AudioBuffer, MixRequest};
#[cfg(not(target_arch = "wasm32"))]
pub use output::AudioOutput;
#[cfg(not(target_arch = "wasm32"))]
pub use render::{
    CacheBytes, ComposeRequest, ComposedFrame, ElementRect, FrameComposer, PendingFrame,
};
pub use text_render::{TextLayer, TextRasterizer};
#[cfg(not(target_arch = "wasm32"))]
pub use waveform::{peaks_for_file, peaks_from_pcm, WaveformPeaks, DEFAULT_BUCKETS};
