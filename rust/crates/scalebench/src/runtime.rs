use std::path::PathBuf;
use std::time::Instant;

use cutix_playback::decode_cache::DecodeCache;
use cutix_playback::render::{ComposeRequest, FrameComposer};
use cutix_playback::MediaMap;
use time::MediaTime;

use crate::gen;
use crate::mem;

fn seconds(value: f64) -> MediaTime {
    MediaTime::from_seconds_f64(value).unwrap_or(MediaTime::ZERO)
}

fn fan_out(source: &PathBuf, count: usize, name: &str) -> (PathBuf, Vec<PathBuf>) {
    let directory = std::env::temp_dir().join(format!("cutix-scalebench-{name}"));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("media dir");
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("mp4");
    let mut paths = Vec::new();
    for index in 0..count {
        let target = directory.join(format!("media-{index}.{extension}"));
        std::fs::copy(source, &target).expect("copy fixture");
        paths.push(target);
    }
    (directory, paths)
}

pub fn decode_growth(distinct_media: usize) {
    println!("== decode cache: memory per distinct media source ==");
    let Some(clip) = fixtures::fixture(fixtures::HD_CLIP) else {
        println!("  {} missing; skipping", fixtures::HD_CLIP);
        return;
    };
    let (directory, paths) = fan_out(&clip, distinct_media, "decode");

    let mut cache = DecodeCache::new();
    let baseline = mem::snapshot();
    println!(
        "  baseline: working_set {:.1} MB, private {:.1} MB",
        mem::mb(baseline.working_set),
        mem::mb(baseline.private)
    );
    println!(
        "{:>8} {:>12} {:>12} {:>14}",
        "sources", "working MB", "private MB", "MB/source"
    );

    for (index, path) in paths.iter().enumerate() {
        let media_id = format!("media-{index}");
        for frame in 0..6 {
            let _ = cache.video_frame(&media_id, path, frame as f64 / fixtures::HD_CLIP_FPS);
        }
        let count = index + 1;
        if count % 10 == 0 || count == paths.len() {
            let now = mem::snapshot();
            let grown = mem::mb(now.private) - mem::mb(baseline.private);
            println!(
                "{count:>8} {:>12.1} {:>12.1} {:>14.2}",
                mem::mb(now.working_set),
                mem::mb(now.private),
                grown / count as f64
            );
        }
    }

    println!(
        "  stats after fill: {:?}, live decoders {}, resident {:.1} MB",
        cache.stats(),
        cache.live_decoders(),
        mem::mb(cache.resident_bytes())
    );
    let before_clear = mem::snapshot();
    cache.clear();
    let after_clear = mem::snapshot();
    println!(
        "  explicit clear() releases: working_set {:+.1} MB, private {:+.1} MB",
        mem::mb(after_clear.working_set) - mem::mb(before_clear.working_set),
        mem::mb(after_clear.private) - mem::mb(before_clear.private)
    );
    drop(cache);
    let after_drop = mem::snapshot();
    println!(
        "  after dropping the cache: working_set {:.1} MB, private {:.1} MB (baseline {:.1} / {:.1})",
        mem::mb(after_drop.working_set),
        mem::mb(after_drop.private),
        mem::mb(baseline.working_set),
        mem::mb(baseline.private)
    );
    let _ = std::fs::remove_dir_all(&directory);
}

