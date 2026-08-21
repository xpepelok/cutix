use std::collections::BTreeSet;
use std::path::Path;

use crate::color::ColorSpec;
use crate::decode::{self, DecodeError, Frame, VideoInfo};
use crate::ffmpeg;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum BackendKind {
    OpenH264,
    Ffmpeg,
}

impl BackendKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::OpenH264 => "openh264",
            Self::Ffmpeg => "ffmpeg",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    video_containers: BTreeSet<&'static str>,
    audio_containers: BTreeSet<&'static str>,
}

impl Capabilities {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn new(video: &[&'static str], audio: &[&'static str]) -> Self {
        Self {
            video_containers: video.iter().copied().collect(),
            audio_containers: audio.iter().copied().collect(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.video_containers.is_empty() && self.audio_containers.is_empty()
    }

    pub fn opens_video(&self, extension: &str) -> bool {
        self.video_containers.contains(extension)
    }

    pub fn opens_audio(&self, extension: &str) -> bool {
        self.audio_containers.contains(extension)
    }

    pub fn video_containers(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.video_containers.iter().copied()
    }

    pub fn audio_containers(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.audio_containers.iter().copied()
    }

    pub fn union(mut self, other: &Self) -> Self {
        self.video_containers.extend(other.video_containers.iter());
        self.audio_containers.extend(other.audio_containers.iter());
        self
    }
}

pub trait DecodeBackend {
    fn kind(&self) -> BackendKind;
    fn capabilities(&self) -> Capabilities;
    fn probe(&self, path: &Path) -> Result<VideoInfo, DecodeError>;
    fn frame_at(
        &self,
        path: &Path,
        seconds: f64,
        override_spec: Option<ColorSpec>,
    ) -> Result<Frame, DecodeError>;
}

pub struct OpenH264Backend;

const OPENH264_CONTAINERS: &[&str] = &["mp4", "m4v", "mov"];

impl DecodeBackend for OpenH264Backend {
    fn kind(&self) -> BackendKind {
        BackendKind::OpenH264
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities::new(OPENH264_CONTAINERS, &[])
    }

    fn probe(&self, path: &Path) -> Result<VideoInfo, DecodeError> {
        decode::probe(path)
    }

    fn frame_at(
        &self,
        path: &Path,
        seconds: f64,
        override_spec: Option<ColorSpec>,
    ) -> Result<Frame, DecodeError> {
        decode::frame_at_with_color(path, seconds, override_spec)
    }
}

pub struct FfmpegBackend;

impl DecodeBackend for FfmpegBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Ffmpeg
    }

    fn capabilities(&self) -> Capabilities {
        if ffmpeg::can_decode() {
            Capabilities::new(ffmpeg::CONTAINERS, ffmpeg::AUDIO_CONTAINERS)
                .union(&Capabilities::new(OPENH264_CONTAINERS, &[]))
        } else {
            Capabilities::none()
        }
    }

    fn probe(&self, path: &Path) -> Result<VideoInfo, DecodeError> {
        if !ffmpeg::is_available() {
            return Err(unavailable_error());
        }
        ffmpeg::probe(path)
    }

    fn frame_at(
        &self,
        path: &Path,
        seconds: f64,
        override_spec: Option<ColorSpec>,
    ) -> Result<Frame, DecodeError> {
        if !ffmpeg::is_available() {
            return Err(unavailable_error());
        }
        ffmpeg::frame_at_with_color(path, seconds, override_spec)
    }
}

fn unavailable_error() -> DecodeError {
    DecodeError::Backend(ffmpeg::status())
}

pub fn backends() -> Vec<Box<dyn DecodeBackend>> {
    vec![Box::new(OpenH264Backend), Box::new(FfmpegBackend)]
}

fn prefers_ffmpeg(path: &Path) -> bool {
    match decode::h264_profile(path) {
        Some(profile) => profile != decode::BASELINE_PROFILE,

        None => false,
    }
}

pub fn capabilities() -> Capabilities {
    backends()
        .iter()
        .fold(Capabilities::none(), |merged, backend| {
            merged.union(&backend.capabilities())
        })
}

