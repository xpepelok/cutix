mod audio;
mod encode;
pub mod sys;
mod video;

pub use audio::{decode_audio, decode_audio_range, probe_audio, AudioBuffer, AudioInfo};
pub use encode::{encode_aac, frame_channel_layout_offset, AAC_FRAME_SAMPLES};
pub use video::{dynamic_range, frame_at_with_color, probe, DynamicRange, FfmpegStream, Transfer};

use std::env;
use std::ffi::c_void;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use libloading::Library;

pub const DECODE_IMPLEMENTED: bool = true;

pub const DIR_ENV: &str = "CUTIX_FFMPEG_DIR";
pub const DISABLE_ENV: &str = "CUTIX_DISABLE_FFMPEG";

pub const MINIMUM_AVCODEC_MAJOR: u32 = 58;

pub const CONTAINERS: &[&str] = &[
    "webm", "mkv", "avi", "ts", "m2ts", "mts", "flv", "ogv", "wmv", "mpg", "mpeg", "3gp",
];

pub const AUDIO_CONTAINERS: &[&str] = &["aac", "flac", "ogg", "opus", "wma", "m4a"];

const STEMS: [&str; 5] = ["avutil", "swresample", "swscale", "avcodec", "avformat"];

#[derive(Debug)]
pub enum Unavailable {
    Disabled,
    NotFound(String),
    Unusable(String),
}

impl fmt::Display for Unavailable {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disabled => write!(formatter, "FFmpeg disabled by {DISABLE_ENV}"),
            Self::NotFound(detail) => write!(formatter, "FFmpeg libraries not found: {detail}"),
            Self::Unusable(detail) => write!(formatter, "FFmpeg libraries unusable: {detail}"),
        }
    }
}

#[allow(non_snake_case)]
pub struct Api {
    pub av_frame_alloc: sys::AvFrameAlloc,
    pub av_frame_free: sys::AvFrameFree,
    pub av_frame_unref: sys::AvFrameUnref,
    pub avformat_open_input: sys::AvformatOpenInput,
    pub avformat_close_input: sys::AvformatCloseInput,
    pub avformat_find_stream_info: sys::AvformatFindStreamInfo,
    pub av_read_frame: sys::AvReadFrame,
    pub avformat_seek_file: sys::AvformatSeekFile,
    pub av_find_best_stream: sys::AvFindBestStream,
    pub avcodec_find_decoder: sys::AvcodecFindDecoder,
    pub av_pix_fmt_desc_get: sys::AvPixFmtDescGet,
    pub avcodec_alloc_context3: sys::AvcodecAllocContext3,
    pub avcodec_free_context: sys::AvcodecFreeContext,
    pub avcodec_parameters_to_context: sys::AvcodecParametersToContext,
    pub avcodec_open2: sys::AvcodecOpen2,
    pub avcodec_send_packet: sys::AvcodecSendPacket,
    pub avcodec_receive_frame: sys::AvcodecReceiveFrame,
    pub avcodec_flush_buffers: sys::AvcodecFlushBuffers,
    pub av_packet_alloc: sys::AvPacketAlloc,
    pub av_packet_free: sys::AvPacketFree,
    pub av_packet_unref: sys::AvPacketUnref,
    pub sws_getContext: sys::SwsGetContext,
    pub sws_scale: sys::SwsScale,
    pub sws_freeContext: sys::SwsFreeContext,
    pub swr_alloc_set_opts2: sys::SwrAllocSetOpts2,
    pub swr_init: sys::SwrInit,
    pub swr_convert: sys::SwrConvert,
    pub swr_free: sys::SwrFree,
}

pub struct EncoderApi {
    pub avcodec_find_encoder_by_name: sys::AvcodecFindEncoderByName,
    pub avcodec_send_frame: sys::AvcodecSendFrame,
    pub avcodec_receive_packet: sys::AvcodecReceivePacket,
    pub avcodec_parameters_alloc: sys::AvcodecParametersAlloc,
    pub avcodec_parameters_free: sys::AvcodecParametersFree,
    pub av_frame_get_buffer: sys::AvFrameGetBuffer,
}

pub struct Ffmpeg {
    api: Api,
    encoder: Option<EncoderApi>,
    avcodec_major: u32,
    origin: String,
    _libraries: Vec<Library>,
}

