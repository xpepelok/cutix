use std::path::Path;

use cutix_export::{PACKAGE_EXTENSION, PackageStage, export_package, import_package};
use cutix_project::model::TimelineElement;
use cutix_project::{MediaStore, Project, ProjectStore};
use serde_json::json;
use time::{FrameRate, MediaTime};

fn seconds(value: f64) -> MediaTime {
    MediaTime::from_seconds_f64(value).expect("time")
}

fn element(id: &str, media_id: &str, start: f64, duration: f64) -> TimelineElement {
    serde_json::from_value(json!({
        "type": "image",
        "id": id,
        "name": id,
        "duration": seconds(duration).as_ticks(),
        "startTime": seconds(start).as_ticks(),
        "trimStart": 0,
        "trimEnd": 0,
        "mediaId": media_id,
        "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 12.0, "y": -4.0 }, "rotate": 15.0 },
        "opacity": 0.75
    }))
    .expect("element")
}

fn seeded_project(store: &ProjectStore) -> Project {
    let mut project = store.create("Round trip").expect("create");
    project.settings.canvas_size.width = 1280;
    project.settings.canvas_size.height = 720;
    project.settings.fps = FrameRate::FPS_24;
    let scene = project.scenes.first_mut().expect("scene");
    scene
        .tracks
        .main
        .elements_mut()
        .push(element("first", "photo", 0.0, 2.0));
    scene
        .tracks
        .main
        .elements_mut()
        .push(element("second", "photo", 2.0, 3.5));
    store.save(&project).expect("save");

    let media = MediaStore::for_project(store, &project.metadata.id);
    let files = media.files_directory();
    std::fs::create_dir_all(&files).expect("media dir");
    std::fs::write(
        files.join("photo.png"),
        b"not really a png, but bytes are bytes",
    )
    .expect("media file");
    project
}

fn timeline_shape(project: &Project) -> Vec<String> {
    project
        .scenes
        .iter()
        .flat_map(|scene| {
            scene
                .tracks
                .all()
                .flat_map(|track| track.elements().to_vec())
        })
        .map(|element| serde_json::to_string(&element).expect("element json"))
        .collect()
}

#[test]
fn a_project_package_round_trips_through_a_second_store() {
    let source_root = tempfile::tempdir().expect("tempdir");
    let target_root = tempfile::tempdir().expect("tempdir");
    let archive_root = tempfile::tempdir().expect("tempdir");
    let source = ProjectStore::new(source_root.path());
    let target = ProjectStore::new(target_root.path());

    let original = seeded_project(&source);
    let archive = archive_root
        .path()
        .join(format!("round-trip.{PACKAGE_EXTENSION}"));

    let mut stages = Vec::new();
    let outcome = export_package(&source, &original, &archive, &mut |progress| {
        if stages.last() != Some(&progress.stage) {
            stages.push(progress.stage);
        }
    })
    .expect("export package");

    assert!(archive.is_file());
    assert!(outcome.bytes > 0);
    assert!(outcome.entries >= 2, "entries {}", outcome.entries);
    assert_eq!(stages.first(), Some(&PackageStage::Collecting));
    assert_eq!(stages.last(), Some(&PackageStage::Finishing));

    let head = std::fs::read(&archive).expect("read archive");
    assert_eq!(&head[0..2], b"PK");

    let reopened = import_package(&target, &archive).expect("import package");

    assert_eq!(reopened.metadata.id, original.metadata.id);
    assert_eq!(reopened.metadata.name, original.metadata.name);
    assert_eq!(
        reopened.settings.canvas_size.width,
        original.settings.canvas_size.width
    );
    assert_eq!(reopened.settings.fps, original.settings.fps);
    assert_eq!(timeline_shape(&reopened), timeline_shape(&original));

    let loaded = target.load(&reopened.metadata.id).expect("load").project;
    assert_eq!(timeline_shape(&loaded), timeline_shape(&original));

    let media = MediaStore::for_project(&target, &reopened.metadata.id);
    let copied = media.files_directory().join("photo.png");
    assert!(
        copied.is_file(),
        "the media library travelled with the project"
    );
    assert_eq!(
        std::fs::read(&copied).expect("read media"),
        b"not really a png, but bytes are bytes"
    );
}

#[test]
fn importing_into_the_store_that_wrote_it_never_overwrites_the_original() {
    let root = tempfile::tempdir().expect("tempdir");
    let archive_root = tempfile::tempdir().expect("tempdir");
    let store = ProjectStore::new(root.path());
    let original = seeded_project(&store);
    let archive = archive_root
        .path()
        .join(format!("copy.{PACKAGE_EXTENSION}"));
    export_package(&store, &original, &archive, &mut |_| {}).expect("export package");

    let imported = import_package(&store, &archive).expect("import package");
    assert_ne!(imported.metadata.id, original.metadata.id);
    assert_eq!(timeline_shape(&imported), timeline_shape(&original));
    assert!(store.exists(&original.metadata.id));
}

#[test]
fn a_file_that_is_not_a_package_is_refused_without_leaving_a_project_behind() {
    let root = tempfile::tempdir().expect("tempdir");
    let store = ProjectStore::new(root.path());
    let before = store.list_project_ids().expect("list");

    let bogus = root.path().join("not-a-package.ocut");
    std::fs::write(&bogus, b"PK\x03\x04 and then nonsense").expect("write");
    assert!(import_package(&store, Path::new(&bogus)).is_err());
    assert_eq!(store.list_project_ids().expect("list"), before);
}

