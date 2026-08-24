use ml::models;

fn main() {
    let Some(model) = models::find_model("modnet") else {
        eprintln!("model not found in registry");
        return;
    };

    println!("model: {}", models::describe(model));
    println!("cache: {}", models::cached_path(model).display());
    println!("cached already: {}", models::is_cached(model));

    let mut last_reported = -1i32;
    let path = match models::ensure_downloaded(model, |progress| {
        let percent = (progress * 100.0) as i32;
        if percent / 10 != last_reported / 10 {
            last_reported = percent;
            println!("  downloading {percent}%");
        }
    }) {
        Ok(path) => path,
        Err(error) => {
            eprintln!("download failed: {error}");
            return;
        }
    };

    println!(
        "downloaded {} MB",
        models::cached_size_mb(model).unwrap_or(0)
    );

    let mut session = match ml::SegmentationModel::load(&path, model.input_size) {
        Ok(session) => session,
        Err(error) => {
            eprintln!("load failed: {error}");
            return;
        }
    };

    let (width, height) = (256usize, 256usize);
    let mut rgba = vec![0u8; width * height * 4];
    for y in 60..196 {
        for x in 80..176 {
            let index = (y * width + x) * 4;
            rgba[index] = 220;
            rgba[index + 1] = 190;
            rgba[index + 2] = 170;
            rgba[index + 3] = 255;
        }
    }

    match session.matte(&rgba, width, height) {
        Ok(alpha) => {
            println!("matte: {} px", alpha.len());
            println!("coverage: {:.1}%", ml::mask_coverage(&alpha) * 100.0);
            println!("MATTE OK");
        }
        Err(error) => eprintln!("inference failed: {error}"),
    }
}
