fn disable_ffmpeg() {
    // Mutating the environment is unsound while another thread may be reading it.
    // These tests run single-threaded against a variable only this test touches.
    unsafe { std::env::set_var(video::ffmpeg::DISABLE_ENV, "1") };
}

#[test]
fn the_kill_switch_turns_ffmpeg_off() {
    disable_ffmpeg();
    assert!(!video::ffmpeg::is_available());
    assert!(!video::ffmpeg::can_decode());
    assert!(
        video::ffmpeg::status().contains("disabled"),
        "status should explain itself, got: {}",
        video::ffmpeg::status()
    );
}

#[test]
fn h264_still_decodes_with_ffmpeg_switched_off() {
    disable_ffmpeg();
    let path = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);

    let info = match video::probe(&path) {
        Ok(info) => info,
        Err(error) => panic!("probe must survive without ffmpeg: {error}"),
    };
    assert_eq!(u32::from(info.width), fixtures::SQUARE_CLIP_WIDTH);

    let frame = video::frame_at(&path, 0.0).expect("decode must survive without ffmpeg");
    assert_eq!(frame.rgba.len(), frame.width * frame.height * 4);
}

#[test]
fn the_mp4_family_is_still_advertised_without_ffmpeg() {
    disable_ffmpeg();
    let capabilities = video::capabilities();
    for extension in ["mp4", "m4v", "mov"] {
        assert!(capabilities.opens_video(extension));
    }
}

#[test]
fn webm_is_refused_rather_than_half_opened_without_ffmpeg() {
    disable_ffmpeg();
    assert!(!video::opens_video("webm"));
    assert!(!video::opens_video("mkv"));
    assert!(video::audio_extensions().is_empty());

    let path = fixtures::fixture_or_skip!(fixtures::VP9_CLIP);
    assert!(
        matches!(
            video::frame_at(&path, 0.0),
            Err(video::DecodeError::UnsupportedContainer(_))
        ),
        "a real webm must be refused outright, not opened and then failed"
    );
}

#[test]
fn the_ffmpeg_backend_reports_why_it_cannot_serve() {
    use video::DecodeBackend;
    disable_ffmpeg();
    let error = match video::FfmpegBackend.probe(std::path::Path::new("anything.webm")) {
        Ok(_) => panic!("the ffmpeg backend must not pretend to work"),
        Err(error) => error,
    };
    assert!(matches!(error, video::DecodeError::Backend(_)));
    assert!(error.to_string().contains("disabled"));
}
