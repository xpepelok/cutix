use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cutix_i18n::{t, t_args};
use cutix_playback::{
    mix, AudioCache, AudioOutput, ElementRect, MixRequest, PlaybackController, PlaybackError,
    PlaybackGeneration, StoreResolver,
};
use cutix_project::model::MediaAssetData;
use cutix_project::{MediaStore, Project, ProjectStore};
use gpui::RenderImage;
use time::{FrameDuration, FrameRate, MediaTime};

fn describe_playback_error(error: &PlaybackError, media_assets: &[MediaAssetData]) -> String {
    let PlaybackError::MediaNotFound(id) = error else {
        return error.to_string();
    };
    match media_assets.iter().find(|asset| asset.id == *id) {
        Some(asset) => t_args("preview.mediaMissing", &[("name", &asset.name)]),
        None => t("preview.mediaMissing.unknown"),
    }
}

const AV_SYNC_TOLERANCE_TICKS: i64 = time::TICKS_PER_SECOND / 20;

const AV_SYNC_MAX_DRIFT_TICKS: i64 = time::TICKS_PER_SECOND * 2;

const AUDIO_STALL_TIMEOUT: Duration = Duration::from_millis(300);

const AUDIO_CHUNK_SECONDS: f64 = 0.2;
const AUDIO_LEAD_SECONDS: f64 = 1.0;
const FPS_WINDOW: Duration = Duration::from_millis(500);

enum AudioCommand {
    Project(Arc<Project>, Option<String>),
    Seek(MediaTime),
    Play(MediaTime),
    Pause,
    Volume(f32),
    Stop,
}

#[derive(Default)]
struct AudioClock {
    active: AtomicBool,
    ticks: AtomicU64,
    starved: AtomicU64,
}

impl AudioClock {
    fn publish(&self, time: MediaTime) {
        self.ticks.store(time.as_ticks() as u64, Ordering::Relaxed);
        self.active.store(true, Ordering::Release);
    }

    fn silence(&self) {
        self.active.store(false, Ordering::Release);
    }

    fn note_starvation(&self, count: u64) {
        self.starved.store(count, Ordering::Relaxed);
    }

    fn starvations(&self) -> u64 {
        self.starved.load(Ordering::Relaxed)
    }

    fn read(&self) -> Option<MediaTime> {
        if !self.active.load(Ordering::Acquire) {
            return None;
        }
        Some(MediaTime::from_ticks(
            self.ticks.load(Ordering::Relaxed) as i64
        ))
    }
}

struct AudioBridge {
    commands: Sender<AudioCommand>,
    warning: Arc<Mutex<Option<String>>>,
    clock: Arc<AudioClock>,
    /// Held so the bridge can wait for the worker in `Drop`. A detached worker outlives
    /// the bridge that owns it and keeps touching the audio device while the process is
    /// being torn down, which on Windows ends the process rather than the thread.
    worker: Option<std::thread::JoinHandle<()>>,
}

