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
    let total = files.len();

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
    std::fs::create_dir_all(&root).map_err(|error| ExportError::Io {
        path: root.display().to_string(),
        detail: error.to_string(),
    })?;

    let mut saw_document = false;
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
        if relative == Path::new(PROJECT_FILE_NAME) {
            saw_document = true;
        }
        let target = root.join(&relative);
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

    if !saw_document {
        let _ = std::fs::remove_dir_all(&root);
        return Err(package_error("the archive has no project.json"));
    }

    let mut project = store.load(&id).map_err(package_error)?.project;
    if project.metadata.id != id {
        project.metadata.id = id;
        store.save(&project).map_err(package_error)?;
    }
    Ok(project)
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