impl Ffmpeg {
    pub fn api(&self) -> &Api {
        &self.api
    }

    pub fn encoder_api(&self) -> Option<&EncoderApi> {
        self.encoder.as_ref()
    }

    pub fn avcodec_major(&self) -> u32 {
        self.avcodec_major
    }

    pub fn origin(&self) -> &str {
        &self.origin
    }
}

unsafe impl Send for Ffmpeg {}
unsafe impl Sync for Ffmpeg {}

fn is_disabled() -> bool {
    match env::var(DISABLE_ENV) {
        Ok(value) => {
            let value = value.trim().to_ascii_lowercase();
            !value.is_empty() && value != "0" && value != "false"
        }
        Err(_) => false,
    }
}

fn configured_directory() -> Option<PathBuf> {
    let raw = env::var_os(DIR_ENV)?;
    if raw.is_empty() {
        return None;
    }
    Some(PathBuf::from(raw))
}

fn prefix_and_suffix(stem: &str) -> (String, &'static str) {
    if cfg!(target_os = "windows") {
        (format!("{stem}-"), ".dll")
    } else if cfg!(target_os = "macos") {
        (format!("lib{stem}."), ".dylib")
    } else {
        (format!("lib{stem}.so."), "")
    }
}

fn unversioned_name(stem: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{stem}.dll")
    } else if cfg!(target_os = "macos") {
        format!("lib{stem}.dylib")
    } else {
        format!("lib{stem}.so")
    }
}

pub fn version_from_file_name(stem: &str, file_name: &str) -> Option<u32> {
    let (prefix, suffix) = prefix_and_suffix(stem);
    let rest = file_name.strip_prefix(&prefix)?;
    let digits = if suffix.is_empty() {
        rest
    } else {
        rest.strip_suffix(suffix)?
    };
    let digits = digits.split('.').next()?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

fn newest_in_directory(directory: &Path, stem: &str) -> Option<PathBuf> {
    let mut best: Option<(u32, PathBuf)> = None;

    for entry in fs::read_dir(directory).ok()?.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(version) = version_from_file_name(stem, name) else {
            continue;
        };
        if best.as_ref().is_none_or(|(current, _)| version > *current) {
            best = Some((version, entry.path()));
        }
    }

    if let Some((_, path)) = best {
        return Some(path);
    }

    let fallback = directory.join(unversioned_name(stem));
    fallback.is_file().then_some(fallback)
}

#[cfg(target_os = "windows")]
fn load_library(path: &Path) -> Result<Library, String> {
    const LOAD_WITH_ALTERED_SEARCH_PATH: u32 = 0x0000_0008;
    unsafe {
        libloading::os::windows::Library::load_with_flags(path, LOAD_WITH_ALTERED_SEARCH_PATH)
    }
    .map(Library::from)
    .map_err(|error| format!("{}: {error}", path.display()))
}

#[cfg(not(target_os = "windows"))]
fn load_library(path: &Path) -> Result<Library, String> {
    unsafe { Library::new(path) }.map_err(|error| format!("{}: {error}", path.display()))
}

fn open_from_directory(directory: &Path, stem: &str) -> Result<Library, String> {
    let path = newest_in_directory(directory, stem)
        .ok_or_else(|| format!("no {stem} library in {}", directory.display()))?;
    load_library(&path)
}

fn open_from_system(stem: &str) -> Result<Library, String> {
    let (prefix, suffix) = prefix_and_suffix(stem);
    let mut attempts = Vec::new();

    for major in (MINIMUM_AVCODEC_MAJOR..=MINIMUM_AVCODEC_MAJOR + 16).rev() {
        let name = format!("{prefix}{major}{suffix}");
        match unsafe { Library::new(&name) } {
            Ok(library) => return Ok(library),
            Err(_) => attempts.push(name),
        }
    }

    let name = unversioned_name(stem);
    match unsafe { Library::new(&name) } {
        Ok(library) => Ok(library),
        Err(error) => Err(format!(
            "{stem}: {error} ({} versioned candidates also failed)",
            attempts.len()
        )),
    }
}

pub const BUNDLE_SUBDIRECTORY: &str = "ffmpeg";

