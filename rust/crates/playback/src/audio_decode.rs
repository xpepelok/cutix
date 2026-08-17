use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{PlaybackError, Result};

#[derive(Clone, Debug)]
pub struct PcmBuffer {
    pub sample_rate: u32,
    pub channels: usize,
    pub samples: Vec<Vec<f32>>,
}

impl PcmBuffer {
    pub fn frame_count(&self) -> usize {
        self.samples.first().map(Vec::len).unwrap_or(0)
    }

    pub fn duration_seconds(&self) -> f64 {
        self.frame_count() as f64 / self.sample_rate.max(1) as f64
    }

    pub fn channel(&self, index: usize) -> &[f32] {
        let index = index.min(self.channels.saturating_sub(1));
        &self.samples[index]
    }
}

#[derive(Default)]
pub struct AudioCache {
    buffers: HashMap<String, (PathBuf, PcmBuffer)>,
}

impl AudioCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load(&mut self, media_id: &str, path: &Path) -> Result<&PcmBuffer> {
        let stale = self
            .buffers
            .get(media_id)
            .map(|(cached, _)| cached != path)
            .unwrap_or(true);
        if stale {
            let buffer = decode_audio(path)?;
            self.buffers
                .insert(media_id.to_owned(), (path.to_path_buf(), buffer));
        }
        Ok(&self.buffers.get(media_id).expect("just inserted").1)
    }

    pub fn forget(&mut self, media_id: &str) {
        self.buffers.remove(media_id);
    }
}

pub fn decode_audio(path: &Path) -> Result<PcmBuffer> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "wav" => decode_wav(path),
        "mp3" => decode_mp3(path),
        other => decode_via_backend(path, other),
    }
}

pub fn decode_audio_window(
    path: &Path,
    start_seconds: f64,
    seconds: f64,
) -> Result<(PcmBuffer, f64)> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "wav" | "mp3" => decode_audio(path).map(|pcm| (pcm, 0.0)),
        other => decode_window_via_backend(path, other, start_seconds, seconds),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn decode_window_via_backend(
    path: &Path,
    extension: &str,
    start_seconds: f64,
    seconds: f64,
) -> Result<(PcmBuffer, f64)> {
    if ensure_backend(extension).is_err() {
        if is_isomp4(extension) {
            return decode_isomp4(path, start_seconds, Some(seconds));
        }
        ensure_backend(extension)?;
    }

    let (buffer, start) = video::ffmpeg::decode_audio_range(path, start_seconds, seconds)
        .map_err(|error| PlaybackError::Decode(format!("{extension}: {error}")))?;

    Ok((
        PcmBuffer {
            sample_rate: buffer.sample_rate,
            channels: buffer.channels.max(1),
            samples: buffer.samples,
        },
        start,
    ))
}

#[cfg(target_arch = "wasm32")]
fn decode_window_via_backend(
    _path: &Path,
    extension: &str,
    _start_seconds: f64,
    _seconds: f64,
) -> Result<(PcmBuffer, f64)> {
    Err(PlaybackError::UnsupportedMedia(format!(
        "no permissive native decoder for '{extension}' audio"
    )))
}

#[cfg(not(target_arch = "wasm32"))]
fn decode_isomp4(
    path: &Path,
    start_seconds: f64,
    seconds: Option<f64>,
) -> Result<(PcmBuffer, f64)> {
    use symphonia::core::audio::{AudioBufferRef, Signal};
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::{FormatOptions, SeekMode, SeekTo};
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;
    use symphonia::core::units::Time;

    let file = std::fs::File::open(path).map_err(|error| PlaybackError::Io(error.to_string()))?;
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|error| PlaybackError::Decode(error.to_string()))?;
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|track| track.codec_params.sample_rate.is_some())
        .ok_or_else(|| PlaybackError::UnsupportedMedia("no audio track".into()))?
        .clone();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|error| PlaybackError::Decode(error.to_string()))?;

    let start = start_seconds.max(0.0);
    let mut window_start = start;
    if start > 0.0 {
        match format.seek(
            SeekMode::Coarse,
            SeekTo::Time {
                time: Time::from(start),
                track_id: Some(track.id),
            },
        ) {
            Ok(sought) => {
                if let Some(rate) = track.codec_params.sample_rate {
                    window_start = sought.actual_ts as f64 / rate.max(1) as f64;
                }
            }
            Err(error) => return Err(PlaybackError::Decode(error.to_string())),
        }
    }

    let mut sample_rate = track.codec_params.sample_rate.unwrap_or(0);
    let mut planes: Vec<Vec<f32>> = Vec::new();
    let limit = seconds.map(|seconds| (seconds.max(0.0) * sample_rate.max(1) as f64) as usize);

    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track.id {
            continue;
        }
        let Ok(decoded) = decoder.decode(&packet) else {
            continue;
        };
        let spec = *decoded.spec();
        sample_rate = spec.rate;
        let channels = spec.channels.count().max(1);
        if planes.len() < channels {
            planes.resize(channels, Vec::new());
        }

        match decoded {
            AudioBufferRef::F32(buffer) => {
                for (index, plane) in planes.iter_mut().enumerate().take(channels) {
                    plane.extend_from_slice(buffer.chan(index));
                }
            }
            other => {
                let mut buffer = other.make_equivalent::<f32>();
                other.convert(&mut buffer);
                for (index, plane) in planes.iter_mut().enumerate().take(channels) {
                    plane.extend_from_slice(buffer.chan(index));
                }
            }
        }

        if limit.is_some_and(|limit| planes.first().is_some_and(|plane| plane.len() >= limit)) {
            break;
        }
    }

    if planes.iter().all(Vec::is_empty) || sample_rate == 0 {
        return Err(PlaybackError::UnsupportedMedia("no decodable audio".into()));
    }

    let channels = planes.len();
    Ok((
        PcmBuffer {
            sample_rate,
            channels,
            samples: planes,
        },
        window_start,
    ))
}

