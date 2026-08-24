use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use openh264::decoder::Decoder;
use openh264::formats::YUVSource;
use openh264::nal_units;

use crate::color::{self, ColorSpec};

#[derive(Debug)]
pub enum DecodeError {
    Io(String),
    Container(String),
    NoVideoTrack,
    NoFrame,
    Decoder(String),
    UnsupportedContainer(String),
    Backend(String),
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(detail) => write!(formatter, "cannot read file: {detail}"),
            Self::Container(detail) => write!(formatter, "cannot parse container: {detail}"),
            Self::NoVideoTrack => write!(formatter, "file has no video track"),
            Self::NoFrame => write!(formatter, "no frame could be decoded"),
            Self::Decoder(detail) => write!(formatter, "decoder failed: {detail}"),
            Self::UnsupportedContainer(detail) => {
                write!(formatter, "unsupported container: {detail}")
            }
            Self::Backend(detail) => write!(formatter, "decode backend unavailable: {detail}"),
        }
    }
}

impl std::error::Error for DecodeError {}

#[derive(Clone)]
pub struct Frame {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

pub struct VideoInfo {
    pub width: u16,
    pub height: u16,
    pub duration_seconds: f64,
    pub frame_count: u32,
}

fn annex_b(sample: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(sample.len() + 16);
    let mut offset = 0;

    while offset + 4 <= sample.len() {
        let length = u32::from_be_bytes([
            sample[offset],
            sample[offset + 1],
            sample[offset + 2],
            sample[offset + 3],
        ]) as usize;
        offset += 4;

        let end = (offset + length).min(sample.len());
        if offset >= end {
            break;
        }
        output.extend_from_slice(&[0, 0, 0, 1]);
        output.extend_from_slice(&sample[offset..end]);
        offset = end;
    }

    output
}

fn require_h264(track: &mp4::Mp4Track) -> Result<(), DecodeError> {
    match track.media_type() {
        Ok(mp4::MediaType::H264) => Ok(()),
        Ok(other) => Err(DecodeError::UnsupportedContainer(format!(
            "the mp4 video track is {other}, which the openh264 backend does not read"
        ))),
        Err(error) => Err(DecodeError::UnsupportedContainer(format!(
            "unrecognised mp4 video sample entry: {error}"
        ))),
    }
}

pub fn probe(path: impl AsRef<Path>) -> Result<VideoInfo, DecodeError> {
    let file = File::open(path.as_ref()).map_err(|error| DecodeError::Io(error.to_string()))?;
    let size = file
        .metadata()
        .map_err(|error| DecodeError::Io(error.to_string()))?
        .len();
    let reader = BufReader::new(file);
    let mp4 = mp4::Mp4Reader::read_header(reader, size)
        .map_err(|error| DecodeError::Container(error.to_string()))?;

    let track = mp4
        .tracks()
        .values()
        .find(|track| track.track_type().ok() == Some(mp4::TrackType::Video))
        .ok_or(DecodeError::NoVideoTrack)?;
    require_h264(track)?;

    Ok(VideoInfo {
        width: track.width(),
        height: track.height(),
        duration_seconds: mp4.duration().as_secs_f64(),
        frame_count: track.sample_count(),
    })
}

pub fn sps_color(path: impl AsRef<Path>) -> Result<Option<color::SpsColor>, DecodeError> {
    let file = File::open(path.as_ref()).map_err(|error| DecodeError::Io(error.to_string()))?;
    let size = file
        .metadata()
        .map_err(|error| DecodeError::Io(error.to_string()))?
        .len();
    let reader = BufReader::new(file);
    let mp4 = mp4::Mp4Reader::read_header(reader, size)
        .map_err(|error| DecodeError::Container(error.to_string()))?;
    let track = mp4
        .tracks()
        .values()
        .find(|track| track.track_type().ok() == Some(mp4::TrackType::Video))
        .ok_or(DecodeError::NoVideoTrack)?;
    Ok(track
        .sequence_parameter_set()
        .ok()
        .and_then(color::color_spec_from_sps))
}

pub fn first_frame(path: impl AsRef<Path>) -> Result<Frame, DecodeError> {
    frame_at(path, 0.0)
}

fn picture_to_frame(picture: &openh264::decoder::DecodedYUV<'_>, spec: ColorSpec) -> Frame {
    let (width, height) = picture.dimensions();
    let mut rgba = vec![255u8; width * height * 4];
    color::i420_to_rgba(
        picture.y(),
        picture.u(),
        picture.v(),
        (width, height),
        picture.strides(),
        spec,
        &mut rgba,
    );

    Frame {
        width,
        height,
        rgba,
    }
}

fn parameter_sets(track: &mp4::Mp4Track) -> Vec<u8> {
    let mut stream = Vec::new();
    for set in [
        track.sequence_parameter_set().ok(),
        track.picture_parameter_set().ok(),
    ]
    .into_iter()
    .flatten()
    {
        if set.is_empty() {
            continue;
        }
        stream.extend_from_slice(&[0, 0, 0, 1]);
        stream.extend_from_slice(set);
    }
    stream
}

fn color_spec_for(track: &mp4::Mp4Track) -> ColorSpec {
    let height = track.height() as usize;
    let signalled = track
        .sequence_parameter_set()
        .ok()
        .and_then(color::color_spec_from_sps);
    color::resolve(signalled, height)
}

pub fn frame_at(path: impl AsRef<Path>, seconds: f64) -> Result<Frame, DecodeError> {
    frame_at_with_color(path, seconds, None)
}

const FORWARD_SEEK_SAMPLES: u32 = 45;

pub const BASELINE_PROFILE: u8 = 66;

pub fn h264_profile(path: impl AsRef<Path>) -> Option<u8> {
    let file = File::open(path.as_ref()).ok()?;
    let size = file.metadata().ok()?.len();
    let mp4 = mp4::Mp4Reader::read_header(BufReader::new(file), size).ok()?;
    let track = mp4
        .tracks()
        .values()
        .find(|track| track.track_type().ok() == Some(mp4::TrackType::Video))?;
    Some(
        track
            .trak
            .mdia
            .minf
            .stbl
            .stsd
            .avc1
            .as_ref()?
            .avcc
            .avc_profile_indication,
    )
}

pub fn frame_at_with_color(
    path: impl AsRef<Path>,
    seconds: f64,
    override_spec: Option<ColorSpec>,
) -> Result<Frame, DecodeError> {
    let file = File::open(path.as_ref()).map_err(|error| DecodeError::Io(error.to_string()))?;
    let size = file
        .metadata()
        .map_err(|error| DecodeError::Io(error.to_string()))?
        .len();
    let reader = BufReader::new(file);
    let mut mp4 = mp4::Mp4Reader::read_header(reader, size)
        .map_err(|error| DecodeError::Container(error.to_string()))?;

    let track = mp4
        .tracks()
        .values()
        .find(|track| track.track_type().ok() == Some(mp4::TrackType::Video))
        .ok_or(DecodeError::NoVideoTrack)?;
    require_h264(track)?;
    let track_id = track.track_id();

    let spec = mp4
        .tracks()
        .get(&track_id)
        .map(color_spec_for)
        .unwrap_or(ColorSpec::assumed_for_height(0));
    let spec = override_spec.unwrap_or(spec);

    let sample_count = mp4
        .tracks()
        .get(&track_id)
        .map(|track| track.sample_count())
        .unwrap_or(0);

    let timescale = mp4
        .tracks()
        .get(&track_id)
        .map(|track| track.timescale())
        .unwrap_or(1000)
        .max(1) as f64;
    let target_ticks = (seconds.max(0.0) * timescale) as u64;

    let mut target_index = 1u32;
    let mut keyframe_index = 1u32;
    for index in 1..=sample_count {
        let Ok(Some(sample)) = mp4.read_sample(track_id, index) else {
            continue;
        };
        if sample.is_sync && sample.start_time <= target_ticks {
            keyframe_index = index;
        }
        if sample.start_time <= target_ticks {
            target_index = index;
        } else {
            break;
        }
    }

    let headers = mp4
        .tracks()
        .get(&track_id)
        .map(parameter_sets)
        .unwrap_or_default();

    let mut decoder = Decoder::new().map_err(|error| DecodeError::Decoder(error.to_string()))?;
    let mut last: Option<Frame> = None;

    for unit in nal_units(&headers) {
        let _ = decoder.decode(unit);
    }

    for index in keyframe_index..=target_index.max(keyframe_index) {
        let Ok(Some(sample)) = mp4.read_sample(track_id, index) else {
            continue;
        };

        let stream = annex_b(&sample.bytes);
        for unit in nal_units(&stream) {
            if let Ok(Some(picture)) = decoder.decode(unit) {
                last = Some(picture_to_frame(&picture, spec));
            }
        }
    }

    if last.is_none() {
        for index in target_index..=sample_count.min(target_index + 60) {
            let Ok(Some(sample)) = mp4.read_sample(track_id, index) else {
                continue;
            };
            let stream = annex_b(&sample.bytes);
            for unit in nal_units(&stream) {
                if let Ok(Some(picture)) = decoder.decode(unit) {
                    return Ok(picture_to_frame(&picture, spec));
                }
            }
        }
    }

    last.ok_or(DecodeError::NoFrame)
}

#[derive(Clone, Copy, Debug)]
struct SampleEntry {
    start_seconds: f64,
    is_sync: bool,
}

pub struct NativeStream {
    mp4: mp4::Mp4Reader<BufReader<File>>,
    track_id: u32,
    timescale: f64,
    sample_count: u32,
    index: Vec<SampleEntry>,
    indexed_through: u32,
    decoder: Option<Decoder>,
    next_sample: u32,
    fed_through: u32,
    last_frame: Option<Frame>,
    info: VideoInfo,
    spec: ColorSpec,
    override_spec: Option<ColorSpec>,
    headers: Vec<u8>,
    seeks: u64,
    decoded_samples: u64,
    refused_samples: u64,
}

impl NativeStream {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DecodeError> {
        let file = File::open(path.as_ref()).map_err(|error| DecodeError::Io(error.to_string()))?;
        let size = file
            .metadata()
            .map_err(|error| DecodeError::Io(error.to_string()))?
            .len();
        let reader = BufReader::new(file);
        let mp4 = mp4::Mp4Reader::read_header(reader, size)
            .map_err(|error| DecodeError::Container(error.to_string()))?;

