use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use bytes::Bytes;
use cutix_playback::AudioBuffer;
use mp4::{
    AacConfig, AudioObjectType, AvcConfig, ChannelConfig, MediaConfig, Mp4Config, Mp4Sample,
    Mp4Writer, SampleFreqIndex, TrackConfig, TrackType,
};

use crate::aac::{self};
use crate::backend::{AudioSupport, ExportArtifacts, VideoSpec};
use crate::error::{ExportError, Result};

pub const TIMESCALE: u32 = 90_000;

pub fn audio_sample_duration(sample_rate: u32) -> Option<u32> {
    let numerator = TIMESCALE as u64 * crate::aac::AAC_FRAME_SAMPLES as u64;
    let denominator = sample_rate.max(1) as u64;
    numerator
        .is_multiple_of(denominator)
        .then(|| (numerator / denominator) as u32)
}

pub fn audio_support() -> AudioSupport {
    match crate::capabilities::ExportCapabilities::probe().audio_strategy() {
        crate::capabilities::AudioStrategy::Aac => AudioSupport::Muxed,
        crate::capabilities::AudioStrategy::WavSidecar => {
            AudioSupport::Sidecar { extension: "wav" }
        }
        crate::capabilities::AudioStrategy::StreamCopy => AudioSupport::Muxed,
    }
}

struct PendingSample {
    bytes: Vec<u8>,
    is_sync: bool,
}

pub struct Mp4Sink {
    destination: PathBuf,
    spec: VideoSpec,
    writer: Option<Mp4Writer<BufWriter<File>>>,
    video_timescale: u32,
    track_added: bool,
    frames: u64,
    audio_frames: u64,
    audio_path: Option<PathBuf>,
    audio_samples: u64,
    pending: Vec<PendingSample>,
    parameter_sets: Option<(Vec<u8>, Vec<u8>)>,
}

impl Mp4Sink {
    pub fn create(destination: &Path, spec: VideoSpec) -> Result<Self> {
        if spec.width == 0 || spec.height == 0 || spec.width % 2 == 1 || spec.height % 2 == 1 {
            return Err(ExportError::InvalidSize {
                width: spec.width,
                height: spec.height,
            });
        }
        let rate = spec.frame_rate;
        if !rate.is_valid()
            || u64::from(rate.numerator) > TIMESCALE as u64 * u64::from(rate.denominator)
        {
            return Err(ExportError::InvalidFrameRate);
        }

        let video_timescale = video_timescale(rate);
        let file = File::create(destination).map_err(|error| ExportError::Io {
            path: destination.display().to_string(),
            detail: error.to_string(),
        })?;
        let writer = Mp4Writer::write_start(
            BufWriter::new(file),
            &Mp4Config {
                major_brand: str::parse("isom").unwrap_or_default(),
                minor_version: 512,
                compatible_brands: vec![
                    str::parse("isom").unwrap_or_default(),
                    str::parse("iso2").unwrap_or_default(),
                    str::parse("avc1").unwrap_or_default(),
                    str::parse("mp41").unwrap_or_default(),
                ],
                timescale: video_timescale,
            },
        )
        .map_err(|error| ExportError::Muxer(error.to_string()))?;

        Ok(Self {
            destination: destination.to_path_buf(),
            spec,
            writer: Some(writer),
            video_timescale,
            track_added: false,
            frames: 0,
            audio_frames: 0,
            audio_path: None,
            audio_samples: 0,
            pending: Vec::new(),
            parameter_sets: None,
        })
    }

    pub fn frames(&self) -> u64 {
        self.frames + self.pending.len() as u64
    }

    fn flush_pending(&mut self) -> Result<()> {
        if !self.track_added {
            let Some((sps, pps)) = self.parameter_sets.clone() else {
                return Ok(());
            };
            let writer = self.writer.as_mut().expect("writer");
            writer
                .add_track(&TrackConfig {
                    track_type: TrackType::Video,
                    timescale: self.video_timescale,
                    language: "und".to_owned(),
                    media_conf: MediaConfig::AvcConfig(AvcConfig {
                        width: self.spec.width as u16,
                        height: self.spec.height as u16,
                        seq_param_set: sps,
                        pic_param_set: pps,
                    }),
                })
                .map_err(|error| ExportError::Muxer(error.to_string()))?;
            self.track_added = true;
        }

        let writer = self.writer.as_mut().expect("writer");
        for sample in self.pending.drain(..) {
            let rate = self.spec.frame_rate;
            let start_time = frame_start(rate, self.frames, self.video_timescale);
            let duration = frame_start(rate, self.frames + 1, self.video_timescale) - start_time;
            writer
                .write_sample(
                    1,
                    &Mp4Sample {
                        start_time,
                        duration: duration as u32,
                        rendering_offset: 0,
                        is_sync: sample.is_sync,
                        bytes: Bytes::from(sample.bytes),
                    },
                )
                .map_err(|error| ExportError::Muxer(error.to_string()))?;
            self.frames += 1;
        }
        Ok(())
    }

