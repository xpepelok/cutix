use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::error::{PlaybackError, Result};
use crate::mix::AudioBuffer;

const RING_SAMPLES: usize = 1 << 20;

const DELIVERY_TAIL: usize = 4_096;

const NO_FLUSH: usize = usize::MAX;

struct Ring {
    slots: Vec<AtomicU32>,
    mask: usize,
    written: AtomicUsize,
    read: AtomicUsize,
    flush_to: AtomicUsize,
}

impl Ring {
    fn new(capacity: usize) -> Self {
        let capacity = capacity.next_power_of_two();
        Self {
            slots: (0..capacity).map(|_| AtomicU32::new(0)).collect(),
            mask: capacity - 1,
            written: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
            flush_to: AtomicUsize::new(NO_FLUSH),
        }
    }

    fn len(&self) -> usize {
        let written = self.written.load(Ordering::Acquire);
        let pending = self.flush_to.load(Ordering::Acquire);
        let from = if pending == NO_FLUSH {
            self.read.load(Ordering::Acquire)
        } else {
            pending
        };
        written.wrapping_sub(from)
    }

    fn occupancy(&self) -> usize {
        self.written
            .load(Ordering::Acquire)
            .wrapping_sub(self.read.load(Ordering::Acquire))
    }

    fn free(&self) -> usize {
        (self.mask + 1) - self.occupancy()
    }

    fn push(&self, samples: &[f32]) -> usize {
        let mut written = self.written.load(Ordering::Relaxed);
        let taken = samples.len().min(self.free());
        for sample in &samples[..taken] {
            self.slots[written & self.mask].store(sample.to_bits(), Ordering::Relaxed);
            written = written.wrapping_add(1);
        }
        self.written.store(written, Ordering::Release);
        taken
    }

    fn pop(&self) -> Option<f32> {
        let read = self.read.load(Ordering::Relaxed);
        if read == self.written.load(Ordering::Acquire) {
            return None;
        }
        let bits = self.slots[read & self.mask].load(Ordering::Relaxed);
        self.read.store(read.wrapping_add(1), Ordering::Release);
        Some(f32::from_bits(bits))
    }

    fn request_flush(&self) {
        self.flush_to
            .store(self.written.load(Ordering::Relaxed), Ordering::Release);
    }

    fn apply_flush(&self) -> bool {
        let target = self.flush_to.load(Ordering::Acquire);
        if target == NO_FLUSH {
            return false;
        }
        self.flush_to.store(NO_FLUSH, Ordering::Release);
        let read = self.read.load(Ordering::Relaxed);
        if target.wrapping_sub(read) <= self.mask + 1 {
            self.read.store(target, Ordering::Release);
        }
        true
    }
}

pub struct AudioOutput {
    stream: cpal::Stream,
    ring: Arc<Ring>,
    delivery: Arc<Mutex<Vec<f32>>>,
    consumed: Arc<AtomicUsize>,
    starved: Arc<AtomicU64>,
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

        let ring = Arc::new(Ring::new(RING_SAMPLES));
        let delivery = Arc::new(Mutex::new(Vec::new()));
        let consumed = Arc::new(AtomicUsize::new(0));
        let starved = Arc::new(AtomicU64::new(0));
        let playing = Arc::new(AtomicBool::new(false));
        let played_frames = Arc::new(AtomicU64::new(0));
        let callbacks = Arc::new(AtomicU64::new(0));
        let gain = Arc::new(AtomicU32::new(1.0f32.to_bits()));

        let callback_ring = Arc::clone(&ring);
        let callback_delivery = Arc::clone(&delivery);
        let callback_consumed = Arc::clone(&consumed);
        let callback_starved = Arc::clone(&starved);
        let callback_playing = Arc::clone(&playing);
        let callback_frames = Arc::clone(&played_frames);
        let callback_count = Arc::clone(&callbacks);
        let callback_gain = Arc::clone(&gain);

