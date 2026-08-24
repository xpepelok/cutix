use std::path::{Path, PathBuf};
use std::sync::Arc;

use cutix_playback::audio_decode::AudioCache;
use cutix_playback::decode_cache::DecodeCache;
use cutix_playback::render::{ComposeRequest, FrameComposer};
use cutix_playback::{MediaMap, PlaybackController};
use cutix_playback::{MixRequest, mix};
use cutix_project::Project;
use cutix_project::model::{TimelineElement, Track};
use serde_json::json;
use time::{MediaTime, TICKS_PER_SECOND};

fn seconds(value: f64) -> MediaTime {
    MediaTime::from_seconds_f64(value).unwrap()
}

fn video_element(media_id: &str, start: f64, duration: f64) -> TimelineElement {
    serde_json::from_value(json!({
        "type": "video",
        "id": format!("element-{media_id}-{start}"),
        "name": media_id,
        "duration": seconds(duration).as_ticks(),
        "startTime": seconds(start).as_ticks(),
        "trimStart": 0,
        "trimEnd": 0,
        "mediaId": media_id,
        "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
        "opacity": 1.0
    }))
    .unwrap()
}

fn audio_element(
    id: &str,
    media_id: &str,
    start: f64,
    duration: f64,
    volume_db: f64,
) -> TimelineElement {
    serde_json::from_value(json!({
        "type": "audio",
        "id": id,
        "name": media_id,
        "duration": seconds(duration).as_ticks(),
        "startTime": seconds(start).as_ticks(),
        "trimStart": 0,
        "trimEnd": 0,
        "sourceType": "media",
        "mediaId": media_id,
        "volume": volume_db
    }))
    .unwrap()
}

fn project_with(elements: Vec<TimelineElement>) -> Project {
    let mut project = Project::new("test", "1970-01-01T00:00:00.000Z".into());
    let scene = project.scenes.first_mut().unwrap();
    for element in elements {
        let is_audio = matches!(element, TimelineElement::Audio(_));
        if is_audio {
            if scene.tracks.audio.is_empty() {
                scene.tracks.audio.push(Track::Audio {
                    id: "audio-track".into(),
                    name: "Audio".into(),
                    elements: Vec::new(),
                    muted: false,
                });
            }
            scene.tracks.audio[0].elements_mut().push(element);
        } else {
            scene.tracks.main.elements_mut().push(element);
        }
    }
    project
}

fn write_wav(path: &Path, sample_rate: u32, samples: &[i16]) {
    let data_len = (samples.len() * 2) as u32;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        bytes.extend_from_slice(&sample.to_le_bytes());
    }
    std::fs::write(path, bytes).unwrap();
}

fn image_element(media_id: &str, patch: serde_json::Value) -> TimelineElement {
    let mut value = json!({
        "type": "image",
        "id": format!("image-{media_id}"),
        "name": media_id,
        "duration": seconds(2.0).as_ticks(),
        "startTime": 0,
        "trimStart": 0,
        "trimEnd": 0,
        "mediaId": media_id,
        "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
        "opacity": 1.0
    });
    let object = value.as_object_mut().unwrap();
    for (key, entry) in patch.as_object().unwrap() {
        object.insert(key.clone(), entry.clone());
    }
    serde_json::from_value(value).unwrap()
}

fn quadrant_png() -> PathBuf {
    let directory = std::env::temp_dir().join("cutix-playback-tests");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("quadrants.png");
    let mut buffer = image::RgbaImage::new(2, 2);
    buffer.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    buffer.put_pixel(1, 0, image::Rgba([0, 255, 0, 255]));
    buffer.put_pixel(0, 1, image::Rgba([0, 0, 255, 255]));
    buffer.put_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
    buffer.save(&path).unwrap();
    path
}

#[test]
fn composes_exact_pixels_for_a_known_layer() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let media = MediaMap::new().with("quadrants", quadrant_png());

    let project = project_with(vec![image_element("quadrants", json!({}))]);
    let frame = composer
        .compose(
            &ComposeRequest {
                project: &project,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 64,
                height: 64,
            },
            &media,
        )
        .unwrap();
    assert_eq!(frame.pixel(1, 1), [255, 0, 0, 255]);
    assert_eq!(frame.pixel(62, 1), [0, 255, 0, 255]);
    assert_eq!(frame.pixel(1, 62), [0, 0, 255, 255]);
    assert_eq!(frame.pixel(62, 62), [255, 255, 255, 255]);

    let scaled = project_with(vec![image_element(
        "quadrants",
        json!({ "transform": { "scaleX": 0.5, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 } }),
    )]);
    let frame = composer
        .compose(
            &ComposeRequest {
                project: &scaled,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 64,
                height: 64,
            },
            &media,
        )
        .unwrap();
    assert_eq!(frame.pixel(8, 8), [0, 0, 0, 255]);
    assert_eq!(frame.pixel(18, 8), [255, 0, 0, 255]);
    assert_eq!(frame.pixel(45, 8), [0, 255, 0, 255]);
    assert_eq!(frame.pixel(55, 8), [0, 0, 0, 255]);

    let cropped = project_with(vec![image_element(
        "quadrants",
        json!({ "crop": { "left": 0.5, "top": 0.0, "right": 0.0, "bottom": 0.0 } }),
    )]);
    let frame = composer
        .compose(
            &ComposeRequest {
                project: &cropped,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 64,
                height: 64,
            },
            &media,
        )
        .unwrap();
    assert_eq!(frame.pixel(10, 10), [0, 0, 0, 255]);
    assert_eq!(frame.pixel(60, 4), [0, 255, 0, 255]);
    assert_eq!(frame.pixel(60, 60), [255, 255, 255, 255]);

    assert_eq!(frame.pixel(40, 4), [60, 195, 0, 255]);

    let faded = project_with(vec![image_element("quadrants", json!({ "opacity": 0.4 }))]);
    let frame = composer
        .compose(
            &ComposeRequest {
                project: &faded,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 64,
                height: 64,
            },
            &media,
        )
        .unwrap();
    let red = frame.pixel(1, 1);
    eprintln!("40% red = {red:?}");
    assert_eq!(red[3], 255);
    assert!((red[0] as i32 - 102).abs() <= 1, "{red:?}");
    assert_eq!(red[1], 0);
    assert_eq!(red[2], 0);
}