    pub fn push_annex_b(&mut self, annex_b: &[u8], is_sync: bool) -> Result<()> {
        let (sets, payload) = split_annex_b(annex_b);
        if let Some(sets) = sets
            && self.parameter_sets.is_none()
        {
            self.parameter_sets = Some(sets);
        }
        if payload.is_empty() {
            return Err(ExportError::Encoder(format!(
                "frame {} carried no coded slices",
                self.frames
            )));
        }

        let bytes = match (is_sync, self.parameter_sets.as_ref()) {
            (true, Some((sps, pps))) => {
                let mut prefixed = Vec::with_capacity(payload.len() + sps.len() + pps.len() + 8);
                for set in [sps, pps] {
                    prefixed.extend_from_slice(&(set.len() as u32).to_be_bytes());
                    prefixed.extend_from_slice(set);
                }
                prefixed.extend_from_slice(&payload);
                prefixed
            }
            _ => payload,
        };
        self.pending.push(PendingSample { bytes, is_sync });
        self.flush_pending()
    }

    fn write_sidecar(&mut self, audio: &AudioBuffer) -> Result<()> {
        let path = self.destination.with_extension("wav");
        crate::wav::write_wav(&path, audio)?;
        self.audio_samples = (audio.interleaved.len() / audio.channels.max(1)) as u64;
        self.audio_path = Some(path);
        Ok(())
    }

    pub fn push_audio(&mut self, audio: &AudioBuffer) -> Result<()> {
        if audio.interleaved.is_empty() {
            return Ok(());
        }

        self.flush_pending()?;
        if !self.track_added {
            return Err(ExportError::Empty);
        }
        if self.audio_frames > 0 || self.audio_path.is_some() {
            return Ok(());
        }
        if !aac::is_available() {
            return self.write_sidecar(audio);
        }

        let track = match aac::encode(audio) {
            Ok(track) => track,
            Err(_) => return self.write_sidecar(audio),
        };
        let duration = audio_sample_duration(track.sample_rate).ok_or_else(|| {
            ExportError::Muxer(format!(
                "{} Hz audio does not divide the {TIMESCALE} timescale evenly",
                track.sample_rate
            ))
        })?;
        let freq_index =
            SampleFreqIndex::try_from(aac::sf_index_for_rate(track.sample_rate).unwrap_or(0xff))
                .map_err(|error| ExportError::Muxer(error.to_string()))?;
        let chan_conf = ChannelConfig::try_from(track.channels.min(255) as u8)
            .map_err(|error| ExportError::Muxer(error.to_string()))?;

        let writer = self.writer.as_mut().expect("writer");
        writer
            .add_track(&TrackConfig {
                track_type: TrackType::Audio,
                timescale: TIMESCALE,
                language: "und".to_owned(),
                media_conf: MediaConfig::AacConfig(AacConfig {
                    bitrate: aac::bitrate_for(track.channels as usize),
                    profile: AudioObjectType::AacLowComplexity,
                    freq_index,
                    chan_conf,
                }),
            })
            .map_err(|error| ExportError::Muxer(error.to_string()))?;

        for (index, packet) in track.packets.iter().enumerate() {
            writer
                .write_sample(
                    2,
                    &Mp4Sample {
                        start_time: index as u64 * duration as u64,
                        duration,
                        rendering_offset: 0,
                        is_sync: true,
                        bytes: Bytes::from(packet.clone()),
                    },
                )
                .map_err(|error| ExportError::Muxer(error.to_string()))?;
        }
        self.audio_frames = track.packets.len() as u64;
        self.audio_samples = track.sample_count();
        Ok(())
    }

