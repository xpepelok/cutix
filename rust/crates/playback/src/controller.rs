use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use cutix_project::Project;
use time::MediaTime;

use crate::error::{PlaybackError, Result};
use crate::media::MediaResolver;
use crate::render::{ComposeRequest, ComposedFrame, FrameComposer};

enum Command {
    Compose {
        time: MediaTime,
        width: u32,
        height: u32,
    },
    SetProject(Arc<Project>),
    ReleaseMedia(String),
    ClearCaches,
    Stop,
}

#[derive(Clone, Debug)]
pub struct FrameSlot {
    pub time: MediaTime,

    pub revision: u64,
    pub frame: Arc<ComposedFrame>,
}

struct Clock {
    anchor_time: MediaTime,
    anchor_instant: Instant,
    rate: f64,
    playing: bool,
}

impl Clock {
    fn now(&self) -> MediaTime {
        if !self.playing {
            return self.anchor_time;
        }
        let elapsed = self.anchor_instant.elapsed().as_secs_f64() * self.rate;
        self.anchor_time + MediaTime::from_seconds_f64(elapsed).unwrap_or(MediaTime::ZERO)
    }

    fn reanchor(&mut self) {
        self.anchor_time = self.now();
        self.anchor_instant = Instant::now();
    }
}

pub struct PlaybackController {
    commands: Sender<Command>,
    latest: Arc<Mutex<Option<FrameSlot>>>,
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
        let latest: Arc<Mutex<Option<FrameSlot>>> = Arc::new(Mutex::new(None));
        let errors: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let ready = Arc::new(AtomicBool::new(false));

        let worker_latest = Arc::clone(&latest);
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
                    worker_latest,
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
            latest,
            errors,
            ready,
            clock: Mutex::new(Clock {
                anchor_time: MediaTime::ZERO,
                anchor_instant: Instant::now(),
                rate: 1.0,
                playing: false,
            }),
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

    pub fn play(&self) {
        let mut clock = self.clock();
        clock.reanchor();
        clock.playing = true;
    }

    pub fn pause(&self) {
        let mut clock = self.clock();
        clock.reanchor();
        clock.playing = false;
    }

    pub fn is_playing(&self) -> bool {
        self.clock().playing
    }

    pub fn seek(&self, time: MediaTime) {
        let mut clock = self.clock();
        clock.anchor_time = time.max(MediaTime::ZERO);
        clock.anchor_instant = Instant::now();
    }

    pub fn set_rate(&self, rate: f64) {
        let mut clock = self.clock();
        clock.reanchor();
        clock.rate = if rate.is_finite() && rate > 0.0 {
            rate
        } else {
            1.0
        };
    }

    pub fn rate(&self) -> f64 {
        self.clock().rate
    }

    pub fn current_time(&self) -> MediaTime {
        self.clock().now()
    }

    pub fn set_project(&self, project: Arc<Project>) {
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
            time,
            width,
            height,
        });
    }

    pub fn latest_frame(&self) -> Option<FrameSlot> {
        self.latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn take_error(&self) -> Option<String> {
        self.errors
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
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

fn run_worker(
    mut composer: FrameComposer,
    mut project: Arc<Project>,
    scene_id: Option<String>,
    media: Box<dyn MediaResolver + Send>,
    requests: Receiver<Command>,
    latest: Arc<Mutex<Option<FrameSlot>>>,
    errors: Arc<Mutex<Option<String>>>,
) {
    let mut revision = 0u64;
    loop {
        let Ok(first) = requests.recv() else {
            return;
        };
        let mut pending = None;
        let mut released: Vec<String> = Vec::new();
        let mut clear_caches = false;
        let apply = |command: Command,
                     project: &mut Arc<Project>,
                     pending: &mut Option<Command>,
                     released: &mut Vec<String>,
                     clear_caches: &mut bool|
         -> bool {
            match command {
                Command::Stop => return false,

                Command::SetProject(next) => *project = next,
                Command::ReleaseMedia(id) => released.push(id),
                Command::ClearCaches => *clear_caches = true,
                compose => *pending = Some(compose),
            }
            true
        };
        if !apply(
            first,
            &mut project,
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

        if clear_caches {
            composer.clear_caches();
        }
        for media_id in &released {
            composer.release_media(media_id);
        }

        match pending {
            Some(Command::Stop)
            | Some(Command::SetProject(_))
            | Some(Command::ReleaseMedia(_))
            | Some(Command::ClearCaches)
            | None => {}
            Some(Command::Compose {
                time,
                width,
                height,
            }) => {
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
                        let mut slot = latest
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        *slot = Some(FrameSlot {
                            time,
                            revision,
                            frame: Arc::new(frame),
                        });
                    }
                    Err(error) => {
                        let mut slot = errors
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        *slot = Some(error.to_string());
                    }
                }
            }
        }
    }
}
