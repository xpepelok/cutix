use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};

use cutix_project::store::PROJECT_FILE_NAME;
use cutix_project::{Project, ProjectStore};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::error::{ExportError, Result};

pub const PACKAGE_EXTENSION: &str = "ocut";

pub const PACKAGE_FORMAT_VERSION: u32 = 1;

const MANIFEST_FILE_NAME: &str = "manifest.json";

pub fn package_extension() -> &'static str {
    PACKAGE_EXTENSION
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackageManifest {
    pub format_version: u32,
    pub project_version: u32,
    pub project_id: String,
    pub project_name: String,
    pub created_at: String,
    pub entries: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageStage {
    Collecting,
    Writing,
    Finishing,
}

#[derive(Clone, Copy, Debug)]
pub struct PackageProgress {
    pub stage: PackageStage,
    pub entry: usize,
    pub total_entries: usize,
}

impl PackageProgress {
    pub fn fraction(&self) -> f32 {
        if self.total_entries == 0 {
            return 0.0;
        }
        (self.entry as f32 / self.total_entries as f32).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug)]
pub struct PackageOutcome {
    pub path: PathBuf,
    pub bytes: u64,
    pub entries: usize,
}

/// The media this project links to from outside its own directory, as
/// `(entry name inside the archive, file on this machine)`.
///
/// Assets that already have a copy inside the project directory are skipped: they travel
/// with the rest of the directory. Assets whose linked file cannot be found are an error
/// rather than an omission — a package that silently leaves out media is not portable, and
/// the person exporting it has no way to tell until they open it somewhere else.
fn linked_sources(store: &ProjectStore, project_id: &str) -> Result<Vec<(String, PathBuf)>> {
    let media = cutix_project::MediaStore::for_project(store, project_id);
    let assets = media.list().map_err(package_error)?;
    let mut linked = Vec::new();
    for asset in assets {
        // Ephemeral assets are scratch state, not part of what the project is made of.
        if asset.ephemeral || media.local_file(&asset).is_file() {
            continue;
        }
        let Some(recorded) = asset.source_path.as_deref() else {
            return Err(unresolved_media(&asset, "has no file and no linked source"));
        };
        let outside = PathBuf::from(recorded);
        if !outside.is_file() {
            return Err(unresolved_media(
                &asset,
                &format!("is linked to {recorded}, which is not on this machine"),
            ));
        }
        let Some(name) = media
            .local_file(&asset)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
        else {
            return Err(unresolved_media(&asset, "has no usable file name"));
        };
        linked.push((format!("media/files/{name}"), outside));
    }
    Ok(linked)
}

/// Names the asset that cannot be packaged, so the message points at something the person
/// exporting can actually go and fix.
fn unresolved_media(asset: &cutix_project::MediaAssetData, detail: &str) -> ExportError {
    ExportError::Package(format!(
        "media `{}` ({}) {detail}, so the package would be incomplete",
        asset.name, asset.id
    ))
}

fn package_error(detail: impl std::fmt::Display) -> ExportError {
    ExportError::Package(detail.to_string())
}

pub fn export_package(
    store: &ProjectStore,
    project: &Project,
    destination: &Path,
    on_progress: &mut dyn FnMut(PackageProgress),
) -> Result<PackageOutcome> {
    store.save(project).map_err(package_error)?;

    let root = store.project_directory(&project.metadata.id);
    on_progress(PackageProgress {
        stage: PackageStage::Collecting,
        entry: 0,
        total_entries: 0,
    });
    let mut files = Vec::new();
    collect(&root, &root, &mut files)?;
    files.sort();

    let linked = linked_sources(store, &project.metadata.id)?;
    let total = files.len() + linked.len();

    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| ExportError::Io {
            path: parent.display().to_string(),
            detail: error.to_string(),
        })?;
    }
    let file = File::create(destination).map_err(|error| ExportError::Io {
        path: destination.display().to_string(),
        detail: error.to_string(),
    })?;
    let mut writer = ZipWriter::new(BufWriter::new(file));
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    let manifest = PackageManifest {
        format_version: PACKAGE_FORMAT_VERSION,
        project_version: cutix_project::CURRENT_PROJECT_VERSION,
        project_id: project.metadata.id.clone(),
        project_name: project.metadata.name.clone(),
        created_at: cutix_project::now_iso(),
        entries: total,
    };
    writer
        .start_file(MANIFEST_FILE_NAME, options)
        .map_err(package_error)?;
    writer
        .write_all(&serde_json::to_vec_pretty(&manifest).map_err(package_error)?)
        .map_err(package_error)?;

    let mut buffer = Vec::new();
    for (index, relative) in files.iter().enumerate() {
        on_progress(PackageProgress {
            stage: PackageStage::Writing,
            entry: index,
            total_entries: total,
        });
        buffer.clear();
        let source = root.join(relative);
        File::open(&source)
            .and_then(|mut handle| handle.read_to_end(&mut buffer))
            .map_err(|error| ExportError::Io {
                path: source.display().to_string(),
                detail: error.to_string(),
            })?;
        writer
            .start_file(zip_name(relative), options)
            .map_err(package_error)?;
        writer.write_all(&buffer).map_err(package_error)?;
    }

    for (index, (relative, source)) in linked.iter().enumerate() {
        on_progress(PackageProgress {
            stage: PackageStage::Writing,
            entry: files.len() + index,
            total_entries: total,
        });
        buffer.clear();
        File::open(source)
            .and_then(|mut handle| handle.read_to_end(&mut buffer))
            .map_err(|error| ExportError::Io {
                path: source.display().to_string(),
                detail: error.to_string(),
            })?;
        writer
            .start_file(relative.clone(), options)
            .map_err(package_error)?;
        writer.write_all(&buffer).map_err(package_error)?;
    }

    on_progress(PackageProgress {
        stage: PackageStage::Finishing,
        entry: total,
        total_entries: total,
    });
    let mut inner = writer.finish().map_err(package_error)?;
    inner.flush().map_err(|error| ExportError::Io {
        path: destination.display().to_string(),
        detail: error.to_string(),
    })?;
    drop(inner);

    let bytes = std::fs::metadata(destination)
        .map(|meta| meta.len())
        .unwrap_or(0);
    Ok(PackageOutcome {
        path: destination.to_path_buf(),
        bytes,
        entries: total,
    })
}

