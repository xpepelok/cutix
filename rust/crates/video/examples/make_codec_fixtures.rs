use std::path::{Path, PathBuf};
use std::process::Command;

struct Recipe {
    output: &'static str,
    args: &'static [&'static str],
}

const CLIP_START: &str = "20";
const CLIP_SECONDS: &str = "2";

const RECIPES: &[Recipe] = &[
    Recipe {
        output: "clip-vp9.webm",
        args: &[
            "-c:v",
            "libvpx-vp9",
            "-b:v",
            "600k",
            "-cpu-used",
            "8",
            "-row-mt",
            "1",
            "-c:a",
            "libopus",
            "-b:a",
            "64k",
        ],
    },
    Recipe {
        output: "clip-vp8.webm",
        args: &[
            "-c:v",
            "libvpx",
            "-b:v",
            "600k",
            "-cpu-used",
            "8",
            "-c:a",
            "libvorbis",
        ],
    },
    Recipe {
        output: "clip-av1.mkv",
        args: &[
            "-c:v",
            "libsvtav1",
            "-preset",
            "10",
            "-crf",
            "40",
            "-c:a",
            "libopus",
            "-b:a",
            "64k",
        ],
    },
    Recipe {
        output: "clip-hevc.mkv",
        args: &[
            "-vf",
            "scale=trunc(iw/8)*8:trunc(ih/8)*8",
            "-c:v",
            "libkvazaar",
            "-c:a",
            "aac",
            "-b:a",
            "96k",
        ],
    },
    Recipe {
        output: "clip-hevc.mp4",
        args: &[
            "-vf",
            "scale=trunc(iw/8)*8:trunc(ih/8)*8",
            "-c:v",
            "libkvazaar",
            "-c:a",
            "aac",
            "-b:a",
            "96k",
            "-tag:v",
            "hvc1",
        ],
    },
    Recipe {
        output: "clip-hdr10.mkv",
        args: &[
            "-vf",
            "scale=trunc(iw/8)*8:trunc(ih/8)*8,format=yuv420p10le",
            "-c:v",
            "libsvtav1",
            "-preset",
            "10",
            "-crf",
            "40",
            "-pix_fmt",
            "yuv420p10le",
            "-color_primaries",
            "bt2020",
            "-color_trc",
            "smpte2084",
            "-colorspace",
            "bt2020nc",
            "-an",
        ],
    },
    Recipe {
        output: "clip-aac.m4a",
        args: &["-vn", "-c:a", "aac", "-b:a", "128k"],
    },
    Recipe {
        output: "clip-flac.flac",
        args: &["-vn", "-c:a", "flac"],
    },
];

fn fixtures_dir() -> PathBuf {
    let mut directory: &Path = Path::new(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = directory.join(".fixtures");
        if candidate.is_dir() {
            return candidate;
        }
        directory = directory
            .parent()
            .unwrap_or_else(|| panic!("no .fixtures/ directory above the video crate"));
    }
}

fn ffmpeg_binary(fixtures: &Path) -> PathBuf {
    let tools = fixtures.join("tools");
    let entries = std::fs::read_dir(&tools).unwrap_or_else(|error| {
        panic!(
            "cannot read {}: {error}\nsee docs/design/ffmpeg.md for how to obtain FFmpeg",
            tools.display()
        )
    });
    for entry in entries.flatten() {
        for candidate in [
            entry.path().join("bin").join("ffmpeg.exe"),
            entry.path().join("bin").join("ffmpeg"),
        ] {
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    panic!(
        "no ffmpeg binary under {}; see docs/design/ffmpeg.md",
        tools.display()
    );
}

fn assert_lgpl_clean(ffmpeg: &Path) {
    let output = Command::new(ffmpeg)
        .args(["-hide_banner", "-version"])
        .output()
        .expect("ffmpeg -version");
    let text = String::from_utf8_lossy(&output.stdout);
    let configuration = text
        .lines()
        .find(|line| line.starts_with("configuration:"))
        .unwrap_or_default();
    let flags: Vec<&str> = configuration.split_whitespace().collect();

    for forbidden in [
        "--enable-gpl",
        "--enable-nonfree",
        "--enable-libx264",
        "--enable-libx265",
    ] {
        assert!(
            !flags.contains(&forbidden),
            "refusing to use a non-LGPL FFmpeg build: {forbidden} is present in {configuration}"
        );
    }
    assert!(
        flags.contains(&"--enable-shared"),
        "expected a shared build, got {configuration}"
    );
}

fn main() {
    let fixtures = fixtures_dir();
    let ffmpeg = ffmpeg_binary(&fixtures);
    assert_lgpl_clean(&ffmpeg);

    let source = fixtures.join(fixtures_source());
    assert!(
        source.is_file(),
        "missing source fixture {}",
        source.display()
    );

    let force = std::env::args().any(|argument| argument == "--force");

    for recipe in RECIPES {
        let output = fixtures.join(recipe.output);
        let existing = std::fs::metadata(&output)
            .map(|meta| meta.len())
            .unwrap_or(0);
        if existing > 0 && !force {
            println!("keep   {}", recipe.output);
            continue;
        }

        let mut command = Command::new(&ffmpeg);
        command
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-y")
            .arg("-ss")
            .arg(CLIP_START)
            .arg("-t")
            .arg(CLIP_SECONDS)
            .arg("-i")
            .arg(&source)
            .args(recipe.args)
            .arg(&output);

        let status = command.status().expect("failed to run ffmpeg");
        assert!(status.success(), "ffmpeg failed for {}", recipe.output);
        let size = std::fs::metadata(&output)
            .map(|meta| meta.len())
            .unwrap_or(0);
        println!("wrote  {} ({size} bytes)", recipe.output);
    }

    make_resolution_change(&fixtures, &ffmpeg, force);
}

const RESOLUTION_CHANGE_SEGMENTS: [(&str, &str); 2] = [("320x240", "1.4"), ("640x480", "2.4")];

fn make_resolution_change(fixtures: &Path, ffmpeg: &Path, force: bool) {
    let output = fixtures.join("clip-resolution-change.ts");
    let existing = std::fs::metadata(&output)
        .map(|meta| meta.len())
        .unwrap_or(0);
    if existing > 0 && !force {
        println!("keep   clip-resolution-change.ts");
        return;
    }

    let mut spliced = Vec::new();
    for (size, offset) in RESOLUTION_CHANGE_SEGMENTS {
        let segment = fixtures.join(format!("segment-{size}.ts"));
        let status = Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
            ])
            .arg(format!("testsrc2=size={size}:rate=10:duration=1"))
            .args([
                "-pix_fmt",
                "yuv422p",
                "-c:v",
                "mpeg2video",
                "-profile:v",
                "0",
            ])
            .args(["-output_ts_offset", offset, "-f", "mpegts"])
            .arg(&segment)
            .status()
            .expect("failed to run ffmpeg");
        assert!(status.success(), "ffmpeg failed for segment {size}");
        spliced.extend(std::fs::read(&segment).expect("read segment"));
        let _ = std::fs::remove_file(&segment);
    }

    std::fs::write(&output, &spliced).expect("write clip-resolution-change.ts");
    println!("wrote  clip-resolution-change.ts ({} bytes)", spliced.len());
}

fn fixtures_source() -> &'static str {
    "tears-of-steel-720p.mov"
}
