use std::fs;

use cutix_project::model::{Project, TimelineElement, Track, Transform, VideoElement};
use cutix_project::store::write_atomic;
use cutix_project::{ProjectError, ProjectStore};
use serde_json::Value;
use time::MediaTime;

fn store() -> (tempfile::TempDir, ProjectStore) {
    let directory = tempfile::tempdir().expect("temp dir");
    let store = ProjectStore::new(directory.path().join("cutix"));
    (directory, store)
}

fn video_element(id: &str, start: f64, duration: f64) -> TimelineElement {
    TimelineElement::Video(VideoElement {
        base: cutix_project::model::BaseElementFields {
            id: id.to_string(),
            name: format!("{id}.mp4"),
            duration: MediaTime::from_seconds_f64(duration).expect("duration"),
            start_time: MediaTime::from_seconds_f64(start).expect("start"),
            trim_start: MediaTime::ZERO,
            trim_end: MediaTime::ZERO,
            source_duration: MediaTime::from_seconds_f64(duration),
            animations: None,
        },
        media_id: "media-1".to_string(),
        volume: Some(-6.0),
        muted: Some(false),
        is_source_audio_enabled: Some(true),
        hidden: None,
        retime: None,
        reversed_from: None,
        transform: Transform::default(),
        crop: None,
        opacity: 1.0,
        blend_mode: None,
        effects: None,
        masks: None,
        cutout: None,
        transition: None,
        motion: None,
        extra: Default::default(),
    })
}

#[test]
fn saved_project_round_trips_through_disk() {
    let (_guard, store) = store();
    let mut project = store.create("My film").expect("create");

    project.metadata.duration = MediaTime::from_seconds_f64(6.5).expect("duration");
    project.timeline_view_state = Some(cutix_project::TimelineViewState {
        zoom_level: 1.5,
        scroll_left: 240.0,
        playhead_time: MediaTime::from_seconds_f64(2.0).expect("playhead"),
    });
    project.scenes[0]
        .tracks
        .main
        .elements_mut()
        .push(video_element("element-1", 1.0, 4.0));
    store.save(&project).expect("save");

    let loaded = store.load(&project.metadata.id).expect("load").project;

    assert_eq!(loaded, project);
    assert_eq!(loaded.computed_duration().as_ticks(), 5 * 120_000);
    assert_eq!(loaded.version, cutix_project::CURRENT_PROJECT_VERSION);

    let on_disk = fs::read(store.project_file(&project.metadata.id)).expect("read");
    let reserialised = serde_json::to_vec(&loaded).expect("serialise");
    assert_eq!(on_disk, reserialised);
}

#[test]
fn new_project_serialises_with_web_field_names() {
    let project = Project::new("Naming", "2024-01-01T00:00:00.000Z".to_string());
    let document: Value = serde_json::to_value(&project).expect("serialise");

    assert!(document["metadata"]["createdAt"].is_string());
    assert!(document["metadata"]["updatedAt"].is_string());
    assert_eq!(document["metadata"]["duration"].as_i64(), Some(0));
    assert!(document["currentSceneId"].is_string());
    assert_eq!(document["settings"]["fps"]["numerator"].as_i64(), Some(30));
    assert_eq!(
        document["settings"]["canvasSize"]["width"].as_i64(),
        Some(1920)
    );
    assert_eq!(
        document["settings"]["background"]["type"].as_str(),
        Some("color")
    );
    assert_eq!(
        document["settings"]["canvasSizeMode"].as_str(),
        Some("preset")
    );
    assert_eq!(document["scenes"][0]["isMain"].as_bool(), Some(true));
    assert_eq!(
        document["scenes"][0]["tracks"]["main"]["type"].as_str(),
        Some("video")
    );
    assert_eq!(document["version"].as_u64(), Some(30));
}

