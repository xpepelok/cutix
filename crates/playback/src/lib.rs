pub mod animation;
mod budget;
pub mod curve;
mod effects_map;
mod error;
mod media;
mod raster_cache;
pub mod resolve;
pub mod retime;
pub mod text_render;
pub mod transitions;

#[cfg(not(target_arch = "wasm32"))]
pub mod audio_decode;
#[cfg(not(target_arch = "wasm32"))]
mod clock;
#[cfg(not(target_arch = "wasm32"))]
mod controller;
#[cfg(not(target_arch = "wasm32"))]
pub mod decode_cache;
#[cfg(not(target_arch = "wasm32"))]
mod mix;
#[cfg(not(target_arch = "wasm32"))]
pub mod output;
#[cfg(not(target_arch = "wasm32"))]
mod queue;
#[cfg(not(target_arch = "wasm32"))]
pub mod render;
#[cfg(not(target_arch = "wasm32"))]
mod waveform;

pub use budget::{DECODER_FOOTPRINT, MemoryBudget};
pub use error::{PlaybackError, Result};
pub use media::{MediaMap, MediaResolver, StoreResolver};
pub use raster_cache::RasterCache;

#[cfg(not(target_arch = "wasm32"))]
pub use audio_decode::{AudioCache, PcmBuffer, decode_audio, decode_audio_window};
#[cfg(not(target_arch = "wasm32"))]
pub use controller::{PlaybackController, QUEUE_DEPTH};
#[cfg(not(target_arch = "wasm32"))]
pub use decode_cache::{DecodeCache, DecodeStats, SourceFrame};
#[cfg(not(target_arch = "wasm32"))]
pub use mix::{AudioBuffer, MixRequest, db_to_linear, mix};
#[cfg(not(target_arch = "wasm32"))]
pub use output::AudioOutput;
#[cfg(not(target_arch = "wasm32"))]
pub use queue::{FrameSlot, PlaybackGeneration};
#[cfg(not(target_arch = "wasm32"))]
pub use render::{
    CacheBytes, ComposeRequest, ComposedFrame, ElementRect, FrameComposer, PendingFrame,
};
pub use text_render::{TextLayer, TextRasterizer};
#[cfg(not(target_arch = "wasm32"))]
pub use waveform::{DEFAULT_BUCKETS, WaveformPeaks, peaks_for_file, peaks_from_pcm};
