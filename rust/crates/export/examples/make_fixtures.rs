use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use bytes::Bytes;
use cutix_export::fit::even;
use cutix_export::{blit_centre, compute_contain_fit, BackendFactory, VideoSpec};
use mp4::{AvcConfig, MediaConfig, Mp4Config, Mp4Sample, Mp4Writer, TrackConfig, TrackType};
use time::FrameRate;
use video::VideoStream;

fn resize_rgba(
    source: &[u8],
    source_width: u32,
    source_height: u32,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut output = vec![255u8; (width * height * 4) as usize];
    for y in 0..height {
        let source_y = ((y as f64 + 0.5) * source_height as f64 / height as f64 - 0.5).max(0.0);
        let y0 = source_y.floor() as u32;
        let y1 = (y0 + 1).min(source_height - 1);
        let fy = source_y - y0 as f64;
        for x in 0..width {
            let source_x = ((x as f64 + 0.5) * source_width as f64 / width as f64 - 0.5).max(0.0);
            let x0 = source_x.floor() as u32;
            let x1 = (x0 + 1).min(source_width - 1);
            let fx = source_x - x0 as f64;
            let at = |px: u32, py: u32, channel: usize| {
                source[((py * source_width + px) * 4) as usize + channel] as f64
            };
            for channel in 0..3 {
                let top = at(x0, y0, channel) * (1.0 - fx) + at(x1, y0, channel) * fx;
                let bottom = at(x0, y1, channel) * (1.0 - fx) + at(x1, y1, channel) * fx;
                let value = top * (1.0 - fy) + bottom * fy;
                output[((y * width + x) * 4) as usize + channel] =
                    value.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    output
}

fn encode_clip(
    source: &Path,
    destination: &Path,
    width: u32,
    height: u32,
    frame_rate: FrameRate,
    frames: u32,
    start_seconds: f64,
    bitrate_bps: u32,
) {
    let mut stream = VideoStream::open(source).expect("open source");
    let source_width = stream.info().width as u32;
    let source_height = stream.info().height as u32;
    let fps = frame_rate.as_f64().expect("frame rate");

    let fit = compute_contain_fit(
        source_width as f64,
        source_height as f64,
        width as f64,
        height as f64,
    );
    let inner_width = even(fit.width.round().max(2.0) as u32).min(width);
    let inner_height = even(fit.height.round().max(2.0) as u32).min(height);
    let offset_x = (width - inner_width) / 2;
    let offset_y = (height - inner_height) / 2;

    let mut backend = OPENH264_MP4_FACTORY
        .create(
            destination,
            VideoSpec {
                width,
                height,
                frame_rate,
                bitrate_bps,
            },
            None,
        )
        .expect("create backend");

    let mut canvas = vec![0u8; (width * height * 4) as usize];
    for index in 0..frames {
        let seconds = start_seconds + index as f64 / fps;
        let frame = stream.frame_at(seconds).expect("decode source frame");
        let scaled = resize_rgba(
            &frame.rgba,
            frame.width as u32,
            frame.height as u32,
            inner_width,
            inner_height,
        );
        blit_centre(
            &scaled,
            inner_width,
            inner_height,
            &mut canvas,
            width,
            height,
            offset_x,
            offset_y,
        );
        backend.push_frame(&canvas).expect("encode frame");
    }
    let artifacts = backend.finish().expect("finish");
    println!(
        "{} -> {}x{} @ {fps} fps, {} frames, {} bytes",
        destination.display(),
        width,
        height,
        artifacts.frames,
        artifacts.bytes
    );
}

fn strip_in_band_parameter_sets(source: &Path, destination: &Path) {
    let file = File::open(source).expect("open clip");
    let size = file.metadata().expect("metadata").len();
    let mut reader =
        mp4::Mp4Reader::read_header(std::io::BufReader::new(file), size).expect("read header");

    let track = reader
        .tracks()
        .values()
        .find(|track| track.track_type().ok() == Some(TrackType::Video))
        .expect("video track");
    let track_id = track.track_id();
    let timescale = track.timescale();
    let width = track.width();
    let height = track.height();
    let sample_count = track.sample_count();
    let sps = track.sequence_parameter_set().expect("sps").to_vec();
    let pps = track.picture_parameter_set().expect("pps").to_vec();

    let mut writer = Mp4Writer::write_start(
        BufWriter::new(File::create(destination).expect("create")),
        &Mp4Config {
            major_brand: str::parse("isom").unwrap_or_default(),
            minor_version: 512,
            compatible_brands: vec![
                str::parse("isom").unwrap_or_default(),
                str::parse("iso2").unwrap_or_default(),
                str::parse("avc1").unwrap_or_default(),
                str::parse("mp41").unwrap_or_default(),
            ],
            timescale: 1000,
        },
    )
    .expect("write start");
    writer
        .add_track(&TrackConfig {
            track_type: TrackType::Video,
            timescale,
            language: "und".to_owned(),
            media_conf: MediaConfig::AvcConfig(AvcConfig {
                width,
                height,
                seq_param_set: sps,
                pic_param_set: pps,
            }),
        })
        .expect("add track");

    let mut dropped = 0u32;
    for index in 1..=sample_count {
        let Ok(Some(sample)) = reader.read_sample(track_id, index) else {
            continue;
        };
        let mut kept = Vec::with_capacity(sample.bytes.len());
        let mut offset = 0usize;
        while offset + 4 <= sample.bytes.len() {
            let length = u32::from_be_bytes([
                sample.bytes[offset],
                sample.bytes[offset + 1],
                sample.bytes[offset + 2],
                sample.bytes[offset + 3],
            ]) as usize;
            let body = offset + 4;
            let end = (body + length).min(sample.bytes.len());
            if body >= end {
                break;
            }
            let kind = sample.bytes[body] & 0x1f;
            if kind == 7 || kind == 8 {
                dropped += 1;
            } else {
                kept.extend_from_slice(&sample.bytes[offset..end]);
            }
            offset = end;
        }
        writer
            .write_sample(
                1,
                &Mp4Sample {
                    start_time: sample.start_time,
                    duration: sample.duration,
                    rendering_offset: sample.rendering_offset,
                    is_sync: sample.is_sync,
                    bytes: Bytes::from(kept),
                },
            )
            .expect("write sample");
    }
    writer.write_end().expect("write end");
    println!(
        "{} -> parameter sets live only in avcC ({dropped} in-band units removed)",
        destination.display()
    );
}

fn starts_without_reservoir(frame: &[u8]) -> bool {
    if frame.len() < 8 || frame[0] != 0xff || frame[1] & 0xe0 != 0xe0 {
        return false;
    }
    let side_info = if frame[1] & 0x01 == 0 { 6 } else { 4 };
    if frame.len() < side_info + 2 {
        return false;
    }
    let main_data_begin = (u16::from(frame[side_info]) << 1) | u16::from(frame[side_info + 1] >> 7);
    main_data_begin == 0
}

fn extract_mp3(source: &Path, destination: &Path, start_seconds: f64, end_seconds: f64) {
    let file = File::open(source).expect("open source");
    let size = file.metadata().expect("metadata").len();
    let mut reader =
        mp4::Mp4Reader::read_header(std::io::BufReader::new(file), size).expect("read header");

    let track = reader
        .tracks()
        .values()
        .find(|track| track.track_type().ok() == Some(TrackType::Audio))
        .expect("audio track");
    let track_id = track.track_id();
    let timescale = track.timescale().max(1) as f64;
    let sample_count = track.sample_count();

    let mut bytes = Vec::new();
    for index in 1..=sample_count {
        let Ok(Some(sample)) = reader.read_sample(track_id, index) else {
            continue;
        };
        let seconds = sample.start_time as f64 / timescale;
        if seconds < start_seconds {
            continue;
        }
        if seconds >= end_seconds {
            break;
        }
        if bytes.is_empty() && !starts_without_reservoir(&sample.bytes) {
            continue;
        }
        bytes.extend_from_slice(&sample.bytes);
    }
    assert!(!bytes.is_empty(), "no audio samples in the requested range");
    std::fs::write(destination, &bytes).expect("write mp3");
    println!(
        "{} -> {:.1}s..{:.1}s of source audio, {} bytes",
        destination.display(),
        start_seconds,
        end_seconds,
        bytes.len()
    );
}

static OPENH264_MP4_FACTORY: &dyn BackendFactory = &cutix_export::openh264_mp4::OPENH264_MP4;

fn main() {
    let directory = fixtures::fixtures_dir().expect("a .fixtures directory");
    let source: PathBuf = directory.join(fixtures::SOURCE);
    assert!(
        source.is_file(),
        "missing {}; see docs/design/fixtures.md",
        source.display()
    );

    let square = directory.join(fixtures::SQUARE_CLIP);
    encode_clip(
        &source,
        &square,
        fixtures::SQUARE_CLIP_WIDTH,
        fixtures::SQUARE_CLIP_HEIGHT,
        FrameRate::FPS_25,
        fixtures::SQUARE_CLIP_FRAMES,
        24.0,
        2_000_000,
    );

    encode_clip(
        &source,
        &directory.join(fixtures::HD_CLIP),
        fixtures::HD_CLIP_WIDTH,
        fixtures::HD_CLIP_HEIGHT,
        FrameRate::FPS_50,
        fixtures::HD_CLIP_FRAMES,
        24.0,
        12_000_000,
    );

    strip_in_band_parameter_sets(&square, &directory.join(fixtures::AVCC_ONLY_CLIP));

    extract_mp3(
        &source,
        &directory.join(fixtures::AUDIO_CLIP),
        0.0,
        fixtures::AUDIO_CLIP_END,
    );
}