#[test]
fn composes_a_known_project_frame() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let clip = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);
    let project = project_with(vec![video_element("clip", 0.0, 2.0)]);
    let media = MediaMap::new().with("clip", &clip);

    let frame = composer
        .compose(
            &ComposeRequest {
                project: &project,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 320,
                height: 180,
            },
            &media,
        )
        .unwrap();

    assert_eq!(frame.width, 320);
    assert_eq!(frame.height, 180);
    assert_eq!(frame.pixels.len(), 320 * 180 * 4);

    assert_eq!(frame.pixel(69, 90), [0, 0, 0, 255]);
    assert_eq!(frame.pixel(250, 90), [0, 0, 0, 255]);
    assert_ne!(frame.pixel(160, 90), [0, 0, 0, 255]);

    let mut cache = DecodeCache::new();
    let source = cache.video_frame("clip", &clip, 0.0).unwrap();
    assert_eq!(
        (source.width as u32, source.height as u32),
        (fixtures::SQUARE_CLIP_WIDTH, fixtures::SQUARE_CLIP_HEIGHT)
    );
    let source_center = {
        let offset = ((200 * source.width + 200) * 4) as usize;
        [
            source.rgba[offset],
            source.rgba[offset + 1],
            source.rgba[offset + 2],
        ]
    };
    let composed_center = frame.pixel(160, 90);
    eprintln!("source centre {source_center:?}, composed centre {composed_center:?}");
    for channel in 0..3 {
        assert!(
            (composed_center[channel] as i32 - source_center[channel] as i32).abs() <= 3,
            "centre pixel drifted: {composed_center:?} vs {source_center:?}"
        );
    }
    for pixel in frame.pixels.as_chunks::<4>().0 {
        assert_eq!(pixel[3], 255);
    }

    let mut half = project_with(vec![video_element("clip", 0.0, 2.0)]);
    if let TimelineElement::Video(element) = &mut half.scenes[0].tracks.main.elements_mut()[0] {
        element.opacity = 0.5;
    }
    let dimmed = composer
        .compose(
            &ComposeRequest {
                project: &half,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 320,
                height: 180,
            },
            &media,
        )
        .unwrap();

    let full = frame.pixel(160, 90);
    let dim = dimmed.pixel(160, 90);
    for channel in 0..3 {
        let expected = (full[channel] as f64 * 0.5).round() as i32;
        assert!(
            (dim[channel] as i32 - expected).abs() <= 2,
            "channel {channel}: full {full:?} dimmed {dim:?}"
        );
    }
}

fn masked_image(shape: &str, params: serde_json::Value) -> TimelineElement {
    let mut mask = json!({
        "id": "mask-1",
        "type": shape,
        "params": {
            "centerX": 0.0,
            "centerY": 0.0,
            "width": 0.5,
            "height": 0.5,
            "rotation": 0.0,
            "feather": 0.0,
            "inverted": false
        }
    });
    for (key, value) in params.as_object().unwrap() {
        mask["params"][key] = value.clone();
    }
    image_element("quadrants", json!({ "masks": [mask] }))
}

fn masked_coverage(frame: &cutix_playback::ComposedFrame, width: u32, height: u32) -> f64 {
    let mut kept = 0.0;
    for y in 0..height {
        for x in 0..width {
            let pixel = frame.pixel(x, y);
            if pixel[0] > 8 || pixel[1] > 8 || pixel[2] > 8 {
                kept += 1.0;
            }
        }
    }
    kept / (width as f64 * height as f64)
}

#[test]
fn a_rectangle_mask_keeps_only_the_pixels_inside_it() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let media = MediaMap::new().with("quadrants", quadrant_png());

    let unmasked = project_with(vec![image_element("quadrants", json!({}))]);
    let before = composer
        .compose(
            &ComposeRequest {
                project: &unmasked,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 64,
                height: 64,
            },
            &media,
        )
        .unwrap();
    assert!(masked_coverage(&before, 64, 64) > 0.99);

    let masked = project_with(vec![masked_image("rectangle", json!({}))]);
    let after = composer
        .compose(
            &ComposeRequest {
                project: &masked,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 64,
                height: 64,
            },
            &media,
        )
        .unwrap();

    let coverage = masked_coverage(&after, 64, 64);
    assert!((coverage - 0.25).abs() < 0.03, "{coverage}");
    assert_ne!(after.pixel(32, 32)[..3], [0, 0, 0], "centre survives");
    assert_eq!(after.pixel(2, 2), [0, 0, 0, 255], "corner is cut away");
}

#[test]
fn inverting_a_mask_swaps_which_pixels_survive() {
    let Ok(mut composer) = FrameComposer::new() else {
        return;
    };
    let media = MediaMap::new().with("quadrants", quadrant_png());
    let project = project_with(vec![masked_image("rectangle", json!({ "inverted": true }))]);
    let frame = composer
        .compose(
            &ComposeRequest {
                project: &project,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 64,
                height: 64,
            },
            &media,
        )
        .unwrap();

    let coverage = masked_coverage(&frame, 64, 64);
    assert!((coverage - 0.75).abs() < 0.03, "{coverage}");
    assert_eq!(frame.pixel(32, 32), [0, 0, 0, 255], "centre is cut away");
    assert_ne!(frame.pixel(2, 2)[..3], [0, 0, 0], "corner survives");
}

#[test]
fn an_inverted_mask_keeps_the_complement_of_the_plain_one() {
    let Ok(mut composer) = FrameComposer::new() else {
        return;
    };
    let media = MediaMap::new().with("quadrants", quadrant_png());

    let mut frames = Vec::new();
    for inverted in [false, true] {
        let project = project_with(vec![masked_image(
            "ellipse",
            json!({ "inverted": inverted }),
        )]);
        frames.push(
            composer
                .compose(
                    &ComposeRequest {
                        project: &project,
                        scene_id: None,
                        time: MediaTime::ZERO,
                        width: 64,
                        height: 64,
                    },
                    &media,
                )
                .unwrap(),
        );
    }

    let mut both = 0;
    let mut neither = 0;
    for y in 0..64 {
        for x in 0..64 {
            let plain = frames[0].pixel(x, y);
            let inverted = frames[1].pixel(x, y);
            let plain_lit = plain[0] > 8 || plain[1] > 8 || plain[2] > 8;
            let inverted_lit = inverted[0] > 8 || inverted[1] > 8 || inverted[2] > 8;
            if plain_lit && inverted_lit {
                both += 1;
            }
            if !plain_lit && !inverted_lit {
                neither += 1;
            }
        }
    }

    assert!(both < 160, "{both} pixels survive both masks");
    assert!(neither < 160, "{neither} pixels survive neither mask");

    let plain = masked_coverage(&frames[0], 64, 64);
    let inverted = masked_coverage(&frames[1], 64, 64);
    assert!(
        (plain + inverted - 1.0).abs() < 0.06,
        "{plain} + {inverted}"
    );
}

#[test]
fn an_ellipse_mask_keeps_less_than_the_rectangle_it_fits_in() {
    let Ok(mut composer) = FrameComposer::new() else {
        return;
    };
    let media = MediaMap::new().with("quadrants", quadrant_png());

    let mut coverages = Vec::new();
    for shape in ["rectangle", "ellipse", "diamond"] {
        let project = project_with(vec![masked_image(shape, json!({}))]);
        let frame = composer
            .compose(
                &ComposeRequest {
                    project: &project,
                    scene_id: None,
                    time: MediaTime::ZERO,
                    width: 64,
                    height: 64,
                },
                &media,
            )
            .unwrap();
        coverages.push(masked_coverage(&frame, 64, 64));
    }

    assert!(coverages[0] > coverages[1], "{coverages:?}");
    assert!(coverages[1] > coverages[2], "{coverages:?}");
    assert!(
        (coverages[1] / coverages[0] - std::f64::consts::PI / 4.0).abs() < 0.06,
        "{coverages:?}"
    );
    assert!(
        (coverages[2] / coverages[0] - 0.5).abs() < 0.06,
        "{coverages:?}"
    );
}

