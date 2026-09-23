use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use cutix_project::Project;
use time::{FrameDuration, MediaTime};

use crate::clock::Clock;
use crate::error::{PlaybackError, Result};
use crate::media::MediaResolver;
use crate::queue::{FrameQueue, FrameSlot, PlaybackGeneration};
use crate::render::{ComposeRequest, FrameComposer, PendingFrame};

pub use crate::queue::QUEUE_DEPTH;

const PIPELINE_DEPTH: usize = 2;

const IDLE_NAP: Duration = Duration::from_millis(1);

const GIVE_UP_AFTER: usize = 30;

const STALL_NAP: Duration = Duration::from_millis(20);

fn nap_after_failure(refused: usize) -> Duration {
    if refused >= GIVE_UP_AFTER {
        STALL_NAP
    } else {
        IDLE_NAP
    }
}

enum Command {
    Compose {
        generation: PlaybackGeneration,
        time: MediaTime,
        width: u32,
        height: u32,
    },
    Stream {
        generation: PlaybackGeneration,
        start: MediaTime,
        width: u32,
        height: u32,
        frame: FrameDuration,
    },
    SetProject(Arc<Project>),
    ReleaseMedia(String),
    ClearCaches,
    Stop,
}

pub struct PlaybackController {
    commands: Sender<Command>,
    queue: FrameQueue,
    presented: Mutex<Option<FrameSlot>>,
    errors: Arc<Mutex<Option<String>>>,
    ready: Arc<AtomicBool>,
    clock: Mutex<Clock>,
    scene_id: Option<String>,
    worker: Option<JoinHandle<()>>,
}

impl PlaybackController {
    pub fn new(
        project: Arc<Project>,
        scene_id: Option<String>,
        media: Box<dyn MediaResolver + Send>,
        matte_root: Option<std::path::PathBuf>,
    ) -> Result<Self> {
        let (commands, requests) = channel();
        let queue = FrameQueue::new();
        let errors: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let ready = Arc::new(AtomicBool::new(false));

        let worker_queue = queue.clone();
        let worker_errors = Arc::clone(&errors);
        let worker_ready = Arc::clone(&ready);
        let worker_scene = scene_id.clone();
        let (started, start_signal) = channel();

        let worker = std::thread::Builder::new()
            .name("cutix-playback".into())
            .spawn(move || {
                let composer = match FrameComposer::new() {
                    Ok(mut composer) => {
                        composer.set_matte_root(matte_root);
                        let _ = started.send(None);
                        composer
                    }
                    Err(error) => {
                        let _ = started.send(Some(error.to_string()));
                        return;
                    }
                };
                worker_ready.store(true, Ordering::Release);
                run_worker(
                    composer,
                    project,
                    worker_scene,
                    media,
                    requests,
                    worker_queue,
                    worker_errors,
                );
            })
            .map_err(|error| PlaybackError::Io(error.to_string()))?;

        match start_signal.recv() {
            Ok(None) => {}
            Ok(Some(error)) => return Err(PlaybackError::Gpu(error)),
            Err(error) => return Err(PlaybackError::Io(error.to_string())),
        }

        Ok(Self {
            commands,
            queue,
            presented: Mutex::new(None),
            errors,
            ready,
            clock: Mutex::new(Clock::stopped_at_start()),
            scene_id,
            worker: Some(worker),
        })
    }

