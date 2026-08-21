use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use cutix_playback::{AudioCache, ComposeRequest, FrameComposer, MediaResolver, MixRequest, mix};
use cutix_project::Project;
use time::{FrameRate, MediaTime};

use crate::backend::{
    AudioSpec, AudioSupport, BackendFactory, ExportArtifacts, STREAM_COPY, VideoSpec,
};
use crate::error::{ExportError, Result};
use crate::fit::{blit_centre, plan_fit};
use crate::presets::ExportQuality;

pub const EXPORT_SAMPLE_RATE: u32 = 48_000;
pub const EXPORT_CHANNELS: usize = 2;

const PIPELINE_DEPTH: usize = 2;

pub struct ExportRequest {
    pub project: Arc<Project>,
    pub scene_id: Option<String>,
    pub destination: PathBuf,
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
    pub quality: ExportQuality,
    pub include_audio: bool,
    pub matte_root: Option<PathBuf>,
    pub backend: &'static dyn BackendFactory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Rendering,
    Audio,

    Packaging,
    Finishing,
}

#[derive(Clone, Copy, Debug)]
pub struct Progress {
    pub stage: Stage,
    pub frame: u64,
    pub total_frames: u64,
    pub elapsed: Duration,
}

impl Progress {
    pub fn fraction(&self) -> f32 {
        if self.total_frames == 0 {
            return 0.0;
        }
        (self.frame as f32 / self.total_frames as f32).clamp(0.0, 1.0)
    }

    pub fn frames_per_second(&self) -> f32 {
        let seconds = self.elapsed.as_secs_f32();
        if seconds <= 0.0 {
            0.0
        } else {
            self.frame as f32 / seconds
        }
    }
}

#[derive(Clone, Debug)]
pub struct ExportOutcome {
    pub artifacts: ExportArtifacts,
    pub elapsed: Duration,
    pub audio_support: AudioSupport,
    pub letterboxed: bool,
    pub encoder: String,

    pub skipped: Vec<String>,
}

impl ExportOutcome {
    pub fn frames_per_second(&self) -> f32 {
        let seconds = self.elapsed.as_secs_f32();
        if seconds <= 0.0 {
            0.0
        } else {
            self.artifacts.frames as f32 / seconds
        }
    }
}

pub fn scene_duration(project: &Project, scene_id: Option<&str>) -> MediaTime {
    let scene = match scene_id {
        Some(id) => project.scenes.iter().find(|scene| scene.id == id),
        None => project
            .scenes
            .iter()
            .find(|scene| scene.id == project.current_scene_id)
            .or_else(|| project.scenes.first()),
    };
    let Some(scene) = scene else {
        return MediaTime::ZERO;
    };
    scene
        .tracks
        .all()
        .flat_map(|track| track.elements())
        .map(|element| element.end_time())
        .fold(MediaTime::ZERO, MediaTime::max)
}

pub fn frame_count(duration: MediaTime, rate: FrameRate) -> u64 {
    let Some(fps) = rate.as_f64().filter(|value| *value > 0.0) else {
        return 0;
    };
    let seconds = duration.to_seconds_f64();
    if seconds <= 0.0 {
        return 0;
    }
    (seconds * fps).round().max(1.0) as u64
}

