fn main() {
    let mut arguments = std::env::args().skip(1);
    let path = arguments.next().unwrap_or_else(|| {
        fixtures::require(fixtures::SQUARE_CLIP)
            .display()
            .to_string()
    });
    let seconds: f64 = arguments
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0.0);

    let info = video::probe(&path).expect("probe");
    println!(
        "{path}: {}x{}, {:.2}s, {} frames",
        info.width, info.height, info.duration_seconds, info.frame_count
    );

    println!("signalled: {:?}", video::sps_color(&path));

    let old = video::frame_at_with_color(
        &path,
        seconds,
        Some(video::ColorSpec {
            full_range: true,
            matrix: video::Matrix::Bt601,
        }),
    )
    .expect("decode");
    let new = video::frame_at(&path, seconds).expect("decode");

    for (label, frame) in [("before (full-range BT.601)", &old), ("after", &new)] {
        let mut histogram = [0u64; 256];
        let mut darkest = 255u8;
        for pixel in frame.rgba.chunks_exact(4) {
            let luma = pixel[0].min(pixel[1]).min(pixel[2]);
            histogram[luma as usize] += 1;
            darkest = darkest.min(luma);
        }
        let total: u64 = histogram.iter().sum();
        let mut running = 0u64;
        let mut first_percentile = 0usize;
        for (value, count) in histogram.iter().enumerate() {
            running += count;
            if running * 100 >= total {
                first_percentile = value;
                break;
            }
        }
        println!(
            "{label:28}: darkest {darkest:3}, 1st-percentile black {first_percentile:3}, \
             pixels at 0: {}",
            histogram[0]
        );
    }

    let mut total = 0u64;
    let mut count = 0u64;
    for (a, b) in old.rgba.chunks_exact(4).zip(new.rgba.chunks_exact(4)) {
        for channel in 0..3 {
            total += (a[channel] as i32 - b[channel] as i32).unsigned_abs() as u64;
            count += 1;
        }
    }
    println!(
        "mean absolute change from the fix: {:.3}",
        total as f64 / count as f64
    );
}
