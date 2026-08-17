use ml::models;

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        fixtures::require(fixtures::SQUARE_CLIP)
            .display()
            .to_string()
    });

    let frame = match video::first_frame(&path) {
        Ok(frame) => frame,
        Err(error) => {
            eprintln!("decode failed: {error}");
            return;
        }
    };
    println!("frame: {}x{}", frame.width, frame.height);

    let Some(model) = models::find_model("modnet") else {
        eprintln!("model not found");
        return;
    };

    let model_path = match models::ensure_downloaded(model, |_| {}) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("download failed: {error}");
            return;
        }
    };

    let mut session = match ml::SegmentationModel::load(&model_path, model.input_size) {
        Ok(session) => session,
        Err(error) => {
            eprintln!("load failed: {error}");
            return;
        }
    };

    let started = std::time::Instant::now();
    match session.matte(&frame.rgba, frame.width, frame.height) {
        Ok(alpha) => {
            let coverage = ml::mask_coverage(&alpha) * 100.0;
            println!(
                "matte: {} px in {} ms",
                alpha.len(),
                started.elapsed().as_millis()
            );
            println!("subject coverage: {coverage:.1}%");
            println!("MATTE ON REAL FRAME OK");
        }
        Err(error) => eprintln!("inference failed: {error}"),
    }
}
