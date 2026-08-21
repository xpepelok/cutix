use std::path::{Path, PathBuf};

use cutix_playback::AudioBuffer;
use time::FrameRate;

use crate::error::Result;
use crate::presets::ExportQuality;

/// What the video track of an export should look like.
#[derive(Clone, Copy, Debug)]
pub struct VideoSpec {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    /// Target bitrate for backends whose rate control uses one.
    ///
    /// Not every encoder can honour this. OpenH264 cannot hold a bitrate without being
    /// allowed to drop frames, so it reads `quality` instead. A backend that ignores this
    /// field must say so rather than appearing to accept it.
    pub bitrate_bps: u32,
    /// The quality the user chose, kept in its own terms rather than only as a bitrate.
    ///
    /// Backends that control quality directly map this to their own knob, so the setting
    /// survives to an encoder that cannot express it as a bitrate.
    pub quality: ExportQuality,
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

    fn is_available(&self) -> bool;

    fn is_hardware(&self) -> bool;

    fn audio_support(&self) -> AudioSupport;

    fn create(
        &self,
        destination: &Path,
        video: VideoSpec,
        audio: Option<AudioSpec>,
    ) -> Result<Box<dyn EncoderBackend>>;
}

pub const STREAM_COPY: &str = "stream-copy";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncoderKind {
    StreamCopy,
    Hardware,
    Software,
}

pub fn encoder_kind(name: &str) -> EncoderKind {
    if name == STREAM_COPY {
        return EncoderKind::StreamCopy;
    }
    #[cfg(not(target_arch = "wasm32"))]
    if name == crate::ffmpeg_mp4::NAME && crate::ffmpeg_mp4::is_hardware() {
        return EncoderKind::Hardware;
    }
    EncoderKind::Software
}

pub fn encoder_label(name: &str) -> String {
    #[cfg(not(target_arch = "wasm32"))]
    if name == crate::ffmpeg_mp4::NAME
        && let Some(chosen) = crate::ffmpeg_mp4::chosen_encoder()
    {
        return chosen.to_owned();
    }
    if name == "openh264-mp4" {
        return "openh264".to_owned();
    }
    name.to_owned()
}

pub fn backends() -> Vec<&'static dyn BackendFactory> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let candidates: [&'static dyn BackendFactory; 2] = [
            &crate::ffmpeg_mp4::FFMPEG_MP4,
            &crate::openh264_mp4::OPENH264_MP4,
        ];
        candidates
            .into_iter()
            .filter(|factory| factory.is_available())
            .collect()
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

pub fn hardware_backend() -> Option<&'static dyn BackendFactory> {
    backends().into_iter().find(|factory| factory.is_hardware())
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
        assert!(
            backends()
                .iter()
                .any(|candidate| candidate.name() == "openh264-mp4")
        );
    }
}