#[test]
fn feathering_a_mask_produces_partially_lit_edge_pixels() {
    let Ok(mut composer) = FrameComposer::new() else {
        return;
    };
    let media = MediaMap::new().with("quadrants", quadrant_png());

    let mut partials = Vec::new();
    for feather in [0.0, 12.0] {
        let project = project_with(vec![masked_image("ellipse", json!({ "feather": feather }))]);
        let frame = composer
            .compose(
                &ComposeRequest {
                    project: &project,
                    scene_id: None,
                    time: MediaTime::ZERO,
                    width: 64,
                    height: 64,
                },
                &media,
            )
            .unwrap();
        let mut count = 0;
        for y in 0..64 {
            for x in 0..64 {
                let pixel = frame.pixel(x, y);
                let peak = pixel[0].max(pixel[1]).max(pixel[2]);
                if peak > 16 && peak < 200 {
                    count += 1;
                }
            }
        }
        partials.push(count);
    }

    assert!(
        partials[1] as f64 > partials[0] as f64 * 1.5,
        "feathered {} vs hard {}",
        partials[1],
        partials[0]
    );
}

#[test]
fn hidden_layer_leaves_the_background_colour() {
    let Ok(mut composer) = FrameComposer::new() else {
        return;
    };
    let clip = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);
    let mut project = project_with(vec![video_element("clip", 0.0, 2.0)]);
    if let TimelineElement::Video(element) = &mut project.scenes[0].tracks.main.elements_mut()[0] {
        element.hidden = Some(true);
    }
    let media = MediaMap::new().with("clip", &clip);
    let frame = composer
        .compose(
            &ComposeRequest {
                project: &project,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 64,
                height: 36,
            },
            &media,
        )
        .unwrap();
    assert_eq!(frame.pixel(32, 18), [0, 0, 0, 255]);
}

#[test]
fn sequential_playback_does_not_reseek() {
    let path = fixtures::fixture_or_skip!(fixtures::HD_CLIP);
    let mut cache = DecodeCache::new();
    let started = std::time::Instant::now();
    let frames = 60;
    for index in 0..frames {
        let seconds = index as f64 / fixtures::HD_CLIP_FPS;
        let frame = cache.video_frame("clip2", &path, seconds).unwrap();
        assert_eq!(
            (frame.width as u32, frame.height as u32),
            (fixtures::HD_CLIP_WIDTH, fixtures::HD_CLIP_HEIGHT)
        );
    }
    let elapsed = started.elapsed();
    eprintln!(
        "sequential: {frames} frames of 1920x1080 in {:?} ({:.1} fps), stats {:?}",
        elapsed,
        frames as f64 / elapsed.as_secs_f64(),
        cache.stats()
    );
    assert_eq!(
        cache.seek_count("clip2"),
        1,
        "only the initial positioning may seek"
    );
}

fn fan_out(source: &Path, count: usize, name: &str) -> Vec<PathBuf> {
    let directory = std::env::temp_dir().join(format!("cutix-playback-{name}"));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    (0..count)
        .map(|index| {
            let target = directory.join(format!("media-{index}.mp4"));
            std::fs::copy(source, &target).unwrap();
            target
        })
        .collect()
}

fn small_budget(decoders: usize) -> cutix_playback::MemoryBudget {
    let mut budget = cutix_playback::MemoryBudget::from_total_bytes(64 * 1024 * 1024);
    budget.decoders = decoders * cutix_playback::DECODER_FOOTPRINT;
    budget
}

#[test]
fn the_decode_cache_stops_growing_at_its_ceiling() {
    let clip = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);
    let paths = fan_out(&clip, 20, "decode-ceiling");
    let mut cache = DecodeCache::with_budget(small_budget(8));
    for (index, path) in paths.iter().enumerate() {
        cache
            .video_frame(&format!("media-{index}"), path, 0.0)
            .unwrap();
        assert!(
            cache.live_decoders() <= 8,
            "{} decoders after {index}",
            cache.live_decoders()
        );
    }
    assert_eq!(cache.live_decoders(), 8);
    assert!(cache.stats().evictions >= 12);
}

#[test]
fn an_evicted_decoder_reopens_with_the_same_pixels() {
    let clip = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);
    let paths = fan_out(&clip, 12, "decode-reopen");
    let mut cache = DecodeCache::with_budget(small_budget(6));
    let first = cache.video_frame("media-0", &paths[0], 0.2).unwrap();
    for (index, path) in paths.iter().enumerate().skip(1) {
        cache
            .video_frame(&format!("media-{index}"), path, 0.0)
            .unwrap();
    }
    assert_eq!(
        cache.seek_count("media-0"),
        0,
        "media-0 must have been evicted"
    );
    let again = cache.video_frame("media-0", &paths[0], 0.2).unwrap();
    assert_eq!((first.width, first.height), (again.width, again.height));
    assert_eq!(
        first.rgba, again.rgba,
        "a reopened decoder must decode the same frame"
    );
    assert_eq!(cache.stats().failures, 0);
}

#[test]
fn eviction_never_costs_the_playing_clip_a_reseek() {
    let clip = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);
    let paths = fan_out(&clip, 12, "decode-hot");
    let mut cache = DecodeCache::with_budget(small_budget(6));
    for index in 0..40 {
        cache.begin_frame();
        cache
            .video_frame("hot", &paths[0], index as f64 / fixtures::SQUARE_CLIP_FPS)
            .unwrap();
        for offset in 0..3 {
            let cold = 1 + (index * 3 + offset) % 11;
            cache
                .video_frame(&format!("cold-{cold}"), &paths[cold], 0.0)
                .unwrap();
        }
    }
    eprintln!("stats under pressure: {:?}", cache.stats());
    assert!(cache.stats().evictions > 0, "the ceiling never engaged");
    assert_eq!(
        cache.seek_count("hot"),
        1,
        "the clip being played sequentially must keep its decoder"
    );
}

#[test]
fn layer_textures_do_not_accumulate_across_a_seeking_session() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let media = MediaMap::new().with("quadrants", quadrant_png());
    let mut elements = Vec::new();
    for index in 0..40 {
        let mut element = image_element("quadrants", json!({}));
        if let TimelineElement::Image(image) = &mut element {
            image.base.id = format!("image-{index}");
            image.base.start_time = seconds(index as f64 * 2.0);
        }
        elements.push(element);
    }
    let project = project_with(elements);
    for index in 0..40 {
        composer
            .compose(
                &ComposeRequest {
                    project: &project,
                    scene_id: None,
                    time: seconds(index as f64 * 2.0 + 0.5),
                    width: 64,
                    height: 64,
                },
                &media,
            )
            .unwrap();
    }
    let bytes = composer.cache_bytes();
    eprintln!("cache bytes after 40 seeks: {bytes:?}");
    assert!(
        bytes.layer_texture_entries <= 4,
        "{} layer textures retained",
        bytes.layer_texture_entries
    );
    assert!(bytes.layer_textures <= composer.budget().layer_textures);
}

