use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::error::{PlaybackError, Result};
use crate::mix::AudioBuffer;

#[derive(Default)]
struct Shared {
    queue: VecDeque<f32>,
    last_callback: Vec<f32>,
    last_delivery: Vec<f32>,
    consumed: usize,
}

pub struct AudioOutput {
    stream: cpal::Stream,
    shared: Arc<Mutex<Shared>>,
    playing: Arc<AtomicBool>,
    gain: Arc<AtomicU32>,
    played_frames: Arc<AtomicU64>,
    callbacks: Arc<AtomicU64>,
    sample_rate: u32,
    channels: usize,
}

impl AudioOutput {
    pub fn open() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| PlaybackError::AudioDevice("no default output device".into()))?;
        let config = device
            .default_output_config()
            .map_err(|error| PlaybackError::AudioDevice(error.to_string()))?;
        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        let shared = Arc::new(Mutex::new(Shared::default()));
        let playing = Arc::new(AtomicBool::new(false));
        let played_frames = Arc::new(AtomicU64::new(0));
        let callbacks = Arc::new(AtomicU64::new(0));
        let gain = Arc::new(AtomicU32::new(1.0f32.to_bits()));

        let callback_shared = Arc::clone(&shared);
        let callback_playing = Arc::clone(&playing);
        let callback_frames = Arc::clone(&played_frames);
        let callback_count = Arc::clone(&callbacks);
        let callback_gain = Arc::clone(&gain);

        let stream = device
            .build_output_stream(
                &config.config(),
                move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    callback_count.fetch_add(1, Ordering::Relaxed);
                    let mut shared = match callback_shared.lock() {
                        Ok(shared) => shared,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    if !callback_playing.load(Ordering::Relaxed) {
                        output.fill(0.0);
                        shared.last_callback.clear();
                        shared.last_callback.extend_from_slice(output);
                        return;
                    }
                    let gain = f32::from_bits(callback_gain.load(Ordering::Relaxed));
                    let mut delivered = 0usize;
                    for slot in output.iter_mut() {
                        match shared.queue.pop_front() {
                            Some(sample) => {
                                *slot = sample * gain;
                                delivered += 1;
                            }
                            None => *slot = 0.0,
                        }
                    }
                    callback_frames
                        .fetch_add((output.len() / channels.max(1)) as u64, Ordering::Relaxed);
                    shared.last_callback.clear();
                    shared.last_callback.extend_from_slice(output);
                    if delivered > 0 {
                        shared.last_delivery.clear();
                        shared.last_delivery.extend_from_slice(&output[..delivered]);
                        shared.consumed += delivered;
                    }
                },
                |error| eprintln!("audio output error: {error}"),
                None,
            )
            .map_err(|error| PlaybackError::AudioDevice(error.to_string()))?;
        stream
            .play()
            .map_err(|error| PlaybackError::AudioDevice(error.to_string()))?;

        Ok(Self {
            stream,
            shared,
            playing,
            gain,
            played_frames,
            callbacks,
            sample_rate,
            channels,
        })
    }

    pub fn set_volume(&self, volume: f32) {
        let clamped = if volume.is_finite() {
            volume.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.gain.store(clamped.to_bits(), Ordering::Relaxed);
    }

    pub fn volume(&self) -> f32 {
        f32::from_bits(self.gain.load(Ordering::Relaxed))
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn channels(&self) -> usize {
        self.channels
    }

    pub fn queue(&self, buffer: &AudioBuffer) {
        let mut shared = self.lock();
        shared.queue.extend(buffer.interleaved.iter().copied());
    }

    pub fn queue_samples(&self, samples: &[f32]) {
        let mut shared = self.lock();
        shared.queue.extend(samples.iter().copied());
    }

    pub fn queued_samples(&self) -> usize {
        self.lock().queue.len()
    }

    pub fn last_callback(&self) -> Vec<f32> {
        self.lock().last_callback.clone()
    }

    pub fn last_delivery(&self) -> (usize, Vec<f32>) {
        let shared = self.lock();
        (shared.consumed, shared.last_delivery.clone())
    }

    pub fn consumed_samples(&self) -> usize {
        self.lock().consumed
    }

    pub fn callback_count(&self) -> u64 {
        self.callbacks.load(Ordering::Relaxed)
    }

    pub fn start(&self) {
        self.playing.store(true, Ordering::Relaxed);
    }

    pub fn pause(&self) {
        self.playing.store(false, Ordering::Relaxed);
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::Relaxed)
    }

    pub fn seek(&self) {
        let mut shared = self.lock();
        shared.queue.clear();
        shared.last_delivery.clear();
        shared.consumed = 0;
        drop(shared);
        self.played_frames.store(0, Ordering::Relaxed);
    }

    pub fn played_frames(&self) -> u64 {
        self.played_frames.load(Ordering::Relaxed)
    }

    pub fn clock_seconds(&self) -> f64 {
        self.played_frames() as f64 / self.sample_rate.max(1) as f64
    }

    pub fn stop(&self) -> Result<()> {
        self.stream
            .pause()
            .map_err(|error| PlaybackError::AudioDevice(error.to_string()))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Shared> {
        match self.shared.lock() {
            Ok(shared) => shared,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}
