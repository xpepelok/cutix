use video::{Region, TrackOptions, track_region_scored};

const SIZE: usize = 128;

fn texture(x: i32, y: i32) -> f32 {
    let mut state = (x as u32).wrapping_mul(0x9e37_79b9) ^ (y as u32).wrapping_mul(0x85eb_ca6b);
    state ^= state >> 13;
    state = state.wrapping_mul(0xc2b2_ae35);
    state ^= state >> 16;
    state as f32 / u32::MAX as f32
}

fn frame(center_x: f32, center_y: f32, radius: f32) -> Vec<f32> {
    let mut data = vec![0.1; SIZE * SIZE];
    for row in 0..SIZE {
        for column in 0..SIZE {
            let dx = column as f32 - center_x;
            let dy = row as f32 - center_y;
            if dx * dx + dy * dy <= radius * radius {
                data[row * SIZE + column] =
                    0.4 + 0.6 * texture((dx + radius) as i32, (dy + radius) as i32);
            }
        }
    }
    data
}

fn main() {
    let samples = 40;
    let radius = 14.0;
    let truth = |index: usize| {
        let time = index as f32 / samples as f32;
        (
            24.0 + 80.0 * time,
            64.0 + 30.0 * (time * std::f32::consts::TAU).sin(),
        )
    };

    let options = TrackOptions {
        max_step: 24.0,
        ..TrackOptions::default()
    };
    let (start_x, start_y) = truth(0);
    let mut region = Region {
        x: start_x - radius,
        y: start_y - radius,
        width: radius * 2.0,
        height: radius * 2.0,
    };

    let mut previous = frame(start_x, start_y, radius);
    let mut worst = 0.0f32;
    println!("index  truth_x truth_y  track_x track_y  error  confidence");
    for index in 1..samples {
        let (truth_x, truth_y) = truth(index);
        let current = frame(truth_x, truth_y, radius);
        let step = track_region_scored(&previous, &current, SIZE, SIZE, region, &options);
        region = step.region;
        let center = region.center();
        let error = ((center.x - truth_x).powi(2) + (center.y - truth_y).powi(2)).sqrt();
        worst = worst.max(error);
        println!(
            "{:5}  {:7.2} {:7.2}  {:7.2} {:7.2}  {:5.2}  {:.3}",
            index, truth_x, truth_y, center.x, center.y, error, step.confidence
        );
        previous = current;
    }
    println!("worst error {:.2}px over {} samples", worst, samples - 1);
}
