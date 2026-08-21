use cutix_project::model::MediaType;
use cutix_project::{MediaStore, ProjectStore, probe};

#[test]
fn probes_video_clips() {
    let square = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);
    let first = probe(&square).expect("probe the square clip");
    assert_eq!(first.media_type, MediaType::Video);
    assert_eq!(first.width, Some(fixtures::SQUARE_CLIP_WIDTH));
    assert_eq!(first.height, Some(fixtures::SQUARE_CLIP_HEIGHT));
    assert!(
        (first.duration.expect("duration") - fixtures::CLIP_SECONDS).abs() < 0.01,
        "unexpected duration {:?}",
        first.duration
    );
    assert!(
        (first.fps.expect("fps") - fixtures::SQUARE_CLIP_FPS).abs() < 0.05,
        "unexpected fps {:?}",
        first.fps
    );
    assert_eq!(
        first.has_audio,
        Some(false),
        "the openh264-mp4 backend writes audio as a WAV sidecar, never as a track"
    );

    let hd = fixtures::fixture_or_skip!(fixtures::HD_CLIP);
    let second = probe(&hd).expect("probe the 1080p clip");
    assert_eq!(second.media_type, MediaType::Video);
    assert_eq!(second.width, Some(fixtures::HD_CLIP_WIDTH));
    assert_eq!(second.height, Some(fixtures::HD_CLIP_HEIGHT));
    assert!(
        (second.duration.expect("duration") - fixtures::CLIP_SECONDS).abs() < 0.01,
        "unexpected duration {:?}",
        second.duration
    );
    assert!(
        (second.fps.expect("fps") - fixtures::HD_CLIP_FPS).abs() < 0.05,
        "unexpected fps {:?}",
        second.fps
    );
    assert_eq!(second.has_audio, Some(false));

    assert!(
        (second.fps.expect("fps") * second.duration.expect("duration")).round() as u32
            == fixtures::HD_CLIP_FRAMES,
        "fps x duration must recover the frame count the encoder wrote"
    );
}

#[test]
fn probes_audio_file() {
    let path = fixtures::fixture_or_skip!(fixtures::AUDIO_CLIP);
    let audio = probe(&path).expect("probe the audio clip");

    assert_eq!(audio.media_type, MediaType::Audio);
    assert_eq!(audio.width, None);
    assert_eq!(audio.height, None);
    assert_eq!(audio.fps, None);
    assert_eq!(audio.has_audio, Some(true));
    let duration = audio.duration.expect("duration");
    assert!(
        (duration - fixtures::AUDIO_CLIP_END).abs() < 0.1,
        "frame-summed duration {duration} should match the {}s extracted",
        fixtures::AUDIO_CLIP_END
    );
}

#[test]
fn imports_media_into_the_project_library() {
    let clip = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);
    let sound = fixtures::fixture_or_skip!(fixtures::AUDIO_CLIP);
    let directory = tempfile::tempdir().expect("temp dir");
    let store = ProjectStore::new(directory.path().join("cutix"));
    let project = store.create("Import test").expect("create");
    let media = MediaStore::for_project(&store, &project.metadata.id);

    let video = media.import(&clip).expect("import video");
    assert_eq!(video.media_type, MediaType::Video);
    assert_eq!(video.name, fixtures::SQUARE_CLIP);
    assert_eq!(video.width, Some(fixtures::SQUARE_CLIP_WIDTH));
    assert!(video.size > 0);
    assert!(media.source_file(&video).is_file());
    assert_eq!(
        video.thumbnail_url.as_deref(),
        Some(format!("thumbnails/{}.png", video.id).as_str())
    );
    let thumbnail = media.thumbnail_file(&video.id);
    assert!(thumbnail.is_file());
    let dimensions = image::image_dimensions(&thumbnail).expect("thumbnail dimensions");
    assert!(dimensions.0 <= 320 && dimensions.1 <= 320);
    assert_eq!(
        dimensions.0, dimensions.1,
        "a square source must give a square thumbnail"
    );
    assert!(media.metadata_file(&video.id).is_file());

    let audio = media.import(&sound).expect("import audio");
    assert_eq!(audio.media_type, MediaType::Audio);
    assert!(audio.duration.is_some());
    assert!(
        audio.thumbnail_url.is_none(),
        "audio should not get a thumbnail"
    );

    let listed = media.list().expect("list");
    assert_eq!(listed.len(), 2);
    assert_eq!(media.get(&video.id).expect("get"), video);

    media.remove(&video.id).expect("remove");
    assert_eq!(media.list().expect("list after remove").len(), 1);
    assert!(
        clip.is_file(),
        "removing an asset must never delete the file it was imported from"
    );
}

