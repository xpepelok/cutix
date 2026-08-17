fn point_at_an_empty_directory() {
    let directory = std::env::temp_dir().join("cutix-ffmpeg-absent");
    std::fs::create_dir_all(&directory).expect("temp dir");
    std::env::set_var(video::ffmpeg::DIR_ENV, &directory);
}

#[test]
fn absent_libraries_are_reported_not_fatal() {
    point_at_an_empty_directory();
    assert!(!video::ffmpeg::is_available());
    let status = video::ffmpeg::status();
    assert!(
        status.contains("not found"),
        "status should name the problem, got: {status}"
    );
}

#[test]
fn h264_still_decodes_when_the_libraries_are_missing() {
    point_at_an_empty_directory();
    let path = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);
    let frame = video::backend::frame_at(&path, 0.0).expect("openh264 must carry the load");
    assert_eq!(frame.rgba.len(), frame.width * frame.height * 4);
}

#[test]
fn capabilities_shrink_to_what_openh264_can_serve() {
    point_at_an_empty_directory();
    let extensions = video::video_extensions();
    assert!(extensions.contains(&"mp4"));
    assert!(!extensions.contains(&"webm"));
}