#[test]
fn crud_operations_behave() {
    let (_guard, store) = store();
    let first = store.create("First").expect("create first");
    let second = store.create("Second").expect("create second");

    let ids = store.list_project_ids().expect("ids");
    assert_eq!(ids.len(), 2);

    let renamed = store.rename(&first.metadata.id, "Renamed").expect("rename");
    assert_eq!(renamed.metadata.name, "Renamed");
    assert_eq!(
        store
            .load(&first.metadata.id)
            .expect("load")
            .project
            .metadata
            .name,
        "Renamed"
    );

    let copy = store.duplicate(&second.metadata.id).expect("duplicate");
    assert_ne!(copy.metadata.id, second.metadata.id);
    assert_eq!(copy.metadata.name, "Second (copy)");
    assert_eq!(store.list_projects().expect("list").len(), 3);

    store.delete(&second.metadata.id).expect("delete");
    assert!(!store.exists(&second.metadata.id));
    assert!(matches!(
        store.load(&second.metadata.id),
        Err(ProjectError::NotFound { .. })
    ));
    assert!(matches!(
        store.delete("missing"),
        Err(ProjectError::NotFound { .. })
    ));
}

#[test]
fn interrupted_write_leaves_the_previous_project_intact() {
    let (_guard, store) = store();
    let mut project = store.create("Safe").expect("create");
    let path = store.project_file(&project.metadata.id);
    let original = fs::read(&path).expect("read original");

    let directory = path.parent().expect("parent");
    let stale = directory.join(".project.json.crash.tmp");
    fs::write(&stale, b"{ truncated").expect("write stale temp");

    assert_eq!(fs::read(&path).expect("read after crash"), original);
    let recovered = store.load(&project.metadata.id).expect("load after crash");
    assert_eq!(recovered.project.metadata.name, "Safe");

    project.metadata.name = "Safer".to_string();
    store.save(&project).expect("save");
    assert_eq!(
        store
            .load(&project.metadata.id)
            .expect("reload")
            .project
            .metadata
            .name,
        "Safer"
    );

    let leftovers: Vec<_> = fs::read_dir(directory)
        .expect("read dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".tmp") && name != ".project.json.crash.tmp")
        .collect();
    assert!(leftovers.is_empty(), "unexpected temp files: {leftovers:?}");
}

#[test]
fn atomic_write_replaces_content_without_partial_states() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("data.json");

    write_atomic(&path, b"first").expect("first write");
    assert_eq!(fs::read(&path).expect("read"), b"first");

    write_atomic(&path, b"second-longer").expect("second write");
    assert_eq!(fs::read(&path).expect("read"), b"second-longer");
}

#[test]
fn importing_a_web_document_migrates_and_stores_it() {
    let (_guard, store) = store();
    let document = serde_json::json!({
        "id": "imported-project",
        "name": "From the web",
        "createdAt": "2023-05-05T00:00:00.000Z",
        "updatedAt": "2023-05-05T00:00:00.000Z",
        "fps": 24,
        "canvasSize": { "width": 1080, "height": 1920 },
        "backgroundType": "blur",
        "blurIntensity": 4
    });

    let project = store.import_document(document).expect("import");

    assert_eq!(project.metadata.id, "imported-project");
    assert_eq!(project.version, 30);
    assert_eq!(project.settings.fps.numerator, 24);
    assert_eq!(project.settings.canvas_size.height, 1920);
    match &project.settings.background {
        cutix_project::Background::Blur { blur_intensity } => {
            assert!((blur_intensity - 20.0).abs() < 1e-9);
        }
        other => panic!("expected blur, got {other:?}"),
    }
    assert!(store.exists("imported-project"));

    let reloaded = store.load("imported-project").expect("reload").project;
    assert_eq!(reloaded, project);
}

#[test]
fn track_lookup_helpers_cover_every_variant() {
    let project = Project::new("Helpers", "2024-01-01T00:00:00.000Z".to_string());
    let scene = project.main_scene().expect("main scene");

    assert!(matches!(scene.tracks.main, Track::Video { .. }));
    assert_eq!(scene.tracks.all().count(), 1);
    assert_eq!(project.computed_duration().as_ticks(), 0);
}

fn fake_matte(seed: u8) -> String {
    let bytes: Vec<u8> = (0..8_192u32)
        .map(|index| ((index as u8).wrapping_mul(31)).wrapping_add(seed))
        .collect();
    cutix_project::encode_base64(&bytes)
}

