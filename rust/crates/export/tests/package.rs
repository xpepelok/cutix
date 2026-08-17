use std::path::Path;

use cutix_export::{export_package, import_package, PackageStage, PACKAGE_EXTENSION};
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
                .into_iter()
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
