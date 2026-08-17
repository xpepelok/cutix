fn mean_brightness(frame: &video::Frame) -> u64 {
    let pixels = (frame.width * frame.height).max(1) as u64;
    frame
        .rgba
        .chunks_exact(4)
        .map(|pixel| (pixel[0] as u64 + pixel[1] as u64 + pixel[2] as u64) / 3)
        .sum::<u64>()
        / pixels
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        fixtures::require(fixtures::SQUARE_CLIP)
            .display()
            .to_string()
    });

    let info = match video::probe(&path) {
        Ok(info) => info,
        Err(error) => {
            eprintln!("probe failed: {error}");
            return;
        }
    };
    println!(
        "{}x{} · {:.2}s · {} samples",
        info.width, info.height, info.duration_seconds, info.frame_count
    );

    let mut previous: Option<Vec<u8>> = None;
    let mut distinct = 0;

    for step in 0..6 {
        let at = info.duration_seconds * step as f64 / 6.0;
        let started = std::time::Instant::now();
        match video::frame_at(&path, at) {
            Ok(frame) => {
                let same = previous
                    .as_ref()
                    .map(|old| *old == frame.rgba)
                    .unwrap_or(false);
                if !same {
                    distinct += 1;
                }
                println!(
                    "  t={at:5.2}s  {}x{}  brightness={:3}  {:4}ms  {}",
                    frame.width,
                    frame.height,
                    mean_brightness(&frame),
                    started.elapsed().as_millis(),
                    if same {
                        "same as previous"
                    } else {
                        "new frame"
                    }
                );
                previous = Some(frame.rgba);
            }
            Err(error) => println!("  t={at:5.2}s  failed: {error}"),
        }
    }

    println!("distinct frames: {distinct} of 6");
    if distinct >= 5 {
        println!("SEEK OK");
    } else {
        println!("SEEK LOOKS WRONG — frames are not changing");
    }
}