pub fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

pub fn backends_for(path: &Path) -> Vec<Box<dyn DecodeBackend>> {
    let extension = extension_of(path);
    let mut candidates: Vec<Box<dyn DecodeBackend>> = backends()
        .into_iter()
        .filter(|backend| backend.capabilities().opens_video(&extension))
        .collect();

    if candidates.len() > 1 && prefers_ffmpeg(path) {
        candidates.sort_by_key(|backend| match backend.kind() {
            BackendKind::Ffmpeg => 0,
            BackendKind::OpenH264 => 1,
        });
    }

    candidates
}

pub fn backend_for(path: &Path) -> Option<Box<dyn DecodeBackend>> {
    backends_for(path).into_iter().next()
}

pub fn opens_video(extension: &str) -> bool {
    capabilities().opens_video(extension)
}

pub fn opens_audio(extension: &str) -> bool {
    capabilities().opens_audio(extension)
}

pub fn video_extensions() -> Vec<&'static str> {
    capabilities().video_containers().collect()
}

pub fn audio_extensions() -> Vec<&'static str> {
    capabilities().audio_containers().collect()
}

pub fn probe(path: impl AsRef<Path>) -> Result<VideoInfo, DecodeError> {
    let path = path.as_ref();
    let mut last: Option<DecodeError> = None;
    for backend in backends_for(path) {
        match backend.probe(path) {
            Ok(info) => return Ok(info),
            Err(error) => last = Some(error),
        }
    }
    Err(last.unwrap_or_else(|| no_backend_error(path)))
}

pub fn dynamic_range(path: impl AsRef<Path>) -> Option<ffmpeg::DynamicRange> {
    let path = path.as_ref();
    if !ffmpeg::is_available() {
        return backend_for(path).map(|_| ffmpeg::DynamicRange::SDR_8_BIT);
    }
    match ffmpeg::dynamic_range(path) {
        Ok(range) => Some(range),
        Err(_) => backend_for(path).map(|_| ffmpeg::DynamicRange::SDR_8_BIT),
    }
}

/// An open video source, decoded by whichever backend could read it.
///
/// The two variants are far apart in size because the FFmpeg one carries its scaler cache
/// and frame buffers inline. They are not boxed: exactly one of these exists per open
/// clip, it is created once and then read from for the life of the clip, so the size of
/// the enum costs a single stack slot while boxing would add an indirection to every
/// frame read.
#[allow(clippy::large_enum_variant)]
pub enum VideoStream {
    Native(decode::NativeStream),
    Ffmpeg(ffmpeg::FfmpegStream),
}

impl VideoStream {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DecodeError> {
        let path = path.as_ref();
        let mut last: Option<DecodeError> = None;
        for backend in backends_for(path) {
            let opened = match backend.kind() {
                BackendKind::OpenH264 => decode::NativeStream::open(path).map(Self::Native),
                BackendKind::Ffmpeg => {
                    if ffmpeg::is_available() {
                        ffmpeg::FfmpegStream::open(path).map(Self::Ffmpeg)
                    } else {
                        Err(unavailable_error())
                    }
                }
            };
            match opened {
                Ok(stream) => return Ok(stream),
                Err(error) => last = Some(error),
            }
        }
        Err(last.unwrap_or_else(|| no_backend_error(path)))
    }

    pub fn backend(&self) -> BackendKind {
        match self {
            Self::Native(_) => BackendKind::OpenH264,
            Self::Ffmpeg(_) => BackendKind::Ffmpeg,
        }
    }

    pub fn info(&self) -> &VideoInfo {
        match self {
            Self::Native(stream) => stream.info(),
            Self::Ffmpeg(stream) => stream.info(),
        }
    }

    pub fn dynamic_range(&self) -> ffmpeg::DynamicRange {
        match self {
            Self::Native(_) => ffmpeg::DynamicRange::SDR_8_BIT,
            Self::Ffmpeg(stream) => stream.dynamic_range(),
        }
    }

