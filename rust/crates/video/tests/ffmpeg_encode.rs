use std::path::PathBuf;

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
            std::env::set_var(video::ffmpeg::DIR_ENV, &bin);
            video::ffmpeg::can_encode()
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

fn tone(seconds: f32, sample_rate: u32, channels: usize) -> Vec<f32> {
    let frames = (seconds * sample_rate as f32) as usize;
    let mut interleaved = Vec::with_capacity(frames * channels);
    for frame in 0..frames {
        let phase = frame as f32 / sample_rate as f32 * 440.0 * std::f32::consts::TAU;
        for channel in 0..channels {
            interleaved.push(phase.sin() * if channel == 0 { 0.5 } else { 0.35 });
        }
    }
    interleaved
}

#[test]
fn a_second_of_stereo_tone_becomes_whole_aac_access_units() {
    ffmpeg_or_skip!();

    let packets = video::ffmpeg::encode_aac(&tone(1.0, 48_000, 2), 2, 48_000, 192_000)
        .expect("the aac encoder must accept planar float input");

    let samples = packets.len() * video::ffmpeg::AAC_FRAME_SAMPLES;
    eprintln!(
        "{} packets, {samples} samples, {} bytes",
        packets.len(),
        packets.iter().map(Vec::len).sum::<usize>()
    );
    assert!(samples >= 48_000, "only {samples} samples were covered");
    assert!(samples < 48_000 + 3 * video::ffmpeg::AAC_FRAME_SAMPLES);
    assert!(packets.iter().all(|packet| !packet.is_empty()));
    assert!(
        packets.iter().all(|packet| packet[0] != 0xff),
        "packets must be raw access units, not ADTS frames"
    );
}

#[test]
fn the_frame_channel_layout_offset_is_discovered_at_runtime() {
    ffmpeg_or_skip!();
    let offset =
        video::ffmpeg::frame_channel_layout_offset().expect("AVFrame::ch_layout must be locatable");
    eprintln!(
        "AVFrame::ch_layout sits at byte {offset} in libavcodec {}",
        video::ffmpeg::instance().expect("loaded").avcodec_major()
    );
    assert!(offset >= 304, "the probe must stay past the pinned prefix");
    assert_eq!(offset % 8, 0);
}

#[test]
fn mono_encodes_as_well_as_stereo() {
    ffmpeg_or_skip!();
    let packets =
        video::ffmpeg::encode_aac(&tone(0.5, 48_000, 1), 1, 48_000, 96_000).expect("mono encode");
    assert!(!packets.is_empty());
}

#[test]
fn an_empty_mixdown_flushes_to_nothing_rather_than_failing() {
    ffmpeg_or_skip!();
    let packets = video::ffmpeg::encode_aac(&[], 2, 48_000, 192_000).expect("no samples");
    assert!(packets.is_empty());
}

#[test]
fn a_rate_the_encoder_cannot_carry_is_refused() {
    ffmpeg_or_skip!();
    assert!(video::ffmpeg::encode_aac(&tone(0.1, 47_999, 2), 2, 47_999, 192_000).is_err());
}
