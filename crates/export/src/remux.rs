use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use cutix_playback::MediaResolver;
use cutix_project::Project;
use cutix_project::model::{Crop, Scene, TimelineElement, Track, Transform, VideoElement};
use mp4::{
    AacConfig, AvcConfig, MediaConfig, MediaType, Mp4Config, Mp4Reader, Mp4Sample, Mp4Writer,
    TrackConfig, TrackType,
};
use time::MediaTime;

use crate::backend::ExportArtifacts;
use crate::error::{ExportError, Result};
use crate::job::{ExportRequest, Progress, Stage};

const COPYABLE_CONTAINERS: [&str; 3] = ["mp4", "m4v", "mov"];

const RATE_TOLERANCE: f64 = 0.01;

#[derive(Clone, Debug, PartialEq)]
pub struct TrimPlan {
    pub source: PathBuf,
    pub start: MediaTime,
    pub duration: MediaTime,
    pub include_audio: bool,
    pub snap_to_keyframe: bool,
}

impl TrimPlan {
    fn end(&self) -> MediaTime {
        MediaTime::from_ticks(self.start.as_ticks() + self.duration.as_ticks())
    }
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

fn crop_is_empty(crop: Option<&Crop>) -> bool {
    match crop {
        None => true,
        Some(crop) => {
            crop.left == 0.0 && crop.top == 0.0 && crop.right == 0.0 && crop.bottom == 0.0
        }
    }
}

fn transform_is_identity(transform: &Transform) -> bool {
    *transform == Transform::default()
}

fn untouched_picture(element: &VideoElement) -> bool {
    element.effects.as_ref().is_none_or(|list| list.is_empty())
        && element.masks.as_ref().is_none_or(|list| list.is_empty())
        && element.cutout.is_none()
        && element.transition.is_none()
        && element.motion.is_none()
        && element.retime.is_none()
        && element.reversed_from.is_none()
        && element.blend_mode.as_deref().unwrap_or("normal") == "normal"
        && element.opacity == 1.0
        && !element.hidden.unwrap_or(false)
        && crop_is_empty(element.crop.as_ref())
        && transform_is_identity(&element.transform)
        && element.base.animations.as_ref().is_none_or(|animations| {
            animations.bindings.is_empty() && animations.channels.is_empty()
        })
}

fn untouched_sound(element: &VideoElement) -> bool {
    element.volume.unwrap_or(1.0) == 1.0
        && !element.muted.unwrap_or(false)
        && element.is_source_audio_enabled.unwrap_or(true)
}

fn lone_video_element(scene: &Scene) -> Option<&VideoElement> {
    if scene
        .tracks
        .overlay
        .iter()
        .any(|track| !track.elements().is_empty())
    {
        return None;
    }
    if scene
        .tracks
        .audio
        .iter()
        .any(|track| !track.elements().is_empty())
    {
        return None;
    }
    match &scene.tracks.main {
        Track::Video {
            elements, hidden, ..
        } if !*hidden && elements.len() == 1 => match &elements[0] {
            TimelineElement::Video(video) => Some(video),
            _ => None,
        },
        _ => None,
    }
}

fn scene_of<'a>(project: &'a Project, scene_id: Option<&str>) -> Option<&'a Scene> {
    match scene_id {
        Some(id) => project.scenes.iter().find(|scene| scene.id == id),
        None => project
            .scenes
            .iter()
            .find(|scene| scene.id == project.current_scene_id)
            .or_else(|| project.scenes.first()),
    }
}

pub fn plan(request: &ExportRequest, media: &dyn MediaResolver) -> Option<TrimPlan> {
    if extension_of(&request.destination) != "mp4" {
        return None;
    }
    if request.project.settings.watermark.is_some() {
        return None;
    }
    let canvas = &request.project.settings.canvas_size;
    if canvas.width != request.width || canvas.height != request.height {
        return None;
    }

    let scene = scene_of(&request.project, request.scene_id.as_deref())?;
    let element = lone_video_element(scene)?;
    if !untouched_picture(element) {
        return None;
    }
    if request.include_audio && !untouched_sound(element) {
        return None;
    }
    if element.base.start_time != MediaTime::ZERO {
        return None;
    }
    if element.base.duration <= MediaTime::ZERO {
        return None;
    }

    let source = media.resolve(&element.media_id)?;
    if !COPYABLE_CONTAINERS.contains(&extension_of(&source).as_str()) {
        return None;
    }

    let requested = request.frame_rate.as_f64()?;
    let probe = video::probe(&source).ok()?;
    if u32::from(probe.width) != request.width || u32::from(probe.height) != request.height {
        return None;
    }
    let native = if probe.duration_seconds > 0.0 && probe.frame_count > 0 {
        f64::from(probe.frame_count) / probe.duration_seconds
    } else {
        return None;
    };
    if (native - requested).abs() / requested > RATE_TOLERANCE {
        return None;
    }

    Some(TrimPlan {
        source,
        start: element.base.trim_start.max(MediaTime::ZERO),
        duration: element.base.duration,
        include_audio: request.include_audio,
        snap_to_keyframe: false,
    })
}