        let track = mp4
            .tracks()
            .values()
            .find(|track| track.track_type().ok() == Some(mp4::TrackType::Video))
            .ok_or(DecodeError::NoVideoTrack)?;
        require_h264(track)?;
        let track_id = track.track_id();
        let timescale = track.timescale().max(1) as f64;
        let sample_count = track.sample_count();
        let info = VideoInfo {
            width: track.width(),
            height: track.height(),
            duration_seconds: mp4.duration().as_secs_f64(),
            frame_count: sample_count,
        };
        let spec = color_spec_for(track);
        let headers = parameter_sets(track);

        Ok(Self {
            mp4,
            track_id,
            timescale,
            sample_count,
            index: Vec::new(),
            indexed_through: 0,
            decoder: None,
            next_sample: 1,
            fed_through: 0,
            last_frame: None,
            info,
            spec,
            override_spec: None,
            headers,
            seeks: 0,
            decoded_samples: 0,
            refused_samples: 0,
        })
    }

    pub fn info(&self) -> &VideoInfo {
        &self.info
    }

    pub fn seek_count(&self) -> u64 {
        self.seeks
    }

    pub fn decoded_sample_count(&self) -> u64 {
        self.decoded_samples
    }

    fn extend_index_to(&mut self, sample: u32) {
        let limit = sample.min(self.sample_count);
        while self.indexed_through < limit {
            let index = self.indexed_through + 1;
            let entry = match self.mp4.read_sample(self.track_id, index) {
                Ok(Some(sample)) => SampleEntry {
                    start_seconds: sample.start_time as f64 / self.timescale,
                    is_sync: sample.is_sync,
                },
                _ => SampleEntry {
                    start_seconds: f64::MAX,
                    is_sync: false,
                },
            };
            self.index.push(entry);
            self.indexed_through = index;
        }
    }