    pub fn finish(mut self) -> Result<ExportArtifacts> {
        self.flush_pending()?;
        if self.frames == 0 {
            return Err(ExportError::Empty);
        }
        let mut writer = self.writer.take().expect("writer");
        writer
            .write_end()
            .map_err(|error| ExportError::Muxer(error.to_string()))?;
        drop(writer);

        if self.audio_frames > 0 {
            patch_sl_config_predefined(&self.destination)?;
        }

        let bytes = std::fs::metadata(&self.destination)
            .map(|meta| meta.len())
            .unwrap_or(0);
        Ok(ExportArtifacts {
            video_path: Some(self.destination.clone()),
            audio_path: self.audio_path.clone(),
            frames: self.frames,
            audio_samples: self.audio_samples,
            bytes,
        })
    }
}

pub(crate) fn patch_sl_config_predefined(path: &Path) -> Result<bool> {
    use std::io::{Read, Seek, SeekFrom, Write};

    let io = |error: std::io::Error| ExportError::Io {
        path: path.display().to_string(),
        detail: error.to_string(),
    };
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(io)?;
    let length = file.metadata().map_err(io)?.len();

    let mut offset = 0u64;
    let mut moov: Option<(u64, u64)> = None;
    while offset + 8 <= length {
        file.seek(SeekFrom::Start(offset)).map_err(io)?;
        let mut header = [0u8; 8];
        file.read_exact(&mut header).map_err(io)?;
        let mut size = u32::from_be_bytes([header[0], header[1], header[2], header[3]]) as u64;
        let mut body = offset + 8;
        if size == 1 {
            let mut large = [0u8; 8];
            file.read_exact(&mut large).map_err(io)?;
            size = u64::from_be_bytes(large);
            body += 8;
        } else if size == 0 {
            size = length - offset;
        }
        if size < 8 || offset + size > length {
            break;
        }
        if &header[4..8] == b"moov" {
            moov = Some((body, offset + size));
            break;
        }
        offset += size;
    }

    let Some((start, end)) = moov else {
        return Ok(false);
    };
    let span = (end - start) as usize;
    let mut bytes = vec![0u8; span];
    file.seek(SeekFrom::Start(start)).map_err(io)?;
    file.read_exact(&mut bytes).map_err(io)?;

    let mut patched = false;
    for index in 0..bytes.len().saturating_sub(4) {
        if &bytes[index..index + 4] != b"esds" {
            continue;
        }

        let box_size = index
            .checked_sub(4)
            .map(|at| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]))
            .unwrap_or(0) as usize;
        let Some(tail) = (index - 4).checked_add(box_size) else {
            continue;
        };
        if tail > bytes.len() || tail < 3 {
            continue;
        }

        if matches!(
            bytes[tail - 3..tail],
            [0x06, 0x00, 0x00] | [0x06, 0x01, 0x00]
        ) {
            file.seek(SeekFrom::Start(start + tail as u64 - 2))
                .map_err(io)?;
            file.write_all(&[0x01, 0x02]).map_err(io)?;
            patched = true;
        }
    }
    file.flush().map_err(io)?;
    Ok(patched)
}

const MAX_TIMESCALE_MULTIPLE: u64 = 16;

fn video_timescale(rate: time::FrameRate) -> u32 {
    let numerator = u64::from(rate.numerator);
    let per_frame = u64::from(TIMESCALE) * u64::from(rate.denominator);
    let multiple = numerator / gcd(numerator, per_frame).max(1);
    if (1..=MAX_TIMESCALE_MULTIPLE).contains(&multiple) {
        TIMESCALE * multiple as u32
    } else {
        TIMESCALE
    }
}