#[test]
fn measures_sequential_composition_throughput() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let path = fixtures::fixture_or_skip!(fixtures::HD_CLIP);
    let project = project_with(vec![video_element("clip2", 0.0, fixtures::CLIP_SECONDS)]);
    let media = MediaMap::new().with("clip2", &path);
    let frames = 60;
    let started = std::time::Instant::now();
    for index in 0..frames {
        let time = seconds(index as f64 / fixtures::HD_CLIP_FPS);
        composer
            .compose(
                &ComposeRequest {
                    project: &project,
                    scene_id: None,
                    time,
                    width: 1920,
                    height: 1080,
                },
                &media,
            )
            .unwrap();
    }
    let elapsed = started.elapsed();
    eprintln!(
        "compose+readback: {frames} frames at 1920x1080 in {:?} ({:.1} fps), stats {:?}",
        elapsed,
        frames as f64 / elapsed.as_secs_f64(),
        composer.stats()
    );
    assert_eq!(composer.seek_count("clip2"), 1);
}

#[test]
fn backwards_seek_reseeks() {
    let path = fixtures::fixture_or_skip!(fixtures::HD_CLIP);
    let mut cache = DecodeCache::new();
    for index in 0..30 {
        cache
            .video_frame("clip2", &path, index as f64 / fixtures::HD_CLIP_FPS)
            .unwrap();
    }
    assert_eq!(cache.seek_count("clip2"), 1);
    cache.video_frame("clip2", &path, 0.0).unwrap();
    assert_eq!(cache.seek_count("clip2"), 2);
    cache
        .video_frame("clip2", &path, 1.0 / fixtures::HD_CLIP_FPS)
        .unwrap();
    assert_eq!(cache.seek_count("clip2"), 2, "forward again must not seek");
}

#[test]
fn a_decode_failure_recovers_on_the_next_request() {
    let clip = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);
    let mut cache = DecodeCache::new();
    let missing = clip.with_file_name("definitely-not-here.mp4");
    let error = cache.video_frame("clip", &missing, 0.0).unwrap_err();
    eprintln!("expected failure: {error}");
    assert_eq!(cache.stats().failures, 1);

    let frame = cache
        .video_frame("clip", &clip, 0.0)
        .expect("the cache must not stay poisoned after a failure");
    assert!(frame.width > 0 && frame.height > 0);
    assert_eq!(cache.stats().failures, 1);
}

#[test]
fn mixes_two_overlapping_clips_by_summing() {
    let directory = std::env::temp_dir().join("cutix-playback-tests");
    std::fs::create_dir_all(&directory).unwrap();
    let sample_rate = 48_000u32;
    let quarter = vec![8_192i16; sample_rate as usize];
    let half = vec![16_384i16; sample_rate as usize];
    let first = directory.join("quarter.wav");
    let second = directory.join("half.wav");
    write_wav(&first, sample_rate, &quarter);
    write_wav(&second, sample_rate, &half);

    let project = project_with(vec![
        audio_element("a", "quarter", 0.0, 1.0, 0.0),
        audio_element("b", "half", 0.5, 0.5, 0.0),
    ]);
    let media = MediaMap::new()
        .with("quarter", &first)
        .with("half", &second);
    let mut cache = AudioCache::new();

    let (buffer, skipped) = mix(
        &MixRequest {
            project: &project,
            scene_id: None,
            start: MediaTime::ZERO,
            duration: seconds(1.0),
            sample_rate,
            channels: 2,
        },
        &media,
        &mut cache,
    )
    .unwrap();

    assert!(skipped.is_empty(), "{skipped:?}");
    assert_eq!(buffer.frame_count(), sample_rate as usize);

    let quarter_value = 8_192.0 / 32_768.0;
    let half_value = 16_384.0 / 32_768.0;
    let before = buffer.sample(1_000, 0);
    let during = buffer.sample(sample_rate as usize * 3 / 4, 0);
    eprintln!("before {before} during {during}");
    assert!((before - quarter_value).abs() < 1e-4, "{before}");
    assert!(
        (during - (quarter_value + half_value)).abs() < 1e-4,
        "{during}"
    );

    let attenuated = project_with(vec![
        audio_element("a", "quarter", 0.0, 1.0, 0.0),
        audio_element("b", "half", 0.5, 0.5, -6.0),
    ]);
    let (buffer, _) = mix(
        &MixRequest {
            project: &attenuated,
            scene_id: None,
            start: MediaTime::ZERO,
            duration: seconds(1.0),
            sample_rate,
            channels: 2,
        },
        &media,
        &mut cache,
    )
    .unwrap();
    let expected = quarter_value + half_value * 10f64.powf(-6.0 / 20.0) as f32;
    let measured = buffer.sample(sample_rate as usize * 3 / 4, 0);
    eprintln!("attenuated {measured} expected {expected}");
    assert!((measured - expected).abs() < 1e-4);
}

#[test]
fn volume_keyframes_ramp_the_mix() {
    let directory = std::env::temp_dir().join("cutix-playback-tests");
    std::fs::create_dir_all(&directory).unwrap();
    let sample_rate = 48_000u32;
    let path = directory.join("full.wav");
    write_wav(&path, sample_rate, &vec![32_767i16; sample_rate as usize]);

    let element: TimelineElement = serde_json::from_value(json!({
        "type": "audio",
        "id": "ramp",
        "name": "ramp",
        "duration": seconds(1.0).as_ticks(),
        "startTime": 0,
        "trimStart": 0,
        "trimEnd": 0,
        "sourceType": "media",
        "mediaId": "full",
        "volume": 0.0,
        "animations": {
            "bindings": {},
            "channels": {
                "volume:value": {
                    "kind": "scalar",
                    "keys": [
                        { "id": "k0", "time": 0, "value": -60.0, "segmentToNext": "linear", "tangentMode": "flat" },
                        { "id": "k1", "time": TICKS_PER_SECOND, "value": 0.0, "segmentToNext": "linear", "tangentMode": "flat" }
                    ]
                }
            }
        }
    }))
    .unwrap();

    let project = project_with(vec![element]);
    let media = MediaMap::new().with("full", &path);
    let mut cache = AudioCache::new();
    let (buffer, _) = mix(
        &MixRequest {
            project: &project,
            scene_id: None,
            start: MediaTime::ZERO,
            duration: seconds(1.0),
            sample_rate,
            channels: 1,
        },
        &media,
        &mut cache,
    )
    .unwrap();

    let midpoint = buffer.sample(sample_rate as usize / 2, 0);
    let expected = 32_767.0 / 32_768.0 * 10f32.powf(-30.0 / 20.0);
    eprintln!("midpoint {midpoint} expected {expected}");
    assert!((midpoint - expected).abs() < 2e-3, "{midpoint}");
    assert!(buffer.sample(0, 0).abs() < 2e-3);
}

#[test]
fn decodes_the_test_mp3() {
    let path = fixtures::fixture_or_skip!(fixtures::AUDIO_CLIP);
    let buffer = cutix_playback::decode_audio(&path).unwrap();
    eprintln!(
        "mp3: {} Hz, {} channels, {:.3} s, peak {:.3}",
        buffer.sample_rate,
        buffer.channels,
        buffer.duration_seconds(),
        buffer
            .channel(0)
            .iter()
            .fold(0.0f32, |peak, value| peak.max(value.abs()))
    );
    assert!(buffer.sample_rate >= 8_000);
    assert!(buffer.duration_seconds() > 1.0);
    assert!(buffer.channel(0).iter().any(|value| value.abs() > 0.01));
}