#[cfg(not(target_arch = "wasm32"))]
fn is_isomp4(extension: &str) -> bool {
    matches!(extension, "mp4" | "m4v" | "m4a" | "mov")
}

#[cfg(not(target_arch = "wasm32"))]
fn ensure_backend(extension: &str) -> Result<()> {
    if !video::ffmpeg::can_decode()
        || (!video::opens_audio(extension) && !video::opens_video(extension))
    {
        return Err(PlaybackError::UnsupportedMedia(format!(
            "no permissive native decoder for '{extension}' audio ({})",
            video::ffmpeg::status()
        )));
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn decode_via_backend(path: &Path, extension: &str) -> Result<PcmBuffer> {
    if ensure_backend(extension).is_err() {
        if is_isomp4(extension) {
            return decode_isomp4(path, 0.0, None).map(|(pcm, _)| pcm);
        }
        ensure_backend(extension)?;
    }

    let buffer = video::ffmpeg::decode_audio(path)
        .map_err(|error| PlaybackError::Decode(format!("{extension}: {error}")))?;

    Ok(PcmBuffer {
        sample_rate: buffer.sample_rate,
        channels: buffer.channels.max(1),
        samples: buffer.samples,
    })
}

#[cfg(target_arch = "wasm32")]
fn decode_via_backend(_path: &Path, extension: &str) -> Result<PcmBuffer> {
    Err(PlaybackError::UnsupportedMedia(format!(
        "no permissive native decoder for '{extension}' audio"
    )))
}

fn decode_mp3(path: &Path) -> Result<PcmBuffer> {
    let bytes = std::fs::read(path).map_err(|error| PlaybackError::Io(error.to_string()))?;
    let (header, samples) = puremp3::read_mp3(&bytes[..])
        .map_err(|error| PlaybackError::Decode(format!("mp3: {error:?}")))?;
    let sample_rate = header.sample_rate.hz();
    let mut left = Vec::new();
    let mut right = Vec::new();
    for (l, r) in samples {
        left.push(l);
        right.push(r);
    }
    let channels = if left == right { 1 } else { 2 };
    let samples = if channels == 1 {
        vec![left]
    } else {
        vec![left, right]
    };
    Ok(PcmBuffer {
        sample_rate,
        channels,
        samples,
    })
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

fn decode_wav(path: &Path) -> Result<PcmBuffer> {
    let bytes = std::fs::read(path).map_err(|error| PlaybackError::Io(error.to_string()))?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(PlaybackError::UnsupportedMedia(
            "not a RIFF/WAVE file".into(),
        ));
    }

    let mut offset = 12usize;
    let mut format_tag = 1u16;
    let mut channels = 1usize;
    let mut sample_rate = 44_100u32;
    let mut bits = 16u16;
    let mut data: Option<(usize, usize)> = None;

    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = read_u32(&bytes, offset + 4) as usize;
        let body = offset + 8;
        if id == b"fmt " && body + 16 <= bytes.len() {
            format_tag = read_u16(&bytes, body);
            channels = read_u16(&bytes, body + 2).max(1) as usize;
            sample_rate = read_u32(&bytes, body + 4);
            bits = read_u16(&bytes, body + 14);
        } else if id == b"data" {
            data = Some((body, size.min(bytes.len().saturating_sub(body))));
        }
        offset = body + size + (size & 1);
    }

    let (start, length) = data
        .ok_or_else(|| PlaybackError::UnsupportedMedia("wav file has no data chunk".to_owned()))?;
    let body = &bytes[start..start + length];

    let mut planes = vec![Vec::new(); channels];
    match (format_tag, bits) {
        (1, 16) => {
            for frame in body.chunks_exact(2 * channels) {
                for channel in 0..channels {
                    let value = i16::from_le_bytes([frame[channel * 2], frame[channel * 2 + 1]]);
                    planes[channel].push(value as f32 / 32_768.0);
                }
            }
        }
        (1, 8) => {
            for frame in body.chunks_exact(channels) {
                for channel in 0..channels {
                    planes[channel].push((frame[channel] as f32 - 128.0) / 128.0);
                }
            }
        }
        (3, 32) => {
            for frame in body.chunks_exact(4 * channels) {
                for channel in 0..channels {
                    let value = f32::from_le_bytes([
                        frame[channel * 4],
                        frame[channel * 4 + 1],
                        frame[channel * 4 + 2],
                        frame[channel * 4 + 3],
                    ]);
                    planes[channel].push(value);
                }
            }
        }
        _ => {
            return Err(PlaybackError::UnsupportedMedia(format!(
                "wav format tag {format_tag} with {bits} bits is not supported"
            )));
        }
    }

    Ok(PcmBuffer {
        sample_rate,
        channels,
        samples: planes,
    })
}