fn cutout_with_inline_mattes(samples: usize) -> Value {
    let frames: Vec<Value> = (0..samples)
        .map(|index| {
            serde_json::json!({
                "sourceTime": (index as f64) * 60_000.0,
                "png": fake_matte(index as u8),
                "coverage": 0.5,
            })
        })
        .collect();
    serde_json::json!({
        "enabled": true,
        "mode": "perFrame",
        "width": 64,
        "height": 64,
        "png": fake_matte(200),
        "invert": false,
        "referenceTime": 0.0,
        "coverage": 0.5,
        "frames": frames,
        "sampleInterval": 60_000.0,
    })
}

fn project_with_inline_cutout(store: &ProjectStore, samples: usize) -> Project {
    let mut project = store.create("Cutout").expect("create");
    let mut element = video_element("element-1", 0.0, 4.0);
    if let TimelineElement::Video(video) = &mut element {
        video.cutout = Some(cutout_with_inline_mattes(samples));
    }
    project.scenes[0].tracks.main.elements_mut().push(element);
    project
}

#[test]
fn saving_moves_inline_mattes_into_files_and_shrinks_the_document() {
    let (_guard, store) = store();
    let project = project_with_inline_cutout(&store, 12);

    let inline_bytes = serde_json::to_vec_pretty(&project)
        .expect("serialise")
        .len();
    store.save(&project).expect("save");
    let on_disk = fs::read(store.project_file(&project.metadata.id)).expect("read");

    assert!(
        on_disk.len() * 8 < inline_bytes,
        "{} vs {inline_bytes}",
        on_disk.len()
    );
    let text = String::from_utf8(on_disk).expect("utf8");
    assert!(!text.contains("\"png\""), "base64 survived the save");
    assert!(text.contains("\"pngPath\""));

    let files: Vec<_> = fs::read_dir(store.matte_directory(&project.metadata.id))
        .expect("mattes dir")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(files.len(), 13, "{files:?}");

    let loaded = store.load(&project.metadata.id).expect("load").project;
    let referenced = cutix_project::matte_referenced_files(&loaded);
    assert_eq!(referenced.len(), 13);
    for relative in referenced {
        assert!(
            store
                .project_directory(&project.metadata.id)
                .join(&relative)
                .is_file(),
            "{relative} missing"
        );
    }
}

#[test]
fn identical_samples_share_one_matte_file() {
    let (_guard, store) = store();
    let mut project = store.create("Repeats").expect("create");
    let mut element = video_element("element-1", 0.0, 4.0);
    let repeated = fake_matte(7);
    if let TimelineElement::Video(video) = &mut element {
        video.cutout = Some(serde_json::json!({
            "enabled": true,
            "mode": "perFrame",
            "width": 64,
            "height": 64,
            "png": repeated,
            "invert": false,
            "referenceTime": 0.0,
            "coverage": 0.5,
            "frames": [
                { "sourceTime": 0.0, "png": repeated, "coverage": 0.5 },
                { "sourceTime": 60_000.0, "png": repeated, "coverage": 0.5 },
            ],
        }));
    }
    project.scenes[0].tracks.main.elements_mut().push(element);
    store.save(&project).expect("save");

    let files = fs::read_dir(store.matte_directory(&project.metadata.id))
        .expect("mattes dir")
        .count();
    assert_eq!(files, 1);
}

#[test]
fn externalizing_leaves_the_callers_document_alone_but_can_be_asked_for() {
    let (_guard, store) = store();
    let mut project = project_with_inline_cutout(&store, 3);
    store.save(&project).expect("save");

    let TimelineElement::Video(video) = &project.scenes[0].tracks.main.elements()[0] else {
        panic!("video");
    };
    assert!(video.cutout.as_ref().expect("cutout")["png"].is_string());

    let moved = store.externalize_mattes(&mut project).expect("externalize");
    assert_eq!(moved, 4);
    let TimelineElement::Video(video) = &project.scenes[0].tracks.main.elements()[0] else {
        panic!("video");
    };
    let cutout = video.cutout.as_ref().expect("cutout");
    assert!(cutout.get("png").is_none());
    assert!(
        cutout["pngPath"]
            .as_str()
            .expect("path")
            .starts_with("mattes/")
    );
    assert_eq!(store.externalize_mattes(&mut project).expect("again"), 0);
}

