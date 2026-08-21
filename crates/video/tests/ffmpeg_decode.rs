use std::path::PathBuf;

use video::{BackendKind, Frame};

fn ffmpeg_bin() -> Option<PathBuf> {
    let tools = fixtures::fixtures_dir()?.join("tools");
    for entry in std::fs::read_dir(tools).ok()?.flatten() {
        let bin = entry.path().join("bin");
        if bin.join("ffmpeg.exe").is_file() || bin.join("ffmpeg").is_file() {
            return Some(bin);
        }
    }
    None
}

fn use_ffmpeg() -> bool {
    match ffmpeg_bin() {
        Some(bin) => {
            // Mutating the environment is unsound while another thread may be reading it.
            // These tests run single-threaded against a variable only this test touches.
            unsafe { std::env::set_var(video::ffmpeg::DIR_ENV, &bin) };
            video::ffmpeg::can_decode()
        }
        None => false,
    }
}

macro_rules! ffmpeg_or_skip {
    () => {
        if !use_ffmpeg() {
            eprintln!(
                "SKIPPED {}:{}: no LGPL FFmpeg under .fixtures/tools; see docs/design/ffmpeg.md",
                file!(),
                line!()
            );
            return;
        }
    };
}

fn mean_luma(frame: &Frame) -> f64 {
    let sum: u64 = frame
        .rgba
        .chunks_exact(4)
        .map(|pixel| (u64::from(pixel[0]) + u64::from(pixel[1]) + u64::from(pixel[2])) / 3)
        .sum();
    sum as f64 / (frame.width * frame.height).max(1) as f64
}

fn is_plausible_picture(frame: &Frame) {
    assert!(frame.width > 0 && frame.height > 0);
    assert_eq!(frame.rgba.len(), frame.width * frame.height * 4);

    let luma = mean_luma(frame);
    assert!(
        luma > 2.0 && luma < 253.0,
        "decoded frame looks like a flat plate, mean luma {luma}"
    );

    let distinct = frame
        .rgba
        .chunks_exact(4)
        .step_by(37)
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect::<std::collections::HashSet<_>>()
        .len();
    assert!(distinct > 8, "decoded frame has only {distinct} colours");
}

#[test]
fn the_libraries_load_and_report_a_supported_version() {
    ffmpeg_or_skip!();
    let instance = video::ffmpeg::instance().expect("ffmpeg must load");
    assert!(instance.avcodec_major() >= video::ffmpeg::MINIMUM_AVCODEC_MAJOR);
    eprintln!("STATUS: {}", video::ffmpeg::status());
}

#[test]
fn every_new_container_decodes_a_real_frame() {
    ffmpeg_or_skip!();

    for (name, description) in fixtures::CODEC_CLIPS {
        let Some(path) = fixtures::fixture(name) else {
            eprintln!("SKIPPED {description}: {}", fixtures::missing(name));
            continue;
        };

        let info = video::ffmpeg::probe(&path)
            .unwrap_or_else(|error| panic!("{description}: probe failed: {error}"));
        assert!(
            info.width > 0 && info.height > 0,
            "{description}: probe reported {}x{}",
            info.width,
            info.height
        );

        let frame = video::ffmpeg::frame_at_with_color(&path, 0.5, None)
            .unwrap_or_else(|error| panic!("{description}: decode failed: {error}"));
        is_plausible_picture(&frame);
        assert_eq!(frame.width, usize::from(info.width));
        assert_eq!(frame.height, usize::from(info.height));

        eprintln!(
            "OK {description}: {}x{} mean luma {:.1}",
            frame.width,
            frame.height,
            mean_luma(&frame)
        );
    }
}

#[test]
fn the_new_containers_route_to_the_ffmpeg_backend() {
    ffmpeg_or_skip!();

    for (name, description) in fixtures::CODEC_CLIPS {
        let Some(path) = fixtures::fixture(name) else {
            continue;
        };
        let backend = video::backend_for(&path)
            .unwrap_or_else(|| panic!("{description}: no backend claims {name}"));
        let expected = if name.ends_with(".mp4") {
            BackendKind::OpenH264
        } else {
            BackendKind::Ffmpeg
        };
        assert_eq!(backend.kind(), expected, "{description} routed wrongly");
    }
}