impl AudioBridge {
    fn spawn(
        project: Arc<Project>,
        scene_id: Option<String>,
        store: MediaStore,
        volume: f32,
    ) -> Option<Self> {
        let (commands, requests) = channel();
        let warning: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let worker_warning = Arc::clone(&warning);
        let clock: Arc<AudioClock> = Arc::new(AudioClock::default());
        let worker_clock = Arc::clone(&clock);

        let worker = std::thread::Builder::new()
            .name("cutix-preview-audio".into())
            .spawn(move || {
                let output = match AudioOutput::open() {
                    Ok(output) => output,
                    Err(error) => {
                        store_warning(&worker_warning, error.to_string());
                        return;
                    }
                };
                output.set_volume(volume);
                let resolver = StoreResolver::new(store);
                let mut cache = AudioCache::new();
                let mut project = project;
                let mut scene_id = scene_id;
                let mut cursor = MediaTime::ZERO;
                let mut origin = MediaTime::ZERO;
                let mut playing = false;
                let mut delivered = 0usize;
                let mut delivered_at = Instant::now();

                loop {
                    match requests.try_recv() {
                        Ok(AudioCommand::Project(next, scene)) => {
                            project = next;
                            scene_id = scene;
                        }
                        Ok(AudioCommand::Seek(time)) => {
                            cursor = time;
                            origin = time;
                            output.seek();
                            worker_clock.silence();
                        }
                        Ok(AudioCommand::Play(time)) => {
                            cursor = time;
                            origin = time;
                            output.seek();
                            output.start();
                            playing = true;
                            delivered = 0;
                            delivered_at = Instant::now();
                        }
                        Ok(AudioCommand::Pause) => {
                            output.pause();
                            output.seek();
                            playing = false;
                            delivered = 0;
                            worker_clock.silence();
                        }
                        Ok(AudioCommand::Volume(level)) => output.set_volume(level),
                        Ok(AudioCommand::Stop) | Err(TryRecvError::Disconnected) => {
                            let _ = output.stop();
                            return;
                        }
                        Err(TryRecvError::Empty) => {}
                    }

                    if !playing {
                        std::thread::sleep(Duration::from_millis(10));
                        continue;
                    }

                    worker_clock.note_starvation(output.starved_callbacks());
                    let consumed = output.consumed_samples();
                    if consumed != delivered {
                        delivered = consumed;
                        delivered_at = Instant::now();
                        let played = consumed as f64
                            / (output.sample_rate().max(1) as f64
                                * output.channels().max(1) as f64);
                        match MediaTime::from_seconds_f64(played) {
                            Some(offset) => worker_clock.publish(MediaTime::from_ticks(
                                origin.as_ticks() + offset.as_ticks(),
                            )),
                            None => worker_clock.silence(),
                        }
                    } else if delivered_at.elapsed() >= AUDIO_STALL_TIMEOUT {
                        worker_clock.silence();
                    }

                    let queued = output.queued_samples() as f64
                        / (output.sample_rate().max(1) as f64 * output.channels().max(1) as f64);
                    if queued >= AUDIO_LEAD_SECONDS {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }

                    let duration =
                        MediaTime::from_seconds_f64(AUDIO_CHUNK_SECONDS).unwrap_or(MediaTime::ZERO);
                    let request = MixRequest {
                        project: &project,
                        scene_id: scene_id.as_deref(),
                        start: cursor,
                        duration,
                        sample_rate: output.sample_rate(),
                        channels: output.channels(),
                    };
                    match mix(&request, &resolver, &mut cache) {
                        Ok((buffer, skipped)) => {
                            output.queue(&buffer);
                            if let Some(reason) = skipped.first() {
                                store_warning(&worker_warning, reason.clone());
                            }
                        }
                        Err(error) => store_warning(&worker_warning, error.to_string()),
                    }
                    cursor = MediaTime::from_ticks(cursor.as_ticks() + duration.as_ticks());
                }
            })
            .ok()?;

        Some(Self {
            commands,
            warning,
            clock,
            worker: Some(worker),
        })
    }

    fn send(&self, command: AudioCommand) {
        let _ = self.commands.send(command);
    }

    fn position(&self) -> Option<MediaTime> {
        self.clock.read()
    }

    fn starvations(&self) -> u64 {
        self.clock.starvations()
    }

