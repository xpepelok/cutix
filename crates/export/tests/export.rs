use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cutix_export::{
    AudioSupport, ExportError, ExportQuality, ExportRequest, Stage, default_backend, run,
};
use cutix_playback::{ComposeRequest, FrameComposer, MediaMap};
use cutix_project::Project;
use cutix_project::model::TimelineElement;
use serde_json::json;
use time::{FrameRate, MediaTime};

fn ffmpeg_ready() -> bool {
    static READY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *READY.get_or_init(|| {
        if video::ffmpeg::can_encode() {
            return true;
        }
        let Some(directory) = fixtures::fixtures_dir() else {
            return false;
        };
        for entry in std::fs::read_dir(directory.join("tools"))
            .ok()
            .into_iter()
            .flatten()
            .flatten()
        {
            let bin = entry.path().join("bin");
            if bin.join("ffmpeg.exe").is_file() || bin.join("ffmpeg").is_file() {
                // Mutating the environment is unsound while another thread may be reading it.
                // This runs once, before any decoder thread exists.
                unsafe { std::env::set_var(video::ffmpeg::DIR_ENV, &bin) };
                break;
            }
        }
        video::ffmpeg::can_encode()
    })
}

fn seconds(value: f64) -> MediaTime {
    MediaTime::from_seconds_f64(value).expect("time")
}

fn gradient_png() -> PathBuf {
    let directory = std::env::temp_dir().join("cutix-export-tests");
    std::fs::create_dir_all(&directory).expect("temp dir");
    let path = directory.join("gradient.png");
    let mut buffer = image::RgbaImage::new(64, 64);
    for y in 0..64u32 {
        for x in 0..64u32 {
            let quadrant = (x / 32) + (y / 32) * 2;
            let ramp = (x * 4) as u8;
            let pixel = match quadrant {
                0 => [ramp, 32, 200, 255],
                1 => [220, ramp, 40, 255],
                2 => [40, 220, ramp, 255],
                _ => [ramp, ramp, ramp, 255],
            };
            buffer.put_pixel(x, y, image::Rgba(pixel));
        }
    }
    buffer.save(&path).expect("write png");
    path
}

fn image_element(duration: f64) -> TimelineElement {
    serde_json::from_value(json!({
        "type": "image",
        "id": "gradient-element",
        "name": "gradient",
        "duration": seconds(duration).as_ticks(),
        "startTime": 0,
        "trimStart": 0,
        "trimEnd": 0,
        "mediaId": "gradient",
        "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
        "opacity": 1.0
    }))
    .expect("element")
}

fn project(width: u32, height: u32, duration: f64) -> Project {
    let mut project = Project::new("export-test", "1970-01-01T00:00:00.000Z".into());
    project.settings.canvas_size.width = width;
    project.settings.canvas_size.height = height;
    project.settings.fps = FrameRate::FPS_30;
    let scene = project.scenes.first_mut().expect("scene");
    scene
        .tracks
        .main
        .elements_mut()
        .push(image_element(duration));
    project
}

fn request(destination: PathBuf, width: u32, height: u32, document: Project) -> ExportRequest {
    ExportRequest {
        project: Arc::new(document),
        scene_id: None,
        destination,
        width,
        height,
        frame_rate: FrameRate::FPS_30,
        quality: ExportQuality::VeryHigh,
        include_audio: false,
        matte_root: None,
        backend: default_backend().expect("backend"),
    }
}