struct Boundary {
    first: u32,
    last: u32,
    origin: u64,
    end: u64,
}

fn starts_on_next_frame(
    target: u64,
    floor_start: u64,
    floor_is_sync: bool,
    next_start: Option<u64>,
    frame_ticks: u64,
) -> bool {
    if floor_start > target {
        return false;
    }
    let past_floor = target - floor_start;
    let Some(to_next) = next_start
        .filter(|next_start| *next_start > target)
        .map(|next_start| next_start - target)
    else {
        return false;
    };
    if to_next >= past_floor {
        return false;
    }
    let marginally_nearer = past_floor - to_next <= frame_ticks / 8;
    !(floor_is_sync && past_floor <= frame_ticks / 2 && marginally_nearer)
}

fn keeps_last_frame(end: u64, start: u64, next: u64) -> bool {
    end.saturating_sub(start) > next.saturating_sub(end)
}

fn sample_start(reader: &mut Mp4Reader<BufReader<File>>, track: u32, sample: u32) -> Option<u64> {
    reader
        .read_sample(track, sample)
        .ok()
        .flatten()
        .map(|sample| sample.start_time)
}

fn last_at_or_before(
    reader: &mut Mp4Reader<BufReader<File>>,
    track: u32,
    count: u32,
    target: u64,
) -> Option<u32> {
    last_before(reader, track, count, target.saturating_add(1)).or(Some(1))
}

fn last_before(
    reader: &mut Mp4Reader<BufReader<File>>,
    track: u32,
    count: u32,
    target: u64,
) -> Option<u32> {
    let mut low = 1u32;
    let mut high = count;
    let mut answer = None;
    while low <= high {
        let middle = low + (high - low) / 2;
        let start = sample_start(reader, track, middle)?;
        if start < target {
            answer = Some(middle);
            low = middle + 1;
        } else {
            if middle == 1 {
                break;
            }
            high = middle - 1;
        }
    }
    answer
}

fn first_at_or_after(
    reader: &mut Mp4Reader<BufReader<File>>,
    track: u32,
    count: u32,
    target: u64,
) -> Option<u32> {
    let mut low = 1u32;
    let mut high = count;
    let mut answer = None;
    while low <= high {
        let middle = low + (high - low) / 2;
        let start = sample_start(reader, track, middle)?;
        if start >= target {
            answer = Some(middle);
            if middle == 1 {
                break;
            }
            high = middle - 1;
        } else {
            low = middle + 1;
        }
    }
    answer
}

fn rescale(time: u64, from: u32, to: u32) -> u64 {
    if from == 0 {
        return 0;
    }
    let from = u128::from(from);
    let to = u128::from(to);
    let scaled = u128::from(time) * to + from / 2;
    u64::try_from(scaled / from).unwrap_or(u64::MAX)
}

fn ticks_to_timescale(time: MediaTime, timescale: u32) -> u64 {
    let seconds = time.to_seconds_f64().max(0.0);
    (seconds * f64::from(timescale)).round() as u64
}

