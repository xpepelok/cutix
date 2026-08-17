use serde::{Deserialize, Serialize};

use crate::MlError;

pub const MAX_SEGMENTATION_SIZE: usize = 1024;
pub const CUTOUT_SAMPLE_RATES: &[u32] = &[1, 2, 4];
pub const DEFAULT_CUTOUT_SAMPLE_RATE: u32 = 2;
pub const MAX_CUTOUT_MATTE_FRAMES: usize = 150;
pub const TICKS_PER_SECOND: i64 = 120_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CutoutMode {
    Static,
    PerFrame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatteSource<'a> {
    Inline(&'a str),
    File(&'a str),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CutoutMatteFrame {
    pub source_time: f64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub png: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub png_path: Option<String>,
    pub coverage: f32,
}

impl CutoutMatteFrame {
    pub fn source(&self) -> MatteSource<'_> {
        match self.png_path.as_deref() {
            Some(path) => MatteSource::File(path),
            None => MatteSource::Inline(&self.png),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementCutout {
    pub enabled: bool,
    pub mode: CutoutMode,
    pub width: u32,
    pub height: u32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub png: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub png_path: Option<String>,
    pub invert: bool,
    pub reference_time: f64,
    pub coverage: f32,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub frames: Option<Vec<CutoutMatteFrame>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub sample_interval: Option<f64>,
}

impl ElementCutout {
    pub fn reference_source(&self) -> MatteSource<'_> {
        match self.png_path.as_deref() {
            Some(path) => MatteSource::File(path),
            None => MatteSource::Inline(&self.png),
        }
    }

    pub fn matte_for(&self, source_ticks: Option<f64>) -> MatteSource<'_> {
        let Some(frames) = self.frames.as_ref().filter(|frames| !frames.is_empty()) else {
            return self.reference_source();
        };
        if self.mode != CutoutMode::PerFrame {
            return self.reference_source();
        }
        let Some(ticks) = source_ticks else {
            return frames[0].source();
        };

        let mut low = 0usize;
        let mut high = frames.len();
        while low < high {
            let middle = (low + high) / 2;
            if frames[middle].source_time < ticks {
                low = middle + 1;
            } else {
                high = middle;
            }
        }
        let after = frames.get(low);
        let before = &frames[low.saturating_sub(1)];
        match after {
            Some(after)
                if (after.source_time - ticks).abs() < (before.source_time - ticks).abs() =>
            {
                after.source()
            }
            _ => before.source(),
        }
    }

    pub fn sample_count(&self) -> usize {
        self.frames.as_ref().map(Vec::len).unwrap_or(0)
    }
}

pub fn matte_coverage(alpha: &[u8]) -> f32 {
    if alpha.is_empty() {
        return 0.0;
    }
    let total: u64 = alpha.iter().map(|value| u64::from(*value)).sum();
    total as f32 / (alpha.len() as f32 * 255.0)
}

pub fn segmentation_size(width: usize, height: usize) -> (usize, usize) {
    let longest = width.max(height);
    if longest <= MAX_SEGMENTATION_SIZE || longest == 0 {
        return (width, height);
    }
    let scale = MAX_SEGMENTATION_SIZE as f64 / longest as f64;
    (
        ((width as f64 * scale).round() as usize).max(1),
        ((height as f64 * scale).round() as usize).max(1),
    )
}

pub fn downscale_rgba(
    rgba: &[u8],
    width: usize,
    height: usize,
    target_width: usize,
    target_height: usize,
) -> Vec<u8> {
    if target_width == width && target_height == height {
        return rgba.to_vec();
    }
    let mut out = vec![0u8; target_width * target_height * 4];
    for y in 0..target_height {
        let source_y = (y * height / target_height.max(1)).min(height.saturating_sub(1));
        for x in 0..target_width {
            let source_x = (x * width / target_width.max(1)).min(width.saturating_sub(1));
            let from = (source_y * width + source_x) * 4;
            let to = (y * target_width + x) * 4;
            if from + 4 <= rgba.len() {
                out[to..to + 4].copy_from_slice(&rgba[from..from + 4]);
            }
        }
    }
    out
}

pub fn encode_matte_png(alpha: &[u8], width: u32, height: u32) -> Result<Vec<u8>, MlError> {
    let mut rgba = Vec::with_capacity(alpha.len() * 4);
    for value in alpha {
        rgba.extend_from_slice(&[255, 255, 255, *value]);
    }
    let buffer: image::RgbaImage =
        image::ImageBuffer::from_raw(width, height, rgba).ok_or(MlError::EmptyImage)?;
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(buffer)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .map_err(|error| MlError::Inference(error.to_string()))?;
    Ok(bytes.into_inner())
}

pub fn decode_matte_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), MlError> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map_err(|error| MlError::Inference(error.to_string()))?
        .to_rgba8();
    let (width, height) = image.dimensions();
    let alpha = image.pixels().map(|pixel| pixel.0[3]).collect();
    Ok((width, height, alpha))
}

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let packed = (b0 << 16) | (b1 << 8) | b2;
        out.push(BASE64_ALPHABET[(packed >> 18) as usize & 63] as char);
        out.push(BASE64_ALPHABET[(packed >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            BASE64_ALPHABET[(packed >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            BASE64_ALPHABET[packed as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut accumulator: u32 = 0;
    let mut bits = 0u32;
    for byte in text.bytes() {
        if byte == b'=' || byte.is_ascii_whitespace() {
            continue;
        }
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32;
        accumulator = (accumulator << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((accumulator >> bits) as u8);
        }
    }
    Some(out)
}

pub fn sample_plan(duration_ticks: i64, rate: u32) -> (Vec<i64>, i64) {
    if duration_ticks <= 0 {
        return (vec![0], 0);
    }
    let interval = ((TICKS_PER_SECOND as f64 / rate.max(1) as f64).round() as i64).max(1);
    let last = (duration_ticks - 1).max(0);
    let planned = last / interval + 1;
    let stride = if planned > MAX_CUTOUT_MATTE_FRAMES as i64 {
        ((last as f64 / (MAX_CUTOUT_MATTE_FRAMES as f64 - 1.0)).ceil() as i64).max(1)
    } else {
        interval
    };
    let mut times = Vec::new();
    let mut clip_time = 0i64;
    while clip_time <= last {
        times.push(clip_time);
        clip_time += stride;
    }
    (times, stride)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(source_time: f64, png: &str) -> CutoutMatteFrame {
        CutoutMatteFrame {
            source_time,
            png: png.to_owned(),
            png_path: None,
            coverage: 0.5,
        }
    }

    fn inline(source: MatteSource<'_>) -> &str {
        match source {
            MatteSource::Inline(text) => text,
            MatteSource::File(path) => panic!("expected inline, got file {path}"),
        }
    }

    fn per_frame() -> ElementCutout {
        ElementCutout {
            enabled: true,
            mode: CutoutMode::PerFrame,
            width: 4,
            height: 4,
            png: String::from("reference"),
            png_path: None,
            invert: false,
            reference_time: 0.0,
            coverage: 0.5,
            frames: Some(vec![
                frame(0.0, "one"),
                frame(60_000.0, "two"),
                frame(120_000.0, "three"),
                frame(180_000.0, "four"),
            ]),
            sample_interval: Some(60_000.0),
        }
    }

    #[test]
    fn the_nearest_sample_wins() {
        let cutout = per_frame();
        assert_eq!(inline(cutout.matte_for(Some(20_000.0))), "one");
        assert_eq!(inline(cutout.matte_for(Some(40_000.0))), "two");
        assert_eq!(inline(cutout.matte_for(Some(95_000.0))), "three");
        assert_eq!(inline(cutout.matte_for(Some(1e9))), "four");
    }

    #[test]
    fn a_tie_keeps_the_earlier_sample() {
        assert_eq!(inline(per_frame().matte_for(Some(30_000.0))), "one");
    }

    #[test]
    fn a_static_cutout_always_returns_the_reference() {
        let mut cutout = per_frame();
        cutout.mode = CutoutMode::Static;
        assert_eq!(inline(cutout.matte_for(Some(120_000.0))), "reference");
    }

    #[test]
    fn an_unknown_time_falls_back_to_the_first_sample() {
        assert_eq!(inline(per_frame().matte_for(None)), "one");
    }

    #[test]
    fn a_file_backed_sample_reports_its_path() {
        let mut cutout = per_frame();
        cutout.png = String::new();
        cutout.png_path = Some(String::from("mattes/reference.png"));
        let frames = cutout.frames.as_mut().expect("frames");
        for (index, frame) in frames.iter_mut().enumerate() {
            frame.png = String::new();
            frame.png_path = Some(format!("mattes/{index}.png"));
        }
        assert_eq!(
            cutout.matte_for(Some(40_000.0)),
            MatteSource::File("mattes/1.png")
        );
        cutout.mode = CutoutMode::Static;
        assert_eq!(
            cutout.matte_for(Some(40_000.0)),
            MatteSource::File("mattes/reference.png")
        );
    }

    #[test]
    fn a_file_backed_cutout_writes_no_base64_into_the_document() {
        let mut cutout = per_frame();
        cutout.png = String::new();
        cutout.png_path = Some(String::from("mattes/reference.png"));
        cutout.frames = None;
        let json = serde_json::to_value(&cutout).expect("json");
        assert!(json.get("png").is_none());
        assert_eq!(json["pngPath"], "mattes/reference.png");
        assert_eq!(
            serde_json::from_value::<ElementCutout>(json).expect("parse"),
            cutout
        );
    }

    #[test]
    fn coverage_weighs_partial_alpha() {
        assert!((matte_coverage(&[255, 255, 0, 0]) - 0.5).abs() < 1e-6);
        assert!((matte_coverage(&[128, 128, 128, 128]) - 128.0 / 255.0).abs() < 1e-6);
        assert_eq!(matte_coverage(&[]), 0.0);
    }

    #[test]
    fn frames_are_downscaled_only_past_the_limit() {
        assert_eq!(segmentation_size(1920, 1080), (1024, 576));
        assert_eq!(segmentation_size(640, 480), (640, 480));
        assert_eq!(segmentation_size(1024, 1024), (1024, 1024));
        assert_eq!(segmentation_size(4000, 100), (1024, 26));
    }

    #[test]
    fn a_matte_survives_a_png_round_trip() {
        let alpha: Vec<u8> = (0..64u16).map(|value| (value * 4) as u8).collect();
        let png = encode_matte_png(&alpha, 8, 8).expect("encode");
        let (width, height, decoded) = decode_matte_png(&png).expect("decode");
        assert_eq!((width, height), (8, 8));
        assert_eq!(decoded, alpha);
    }

    #[test]
    fn base64_round_trips_arbitrary_bytes() {
        for length in 0..40usize {
            let bytes: Vec<u8> = (0..length).map(|value| (value * 7 % 251) as u8).collect();
            let text = base64_encode(&bytes);
            assert_eq!(base64_decode(&text).expect("decode"), bytes, "{length}");
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_encode(b"M"), "TQ==");
    }

    #[test]
    fn the_sample_plan_uses_the_requested_rate() {
        let (times, stride) = sample_plan(TICKS_PER_SECOND * 3, 2);
        assert_eq!(stride, 60_000);
        assert_eq!(times.len(), 6);
        assert_eq!(times.first(), Some(&0));
        assert_eq!(times.last(), Some(&300_000));
    }

    #[test]
    fn the_sample_plan_caps_the_frame_count() {
        let (times, stride) = sample_plan(TICKS_PER_SECOND * 600, 4);
        assert!(times.len() <= MAX_CUTOUT_MATTE_FRAMES, "{}", times.len());
        assert!(stride > 30_000);
    }

    #[test]
    fn a_still_image_gets_one_sample() {
        assert_eq!(sample_plan(0, 2).0, vec![0]);
    }

    #[test]
    fn the_document_shape_matches_the_web() {
        let json = serde_json::to_value(per_frame()).expect("json");
        assert_eq!(json["mode"], "perFrame");
        assert!(json.get("referenceTime").is_some());
        assert!(json.get("sampleInterval").is_some());
        let parsed: ElementCutout = serde_json::from_value(json).expect("parse");
        assert_eq!(parsed, per_frame());
    }
}