#[test]
fn an_mp4_with_no_sound_says_so_rather_than_blaming_the_decoder() {
    let clip = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);
    let error = cutix_playback::decode_audio(&clip).unwrap_err();
    let message = error.to_string();
    assert!(message.contains("no audio track"), "{message}");
}

#[test]
fn aac_decodes_without_ffmpeg() {
    let clip = fixtures::fixture_or_skip!(fixtures::AAC_CLIP);
    let buffer = cutix_playback::decode_audio(&clip).expect("aac decodes");
    assert!(buffer.sample_rate >= 8_000);
    assert!(buffer.duration_seconds() > 0.0);
    assert!(buffer.channel(0).iter().any(|value| value.abs() > 0.001));
}

#[test]
fn a_window_of_aac_starts_where_it_was_asked_for() {
    let clip = fixtures::fixture_or_skip!(fixtures::AAC_CLIP);
    let whole = cutix_playback::decode_audio(&clip).expect("aac decodes");
    if whole.duration_seconds() < 1.5 {
        return;
    }
    let (window, start) =
        cutix_playback::decode_audio_window(&clip, 1.0, 0.5).expect("a window decodes");
    assert!(
        start <= 1.0 + 1e-3,
        "the window began after what was asked for: {start}"
    );
    assert!(window.duration_seconds() > 0.0);
    assert!(window.duration_seconds() < whole.duration_seconds());
}

#[test]
fn audio_output_receives_the_mixdown() {
    let Ok(output) = cutix_playback::AudioOutput::open() else {
        eprintln!("no audio device; skipping");
        return;
    };
    let samples: Vec<f32> = (0..output.sample_rate() as usize * output.channels())
        .map(|index| ((index / output.channels()) as f32 * 0.01).sin() * 0.25)
        .collect();
    output.queue_samples(&samples);
    let queued = output.queued_samples();
    assert_eq!(queued, samples.len());
    output.start();

    let started = std::time::Instant::now();
    while output.consumed_samples() == 0 && started.elapsed().as_secs_f64() < 30.0 {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(output.callback_count() > 0, "the device callback never ran");
    let (consumed, delivered) = output.last_delivery();
    assert!(consumed > 0, "the device never took samples from the queue");
    assert!(!delivered.is_empty());
    assert!(consumed <= queued);
    eprintln!(
        "device {} Hz x{} channels, {} callbacks, {} samples consumed, first delivered {:?}",
        output.sample_rate(),
        output.channels(),
        output.callback_count(),
        consumed,
        &delivered[..delivered.len().min(4)]
    );
    let offset = consumed - delivered.len();
    assert_eq!(
        delivered,
        &samples[offset..offset + delivered.len()],
        "the callback must receive exactly the queued mixdown"
    );

    output.pause();
    output.seek();
    assert_eq!(output.queued_samples(), 0);
    assert_eq!(output.played_frames(), 0);
    output.stop().unwrap();
}

#[test]
fn the_controller_serves_frames_without_blocking_the_caller() {
    let clip = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);
    let project = Arc::new(project_with(vec![video_element("clip", 0.0, 2.0)]));
    let media = MediaMap::new().with("clip", &clip);
    let Ok(controller) = PlaybackController::new(project, None, Box::new(media), None) else {
        eprintln!("no gpu adapter; skipping");
        return;
    };

    assert!(!controller.is_playing());
    controller.request_frame(MediaTime::ZERO, 160, 90);

    let started = std::time::Instant::now();
    let mut slot = None;
    while started.elapsed().as_secs_f64() < 20.0 {
        if let Some(found) = controller.latest_frame() {
            slot = Some(found);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let slot = slot.unwrap_or_else(|| panic!("no frame: {:?}", controller.take_error()));
    assert_eq!(slot.time, MediaTime::ZERO);
    assert_eq!(slot.frame.width, 160);

    controller.seek(seconds(0.5));
    assert_eq!(controller.current_time(), seconds(0.5));
    controller.set_rate(2.0);
    controller.play();
    std::thread::sleep(std::time::Duration::from_millis(120));
    let advanced = controller.current_time();
    assert!(
        advanced > seconds(0.5),
        "clock did not advance: {advanced:?}"
    );
    controller.pause();
    let paused = controller.current_time();
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert_eq!(controller.current_time(), paused);
}

fn text_element(content: &str, patch: serde_json::Value) -> TimelineElement {
    let mut value = json!({
        "type": "text",
        "id": "text-element",
        "name": "text",
        "duration": seconds(2.0).as_ticks(),
        "startTime": 0,
        "trimStart": 0,
        "trimEnd": 0,
        "content": content,
        "fontSize": 15.0,
        "fontFamily": "Arial",
        "color": "#ffffff",
        "background": { "enabled": false, "color": "#000000" },
        "textAlign": "center",
        "fontWeight": "bold",
        "fontStyle": "normal",
        "textDecoration": "none",
        "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
        "opacity": 1.0
    });
    let object = value.as_object_mut().unwrap();
    for (key, entry) in patch.as_object().unwrap() {
        object.insert(key.clone(), entry.clone());
    }
    serde_json::from_value(value).unwrap()
}

#[test]
fn a_text_element_paints_instead_of_being_skipped() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let project = project_with(vec![text_element("cutix", json!({}))]);
    let media = MediaMap::new();

    let frame = composer
        .compose(
            &ComposeRequest {
                project: &project,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 640,
                height: 360,
            },
            &media,
        )
        .unwrap();

    assert!(
        !frame.skipped.iter().any(|kind| kind == "text"),
        "{:?}",
        frame.skipped
    );
    let lit = frame
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[0] > 180 && pixel[1] > 180 && pixel[2] > 180)
        .count();
    assert!(lit > 100, "{lit} lit pixels");
    assert_eq!(frame.rects.len(), 1);
    assert_eq!(frame.rects[0].0, "text-element");
}