fn video_boundary(
    reader: &mut Mp4Reader<BufReader<File>>,
    track: u32,
    count: u32,
    timescale: u32,
    plan: &TrimPlan,
    frame_ticks: u64,
) -> Result<Boundary> {
    let start_target = ticks_to_timescale(plan.start, timescale);
    let end_target = ticks_to_timescale(plan.end(), timescale);

    let floor = last_at_or_before(reader, track, count, start_target)
        .ok_or_else(|| ExportError::Encoder("the trim starts past the last frame".to_owned()))?;
    let floor_sample = reader
        .read_sample(track, floor)
        .map_err(|error| ExportError::Muxer(error.to_string()))?
        .ok_or_else(|| ExportError::Encoder("the first trimmed frame is missing".to_owned()))?;
    let next_start = (floor < count)
        .then(|| sample_start(reader, track, floor + 1))
        .flatten();
    let first = if starts_on_next_frame(
        start_target,
        floor_sample.start_time,
        floor_sample.is_sync,
        next_start,
        frame_ticks,
    ) {
        floor + 1
    } else {
        floor
    };
    let sample = reader
        .read_sample(track, first)
        .map_err(|error| ExportError::Muxer(error.to_string()))?
        .ok_or_else(|| ExportError::Encoder("the first trimmed frame is missing".to_owned()))?;

    let mut first = first;
    let mut sample = sample;
    if !sample.is_sync {
        if !plan.snap_to_keyframe {
            return Err(ExportError::Encoder(
                "the cut does not land on a keyframe".to_owned(),
            ));
        }
        let mut walk = first;
        loop {
            if walk <= 1 {
                walk = 1;
                break;
            }
            walk -= 1;
            let candidate = reader
                .read_sample(track, walk)
                .map_err(|error| ExportError::Muxer(error.to_string()))?;
            if candidate.as_ref().is_some_and(|sample| sample.is_sync) {
                break;
            }
        }
        first = walk;
        sample = reader
            .read_sample(track, first)
            .map_err(|error| ExportError::Muxer(error.to_string()))?
            .ok_or_else(|| ExportError::Encoder("no keyframe before the cut".to_owned()))?;
        if !sample.is_sync {
            return Err(ExportError::Encoder(
                "the source opens without a keyframe".to_owned(),
            ));
        }
    }
    let last = match last_before(reader, track, count, end_target) {
        Some(candidate) => {
            let candidate_sample = reader
                .read_sample(track, candidate)
                .map_err(|error| ExportError::Muxer(error.to_string()))?
                .ok_or_else(|| {
                    ExportError::Encoder("the last trimmed frame is missing".to_owned())
                })?;
            let next = if candidate < count {
                sample_start(reader, track, candidate + 1)
            } else {
                None
            }
            .unwrap_or(candidate_sample.start_time + u64::from(candidate_sample.duration));
            if keeps_last_frame(end_target, candidate_sample.start_time, next) {
                candidate
            } else {
                candidate.saturating_sub(1)
            }
        }
        None => first,
    }
    .max(first);

    Ok(Boundary {
        first,
        last,
        origin: sample.start_time,
        end: end_target,
    })
}

pub fn is_possible(request: &ExportRequest, media: &dyn MediaResolver) -> bool {
    plan(request, media).is_some()
}

pub fn trim(
    source: &Path,
    start: MediaTime,
    duration: MediaTime,
    include_audio: bool,
    destination: &Path,
) -> Result<ExportArtifacts> {
    if !COPYABLE_CONTAINERS.contains(&extension_of(source).as_str()) {
        return Err(ExportError::Encoder(format!(
            "{} is not a container the cut can be copied out of",
            extension_of(source)
        )));
    }
    let plan = TrimPlan {
        source: source.to_path_buf(),
        start: start.max(MediaTime::ZERO),
        duration,
        include_audio,
        snap_to_keyframe: true,
    };
    run(&plan, destination, &AtomicBool::new(false), &mut |_| {})
}

