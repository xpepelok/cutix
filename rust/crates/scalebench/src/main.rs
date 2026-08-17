mod gen;
mod mem;
mod runtime;

use std::time::Instant;

use cutix_project::ProjectStore;

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn scratch(name: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!("cutix-scalebench-{name}"));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("scratch");
    directory
}

fn document_stats(project: &cutix_project::Project) -> (usize, usize) {
    let scene = project.main_scene().expect("scene");
    let tracks = scene.tracks.all().count();
    let elements = scene.tracks.all().map(|track| track.elements().len()).sum();
    (tracks, elements)
}

fn legacy_save(project: &cutix_project::Project) -> Vec<u8> {
    let document = project.clone();
    serde_json::to_vec_pretty(&document).expect("serialise")
}

fn sizes() {
    println!("== project document scale: save / load / list ==");
    println!(
        "{:>9} {:>7} {:>8} {:>8} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "elements",
        "tracks",
        "json MB",
        "was MB",
        "build ms",
        "save ms",
        "was save",
        "load ms",
        "migrate ms"
    );

    for elements in [50usize, 200, 500, 1000, 2000, 5000] {
        let shape = gen::Shape {
            elements,
            media_files: (elements / 5).clamp(10, 200),
            minutes: 30.0,
            ..gen::Shape::default()
        };

        let start = Instant::now();
        let project = gen::build(&shape, 0x5EED);
        let build_ms = elapsed_ms(start);
        let (tracks, counted) = document_stats(&project);

        let directory = scratch(&format!("sizes-{elements}"));
        let store = ProjectStore::new(&directory);

        let start = Instant::now();
        store.save(&project).expect("save");
        let save_ms = elapsed_ms(start);

        let path = store.project_file(&project.metadata.id);
        let bytes = std::fs::metadata(&path).expect("stat").len();

        let legacy_path = directory.join("legacy-project.json");
        let start = Instant::now();
        let legacy = legacy_save(&project);
        cutix_project::store::write_atomic(&legacy_path, &legacy).expect("legacy write");
        let legacy_save_ms = elapsed_ms(start);
        let legacy_bytes = legacy.len() as u64;
        let _ = std::fs::remove_file(&legacy_path);

        let start = Instant::now();
        let loaded = store.load(&project.metadata.id).expect("load");
        let load_ms = elapsed_ms(start);
        assert_eq!(loaded.project.metadata.id, project.metadata.id);

        let raw = store.load_raw(&project.metadata.id).expect("raw");
        let mut downgraded = raw.clone();
        downgraded["version"] = serde_json::json!(1);
        let start = Instant::now();
        let _ = cutix_project::migrate_to_current(downgraded, "1970-01-01T00:00:00.000Z");
        let migrate_ms = elapsed_ms(start);

        println!(
            "{counted:>9} {tracks:>7} {:>8.2} {:>8.2} {build_ms:>10.1} {save_ms:>10.1} \
             {legacy_save_ms:>10.1} {load_ms:>10.1} {migrate_ms:>10.1}",
            bytes as f64 / (1024.0 * 1024.0),
            legacy_bytes as f64 / (1024.0 * 1024.0)
        );
        let _ = std::fs::remove_dir_all(&directory);
    }
}

fn library() {
    println!("== project library listing: cost of the project grid ==");
    println!(
        "{:>8} {:>10} {:>12} {:>14} {:>12} {:>14}",
        "projects", "total MB", "list ms", "ms/project", "was list ms", "was ms/proj"
    );

    for count in [1usize, 5, 10, 25, 50] {
        let directory = scratch(&format!("library-{count}"));
        let store = ProjectStore::new(&directory);
        let shape = gen::Shape {
            elements: 500,
            ..gen::Shape::default()
        };
        let mut total = 0u64;
        for index in 0..count {
            let project = gen::build(&shape, 0x5EED + index as u64);
            store.save(&project).expect("save");
            total += std::fs::metadata(store.project_file(&project.metadata.id))
                .expect("stat")
                .len();
        }

        let start = Instant::now();
        let mut legacy = Vec::new();
        for id in store.list_project_ids().expect("ids") {
            legacy.push(store.load(&id).expect("load").project.summary());
        }
        let legacy_list_ms = elapsed_ms(start);
        assert_eq!(legacy.len(), count);

        let start = Instant::now();
        let summaries = store.list_projects().expect("list");
        let list_ms = elapsed_ms(start);
        assert_eq!(summaries.len(), count);

        println!(
            "{count:>8} {:>10.2} {list_ms:>12.1} {:>14.1} {legacy_list_ms:>12.1} {:>14.1}",
            total as f64 / (1024.0 * 1024.0),
            list_ms / count as f64,
            legacy_list_ms / count as f64
        );
        let _ = std::fs::remove_dir_all(&directory);
    }
}