#[test]
fn deleting_a_cutout_sweeps_its_matte_files_on_the_next_save() {
    let (_guard, store) = store();
    let mut project = project_with_inline_cutout(&store, 4);
    store.save(&project).expect("save");
    let directory = store.matte_directory(&project.metadata.id);
    assert_eq!(fs::read_dir(&directory).expect("mattes").count(), 5);

    if let TimelineElement::Video(video) = &mut project.scenes[0].tracks.main.elements_mut()[0] {
        video.cutout = None;
    }
    store.save(&project).expect("save again");
    assert_eq!(fs::read_dir(&directory).expect("mattes").count(), 0);
}

#[test]
fn re_running_a_cutout_drops_the_previous_mattes() {
    let (_guard, store) = store();
    let mut project = project_with_inline_cutout(&store, 3);
    store.save(&project).expect("save");
    let directory = store.matte_directory(&project.metadata.id);
    assert_eq!(fs::read_dir(&directory).expect("mattes").count(), 4);

    if let TimelineElement::Video(video) = &mut project.scenes[0].tracks.main.elements_mut()[0] {
        video.cutout = Some(serde_json::json!({
            "enabled": true,
            "mode": "single",
            "width": 64,
            "height": 64,
            "png": fake_matte(250),
            "invert": false,
            "referenceTime": 0.0,
            "coverage": 0.5,
        }));
    }
    store.save(&project).expect("save again");
    let files: Vec<_> = fs::read_dir(&directory)
        .expect("mattes")
        .map(|entry| entry.expect("entry").file_name())
        .collect();
    assert_eq!(files.len(), 1, "{files:?}");
}

#[test]
fn a_duplicated_project_keeps_its_matte_files() {
    let (_guard, store) = store();
    let project = project_with_inline_cutout(&store, 2);
    store.save(&project).expect("save");

    let copy = store.duplicate(&project.metadata.id).expect("duplicate");
    let loaded = store.load(&copy.metadata.id).expect("load").project;
    let referenced = cutix_project::matte_referenced_files(&loaded);
    assert_eq!(referenced.len(), 3);
    for relative in referenced {
        assert!(
            store
                .project_directory(&copy.metadata.id)
                .join(&relative)
                .is_file(),
            "{relative} missing from the copy"
        );
    }
}

#[test]
fn a_composed_frame_is_written_as_a_readable_png_thumbnail() {
    let directory = tempfile::tempdir().expect("tempdir");
    let store = ProjectStore::new(directory.path());
    let project = store.create("thumbnail").expect("create");

    let (width, height) = (1920u32, 1080u32);
    let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
    for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
        pixel[0] = (index % 256) as u8;
        pixel[1] = 64;
        pixel[2] = 200;
        pixel[3] = 255;
    }

    let path = store
        .save_thumbnail_from_rgba(&project.metadata.id, width, height, &rgba)
        .expect("thumbnail");
    assert_eq!(path, store.thumbnail_file(&project.metadata.id));

    let decoded = image::open(&path).expect("read back").to_rgba8();
    assert_eq!(decoded.width(), 480, "the long edge is capped at 480");
    assert_eq!(decoded.height(), 270, "the aspect ratio is preserved");
    let centre = decoded.get_pixel(240, 135);
    assert!(
        centre[1].abs_diff(64) <= 2 && centre[2].abs_diff(200) <= 2,
        "the picture survived the rescale, got {centre:?}"
    );
}

#[test]
fn a_thumbnail_smaller_than_the_cap_is_not_upscaled() {
    let directory = tempfile::tempdir().expect("tempdir");
    let store = ProjectStore::new(directory.path());
    let project = store.create("small").expect("create");

    let rgba = vec![255u8; 32 * 18 * 4];
    let path = store
        .save_thumbnail_from_rgba(&project.metadata.id, 32, 18, &rgba)
        .expect("thumbnail");
    let decoded = image::open(&path).expect("read back").to_rgba8();
    assert_eq!((decoded.width(), decoded.height()), (32, 18));
}

#[test]
fn a_frame_whose_buffer_is_short_is_refused_rather_than_written() {
    let directory = tempfile::tempdir().expect("tempdir");
    let store = ProjectStore::new(directory.path());
    let project = store.create("short").expect("create");

    assert!(
        store
            .save_thumbnail_from_rgba(&project.metadata.id, 64, 64, &[0u8; 16])
            .is_err()
    );
    assert!(!store.thumbnail_file(&project.metadata.id).is_file());
}