pub fn run(
    request: &ExportRequest,
    media: &dyn MediaResolver,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<ExportOutcome> {
    if request.width == 0 || request.height == 0 {
        return Err(ExportError::InvalidSize {
            width: request.width,
            height: request.height,
        });
    }
    let fps = request
        .frame_rate
        .as_f64()
        .filter(|value| *value > 0.0)
        .ok_or(ExportError::InvalidFrameRate)?;

    let duration = scene_duration(&request.project, request.scene_id.as_deref());
    let total_frames = frame_count(duration, request.frame_rate);
    if total_frames == 0 {
        return Err(ExportError::Empty);
    }

    if let Some(plan) = crate::remux::plan(request, media) {
        let started = Instant::now();
        match crate::remux::run(&plan, &request.destination, cancel, on_progress) {
            Ok(artifacts) => {
                on_progress(Progress {
                    stage: Stage::Finishing,
                    frame: artifacts.frames,
                    total_frames: artifacts.frames.max(1),
                    elapsed: started.elapsed(),
                });
                return Ok(ExportOutcome {
                    artifacts,
                    elapsed: started.elapsed(),
                    audio_support: AudioSupport::Muxed,
                    letterboxed: false,
                    encoder: STREAM_COPY.to_owned(),
                    skipped: Vec::new(),
                });
            }
            Err(ExportError::Cancelled) => return Err(ExportError::Cancelled),
            Err(_) => {}
        }
    }

    let canvas = request.project.settings.canvas_size.clone();
    let plan = plan_fit(canvas.width, canvas.height, request.width, request.height);
    let video = VideoSpec {
        width: request.width,
        height: request.height,
        frame_rate: request.frame_rate,
        bitrate_bps: request
            .quality
            .bitrate_bps(request.width, request.height, fps),
        quality: request.quality,
    };
    let audio_spec = request.include_audio.then_some(AudioSpec {
        sample_rate: EXPORT_SAMPLE_RATE,
        channels: EXPORT_CHANNELS,
    });

    let mut composer =
        FrameComposer::new().map_err(|error| ExportError::Compose(error.to_string()))?;
    composer.set_matte_root(request.matte_root.clone());

    let mut backend = request
        .backend
        .create(&request.destination, video, audio_spec)?;
    let audio_support = backend.audio_support();

    let mut canvas_pixels = vec![0u8; request.width as usize * request.height as usize * 4];
    let mut skipped: Vec<String> = Vec::new();
    let started = Instant::now();

    composer.set_pipeline_depth(PIPELINE_DEPTH);
    let mut in_flight: VecDeque<cutix_playback::PendingFrame> = VecDeque::new();
    let mut submitted = 0u64;
    let mut written = 0u64;

    while written < total_frames {
        if cancel.load(Ordering::Relaxed) {
            return Err(ExportError::Cancelled);
        }

        while submitted < total_frames && in_flight.len() < PIPELINE_DEPTH {
            // Positions come from the frame index against the exact rational frame
            // duration. Deriving them from a float fps drifts against the timeline the
            // player showed, and the rounding policy belongs in the `time` crate rather
            // than being re-invented per pipeline.
            let time = MediaTime::from_frame(submitted as i64, request.frame_rate)
                .ok_or(ExportError::InvalidFrameRate)?;
            let pending = composer
                .submit_frame(
                    &ComposeRequest {
                        project: &request.project,
                        scene_id: request.scene_id.as_deref(),
                        time,
                        width: plan.inner_width,
                        height: plan.inner_height,
                    },
                    media,
                )
                .map_err(|error| ExportError::Compose(error.to_string()))?;
            in_flight.push_back(pending);
            submitted += 1;
        }

        let Some(pending) = in_flight.pop_front() else {
            break;
        };
        let frame = composer.resolve(pending);
        for reason in &frame.skipped {
            if !skipped.contains(reason) {
                skipped.push(reason.clone());
            }
        }

        if plan.letterboxed {
            blit_centre(
                crate::fit::RgbaFrame {
                    pixels: &frame.pixels,
                    width: frame.width,
                    height: frame.height,
                },
                crate::fit::RgbaFrameMut {
                    pixels: &mut canvas_pixels,
                    width: request.width,
                    height: request.height,
                },
                plan.offset_x,
                plan.offset_y,
            );
            backend.push_frame(&canvas_pixels)?;
        } else {
            backend.push_frame(&frame.pixels)?;
        }

        written += 1;
        on_progress(Progress {
            stage: Stage::Rendering,
            frame: written,
            total_frames,
            elapsed: started.elapsed(),
        });
    }

    if request.include_audio && audio_support != AudioSupport::None {
        if cancel.load(Ordering::Relaxed) {
            return Err(ExportError::Cancelled);
        }
        on_progress(Progress {
            stage: Stage::Audio,
            frame: total_frames,
            total_frames,
            elapsed: started.elapsed(),
        });
        let mut cache = AudioCache::new();
        let (buffer, audio_skipped) = mix(
            &MixRequest {
                project: &request.project,
                scene_id: request.scene_id.as_deref(),
                start: MediaTime::ZERO,
                duration,
                sample_rate: EXPORT_SAMPLE_RATE,
                channels: EXPORT_CHANNELS,
            },
            media,
            &mut cache,
        )
        .map_err(|error| ExportError::Compose(error.to_string()))?;
        for reason in audio_skipped {
            if !skipped.contains(&reason) {
                skipped.push(reason);
            }
        }
        if buffer.peak() > 0.0 {
            backend.push_audio(&buffer)?;
        }
    }

    on_progress(Progress {
        stage: Stage::Finishing,
        frame: total_frames,
        total_frames,
        elapsed: started.elapsed(),
    });
    let artifacts = backend.finish()?;

    Ok(ExportOutcome {
        artifacts,
        elapsed: started.elapsed(),
        audio_support,
        letterboxed: plan.letterboxed,
        encoder: request.backend.name().to_owned(),
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_count_rounds_to_the_nearest_whole_frame() {
        let two_seconds = MediaTime::from_seconds_f64(2.0).expect("time");
        assert_eq!(frame_count(two_seconds, FrameRate::FPS_30), 60);
        assert_eq!(frame_count(MediaTime::ZERO, FrameRate::FPS_30), 0);
        assert_eq!(frame_count(two_seconds, FrameRate::new(0, 1)), 0);
    }

    #[test]
    fn progress_reports_a_bounded_fraction() {
        let progress = Progress {
            stage: Stage::Rendering,
            frame: 5,
            total_frames: 10,
            elapsed: Duration::from_secs(1),
        };
        assert!((progress.fraction() - 0.5).abs() < f32::EPSILON);
        assert_eq!(progress.frames_per_second(), 5.0);
        let empty = Progress {
            stage: Stage::Rendering,
            frame: 0,
            total_frames: 0,
            elapsed: Duration::ZERO,
        };
        assert_eq!(empty.fraction(), 0.0);
        assert_eq!(empty.frames_per_second(), 0.0);
    }
}