/// Reads a package into the library as a new project.
///
/// The extraction is transactional: everything lands in a temporary directory beside the
/// final one and is validated there, and only a complete, parseable project is moved into
/// place. A malformed archive therefore leaves no half-written project in the library.
///
/// Media that the package carries a copy of is re-pointed at that copy. The source path
/// recorded when the project was exported names a file on the machine it came from; on
/// this machine that path is either absent or, worse, a different file. See
/// `repoint_media_at_local_copies`.
pub fn import_package(store: &ProjectStore, archive: &Path) -> Result<Project> {
    let file = File::open(archive).map_err(|error| ExportError::Io {
        path: archive.display().to_string(),
        detail: error.to_string(),
    })?;
    let mut zip = ZipArchive::new(BufReader::new(file)).map_err(package_error)?;
    let manifest = read_manifest(&mut zip)?;
    if manifest.format_version > PACKAGE_FORMAT_VERSION {
        return Err(package_error(format!(
            "this file was written by a newer version (format {} > {PACKAGE_FORMAT_VERSION})",
            manifest.format_version
        )));
    }

    let mut id = manifest.project_id.clone();
    if id.trim().is_empty() || store.exists(&id) {
        id = uuid::Uuid::new_v4().to_string();
    }
    let root = store.project_directory(&id);
    // A sibling of the final directory, so the commit is a rename within one filesystem.
    let staging = staging_directory(&root);
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|error| ExportError::Io {
        path: staging.display().to_string(),
        detail: error.to_string(),
    })?;

    let staged = extract_into(&mut zip, &staging)
        .and_then(|()| validate_staged(&staging))
        .and_then(|()| repoint_media_at_local_copies(&staging));
    if let Err(error) = staged {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error);
    }

    // Commit. Nothing has been written under the final path until this point.
    if let Some(parent) = root.parent() {
        std::fs::create_dir_all(parent).map_err(|error| ExportError::Io {
            path: parent.display().to_string(),
            detail: error.to_string(),
        })?;
    }
    if let Err(error) = std::fs::rename(&staging, &root) {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(ExportError::Io {
            path: root.display().to_string(),
            detail: error.to_string(),
        });
    }

    let loaded = match store.load(&id) {
        Ok(loaded) => loaded,
        Err(error) => {
            // The staged copy validated but the store still refuses it. Do not leave a
            // project the library cannot open sitting in the library.
            let _ = std::fs::remove_dir_all(&root);
            return Err(package_error(error));
        }
    };
    let mut project = loaded.project;
    if project.metadata.id != id {
        project.metadata.id = id;
        store.save(&project).map_err(package_error)?;
    }
    Ok(project)
}

/// Where a package is unpacked before it is allowed into the library.
fn staging_directory(root: &Path) -> PathBuf {
    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_owned());
    let parent = root.parent().unwrap_or(root);
    parent.join(format!(".importing-{name}"))
}

