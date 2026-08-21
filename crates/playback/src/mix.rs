use cutix_project::Project;
use cutix_project::model::{ElementTransition, RetimeConfig, TimelineElement, Track};
use time::MediaTime;

use crate::animation::{has_channel, scalar_at};
use crate::audio_decode::{AudioCache, PcmBuffer};
use crate::error::{PlaybackError, Result};
use crate::media::MediaResolver;
use crate::resolve::apply_transition_easing;
use crate::retime::source_offset_seconds;

pub const VOLUME_DB_MIN: f64 = -60.0;
pub const VOLUME_DB_MAX: f64 = 20.0;
pub fn db_to_linear(db: f64) -> f64 {
    let db = if db.is_finite() {
        db.clamp(VOLUME_DB_MIN, VOLUME_DB_MAX)
    } else {
        0.0
    };
    10f64.powf(db / 20.0)
}

#[derive(Clone, Debug)]
pub struct AudioBuffer {
    pub sample_rate: u32,
    pub channels: usize,
    pub interleaved: Vec<f32>,
}

impl AudioBuffer {
    /// How many frames the buffer holds. A buffer with no channels holds none.
    pub fn frame_count(&self) -> usize {
        self.interleaved
            .len()
            .checked_div(self.channels)
            .unwrap_or(0)
    }

    pub fn sample(&self, frame: usize, channel: usize) -> f32 {
        self.interleaved[frame * self.channels + channel]
    }

    pub fn peak(&self) -> f32 {
        self.interleaved
            .iter()
            .fold(0.0f32, |peak, value| peak.max(value.abs()))
    }
}

pub struct MixRequest<'a> {
    pub project: &'a Project,
    pub scene_id: Option<&'a str>,
    pub start: MediaTime,
    pub duration: MediaTime,
    pub sample_rate: u32,
    pub channels: usize,
}

struct AudibleElement<'a> {
    element: &'a TimelineElement,
    media_id: &'a str,
    volume_db: f64,
    retime: Option<&'a RetimeConfig>,
    transition: Option<&'a ElementTransition>,
}

fn audible<'a>(track: &'a Track, element: &'a TimelineElement) -> Option<AudibleElement<'a>> {
    let track_muted = match track {
        Track::Audio { muted, .. } | Track::Video { muted, .. } => *muted,
        _ => return None,
    };
    if track_muted {
        return None;
    }
    match element {
        TimelineElement::Audio(audio) => {
            if audio.muted.unwrap_or(false) {
                return None;
            }
            Some(AudibleElement {
                element,
                media_id: audio.media_id.as_deref()?,
                volume_db: audio.volume,
                retime: audio.retime.as_ref(),
                transition: None,
            })
        }
        TimelineElement::Video(video) => {
            if video.muted.unwrap_or(false) || !video.is_source_audio_enabled.unwrap_or(true) {
                return None;
            }
            Some(AudibleElement {
                element,
                media_id: &video.media_id,
                volume_db: video.volume.unwrap_or(0.0),
                retime: video.retime.as_ref(),
                transition: video.transition.as_ref(),
            })
        }
        _ => None,
    }
}

fn crossfade_gain(transition: Option<&ElementTransition>, local_ticks: f64) -> f64 {
    let Some(transition) = transition else {
        return 1.0;
    };
    let span = transition.duration.as_ticks() as f64;
    if span <= 0.0 {
        return 1.0;
    }
    apply_transition_easing(local_ticks / span, transition.easing.as_deref()).clamp(0.0, 1.0)
}

fn sample_at(buffer: &PcmBuffer, channel: usize, source_seconds: f64) -> f32 {
    let data = buffer.channel(channel);
    if data.is_empty() {
        return 0.0;
    }
    let index = source_seconds * buffer.sample_rate as f64;
    if index < 0.0 {
        return 0.0;
    }
    let lower = index.floor() as usize;
    if lower >= data.len() {
        return 0.0;
    }
    let upper = (lower + 1).min(data.len() - 1);
    let fraction = (index - lower as f64) as f32;
    data[lower] * (1.0 - fraction) + data[upper] * fraction
}

pub fn mix(
    request: &MixRequest<'_>,
    media: &dyn MediaResolver,
    cache: &mut AudioCache,
) -> Result<(AudioBuffer, Vec<String>)> {
    let scene = match request.scene_id {
        Some(id) => request
            .project
            .scenes
            .iter()
            .find(|scene| scene.id == id)
            .ok_or_else(|| PlaybackError::SceneNotFound(id.to_owned()))?,
        None => request
            .project
            .scenes
            .iter()
            .find(|scene| scene.id == request.project.current_scene_id)
            .or_else(|| request.project.scenes.first())
            .ok_or_else(|| PlaybackError::SceneNotFound("<current>".to_owned()))?,
    };

    let channels = request.channels.max(1);
    let sample_rate = request.sample_rate.max(1);
    let frames = (request.duration.to_seconds_f64() * sample_rate as f64)
        .ceil()
        .max(0.0) as usize;
    let mut interleaved = vec![0.0f32; frames * channels];
    let mut skipped = Vec::new();
    let range_start = request.start.to_seconds_f64();
    let range_end = range_start + request.duration.to_seconds_f64();

    for track in scene.tracks.all() {
        for element in track.elements() {
            let Some(entry) = audible(track, element) else {
                continue;
            };
            let base = entry.element.base();
            let element_start = base.start_time.to_seconds_f64();
            let element_end = base.start_time.to_seconds_f64() + base.duration.to_seconds_f64();
            if element_end <= range_start || element_start >= range_end {
                continue;
            }
            let Some(path) = media.resolve(entry.media_id) else {
                skipped.push(format!("media:{}", entry.media_id));
                continue;
            };
            let buffer = match cache.load(entry.media_id, &path) {
                Ok(buffer) => buffer.clone(),
                Err(error) => {
                    skipped.push(format!("{}: {error}", entry.media_id));
                    continue;
                }
            };

            let animations = base.animations.as_ref();
            let animated_volume = has_channel(animations, "volume");
            let trim_start = base.trim_start.to_seconds_f64();

            for frame in 0..frames {
                let timeline_seconds = range_start + frame as f64 / sample_rate as f64;
                let clip_seconds = timeline_seconds - element_start;
                if clip_seconds < 0.0 || clip_seconds >= base.duration.to_seconds_f64() {
                    continue;
                }
                let source_seconds = trim_start + source_offset_seconds(entry.retime, clip_seconds);
                let local_ticks = clip_seconds * time::TICKS_PER_SECOND as f64;
                let base_db = if animated_volume {
                    scalar_at(
                        animations,
                        "volume",
                        entry.volume_db,
                        MediaTime::from_ticks(local_ticks.round() as i64),
                    )
                } else {
                    entry.volume_db
                };
                let gain = db_to_linear(base_db) * crossfade_gain(entry.transition, local_ticks);
                if gain == 0.0 {
                    continue;
                }
                for channel in 0..channels {
                    let value = sample_at(&buffer, channel, source_seconds) * gain as f32;
                    interleaved[frame * channels + channel] += value;
                }
            }
        }
    }

    Ok((
        AudioBuffer {
            sample_rate,
            channels,
            interleaved,
        },
        skipped,
    ))
}
