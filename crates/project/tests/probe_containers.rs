use std::path::PathBuf;

use cutix_project::probe::{extension_of, is_supported, probe, supported_extensions};

fn ffmpeg_ready() -> bool {
    let Some(root) = fixtures::fixtures_dir() else {
        return false;
    };
    let Ok(entries) = std::fs::read_dir(root.join("tools")) else {
        return false;
    };
    for entry in entries.flatten() {
        let bin = entry.path().join("bin");
        if bin.join("ffmpeg.exe").is_file() || bin.join("ffmpeg").is_file() {
            // Mutating the environment is unsound while another thread may be reading it.
            // These tests run single-threaded against a variable only this test touches.
            unsafe { std::env::set_var(video::ffmpeg::DIR_ENV, &bin) };
            return video::ffmpeg::can_decode();
        }
    }
    false
}

#[test]
fn the_mp4_family_is_supported_with_or_without_ffmpeg() {
    for extension in ["mp4", "m4v", "mov", "mp3", "wav", "png"] {
        assert!(
            supported_extensions().contains(&extension.to_string()),
            "{extension} must always be supported"
        );
    }
}

#[test]
fn an_unknown_extension_is_refused_with_a_reason() {
    let error = probe(&PathBuf::from("file.reallynotamediafile")).expect_err("refusal");
    let text = error.to_string();
    assert!(
        text.contains("reallynotamediafile"),
        "the error should name the extension, got: {text}"
    );
}

#[test]
fn the_supported_list_never_advertises_what_no_backend_opens() {
    for extension in supported_extensions() {
        let known_native = matches!(
            extension.as_str(),
            "mp4" | "m4v" | "mov" | "m4a" | "mp3" | "wav"
        ) || matches!(
            extension.as_str(),
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "webp" | "tiff"
        );
        assert!(
            known_native || video::opens_video(&extension) || video::opens_audio(&extension),
            "{extension} is advertised but nothing opens it"
        );
    }
}

#[test]
fn the_new_containers_probe_once_ffmpeg_is_present() {
    if !ffmpeg_ready() {
        eprintln!("SKIPPED: no LGPL FFmpeg under .fixtures/tools");
        return;
    }

    for (name, description) in fixtures::CODEC_CLIPS {
        let Some(path) = fixtures::fixture(name) else {
            continue;
        };
        assert!(is_supported(&path), "{description} should be supported");

        let result = probe(&path).unwrap_or_else(|error| panic!("{description}: {error}"));
        assert_eq!(
            result.media_type,
            cutix_project::MediaType::Video,
            "{description} should probe as video"
        );
        assert!(result.width.unwrap_or(0) > 0, "{description}: no width");
        assert!(result.height.unwrap_or(0) > 0, "{description}: no height");
        assert!(
            result.duration.unwrap_or(0.0) > 0.5,
            "{description}: duration {:?}",
            result.duration
        );
        assert_eq!(
            result.has_audio,
            Some(true),
            "{description} was muxed with an audio track"
        );

        eprintln!(
            "OK {description} ({}): {}x{} {:.2}s",
            extension_of(&path),
            result.width.unwrap(),
            result.height.unwrap(),
            result.duration.unwrap()
        );
    }
}

#[test]
fn a_standalone_audio_file_probes_as_audio() {
    if !ffmpeg_ready() {
        return;
    }
    for name in [fixtures::AAC_CLIP, fixtures::FLAC_CLIP] {
        let Some(path) = fixtures::fixture(name) else {
            continue;
        };
        let result = probe(&path).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(result.media_type, cutix_project::MediaType::Audio);
        assert_eq!(result.has_audio, Some(true));
    }
}