pub fn holds_every_library(directory: &Path) -> bool {
    directory.is_dir()
        && STEMS
            .iter()
            .all(|stem| newest_in_directory(directory, stem).is_some())
}

fn bundled_directories() -> Vec<PathBuf> {
    let Ok(executable) = env::current_exe() else {
        return Vec::new();
    };
    let Some(base) = executable.parent() else {
        return Vec::new();
    };
    vec![base.join(BUNDLE_SUBDIRECTORY), base.to_path_buf()]
}

fn load() -> Result<Ffmpeg, Unavailable> {
    if is_disabled() {
        return Err(Unavailable::Disabled);
    }

    let directory = configured_directory();
    if let Some(directory) = directory.as_deref() {
        if !directory.is_dir() {
            return Err(Unavailable::NotFound(format!(
                "{DIR_ENV} is not a directory: {}",
                directory.display()
            )));
        }
    }

    let directory = directory.or_else(|| {
        bundled_directories()
            .into_iter()
            .find(|candidate| holds_every_library(candidate))
    });

    let origin = match directory.as_deref() {
        Some(directory) => directory.display().to_string(),
        None => "system library path".to_string(),
    };

    let mut libraries = Vec::new();
    for stem in STEMS {
        let library = match directory.as_deref() {
            Some(directory) => open_from_directory(directory, stem),
            None => open_from_system(stem),
        }
        .map_err(Unavailable::NotFound)?;
        libraries.push((stem, library));
    }

    let find = |wanted: &str| {
        libraries
            .iter()
            .find(|(stem, _)| *stem == wanted)
            .map(|(_, library)| library)
            .expect("every stem in STEMS is loaded")
    };
    let avutil = find("avutil");
    let avcodec = find("avcodec");
    let avformat = find("avformat");
    let swscale = find("swscale");
    let swresample = find("swresample");

    let avcodec_version = unsafe { symbol::<sys::AvcodecVersion>(avcodec, b"avcodec_version\0") }
        .map_err(Unavailable::Unusable)?;
    let avcodec_major = unsafe { avcodec_version() } >> 16;
    if avcodec_major < MINIMUM_AVCODEC_MAJOR {
        return Err(Unavailable::Unusable(format!(
            "libavcodec major {avcodec_major} is older than the minimum {MINIMUM_AVCODEC_MAJOR}"
        )));
    }

    let api = unsafe { bind(avutil, avcodec, avformat, swscale, swresample) }
        .map_err(Unavailable::Unusable)?;
    let encoder = unsafe { bind_encoder(avutil, avcodec) }.ok();
    let _ = find;

    Ok(Ffmpeg {
        api,
        encoder,
        avcodec_major,
        origin,
        _libraries: libraries.into_iter().map(|(_, library)| library).collect(),
    })
}

unsafe fn symbol<T: Copy>(library: &Library, name: &[u8]) -> Result<T, String> {
    let looked_up: libloading::Symbol<'_, T> = unsafe { library.get(name) }.map_err(|error| {
        format!(
            "{} missing: {error}",
            String::from_utf8_lossy(&name[..name.len().saturating_sub(1)])
        )
    })?;
    Ok(*looked_up)
}

