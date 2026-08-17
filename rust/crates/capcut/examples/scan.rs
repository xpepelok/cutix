fn main() {
    let Some(directory) = capcut::default_drafts_directory() else {
        eprintln!("LOCALAPPDATA is not set");
        return;
    };

    println!("scanning {}", directory.display());
    let drafts = capcut::find_drafts(&directory);
    println!("found {} draft(s)", drafts.len());

    for path in drafts {
        match capcut::load_draft(&path) {
            Ok(draft) => {
                println!("\n--- {}", draft.name);
                println!(
                    "canvas: {}x{} | fps: {} | dur: {:.2}",
                    draft.canvas.0, draft.canvas.1, draft.fps, draft.duration_seconds
                );
                println!(
                    "media: {} | texts: {} | stickers: {}",
                    draft.media.len(),
                    draft.texts.len(),
                    draft.stickers.len()
                );
                println!("segments: {}", draft.segments.len());
                for segment in &draft.segments {
                    println!(
                        "  {:<7} start={:.2} dur={:.2} speed={} vol={}",
                        segment.track_type,
                        segment.start_seconds,
                        segment.duration_seconds,
                        segment.speed,
                        segment.volume
                    );
                }
                println!("--- local assets:");
                for asset in capcut::collect_local_assets(&draft) {
                    let exists = std::path::Path::new(&asset.path).is_file();
                    println!("  {:<8} {:<24} exists={exists}", asset.kind, asset.name);
                }
                match capcut::draft_to_template(&draft) {
                    Ok(manifest) => match template::validate(&manifest) {
                        Ok(()) => println!("VALID: {} slots", manifest.slots.len()),
                        Err(error) => println!("INVALID: {error}"),
                    },
                    Err(error) => println!("template error: {error}"),
                }
            }
            Err(error) => println!("{}: {error}", path.display()),
        }
    }
}