#[test]
fn both_backends_agree_on_the_same_h264_frame() {
    ffmpeg_or_skip!();

    for name in [fixtures::SQUARE_CLIP, fixtures::HD_CLIP] {
        let Some(path) = fixtures::fixture(name) else {
            eprintln!("SKIPPED: {}", fixtures::missing(name));
            continue;
        };

        let native = video::first_frame(&path).expect("openh264 decode");
        let through_ffmpeg =
            video::ffmpeg::frame_at_with_color(&path, 0.0, None).expect("ffmpeg decode");

        assert_eq!(
            (native.width, native.height),
            (through_ffmpeg.width, through_ffmpeg.height),
            "{name}: backends disagree on dimensions"
        );

        let differing = native
            .rgba
            .iter()
            .zip(&through_ffmpeg.rgba)
            .filter(|(left, right)| left != right)
            .count();
        let worst = native
            .rgba
            .iter()
            .zip(&through_ffmpeg.rgba)
            .map(|(left, right)| left.abs_diff(*right))
            .max()
            .unwrap_or(0);

        eprintln!(
            "{name}: {differing} of {} channel samples differ, worst delta {worst}",
            native.rgba.len()
        );
        assert_eq!(
            worst, 0,
            "{name}: the two backends must produce identical RGBA, worst delta {worst}"
        );
    }
}

#[test]
fn both_backends_agree_under_an_explicit_colour_override() {
    ffmpeg_or_skip!();

    let Some(path) = fixtures::fixture(fixtures::SQUARE_CLIP) else {
        return;
    };

    for spec in [
        video::ColorSpec {
            full_range: false,
            matrix: video::Matrix::Bt709,
        },
        video::ColorSpec {
            full_range: true,
            matrix: video::Matrix::Bt601,
        },
    ] {
        let native = video::frame_at_with_color(&path, 0.0, Some(spec)).expect("openh264");
        let through_ffmpeg =
            video::ffmpeg::frame_at_with_color(&path, 0.0, Some(spec)).expect("ffmpeg");
        assert_eq!(
            native.rgba, through_ffmpeg.rgba,
            "colour override {spec:?} must survive both backends identically"
        );
    }
}

#[test]
fn seeking_forward_lands_on_a_later_frame() {
    ffmpeg_or_skip!();

    let Some(path) = fixtures::fixture(fixtures::VP9_CLIP) else {
        return;
    };

    let mut stream = video::ffmpeg::FfmpegStream::open(&path).expect("open vp9");
    let early = stream.frame_at(0.0).expect("first frame");
    let later = stream.frame_at(1.5).expect("later frame");

    assert_eq!((early.width, early.height), (later.width, later.height));
    assert_ne!(
        early.rgba, later.rgba,
        "seeking 1.5s into the clip should not return the same picture"
    );
}

#[test]
fn sequential_playback_does_not_reseek_on_the_ffmpeg_backend() {
    ffmpeg_or_skip!();

    for (name, description) in [
        (fixtures::VP9_CLIP, "vp9 in webm"),
        (fixtures::HEVC_MKV_CLIP, "hevc in matroska"),
        (fixtures::HEVC_MP4_CLIP, "hevc in mp4"),
    ] {
        let Some(path) = fixtures::fixture(name) else {
            eprintln!("SKIPPED {description}: {}", fixtures::missing(name));
            continue;
        };

        let mut stream = video::VideoStream::open(&path).expect("open");
        assert_eq!(
            stream.backend(),
            BackendKind::Ffmpeg,
            "{description} must be served by ffmpeg"
        );

        let frames = 40;
        for index in 0..frames {
            let frame = stream
                .frame_at(index as f64 / 24.0)
                .unwrap_or_else(|error| panic!("{description} frame {index}: {error}"));
            assert!(frame.width > 0 && frame.height > 0);
        }
        eprintln!(
            "{description}: {frames} sequential frames, {} seeks, {} decoded",
            stream.seek_count(),
            stream.decoded_sample_count()
        );
        assert_eq!(
            stream.seek_count(),
            1,
            "{description}: only the initial positioning may seek"
        );
        assert!(
            stream.decoded_sample_count() >= frames,
            "{description}: fewer frames were decoded than were asked for"
        );
    }
}

