use std::path::{Path, PathBuf};

use cutix_playback::AudioBuffer;
use time::FrameRate;

use crate::error::Result;

#[derive(Clone, Copy, Debug)]
pub struct VideoSpec {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    pub bitrate_bps: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct AudioSpec {
    pub sample_rate: u32,
    pub channels: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioSupport {
    None,
    Sidecar { extension: &'static str },
    Muxed,
}

#[derive(Clone, Debug, Default)]
pub struct ExportArtifacts {
    pub video_path: Option<PathBuf>,

    pub audio_path: Option<PathBuf>,
    pub frames: u64,

    pub audio_samples: u64,
    pub bytes: u64,
}

pub trait EncoderBackend: Send {
    fn name(&self) -> &'static str;

    fn audio_support(&self) -> AudioSupport;

    fn push_frame(&mut self, rgba: &[u8]) -> Result<()>;

    fn push_audio(&mut self, audio: &AudioBuffer) -> Result<()>;

    fn finish(self: Box<Self>) -> Result<ExportArtifacts>;
}

pub trait BackendFactory: Send + Sync {
    fn name(&self) -> &'static str;

    fn audio_support(&self) -> AudioSupport;

    fn create(
        &self,
        destination: &Path,
        video: VideoSpec,
        audio: Option<AudioSpec>,
    ) -> Result<Box<dyn EncoderBackend>>;
}

pub fn backends() -> Vec<&'static dyn BackendFactory> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        vec![&crate::openh264_mp4::OPENH264_MP4]
    }
    #[cfg(target_arch = "wasm32")]
    {
        Vec::new()
    }
}

pub fn backend_named(name: &str) -> Option<&'static dyn BackendFactory> {
    backends()
        .into_iter()
        .find(|factory| factory.name() == name)
}

pub fn default_backend() -> Option<&'static dyn BackendFactory> {
    backends().into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_backend_is_registered_and_findable_by_name() {
        let factory = default_backend().expect("a backend");
        assert_eq!(
            backend_named(factory.name()).map(|f| f.name()),
            Some(factory.name())
        );
        assert!(backend_named("ffmpeg").is_none());
    }
}
