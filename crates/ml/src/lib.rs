pub mod matte;
mod mel;
pub mod models;
mod tokenizer;
mod whisper;

pub use matte::{
    CUTOUT_SAMPLE_RATES, CutoutMatteFrame, CutoutMode, DEFAULT_CUTOUT_SAMPLE_RATE, ElementCutout,
    MAX_CUTOUT_MATTE_FRAMES, MAX_SEGMENTATION_SIZE, MatteSource, base64_decode, base64_encode,
    decode_matte_png, encode_matte_png, matte_coverage, sample_plan, segmentation_size,
};
pub use whisper::{
    DEFAULT_WHISPER_MODEL, Transcript, TranscriptSegment, WHISPER_MODELS, WhisperProgress,
    WhisperSpec, ensure_whisper_downloaded, find_whisper, transcribe, whisper_cached_size_bytes,
    whisper_directory, whisper_is_cached,
};

use std::path::Path;

use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::value::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MlError {
    #[error("failed to load model: {0}")]
    Load(String),
    #[error("inference failed: {0}")]
    Inference(String),
    #[error("model returned no usable output")]
    EmptyOutput,
    #[error("image dimensions must be positive")]
    EmptyImage,
    #[error("model download failed: {0}")]
    Download(String),
    #[error("operation cancelled")]
    Cancelled,
    #[error("tokenizer error: {0}")]
    Tokenizer(String),
    #[error("unknown model: {0}")]
    UnknownModel(String),
    #[error("unknown language: {0}")]
    UnknownLanguage(String),
    #[error("audio buffer is empty")]
    EmptyAudio,
}

pub struct SegmentationModel {
    session: Session,
    input_size: usize,
}

impl SegmentationModel {
    pub fn load(path: impl AsRef<Path>, input_size: usize) -> Result<Self, MlError> {
        let session = Session::builder()
            .map_err(|error| MlError::Load(error.to_string()))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|error| MlError::Load(error.to_string()))?
            .commit_from_file(path)
            .map_err(|error| MlError::Load(error.to_string()))?;

        Ok(Self {
            session,
            input_size,
        })
    }

    pub fn matte(&mut self, rgba: &[u8], width: usize, height: usize) -> Result<Vec<u8>, MlError> {
        if width == 0 || height == 0 {
            return Err(MlError::EmptyImage);
        }

        let (shape, data) = preprocess(rgba, width, height, self.input_size);
        let input = Value::from_array((shape, data))
            .map_err(|error| MlError::Inference(error.to_string()))?;

        let name = self
            .session
            .inputs()
            .first()
            .map(|input| input.name().to_string())
            .ok_or(MlError::EmptyOutput)?;

        let outputs = self
            .session
            .run(ort::inputs![name => input])
            .map_err(|error| MlError::Inference(error.to_string()))?;

        let first_output = outputs.iter().next().ok_or(MlError::EmptyOutput)?.1;
        let (shape, data) = first_output
            .try_extract_tensor::<f32>()
            .map_err(|error| MlError::Inference(error.to_string()))?;

        let mask_side = shape.last().copied().unwrap_or(0) as usize;
        if mask_side == 0 {
            return Err(MlError::EmptyOutput);
        }

        Ok(resize_mask(data, mask_side, width, height))
    }
}

pub type Tensor = (Vec<i64>, Vec<f32>);

pub fn preprocess(rgba: &[u8], width: usize, height: usize, size: usize) -> Tensor {
    let mut data = vec![0.0_f32; 3 * size * size];
    let plane = size * size;

    for y in 0..size {
        let source_y = y * height / size;
        for x in 0..size {
            let source_x = x * width / size;
            let index = (source_y * width + source_x) * 4;
            if index + 2 >= rgba.len() {
                continue;
            }
            let position = y * size + x;
            data[position] = rgba[index] as f32 / 127.5 - 1.0;
            data[plane + position] = rgba[index + 1] as f32 / 127.5 - 1.0;
            data[plane * 2 + position] = rgba[index + 2] as f32 / 127.5 - 1.0;
        }
    }

    (vec![1, 3, size as i64, size as i64], data)
}

pub fn resize_mask(mask: &[f32], mask_side: usize, width: usize, height: usize) -> Vec<u8> {
    let mut alpha = vec![0u8; width * height];
    if mask_side == 0 || mask.is_empty() {
        return alpha;
    }

    for y in 0..height {
        let source_y = (y * mask_side / height).min(mask_side - 1);
        for x in 0..width {
            let source_x = (x * mask_side / width).min(mask_side - 1);
            let value = mask
                .get(source_y * mask_side + source_x)
                .copied()
                .unwrap_or(0.0);
            alpha[y * width + x] = (value.clamp(0.0, 1.0) * 255.0) as u8;
        }
    }

    alpha
}

pub fn mask_coverage(alpha: &[u8]) -> f32 {
    if alpha.is_empty() {
        return 0.0;
    }
    let covered = alpha.iter().filter(|value| **value > 127).count();
    covered as f32 / alpha.len() as f32
}

pub fn channel_means(tensor: &Tensor) -> Vec<f32> {
    let (shape, data) = tensor;
    let channels = shape.get(1).copied().unwrap_or(0) as usize;
    if channels == 0 {
        return Vec::new();
    }
    let plane = data.len() / channels;
    (0..channels)
        .map(|channel| {
            let slice = &data[channel * plane..(channel + 1) * plane];
            slice.iter().sum::<f32>() / plane as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_scales_into_unit_range() {
        let rgba = vec![255u8; 4 * 4 * 4];
        let (shape, data) = preprocess(&rgba, 4, 4, 2);
        assert_eq!(shape, vec![1, 3, 2, 2]);
        assert_eq!(data.len(), 12);
        for value in &data {
            assert!((*value - 1.0).abs() < 1e-6, "unexpected value {value}");
        }
    }

    #[test]
    fn preprocess_maps_black_to_minus_one() {
        let rgba = vec![0u8; 4 * 4 * 4];
        let (_, data) = preprocess(&rgba, 4, 4, 2);
        for value in &data {
            assert!((*value + 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn channel_means_separates_planes() {
        let mut rgba = vec![0u8; 2 * 2 * 4];
        for pixel in 0..4 {
            rgba[pixel * 4] = 255;
        }
        let means = channel_means(&preprocess(&rgba, 2, 2, 2));
        assert!((means[0] - 1.0).abs() < 1e-6);
        assert!((means[1] + 1.0).abs() < 1e-6);
    }

    #[test]
    fn resize_mask_matches_requested_size() {
        let mask = vec![1.0; 4 * 4];
        let alpha = resize_mask(&mask, 4, 8, 6);
        assert_eq!(alpha.len(), 8 * 6);
        assert!(alpha.iter().all(|value| *value == 255));
    }

    #[test]
    fn resize_mask_clamps_values() {
        let mask = vec![-3.0, 5.0, 0.5, 0.0];
        let alpha = resize_mask(&mask, 2, 2, 2);
        assert_eq!(alpha[0], 0);
        assert_eq!(alpha[1], 255);
    }

    #[test]
    fn coverage_counts_opaque_pixels() {
        let alpha = vec![255, 255, 0, 0];
        assert!((mask_coverage(&alpha) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn coverage_of_empty_mask_is_zero() {
        assert_eq!(mask_coverage(&[]), 0.0);
    }
}