    fn index_covering(&mut self, seconds: f64) {
        while self.indexed_through < self.sample_count {
            if let Some(last) = self.index.last()
                && last.start_seconds > seconds
            {
                return;
            }
            self.extend_index_to(self.indexed_through + 64);
        }
    }

    fn sample_for(&mut self, seconds: f64) -> u32 {
        let seconds = seconds.max(0.0);
        self.index_covering(seconds);

        let found = self
            .index
            .partition_point(|entry| entry.start_seconds <= seconds);
        (found.max(1) as u32).min(self.sample_count.max(1))
    }

    pub fn decode_health(&self) -> (u64, u64) {
        (self.decoded_samples, self.refused_samples)
    }

    pub fn last_timestamp(&self) -> f64 {
        let served = self.fed_through.max(1) as usize;
        self.index
            .get(served - 1)
            .map(|entry| entry.start_seconds)
            .unwrap_or(0.0)
    }

    fn sync_sample_at_or_before(&self, sample: u32) -> u32 {
        let mut keyframe = 1u32;
        for offset in 0..sample.min(self.index.len() as u32) {
            if self.index[offset as usize].is_sync {
                keyframe = offset + 1;
            }
        }
        keyframe
    }

    fn reset_to(&mut self, sample: u32) -> Result<(), DecodeError> {
        let mut decoder =
            Decoder::new().map_err(|error| DecodeError::Decoder(error.to_string()))?;
        for unit in nal_units(&self.headers) {
            let _ = decoder.decode(unit);
        }
        self.decoder = Some(decoder);
        self.next_sample = sample;
        self.fed_through = sample.saturating_sub(1);
        self.last_frame = None;
        Ok(())
    }