unsafe fn bind(
    avutil: &Library,
    avcodec: &Library,
    avformat: &Library,
    swscale: &Library,
    swresample: &Library,
) -> Result<Api, String> {
    unsafe {
        Ok(Api {
            av_frame_alloc: symbol(avutil, b"av_frame_alloc\0")?,
            av_frame_free: symbol(avutil, b"av_frame_free\0")?,
            av_frame_unref: symbol(avutil, b"av_frame_unref\0")?,
            avformat_open_input: symbol(avformat, b"avformat_open_input\0")?,
            avformat_close_input: symbol(avformat, b"avformat_close_input\0")?,
            avformat_find_stream_info: symbol(avformat, b"avformat_find_stream_info\0")?,
            av_read_frame: symbol(avformat, b"av_read_frame\0")?,
            avformat_seek_file: symbol(avformat, b"avformat_seek_file\0")?,
            av_find_best_stream: symbol(avformat, b"av_find_best_stream\0")?,
            avcodec_find_decoder: symbol(avcodec, b"avcodec_find_decoder\0")?,
            av_pix_fmt_desc_get: symbol(avutil, b"av_pix_fmt_desc_get\0")?,
            avcodec_alloc_context3: symbol(avcodec, b"avcodec_alloc_context3\0")?,
            avcodec_free_context: symbol(avcodec, b"avcodec_free_context\0")?,
            avcodec_parameters_to_context: symbol(avcodec, b"avcodec_parameters_to_context\0")?,
            avcodec_open2: symbol(avcodec, b"avcodec_open2\0")?,
            avcodec_send_packet: symbol(avcodec, b"avcodec_send_packet\0")?,
            avcodec_receive_frame: symbol(avcodec, b"avcodec_receive_frame\0")?,
            avcodec_flush_buffers: symbol(avcodec, b"avcodec_flush_buffers\0")?,
            av_packet_alloc: symbol(avcodec, b"av_packet_alloc\0")?,
            av_packet_free: symbol(avcodec, b"av_packet_free\0")?,
            av_packet_unref: symbol(avcodec, b"av_packet_unref\0")?,
            sws_getContext: symbol(swscale, b"sws_getContext\0")?,
            sws_scale: symbol(swscale, b"sws_scale\0")?,
            sws_freeContext: symbol(swscale, b"sws_freeContext\0")?,
            swr_alloc_set_opts2: symbol(swresample, b"swr_alloc_set_opts2\0")?,
            swr_init: symbol(swresample, b"swr_init\0")?,
            swr_convert: symbol(swresample, b"swr_convert\0")?,
            swr_free: symbol(swresample, b"swr_free\0")?,
        })
    }
}

unsafe fn bind_encoder(avutil: &Library, avcodec: &Library) -> Result<EncoderApi, String> {
    unsafe {
        Ok(EncoderApi {
            avcodec_find_encoder_by_name: symbol(avcodec, b"avcodec_find_encoder_by_name\0")?,
            avcodec_send_frame: symbol(avcodec, b"avcodec_send_frame\0")?,
            avcodec_receive_packet: symbol(avcodec, b"avcodec_receive_packet\0")?,
            avcodec_parameters_alloc: symbol(avcodec, b"avcodec_parameters_alloc\0")?,
            avcodec_parameters_free: symbol(avcodec, b"avcodec_parameters_free\0")?,
            av_frame_get_buffer: symbol(avutil, b"av_frame_get_buffer\0")?,
        })
    }
}

pub fn can_encode() -> bool {
    instance()
        .map(|ffmpeg| ffmpeg.encoder_api().is_some())
        .unwrap_or(false)
}

static LOADED: OnceLock<Result<Ffmpeg, String>> = OnceLock::new();

pub fn instance() -> Result<&'static Ffmpeg, &'static str> {
    match LOADED.get_or_init(|| load().map_err(|error| error.to_string())) {
        Ok(ffmpeg) => Ok(ffmpeg),
        Err(detail) => Err(detail.as_str()),
    }
}

pub fn is_available() -> bool {
    instance().is_ok()
}

pub fn can_decode() -> bool {
    DECODE_IMPLEMENTED && is_available()
}

pub fn status() -> String {
    match instance() {
        Ok(ffmpeg) => format!(
            "ffmpeg ready (libavcodec {}, from {})",
            ffmpeg.avcodec_major(),
            ffmpeg.origin()
        ),
        Err(detail) => detail.to_string(),
    }
}

pub(crate) fn null_mut<T>() -> *mut T {
    std::ptr::null_mut()
}

