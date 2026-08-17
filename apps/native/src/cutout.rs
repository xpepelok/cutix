use std::path::{Path, PathBuf};

use cutix_i18n::{t, t_args};
use ml::{
    base64_encode, encode_matte_png, matte_coverage, models, sample_plan, segmentation_size,
    CutoutMatteFrame, CutoutMode, ElementCutout, MlError, SegmentationModel,
};

use crate::ai::Job;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutoutRequestMode {
    Static,
    PerFrame { rate: u32 },
}

pub struct CutoutRequest {
    pub source: PathBuf,
    pub is_video: bool,

    pub reference_seconds: f64,

    pub duration_ticks: i64,

    pub trim_start_ticks: i64,
    pub mode: CutoutRequestMode,
    pub invert: bool,
    pub model_key: String,
}

fn read_frame(
    path: &Path,
    is_video: bool,
    seconds: f64,
) -> Result<(Vec<u8>, usize, usize), String> {
    if is_video {
        let frame =
            video::decode::frame_at(path, seconds.max(0.0)).map_err(|error| error.to_string())?;
        return Ok((frame.rgba, frame.width as usize, frame.height as usize));
    }
    let image = image::open(path)
        .map_err(|error| error.to_string())?
        .to_rgba8();
    let (width, height) = image.dimensions();
    Ok((image.into_raw(), width as usize, height as usize))
}

fn matte_of(
    model: &mut SegmentationModel,
    path: &Path,
    is_video: bool,
    seconds: f64,
) -> Result<(Vec<u8>, u32, u32), String> {
    let (rgba, width, height) = read_frame(path, is_video, seconds)?;
    if width == 0 || height == 0 {
        return Err(t("cutout.noFrame"));
    }
    let (target_width, target_height) = segmentation_size(width, height);
    let scaled = ml::matte::downscale_rgba(&rgba, width, height, target_width, target_height);
    let alpha = model
        .matte(&scaled, target_width, target_height)
        .map_err(|error: MlError| error.to_string())?;
    Ok((alpha, target_width as u32, target_height as u32))
}

pub fn compute(request: &CutoutRequest, job: &Job) -> Result<ElementCutout, String> {
    let spec = models::find_model(&request.model_key).ok_or_else(|| t("cutout.failed"))?;

    let cached = models::is_cached(spec);
    let path = models::ensure_downloaded(spec, |progress| {
        if !cached {
            job.publish(
                t_args(
                    "cutout.downloading",
                    &[("percent", &((progress * 100.0).round() as i64).to_string())],
                ),
                progress,
            );
        }
    })
    .map_err(|error| error.to_string())?;

    if job.is_cancelled() {
        return Err(t("cutout.cancel"));
    }

    job.publish(t("cutout.processing"), 0.0);
    let mut model =
        SegmentationModel::load(&path, spec.input_size).map_err(|error| error.to_string())?;

    let (reference, width, height) = matte_of(
        &mut model,
        &request.source,
        request.is_video,
        request.reference_seconds,
    )?;
    let reference_coverage = matte_coverage(&reference);
    let reference_png = base64_encode(
        &encode_matte_png(&reference, width, height).map_err(|error| error.to_string())?,
    );

    let CutoutRequestMode::PerFrame { rate } = request.mode else {
        return Ok(ElementCutout {
            enabled: true,
            mode: CutoutMode::Static,
            width,
            height,
            png: reference_png,
            png_path: None,
            invert: request.invert,
            reference_time: (request.reference_seconds * ml::matte::TICKS_PER_SECOND as f64)
                .round(),
            coverage: reference_coverage,
            frames: None,
            sample_interval: None,
        });
    };

    let (times, stride) = sample_plan(request.duration_ticks, rate);
    let total = times.len();
    let mut frames: Vec<CutoutMatteFrame> = Vec::with_capacity(total);

    job.publish(
        t_args(
            "cutout.perFrameProgress",
            &[("completed", "0"), ("total", &total.to_string())],
        ),
        0.0,
    );

    for (index, local_ticks) in times.iter().enumerate() {
        if job.is_cancelled() {
            return Err(t("cutout.cancel"));
        }
        let source_ticks = request.trim_start_ticks + local_ticks;
        let seconds = source_ticks as f64 / ml::matte::TICKS_PER_SECOND as f64;

        let Ok((alpha, sample_width, sample_height)) =
            matte_of(&mut model, &request.source, request.is_video, seconds)
        else {
            continue;
        };
        let png = encode_matte_png(&alpha, sample_width, sample_height)
            .map_err(|error| error.to_string())?;
        frames.push(CutoutMatteFrame {
            source_time: source_ticks as f64,
            png: base64_encode(&png),
            png_path: None,
            coverage: matte_coverage(&alpha),
        });
        let completed = index + 1;
        job.publish(
            t_args(
                "cutout.perFrameProgress",
                &[
                    ("completed", &completed.to_string()),
                    ("total", &total.to_string()),
                ],
            ),
            completed as f32 / total.max(1) as f32,
        );
    }

    if frames.is_empty() {
        return Err(t("cutout.noFrame"));
    }

    frames.sort_by(|left, right| left.source_time.total_cmp(&right.source_time));
    let coverage = frames.iter().map(|frame| frame.coverage).sum::<f32>() / frames.len() as f32;
    let reference_time = frames[0].source_time;

    Ok(ElementCutout {
        enabled: true,
        mode: CutoutMode::PerFrame,
        width,
        height,
        png: reference_png,
        png_path: None,
        invert: request.invert,
        reference_time,
        coverage,
        frames: Some(frames),
        sample_interval: Some(stride as f64),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_static_request_carries_no_sample_rate() {
        assert_eq!(CutoutRequestMode::Static, CutoutRequestMode::Static);
        assert_ne!(
            CutoutRequestMode::Static,
            CutoutRequestMode::PerFrame { rate: 2 }
        );
    }

    #[test]
    fn the_default_rate_is_one_of_the_offered_rates() {
        assert!(ml::CUTOUT_SAMPLE_RATES.contains(&ml::DEFAULT_CUTOUT_SAMPLE_RATE));
    }

    #[test]
    fn every_cutout_string_is_translated() {
        for key in [
            "cutout.title",
            "cutout.remove",
            "cutout.reapply",
            "cutout.model",
            "cutout.downloading",
            "cutout.processing",
            "cutout.done",
            "cutout.failed",
            "cutout.hint",
            "cutout.noFrame",
            "cutout.noMedia",
            "cutout.enabled",
            "cutout.invert",
            "cutout.clear",
            "cutout.cancel",
            "cutout.rate",
            "cutout.staticNotice",
            "cutout.perFrame",
            "cutout.perFrameProgress",
            "cutout.perFrameDone",
            "cutout.perFrameNotice",
        ] {
            assert_ne!(t(key), key, "{key}");
        }
    }
}