#[test]
fn backwards_seek_reseeks_on_the_ffmpeg_backend() {
    ffmpeg_or_skip!();

    let Some(path) = fixtures::fixture(fixtures::VP9_CLIP) else {
        return;
    };
    let mut stream = video::VideoStream::open(&path).expect("open");
    assert_eq!(stream.backend(), BackendKind::Ffmpeg);

    for index in 0..30 {
        stream.frame_at(index as f64 / 24.0).expect("forward");
    }
    assert_eq!(stream.seek_count(), 1);

    let early = stream.frame_at(0.0).expect("jump back to the start");
    assert_eq!(
        stream.seek_count(),
        2,
        "a backwards request must reposition the demuxer"
    );

    stream.frame_at(1.0 / 24.0).expect("forward again");
    assert_eq!(stream.seek_count(), 2, "forward again must not seek");

    let late = stream.frame_at(1.5).expect("forward a long way");
    assert_eq!(
        stream.seek_count(),
        2,
        "a forward jump must not seek either"
    );
    assert_ne!(
        early.rgba, late.rgba,
        "the stream must actually have moved between the two reads"
    );
}

#[test]
fn repeating_the_same_timestamp_decodes_nothing_new() {
    ffmpeg_or_skip!();

    let Some(path) = fixtures::fixture(fixtures::VP9_CLIP) else {
        return;
    };
    let mut stream = video::VideoStream::open(&path).expect("open");
    let first = stream.frame_at(0.5).expect("first read");
    let decoded = stream.decoded_sample_count();
    let again = stream.frame_at(0.5).expect("same timestamp again");

    assert_eq!(stream.seek_count(), 1);
    assert_eq!(
        stream.decoded_sample_count(),
        decoded,
        "asking for the same instant twice must be served from the held frame"
    );
    assert_eq!(first.rgba, again.rgba);
}

#[test]
fn the_unified_stream_picks_the_decoder_the_profile_calls_for() {
    ffmpeg_or_skip!();
    let Some(path) = fixtures::fixture(fixtures::SQUARE_CLIP) else {
        return;
    };
    let profile = video::decode::h264_profile(&path).expect("an avc profile");
    let expected = if profile == video::decode::BASELINE_PROFILE {
        BackendKind::OpenH264
    } else {
        BackendKind::Ffmpeg
    };
    let stream = video::VideoStream::open(&path).expect("open");
    assert_eq!(
        stream.backend(),
        expected,
        "a file of profile {profile} went to the wrong decoder"
    );
}

#[test]
fn the_unified_stream_matches_the_one_shot_decode() {
    ffmpeg_or_skip!();
    let Some(path) = fixtures::fixture(fixtures::VP9_CLIP) else {
        return;
    };
    let mut stream = video::VideoStream::open(&path).expect("open");
    for seconds in [0.0, 0.5, 1.0] {
        let streamed = stream.frame_at(seconds).expect("streamed");
        let one_shot = video::frame_at(&path, seconds).expect("one shot");
        assert_eq!(
            (streamed.width, streamed.height),
            (one_shot.width, one_shot.height)
        );
        assert_eq!(
            streamed.rgba, one_shot.rgba,
            "the positioned stream and the one-shot path must agree at {seconds}s"
        );
    }
}

