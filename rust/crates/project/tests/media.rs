use cutix_project::model::MediaType;
use cutix_project::{probe, MediaStore, ProjectStore};

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
    assert!(!media.source_file(&video).exists());
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