#[test]
fn a_positioned_text_element_moves_its_ink() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let media = MediaMap::new();
    let centre = |project: &Project, composer: &mut FrameComposer| {
        let frame = composer
            .compose(
                &ComposeRequest {
                    project,
                    scene_id: None,
                    time: MediaTime::ZERO,
                    width: 640,
                    height: 360,
                },
                &media,
            )
            .unwrap();
        frame.rects[0].1.center_x
    };

    let base = project_with(vec![text_element("cutix", json!({}))]);
    let moved = project_with(vec![text_element(
        "cutix",
        json!({ "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 120.0, "y": 0.0 }, "rotate": 0.0 } }),
    )]);

    let before = centre(&base, &mut composer);
    let after = centre(&moved, &mut composer);
    assert!((after - before - 40.0).abs() < 1.0, "{before} -> {after}");
}

fn sticker_element(sticker_id: &str, patch: serde_json::Value) -> TimelineElement {
    let mut value = json!({
        "type": "sticker",
        "id": "sticker-element",
        "name": "sticker",
        "duration": seconds(2.0).as_ticks(),
        "startTime": 0,
        "trimStart": 0,
        "trimEnd": 0,
        "stickerId": sticker_id,
        "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
        "opacity": 1.0
    });
    let object = value.as_object_mut().unwrap();
    for (key, entry) in patch.as_object().unwrap() {
        object.insert(key.clone(), entry.clone());
    }
    serde_json::from_value(value).unwrap()
}

fn graphic_element(
    definition_id: &str,
    params: serde_json::Value,
    patch: serde_json::Value,
) -> TimelineElement {
    let mut value = json!({
        "type": "graphic",
        "id": "graphic-element",
        "name": "graphic",
        "duration": seconds(2.0).as_ticks(),
        "startTime": 0,
        "trimStart": 0,
        "trimEnd": 0,
        "definitionId": definition_id,
        "params": params,
        "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
        "opacity": 1.0
    });
    let object = value.as_object_mut().unwrap();
    for (key, entry) in patch.as_object().unwrap() {
        object.insert(key.clone(), entry.clone());
    }
    serde_json::from_value(value).unwrap()
}

fn compose_640x360(
    project: &Project,
    composer: &mut FrameComposer,
) -> cutix_playback::ComposedFrame {
    composer
        .compose(
            &ComposeRequest {
                project,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 640,
                height: 360,
            },
            &MediaMap::new(),
        )
        .unwrap()
}

#[test]
fn a_sticker_paints_its_real_pixels_instead_of_being_skipped() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let project = project_with(vec![sticker_element("flags:JP", json!({}))]);
    let frame = compose_640x360(&project, &mut composer);

    assert!(
        !frame.skipped.iter().any(|kind| kind.starts_with("sticker")),
        "{:?}",
        frame.skipped
    );
    let centre = frame.pixel(320, 180);
    assert!(centre[0] > 170, "centre should be red {centre:?}");
    assert!(
        centre[1] < 100 && centre[2] < 100,
        "centre should be red {centre:?}"
    );

    let white = frame.pixel(320, 40);
    assert!(
        white[0] > 220 && white[1] > 220 && white[2] > 220,
        "top of the flag should be white {white:?}"
    );

    assert_eq!(frame.rects.len(), 1);
    assert_eq!(frame.rects[0].0, "sticker-element");
    let rect = frame.rects[0].1;
    assert!((rect.center_x - 320.0).abs() < 0.5, "{rect:?}");
    assert!((rect.center_y - 180.0).abs() < 0.5, "{rect:?}");
    assert!(
        (rect.height - 360.0).abs() < 1.0,
        "sticker should fit the height {rect:?}"
    );
}

#[test]
fn a_scaled_and_moved_sticker_lands_where_the_transform_says() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let project = project_with(vec![sticker_element(
        "flags:JP",
        json!({
            "transform": { "scaleX": 0.25, "scaleY": 0.25, "position": { "x": -100.0, "y": 60.0 }, "rotate": 0.0 }
        }),
    )]);
    let frame = compose_640x360(&project, &mut composer);
    let rect = frame.rects[0].1;

    assert!(
        (rect.center_x - (320.0 - 100.0 / 3.0)).abs() < 0.5,
        "{rect:?}"
    );
    assert!(
        (rect.center_y - (180.0 + 60.0 / 3.0)).abs() < 0.5,
        "{rect:?}"
    );
    assert!((rect.height - 90.0).abs() < 1.0, "{rect:?}");

    let inside = frame.pixel(rect.center_x as u32, rect.center_y as u32);
    assert!(
        inside[0] > 170 && inside[1] < 100,
        "moved sticker centre {inside:?}"
    );
    let outside = frame.pixel(600, 40);
    assert_eq!(outside[3], 255);
    assert!(
        outside[0] < 40 && outside[1] < 40,
        "outside the sticker is background {outside:?}"
    );
}

#[test]
fn a_hidden_sticker_paints_nothing() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let project = project_with(vec![sticker_element("flags:JP", json!({ "hidden": true }))]);
    let frame = compose_640x360(&project, &mut composer);
    assert!(frame.rects.is_empty());
    let centre = frame.pixel(320, 180);
    assert!(
        centre[0] < 20 && centre[1] < 20 && centre[2] < 20,
        "{centre:?}"
    );
}

#[test]
fn an_unknown_sticker_is_reported_not_rendered() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let project = project_with(vec![sticker_element("flags:NOT-A-COUNTRY", json!({}))]);
    let frame = compose_640x360(&project, &mut composer);
    assert!(frame.rects.is_empty());
    assert!(
        frame
            .skipped
            .iter()
            .any(|entry| entry.starts_with("sticker:")),
        "{:?}",
        frame.skipped
    );
}

#[test]
fn a_graphic_shape_paints_its_fill_colour() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let project = project_with(vec![graphic_element(
        "ellipse",
        json!({ "fill": "#00ff00" }),
        json!({}),
    )]);
    let frame = compose_640x360(&project, &mut composer);

    assert!(frame.skipped.is_empty(), "{:?}", frame.skipped);
    let centre = frame.pixel(320, 180);
    assert!(centre[1] > 200, "centre should be green {centre:?}");
    assert!(
        centre[0] < 80 && centre[2] < 80,
        "centre should be green {centre:?}"
    );

    let rect = frame.rects[0].1;
    assert!(
        (rect.width - 360.0).abs() < 1.0,
        "square source contain-fits {rect:?}"
    );
    let corner = frame.pixel(320 - 175, 180 - 175);
    assert!(
        corner[1] < 80,
        "ellipse corner should be background {corner:?}"
    );
}

#[test]
fn a_graphic_stroke_paints_a_border() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let project = project_with(vec![graphic_element(
        "rectangle",
        json!({ "fill": "#ffffff", "stroke": "#ff0000", "strokeWidth": 40, "strokeAlign": "inside" }),
        json!({}),
    )]);
    let frame = compose_640x360(&project, &mut composer);

    let border = frame.pixel(320, 5);
    assert!(
        border[0] > 170 && border[1] < 90,
        "top border should be red {border:?}"
    );
    let middle = frame.pixel(320, 180);
    assert!(
        middle[0] > 220 && middle[1] > 220 && middle[2] > 220,
        "middle should be the white fill {middle:?}"
    );
}

#[test]
fn text_ink_is_centred_and_inside_the_canvas() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let project = project_with(vec![text_element(
        "Native captions from SRT",
        json!({
            "fontSize": 5.0,
            "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 }
        }),
    )]);
    let frame = compose_640x360(&project, &mut composer);

    let mut min_x = u32::MAX;
    let mut max_x = 0u32;
    for y in 0..frame.height {
        for x in 0..frame.width {
            if frame.pixel(x, y)[0] > 120 {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
            }
        }
    }
    assert!(min_x != u32::MAX, "nothing was drawn");
    let centre = (min_x + max_x) as f64 / 2.0;
    let width = (max_x - min_x) as f64;
    assert!(
        (centre - 320.0).abs() < 8.0,
        "ink centre {centre} should be near 320"
    );
    assert!(
        max_x < frame.width - 1,
        "ink touches the right edge: {max_x}"
    );

    assert!(
        (150.0..320.0).contains(&width),
        "ink is {width}px wide, which means the advances are wrong"
    );
}

