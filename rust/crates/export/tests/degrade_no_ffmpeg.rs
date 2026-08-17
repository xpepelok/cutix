use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use cutix_export::{default_backend, run, AudioSupport, ExportQuality, ExportRequest};
use cutix_playback::{FrameComposer, MediaMap};
use cutix_project::model::TimelineElement;
use cutix_project::Project;
use serde_json::json;
use time::{FrameRate, MediaTime};

fn without_ffmpeg() {
    std::env::set_var(video::ffmpeg::DISABLE_ENV, "1");
}

fn seconds(value: f64) -> MediaTime {
    MediaTime::from_seconds_f64(value).expect("time")
}

fn gradient_png() -> PathBuf {
    let directory = std::env::temp_dir().join("cutix-export-degrade");
    std::fs::create_dir_all(&directory).expect("temp dir");
    let path = directory.join("gradient.png");
    let mut buffer = image::RgbaImage::new(64, 64);
    for y in 0..64u32 {
        for x in 0..64u32 {
            buffer.put_pixel(x, y, image::Rgba([(x * 4) as u8, 32, 200, 255]));
        }
    }
    buffer.save(&path).expect("write png");
    path
}

fn tone_wav(path: &std::path::Path) {
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

fn project_with_audio(width: u32, height: u32, duration: f64) -> Project {
    let mut project = Project::new("degrade-test", "1970-01-01T00:00:00.000Z".into());
    project.settings.canvas_size.width = width;
    project.settings.canvas_size.height = height;
    project.settings.fps = FrameRate::FPS_30;
    let element: TimelineElement = serde_json::from_value(json!({
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
    .expect("element");
    let scene = project.scenes.first_mut().expect("scene");
    scene.tracks.main.elements_mut().push(element);
    scene.tracks.audio.push(
        serde_json::from_value(json!({
            "type": "audio",
            "id": "audio-track",
            "name": "Audio",
            "muted": false,
            "elements": [{
                "type": "audio",
                "id": "tone-element",
                "name": "tone",
                "duration": seconds(duration).as_ticks(),
                "startTime": 0,
                "trimStart": 0,
                "trimEnd": 0,
                "sourceType": "media",
                "mediaId": "tone",
                "volume": 1.0
            }]
        }))
        .expect("track"),
    );
    project
}

#[test]
fn the_encoder_reports_that_it_cannot_reach_ffmpeg() {
    without_ffmpeg();
    assert!(!video::ffmpeg::can_encode());
    assert!(!cutix_export::aac::is_available());
    assert!(
        cutix_export::aac::unavailable_reason().contains("disabled"),
        "the reason must be a sentence, got: {}",
        cutix_export::aac::unavailable_reason()
    );
}

#[test]
fn the_backend_advertises_a_sidecar_before_a_job_starts() {
    without_ffmpeg();
    let factory = default_backend().expect("backend");
    assert_eq!(
        factory.audio_support(),
        AudioSupport::Sidecar { extension: "wav" },
        "the dialog must be able to warn before any frame is rendered"
    );
}

#[test]
fn video_export_still_works_and_audio_lands_beside_it() {
    without_ffmpeg();
    if FrameComposer::new().is_err() {
        eprintln!("no gpu adapter; skipping");
        return;
    }
    let directory = tempfile::tempdir().expect("tempdir");
    let destination = directory.path().join("degraded.mp4");
    let tone = directory.path().join("tone.wav");
    tone_wav(&tone);

    let media = MediaMap::new()
        .with("gradient", gradient_png())
        .with("tone", tone);
    let job = ExportRequest {
        project: Arc::new(project_with_audio(320, 240, 1.0)),
        scene_id: None,
        destination: destination.clone(),
        width: 320,
        height: 240,
        frame_rate: FrameRate::FPS_30,
        quality: ExportQuality::VeryHigh,
        include_audio: true,
        matte_root: None,
        backend: default_backend().expect("backend"),
    };

    let outcome = run(&job, &media, &AtomicBool::new(false), &mut |_| {})
        .expect("video export must not depend on FFmpeg being installed");

    assert_eq!(outcome.artifacts.frames, 30);
    assert_eq!(
        outcome.audio_support,
        AudioSupport::Sidecar { extension: "wav" }
    );

    let info = video::probe(&destination).expect("the mp4 must still decode");
    assert_eq!((info.width, info.height), (320, 240));
    assert_eq!(info.frame_count, 30);
    let frame = video::frame_at(&destination, 0.5).expect("a frame must decode");
    assert_eq!(frame.rgba.len(), frame.width * frame.height * 4);

    let sidecar = outcome
        .artifacts
        .audio_path
        .expect("the mixdown must be written, not dropped");
    assert_eq!(sidecar, destination.with_extension("wav"));
    assert_eq!(outcome.artifacts.audio_samples, 48_000);

    let bytes = std::fs::read(&sidecar).expect("read the sidecar");
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    let channels = u16::from_le_bytes(bytes[22..24].try_into().unwrap());
    let rate = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let data = u32::from_le_bytes(bytes[40..44].try_into().unwrap());
    assert_eq!((channels, rate), (2, 48_000));
    assert_eq!(
        data as u64,
        outcome.artifacts.audio_samples * u64::from(channels) * 2
    );

    let peak = bytes[44..]
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]).unsigned_abs())
        .max()
        .unwrap_or(0);
    eprintln!(
        "sidecar: {} bytes, {rate} Hz, {channels} ch, peak {peak}",
        bytes.len()
    );
    assert!(
        peak > 1000,
        "the sidecar decoded to near silence, peak {peak}"
    );
}
