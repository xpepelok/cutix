use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::error::{io_error, Result};
use crate::model::{Project, TimelineElement};
use crate::store::write_atomic;

pub const MATTE_DIRECTORY_NAME: &str = "mattes";

fn matte_file_name(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{MATTE_DIRECTORY_NAME}/{hash:016x}{:08x}.png", bytes.len())
}

fn cutout_of_mut(element: &mut TimelineElement) -> Option<&mut Value> {
    match element {
        TimelineElement::Video(inner) => inner.cutout.as_mut(),
        TimelineElement::Image(inner) => inner.cutout.as_mut(),
        _ => None,
    }
}

fn externalize_one(
    holder: &mut Value,
    directory: &Path,
    written: &mut HashMap<String, String>,
) -> Result<bool> {
    let Some(object) = holder.as_object_mut() else {
        return Ok(false);
    };
    let encoded = match object.get("png").and_then(Value::as_str) {
        Some(text) if !text.is_empty() => text.to_owned(),
        _ => return Ok(false),
    };

    let relative = match written.get(&encoded) {
        Some(existing) => existing.clone(),
        None => {
            let Some(bytes) = crate::base64::decode(&encoded) else {
                return Ok(false);
            };
            let relative = matte_file_name(&bytes);
            let path = directory.join(
                relative
                    .strip_prefix(&format!("{MATTE_DIRECTORY_NAME}/"))
                    .unwrap_or(&relative),
            );
            if !path.is_file() {
                fs::create_dir_all(directory).map_err(io_error(directory))?;
                write_atomic(&path, &bytes)?;
            }
            written.insert(encoded, relative.clone());
            relative
        }
    };

    object.remove("png");
    object.insert(String::from("pngPath"), Value::String(relative));
    Ok(true)
}

pub fn externalize(project: &mut Project, directory: &Path) -> Result<usize> {
    let mut moved = 0usize;
    let mut written: HashMap<String, String> = HashMap::new();
    for scene in &mut project.scenes {
        let tracks = std::iter::once(&mut scene.tracks.main)
            .chain(scene.tracks.overlay.iter_mut())
            .chain(scene.tracks.audio.iter_mut());
        for track in tracks {
            for element in track.elements_mut() {
                let Some(cutout) = cutout_of_mut(element) else {
                    continue;
                };
                if externalize_one(cutout, directory, &mut written)? {
                    moved += 1;
                }
                let Some(frames) = cutout.get_mut("frames").and_then(Value::as_array_mut) else {
                    continue;
                };
                for frame in frames {
                    if externalize_one(frame, directory, &mut written)? {
                        moved += 1;
                    }
                }
            }
        }
    }
    Ok(moved)
}

pub fn referenced_files(project: &Project) -> Vec<String> {
    let mut paths = Vec::new();
    let mut collect = |value: &Value| {
        if let Some(path) = value.get("pngPath").and_then(Value::as_str) {
            paths.push(path.to_owned());
        }
    };
    for scene in &project.scenes {
        for track in scene.tracks.all() {
            for element in track.elements() {
                let cutout = match element {
                    TimelineElement::Video(inner) => inner.cutout.as_ref(),
                    TimelineElement::Image(inner) => inner.cutout.as_ref(),
                    _ => None,
                };
                let Some(cutout) = cutout else { continue };
                collect(cutout);
                if let Some(frames) = cutout.get("frames").and_then(Value::as_array) {
                    for frame in frames {
                        collect(frame);
                    }
                }
            }
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

pub fn sweep_orphans(project: &Project, directory: &Path) -> Result<usize> {
    if !directory.is_dir() {
        return Ok(0);
    }
    let kept: std::collections::HashSet<String> = referenced_files(project)
        .into_iter()
        .filter_map(|path| {
            path.rsplit(['/', '\\'])
                .next()
                .map(|name| name.to_ascii_lowercase())
        })
        .collect();

    let mut removed = 0usize;
    for entry in fs::read_dir(directory).map_err(io_error(directory))? {
        let path = entry.map_err(io_error(directory))?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("png") {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if kept.contains(&name.to_ascii_lowercase()) {
            continue;
        }
        if fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_bytes_land_in_one_file() {
        assert_eq!(matte_file_name(b"abc"), matte_file_name(b"abc"));
        assert_ne!(matte_file_name(b"abc"), matte_file_name(b"abd"));
        assert!(matte_file_name(b"abc").starts_with("mattes/"));
        assert!(matte_file_name(b"abc").ends_with(".png"));
    }

    #[test]
    fn a_missing_directory_sweeps_nothing() {
        let project = Project::new("sweep", "2026-01-01T00:00:00.000Z".to_owned());
        let directory = std::env::temp_dir().join("cutix-no-such-matte-directory");
        assert_eq!(sweep_orphans(&project, &directory).unwrap(), 0);
    }
}