fn stripes_png() -> PathBuf {
    let directory = std::env::temp_dir().join("cutix-playback-tests");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("stripes.png");
    let mut buffer = image::RgbaImage::new(256, 256);
    for y in 0..256u32 {
        for x in 0..256u32 {
            let value = if (x / 4) % 2 == 0 { 0u8 } else { 255u8 };
            buffer.put_pixel(x, y, image::Rgba([value, value, value, 255]));
        }
    }
    buffer.save(&path).unwrap();
    path
}

fn right_half_matte() -> (Vec<u8>, u32, u32) {
    let (width, height) = (256usize, 256usize);
    let alpha: Vec<u8> = (0..width * height)
        .map(|index| if index % width < width / 2 { 0 } else { 255 })
        .collect();
    (alpha, width as u32, height as u32)
}

#[test]
fn a_mask_stroke_paints_its_colour_on_the_mask_edge() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let media = MediaMap::new().with("quadrants", quadrant_png());
    let project = project_with(vec![image_element(
        "quadrants",
        json!({
            "masks": [{
                "id": "mask-1",
                "type": "rectangle",
                "params": {
                    "centerX": 0.0,
                    "centerY": 0.0,
                    "width": 0.5,
                    "height": 0.5,
                    "rotation": 0.0,
                    "feather": 0.0,
                    "inverted": false,
                    "strokeWidth": 8.0,
                    "strokeColor": "#ff00ff",
                    "strokeAlign": "center"
                }
            }]
        }),
    )]);

    let frame = composer
        .compose(
            &ComposeRequest {
                project: &project,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 200,
                height: 200,
            },
            &media,
        )
        .expect("compose");

    assert_eq!(frame.pixel(48, 100), [255, 0, 255, 255], "on the stroke");
    assert_eq!(
        frame.pixel(100, 48),
        [255, 0, 255, 255],
        "on the top stroke"
    );
    assert_eq!(frame.pixel(20, 100), [0, 0, 0, 255], "outside the mask");
    assert_ne!(frame.pixel(100, 100), [255, 0, 255, 255], "inside the mask");
}

#[test]
fn background_blur_keeps_the_subject_sharp_and_softens_everything_else() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let media = MediaMap::new().with("stripes", stripes_png());
    let (alpha, width, height) = right_half_matte();
    let png = ml::encode_matte_png(&alpha, width, height).expect("png");
    let cutout = json!({
        "enabled": true,
        "mode": "static",
        "width": width,
        "height": height,
        "png": ml::base64_encode(&png),
        "invert": false,
        "referenceTime": 0.0,
        "coverage": 0.5
    });

    let plain = project_with(vec![image_element(
        "stripes",
        json!({ "cutout": cutout.clone() }),
    )]);
    let blurred = project_with(vec![image_element(
        "stripes",
        json!({
            "cutout": cutout,
            "effects": [{
                "id": "fx",
                "type": "background-blur",
                "enabled": true,
                "params": { "strength": 100 }
            }]
        }),
    )]);

    fn request(project: &Project, size: u32) -> ComposeRequest<'_> {
        ComposeRequest {
            project,
            scene_id: None,
            time: MediaTime::ZERO,
            width: size,
            height: size,
        }
    }
    let without = composer
        .compose(&request(&plain, 256), &media)
        .expect("compose");
    let with = composer
        .compose(&request(&blurred, 256), &media)
        .expect("compose");

    assert_eq!(without.pixel(64, 128), [0, 0, 0, 255]);

    let background: Vec<u8> = (40..88).map(|x| with.pixel(x, 128)[0]).collect();
    let spread = background.iter().copied().max().unwrap() as i32
        - background.iter().copied().min().unwrap() as i32;
    assert!(
        background.iter().any(|value| *value > 40),
        "background was not drawn at all: {background:?}"
    );
    assert!(
        spread < 120,
        "background is still sharp: spread {spread} (subject side is 255)"
    );

    let subject: Vec<u8> = (160..224).map(|x| with.pixel(x, 128)[0]).collect();
    let subject_spread = subject.iter().copied().max().unwrap() as i32
        - subject.iter().copied().min().unwrap() as i32;
    assert!(
        subject_spread > 200,
        "subject was blurred too: spread {subject_spread}"
    );
    for x in 160..224 {
        assert_eq!(
            with.pixel(x, 128),
            without.pixel(x, 128),
            "subject moved at x={x}"
        );
    }
}

#[test]
fn a_matte_stored_as_a_file_composes_the_same_as_an_inline_one() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let media = MediaMap::new().with("stripes", stripes_png());
    let (alpha, width, height) = right_half_matte();
    let png = ml::encode_matte_png(&alpha, width, height).expect("png");

    let directory = std::env::temp_dir().join("cutix-matte-file-test");
    std::fs::create_dir_all(directory.join("mattes")).unwrap();
    std::fs::write(directory.join("mattes/subject.png"), &png).unwrap();

    let base = |matte: serde_json::Value| {
        project_with(vec![image_element("stripes", json!({ "cutout": matte }))])
    };
    let inline = base(json!({
        "enabled": true, "mode": "static", "width": width, "height": height,
        "png": ml::base64_encode(&png), "invert": false, "referenceTime": 0.0, "coverage": 0.5
    }));
    let filed = base(json!({
        "enabled": true, "mode": "static", "width": width, "height": height,
        "pngPath": "mattes/subject.png", "invert": false, "referenceTime": 0.0, "coverage": 0.5
    }));

    fn request(project: &Project, size: u32) -> ComposeRequest<'_> {
        ComposeRequest {
            project,
            scene_id: None,
            time: MediaTime::ZERO,
            width: size,
            height: size,
        }
    }
    let from_inline = composer
        .compose(&request(&inline, 128), &media)
        .expect("compose");
    composer.set_matte_root(Some(directory.clone()));
    let from_file = composer
        .compose(&request(&filed, 128), &media)
        .expect("compose");

    let lit = |frame: &cutix_playback::render::ComposedFrame, from: u32, to: u32| {
        (from..to).filter(|x| frame.pixel(*x, 64)[0] > 0).count()
    };

    assert_eq!(from_inline.pixels, from_file.pixels);
    assert_eq!(lit(&from_file, 0, 64), 0, "the cut half is fully black");
    assert!(lit(&from_file, 64, 128) > 20, "the kept half survives");

    composer.set_matte_root(None);
    let unresolved = composer
        .compose(&request(&filed, 128), &media)
        .expect("compose");
    assert!(
        lit(&unresolved, 0, 64) > 20,
        "an unresolved matte masks nothing"
    );
}