#[test]
fn an_exported_mp4_decodes_back_to_the_frames_the_composer_produced() {
    let _ = ffmpeg_ready();
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let directory = tempfile::tempdir().expect("tempdir");
    let destination = directory.path().join("export.mp4");
    let media = MediaMap::new().with("gradient", gradient_png());
    let document = project(320, 240, 2.0);
    let job = request(destination.clone(), 320, 240, document.clone());

    let cancel = AtomicBool::new(false);
    let mut stages = Vec::new();
    let outcome = run(&job, &media, &cancel, &mut |progress| {
        if stages.last() != Some(&progress.stage) {
            stages.push(progress.stage);
        }
    })
    .expect("export");

    assert_eq!(outcome.artifacts.frames, 60);
    assert_eq!(stages.first(), Some(&Stage::Rendering));
    assert_eq!(stages.last(), Some(&Stage::Finishing));
    assert!(outcome.artifacts.bytes > 0);
    assert!(!outcome.letterboxed);

    let info = video::probe(&destination).expect("probe");
    assert_eq!(info.width, 320);
    assert_eq!(info.height, 240);
    assert_eq!(info.frame_count, 60);
    assert!(
        (info.duration_seconds - 2.0).abs() < 0.001,
        "duration {}",
        info.duration_seconds
    );

    let mut worst_max = 0u32;
    let mut worst_mean = 0.0f64;
    let mut worst_large = 0.0f64;
    for index in [0u64, 15, 30, 59] {
        let time = MediaTime::from_ticks(
            ((index as f64 / 30.0) * time::TICKS_PER_SECOND as f64).round() as i64,
        );
        let expected = composer
            .compose(
                &ComposeRequest {
                    project: &job.project,
                    scene_id: None,
                    time,
                    width: 320,
                    height: 240,
                },
                &media,
            )
            .expect("compose");
        let decoded = video::frame_at(&destination, index as f64 / 30.0).expect("decode");
        assert_eq!((decoded.width, decoded.height), (320, 240));

        let mut max = 0u32;
        let mut total = 0u64;
        let mut count = 0u64;
        let mut large = 0u64;
        for (actual, wanted) in decoded
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .zip(expected.pixels.as_chunks::<4>().0)
        {
            for channel in 0..3 {
                let delta = (actual[channel] as i32 - wanted[channel] as i32).unsigned_abs();
                max = max.max(delta);
                total += delta as u64;
                count += 1;
                if delta > 16 {
                    large += 1;
                }
            }
        }
        let mean = total as f64 / count as f64;
        let large_share = large as f64 / count as f64;
        eprintln!(
            "frame {index}: max delta {max}, mean delta {mean:.3}, share over 16: {:.4}%",
            large_share * 100.0
        );
        worst_max = worst_max.max(max);
        worst_mean = worst_mean.max(mean);
        worst_large = worst_large.max(large_share);
    }
    eprintln!(
        "worst across sampled frames: max {worst_max}, mean {worst_mean:.3},          share over 16 {:.4}%; {} bytes, {:.1} fps",
        worst_large * 100.0,
        outcome.artifacts.bytes,
        outcome.frames_per_second()
    );
    assert!(
        worst_mean < 3.0,
        "mean delta {worst_mean} is not quantisation noise"
    );

    assert!(
        worst_large < 0.02,
        "share of channels off by more than 16 is {worst_large}, which is too many for chroma          subsampling alone"
    );
    assert!(
        worst_max < 64,
        "max delta {worst_max} suggests a geometry error"
    );
}

#[test]
fn a_vertical_preset_letterboxes_a_wide_project_and_stays_even_sized() {
    let _ = ffmpeg_ready();
    if FrameComposer::new().is_err() {
        eprintln!("no gpu adapter; skipping");
        return;
    }
    let directory = tempfile::tempdir().expect("tempdir");
    let destination = directory.path().join("vertical.mp4");
    let media = MediaMap::new().with("gradient", gradient_png());
    let job = request(destination.clone(), 540, 960, project(1920, 1080, 0.5));

    let cancel = AtomicBool::new(false);
    let outcome = run(&job, &media, &cancel, &mut |_| {}).expect("export");
    assert!(outcome.letterboxed);
    assert_eq!(outcome.artifacts.frames, 15);

    let info = video::probe(&destination).expect("probe");
    assert_eq!((info.width, info.height), (540, 960));

    let frame = video::frame_at(&destination, 0.0).expect("decode");
    let top_row = &frame.rgba[0..4];
    assert!(
        top_row[0] <= 6 && top_row[1] <= 6 && top_row[2] <= 6,
        "the letterbox bar should decode back to black, got {top_row:?}"
    );
    let middle = ((480 * 540) + 270) * 4;
    let centre = &frame.rgba[middle..middle + 3];
    assert!(
        centre.iter().any(|channel| *channel > 32),
        "the picture should sit in the middle, got {centre:?}"
    );
}

#[test]
fn cancelling_stops_the_render_and_reports_it() {
    let _ = ffmpeg_ready();
    if FrameComposer::new().is_err() {
        eprintln!("no gpu adapter; skipping");
        return;
    }
    let directory = tempfile::tempdir().expect("tempdir");
    let destination = directory.path().join("cancelled.mp4");
    let media = MediaMap::new().with("gradient", gradient_png());
    let job = request(destination, 320, 240, project(320, 240, 5.0));

    let cancel = AtomicBool::new(false);
    let error = run(&job, &media, &cancel, &mut |progress| {
        if progress.frame >= 3 {
            cancel.store(true, Ordering::Relaxed);
        }
    })
    .expect_err("cancelled");
    assert!(matches!(error, ExportError::Cancelled));
}

