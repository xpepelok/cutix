fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| String::from(r"C:\Users\ksana\Videos\test-clip2.mp4"));
    let path = std::path::PathBuf::from(path);

    println!("file   : {}", path.display());
    println!("exists : {}", path.is_file());
    println!("ffmpeg : {}", video::ffmpeg::status());
    println!("decode : {}", video::ffmpeg::can_decode());

    match cutix_playback::audio_decode::decode_audio(&path) {
        Ok(pcm) => println!(
            "audio  : {} Hz, {} channels, {:.2}s",
            pcm.sample_rate,
            pcm.channels,
            pcm.duration_seconds()
        ),
        Err(error) => println!("audio  : FAILED — {error}"),
    }
}
