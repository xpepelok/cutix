fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        fixtures::require(fixtures::SQUARE_CLIP)
            .display()
            .to_string()
    });

    println!("file: {path}");

    match video::probe(&path) {
        Ok(info) => println!(
            "probe: {}x{} · {:.2}s · {} samples",
            info.width, info.height, info.duration_seconds, info.frame_count
        ),
        Err(error) => {
            eprintln!("probe failed: {error}");
            return;
        }
    }

    match video::first_frame(&path) {
        Ok(frame) => {
            let pixels = frame.width * frame.height;
            let mean: u64 = frame
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .map(|pixel| (pixel[0] as u64 + pixel[1] as u64 + pixel[2] as u64) / 3)
                .sum::<u64>()
                / pixels.max(1) as u64;

            println!("frame: {}x{} rgba", frame.width, frame.height);
            println!("bytes: {}", frame.rgba.len());
            println!("mean brightness: {mean}");
            println!("DECODE OK");
        }
        Err(error) => eprintln!("decode failed: {error}"),
    }
}