#[test]
fn an_empty_project_is_refused_instead_of_producing_a_zero_frame_file() {
    let _ = ffmpeg_ready();
    let directory = tempfile::tempdir().expect("tempdir");
    let destination = directory.path().join("empty.mp4");
    let document = Project::new("empty", "1970-01-01T00:00:00.000Z".into());
    let job = request(destination.clone(), 320, 240, document);
    let cancel = AtomicBool::new(false);
    let error = run(&job, &MediaMap::new(), &cancel, &mut |_| {}).expect_err("refused");
    assert!(matches!(error, ExportError::Empty));
    assert!(!destination.exists());
}

#[test]
fn the_shipped_backend_muxes_audio_when_ffmpeg_can_encode_it() {
    let ready = ffmpeg_ready();
    let factory = default_backend().expect("backend");
    let expected = if ready {
        AudioSupport::Muxed
    } else {
        AudioSupport::Sidecar { extension: "wav" }
    };
    assert_eq!(factory.audio_support(), expected);
    assert_ne!(
        factory.audio_support(),
        AudioSupport::None,
        "audio must never be silently dropped"
    );
}

#[test]
fn a_project_with_audio_gets_an_audio_track_inside_the_mp4() {
    if !ffmpeg_ready() {
        eprintln!(
            "SKIPPED: no FFmpeg encoder; the sidecar path is covered by degrade_no_ffmpeg.rs"
        );
        return;
    }
    if FrameComposer::new().is_err() {
        eprintln!("no gpu adapter; skipping");
        return;
    }
    let directory = tempfile::tempdir().expect("tempdir");
    let destination = directory.path().join("with-audio.mp4");
    let tone = directory.path().join("tone.wav");
    write_tone(&tone);

    let mut document = project(320, 240, 1.0);
    let scene = document.scenes.first_mut().expect("scene");
    scene.tracks.audio.push(
        serde_json::from_value(json!({
            "type": "audio",
            "id": "audio-track",
            "name": "Audio",
            "elements": [{
                "type": "audio",
                "id": "tone-element",
                "name": "tone",
                "duration": seconds(1.0).as_ticks(),
                "startTime": 0,
                "trimStart": 0,
                "trimEnd": 0,
                "sourceType": "media",
                "mediaId": "tone",
                "volume": 1.0
            }],
            "muted": false
        }))
        .expect("track"),
    );

    let media = MediaMap::new()
        .with("gradient", gradient_png())
        .with("tone", tone);
    let mut job = request(destination.clone(), 320, 240, document);
    job.include_audio = true;

    let cancel = AtomicBool::new(false);
    let outcome = run(&job, &media, &cancel, &mut |_| {}).expect("export");
    assert_eq!(outcome.audio_support, AudioSupport::Muxed);
    assert!(
        outcome.artifacts.audio_path.is_none(),
        "no sidecar is written any more"
    );
    assert!(outcome.artifacts.audio_samples >= 48_000);
    assert!(!directory.path().join("with-audio.wav").exists());

    let info = video::probe(&destination).expect("probe");
    assert_eq!(info.frame_count, 30);

    let decoded = decode_audio(&destination);
    assert_eq!(decoded.sample_rate, 48_000);
    assert_eq!(decoded.channels, 2);
    assert!(
        decoded.peak > 0.05,
        "audio decoded to near silence: peak {}",
        decoded.peak
    );
    assert!(
        decoded.rms > 0.01,
        "audio decoded to near silence: rms {}",
        decoded.rms
    );

    let video_seconds = f64::from(info.frame_count) / 30.0;
    let overhang = decoded.seconds() - video_seconds;
    let padding = 2.0 * f64::from(cutix_export::aac::AAC_FRAME_SAMPLES) / 48_000.0;
    eprintln!(
        "video {:.6}s, audio {:.6}s, overhang {:.6}s ({:.1} ms), aac padding bound {:.6}s",
        video_seconds,
        decoded.seconds(),
        overhang,
        overhang * 1000.0,
        padding
    );
    assert!(
        overhang >= 0.0,
        "the audio track is {overhang:.6}s short of the video"
    );
    assert!(
        overhang <= padding,
        "the audio runs {overhang:.6}s past the video, more than AAC framing alone explains"
    );
    assert_eq!(
        decoded.frames, outcome.artifacts.audio_samples,
        "every sample the muxer claimed must decode back"
    );
}