    fn feed_one(&mut self) -> Result<bool, DecodeError> {
        if self.next_sample > self.sample_count {
            return Ok(false);
        }
        let index = self.next_sample;
        let sample = match self.mp4.read_sample(self.track_id, index) {
            Ok(Some(sample)) => sample,
            Ok(None) => {
                self.next_sample += 1;
                self.fed_through = index;
                return Ok(false);
            }
            Err(error) => {
                self.decoder = None;
                return Err(DecodeError::Container(error.to_string()));
            }
        };
        let stream = annex_b(&sample.bytes);
        let spec = self.override_spec.unwrap_or(self.spec);
        let decoder = self
            .decoder
            .as_mut()
            .ok_or_else(|| DecodeError::Decoder("decoder is not positioned".into()))?;

        let mut decoded = match decoder.decode(&stream) {
            Ok(picture) => picture.map(|picture| picture_to_frame(&picture, spec)),
            Err(_) => None,
        };

        if decoded.is_none() {
            for unit in nal_units(&stream) {
                match decoder.decode(unit) {
                    Ok(Some(picture)) => decoded = Some(picture_to_frame(&picture, spec)),
                    Ok(None) => {}
                    Err(_) => {}
                }
            }
        }

        match decoded {
            Some(frame) => {
                self.last_frame = Some(frame);
                self.decoded_samples += 1;
            }

            None => self.refused_samples += 1,
        }
        self.next_sample += 1;
        self.fed_through = index;
        Ok(true)
    }

    pub fn frame_at(&mut self, seconds: f64) -> Result<Frame, DecodeError> {
        self.frame_at_with_color(seconds, None)
    }

    pub fn frame_at_with_color(
        &mut self,
        seconds: f64,
        override_spec: Option<ColorSpec>,
    ) -> Result<Frame, DecodeError> {
        if self.override_spec != override_spec {
            self.override_spec = override_spec;
            self.decoder = None;
        }
        if self.sample_count == 0 {
            return Err(DecodeError::NoFrame);
        }
        let target = self.sample_for(seconds);

        let far_ahead = target > self.fed_through.saturating_add(FORWARD_SEEK_SAMPLES);
        let jump = far_ahead && self.sync_sample_at_or_before(target) > self.fed_through;

        if self.decoder.is_none() || target < self.fed_through || jump {
            let keyframe = self.sync_sample_at_or_before(target);
            self.seeks += 1;
            self.reset_to(keyframe)?;
        }

        while self.fed_through < target {
            if !self.feed_one()? && self.next_sample > self.sample_count {
                break;
            }
        }

        if self.last_frame.is_none() {
            let limit = (target + 60).min(self.sample_count);
            while self.last_frame.is_none() && self.next_sample <= limit {
                self.feed_one()?;
            }
        }

        self.last_frame.clone().ok_or(DecodeError::NoFrame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annex_b_inserts_start_codes() {
        let sample = [0, 0, 0, 2, 0x65, 0x88, 0, 0, 0, 1, 0x41];
        let converted = annex_b(&sample);
        assert_eq!(&converted[0..4], &[0, 0, 0, 1]);
        assert_eq!(converted[4], 0x65);
        assert_eq!(&converted[6..10], &[0, 0, 0, 1]);
        assert_eq!(converted[10], 0x41);
    }

    #[test]
    fn annex_b_ignores_a_truncated_tail() {
        let sample = [0, 0, 0, 9, 0x65];
        let converted = annex_b(&sample);
        assert_eq!(&converted[0..4], &[0, 0, 0, 1]);
        assert_eq!(converted.len(), 5);
    }

    #[test]
    fn annex_b_of_empty_input_is_empty() {
        assert!(annex_b(&[]).is_empty());
    }

    #[test]
    fn missing_file_reports_io_error() {
        assert!(matches!(
            probe("definitely-not-here.mp4"),
            Err(DecodeError::Io(_))
        ));
    }
}