fn edit_churn(cycles: u64) {
    println!("== in-memory document churn: does editing a big project leak? ==");
    let shape = gen::Shape {
        elements: 2000,
        ..gen::Shape::default()
    };
    let directory = scratch("churn");
    let store = ProjectStore::new(&directory);
    let baseline = mem::snapshot();
    mem::report("baseline");

    for cycle in 0..cycles {
        let mut project = gen::build(&shape, 0x5EED + cycle);
        store.save(&project).expect("save");
        let id = project.metadata.id.clone();
        for _ in 0..10 {
            let scene = project.scenes.first_mut().expect("scene");
            let elements = scene.tracks.main.elements_mut();
            if let Some(element) = elements.pop() {
                elements.insert(0, element);
            }
            store.save(&project).expect("save");
        }
        let reloaded = store.load(&id).expect("load");
        drop(reloaded);
        drop(project);
        let _ = std::fs::remove_dir_all(store.project_directory(&id));
        mem::report(&format!("after cycle {}", cycle + 1));
    }

    let end = mem::snapshot();
    println!(
        "  delta over {cycles} open/edit/save/close cycles: working_set {:+.1} MB, private {:+.1} MB",
        mem::mb(end.working_set) - mem::mb(baseline.working_set),
        mem::mb(end.private) - mem::mb(baseline.private)
    );
    let _ = std::fs::remove_dir_all(&directory);
}

fn best<F: FnMut()>(mut run: F) -> f64 {
    for _ in 0..5 {
        run();
    }
    let mut best = f64::MAX;
    for _ in 0..30 {
        let start = Instant::now();
        run();
        best = best.min(elapsed_ms(start));
    }
    best
}

fn edits() {
    println!("== per-edit UI-thread cost ==");
    println!(
        "{:>9} {:>10} {:>10} {:>13} {:>15} {:>13} {:>13}",
        "elements",
        "clone ms",
        "cmp ms",
        "externals ms",
        "prj clone ms",
        "was ms/edit",
        "now ms/edit"
    );

    for elements in [500usize, 2000, 5000] {
        let shape = gen::Shape {
            elements,
            media_files: (elements / 5).clamp(10, 200),
            minutes: 30.0,
            ..gen::Shape::default()
        };
        let mut project = gen::build(&shape, 0x5EED);
        let directory = scratch(&format!("edits-{elements}"));
        let store = ProjectStore::new(&directory);

        let tracks = project.scenes[0].tracks.clone();
        let other = tracks.clone();

        let clone_ms = best(|| drop(std::hint::black_box(tracks.clone())));
        let compare_ms = best(|| {
            std::hint::black_box(tracks == other);
        });
        let externalize_ms = best(|| {
            let _ = store.externalize_mattes(&mut project);
        });
        let project_clone_ms = best(|| drop(std::hint::black_box(project.clone())));

        let was = 3.0 * clone_ms + compare_ms + externalize_ms + project_clone_ms;
        let now = clone_ms + compare_ms;

        println!(
            "{elements:>9} {clone_ms:>10.3} {compare_ms:>10.3} {externalize_ms:>13.3} \
             {project_clone_ms:>15.3} {was:>13.3} {now:>13.3}"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }
}

fn arg(position: usize) -> Option<usize> {
    std::env::args().nth(position)?.parse().ok()
}

fn main() {
    let command = std::env::args().nth(1).unwrap_or_else(|| "all".to_string());
    match command.as_str() {
        "sizes" => sizes(),
        "library" => library(),
        "edits" => edits(),
        "churn" => edit_churn(arg(2).unwrap_or(20) as u64),
        "decode" => runtime::decode_growth(arg(2).unwrap_or(60)),
        "compose" => runtime::compose(arg(2).unwrap_or(300), arg(3).unwrap_or(400)),
        "cycles" => runtime::compose_cycles(
            arg(2).unwrap_or(12),
            arg(3).unwrap_or(200),
            arg(4).unwrap_or(40),
        ),
        _ => {
            sizes();
            println!();
            library();
            println!();
            edit_churn(20);
        }
    }
}