#[test]
fn the_esds_sl_config_is_corrected_in_place() {
    if !ffmpeg_ready() {
        return;
    }
    if FrameComposer::new().is_err() {
        eprintln!("no gpu adapter; skipping");
        return;
    }
    let directory = tempfile::tempdir().expect("tempdir");
    let destination = directory.path().join("esds.mp4");
    let tone = directory.path().join("tone.wav");
    write_tone(&tone);

    let mut document = project(320, 240, 0.5);
    document
        .scenes
        .first_mut()
        .expect("scene")
        .tracks
        .audio
        .push(
            serde_json::from_value(json!({
                "type": "audio", "id": "t", "name": "Audio", "muted": false,
                "elements": [{
                    "type": "audio", "id": "e", "name": "tone",
                    "duration": seconds(0.5).as_ticks(), "startTime": 0,
                    "trimStart": 0, "trimEnd": 0,
                    "sourceType": "media", "mediaId": "tone", "volume": 1.0
                }]
            }))
            .expect("track"),
        );
    let media = MediaMap::new()
        .with("gradient", gradient_png())
        .with("tone", tone);
    let mut job = request(destination.clone(), 320, 240, document);
    job.include_audio = true;
    run(&job, &media, &AtomicBool::new(false), &mut |_| {}).expect("export");

    let bytes = std::fs::read(&destination).expect("read back");
    let at = bytes
        .windows(4)
        .position(|window| window == b"esds")
        .expect("an esds box");
    let tail = &bytes[at..];
    let sl = tail
        .windows(3)
        .position(|window| window == [0x06, 0x01, 0x02])
        .expect("a corrected SLConfigDescriptor: length 1, predefined 2");
    eprintln!("esds at {at}, corrected SL config {sl} bytes into it");
    assert!(
        !tail[..sl + 3]
            .windows(3)
            .any(|window| window == [0x06, 0x00, 0x00]),
        "the malformed `06 00 00` form must not survive"
    );
}

struct DecodedAudio {
    sample_rate: u32,
    channels: usize,
    frames: u64,
    peak: f32,
    rms: f32,
}

impl DecodedAudio {
    fn seconds(&self) -> f64 {
        self.frames as f64 / self.sample_rate.max(1) as f64
    }
}

fn decode_audio(path: &std::path::Path) -> DecodedAudio {
    use symphonia::core::audio::{AudioBufferRef, Signal};
    use symphonia::core::codecs::DecoderOptions;
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path).expect("open export");
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension("mp4");
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            stream,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .expect("symphonia recognises the container");
    let mut format = probed.format;
    let track = format
        .tracks()
        .iter()
        .find(|track| {
            track.codec_params.channels.is_some() || track.codec_params.sample_rate.is_some()
        })
        .expect("an audio track in the mp4")
        .clone();
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .expect("an aac decoder");

    let mut sample_rate = track.codec_params.sample_rate.unwrap_or(0);
    let mut channels = 0usize;
    let mut frames = 0u64;
    let mut peak = 0f32;
    let mut energy = 0f64;
    let mut samples = 0u64;
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track.id {
            continue;
        }
        let Ok(buffer) = decoder.decode(&packet) else {
            continue;
        };
        let spec = *buffer.spec();
        sample_rate = spec.rate;
        channels = spec.channels.count();
        frames += buffer.frames() as u64;
        if let AudioBufferRef::F32(plane) = buffer {
            for channel in 0..channels {
                for value in plane.chan(channel) {
                    peak = peak.max(value.abs());
                    energy += (*value as f64) * (*value as f64);
                    samples += 1;
                }
            }
        }
    }

    DecodedAudio {
        sample_rate,
        channels,
        frames,
        peak,
        rms: if samples == 0 {
            0.0
        } else {
            (energy / samples as f64).sqrt() as f32
        },
    }
}

fn write_tone(path: &std::path::Path) {
    let sample_rate = 48_000u32;
    let samples: Vec<i16> = (0..sample_rate)
        .map(|index| {
            ((index as f64 / sample_rate as f64 * 440.0 * std::f64::consts::TAU).sin() * 12000.0)
                as i16
        })
        .collect();
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in &samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, bytes).expect("write tone");
}

