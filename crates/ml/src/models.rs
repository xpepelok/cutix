use std::fs;
use std::path::{Path, PathBuf};

use crate::MlError;

#[derive(Clone, Copy, Debug)]
pub struct ModelSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub license: &'static str,
    pub url: &'static str,
    pub file_name: &'static str,
    pub input_size: usize,
    pub approximate_size_mb: u32,
}

pub const SEGMENTATION_MODELS: &[ModelSpec] = &[ModelSpec {
    key: "modnet",
    label: "MODNet",
    license: "Apache-2.0",
    url: "https://huggingface.co/Xenova/modnet/resolve/main/onnx/model.onnx",
    file_name: "modnet.onnx",
    input_size: 512,
    approximate_size_mb: 25,
}];

pub fn find_model(key: &str) -> Option<&'static ModelSpec> {
    SEGMENTATION_MODELS.iter().find(|model| model.key == key)
}

pub fn cache_directory() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".cache"))
        })
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("cutix").join("models")
}

pub fn cached_path(model: &ModelSpec) -> PathBuf {
    cache_directory().join(model.file_name)
}

pub fn is_cached(model: &ModelSpec) -> bool {
    let path = cached_path(model);
    path.is_file()
        && fs::metadata(&path)
            .map(|meta| meta.len() > 1024)
            .unwrap_or(false)
}

pub fn ensure_downloaded(
    model: &ModelSpec,
    mut on_progress: impl FnMut(f32),
) -> Result<PathBuf, MlError> {
    let target = cached_path(model);
    if is_cached(model) {
        on_progress(1.0);
        return Ok(target);
    }

    crate::download::download_to(model.url, &target, &mut on_progress, &|| false)?;
    Ok(target)
}

pub fn cached_size_mb(model: &ModelSpec) -> Option<u64> {
    fs::metadata(cached_path(model))
        .ok()
        .map(|meta| meta.len() / (1024 * 1024))
}

pub fn remove_cached(model: &ModelSpec) -> Result<(), MlError> {
    let path = cached_path(model);
    if path.exists() {
        fs::remove_file(&path).map_err(|error| MlError::Download(error.to_string()))?;
    }
    Ok(())
}

pub fn describe(model: &ModelSpec) -> String {
    format!(
        "{} · {} · ~{} MB",
        model.label, model.license, model.approximate_size_mb
    )
}

pub fn is_inside_cache(path: &Path) -> bool {
    path.starts_with(cache_directory())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_not_empty() {
        assert!(!SEGMENTATION_MODELS.is_empty());
    }

    #[test]
    fn every_model_has_a_permissive_licence() {
        for model in SEGMENTATION_MODELS {
            assert!(
                model.license.contains("Apache") || model.license.contains("MIT"),
                "{} has licence {}",
                model.key,
                model.license
            );
        }
    }

    #[test]
    fn models_are_looked_up_by_key() {
        assert!(find_model("modnet").is_some());
        assert!(find_model("nope").is_none());
    }

    #[test]
    fn cached_path_sits_in_the_cache_directory() {
        let model = find_model("modnet").expect("model");
        assert!(is_inside_cache(&cached_path(model)));
    }

    #[test]
    fn description_mentions_the_licence() {
        let model = find_model("modnet").expect("model");
        assert!(describe(model).contains("Apache-2.0"));
    }
}