#[test]
fn duplicating_a_project_copies_its_media_library() {
    let sound = fixtures::fixture_or_skip!(fixtures::AUDIO_CLIP);
    let directory = tempfile::tempdir().expect("temp dir");
    let store = ProjectStore::new(directory.path().join("cutix"));
    let project = store.create("Original").expect("create");
    let media = MediaStore::for_project(&store, &project.metadata.id);
    let asset = media.import(&sound).expect("import");

    let copy = store.duplicate(&project.metadata.id).expect("duplicate");
    let copied_media = MediaStore::for_project(&store, &copy.metadata.id);

    let copied = copied_media.get(&asset.id).expect("copied asset");
    assert_eq!(copied, asset);
    assert!(copied_media.source_file(&copied).is_file());
}

fn still_image(directory: &std::path::Path, name: &str) -> std::path::PathBuf {
    let path = directory.join(name);
    let mut canvas = image::RgbaImage::new(8, 8);
    for (x, y, pixel) in canvas.enumerate_pixels_mut() {
        *pixel = image::Rgba([x as u8 * 8, y as u8 * 8, 128, 255]);
    }
    canvas.save(&path).expect("write png");
    path
}

#[test]
fn importing_links_the_original_instead_of_copying_it() {
    let directory = tempfile::tempdir().expect("temp dir");
    let outside = still_image(directory.path(), "outside.png");
    let store = ProjectStore::new(directory.path().join("cutix"));
    let project = store.create("Link test").expect("create");
    let media = MediaStore::for_project(&store, &project.metadata.id);

    let asset = media.import(&outside).expect("import");

    assert_eq!(
        asset.source_path.as_deref().map(std::path::Path::new),
        Some(
            outside
                .canonicalize()
                .unwrap_or_else(|_| outside.clone())
                .as_path()
        ),
        "the asset must remember where the file actually lives"
    );
    assert_eq!(
        media.source_file(&asset),
        outside.canonicalize().unwrap_or(outside.clone())
    );
    assert!(
        !media.local_file(&asset).exists(),
        "nothing may be copied into the project"
    );
    assert!(outside.is_file(), "the original must be left alone");
}

#[test]
fn a_link_that_went_missing_falls_back_to_a_copy_inside_the_project() {
    let directory = tempfile::tempdir().expect("temp dir");
    let outside = still_image(directory.path(), "moved.png");
    let store = ProjectStore::new(directory.path().join("cutix"));
    let project = store.create("Fallback test").expect("create");
    let media = MediaStore::for_project(&store, &project.metadata.id);

    let asset = media.import(&outside).expect("import");
    let inside = media.local_file(&asset);
    std::fs::create_dir_all(inside.parent().expect("parent")).expect("files dir");
    std::fs::copy(&outside, &inside).expect("stage a copy");
    std::fs::remove_file(&outside).expect("move the original away");

    assert_eq!(
        media.source_file(&asset),
        inside,
        "a project that carries its own copy must still open when the link breaks"
    );
}

#[test]
fn an_old_project_without_a_link_still_resolves_inside_itself() {
    let directory = tempfile::tempdir().expect("temp dir");
    let store = ProjectStore::new(directory.path().join("cutix"));
    let project = store.create("Old project").expect("create");
    let media = MediaStore::for_project(&store, &project.metadata.id);

    let asset = cutix_project::model::MediaAssetData {
        id: "abc".to_string(),
        name: "clip.mp4".to_string(),
        media_type: MediaType::Video,
        size: 1,
        last_modified: 0,
        width: None,
        height: None,
        duration: None,
        fps: None,
        has_audio: None,
        ephemeral: false,
        thumbnail_url: None,
        file_name: Some("abc.mp4".to_string()),
        source_path: None,
    };

    assert_eq!(
        media.source_file(&asset),
        media.local_file(&asset),
        "assets written before linking existed must keep resolving inside the project"
    );
}

#[test]
fn deleting_a_project_leaves_the_files_it_linked_to_alone() {
    let directory = tempfile::tempdir().expect("temp dir");
    let outside = still_image(directory.path(), "keepme.png");
    let store = ProjectStore::new(directory.path().join("cutix"));
    let project = store.create("Throwaway").expect("create");
    let media = MediaStore::for_project(&store, &project.metadata.id);
    let asset = media.import(&outside).expect("import");

    assert_eq!(
        media.source_file(&asset),
        outside.canonicalize().unwrap_or(outside.clone()),
        "the asset must be linked, not copied, for this test to mean anything"
    );

    store
        .delete(&project.metadata.id)
        .expect("delete the project");

    assert!(
        !store.project_directory(&project.metadata.id).exists(),
        "the project folder should be gone"
    );
    assert!(
        outside.is_file(),
        "deleting a project must never touch the video it pointed at"
    );
}

#[test]
fn removing_one_asset_leaves_the_file_it_linked_to_alone() {
    let directory = tempfile::tempdir().expect("temp dir");
    let outside = still_image(directory.path(), "linked.png");
    let store = ProjectStore::new(directory.path().join("cutix"));
    let project = store.create("Keeper").expect("create");
    let media = MediaStore::for_project(&store, &project.metadata.id);
    let asset = media.import(&outside).expect("import");

    media.remove(&asset.id).expect("remove the asset");

    assert!(media.list().expect("list").is_empty());
    assert!(
        outside.is_file(),
        "removing an asset must never delete the file it was linked to"
    );
}