#[test]
fn flat_colours_survive_an_encode_and_decode_round_trip() {
    let _ = ffmpeg_ready();
    if FrameComposer::new().is_err() {
        eprintln!("no gpu adapter; skipping");
        return;
    }

    for colour in [
        [0u8, 0, 0],
        [255, 255, 255],
        [128, 128, 128],
        [16, 16, 16],
        [200, 40, 40],
        [40, 200, 40],
        [40, 40, 200],
    ] {
        let directory = tempfile::tempdir().expect("tempdir");
        let destination = directory.path().join("flat.mp4");
        let media = MediaMap::new().with("gradient", flat_png(colour));
        let job = request(destination.clone(), 320, 240, project(320, 240, 0.5));

        let cancel = AtomicBool::new(false);
        run(&job, &media, &cancel, &mut |_| {}).expect("export");

        let decoded = video::frame_at(&destination, 0.2).expect("decode");

        let mut worst = 0u32;
        let mut total = 0u64;
        let mut count = 0u64;
        for y in 60..180usize {
            for x in 120..200usize {
                let pixel = (y * 320 + x) * 4;
                for (channel, expected) in colour.iter().enumerate() {
                    let delta =
                        (decoded.rgba[pixel + channel] as i32 - *expected as i32).unsigned_abs();
                    worst = worst.max(delta);
                    total += delta as u64;
                    count += 1;
                }
            }
        }
        let mean = total as f64 / count as f64;
        eprintln!("flat {colour:?}: max delta {worst}, mean delta {mean:.3}");
        assert!(
            worst <= 6,
            "flat {colour:?} came back off by {worst}, which is more than quantisation"
        );
    }
}

fn flat_png(colour: [u8; 3]) -> PathBuf {
    let directory = std::env::temp_dir().join("cutix-export-tests");
    std::fs::create_dir_all(&directory).expect("temp dir");
    let path = directory.join(format!(
        "flat-{}-{}-{}.png",
        colour[0], colour[1], colour[2]
    ));
    let buffer =
        image::RgbaImage::from_pixel(64, 64, image::Rgba([colour[0], colour[1], colour[2], 255]));
    buffer.save(&path).expect("write png");
    path
}

#[test]
fn a_pipelined_export_keeps_frames_in_order() {
    let _ = ffmpeg_ready();
    if FrameComposer::new().is_err() {
        eprintln!("no gpu adapter; skipping");
        return;
    }
    let directory = tempfile::tempdir().expect("tempdir");
    let destination = directory.path().join("ordered.mp4");

    let colours = [[220u8, 40, 40], [40, 220, 40], [40, 40, 220]];
    let mut document = Project::new("order-test", "1970-01-01T00:00:00.000Z".into());
    document.settings.canvas_size.width = 320;
    document.settings.canvas_size.height = 240;
    document.settings.fps = FrameRate::FPS_30;
    let mut media = MediaMap::new();
    for (index, colour) in colours.iter().enumerate() {
        media = media.with(format!("flat{index}"), flat_png(*colour));
        let element = serde_json::from_value(json!({
            "type": "image",
            "id": format!("flat-{index}"),
            "name": format!("flat {index}"),
            "duration": seconds(1.0).as_ticks(),
            "startTime": seconds(index as f64).as_ticks(),
            "trimStart": 0,
            "trimEnd": 0,
            "mediaId": format!("flat{index}"),
            "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
            "opacity": 1.0
        }))
        .expect("element");
        document
            .scenes
            .first_mut()
            .expect("scene")
            .tracks
            .main
            .elements_mut()
            .push(element);
    }

    let job = request(destination.clone(), 320, 240, document);
    let cancel = AtomicBool::new(false);
    let outcome = run(&job, &media, &cancel, &mut |_| {}).expect("export");
    assert_eq!(outcome.artifacts.frames, 90);

    let mut stream = video::VideoStream::open(&destination).expect("open");
    let mut nearest = Vec::new();
    for index in 0..90u32 {
        let frame = stream
            .frame_at(index as f64 / 30.0 + 0.001)
            .expect("decode");
        let pixel = (120 * 320 + 160) * 4;
        let sample = [
            frame.rgba[pixel] as i32,
            frame.rgba[pixel + 1] as i32,
            frame.rgba[pixel + 2] as i32,
        ];
        let (best, _) = colours
            .iter()
            .enumerate()
            .min_by_key(|(_, colour)| {
                (0..3)
                    .map(|c| (sample[c] - colour[c] as i32).abs())
                    .sum::<i32>()
            })
            .expect("nearest colour");
        nearest.push(best);
    }

    let mut changes: Vec<u32> = Vec::new();
    for index in 1..nearest.len() {
        if nearest[index] != nearest[index - 1] {
            changes.push(index as u32);
        }
    }
    eprintln!("colour indices: {nearest:?}");
    assert_eq!(nearest[0], 0, "the clip must open on the first colour");
    assert_eq!(
        changes.len(),
        2,
        "expected exactly two colour changes, got {changes:?}"
    );
    assert!(
        changes[0].abs_diff(30) <= 1,
        "the first change landed at frame {} rather than 30",
        changes[0]
    );
    assert!(
        changes[1].abs_diff(60) <= 1,
        "the second change landed at frame {} rather than 60",
        changes[1]
    );
}

