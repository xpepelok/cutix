use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{io_error, ProjectError, Result};
use crate::model::{MediaAssetData, MediaType};
use crate::probe::{self, ProbeResult};
use crate::store::{write_atomic, ProjectStore};

pub const THUMBNAIL_MAX_EDGE: u32 = 320;

#[derive(Debug, Clone)]
pub struct MediaStore {
    root: PathBuf,
}

impl MediaStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn for_project(store: &ProjectStore, project_id: &str) -> Self {
        Self::new(store.media_directory(project_id))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn metadata_directory(&self) -> PathBuf {
        self.root.join("metadata")
    }

    pub fn files_directory(&self) -> PathBuf {
        self.root.join("files")
    }

    pub fn thumbnails_directory(&self) -> PathBuf {
        self.root.join("thumbnails")
    }

    pub fn metadata_file(&self, media_id: &str) -> PathBuf {
        self.metadata_directory().join(format!("{media_id}.json"))
    }

    pub fn thumbnail_file(&self, media_id: &str) -> PathBuf {
        self.thumbnails_directory().join(format!("{media_id}.png"))
    }

    pub fn source_file(&self, asset: &MediaAssetData) -> PathBuf {
        let name = asset.file_name.clone().unwrap_or_else(|| asset.id.clone());
        self.files_directory().join(name)
    }

    pub fn list(&self) -> Result<Vec<MediaAssetData>> {
        let directory = self.metadata_directory();
        if !directory.exists() {
            return Ok(Vec::new());
        }

        let mut assets = Vec::new();
        for entry in fs::read_dir(&directory).map_err(io_error(&directory))? {
            let path = entry.map_err(io_error(&directory))?.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let bytes = fs::read(&path).map_err(io_error(&path))?;
            let asset: MediaAssetData =
                serde_json::from_slice(&bytes).map_err(|error| ProjectError::Json {
                    path: path.clone(),
                    detail: error.to_string(),
                })?;
            assets.push(asset);
        }
        assets.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(assets)
    }

    pub fn get(&self, media_id: &str) -> Result<MediaAssetData> {
        let path = self.metadata_file(media_id);
        if !path.is_file() {
            return Err(ProjectError::NotFound {
                id: media_id.to_string(),
            });
        }
        let bytes = fs::read(&path).map_err(io_error(&path))?;
        serde_json::from_slice(&bytes).map_err(|error| ProjectError::Json {
            path,
            detail: error.to_string(),
        })
    }

    pub fn write_metadata(&self, asset: &MediaAssetData) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(asset).map_err(|error| ProjectError::Json {
            path: self.metadata_file(&asset.id),
            detail: error.to_string(),
        })?;
        write_atomic(&self.metadata_file(&asset.id), &bytes)
    }

    pub fn remove(&self, media_id: &str) -> Result<()> {
        let asset = self.get(media_id)?;
        let source = self.source_file(&asset);
        if source.is_file() {
            fs::remove_file(&source).map_err(io_error(&source))?;
        }
        let thumbnail = self.thumbnail_file(media_id);
        if thumbnail.is_file() {
            fs::remove_file(&thumbnail).map_err(io_error(&thumbnail))?;
        }
        let metadata = self.metadata_file(media_id);
        fs::remove_file(&metadata).map_err(io_error(&metadata))
    }

    pub fn import(&self, source: &Path) -> Result<MediaAssetData> {
        let probe_result = probe::probe(source)?;
        let metadata = fs::metadata(source).map_err(io_error(source))?;
        let media_id = uuid::Uuid::new_v4().to_string();
        let extension = probe::extension_of(source);
        let file_name = if extension.is_empty() {
            media_id.clone()
        } else {
            format!("{media_id}.{extension}")
        };

        let files_directory = self.files_directory();
        fs::create_dir_all(&files_directory).map_err(io_error(&files_directory))?;
        let target = files_directory.join(&file_name);
        fs::copy(source, &target).map_err(io_error(&target))?;

        let last_modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0);

        let mut asset = MediaAssetData {
            id: media_id.clone(),
            name: source
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(&media_id)
                .to_string(),
            media_type: probe_result.media_type,
            size: metadata.len(),
            last_modified,
            width: probe_result.width,
            height: probe_result.height,
            duration: probe_result.duration,
            fps: probe_result.fps,
            has_audio: probe_result.has_audio,
            ephemeral: false,
            thumbnail_url: None,
            file_name: Some(file_name),
        };

        if let Ok(Some(path)) = self.generate_thumbnail(&media_id, &target, &probe_result) {
            asset.thumbnail_url = path
                .strip_prefix(&self.root)
                .ok()
                .map(|relative| relative.to_string_lossy().replace('\\', "/"));
        }

        self.write_metadata(&asset)?;
        Ok(asset)
    }

    pub fn generate_thumbnail(
        &self,
        media_id: &str,
        source: &Path,
        probe_result: &ProbeResult,
    ) -> Result<Option<PathBuf>> {
        let image = match probe_result.media_type {
            MediaType::Image => image::open(source)
                .map_err(|error| ProjectError::Probe {
                    detail: error.to_string(),
                })?
                .to_rgba8(),
            MediaType::Video => {
                let Some(frame) = decode_first_frame(source) else {
                    return Ok(None);
                };
                frame
            }
            MediaType::Audio => return Ok(None),
        };

        let (width, height) = (image.width(), image.height());
        if width == 0 || height == 0 {
            return Ok(None);
        }
        let scale = f64::from(THUMBNAIL_MAX_EDGE) / f64::from(width.max(height));
        let thumbnail = if scale < 1.0 {
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
        image::DynamicImage::ImageRgba8(thumbnail)
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .map_err(|error| ProjectError::Probe {
                detail: error.to_string(),
            })?;

        let path = self.thumbnail_file(media_id);
        write_atomic(&path, &bytes)?;
        Ok(Some(path))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn decode_first_frame(source: &Path) -> Option<image::RgbaImage> {
    let frame = video::first_frame(source).ok()?;
    image::RgbaImage::from_raw(frame.width as u32, frame.height as u32, frame.rgba)
}

#[cfg(target_arch = "wasm32")]
fn decode_first_frame(_source: &Path) -> Option<image::RgbaImage> {
    None
}
