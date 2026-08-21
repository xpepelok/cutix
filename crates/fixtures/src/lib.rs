use std::path::{Path, PathBuf};

pub const SOURCE: &str = "tears-of-steel-720p.mov";
pub const SUBTITLES_EN: &str = "tos-en.srt";
pub const SUBTITLES_RU: &str = "tos-ru.srt";

pub const SQUARE_CLIP: &str = "clip-square.mp4";
pub const HD_CLIP: &str = "clip-1080p.mp4";
pub const AVCC_ONLY_CLIP: &str = "clip-avcc-only.mp4";
pub const AUDIO_CLIP: &str = "clip-audio.mp3";

pub const VP9_CLIP: &str = "clip-vp9.webm";
pub const VP8_CLIP: &str = "clip-vp8.webm";
pub const AV1_CLIP: &str = "clip-av1.mkv";
pub const HEVC_MKV_CLIP: &str = "clip-hevc.mkv";
pub const HEVC_MP4_CLIP: &str = "clip-hevc.mp4";
pub const HDR10_CLIP: &str = "clip-hdr10.mkv";
pub const RESOLUTION_CHANGE_CLIP: &str = "clip-resolution-change.ts";
pub const RESOLUTION_CHANGE_FIRST: (u32, u32) = (320, 240);
pub const RESOLUTION_CHANGE_SECOND: (u32, u32) = (640, 480);
pub const RESOLUTION_CHANGE_START: f64 = 2.8;
pub const AAC_CLIP: &str = "clip-aac.m4a";
pub const FLAC_CLIP: &str = "clip-flac.flac";

pub const CODEC_CLIPS: &[(&str, &str)] = &[
    (VP9_CLIP, "vp9 in webm"),
    (VP8_CLIP, "vp8 in webm"),
    (AV1_CLIP, "av1 in matroska"),
    (HEVC_MKV_CLIP, "hevc in matroska"),
    (HEVC_MP4_CLIP, "hevc in mp4"),
];

pub const SQUARE_CLIP_WIDTH: u32 = 400;
pub const SQUARE_CLIP_HEIGHT: u32 = 400;
pub const SQUARE_CLIP_FPS: f64 = 25.0;
pub const SQUARE_CLIP_FRAMES: u32 = 75;

pub const HD_CLIP_WIDTH: u32 = 1920;
pub const HD_CLIP_HEIGHT: u32 = 1080;
pub const HD_CLIP_FPS: f64 = 50.0;
pub const HD_CLIP_FRAMES: u32 = 150;

pub const AUDIO_CLIP_END: f64 = 48.0;
pub const AUDIO_DIALOGUE_START: f64 = 22.0;
pub const AUDIO_DIALOGUE_END: f64 = 46.0;

pub const CLIP_SECONDS: f64 = 3.0;

pub fn fixtures_dir() -> Option<PathBuf> {
    let mut directory: &Path = Path::new(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = directory.join(".fixtures");
        if candidate.is_dir() {
            return Some(candidate);
        }
        directory = directory.parent()?;
    }
}

pub fn fixture(name: &str) -> Option<PathBuf> {
    let path = fixtures_dir()?.join(name);
    path.is_file().then_some(path)
}

pub fn require(name: &str) -> PathBuf {
    fixture(name).unwrap_or_else(|| panic!("{}", missing(name)))
}

pub fn missing(name: &str) -> String {
    match fixtures_dir() {
        Some(directory) => format!(
            "'{name}' is not in {}; run `cargo run -p cutix-export --example make_fixtures` or `cargo run -p video --example make_codec_fixtures`",
            directory.display()
        ),
        None => "no .fixtures/ directory; see docs/design/fixtures.md".to_string(),
    }
}

#[macro_export]
macro_rules! fixture_or_skip {
    ($name:expr) => {
        match $crate::fixture($name) {
            Some(path) => path,
            None => {
                eprintln!(
                    "SKIPPED {}:{}: {}",
                    file!(),
                    line!(),
                    $crate::missing($name)
                );
                return;
            }
        }
    };
}