    pub fn scene_id(&self) -> Option<&str> {
        self.scene_id.as_deref()
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    pub fn generation(&self) -> PlaybackGeneration {
        self.queue.generation()
    }

    pub fn play(&self) {
        self.clock().play();
    }

    pub fn pause(&self) {
        self.clock().pause();
    }

    pub fn is_playing(&self) -> bool {
        self.clock().is_playing()
    }

    pub fn seek(&self, time: MediaTime) {
        self.clock().seek(time);
        self.invalidate();
    }

    pub fn retime(&self, time: MediaTime) {
        self.clock().seek(time);
    }

    pub fn set_rate(&self, rate: f64) {
        self.clock().set_rate(rate);
    }

    pub fn rate(&self) -> f64 {
        self.clock().rate()
    }

    pub fn current_time(&self) -> MediaTime {
        self.clock().now()
    }

    pub fn set_project(&self, project: Arc<Project>) {
        self.invalidate();
        let _ = self.commands.send(Command::SetProject(project));
    }

    pub fn release_media(&self, media_id: &str) {
        let _ = self
            .commands
            .send(Command::ReleaseMedia(media_id.to_owned()));
    }

    pub fn clear_caches(&self) {
        let _ = self.commands.send(Command::ClearCaches);
    }

    pub fn request_frame(&self, time: MediaTime, width: u32, height: u32) {
        let _ = self.commands.send(Command::Compose {
            generation: self.queue.generation(),
            time,
            width,
            height,
        });
    }

    pub fn stream_from(&self, start: MediaTime, width: u32, height: u32, frame: FrameDuration) {
        let _ = self.commands.send(Command::Stream {
            generation: self.queue.generation(),
            start,
            width,
            height,
            frame,
        });
    }

    pub fn queued_frames(&self) -> usize {
        self.queue.len()
    }

    pub fn discard_queued(&self) {
        self.invalidate();
    }

    pub fn latest_frame(&self) -> Option<FrameSlot> {
        let due = self.current_time();
        let playing = self.is_playing();
        let generation = self.queue.generation();
        let mut presented = self
            .presented
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if presented
            .as_ref()
            .is_some_and(|slot| slot.generation != generation)
        {
            *presented = None;
        }
        let force_first = presented.is_none();
        if let Some(fresh) = self
            .queue
            .take_due(generation, force_first, |slot| !playing || slot.time <= due)
        {
            *presented = Some(fresh);
        }
        presented.clone()
    }

    pub fn take_error(&self) -> Option<String> {
        self.errors
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }

    fn invalidate(&self) -> PlaybackGeneration {
        let generation = self.queue.invalidate();
        *self
            .presented
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        generation
    }

    fn clock(&self) -> std::sync::MutexGuard<'_, Clock> {
        self.clock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Drop for PlaybackController {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

struct Stream {
    generation: PlaybackGeneration,
    start: MediaTime,
    width: u32,
    height: u32,
    frame: FrameDuration,
    index: i64,
}

impl Stream {
    fn time_of(&self, index: i64) -> Option<MediaTime> {
        let offset = self.frame.ticks_at_frame(index)?;
        Some(MediaTime::from_ticks(
            self.start.as_ticks().checked_add(offset)?,
        ))
    }
}

fn store_error(errors: &Mutex<Option<String>>, detail: String) {
    *errors
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(detail);
}

fn run_worker(
    mut composer: FrameComposer,
    mut project: Arc<Project>,
    scene_id: Option<String>,
    media: Box<dyn MediaResolver + Send>,
    requests: Receiver<Command>,
    queue: FrameQueue,
    errors: Arc<Mutex<Option<String>>>,
) {
    let mut revision = 0u64;
    let mut stream: Option<Stream> = None;
    let mut in_flight: VecDeque<(MediaTime, PlaybackGeneration, PendingFrame)> = VecDeque::new();
    let mut refused = 0usize;

    composer.set_pipeline_depth(PIPELINE_DEPTH);

    loop {
        let idle = stream.is_none() && in_flight.is_empty();
        let first = if idle {
            match requests.recv() {
                Ok(command) => Some(command),
                Err(_) => return,
            }
        } else {
            match requests.try_recv() {
                Ok(command) => Some(command),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => return,
            }
        };

        let mut pending = None;
        let mut released: Vec<String> = Vec::new();
        let mut clear_caches = false;
        let mut project_replaced = false;
        let apply = |command: Command,
                     project: &mut Arc<Project>,
                     project_replaced: &mut bool,
                     pending: &mut Option<Command>,
                     released: &mut Vec<String>,
                     clear_caches: &mut bool|
         -> bool {
            match command {
                Command::Stop => return false,

                Command::SetProject(next) => {
                    *project = next;
                    *project_replaced = true;
                }
                Command::ReleaseMedia(id) => released.push(id),
                Command::ClearCaches => *clear_caches = true,
                compose => *pending = Some(compose),
            }
            true
        };

        if let Some(first) = first {
            if !apply(
                first,
                &mut project,
                &mut project_replaced,
                &mut pending,
                &mut released,
                &mut clear_caches,
            ) {
                return;
            }

            loop {
                match requests.try_recv() {
                    Ok(command) => {
                        if !apply(
                            command,
                            &mut project,
                            &mut project_replaced,
                            &mut pending,
                            &mut released,
                            &mut clear_caches,
                        ) {
                            return;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }
        }

        if clear_caches {
            composer.clear_caches();
        }
        for media_id in &released {
            composer.release_media(media_id);
        }

        if project_replaced {
            stream = None;
            in_flight.clear();
            queue.clear();
            refused = 0;
        }

        match pending {
            Some(Command::Compose {
                generation,
                time,
                width,
                height,
            }) => {
                stream = None;
                in_flight.clear();
                let request = ComposeRequest {
                    project: &project,
                    scene_id: scene_id.as_deref(),
                    time,
                    width,
                    height,
                };
                match composer.compose(&request, media.as_ref()) {
                    Ok(frame) => {
                        revision += 1;
                        queue.replace_with(FrameSlot {
                            time,
                            generation,
                            revision,
                            frame: Arc::new(frame),
                        });
                    }
                    Err(error) => store_error(&errors, error.to_string()),
                }
                continue;
            }
            Some(Command::Stream {
                generation,
                start,
                width,
                height,
                frame,
            }) => {
                in_flight.clear();
                queue.clear();
                refused = 0;
                stream = Some(Stream {
                    generation,
                    start,
                    width,
                    height,
                    frame,
                    index: 0,
                });
            }
            _ => {}
        }

        let Some(active) = stream.as_mut() else {
            in_flight.clear();
            std::thread::sleep(IDLE_NAP);
            continue;
        };

        let mut failed = None;
        while in_flight.len() < PIPELINE_DEPTH && queue.len() + in_flight.len() < QUEUE_DEPTH {
            let Some(time) = active.time_of(active.index) else {
                stream = None;
                break;
            };
            let request = ComposeRequest {
                project: &project,
                scene_id: scene_id.as_deref(),
                time,
                width: active.width,
                height: active.height,
            };
            match composer.submit_frame(&request, media.as_ref()) {
                Ok(frame) => {
                    refused = 0;
                    in_flight.push_back((time, active.generation, frame));
                    active.index += 1;
                }
                Err(error) => {
                    refused += 1;
                    active.index += 1;
                    failed = Some(error.to_string());
                    break;
                }
            }
        }

        if let Some(detail) = failed {
            store_error(&errors, detail);
            std::thread::sleep(nap_after_failure(refused));
            continue;
        }

        match in_flight.pop_front() {
            Some((time, generation, frame)) => {
                revision += 1;
                queue.publish(FrameSlot {
                    time,
                    generation,
                    revision,
                    frame: Arc::new(composer.resolve(frame)),
                });
            }
            None => std::thread::sleep(IDLE_NAP),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::FrameRate;

    fn stream_at(rate: FrameRate) -> Stream {
        Stream {
            generation: PlaybackGeneration::FIRST,
            start: MediaTime::from_ticks(1_000),
            width: 320,
            height: 240,
            frame: rate.frame_duration().expect("a valid rate"),
            index: 0,
        }
    }

    #[test]
    fn a_stream_walks_forward_one_frame_at_a_time() {
        let stream = stream_at(FrameRate::FPS_60);
        assert_eq!(stream.time_of(0).map(MediaTime::as_ticks), Some(1_000));
        assert_eq!(stream.time_of(1).map(MediaTime::as_ticks), Some(3_000));
        assert_eq!(stream.time_of(30).map(MediaTime::as_ticks), Some(61_000));
    }

    #[test]
    fn a_stream_at_a_rate_with_a_fractional_frame_still_runs_at_the_right_speed() {
        let rate = FrameRate::nearest(23.0).expect("a real rate");
        assert_eq!(rate.ticks_per_frame(), None);
        let stream = stream_at(rate);
        assert_eq!(stream.time_of(23).map(MediaTime::as_ticks), Some(121_000));
        let last = stream.time_of(23 * 3_600).expect("an hour of frames");
        assert_eq!(last.as_ticks(), 1_000 + 3_600 * 120_000);
    }

    #[test]
    fn a_run_of_failures_slows_the_worker_down_rather_than_stopping_it() {
        assert_eq!(nap_after_failure(0), IDLE_NAP);
        assert_eq!(nap_after_failure(GIVE_UP_AFTER - 1), IDLE_NAP);
        assert_eq!(nap_after_failure(GIVE_UP_AFTER), STALL_NAP);
        assert!(
            STALL_NAP > IDLE_NAP,
            "a stalled stream must stop hammering the decoder"
        );
    }
}