/// Adds a media asset that lives outside the project directory, as a linked file.
fn linked_asset(store: &ProjectStore, project_id: &str, source: &Path) -> String {
    let media = MediaStore::for_project(store, project_id);
    let asset: cutix_project::MediaAssetData = serde_json::from_value(json!({
        "id": "linked",
        "name": "linked clip",
        "type": "image",
        "size": 34,
        "lastModified": 0,
        "fileName": "linked.png",
        "sourcePath": source.display().to_string(),
    }))
    .expect("asset metadata");
    std::fs::create_dir_all(media.metadata_directory()).expect("metadata dir");
    media.write_metadata(&asset).expect("write metadata");
    asset.id
}

#[test]
fn an_imported_package_uses_its_own_copy_of_linked_media_not_the_path_it_came_from() {
    let source_root = tempfile::tempdir().expect("tempdir");
    let target_root = tempfile::tempdir().expect("tempdir");
    let outside_root = tempfile::tempdir().expect("tempdir");
    let archive_root = tempfile::tempdir().expect("tempdir");
    let source = ProjectStore::new(source_root.path());
    let target = ProjectStore::new(target_root.path());

    // A file the project links to from outside its own directory.
    let outside = outside_root.path().join("linked.png");
    std::fs::write(&outside, b"the bytes the project was made with").expect("linked file");

    let project = seeded_project(&source);
    linked_asset(&source, &project.metadata.id, &outside);

    let archive = archive_root
        .path()
        .join(format!("linked.{PACKAGE_EXTENSION}"));
    export_package(&source, &project, &archive, &mut |_| {}).expect("export package");

    // The original is replaced by something else at the same absolute path. This is the
    // case that quietly ruins a project: the recorded path still resolves, to the wrong
    // file. Deleting it would only have shown up as missing media.
    std::fs::write(&outside, b"a completely different file now").expect("overwrite");

    let imported = import_package(&target, &archive).expect("import package");
    let media = MediaStore::for_project(&target, &imported.metadata.id);
    let asset = media.get("linked").expect("the linked asset came across");
    assert_eq!(
        asset.source_path, None,
        "the imported project still points outside itself"
    );

    let resolved = media.source_file(&asset);
    assert!(
        resolved.starts_with(target.project_directory(&imported.metadata.id)),
        "resolved to {} which is outside the project",
        resolved.display()
    );
    assert_eq!(
        std::fs::read(&resolved).expect("read media"),
        b"the bytes the project was made with",
        "the package copy is what the project uses"
    );
}

#[test]
fn a_package_cannot_be_written_while_a_linked_file_is_missing() {
    let root = tempfile::tempdir().expect("tempdir");
    let outside_root = tempfile::tempdir().expect("tempdir");
    let archive_root = tempfile::tempdir().expect("tempdir");
    let store = ProjectStore::new(root.path());
    let project = seeded_project(&store);

    let outside = outside_root.path().join("gone.png");
    std::fs::write(&outside, b"here for now").expect("linked file");
    linked_asset(&store, &project.metadata.id, &outside);
    std::fs::remove_file(&outside).expect("remove the linked file");

    let archive = archive_root
        .path()
        .join(format!("incomplete.{PACKAGE_EXTENSION}"));
    let error = export_package(&store, &project, &archive, &mut |_| {})
        .expect_err("an incomplete package must be refused");
    let message = error.to_string();
    assert!(
        message.contains("linked clip") && message.contains("gone.png"),
        "the error must name the media that is missing: {message}"
    );
}

#[test]
fn a_package_whose_project_is_corrupt_leaves_nothing_in_the_library() {
    let source_root = tempfile::tempdir().expect("tempdir");
    let target_root = tempfile::tempdir().expect("tempdir");
    let archive_root = tempfile::tempdir().expect("tempdir");
    let source = ProjectStore::new(source_root.path());
    let target = ProjectStore::new(target_root.path());

    let project = seeded_project(&source);
    let archive = archive_root
        .path()
        .join(format!("corrupt.{PACKAGE_EXTENSION}"));
    export_package(&source, &project, &archive, &mut |_| {}).expect("export package");

    // Rewrite the archive with the project document truncated. Everything else in it is
    // still valid, so extraction gets a long way in before the problem is discovered.
    let corrupt = archive_root
        .path()
        .join(format!("truncated.{PACKAGE_EXTENSION}"));
    rewrite_with_broken_project(&archive, &corrupt);

    let before = target.list_project_ids().expect("list");
    assert!(
        import_package(&target, &corrupt).is_err(),
        "a corrupt project document must be refused"
    );
    assert_eq!(
        target.list_project_ids().expect("list"),
        before,
        "a half-extracted project was left in the library"
    );
    let leftovers: Vec<_> = std::fs::read_dir(target_root.path())
        .expect("read library")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".importing-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "the staging directory was not cleaned up: {leftovers:?}"
    );
}

/// Copies an archive, replacing project.json with bytes that are not a project.
fn rewrite_with_broken_project(from: &Path, to: &Path) {
    let mut reader =
        zip::ZipArchive::new(std::fs::File::open(from).expect("open archive")).expect("read zip");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(to).expect("create archive"));
    let options = zip::write::SimpleFileOptions::default();
    for index in 0..reader.len() {
        let mut entry = reader.by_index(index).expect("entry");
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut entry, &mut bytes).expect("read entry");
        if name == "project.json" {
            bytes = b"{ this is not json".to_vec();
        }
        writer.start_file(name, options).expect("start entry");
        std::io::Write::write_all(&mut writer, &bytes).expect("write entry");
    }
    writer.finish().expect("finish archive");
}