#[test]
fn the_default_backend_prefers_ffmpeg_whenever_it_can_encode_h264() {
    let factory = default_backend().expect("backend");
    let expected = if cutix_export::chosen_encoder().is_some() {
        cutix_export::FFMPEG_MP4_NAME
    } else {
        "openh264-mp4"
    };
    assert_eq!(factory.name(), expected);
}

fn video_element(media_id: &str, trim_start: f64, duration: f64) -> TimelineElement {
    serde_json::from_value(json!({
        "type": "video",
        "id": "trimmed-element",
        "name": "trimmed",
        "duration": seconds(duration).as_ticks(),
        "startTime": 0,
        "trimStart": seconds(trim_start).as_ticks(),
        "trimEnd": 0,
        "mediaId": media_id,
        "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
        "opacity": 1.0
    }))
    .expect("element")
}

fn trim_project(width: u32, height: u32, trim_start: f64, duration: f64) -> Project {
    let mut document = Project::new("trim-test", "1970-01-01T00:00:00.000Z".into());
    document.settings.canvas_size.width = width;
    document.settings.canvas_size.height = height;
    document.settings.fps = FrameRate::FPS_30;
    let scene = document.scenes.first_mut().expect("scene");
    scene
        .tracks
        .main
        .elements_mut()
        .push(video_element("source", trim_start, duration));
    document
}

fn rendered_source(directory: &std::path::Path) -> PathBuf {
    let destination = directory.join("source.mp4");
    let media = MediaMap::new().with("gradient", gradient_png());
    let job = request(destination.clone(), 320, 240, project(320, 240, 3.0));
    let cancel = AtomicBool::new(false);
    run(&job, &media, &cancel, &mut |_| {}).expect("source export");
    destination
}

#[test]
fn a_plain_trim_is_stream_copied_instead_of_re_encoded() {
    if FrameComposer::new().is_err() {
        eprintln!("no gpu adapter; skipping");
        return;
    }
    let directory = tempfile::tempdir().expect("tempdir");
    let source = rendered_source(directory.path());

    let destination = directory.path().join("trimmed.mp4");
    let media = MediaMap::new().with("source", source.clone());
    let job = request(
        destination.clone(),
        320,
        240,
        trim_project(320, 240, 1.0, 1.0),
    );

    let cancel = AtomicBool::new(false);
    let outcome = run(&job, &media, &cancel, &mut |_| {}).expect("trim export");

    assert_eq!(outcome.encoder, cutix_export::STREAM_COPY);
    assert_eq!(outcome.artifacts.frames, 30);
    assert!(outcome.artifacts.bytes > 0);

    let info = video::probe(&destination).expect("probe");
    assert_eq!((info.width, info.height), (320, 240));
    assert_eq!(info.frame_count, 30);

    let copied = video::frame_at(&destination, 0.0).expect("decode copy");
    let original = video::frame_at(&source, 1.0).expect("decode source");
    assert_eq!(copied.rgba, original.rgba);
}

#[test]
fn a_cut_that_misses_a_keyframe_falls_back_to_a_full_render() {
    if FrameComposer::new().is_err() {
        eprintln!("no gpu adapter; skipping");
        return;
    }
    let directory = tempfile::tempdir().expect("tempdir");
    let source = rendered_source(directory.path());

    let destination = directory.path().join("offcut.mp4");
    let media = MediaMap::new().with("source", source);
    let job = request(
        destination.clone(),
        320,
        240,
        trim_project(320, 240, 0.5, 1.0),
    );

    let cancel = AtomicBool::new(false);
    let outcome = run(&job, &media, &cancel, &mut |_| {}).expect("trim export");

    assert_ne!(outcome.encoder, cutix_export::STREAM_COPY);
    assert_eq!(outcome.artifacts.frames, 30);
}