        let stream = device
            .build_output_stream(
                &config.config(),
                move |output: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    callback_count.fetch_add(1, Ordering::Relaxed);
                    if callback_ring.apply_flush() {
                        callback_consumed.store(0, Ordering::Relaxed);
                        callback_starved.store(0, Ordering::Relaxed);
                    }
                    if !callback_playing.load(Ordering::Relaxed) {
                        output.fill(0.0);
                        return;
                    }

                    let gain = f32::from_bits(callback_gain.load(Ordering::Relaxed));
                    let mut delivered = 0usize;
                    for slot in output.iter_mut() {
                        match callback_ring.pop() {
                            Some(sample) => {
                                *slot = sample * gain;
                                delivered += 1;
                            }
                            None => *slot = 0.0,
                        }
                    }

                    callback_frames
                        .fetch_add((output.len() / channels.max(1)) as u64, Ordering::Relaxed);
                    if delivered < output.len() {
                        callback_starved.fetch_add(1, Ordering::Relaxed);
                    }
                    if delivered > 0 {
                        callback_consumed.fetch_add(delivered, Ordering::Relaxed);
                        if let Ok(mut tail) = callback_delivery.try_lock() {
                            tail.clear();
                            let from = delivered.saturating_sub(DELIVERY_TAIL);
                            tail.extend_from_slice(&output[from..delivered]);
                        }
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
            ring,
            delivery,
            consumed,
            starved,
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
        self.ring.push(&buffer.interleaved);
    }

    pub fn queue_samples(&self, samples: &[f32]) {
        self.ring.push(samples);
    }

    pub fn queued_samples(&self) -> usize {
        self.ring.len()
    }

    pub fn free_samples(&self) -> usize {
        self.ring.free()
    }

    pub fn starved_callbacks(&self) -> u64 {
        self.starved.load(Ordering::Relaxed)
    }

    pub fn last_delivery(&self) -> (usize, Vec<f32>) {
        let tail = self
            .delivery
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        (self.consumed_samples(), tail)
    }

    pub fn consumed_samples(&self) -> usize {
        self.consumed.load(Ordering::Relaxed)
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
        self.ring.request_flush();
        self.delivery
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clear();
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ring_hands_back_what_was_put_in_it_in_order() {
        let ring = Ring::new(8);
        assert_eq!(ring.push(&[1.0, 2.0, 3.0]), 3);
        assert_eq!(ring.len(), 3);
        assert_eq!(ring.pop(), Some(1.0));
        assert_eq!(ring.pop(), Some(2.0));
        assert_eq!(ring.pop(), Some(3.0));
        assert_eq!(ring.pop(), None);
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn a_full_ring_takes_what_fits_and_refuses_the_rest() {
        let ring = Ring::new(4);
        let samples = [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        assert_eq!(ring.push(&samples), 4);
        assert_eq!(ring.free(), 0);
        assert_eq!(ring.push(&[9.0]), 0);
        assert_eq!(ring.pop(), Some(1.0));
        assert_eq!(ring.push(&[9.0]), 1);
    }

    #[test]
    fn a_flush_drops_what_was_queued_before_it_and_keeps_what_came_after() {
        let ring = Ring::new(8);
        ring.push(&[1.0, 2.0, 3.0]);
        ring.request_flush();
        ring.push(&[4.0, 5.0]);

        assert!(ring.apply_flush(), "the reader applies a pending flush");
        assert_eq!(ring.pop(), Some(4.0));
        assert_eq!(ring.pop(), Some(5.0));
        assert_eq!(ring.pop(), None);
        assert!(!ring.apply_flush(), "a flush is applied exactly once");
    }

    #[test]
    fn only_the_reader_ever_moves_the_read_cursor() {
        let ring = Ring::new(8);
        ring.push(&[1.0, 2.0]);
        let before = ring.read.load(Ordering::Relaxed);

        ring.request_flush();
        assert_eq!(
            ring.read.load(Ordering::Relaxed),
            before,
            "asking for a flush must not touch the cursor the audio callback owns"
        );
        assert_eq!(ring.occupancy(), 2, "the samples are still there");
        assert_eq!(ring.len(), 0, "but none of them will survive the flush");

        ring.apply_flush();
        assert_eq!(ring.occupancy(), 0);
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn the_capacity_is_rounded_up_to_a_power_of_two() {
        let ring = Ring::new(5);
        assert_eq!(ring.mask + 1, 8);
        assert_eq!(ring.free(), 8);
    }

    #[test]
    fn a_reader_and_a_writer_can_race_without_losing_a_sample() {
        let ring = Arc::new(Ring::new(64));
        let writer = Arc::clone(&ring);
        let total = 20_000usize;

        let producer = std::thread::spawn(move || {
            let mut sent = 0usize;
            while sent < total {
                let sample = [sent as f32];
                if writer.push(&sample) == 1 {
                    sent += 1;
                }
            }
        });

        let mut seen = 0usize;
        while seen < total {
            if let Some(sample) = ring.pop() {
                assert_eq!(sample, seen as f32, "samples arrived out of order");
                seen += 1;
            }
        }
        producer.join().expect("producer");
    }
}
