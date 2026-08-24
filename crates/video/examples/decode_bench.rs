use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("path");
    let frames: u32 = std::env::args()
        .nth(2)
        .and_then(|v| v.parse().ok())
        .unwrap_or(120);
    let path = std::path::Path::new(&path);

    println!("{}", video::ffmpeg::status());
    let info = video::probe(path).expect("probe");
    let fps = f64::from(info.frame_count) / info.duration_seconds;
    println!("{}x{} at {:.1} fps", info.width, info.height, fps);

    let mut stream = video::VideoStream::open(path).expect("open");
    let _ = stream.frame_at(0.0);

    let started = Instant::now();
    for index in 1..=frames {
        let _ = stream.frame_at(index as f64 / fps).expect("frame");
    }
    let each = started.elapsed() / frames;
    println!("frame_at (decode + convert + allocate): {each:?} per frame");
}