#[test]
fn a_corrupt_file_is_an_error_not_a_crash() {
    ffmpeg_or_skip!();

    let path = std::env::temp_dir().join("cutix-not-a-video.webm");
    std::fs::write(&path, b"this is definitely not a matroska file").expect("write");

    assert!(video::ffmpeg::probe(&path).is_err());
    assert!(video::ffmpeg::frame_at_with_color(&path, 0.0, None).is_err());

    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_missing_file_is_an_error_not_a_crash() {
    ffmpeg_or_skip!();
    assert!(video::ffmpeg::probe(std::path::Path::new("nowhere.webm")).is_err());
}

#[test]
fn capabilities_open_up_once_ffmpeg_is_present() {
    ffmpeg_or_skip!();

    let capabilities = video::capabilities();
    for extension in ["webm", "mkv", "avi"] {
        assert!(
            capabilities.opens_video(extension),
            "{extension} should be served once ffmpeg is loaded"
        );
    }
    assert!(capabilities.opens_video("mp4"));
    assert!(capabilities.opens_audio("aac"));
}

#[test]
fn aac_and_flac_decode_to_planar_f32() {
    ffmpeg_or_skip!();

    for (name, description) in [
        (fixtures::AAC_CLIP, "aac in m4a"),
        (fixtures::FLAC_CLIP, "flac"),
    ] {
        let Some(path) = fixtures::fixture(name) else {
            eprintln!("SKIPPED {description}: {}", fixtures::missing(name));
            continue;
        };

        let buffer = video::ffmpeg::decode_audio(&path)
            .unwrap_or_else(|error| panic!("{description}: {error}"));

        assert!(
            buffer.sample_rate >= 8_000,
            "{description}: odd sample rate"
        );
        assert!(
            buffer.channels >= 1 && buffer.channels <= 8,
            "{description}: odd channel count {}",
            buffer.channels
        );
        assert_eq!(buffer.samples.len(), buffer.channels);
        assert!(
            buffer.duration_seconds() > 1.0,
            "{description}: only {:.3}s decoded",
            buffer.duration_seconds()
        );

        let peak = buffer.samples[0]
            .iter()
            .fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        assert!(
            peak > 0.001 && peak <= 1.5,
            "{description}: peak {peak} does not look like normalised audio"
        );

        eprintln!(
            "OK {description}: {} Hz, {} ch, {:.2}s, peak {peak:.3}",
            buffer.sample_rate,
            buffer.channels,
            buffer.duration_seconds()
        );
    }
}

#[test]
fn audio_from_a_video_container_is_reachable() {
    ffmpeg_or_skip!();
    let Some(path) = fixtures::fixture(fixtures::VP9_CLIP) else {
        return;
    };
    let buffer = video::ffmpeg::decode_audio(&path).expect("opus inside webm");
    assert!(buffer.duration_seconds() > 1.0);
}

#[test]
fn a_video_only_file_reports_no_audio_track() {
    ffmpeg_or_skip!();
    let Some(path) = fixtures::fixture(fixtures::SQUARE_CLIP) else {
        return;
    };
    assert!(video::ffmpeg::decode_audio(&path).is_err());
}

#[test]
fn hevc_in_mp4_falls_back_from_openh264_to_ffmpeg() {
    ffmpeg_or_skip!();

    let Some(path) = fixtures::fixture(fixtures::HEVC_MP4_CLIP) else {
        return;
    };

    assert!(
        video::first_frame(&path).is_err(),
        "openh264 alone must not claim to decode HEVC"
    );

    let frame =
        video::frame_at(&path, 0.5).expect("the seam must fall back to ffmpeg for HEVC in mp4");
    is_plausible_picture(&frame);

    let candidates = video::backends_for(&path);
    assert_eq!(candidates.len(), 2, "mp4 should offer both backends");
    assert_eq!(candidates[0].kind(), BackendKind::OpenH264);
    assert_eq!(candidates[1].kind(), BackendKind::Ffmpeg);
}

#[test]
fn hevc_in_mp4_probes_to_its_coded_size_not_the_tkhd_display_size() {
    ffmpeg_or_skip!();

    let Some(path) = fixtures::fixture(fixtures::HEVC_MP4_CLIP) else {
        return;
    };

    let through_mp4 = video::decode::probe(&path);
    assert!(
        matches!(
            through_mp4,
            Err(video::DecodeError::UnsupportedContainer(_))
        ),
        "the mp4-crate reader must decline HEVC rather than report tkhd dimensions"
    );

    let info = video::probe(&path).expect("the seam must fall through to ffmpeg");
    let frame = video::frame_at(&path, 0.5).expect("decode");
    eprintln!(
        "hevc in mp4: probe {}x{}, decode {}x{}",
        info.width, info.height, frame.width, frame.height
    );
    assert_eq!((info.width, info.height), (1280, 528));
    assert_eq!(
        (usize::from(info.width), usize::from(info.height)),
        (frame.width, frame.height),
        "probe and decode must agree"
    );
}

#[test]
fn every_container_probes_to_the_size_it_decodes_to() {
    ffmpeg_or_skip!();

    for (name, description) in fixtures::CODEC_CLIPS {
        let Some(path) = fixtures::fixture(name) else {
            continue;
        };
        let info = video::probe(&path).unwrap_or_else(|error| panic!("{description}: {error}"));
        let frame =
            video::frame_at(&path, 0.5).unwrap_or_else(|error| panic!("{description}: {error}"));
        assert_eq!(
            (usize::from(info.width), usize::from(info.height)),
            (frame.width, frame.height),
            "{description}: probe says {}x{} but decode gives {}x{}",
            info.width,
            info.height,
            frame.width,
            frame.height
        );
        eprintln!("OK {description}: {}x{}", info.width, info.height);
    }
}

#[test]
fn h264_in_mp4_is_routed_by_its_profile() {
    ffmpeg_or_skip!();
    let Some(path) = fixtures::fixture(fixtures::SQUARE_CLIP) else {
        return;
    };
    let profile = video::decode::h264_profile(&path).expect("an avc profile");
    let expected = if profile == video::decode::BASELINE_PROFILE {
        BackendKind::OpenH264
    } else {
        BackendKind::Ffmpeg
    };
    let candidates = video::backends_for(&path);
    assert_eq!(candidates[0].kind(), expected, "profile {profile}");
    assert!(candidates[0].frame_at(&path, 0.0, None).is_ok());
}

#[test]
fn ten_bit_and_hdr_signalling_is_detected_and_reported() {
    ffmpeg_or_skip!();

    let Some(path) = fixtures::fixture(fixtures::HDR10_CLIP) else {
        eprintln!("SKIPPED: {}", fixtures::missing(fixtures::HDR10_CLIP));
        return;
    };

    let range = video::dynamic_range(&path).expect("a dynamic range for a file we can open");
    eprintln!("{}: {}", fixtures::HDR10_CLIP, range.describe());

    assert_eq!(
        range.bit_depth, 10,
        "the 10-bit source must be reported as 10-bit"
    );
    assert!(range.is_high_bit_depth());
    assert!(
        range.wide_gamut,
        "BT.2020 signalling must be carried through"
    );
    assert!(
        range.is_reduced_by_decoding(),
        "the pipeline truncates this to 8-bit, so it must say so"
    );
    assert!(
        range.describe().contains("truncated to 8-bit"),
        "the description must name the loss, got: {}",
        range.describe()
    );

    let stream = video::VideoStream::open(&path).expect("open");
    assert_eq!(
        stream.dynamic_range(),
        range,
        "the stream must agree with the probe"
    );
}

#[test]
fn an_eight_bit_sdr_clip_is_not_flagged_as_reduced() {
    ffmpeg_or_skip!();

    for name in [fixtures::VP9_CLIP, fixtures::HEVC_MP4_CLIP] {
        let Some(path) = fixtures::fixture(name) else {
            continue;
        };
        let range = video::dynamic_range(&path).expect("a dynamic range");
        eprintln!("{name}: {}", range.describe());
        assert_eq!(range.bit_depth, 8, "{name} is an 8-bit source");
        assert!(!range.is_hdr(), "{name} must not be called HDR");
        assert!(
            !range.is_reduced_by_decoding(),
            "{name} loses nothing and must not claim it does"
        );
    }
}

#[test]
fn a_file_only_openh264_serves_answers_without_pretending_to_measure() {
    let Some(path) = fixtures::fixture(fixtures::SQUARE_CLIP) else {
        return;
    };
    let _stream = video::decode::NativeStream::open(&path).expect("open");

    let range = video::ffmpeg::DynamicRange::SDR_8_BIT;
    assert!(!range.is_reduced_by_decoding());
    assert_eq!(range.bit_depth, 8);
}

#[test]
fn a_mid_stream_resolution_change_is_decoded_without_reusing_a_stale_scaler() {
    ffmpeg_or_skip!();

    let Some(path) = fixtures::fixture(fixtures::RESOLUTION_CHANGE_CLIP) else {
        return;
    };

    let mut stream = video::ffmpeg::FfmpegStream::open(&path).expect("open the spliced stream");
    let mut seen: Vec<(usize, usize)> = Vec::new();

    for step in 0..20 {
        let seconds = fixtures::RESOLUTION_CHANGE_START + f64::from(step) * 0.1;
        let Ok(frame) = stream.frame_at(seconds) else {
            continue;
        };
        assert_eq!(
            frame.rgba.len(),
            frame.width * frame.height * 4,
            "frame at {seconds:.1}s reports {}x{} but carries {} bytes",
            frame.width,
            frame.height,
            frame.rgba.len()
        );
        let bottom = &frame.rgba[frame.rgba.len() - frame.width * 4..];
        assert!(
            bottom.chunks_exact(4).any(|pixel| pixel[0..3] != [0, 0, 0]),
            "the last row at {seconds:.1}s ({}x{}) is entirely black, which is what a scaler \
             built for the previous resolution leaves behind",
            frame.width,
            frame.height
        );
        let size = (frame.width, frame.height);
        if seen.last() != Some(&size) {
            seen.push(size);
        }
    }

    eprintln!("resolution change: decoded sizes {seen:?}");
    let first = (
        fixtures::RESOLUTION_CHANGE_FIRST.0 as usize,
        fixtures::RESOLUTION_CHANGE_FIRST.1 as usize,
    );
    let second = (
        fixtures::RESOLUTION_CHANGE_SECOND.0 as usize,
        fixtures::RESOLUTION_CHANGE_SECOND.1 as usize,
    );
    assert!(
        seen.contains(&first),
        "the first resolution never appeared: {seen:?}"
    );
    assert!(
        seen.contains(&second),
        "the second resolution never appeared: {seen:?}"
    );
}

#[test]
fn skipping_ahead_lands_on_the_same_frame_as_walking_there() {
    ffmpeg_or_skip!();
    let Some(path) = fixtures::fixture(fixtures::VP9_CLIP) else {
        return;
    };

    let target = 1.0;

    let mut walked = video::ffmpeg::FfmpegStream::open(&path).expect("open");
    let mut step = 0.0;
    while step < target {
        let _ = walked.frame_at(step);
        step += 1.0 / 60.0;
    }
    let walked_frame = walked.frame_at(target).expect("walked");

    let mut jumped = video::ffmpeg::FfmpegStream::open(&path).expect("open");
    let _ = jumped.frame_at(0.0).expect("first");
    let jumped_frame = jumped.frame_at(target).expect("jumped");

    assert_eq!(
        (walked_frame.width, walked_frame.height),
        (jumped_frame.width, jumped_frame.height)
    );
    assert_eq!(
        walked_frame.rgba, jumped_frame.rgba,
        "a jump forward must not change which frame is chosen, only how much work it costs"
    );
    assert!(
        jumped.decoded_sample_count() > 1,
        "the jump has to walk the stream, otherwise the test proves nothing"
    );
}
