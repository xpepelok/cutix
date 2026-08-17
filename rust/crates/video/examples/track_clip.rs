use std::env;

use video::{frame_at, probe, track_region_scored, Region, TrackOptions};

const SIZE: usize = 128;

fn luma(frame: &video::Frame) -> Vec<f32> {
    let mut data = vec![0.0; SIZE * SIZE];
    for row in 0..SIZE {
        let source_row = row * frame.height / SIZE;
        for column in 0..SIZE {
            let source_column = column * frame.width / SIZE;
            let offset = (source_row * frame.width + source_column) * 4;
            data[row * SIZE + column] = (0.299 * frame.rgba[offset] as f32
                + 0.587 * frame.rgba[offset + 1] as f32
                + 0.114 * frame.rgba[offset + 2] as f32)
                / 255.0;
        }
    }
    data
}

fn main() {
    let mut args = env::args().skip(1);
    let path = args.next().expect("usage: track_clip <path> [x y w h]");
    let x: f32 = args.next().map(|v| v.parse().unwrap()).unwrap_or(0.35);
    let y: f32 = args.next().map(|v| v.parse().unwrap()).unwrap_or(0.35);
    let width: f32 = args.next().map(|v| v.parse().unwrap()).unwrap_or(0.3);
    let height: f32 = args.next().map(|v| v.parse().unwrap()).unwrap_or(0.3);

    let info = probe(&path).expect("probe failed");
    println!(
        "clip {}x{} duration {:.2}s frames {}",
        info.width, info.height, info.duration_seconds, info.frame_count
    );

    let start: f64 = env::var("TRACK_START")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let span: f64 = env::var("TRACK_SPAN")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(info.duration_seconds);
    let samples: usize = env::var("TRACK_SAMPLES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(40);
    let step = span / samples as f64;
    let mut region = Region {
        x: x * SIZE as f32,
        y: y * SIZE as f32,
        width: width * SIZE as f32,
        height: height * SIZE as f32,
    };

    let mut previous: Option<Vec<f32>> = None;
    let options = TrackOptions {
        max_step: 32.0,
        ..TrackOptions::default()
    };

    for index in 0..samples {
        let seconds = start + index as f64 * step;
        let frame = match frame_at(&path, seconds) {
            Ok(frame) => frame,
            Err(error) => {
                println!("{:6.2}s decode error {error}", seconds);
                continue;
            }
        };
        let current = luma(&frame);
        if let Some(reference) = previous.as_ref() {
            let motion: f32 = reference
                .iter()
                .zip(current.iter())
                .map(|(a, b)| (a - b).abs())
                .sum::<f32>()
                / (SIZE * SIZE) as f32;
            let step = track_region_scored(reference, &current, SIZE, SIZE, region, &options);
            region = step.region;
            let center = region.center();
            println!(
                "{:6.2}s center=({:6.2}, {:6.2}) norm=({:.3}, {:.3}) confidence={:.3} motion={:.4}",
                seconds,
                center.x,
                center.y,
                center.x / SIZE as f32,
                center.y / SIZE as f32,
                step.confidence,
                motion
            );
        } else {
            let center = region.center();
            println!(
                "{:6.2}s center=({:6.2}, {:6.2}) anchor",
                seconds, center.x, center.y
            );
        }
        previous = Some(current);
    }
}
