use std::path::Path;

use crate::error::{ProjectError, Result, io_error};
use crate::model::MediaType;

#[derive(Debug, Clone, PartialEq)]
pub struct ProbeResult {
    pub media_type: MediaType,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub duration: Option<f64>,
    pub fps: Option<f64>,
    pub has_audio: Option<bool>,
}

pub fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tiff"];

pub fn supported_extensions() -> Vec<String> {
    let mut extensions: Vec<String> = ["mp4", "m4v", "mov", "m4a", "mp3", "wav"]
        .iter()
        .chain(IMAGE_EXTENSIONS.iter())
        .map(|extension| (*extension).to_string())
        .collect();

    #[cfg(not(target_arch = "wasm32"))]
    {
        for extension in video::video_extensions()
            .into_iter()
            .chain(video::audio_extensions())
        {
            extensions.push(extension.to_string());
        }
    }

    extensions.sort();
    extensions.dedup();
    extensions
}

pub fn is_supported(path: &Path) -> bool {
    supported_extensions().contains(&extension_of(path))
}

pub fn probe(path: &Path) -> Result<ProbeResult> {
    let extension = extension_of(path);
    match extension.as_str() {
        "mp4" | "m4v" | "mov" | "m4a" => probe_mp4(path),
        "mp3" => probe_mp3(path),
        "wav" => probe_wav(path),
        _ if IMAGE_EXTENSIONS.contains(&extension.as_str()) => probe_image(path),
        other => probe_via_backend(path, other),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn probe_via_backend(path: &Path, extension: &str) -> Result<ProbeResult> {
    if video::opens_video(extension) {
        let info = video::ffmpeg::probe(path).map_err(|error| ProjectError::Probe {
            detail: error.to_string(),
        })?;
        let duration = info.duration_seconds;
        let fps = if duration > 0.0 && info.frame_count > 0 {
            Some(f64::from(info.frame_count) / duration)
        } else {
            None
        };
        return Ok(ProbeResult {
            media_type: MediaType::Video,
            width: Some(u32::from(info.width)),
            height: Some(u32::from(info.height)),
            duration: Some(duration),
            fps,
            has_audio: Some(video::ffmpeg::probe_audio(path).is_ok()),
        });
    }

    if video::opens_audio(extension) {
        let info = video::ffmpeg::probe_audio(path).map_err(|error| ProjectError::Probe {
            detail: error.to_string(),
        })?;
        return Ok(ProbeResult {
            media_type: MediaType::Audio,
            width: None,
            height: None,
            duration: Some(info.duration_seconds),
            fps: None,
            has_audio: Some(true),
        });
    }

    Err(unsupported(extension))
}

#[cfg(target_arch = "wasm32")]
fn probe_via_backend(_path: &Path, extension: &str) -> Result<ProbeResult> {
    Err(unsupported(extension))
}

fn unsupported(extension: &str) -> ProjectError {
    #[cfg(not(target_arch = "wasm32"))]
    let detail = format!(
        "no decode backend opens '{extension}' ({})",
        video::ffmpeg::status()
    );
    #[cfg(target_arch = "wasm32")]
    let detail = format!("unknown extension '{extension}'");

    ProjectError::UnsupportedMedia { detail }
}

fn probe_image(path: &Path) -> Result<ProbeResult> {
    let dimensions = image::image_dimensions(path).map_err(|error| ProjectError::Probe {
        detail: error.to_string(),
    })?;

    Ok(ProbeResult {
        media_type: MediaType::Image,
        width: Some(dimensions.0),
        height: Some(dimensions.1),
        duration: None,
        fps: None,
        has_audio: Some(false),
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn probe_mp4(path: &Path) -> Result<ProbeResult> {
    use mp4::TrackType;

    let file = std::fs::File::open(path).map_err(io_error(path))?;
    let size = file.metadata().map_err(io_error(path))?.len();
    let reader = std::io::BufReader::new(file);
    let mp4 = mp4::Mp4Reader::read_header(reader, size).map_err(|error| ProjectError::Probe {
        detail: error.to_string(),
    })?;

    let duration = mp4.duration().as_secs_f64();
    let has_audio = mp4
        .tracks()
        .values()
        .any(|track| track.track_type().ok() == Some(TrackType::Audio));
    let video = mp4
        .tracks()
        .values()
        .find(|track| track.track_type().ok() == Some(TrackType::Video));

    let Some(video) = video else {
        return Ok(ProbeResult {
            media_type: MediaType::Audio,
            width: None,
            height: None,
            duration: Some(duration),
            fps: None,
            has_audio: Some(has_audio),
        });
    };

    let fps = if duration > 0.0 {
        Some(f64::from(video.sample_count()) / duration)
    } else {
        None
    };

    let (width, height) = video::probe(path)
        .map(|info| (u32::from(info.width), u32::from(info.height)))
        .unwrap_or_else(|_| (u32::from(video.width()), u32::from(video.height())));

    Ok(ProbeResult {
        media_type: MediaType::Video,
        width: Some(width),
        height: Some(height),
        duration: Some(duration),
        fps,
        has_audio: Some(has_audio),
    })
}

#[cfg(target_arch = "wasm32")]
fn probe_mp4(_path: &Path) -> Result<ProbeResult> {
    Err(ProjectError::UnsupportedMedia {
        detail: "container probing is native-only".to_string(),
    })
}

fn probe_wav(path: &Path) -> Result<ProbeResult> {
    let bytes = std::fs::read(path).map_err(io_error(path))?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(ProjectError::Probe {
            detail: "not a RIFF/WAVE file".to_string(),
        });
    }

    let mut offset = 12usize;
    let mut byte_rate = 0u32;
    let mut data_size = 0u32;

    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]);
        let body = offset + 8;

        if id == b"fmt " && body + 16 <= bytes.len() {
            byte_rate = u32::from_le_bytes([
                bytes[body + 8],
                bytes[body + 9],
                bytes[body + 10],
                bytes[body + 11],
            ]);
        } else if id == b"data" {
            data_size = size;
        }

        offset = body + size as usize + (size as usize % 2);
    }

    let duration = if byte_rate > 0 {
        Some(f64::from(data_size) / f64::from(byte_rate))
    } else {
        None
    };

    Ok(ProbeResult {
        media_type: MediaType::Audio,
        width: None,
        height: None,
        duration,
        fps: None,
        has_audio: Some(true),
    })
}

const MPEG1_LAYER3_BITRATES: [u32; 16] = [
    0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
];
const MPEG2_LAYER3_BITRATES: [u32; 16] = [
    0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0,
];
const SAMPLE_RATES: [[u32; 3]; 4] = [
    [11025, 12000, 8000],
    [0, 0, 0],
    [22050, 24000, 16000],
    [44100, 48000, 32000],
];

fn id3v2_length(bytes: &[u8]) -> usize {
    if bytes.len() < 10 || &bytes[0..3] != b"ID3" {
        return 0;
    }
    let size = ((u32::from(bytes[6]) & 0x7f) << 21)
        | ((u32::from(bytes[7]) & 0x7f) << 14)
        | ((u32::from(bytes[8]) & 0x7f) << 7)
        | (u32::from(bytes[9]) & 0x7f);
    10 + size as usize
}

fn probe_mp3(path: &Path) -> Result<ProbeResult> {
    let bytes = std::fs::read(path).map_err(io_error(path))?;
    let mut offset = id3v2_length(&bytes);
    let mut duration = 0.0f64;
    let mut frames = 0u32;

    while offset + 4 <= bytes.len() {
        if bytes[offset] != 0xff || bytes[offset + 1] & 0xe0 != 0xe0 {
            offset += 1;
            continue;
        }

        let version_bits = (bytes[offset + 1] >> 3) & 0x03;
        let layer_bits = (bytes[offset + 1] >> 1) & 0x03;
        let bitrate_index = ((bytes[offset + 2] >> 4) & 0x0f) as usize;
        let sample_rate_index = ((bytes[offset + 2] >> 2) & 0x03) as usize;
        let padding = usize::from((bytes[offset + 2] >> 1) & 0x01);

        if version_bits == 1 || layer_bits == 0 || sample_rate_index == 3 || bitrate_index == 15 {
            offset += 1;
            continue;
        }

        let sample_rate = SAMPLE_RATES[version_bits as usize][sample_rate_index];
        let bitrate = if version_bits == 3 {
            MPEG1_LAYER3_BITRATES[bitrate_index]
        } else {
            MPEG2_LAYER3_BITRATES[bitrate_index]
        } * 1000;

        if sample_rate == 0 || bitrate == 0 {
            offset += 1;
            continue;
        }

        let samples_per_frame: u32 = if version_bits == 3 { 1152 } else { 576 };
        let frame_length = (samples_per_frame / 8 * bitrate / sample_rate) as usize + padding;
        if frame_length == 0 {
            offset += 1;
            continue;
        }

        duration += f64::from(samples_per_frame) / f64::from(sample_rate);
        frames += 1;
        offset += frame_length;
    }

    if frames == 0 {
        return Err(ProjectError::Probe {
            detail: "no MPEG audio frames found".to_string(),
        });
    }

    Ok(ProbeResult {
        media_type: MediaType::Audio,
        width: None,
        height: None,
        duration: Some(duration),
        fps: None,
        has_audio: Some(true),
    })
}