#[test]
fn a_summary_sidecar_is_written_beside_the_project_and_answers_the_listing() {
    let (_guard, store) = store();
    let mut project = Project::new("Sidecar", "2024-01-01T00:00:00.000Z".to_string());
    project.metadata.duration = MediaTime::from_seconds_f64(5.0).expect("duration");
    store.save(&project).expect("save");

    let sidecar = store.summary_file(&project.metadata.id);
    assert!(sidecar.is_file(), "saving writes the sidecar");

    let summary = store.summary(&project.metadata.id).expect("summary");
    assert_eq!(summary, project.summary());
    assert_eq!(store.list_projects().expect("list"), vec![summary]);
}

#[test]
fn a_missing_sidecar_is_rebuilt_from_the_document() {
    let (_guard, store) = store();
    let project = Project::new("Rebuild", "2024-01-01T00:00:00.000Z".to_string());
    store.save(&project).expect("save");

    fs::remove_file(store.summary_file(&project.metadata.id)).expect("drop sidecar");
    assert_eq!(
        store.summary(&project.metadata.id).expect("summary"),
        project.summary()
    );
    assert!(
        store.summary_file(&project.metadata.id).is_file(),
        "the fallback repairs the sidecar"
    );
}

#[test]
fn a_document_changed_underneath_the_sidecar_is_not_reported_from_it() {
    let (_guard, store) = store();
    let project = Project::new("Stale", "2024-01-01T00:00:00.000Z".to_string());
    store.save(&project).expect("save");
    assert_eq!(
        store.summary(&project.metadata.id).expect("summary").name,
        "Stale"
    );

    let path = store.project_file(&project.metadata.id);
    let mut document: Value =
        serde_json::from_slice(&fs::read(&path).expect("read")).expect("json");
    document["metadata"]["name"] = Value::String("Renamed on disk".to_string());
    document["metadata"]["updatedAt"] = Value::String("2025-05-05T00:00:00.000Z".to_string());
    write_atomic(&path, &serde_json::to_vec(&document).expect("serialise")).expect("write");

    let summary = store.summary(&project.metadata.id).expect("summary");
    assert_eq!(summary.name, "Renamed on disk");
    assert_eq!(summary.updated_at, "2025-05-05T00:00:00.000Z");
}

#[test]
fn listing_a_legacy_document_never_shortcuts_migrating_it_when_it_is_opened() {
    let (_guard, store) = store();

    let legacy = serde_json::json!({
        "id": "legacy-project",
        "name": "From an older build",
        "createdAt": "2023-05-05T00:00:00.000Z",
        "updatedAt": "2023-05-05T00:00:00.000Z",
        "fps": 24,
        "canvasSize": { "width": 1080, "height": 1920 }
    });
    let path = store.project_file("legacy-project");
    write_atomic(&path, &serde_json::to_vec(&legacy).expect("serialise")).expect("write");

    let summary = store.summary("legacy-project").expect("summary");
    assert_eq!(summary.name, "From an older build");
    assert!(store.summary_file("legacy-project").is_file());

    let untouched: Value = serde_json::from_slice(&fs::read(&path).expect("read")).expect("json");
    assert_eq!(untouched, legacy, "listing rewrote the document");

    let loaded = store.load("legacy-project").expect("load");
    assert_eq!(
        loaded.project.version,
        cutix_project::CURRENT_PROJECT_VERSION
    );
    assert_eq!(loaded.project.settings.fps.numerator, 24);
    assert_eq!(loaded.project.settings.canvas_size.height, 1920);
}

#[test]
fn a_project_without_cutouts_is_written_without_copying_the_document() {
    let (_guard, store) = store();
    let mut project = Project::new("No cutouts", "2024-01-01T00:00:00.000Z".to_string());
    project.scenes[0]
        .tracks
        .main
        .elements_mut()
        .push(video_element("element-1", 0.0, 3.0));
    let before = project.clone();
    store.save(&project).expect("save");

    assert_eq!(project, before, "the caller's document is untouched");
    assert_eq!(
        store.load(&project.metadata.id).expect("load").project,
        project
    );
}
