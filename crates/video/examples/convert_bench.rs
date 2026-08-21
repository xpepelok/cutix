use std::time::Instant;

fn main() {
    let (width, height) = (1920usize, 1080usize);
    let y = vec![128u8; width * height];
    let u = vec![100u8; width * height / 4];
    let v = vec![160u8; width * height / 4];
    let mut rgba = vec![0u8; width * height * 4];

    let rounds = 100;
    let started = Instant::now();
    for _ in 0..rounds {
        video::color::i420_to_rgba(
            &y,
            &u,
            &v,
            (width, height),
            (width, width / 2, width / 2),
            video::ColorSpec::assumed_for_height(height),
            &mut rgba,
        );
    }
    let each = started.elapsed() / rounds;
    println!("i420_to_rgba 1920x1080: {each:?} per frame");

    let started = Instant::now();
    for _ in 0..rounds {
        let fresh = vec![0u8; width * height * 4];
        std::hint::black_box(&fresh);
    }
    println!(
        "fresh rgba allocation: {:?} per frame",
        started.elapsed() / rounds
    );

    let started = Instant::now();
    for _ in 0..rounds {
        let copy = rgba.clone();
        std::hint::black_box(&copy);
    }
    println!("rgba clone: {:?} per frame", started.elapsed() / rounds);
}