#[test]
fn a_blurred_canvas_background_covers_the_canvas_instead_of_being_skipped() {
    let Ok(mut composer) = FrameComposer::new() else {
        eprintln!("no gpu adapter; skipping");
        return;
    };
    let media = MediaMap::new().with("stripes", stripes_png());
    let mut project = project_with(vec![image_element(
        "stripes",
        json!({
            "transform": {
                "scaleX": 0.25,
                "scaleY": 0.25,
                "position": { "x": 0.0, "y": 0.0 },
                "rotate": 0.0
            }
        }),
    )]);
    project.settings.background = cutix_project::Background::Blur {
        blur_intensity: 100.0,
    };

    let frame = composer
        .compose(
            &ComposeRequest {
                project: &project,
                scene_id: None,
                time: MediaTime::ZERO,
                width: 256,
                height: 256,
            },
            &media,
        )
        .expect("compose");

    assert!(
        !frame.skipped.iter().any(|entry| entry == "background.blur"),
        "{:?}",
        frame.skipped
    );

    let edge: Vec<u8> = (4..60).map(|x| frame.pixel(x, 8)[0]).collect();
    let spread =
        edge.iter().copied().max().unwrap() as i32 - edge.iter().copied().min().unwrap() as i32;
    assert!(
        edge.iter().any(|value| *value > 40),
        "backdrop missing: {edge:?}"
    );
    assert!(spread < 120, "backdrop is not blurred: spread {spread}");
}

#[test]
fn replacing_the_project_never_presents_a_frame_from_the_old_one() {
    let clip = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);
    let project = Arc::new(project_with(vec![video_element("clip", 0.0, 2.0)]));
    let media = MediaMap::new().with("clip", &clip);
    let Ok(controller) = PlaybackController::new(project, None, Box::new(media), None) else {
        eprintln!("no gpu adapter; skipping");
        return;
    };

    let first = controller.generation();
    let frame = time::FrameRate::FPS_30
        .frame_duration()
        .expect("a valid rate");
    controller.play();
    controller.stream_from(MediaTime::ZERO, 160, 90, frame);

    // Let the worker get frames in flight for the original project.
    let started = std::time::Instant::now();
    while controller.latest_frame().is_none() && started.elapsed().as_secs_f64() < 20.0 {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        controller.latest_frame().is_some(),
        "no frame for the original project: {:?}",
        controller.take_error()
    );

    let replacement = Arc::new(project_with(vec![video_element("clip", 0.0, 1.0)]));
    controller.set_project(replacement);
    let second = controller.generation();
    assert_ne!(second, first, "replacing the project must end the old era");

    // Every frame the worker was composing belongs to the previous project. None of them
    // may reach the presenter, however long the worker takes to finish them.
    let watched = std::time::Instant::now();
    while watched.elapsed().as_secs_f64() < 1.0 {
        if let Some(slot) = controller.latest_frame() {
            assert_eq!(
                slot.generation, second,
                "a frame composed for the replaced project was presented"
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn seeking_ends_the_era_so_frames_for_the_old_playhead_are_dropped() {
    let clip = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);
    let project = Arc::new(project_with(vec![video_element("clip", 0.0, 2.0)]));
    let media = MediaMap::new().with("clip", &clip);
    let Ok(controller) = PlaybackController::new(project, None, Box::new(media), None) else {
        eprintln!("no gpu adapter; skipping");
        return;
    };

    let frame = time::FrameRate::FPS_30
        .frame_duration()
        .expect("a valid rate");
    let before = controller.generation();
    controller.play();
    controller.stream_from(MediaTime::ZERO, 160, 90, frame);

    let started = std::time::Instant::now();
    while controller.latest_frame().is_none() && started.elapsed().as_secs_f64() < 20.0 {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        controller.latest_frame().is_some(),
        "no frame to invalidate"
    );

    // A deliberate jump, as pressing on the timeline makes.
    controller.seek(seconds(1.5));
    let forward = controller.generation();
    assert_ne!(forward, before, "seeking must end the old era");
    assert!(
        controller.latest_frame().is_none(),
        "the frame composed for the old playhead survived the seek"
    );

    // And a backward correction.
    controller.seek(seconds(0.25));
    let backward = controller.generation();
    assert_ne!(backward, forward, "each seek ends its own era");
    assert!(controller.latest_frame().is_none());
    assert_eq!(controller.current_time(), seconds(0.25));
}

#[test]
fn a_rate_whose_frame_is_not_whole_ticks_still_streams_at_the_right_speed() {
    let clip = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);
    let project = Arc::new(project_with(vec![video_element("clip", 0.0, 2.0)]));
    let media = MediaMap::new().with("clip", &clip);
    let Ok(controller) = PlaybackController::new(project, None, Box::new(media), None) else {
        eprintln!("no gpu adapter; skipping");
        return;
    };

    // 23 fps snaps onto no broadcast rate and one frame is not a whole number of ticks.
    let rate = time::FrameRate::nearest(23.0).expect("a real rate");
    assert_eq!(rate.ticks_per_frame(), None);
    let frame = rate.frame_duration().expect("a valid rate has a duration");
    controller.stream_from(MediaTime::ZERO, 160, 90, frame);

    let started = std::time::Instant::now();
    let mut seen: Vec<MediaTime> = Vec::new();
    while seen.len() < 3 && started.elapsed().as_secs_f64() < 20.0 {
        if let Some(slot) = controller.latest_frame()
            && seen.last() != Some(&slot.time)
        {
            seen.push(slot.time);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(seen.len() >= 2, "no frames: {:?}", controller.take_error());

    // Consecutive frames must be about a twenty-third of a second apart. The old integer
    // fallback produced a single tick here, which is where the runaway speed came from.
    let step = seen[1].as_ticks() - seen[0].as_ticks();
    let expected = TICKS_PER_SECOND / 23;
    assert!(
        (step - expected).abs() <= 1,
        "frames {step} ticks apart, expected about {expected}"
    );
}

/// The audio clock is corrected against the device continuously, and past the sync
/// tolerance on any device carrying real lead. Correcting through `seek` would empty the
/// pipeline each time, leaving nothing to present between corrections: sound plays on
/// while the picture sits on whichever frame arrived first.
#[test]
fn correcting_the_clock_keeps_the_frames_already_composed() {
    let clip = fixtures::fixture_or_skip!(fixtures::SQUARE_CLIP);
    let project = Arc::new(project_with(vec![video_element("clip", 0.0, 2.0)]));
    let media = MediaMap::new().with("clip", &clip);
    let Ok(controller) = PlaybackController::new(project, None, Box::new(media), None) else {
        eprintln!("no gpu adapter; skipping");
        return;
    };

    let frame = time::FrameRate::FPS_30
        .frame_duration()
        .expect("a valid rate");
    let before = controller.generation();
    controller.play();
    controller.stream_from(MediaTime::ZERO, 160, 90, frame);

    let started = std::time::Instant::now();
    while controller.latest_frame().is_none() && started.elapsed().as_secs_f64() < 20.0 {
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(
        controller.latest_frame().is_some(),
        "no frame to correct across"
    );

    controller.retime(seconds(0.5));
    assert_eq!(
        controller.generation(),
        before,
        "correcting the clock must not end the era"
    );
    assert!(
        controller.latest_frame().is_some(),
        "correcting the clock must not discard the composed frames"
    );
    assert!(
        controller.current_time() >= seconds(0.5),
        "the correction must actually move the playhead"
    );
}