pub fn compose(elements: usize, frames: usize) {
    println!("== composition: {elements} elements, {frames} composed frames at 1920x1080 ==");
    let Ok(mut composer) = FrameComposer::new() else {
        println!("  no gpu adapter; skipping");
        return;
    };
    let Some(clip) = fixtures::fixture(fixtures::HD_CLIP) else {
        println!("  {} missing; skipping", fixtures::HD_CLIP);
        return;
    };

    let media_files = 40usize;
    let (directory, paths) = fan_out(&clip, media_files, "compose");
    let mut media = MediaMap::new();
    for (index, path) in paths.iter().enumerate() {
        media = media.with(&format!("media-{index}"), path);
    }

    let shape = gen::Shape {
        elements,
        media_files,
        minutes: 10.0,
        ..gen::Shape::default()
    };
    let project = gen::build(&shape, 0x5EED);

    let baseline = mem::snapshot();
    println!(
        "  baseline: working_set {:.1} MB, private {:.1} MB",
        mem::mb(baseline.working_set),
        mem::mb(baseline.private)
    );
    println!(
        "{:>8} {:>12} {:>12} {:>12} {:>12}",
        "frame", "ms (mean)", "ms (worst)", "working MB", "private MB"
    );

    let mut window_total = 0.0f64;
    let mut window_worst = 0.0f64;
    let mut window_count = 0usize;
    let span = 10.0 * 60.0;

    for index in 0..frames {
        let time = if index < frames / 2 {
            (index as f64) / 30.0
        } else {
            ((index as u64).wrapping_mul(2_654_435_761) % 100_000) as f64 / 100_000.0 * span
        };

        let start = Instant::now();
        let result = composer.compose(
            &ComposeRequest {
                project: &project,
                scene_id: None,
                time: seconds(time),
                width: 1920,
                height: 1080,
            },
            &media,
        );
        let took = start.elapsed().as_secs_f64() * 1000.0;
        if result.is_err() {
            println!("  compose failed at frame {index}");
            break;
        }
        window_total += took;
        window_worst = window_worst.max(took);
        window_count += 1;

        if window_count == 25 || index + 1 == frames {
            let now = mem::snapshot();
            println!(
                "{:>8} {:>12.1} {:>12.1} {:>12.1} {:>12.1}",
                index + 1,
                window_total / window_count as f64,
                window_worst,
                mem::mb(now.working_set),
                mem::mb(now.private)
            );
            window_total = 0.0;
            window_worst = 0.0;
            window_count = 0;
        }
    }

    let end = mem::snapshot();
    println!(
        "  growth across the run: working_set {:+.1} MB, private {:+.1} MB",
        mem::mb(end.working_set) - mem::mb(baseline.working_set),
        mem::mb(end.private) - mem::mb(baseline.private)
    );
    println!("  decode stats: {:?}", composer.stats());
    println!("  cache bytes: {:?}", composer.cache_bytes());
    println!("  budget: {:?}", composer.budget());
    println!(
        "  accounted: {:.1} MB",
        mem::mb(composer.cache_bytes().total())
    );
    println!("  matte cache (hits, misses): {:?}", composer.matte_stats());

    drop(composer);
    let after = mem::snapshot();
    println!(
        "  after dropping the composer: working_set {:.1} MB, private {:.1} MB (baseline {:.1} / {:.1})",
        mem::mb(after.working_set),
        mem::mb(after.private),
        mem::mb(baseline.working_set),
        mem::mb(baseline.private)
    );
    let _ = std::fs::remove_dir_all(&directory);
}

pub fn compose_cycles(cycles: usize, elements: usize, frames: usize) {
    println!("== open/compose/close repeated {cycles} times ==");
    let Some(clip) = fixtures::fixture(fixtures::HD_CLIP) else {
        println!("  {} missing; skipping", fixtures::HD_CLIP);
        return;
    };
    let media_files = 20usize;
    let (directory, paths) = fan_out(&clip, media_files, "cycles");
    let mut media = MediaMap::new();
    for (index, path) in paths.iter().enumerate() {
        media = media.with(&format!("media-{index}"), path);
    }
    let shape = gen::Shape {
        elements,
        media_files,
        minutes: 5.0,
        ..gen::Shape::default()
    };

    let baseline = mem::snapshot();
    println!(
        "  baseline: working_set {:.1} MB, private {:.1} MB",
        mem::mb(baseline.working_set),
        mem::mb(baseline.private)
    );

    for cycle in 0..cycles {
        let project = gen::build(&shape, 0x5EED + cycle as u64);
        let Ok(mut composer) = FrameComposer::new() else {
            println!("  no gpu adapter; skipping");
            return;
        };
        for index in 0..frames {
            let time = (index as f64) * 0.37;
            let _ = composer.compose(
                &ComposeRequest {
                    project: &project,
                    scene_id: None,
                    time: seconds(time),
                    width: 1920,
                    height: 1080,
                },
                &media,
            );
        }
        drop(composer);
        drop(project);
        let now = mem::snapshot();
        println!(
            "  cycle {:>3}: working_set {:>8.1} MB  private {:>8.1} MB  (private {:+.1} vs baseline)",
            cycle + 1,
            mem::mb(now.working_set),
            mem::mb(now.private),
            mem::mb(now.private) - mem::mb(baseline.private)
        );
    }
    let _ = std::fs::remove_dir_all(&directory);
}