pub(crate) fn null_void() -> *mut c_void {
    std::ptr::null_mut()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_versioned_library_name_yields_its_major() {
        if cfg!(target_os = "windows") {
            assert_eq!(
                version_from_file_name("avcodec", "avcodec-63.dll"),
                Some(63)
            );
            assert_eq!(version_from_file_name("avutil", "avutil-61.dll"), Some(61));
        } else if cfg!(target_os = "macos") {
            assert_eq!(
                version_from_file_name("avcodec", "libavcodec.61.dylib"),
                Some(61)
            );
        } else {
            assert_eq!(
                version_from_file_name("avcodec", "libavcodec.so.61"),
                Some(61)
            );
        }
    }

    #[test]
    fn an_unrelated_file_is_not_mistaken_for_a_library() {
        for name in [
            "readme.txt",
            "avcodec.dll",
            "avcodecx-63.dll",
            "other-63.dll",
        ] {
            assert_eq!(
                version_from_file_name("avcodec", name),
                None,
                "{name} must not parse as a versioned avcodec"
            );
        }
    }

    #[test]
    fn a_non_numeric_version_is_rejected() {
        let name = if cfg!(target_os = "windows") {
            "avcodec-beta.dll"
        } else if cfg!(target_os = "macos") {
            "libavcodec.beta.dylib"
        } else {
            "libavcodec.so.beta"
        };
        assert_eq!(version_from_file_name("avcodec", name), None);
    }

    #[test]
    fn discovery_prefers_the_highest_major_present() {
        let directory = std::env::temp_dir().join("cutix-ffmpeg-discovery");
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("temp dir");

        for major in [58u32, 63, 60] {
            let name = if cfg!(target_os = "windows") {
                format!("avcodec-{major}.dll")
            } else if cfg!(target_os = "macos") {
                format!("libavcodec.{major}.dylib")
            } else {
                format!("libavcodec.so.{major}")
            };
            fs::write(directory.join(name), b"not a real library").expect("write");
        }

        let found = newest_in_directory(&directory, "avcodec").expect("a candidate");
        let name = found.file_name().and_then(|name| name.to_str()).unwrap();
        assert!(name.contains("63"), "expected the newest major, got {name}");

        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn an_empty_directory_yields_no_candidate() {
        let directory = std::env::temp_dir().join("cutix-ffmpeg-empty");
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("temp dir");
        assert!(newest_in_directory(&directory, "avcodec").is_none());
        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_bogus_library_file_fails_to_load_without_panicking() {
        let directory = std::env::temp_dir().join("cutix-ffmpeg-bogus");
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("temp dir");
        let name = if cfg!(target_os = "windows") {
            "avcodec-63.dll"
        } else if cfg!(target_os = "macos") {
            "libavcodec.63.dylib"
        } else {
            "libavcodec.so.63"
        };
        fs::write(directory.join(name), b"definitely not a shared object").expect("write");

        assert!(open_from_directory(&directory, "avcodec").is_err());

        let _ = fs::remove_dir_all(&directory);
    }

    fn library_name(stem: &str, major: u32) -> String {
        if cfg!(target_os = "windows") {
            format!("{stem}-{major}.dll")
        } else if cfg!(target_os = "macos") {
            format!("lib{stem}.{major}.dylib")
        } else {
            format!("lib{stem}.so.{major}")
        }
    }

    #[test]
    fn a_directory_holding_every_library_is_recognised_as_a_bundle() {
        let directory = std::env::temp_dir().join("cutix-ffmpeg-bundle-complete");
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("temp dir");

        for stem in STEMS {
            fs::write(directory.join(library_name(stem, 63)), b"placeholder").expect("write");
        }
        assert!(holds_every_library(&directory));

        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_bundle_missing_one_library_is_not_accepted() {
        let directory = std::env::temp_dir().join("cutix-ffmpeg-bundle-partial");
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("temp dir");

        for stem in STEMS.iter().filter(|stem| **stem != "swscale") {
            fs::write(directory.join(library_name(stem, 63)), b"placeholder").expect("write");
        }
        assert!(
            !holds_every_library(&directory),
            "a bundle without swscale must not be treated as usable"
        );

        assert!(!holds_every_library(&directory.join("absent")));

        let _ = fs::remove_dir_all(&directory);
    }

    #[test]
    fn the_bundle_directory_is_searched_beside_the_executable() {
        let candidates = bundled_directories();
        assert_eq!(
            candidates.len(),
            2,
            "expected the subdirectory and the exe directory"
        );
        assert_eq!(
            candidates[0].file_name().and_then(|name| name.to_str()),
            Some(BUNDLE_SUBDIRECTORY)
        );
        let executable = env::current_exe().expect("current exe");
        assert_eq!(candidates[1], executable.parent().expect("parent"));
    }

    #[test]
    fn status_is_always_a_sentence_never_a_panic() {
        assert!(!status().is_empty());
    }
}