    fn warning(&self) -> Option<String> {
        self.warning
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

impl Drop for AudioBridge {
    fn drop(&mut self) {
        let _ = self.commands.send(AudioCommand::Stop);
        // Waited for, not just asked to stop. Swapping projects drops one bridge and
        // spawns the next, so without this the old worker is still holding the output
        // device when the new one opens it, and still running when the process exits.
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn store_warning(slot: &Arc<Mutex<Option<String>>>, message: String) {
    let mut guard = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.as_deref() != Some(message.as_str()) {
        *guard = Some(message);
    }
}

/// Everything about a running frame stream that, if it changes, makes the stream wrong.
///
/// Comparing the whole key each tick is what ties the preview to the controller's
/// cancellation contract: a seek, a project swap or an audio clock correction advances the
/// generation, the key stops matching, and the stream is rebuilt from the corrected clock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StreamKey {
    width: u32,
    height: u32,
    frame: FrameDuration,
    generation: PlaybackGeneration,
}

pub struct PreviewEngine {
    controller: Option<PlaybackController>,
    audio: Option<AudioBridge>,
    scene_id: Option<String>,
    pub image: Option<Arc<RenderImage>>,

    pub rects: Vec<(String, ElementRect)>,

    pub frame_size: (u32, u32),
    image_time: MediaTime,
    image_revision: u64,
    pub error: Option<String>,
    pending: bool,

    requested: Option<(i64, u32, u32)>,
    /// The stream currently running, keyed by everything that would invalidate it: the
    /// output size, the frame duration and the playback generation it was started in.
    /// When the controller advances its generation — a seek, a project swap, an audio
    /// clock correction — this no longer matches and the stream is restarted from the
    /// corrected clock position.
    streaming: Option<StreamKey>,
    audio_starvations: std::cell::Cell<u64>,
    volume: f32,
    muted: bool,
    presented: u32,
    window_started: Instant,
    pub frames_per_second: f32,
    thumbnail: Option<Thumbnail>,
    thumbnail_taken: Option<Instant>,
}

#[derive(Clone)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

const THUMBNAIL_MAX_EDGE: u32 = 480;
const THUMBNAIL_INTERVAL: Duration = Duration::from_secs(10);

impl Default for PreviewEngine {
    fn default() -> Self {
        Self {
            controller: None,
            audio: None,
            scene_id: None,
            image: None,
            rects: Vec::new(),
            frame_size: (0, 0),
            image_time: MediaTime::ZERO,
            image_revision: 0,
            error: None,
            pending: false,
            requested: None,
            streaming: None,
            audio_starvations: std::cell::Cell::new(0),
            volume: 1.0,
            muted: false,
            presented: 0,
            window_started: Instant::now(),
            frames_per_second: 0.0,
            thumbnail: None,
            thumbnail_taken: None,
        }
    }
}

impl PreviewEngine {
    pub fn open(
        &mut self,
        project: &Project,
        scene_id: Option<String>,
        store: &ProjectStore,
        media_assets: &[MediaAssetData],
    ) -> Option<Arc<RenderImage>> {
        let media = MediaStore::for_project(store, &project.metadata.id);
        let document = Arc::new(project.clone());
        let stale = self.image.take();
        self.thumbnail = None;
        self.thumbnail_taken = None;

        match PlaybackController::new(
            Arc::clone(&document),
            scene_id.clone(),
            Box::new(StoreResolver::new(MediaStore::for_project(
                store,
                &project.metadata.id,
            ))),
            Some(store.project_directory(&project.metadata.id)),
        ) {
            Ok(controller) => {
                self.controller = Some(controller);
                self.error = None;
            }
            Err(error) => {
                self.controller = None;
                self.error = Some(describe_playback_error(&error, media_assets));
            }
        }
        self.audio = AudioBridge::spawn(document, scene_id.clone(), media, self.effective_volume());
        self.scene_id = scene_id;
        self.image_time = MediaTime::ZERO;
        self.pending = false;
        self.requested = None;
        stale
    }

    pub fn close(&mut self) -> Option<Arc<RenderImage>> {
        self.controller = None;
        self.audio = None;
        self.error = None;
        self.frames_per_second = 0.0;
        self.image.take()
    }

    pub fn is_open(&self) -> bool {
        self.controller.is_some()
    }

    pub fn is_pending(&self) -> bool {
        self.pending
    }

    pub fn restore_audio(&mut self, volume: f32, muted: bool) {
        self.volume = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.muted = muted;
    }

    pub fn volume(&self) -> f32 {
        self.volume
    }

    pub fn is_muted(&self) -> bool {
        self.muted
    }

    pub fn set_volume(&mut self, volume: f32) {
        let clamped = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.volume = clamped;
        self.muted = clamped <= 0.0;
        self.push_volume();
    }

    pub fn set_muted(&mut self, muted: bool) {
        self.muted = muted;
        if !muted && self.volume <= 0.0 {
            self.volume = 1.0;
        }
        self.push_volume();
    }

    pub fn toggle_mute(&mut self) {
        self.set_muted(!self.muted);
    }

    fn effective_volume(&self) -> f32 {
        if self.muted {
            0.0
        } else {
            self.volume
        }
    }

    fn push_volume(&self) {
        if let Some(audio) = self.audio.as_ref() {
            audio.send(AudioCommand::Volume(self.effective_volume()));
        }
    }

    pub fn audio_warning(&self) -> Option<String> {
        self.audio.as_ref().and_then(AudioBridge::warning)
    }

    pub fn sync_project(&mut self, project: &Project) {
        let document = Arc::new(project.clone());
        if let Some(controller) = self.controller.as_ref() {
            controller.set_project(Arc::clone(&document));
        }
        if let Some(audio) = self.audio.as_ref() {
            audio.send(AudioCommand::Project(document, self.scene_id.clone()));
        }
        self.pending = false;
        self.requested = None;
    }

    pub fn is_playing(&self) -> bool {
        self.controller
            .as_ref()
            .is_some_and(PlaybackController::is_playing)
    }

    pub fn current_time(&self) -> MediaTime {
        self.controller
            .as_ref()
            .map(PlaybackController::current_time)
            .unwrap_or(MediaTime::ZERO)
    }

    pub fn play(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        controller.play();
        self.streaming = None;
        self.presented = 0;
        self.window_started = Instant::now();
        if let Some(audio) = self.audio.as_ref() {
            audio.send(AudioCommand::Play(controller.current_time()));
        }
    }

    pub fn pause(&mut self) {
        if let Some(controller) = self.controller.as_ref() {
            controller.pause();
        }
        if let Some(audio) = self.audio.as_ref() {
            audio.send(AudioCommand::Pause);
        }
        self.streaming = None;
        self.frames_per_second = 0.0;
    }

    pub fn toggle(&mut self) {
        if self.is_playing() {
            self.pause();
        } else {
            self.play();
        }
    }

    pub fn seek(&mut self, time: MediaTime) {
        let time = time.max(MediaTime::ZERO);
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        // Seeking ends the controller's current generation, which drops every queued and
        // in-flight frame composed for the old playhead position.
        controller.seek(time);
        if let Some(audio) = self.audio.as_ref() {
            audio.send(if controller.is_playing() {
                AudioCommand::Play(time)
            } else {
                AudioCommand::Seek(time)
            });
        }
        self.pending = false;
        self.requested = None;
        self.streaming = None;
    }

    pub fn take_thumbnail(&mut self) -> Option<Thumbnail> {
        self.thumbnail.take()
    }

    pub fn tick(&mut self, width: u32, height: u32, rate: FrameRate) -> Option<Arc<RenderImage>> {
        let controller = self.controller.as_ref()?;
        if let Some(error) = controller.take_error() {
            self.error = Some(error);
        }
        self.reconcile_audio_clock(controller);

        // Every valid rate has an exact frame duration. A rate that has none is not a
        // rate at all, and there is no honest frame length to substitute for it, so the
        // preview holds its last picture rather than racing through the timeline.
        let frame = rate.frame_duration()?;
        let playing = controller.is_playing();
        let clock = controller.current_time();
        let clock_frame = frame.frame_floor(clock.as_ticks()).unwrap_or(0);
        let frame_time = MediaTime::from_frame(clock_frame, rate).unwrap_or(MediaTime::ZERO);
        let slot = controller.latest_frame();

        if playing {
            let wanted = StreamKey {
                width: width.max(1),
                height: height.max(1),
                frame,
                generation: controller.generation(),
            };
            if self.streaming != Some(wanted) {
                controller.stream_from(frame_time, wanted.width, wanted.height, frame);
                self.streaming = Some(wanted);
                self.requested = None;
                self.pending = true;
            }
        } else {
            self.streaming = None;
            let request = (clock_frame, width.max(1), height.max(1));
            if self.requested != Some(request) {
                controller.request_frame(frame_time, request.1, request.2);
                self.requested = Some(request);
                self.pending = true;
            }
        }

        let slot = slot?;
        if self.image.is_some() && slot.revision == self.image_revision {
            return None;
        }

        let composed = &slot.frame;
        if composed.width == 0 || composed.height == 0 {
            return None;
        }
        let image = to_render_image(composed.width, composed.height, &composed.pixels)?;
        if self
            .thumbnail_taken
            .is_none_or(|taken| taken.elapsed() >= THUMBNAIL_INTERVAL)
        {
            self.thumbnail = shrink(composed.width, composed.height, &composed.pixels);
            self.thumbnail_taken = Some(Instant::now());
        }
        self.rects = composed.rects.clone();
        self.frame_size = (composed.width, composed.height);
        let stale = self.image.replace(image);
        self.image_time = slot.time;
        self.image_revision = slot.revision;
        self.pending = false;
        self.error = None;

        self.presented += 1;
        let elapsed = self.window_started.elapsed();
        if elapsed >= FPS_WINDOW {
            self.frames_per_second = self.presented as f32 / elapsed.as_secs_f32();
            self.presented = 0;
            self.window_started = Instant::now();
        }
        stale
    }
}

impl PreviewEngine {
    fn reconcile_audio_clock(&self, controller: &PlaybackController) {
        if !controller.is_playing() {
            return;
        }
        let Some(bridge) = self.audio.as_ref() else {
            return;
        };
        let starvations = bridge.starvations();
        let starved = starvations != self.audio_starvations.get();
        self.audio_starvations.set(starvations);
        if starved {
            return;
        }
        let Some(audio) = bridge.position() else {
            return;
        };
        let drift = audio.as_ticks() - controller.current_time().as_ticks();
        if drift.abs() < AV_SYNC_TOLERANCE_TICKS || drift.abs() > AV_SYNC_MAX_DRIFT_TICKS {
            return;
        }
        // Corrected through `retime`, not `seek`: the queued frames carry their own
        // timeline times and stay valid across a clock correction, and the stream key in
        // `tick` keeps matching so the stream runs on. Correcting through `seek` would
        // discard the pipeline every time the clock drifted, and drift past the tolerance
        // is the normal state of an audio device running a second of lead — the picture
        // would sit still while the sound played on.
        controller.retime(audio);
    }
}

fn shrink(width: u32, height: u32, rgba: &[u8]) -> Option<Thumbnail> {
    if width == 0 || height == 0 || rgba.len() < (width as usize) * (height as usize) * 4 {
        return None;
    }
    let long = width.max(height);
    let (target_width, target_height) = if long <= THUMBNAIL_MAX_EDGE {
        (width, height)
    } else {
        let scale = f64::from(THUMBNAIL_MAX_EDGE) / f64::from(long);
        (
            ((f64::from(width) * scale).round() as u32).max(1),
            ((f64::from(height) * scale).round() as u32).max(1),
        )
    };

    let mut pixels = Vec::with_capacity((target_width as usize) * (target_height as usize) * 4);
    for row in 0..target_height {
        let source_row = (row as u64 * height as u64 / target_height as u64) as u32;
        for column in 0..target_width {
            let source_column = (column as u64 * width as u64 / target_width as u64) as u32;
            let offset = ((source_row as usize) * (width as usize) + source_column as usize) * 4;
            pixels.extend_from_slice(&rgba[offset..offset + 4]);
        }
    }

    Some(Thumbnail {
        width: target_width,
        height: target_height,
        rgba: pixels,
    })
}

fn to_render_image(width: u32, height: u32, rgba: &[u8]) -> Option<Arc<RenderImage>> {
    let expected = (width as usize) * (height as usize) * 4;
    if rgba.len() < expected {
        return None;
    }
    let mut bgra = rgba[..expected].to_vec();
    for pixel in bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let buffer = image::ImageBuffer::from_raw(width, height, bgra)?;
    Some(Arc::new(RenderImage::new(vec![image::Frame::new(buffer)])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thumbnail_is_capped_on_its_long_edge_and_keeps_its_aspect() {
        let rgba = vec![7u8; 1920 * 1080 * 4];
        let thumbnail = shrink(1920, 1080, &rgba).expect("thumbnail");
        assert_eq!((thumbnail.width, thumbnail.height), (480, 270));
        assert_eq!(
            thumbnail.rgba.len(),
            (thumbnail.width as usize) * (thumbnail.height as usize) * 4
        );
        assert!(thumbnail.rgba.iter().all(|byte| *byte == 7));
    }

    #[test]
    fn a_small_frame_is_copied_rather_than_upscaled() {
        let rgba = vec![1u8; 64 * 64 * 4];
        let thumbnail = shrink(64, 64, &rgba).expect("thumbnail");
        assert_eq!((thumbnail.width, thumbnail.height), (64, 64));
    }

    #[test]
    fn a_thumbnail_samples_across_the_whole_frame() {
        let (width, height) = (960u32, 540u32);
        let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
        let mark = |rgba: &mut Vec<u8>, x: u32, y: u32, value: u8| {
            rgba[((y as usize) * (width as usize) + x as usize) * 4] = value;
        };
        mark(&mut rgba, 0, 0, 200);
        mark(&mut rgba, 958, 538, 111);

        let thumbnail = shrink(width, height, &rgba).expect("thumbnail");
        assert_eq!((thumbnail.width, thumbnail.height), (480, 270));
        assert_eq!(thumbnail.rgba[0], 200);
        let corner = ((269 * 480) + 479) * 4;
        assert_eq!(
            thumbnail.rgba[corner], 111,
            "the far corner of the source must reach the far corner of the thumbnail"
        );
    }

    #[test]
    fn a_frame_with_a_short_buffer_yields_no_thumbnail() {
        assert!(shrink(64, 64, &[0u8; 16]).is_none());
        assert!(shrink(0, 0, &[]).is_none());
    }

    #[test]
    fn rgba_pixels_are_swapped_into_bgra() {
        let rgba = vec![10u8, 20, 30, 255];
        let image = to_render_image(1, 1, &rgba).expect("image");
        assert_eq!(image.as_bytes(0), Some(&[30u8, 20, 10, 255][..]));
    }

    #[test]
    fn a_short_buffer_is_rejected_instead_of_panicking() {
        assert!(to_render_image(4, 4, &[0, 0, 0, 255]).is_none());
    }

    fn asset(id: &str, name: &str) -> MediaAssetData {
        MediaAssetData {
            id: id.to_owned(),
            name: name.to_owned(),
            media_type: cutix_project::MediaType::Video,
            size: 0,
            last_modified: 0,
            width: None,
            height: None,
            duration: None,
            fps: None,
            has_audio: None,
            ephemeral: false,
            thumbnail_url: None,
            file_name: None,
            source_path: None,
        }
    }

    #[test]
    fn a_missing_file_is_named_after_its_asset() {
        cutix_i18n::bootstrap();
        cutix_i18n::set_locale("en");
        let assets = vec![asset("m1", "beach.mp4")];
        let message = describe_playback_error(&PlaybackError::MediaNotFound("m1".into()), &assets);
        assert!(message.contains("beach.mp4"), "{message}");
        assert!(!message.contains("m1"), "{message}");
        assert_ne!(message, "preview.mediaMissing");
    }

    #[test]
    fn an_unknown_id_falls_back_to_the_generic_wording() {
        cutix_i18n::bootstrap();
        cutix_i18n::set_locale("en");
        let assets = vec![asset("m1", "beach.mp4")];
        let message =
            describe_playback_error(&PlaybackError::MediaNotFound("gone".into()), &assets);
        assert_eq!(message, cutix_i18n::t("preview.mediaMissing.unknown"));
        assert!(!message.contains("gone"));
    }

    #[test]
    fn every_other_variant_is_passed_through_verbatim() {
        let error = PlaybackError::UnsupportedMedia("no decoder for 'aac'".into());
        assert_eq!(
            describe_playback_error(&error, &[]),
            error.to_string(),
            "only the missing-media case is reworded"
        );
    }

    #[test]
    fn a_fresh_engine_reports_no_transport() {
        let engine = PreviewEngine::default();
        assert!(!engine.is_open());
        assert!(!engine.is_playing());
        assert_eq!(engine.current_time(), MediaTime::ZERO);
    }
}