#[test]
fn an_effect_on_the_clip_rules_out_the_copy_path() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut document = trim_project(320, 240, 0.0, 1.0);
    let scene = document.scenes.first_mut().expect("scene");
    if let TimelineElement::Video(video) = &mut scene.tracks.main.elements_mut()[0] {
        video.effects = Some(vec![
            serde_json::from_value(json!({
                "id": "e1",
                "type": "brightness",
                "params": {},
                "enabled": true
            }))
            .expect("effect"),
        ]);
    }
    let media = MediaMap::new().with("source", directory.path().join("source.mp4"));
    let job = request(directory.path().join("out.mp4"), 320, 240, document);
    assert!(cutix_export::remux::plan(&job, &media).is_none());
}

#[test]
fn a_quick_trim_copies_a_span_out_of_a_clip_without_re_encoding() {
    if FrameComposer::new().is_err() {
        eprintln!("no gpu adapter; skipping");
        return;
    }
    let directory = tempfile::tempdir().expect("tempdir");
    let source = rendered_source(directory.path());
    let destination = directory.path().join("quick.mp4");

    let artifacts =
        cutix_export::remux::trim(&source, seconds(1.0), seconds(1.0), false, &destination)
            .expect("quick trim");

    assert!(artifacts.frames > 0);
    let info = video::probe(&destination).expect("probe");
    assert_eq!((info.width, info.height), (320, 240));
    assert!(info.duration_seconds > 0.5);
}

#[test]
fn a_quick_trim_snaps_back_to_a_keyframe_rather_than_refusing() {
    if FrameComposer::new().is_err() {
        eprintln!("no gpu adapter; skipping");
        return;
    }
    let directory = tempfile::tempdir().expect("tempdir");
    let source = rendered_source(directory.path());
    let destination = directory.path().join("offcut.mp4");

    let artifacts =
        cutix_export::remux::trim(&source, seconds(0.5), seconds(1.0), false, &destination)
            .expect("a cut between keyframes must still produce a file");
    assert!(artifacts.frames > 0);
}

#[test]
fn a_quick_trim_refuses_a_container_it_cannot_copy_from() {
    let directory = tempfile::tempdir().expect("tempdir");
    let source = directory.path().join("clip.webm");
    std::fs::write(&source, b"not really a webm").expect("write");
    assert!(
        cutix_export::remux::trim(
            &source,
            seconds(0.0),
            seconds(1.0),
            false,
            &directory.path().join("out.mp4"),
        )
        .is_err()
    );
}

#[test]
fn every_quality_preset_changes_the_size_of_what_openh264_produces() {
    let Ok(_) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let backend = cutix_export::backend_named("openh264-mp4").expect("the shipped backend");
    let directory = tempfile::tempdir().expect("a temporary directory");
    let source = gradient_png();

    let mut sizes = Vec::new();
    for quality in cutix_export::EXPORT_QUALITIES {
        let destination = directory.path().join(format!("{quality:?}.mp4"));
        let mut export = request(destination.clone(), 320, 240, project(320, 240, 1.0));
        export.quality = quality;
        export.backend = backend;
        let media = MediaMap::new().with("gradient", &source);
        let cancel = AtomicBool::new(false);
        let outcome = run(&export, &media, &cancel, &mut |_| {}).expect("the export ran");
        assert_eq!(
            outcome.artifacts.frames, 30,
            "{quality:?} dropped frames; an export must never skip"
        );
        let size = std::fs::metadata(&destination)
            .expect("the file exists")
            .len();
        assert!(size > 0, "{quality:?} produced an empty file");
        sizes.push((quality, size));
    }

    // The quality setting is expressed as a quantiser range, so a higher quality really
    // does keep more detail. If the setting were being ignored — as a declared bitrate is
    // in this encoder — every file would come out the same size.
    for pair in sizes.windows(2) {
        let (lower, small) = pair[0];
        let (higher, large) = pair[1];
        assert!(
            large > small,
            "{higher:?} produced {large} bytes, not more than {lower:?} at {small} bytes"
        );
    }
}