/// Unpacks every file in the archive under `staging`.
///
/// Entries whose name escapes the archive root are skipped: `enclosed_name` answers `None`
/// for an absolute path or one containing a parent-directory component, which is how a
/// package could otherwise write outside the project directory.
fn extract_into<R: Read + Seek>(zip: &mut ZipArchive<R>, staging: &Path) -> Result<()> {
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(package_error)?;
        if entry.is_dir() {
            continue;
        }

        let Some(relative) = entry.enclosed_name() else {
            continue;
        };
        if relative == Path::new(MANIFEST_FILE_NAME) {
            continue;
        }
        let target = staging.join(&relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| ExportError::Io {
                path: parent.display().to_string(),
                detail: error.to_string(),
            })?;
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(package_error)?;
        cutix_project::store::write_atomic(&target, &bytes).map_err(package_error)?;
    }
    Ok(())
}

/// Checks that the staged directory holds something the library will be able to open.
///
/// Runs before the commit so that a truncated or hand-edited archive is rejected while the
/// only thing on disk is a temporary directory we are about to delete.
fn validate_staged(staging: &Path) -> Result<()> {
    let document = staging.join(PROJECT_FILE_NAME);
    if !document.is_file() {
        return Err(package_error("the archive has no project.json"));
    }
    let bytes = std::fs::read(&document).map_err(|error| ExportError::Io {
        path: document.display().to_string(),
        detail: error.to_string(),
    })?;
    let raw: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| package_error(format!("project.json is not valid JSON: {error}")))?;
    // Migration is what the store would run on load, so run it here where a failure is
    // still recoverable.
    let (migrated, _report) = cutix_project::migrate_to_current(raw, &cutix_project::now_iso());
    serde_json::from_value::<Project>(migrated).map_err(|error| {
        package_error(format!(
            "project.json does not describe a project this version understands: {error}"
        ))
    })?;

    let metadata = staging.join("media").join("metadata");
    if !metadata.is_dir() {
        return Ok(());
    }
    let entries = std::fs::read_dir(&metadata).map_err(|error| ExportError::Io {
        path: metadata.display().to_string(),
        detail: error.to_string(),
    })?;
    for entry in entries {
        let path = entry
            .map_err(|error| ExportError::Io {
                path: metadata.display().to_string(),
                detail: error.to_string(),
            })?
            .path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let bytes = std::fs::read(&path).map_err(|error| ExportError::Io {
            path: path.display().to_string(),
            detail: error.to_string(),
        })?;
        serde_json::from_slice::<cutix_project::MediaAssetData>(&bytes).map_err(|error| {
            package_error(format!(
                "media metadata {} is not readable: {error}",
                path.display()
            ))
        })?;
    }
    Ok(())
}

/// Drops the recorded source path of any asset the package carries a copy of.
///
/// That path was absolute on the machine that exported the package. Here it either points
/// at nothing or, in the case that actually loses work, at an unrelated file that happens
/// to sit at the same location. The copy inside the project directory is the one this
/// project means, so the link is removed and the local copy resolves instead.
fn repoint_media_at_local_copies(staging: &Path) -> Result<()> {
    let media = cutix_project::MediaStore::new(staging.join("media"));
    let Ok(assets) = media.list() else {
        // Validation already accepted the metadata; a project with no media directory has
        // nothing to repoint.
        return Ok(());
    };
    for mut asset in assets {
        if asset.source_path.is_none() {
            continue;
        }
        if !media.local_file(&asset).is_file() {
            // No copy travelled with the package, so the link is all this asset has.
            continue;
        }
        asset.source_path = None;
        media.write_metadata(&asset).map_err(package_error)?;
    }
    Ok(())
}

fn read_manifest<R: Read + Seek>(zip: &mut ZipArchive<R>) -> Result<PackageManifest> {
    let mut entry = zip
        .by_name(MANIFEST_FILE_NAME)
        .map_err(|_| package_error("the archive has no manifest.json"))?;
    let mut bytes = Vec::new();
    entry.read_to_end(&mut bytes).map_err(package_error)?;
    serde_json::from_slice(&bytes).map_err(package_error)
}

fn collect(root: &Path, directory: &Path, into: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(directory).map_err(|error| ExportError::Io {
        path: directory.display().to_string(),
        detail: error.to_string(),
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| ExportError::Io {
            path: directory.display().to_string(),
            detail: error.to_string(),
        })?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            collect(root, &path, into)?;
        } else if let Ok(relative) = path.strip_prefix(root) {
            into.push(relative.to_path_buf());
        }
    }
    Ok(())
}

fn zip_name(relative: &Path) -> String {
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_names_use_forward_slashes_on_every_host() {
        assert_eq!(
            zip_name(Path::new("media").join("files").join("a.mp4").as_path()),
            "media/files/a.mp4"
        );
    }

    #[test]
    fn a_progress_fraction_is_bounded() {
        let progress = PackageProgress {
            stage: PackageStage::Writing,
            entry: 3,
            total_entries: 4,
        };
        assert!((progress.fraction() - 0.75).abs() < f32::EPSILON);
        assert_eq!(
            PackageProgress {
                stage: PackageStage::Collecting,
                entry: 0,
                total_entries: 0
            }
            .fraction(),
            0.0
        );
    }
}
