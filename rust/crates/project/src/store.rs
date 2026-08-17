use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{io_error, ProjectError, Result};
use crate::migrate::{self, MigrationReport};
use crate::model::{Project, ProjectSummary, TimelineElement};

pub const PROJECT_FILE_NAME: &str = "project.json";
pub const SUMMARY_FILE_NAME: &str = "summary.json";
pub const THUMBNAIL_FILE_NAME: &str = "thumbnail.png";
pub const PROJECT_THUMBNAIL_MAX_EDGE: u32 = 480;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocumentStamp {
    size: u64,
    modified_nanos: u128,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SummarySidecar {
    stamp: DocumentStamp,
    summary: ProjectSummary,
}

fn stamp_of(path: &Path) -> Option<DocumentStamp> {
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(DocumentStamp {
        size: metadata.len(),
        modified_nanos: modified,
    })
}

fn cutout_of(element: &TimelineElement) -> Option<&Value> {
    match element {
        TimelineElement::Video(inner) => inner.cutout.as_ref(),
        TimelineElement::Image(inner) => inner.cutout.as_ref(),
        _ => None,
    }
}

fn holds_inline_png(value: &Value) -> bool {
    value
        .get("png")
        .and_then(Value::as_str)
        .is_some_and(|text| !text.is_empty())
}

fn has_inline_mattes(project: &Project) -> bool {
    project.scenes.iter().any(|scene| {
        scene.tracks.all().any(|track| {
            track.elements().iter().any(|element| {
                let Some(cutout) = cutout_of(element) else {
                    return false;
                };
                if holds_inline_png(cutout) {
                    return true;
                }
                cutout
                    .get("frames")
                    .and_then(Value::as_array)
                    .is_some_and(|frames| frames.iter().any(holds_inline_png))
            })
        })
    })
}

#[derive(Debug, Clone)]
pub struct ProjectStore {
    root: PathBuf,
}

impl ProjectStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn with_app_data_directory() -> Result<Self> {
        let base = dirs::data_local_dir().ok_or(ProjectError::NoAppDataDirectory)?;
        Ok(Self::new(base.join("cutix")))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn projects_directory(&self) -> PathBuf {
        self.root.join("projects")
    }

    pub fn project_directory(&self, project_id: &str) -> PathBuf {
        self.projects_directory().join(project_id)
    }

    pub fn project_file(&self, project_id: &str) -> PathBuf {
        self.project_directory(project_id).join(PROJECT_FILE_NAME)
    }

    pub fn thumbnail_file(&self, project_id: &str) -> PathBuf {
        self.project_directory(project_id).join(THUMBNAIL_FILE_NAME)
    }

    pub fn media_directory(&self, project_id: &str) -> PathBuf {
        self.project_directory(project_id).join("media")
    }

    pub fn matte_directory(&self, project_id: &str) -> PathBuf {
        self.project_directory(project_id)
            .join(crate::mattes::MATTE_DIRECTORY_NAME)
    }

    pub fn externalize_mattes(&self, project: &mut Project) -> Result<usize> {
        crate::mattes::externalize(project, &self.matte_directory(&project.metadata.id))
    }

    pub fn list_project_ids(&self) -> Result<Vec<String>> {
        let directory = self.projects_directory();
        if !directory.exists() {
            return Ok(Vec::new());
        }

        let mut ids = Vec::new();
        for entry in fs::read_dir(&directory).map_err(io_error(&directory))? {
            let entry = entry.map_err(io_error(&directory))?;
            if !entry.path().join(PROJECT_FILE_NAME).is_file() {
                continue;
            }
            if let Some(name) = entry.file_name().to_str() {
                ids.push(name.to_string());
            }
        }
        ids.sort();
        Ok(ids)
    }

    pub fn summary_file(&self, project_id: &str) -> PathBuf {
        self.project_directory(project_id).join(SUMMARY_FILE_NAME)
    }

    fn read_sidecar(&self, project_id: &str, stamp: &DocumentStamp) -> Option<ProjectSummary> {
        let bytes = fs::read(self.summary_file(project_id)).ok()?;
        let sidecar: SummarySidecar = serde_json::from_slice(&bytes).ok()?;
        (sidecar.stamp == *stamp).then_some(sidecar.summary)
    }

    fn write_sidecar(&self, project_id: &str, summary: &ProjectSummary) {
        let Some(stamp) = stamp_of(&self.project_file(project_id)) else {
            return;
        };
        let sidecar = SummarySidecar {
            stamp,
            summary: summary.clone(),
        };
        if let Ok(bytes) = serde_json::to_vec(&sidecar) {
            let _ = write_atomic(&self.summary_file(project_id), &bytes);
        }
    }

    pub fn summary(&self, project_id: &str) -> Result<ProjectSummary> {
        let Some(stamp) = stamp_of(&self.project_file(project_id)) else {
            return Err(ProjectError::NotFound {
                id: project_id.to_string(),
            });
        };
        if let Some(summary) = self.read_sidecar(project_id, &stamp) {
            return Ok(summary);
        }
        let summary = self.load(project_id)?.project.summary();
        self.write_sidecar(project_id, &summary);
        Ok(summary)
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectSummary>> {
        let mut summaries = Vec::new();
        for id in self.list_project_ids()? {
            match self.summary(&id) {
                Ok(summary) => summaries.push(summary),
                Err(ProjectError::NotFound { .. }) => continue,
                Err(error) => return Err(error),
            }
        }
        summaries.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        Ok(summaries)
    }

    pub fn exists(&self, project_id: &str) -> bool {
        self.project_file(project_id).is_file()
    }

    pub fn load_raw(&self, project_id: &str) -> Result<Value> {
        let path = self.project_file(project_id);
        if !path.is_file() {
            return Err(ProjectError::NotFound {
                id: project_id.to_string(),
            });
        }
        let bytes = fs::read(&path).map_err(io_error(&path))?;
        serde_json::from_slice(&bytes).map_err(|error| ProjectError::Json {
            path,
            detail: error.to_string(),
        })
    }

    pub fn load(&self, project_id: &str) -> Result<LoadedProject> {
        let raw = self.load_raw(project_id)?;
        let (migrated, report) = migrate::migrate_to_current(raw, &now_iso());
        let project = serde_json::from_value(migrated).map_err(|error| ProjectError::Json {
            path: self.project_file(project_id),
            detail: error.to_string(),
        })?;
        Ok(LoadedProject { project, report })
    }

    pub fn save(&self, project: &Project) -> Result<()> {
        let directory = self.project_directory(&project.metadata.id);
        fs::create_dir_all(&directory).map_err(io_error(&directory))?;

        let mattes = self.matte_directory(&project.metadata.id);
        let externalized;
        let document = if has_inline_mattes(project) {
            let mut copy = project.clone();
            crate::mattes::externalize(&mut copy, &mattes)?;
            externalized = copy;
            &externalized
        } else {
            project
        };
        let _ = crate::mattes::sweep_orphans(document, &mattes);
        let bytes = serde_json::to_vec(document).map_err(|error| ProjectError::Json {
            path: self.project_file(&project.metadata.id),
            detail: error.to_string(),
        })?;
        write_atomic(&self.project_file(&project.metadata.id), &bytes)?;
        self.write_sidecar(&project.metadata.id, &document.summary());
        Ok(())
    }

    pub fn create(&self, name: impl Into<String>) -> Result<Project> {
        let project = Project::new(name, now_iso());
        if self.exists(&project.metadata.id) {
            return Err(ProjectError::AlreadyExists {
                id: project.metadata.id,
            });
        }
        self.save(&project)?;
        Ok(project)
    }

    pub fn rename(&self, project_id: &str, name: impl Into<String>) -> Result<Project> {
        let mut project = self.load(project_id)?.project;
        project.metadata.name = name.into();
        project.metadata.updated_at = now_iso();
        self.save(&project)?;
        Ok(project)
    }

    pub fn duplicate(&self, project_id: &str) -> Result<Project> {
        let mut project = self.load(project_id)?.project;
        let now = now_iso();
        project.metadata.id = uuid::Uuid::new_v4().to_string();
        project.metadata.name = format!("{} (copy)", project.metadata.name);
        project.metadata.created_at = now.clone();
        project.metadata.updated_at = now;
        self.save(&project)?;

        let source_media = self.media_directory(project_id);
        if source_media.is_dir() {
            copy_directory(&source_media, &self.media_directory(&project.metadata.id))?;
        }
        let source_mattes = self.matte_directory(project_id);
        if source_mattes.is_dir() {
            copy_directory(&source_mattes, &self.matte_directory(&project.metadata.id))?;
        }
        let source_thumbnail = self.thumbnail_file(project_id);
        if source_thumbnail.is_file() {
            let target = self.thumbnail_file(&project.metadata.id);
            fs::copy(&source_thumbnail, &target).map_err(io_error(&target))?;
        }

        Ok(project)
    }

    pub fn delete(&self, project_id: &str) -> Result<()> {
        let directory = self.project_directory(project_id);
        if !directory.exists() {
            return Err(ProjectError::NotFound {
                id: project_id.to_string(),
            });
        }
        fs::remove_dir_all(&directory).map_err(io_error(&directory))
    }

    pub fn import_document(&self, document: Value) -> Result<Project> {
        let (migrated, _) = migrate::migrate_to_current(document, &now_iso());
        let project: Project =
            serde_json::from_value(migrated).map_err(|error| ProjectError::Json {
                path: self.projects_directory(),
                detail: error.to_string(),
            })?;
        self.save(&project)?;
        Ok(project)
    }

    pub fn save_thumbnail(&self, project_id: &str, png_bytes: &[u8]) -> Result<PathBuf> {
        let directory = self.project_directory(project_id);
        fs::create_dir_all(&directory).map_err(io_error(&directory))?;
        let path = self.thumbnail_file(project_id);
        write_atomic(&path, png_bytes)?;
        Ok(path)
    }

    pub fn save_thumbnail_from_rgba(
        &self,
        project_id: &str,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<PathBuf> {
        let expected = (width as usize) * (height as usize) * 4;
        if width == 0 || height == 0 || rgba.len() < expected {
            return Err(ProjectError::Probe {
                detail: format!("frame {width}x{height} does not match {} bytes", rgba.len()),
            });
        }
        let image = image::RgbaImage::from_raw(width, height, rgba[..expected].to_vec()).ok_or(
            ProjectError::Probe {
                detail: "frame buffer could not be wrapped".to_string(),
            },
        )?;

        let scale = f64::from(PROJECT_THUMBNAIL_MAX_EDGE) / f64::from(width.max(height));
        let scaled = if scale < 1.0 {
            image::imageops::resize(
                &image,
                ((f64::from(width) * scale).round() as u32).max(1),
                ((f64::from(height) * scale).round() as u32).max(1),
                image::imageops::FilterType::Triangle,
            )
        } else {
            image
        };

        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgba8(scaled)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .map_err(|error| ProjectError::Probe {
                detail: error.to_string(),
            })?;

        self.save_thumbnail(project_id, &bytes)
    }
}

#[derive(Debug, Clone)]
pub struct LoadedProject {
    pub project: Project,
    pub report: MigrationReport,
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(directory).map_err(io_error(directory))?;

    let temp_path = directory.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("file"),
        uuid::Uuid::new_v4()
    ));

    {
        let mut file = fs::File::create(&temp_path).map_err(io_error(&temp_path))?;
        file.write_all(bytes).map_err(io_error(&temp_path))?;
        file.sync_all().map_err(io_error(&temp_path))?;
    }

    match fs::rename(&temp_path, path) {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = fs::remove_file(&temp_path);
            Err(io_error(path)(error))
        }
    }
}

fn copy_directory(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target).map_err(io_error(target))?;
    for entry in fs::read_dir(source).map_err(io_error(source))? {
        let entry = entry.map_err(io_error(source))?;
        let entry_target = target.join(entry.file_name());
        if entry.path().is_dir() {
            copy_directory(&entry.path(), &entry_target)?;
        } else {
            fs::copy(entry.path(), &entry_target).map_err(io_error(&entry_target))?;
        }
    }
    Ok(())
}

pub fn now_iso() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format_iso(now.as_secs() as i64, now.subsec_millis())
}

fn format_iso(unix_seconds: i64, milliseconds: u32) -> String {
    let days = unix_seconds.div_euclid(86_400);
    let seconds_of_day = unix_seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{milliseconds:03}Z",
        seconds_of_day / 3600,
        (seconds_of_day % 3600) / 60,
        seconds_of_day % 60,
    )
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_epoch() {
        assert_eq!(format_iso(0, 0), "1970-01-01T00:00:00.000Z");
        assert_eq!(format_iso(1_700_000_000, 123), "2023-11-14T22:13:20.123Z");
    }
}