    pub fn seek_count(&self) -> u64 {
        match self {
            Self::Native(stream) => stream.seek_count(),
            Self::Ffmpeg(stream) => stream.seek_count(),
        }
    }

    pub fn decoded_sample_count(&self) -> u64 {
        match self {
            Self::Native(stream) => stream.decoded_sample_count(),
            Self::Ffmpeg(stream) => stream.decoded_sample_count(),
        }
    }

    pub fn struggling(&self) -> bool {
        match self {
            Self::Native(stream) => {
                let (decoded, refused) = stream.decode_health();
                decoded + refused > 30 && refused > decoded * 2
            }
            Self::Ffmpeg(_) => false,
        }
    }

    pub fn last_timestamp(&self) -> f64 {
        match self {
            Self::Native(stream) => stream.last_timestamp(),
            Self::Ffmpeg(stream) => stream.last_timestamp(),
        }
    }

    pub fn frame_at(&mut self, seconds: f64) -> Result<Frame, DecodeError> {
        self.frame_at_with_color(seconds, None)
    }

    pub fn frame_at_with_color(
        &mut self,
        seconds: f64,
        override_spec: Option<ColorSpec>,
    ) -> Result<Frame, DecodeError> {
        match self {
            Self::Native(stream) => stream.frame_at_with_color(seconds, override_spec),
            Self::Ffmpeg(stream) => stream.frame_at_with_color(seconds, override_spec),
        }
    }
}

fn no_backend_error(path: &Path) -> DecodeError {
    DecodeError::UnsupportedContainer(format!(
        "no available backend opens '{}' ({})",
        extension_of(path),
        ffmpeg::status()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openh264_serves_the_mp4_family() {
        let backend = OpenH264Backend;
        let capabilities = backend.capabilities();
        for extension in ["mp4", "m4v", "mov"] {
            assert!(
                capabilities.opens_video(extension),
                "openh264 should open {extension}"
            );
        }
        assert!(!capabilities.opens_video("webm"));
    }

    #[test]
    fn openh264_is_chosen_for_the_mp4_family() {
        for name in ["a.mp4", "b.m4v", "c.mov", "d.MOV"] {
            let backend = backend_for(Path::new(name)).expect("a backend for {name}");
            assert_eq!(backend.kind(), BackendKind::OpenH264);
        }
    }

    #[test]
    fn ffmpeg_capabilities_track_whether_it_can_actually_decode() {
        assert_eq!(
            FfmpegBackend.capabilities().is_empty(),
            !ffmpeg::can_decode(),
            "capabilities must be empty exactly when ffmpeg cannot decode"
        );
    }

    #[test]
    fn an_unservable_container_is_refused_not_half_opened() {
        let container = if ffmpeg::can_decode() {
            "whatever.thisisnotacontainer"
        } else {
            "whatever.webm"
        };
        assert!(matches!(
            VideoStream::open(Path::new(container)),
            Err(DecodeError::UnsupportedContainer(_))
        ));
        assert!(matches!(
            probe(Path::new(container)),
            Err(DecodeError::UnsupportedContainer(_))
        ));
    }

    #[test]
    fn advertised_extensions_are_exactly_what_some_backend_opens() {
        for extension in video_extensions() {
            assert!(
                backend_for(Path::new(&format!("sample.{extension}"))).is_some(),
                "{extension} is advertised but no backend claims it"
            );
        }
    }

    #[test]
    fn capabilities_union_merges_both_sides() {
        let left = Capabilities::new(&["mp4"], &[]);
        let right = Capabilities::new(&["webm"], &["flac"]);
        let merged = left.union(&right);
        assert!(merged.opens_video("mp4"));
        assert!(merged.opens_video("webm"));
        assert!(merged.opens_audio("flac"));
        assert!(!merged.opens_audio("mp4"));
    }

    #[test]
    fn an_empty_capability_set_claims_nothing() {
        let capabilities = Capabilities::none();
        assert!(capabilities.is_empty());
        assert!(!capabilities.opens_video("mp4"));
        assert!(!capabilities.opens_audio("aac"));
    }
}