/// Renders a three second clip that carries both a video and an AAC audio track, so a
/// trim across it exercises the stream-copy path with real packet boundaries.
fn rendered_source_with_audio(directory: &std::path::Path) -> PathBuf {
    let destination = directory.join("source-with-audio.mp4");
    let tone = directory.join("tone.wav");
    write_tone(&tone);

    let mut document = project(320, 240, 3.0);
    let scene = document.scenes.first_mut().expect("scene");
    scene.tracks.audio.push(
        serde_json::from_value(json!({
            "type": "audio",
            "id": "audio-track",
            "name": "Audio",
            "elements": [{
                "type": "audio",
                "id": "tone-element",
                "name": "tone",
                "duration": seconds(3.0).as_ticks(),
                "startTime": 0,
                "trimStart": 0,
                "trimEnd": 0,
                "sourceType": "media",
                "mediaId": "tone",
                "volume": 1.0
            }],
            "muted": false
        }))
        .expect("track"),
    );

    let media = MediaMap::new()
        .with("gradient", gradient_png())
        .with("tone", tone);
    let mut job = request(destination.clone(), 320, 240, document);
    job.include_audio = true;
    let cancel = AtomicBool::new(false);
    run(&job, &media, &cancel, &mut |_| {}).expect("source export");
    destination
}

/// Reads the presentation time of the first sample of each track in an MP4, in seconds.
fn first_sample_times(path: &std::path::Path) -> Vec<(mp4::TrackType, f64)> {
    let file = std::fs::File::open(path).expect("open the trimmed file");
    let size = file.metadata().expect("file metadata").len();
    let mut reader =
        mp4::Mp4Reader::read_header(std::io::BufReader::new(file), size).expect("read the mp4");
    let tracks: Vec<(u32, u32, mp4::TrackType)> = reader
        .tracks()
        .iter()
        .filter_map(|(id, track)| Some((*id, track.timescale().max(1), track.track_type().ok()?)))
        .collect();
    let mut times = Vec::new();
    for (id, timescale, kind) in tracks {
        let Ok(Some(sample)) = reader.read_sample(id, 1) else {
            continue;
        };
        times.push((kind, sample.start_time as f64 / f64::from(timescale)));
    }
    times
}

#[test]
fn a_trim_inside_an_aac_packet_does_not_copy_audio_from_before_the_cut() {
    if !ffmpeg_ready() {
        eprintln!("SKIPPED: no FFmpeg encoder, so no AAC source to trim");
        return;
    }
    if FrameComposer::new().is_err() {
        eprintln!("no gpu adapter; skipping");
        return;
    }
    let directory = tempfile::tempdir().expect("tempdir");
    let source = rendered_source_with_audio(directory.path());
    let destination = directory.path().join("trimmed-with-audio.mp4");

    // One AAC packet is 1024 samples, about 21.3 ms at 48 kHz. A cut at 1.01 s lands well
    // inside a packet rather than on its boundary, which is the case that used to copy a
    // packet starting before the cut and then zero its timestamp.
    let artifacts =
        cutix_export::remux::trim(&source, seconds(1.01), seconds(1.0), true, &destination)
            .expect("the trim ran");
    assert!(artifacts.frames > 0);
    assert!(
        artifacts.audio_samples > 0,
        "the audio track was dropped rather than trimmed"
    );

    let packet_seconds = f64::from(cutix_export::aac::AAC_FRAME_SAMPLES) / 48_000.0;
    let times = first_sample_times(&destination);
    let audio_start = times
        .iter()
        .find(|(kind, _)| *kind == mp4::TrackType::Audio)
        .map(|(_, start)| *start)
        .expect("the result has an audio track");
    let video_start = times
        .iter()
        .find(|(kind, _)| *kind == mp4::TrackType::Video)
        .map(|(_, start)| *start)
        .expect("the result has a video track");

    assert_eq!(video_start, 0.0, "video must start at time zero");
    assert!(
        audio_start >= 0.0,
        "audio starts at {audio_start}s, before the start of the file"
    );
    // Audio may begin up to one packet late, because the packet straddling the cut is
    // dropped rather than copied. What it must never do is begin with material from
    // before the cut, which is what a zeroed straddling packet produces.
    assert!(
        audio_start < packet_seconds * 1.5,
        "audio starts {audio_start}s in, more than the one dropped packet explains"
    );

    let decoded = decode_audio(&destination);
    assert!(
        decoded.peak > 0.05,
        "the trimmed audio decoded to near silence: peak {}",
        decoded.peak
    );
    // The copied span must not run past the requested duration by more than the framing.
    assert!(
        decoded.seconds() <= 1.0 + 2.0 * packet_seconds,
        "the trimmed audio is {:.6}s long, past the one second asked for",
        decoded.seconds()
    );
}