fn gcd(mut a: u64, mut b: u64) -> u64 {
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn frame_start(rate: time::FrameRate, index: u64, timescale: u32) -> u64 {
    let numerator = u128::from(rate.numerator);
    let scaled = u128::from(index) * u128::from(timescale) * u128::from(rate.denominator);
    ((scaled + numerator / 2) / numerator) as u64
}

type ParameterSets = (Vec<u8>, Vec<u8>);

fn split_annex_b(stream: &[u8]) -> (Option<ParameterSets>, Vec<u8>) {
    let mut sps = None;
    let mut pps = None;
    let mut payload = Vec::with_capacity(stream.len());

    for unit in openh264::nal_units(stream) {
        let start = start_code_length(unit);

        let mut end = unit.len();
        while end > start && unit[end - 1] == 0 {
            end -= 1;
        }
        let body = &unit[start..end];
        if body.is_empty() {
            continue;
        }
        match body[0] & 0x1f {
            7 => sps = Some(body.to_vec()),
            8 => pps = Some(body.to_vec()),
            _ => {
                payload.extend_from_slice(&(body.len() as u32).to_be_bytes());
                payload.extend_from_slice(body);
            }
        }
    }

    let sets = match (sps, pps) {
        (Some(sps), Some(pps)) => Some((sps, pps)),
        _ => None,
    };
    (sets, payload)
}

fn start_code_length(unit: &[u8]) -> usize {
    if unit.starts_with(&[0, 0, 0, 1]) {
        4
    } else if unit.starts_with(&[0, 0, 1]) {
        3
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_sets_are_lifted_out_and_slices_are_length_prefixed() {
        let mut stream = Vec::new();
        stream.extend_from_slice(&[0, 0, 0, 1, 0x67, 0xaa, 0xbb]);
        stream.extend_from_slice(&[0, 0, 0, 1, 0x68, 0xcc]);
        stream.extend_from_slice(&[0, 0, 0, 1, 0x65, 0x01, 0x02, 0x03]);

        let (sets, payload) = split_annex_b(&stream);
        let (sps, pps) = sets.expect("parameter sets");
        assert_eq!(sps, vec![0x67, 0xaa, 0xbb]);
        assert_eq!(pps, vec![0x68, 0xcc]);
        assert_eq!(payload, vec![0, 0, 0, 4, 0x65, 0x01, 0x02, 0x03]);
    }

    #[test]
    fn a_stream_without_parameter_sets_still_yields_its_slices() {
        let stream = [0u8, 0, 0, 1, 0x41, 0x09];
        let (sets, payload) = split_annex_b(&stream);
        assert!(sets.is_none());
        assert_eq!(payload, vec![0, 0, 0, 2, 0x41, 0x09]);
    }

    #[test]
    fn an_odd_output_size_is_rejected_before_any_file_is_created() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("odd.mp4");
        let outcome = Mp4Sink::create(
            &path,
            VideoSpec {
                width: 101,
                height: 100,
                frame_rate: time::FrameRate::FPS_30,
                bitrate_bps: 1_000_000,
                quality: crate::presets::ExportQuality::Medium,
            },
        );
        assert!(matches!(
            outcome.err(),
            Some(ExportError::InvalidSize { .. })
        ));
        assert!(!path.exists());
    }

    #[test]
    fn frame_starts_follow_the_exact_rate_without_drifting() {
        let rate = time::FrameRate::FPS_59_94;
        assert_eq!(
            frame_start(rate, 60_000, TIMESCALE),
            1_001 * u64::from(TIMESCALE)
        );
        let durations: Vec<u64> = (0..4)
            .map(|index| {
                frame_start(rate, index + 1, TIMESCALE) - frame_start(rate, index, TIMESCALE)
            })
            .collect();
        assert_eq!(durations, vec![1_502, 1_501, 1_502, 1_501]);
        assert_eq!(
            frame_start(time::FrameRate::FPS_30, 7, TIMESCALE),
            7 * 3_000
        );
        assert_eq!(
            frame_start(time::FrameRate::FPS_23_976, 24, TIMESCALE),
            90_090
        );
    }

    #[test]
    fn ntsc_rates_get_a_timescale_with_a_constant_frame_duration() {
        for (rate, timescale, duration) in [
            (time::FrameRate::FPS_59_94, 180_000, 3_003),
            (time::FrameRate::FPS_23_976, 360_000, 15_015),
            (time::FrameRate::FPS_30, 90_000, 3_000),
            (
                time::FrameRate {
                    numerator: 30_000,
                    denominator: 1_001,
                },
                90_000,
                3_003,
            ),
        ] {
            assert_eq!(video_timescale(rate), timescale, "{rate:?}");
            for index in [0, 1, 2, 999, 215_999] {
                let span =
                    frame_start(rate, index + 1, timescale) - frame_start(rate, index, timescale);
                assert_eq!(span, duration, "{rate:?} frame {index}");
            }
            assert_eq!(timescale % TIMESCALE, 0);
        }
    }

    #[test]
    fn a_rate_needing_a_huge_timescale_keeps_ninety_kilohertz() {
        let rate = time::FrameRate {
            numerator: 90_001,
            denominator: 1_000,
        };
        assert_eq!(video_timescale(rate), TIMESCALE);
    }

    #[test]
    fn the_audio_sample_duration_divides_the_timescale_or_refuses() {
        assert_eq!(audio_sample_duration(48_000), Some(1_920));
        assert_eq!(audio_sample_duration(44_100), None);
    }
}
