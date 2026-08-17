use cutix_playback::decode_audio;

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
            std::env::set_var(video::ffmpeg::DIR_ENV, &bin);
            return video::ffmpeg::can_decode();
        }
    }
    false
}

#[test]
fn wav_and_mp3_keep_their_existing_contract() {
    let path = fixtures::fixture_or_skip!(fixtures::AUDIO_CLIP);
    let buffer = decode_audio(&path).expect("mp3 must still decode");
    assert!(buffer.sample_rate > 0);
    assert_eq!(buffer.samples.len(), buffer.channels);
    assert!(buffer.duration_seconds() > 0.0);
}

#[test]
fn an_unknown_extension_is_still_refused_clearly() {
    let path = std::env::temp_dir().join("cutix-audio.unknownext");
    std::fs::write(&path, b"nope").expect("write");
    let error = decode_audio(&path).expect_err("refusal");
    assert!(error.to_string().contains("unknownext"));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn aac_and_flac_arrive_through_the_same_contract() {
    if !ffmpeg_ready() {
        eprintln!("SKIPPED: no LGPL FFmpeg under .fixtures/tools");
        return;
    }

    for name in [fixtures::AAC_CLIP, fixtures::FLAC_CLIP] {
        let Some(path) = fixtures::fixture(name) else {
            continue;
        };
        let buffer = decode_audio(&path).unwrap_or_else(|error| panic!("{name}: {error}"));

        assert_eq!(
            buffer.samples.len(),
            buffer.channels,
            "{name}: PcmBuffer must stay planar-per-channel"
        );
        assert!(buffer.frame_count() > 0, "{name}: no samples");
        assert!(buffer.duration_seconds() > 1.0, "{name}: too short");
        assert_eq!(buffer.channel(0).len(), buffer.frame_count());

        eprintln!(
            "OK {name}: {} Hz, {} ch, {:.2}s",
            buffer.sample_rate,
            buffer.channels,
            buffer.duration_seconds()
        );
    }
}

#[test]
fn audio_inside_a_webm_is_reachable_through_the_contract() {
    if !ffmpeg_ready() {
        return;
    }
    let Some(path) = fixtures::fixture(fixtures::VP9_CLIP) else {
        return;
    };
    let buffer = decode_audio(&path).expect("opus in webm");
    assert!(buffer.duration_seconds() > 1.0);
}
