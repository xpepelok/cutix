use video::{first_frame, probe, VideoStream};

fn mean_luma(frame: &video::Frame) -> f64 {
    let sum: u64 = frame
        .rgba
        .chunks_exact(4)
        .map(|pixel| (pixel[0] as u64 + pixel[1] as u64 + pixel[2] as u64) / 3)
        .sum();
    sum as f64 / (frame.width * frame.height).max(1) as f64
}

#[test]
fn a_clip_whose_parameter_sets_live_only_in_avcc_still_decodes() {
    let stripped = fixtures::fixture_or_skip!(fixtures::AVCC_ONLY_CLIP);
    let reference = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);

    let info = probe(&stripped).expect("probe");
    assert_eq!(info.width as u32, fixtures::SQUARE_CLIP_WIDTH);
    assert_eq!(info.frame_count, fixtures::SQUARE_CLIP_FRAMES);

    let stripped_frame = first_frame(&stripped).expect("the avcC-only clip must decode");
    let reference_frame = first_frame(&reference).expect("the reference clip must decode");

    assert_eq!(
        (stripped_frame.width, stripped_frame.height),
        (reference_frame.width, reference_frame.height)
    );
    assert_eq!(
        stripped_frame.rgba, reference_frame.rgba,
        "the same coded slices must decode identically whether or not the parameter sets repeat in-band"
    );
    assert!(
        mean_luma(&stripped_frame) > 1.0,
        "a frame that decoded to all black would pass a shape-only check"
    );
}

#[test]
fn a_stream_over_an_avcc_only_clip_seeks_and_decodes() {
    let stripped = fixtures::fixture_or_skip!(fixtures::AVCC_ONLY_CLIP);
    let mut stream = VideoStream::open(&stripped).expect("open");

    let last = fixtures::CLIP_SECONDS - 1.0 / fixtures::SQUARE_CLIP_FPS;
    let late = stream.frame_at(last).expect("decode the last frame");
    assert_eq!(stream.seek_count(), 1);

    let early = stream.frame_at(0.0).expect("decode after seeking back");
    assert_eq!(
        stream.seek_count(),
        2,
        "a backwards seek must reset the decoder"
    );

    assert!(mean_luma(&early) > 1.0 && mean_luma(&late) > 1.0);
    assert_ne!(
        early.rgba, late.rgba,
        "seeking must actually move: the reset must re-feed the avcC parameter sets"
    );
}

#[test]
fn the_backend_seam_decodes_a_real_frame() {
    let path = fixtures::fixture_or_skip!(fixtures::HD_CLIP);

    let backend = video::backend_for(&path).expect("a backend for the HD clip");
    assert_eq!(backend.kind(), video::BackendKind::OpenH264);

    let frame = video::backend::frame_at(&path, 0.0).expect("frame through the seam");
    assert_eq!(frame.width as u32, fixtures::HD_CLIP_WIDTH);
    assert_eq!(frame.height as u32, fixtures::HD_CLIP_HEIGHT);
    assert_eq!(frame.rgba.len(), frame.width * frame.height * 4);
}

#[test]
fn routing_through_the_seam_changes_no_pixel() {
    let path = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);

    for seconds in [0.0, 1.0, 2.0] {
        let direct = video::frame_at(&path, seconds).expect("direct openh264 path");
        let seamed = video::backend::frame_at(&path, seconds).expect("seam path");
        assert_eq!((direct.width, direct.height), (seamed.width, seamed.height));
        assert_eq!(
            direct.rgba, seamed.rgba,
            "seam must be pixel-identical to the direct path at {seconds}s"
        );
    }
}