pub fn run(
    plan: &TrimPlan,
    destination: &Path,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<ExportArtifacts> {
    let started = Instant::now();
    let file = File::open(&plan.source).map_err(|error| ExportError::Io {
        path: plan.source.display().to_string(),
        detail: error.to_string(),
    })?;
    let size = file
        .metadata()
        .map_err(|error| ExportError::Io {
            path: plan.source.display().to_string(),
            detail: error.to_string(),
        })?
        .len();
    let mut reader = Mp4Reader::read_header(BufReader::new(file), size)
        .map_err(|error| ExportError::Muxer(error.to_string()))?;

    if reader.is_fragmented() {
        return Err(ExportError::Encoder(
            "a fragmented mp4 cannot be stream-copied".to_owned(),
        ));
    }

    let mut video_track = None;
    let mut audio_track = None;
    for (id, track) in reader.tracks() {
        match (track.track_type(), track.media_type()) {
            (Ok(TrackType::Video), Ok(MediaType::H264)) if video_track.is_none() => {
                video_track = Some(*id);
            }
            (Ok(TrackType::Audio), Ok(MediaType::AAC)) if audio_track.is_none() => {
                audio_track = Some(*id);
            }
            _ => {}
        }
    }
    let video_track = video_track
        .ok_or_else(|| ExportError::Encoder("no copyable h264 track in the source".to_owned()))?;
    let audio_track = plan.include_audio.then_some(audio_track).flatten();

    let (video_timescale, video_count, video_seconds, width, height, sps, pps) = {
        let track = reader
            .tracks()
            .get(&video_track)
            .ok_or_else(|| ExportError::Encoder("the video track vanished".to_owned()))?;
        (
            track.timescale(),
            track.sample_count(),
            track.duration().as_secs_f64(),
            track.width(),
            track.height(),
            track
                .sequence_parameter_set()
                .map_err(|error| ExportError::Muxer(error.to_string()))?
                .to_vec(),
            track
                .picture_parameter_set()
                .map_err(|error| ExportError::Muxer(error.to_string()))?
                .to_vec(),
        )
    };
    if video_count == 0 {
        return Err(ExportError::Empty);
    }

    let frame_ticks = (f64::from(video_timescale) * video_seconds / f64::from(video_count.max(1)))
        .round()
        .max(1.0) as u64;
    let boundary = video_boundary(
        &mut reader,
        video_track,
        video_count,
        video_timescale,
        plan,
        frame_ticks,
    )?;
    let total = u64::from(boundary.last - boundary.first + 1);

    let audio = match audio_track {
        Some(id) => {
            let track = reader
                .tracks()
                .get(&id)
                .ok_or_else(|| ExportError::Encoder("the audio track vanished".to_owned()))?;
            let config = AacConfig {
                bitrate: track.bitrate(),
                profile: track
                    .audio_profile()
                    .map_err(|error| ExportError::Muxer(error.to_string()))?,
                freq_index: track
                    .sample_freq_index()
                    .map_err(|error| ExportError::Muxer(error.to_string()))?,
                chan_conf: track
                    .channel_config()
                    .map_err(|error| ExportError::Muxer(error.to_string()))?,
            };
            Some((id, track.timescale(), track.sample_count(), config))
        }
        None => None,
    };

    let target = File::create(destination).map_err(|error| ExportError::Io {
        path: destination.display().to_string(),
        detail: error.to_string(),
    })?;
    let mut writer = Mp4Writer::write_start(
        BufWriter::new(target),
        &Mp4Config {
            major_brand: str::parse("isom").unwrap_or_default(),
            minor_version: 512,
            compatible_brands: vec![
                str::parse("isom").unwrap_or_default(),
                str::parse("iso2").unwrap_or_default(),
                str::parse("avc1").unwrap_or_default(),
                str::parse("mp41").unwrap_or_default(),
            ],
            timescale: 1_000,
        },
    )
    .map_err(|error| ExportError::Muxer(error.to_string()))?;

    writer
        .add_track(&TrackConfig {
            track_type: TrackType::Video,
            timescale: video_timescale,
            language: "und".to_owned(),
            media_conf: MediaConfig::AvcConfig(AvcConfig {
                width,
                height,
                seq_param_set: sps,
                pic_param_set: pps,
            }),
        })
        .map_err(|error| ExportError::Muxer(error.to_string()))?;
    if let Some((_, timescale, _, config)) = audio.as_ref() {
        writer
            .add_track(&TrackConfig {
                track_type: TrackType::Audio,
                timescale: *timescale,
                language: "und".to_owned(),
                media_conf: MediaConfig::AacConfig(config.clone()),
            })
            .map_err(|error| ExportError::Muxer(error.to_string()))?;
    }

    let mut copied = 0u64;
    let mut bytes = 0u64;
    for id in boundary.first..=boundary.last {
        if cancel.load(Ordering::Relaxed) {
            return Err(ExportError::Cancelled);
        }
        let Some(sample) = reader
            .read_sample(video_track, id)
            .map_err(|error| ExportError::Muxer(error.to_string()))?
        else {
            break;
        };
        bytes += sample.bytes.len() as u64;
        writer
            .write_sample(
                1,
                &Mp4Sample {
                    start_time: sample.start_time.saturating_sub(boundary.origin),
                    duration: sample.duration,
                    rendering_offset: sample.rendering_offset,
                    is_sync: sample.is_sync,
                    bytes: sample.bytes,
                },
            )
            .map_err(|error| ExportError::Muxer(error.to_string()))?;
        copied += 1;
        on_progress(Progress {
            stage: Stage::Packaging,
            frame: copied,
            total_frames: total,
            elapsed: started.elapsed(),
        });
    }

    let mut audio_samples = 0u64;
    if let Some((id, timescale, count, _)) = audio.as_ref() {
        let origin = rescale(boundary.origin, video_timescale, *timescale);
        let end_target = rescale(boundary.end, video_timescale, *timescale);

        let first = first_at_or_after(&mut reader, *id, *count, origin);
        let last = last_before(&mut reader, *id, *count, end_target);
        if let (Some(first), Some(last)) = (first, last) {
            for index in first..=last.max(first) {
                if cancel.load(Ordering::Relaxed) {
                    return Err(ExportError::Cancelled);
                }
                let Some(sample) = reader
                    .read_sample(*id, index)
                    .map_err(|error| ExportError::Muxer(error.to_string()))?
                else {
                    break;
                };
                bytes += sample.bytes.len() as u64;
                let duration = sample.duration;
                writer
                    .write_sample(
                        2,
                        &Mp4Sample {
                            start_time: sample.start_time.saturating_sub(origin),
                            duration,
                            rendering_offset: 0,
                            is_sync: true,
                            bytes: sample.bytes,
                        },
                    )
                    .map_err(|error| ExportError::Muxer(error.to_string()))?;
                audio_samples += u64::from(duration);
            }
        }
    }

    writer
        .write_end()
        .map_err(|error| ExportError::Muxer(error.to_string()))?;
    drop(writer);

    if audio_samples > 0 {
        crate::mp4_sink::patch_sl_config_predefined(destination)?;
    }

    if copied == 0 {
        return Err(ExportError::Empty);
    }

    let written = std::fs::metadata(destination)
        .map(|meta| meta.len())
        .unwrap_or(bytes);
    Ok(ExportArtifacts {
        video_path: Some(destination.to_path_buf()),
        audio_path: None,
        frames: copied,
        audio_samples,
        bytes: written,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_mp4_family_is_stream_copyable() {
        for name in ["a.mp4", "b.MOV", "c.m4v"] {
            assert!(COPYABLE_CONTAINERS.contains(&extension_of(Path::new(name)).as_str()));
        }
        for name in ["a.webm", "b.mkv", "c"] {
            assert!(!COPYABLE_CONTAINERS.contains(&extension_of(Path::new(name)).as_str()));
        }
    }

    #[test]
    fn a_cut_between_frames_starts_on_the_nearest_one() {
        assert!(!starts_on_next_frame(1_000, 0, false, Some(3_000), 3_000));
        assert!(starts_on_next_frame(2_000, 0, false, Some(3_000), 3_000));
        assert!(!starts_on_next_frame(1_500, 0, false, Some(3_000), 3_000));
        assert!(!starts_on_next_frame(2_900, 0, false, None, 3_000));
        assert!(!starts_on_next_frame(0, 100, false, Some(3_000), 3_000));
    }

    #[test]
    fn a_cut_near_a_keyframe_stays_on_it_even_when_the_next_frame_is_nearer() {
        assert!(!starts_on_next_frame(1_200, 0, true, Some(2_200), 3_000));
        assert!(starts_on_next_frame(1_200, 0, false, Some(2_200), 3_000));
        assert!(starts_on_next_frame(1_600, 0, true, Some(2_200), 3_000));
    }

    #[test]
    fn a_keyframe_does_not_hold_a_cut_whose_next_frame_is_far_nearer() {
        assert!(starts_on_next_frame(1_450, 0, true, Some(1_500), 3_000));
        assert!(!starts_on_next_frame(1_450, 0, true, Some(2_700), 3_000));
    }

    #[test]
    fn a_short_last_frame_of_a_whole_clip_copy_is_kept() {
        assert!(keeps_last_frame(88_000, 87_000, 88_000));
    }

    #[test]
    fn back_to_back_cuts_share_every_frame_out_exactly_once() {
        let frame = 3_003u64;
        for past in [0, 1, 1_000, 1_501, 1_502, 1_503, 2_000, 3_002] {
            let end = frame + past;
            let kept_by_first_cut = keeps_last_frame(end, frame, 2 * frame);
            let starts_second_cut =
                !starts_on_next_frame(end, frame, false, Some(2 * frame), frame);
            assert!(
                kept_by_first_cut != starts_second_cut,
                "{past} ticks in: kept {kept_by_first_cut}, starts the next cut {starts_second_cut}"
            );
        }
        assert!(!keeps_last_frame(1_500, 0, 3_000));
        assert!(!starts_on_next_frame(1_500, 0, false, Some(3_000), 3_000));
    }

    #[test]
    fn an_absent_crop_and_a_zero_crop_are_both_empty() {
        assert!(crop_is_empty(None));
        assert!(crop_is_empty(Some(&Crop::default())));
        assert!(!crop_is_empty(Some(&Crop {
            left: 0.1,
            ..Crop::default()
        })));
    }

    #[test]
    fn the_default_transform_is_the_identity_one() {
        assert!(transform_is_identity(&Transform::default()));
        let moved = Transform {
            scale_x: 1.5,
            ..Transform::default()
        };
        assert!(!transform_is_identity(&moved));
    }
}
