//! The frame queue between the playback worker and the thread that draws.
//!
//! Composing a frame takes long enough that the worker is always working on a timeline
//! that the main thread may already have abandoned — the project can be replaced, or the
//! playhead moved, while frames are in flight. Every request therefore carries a
//! [`PlaybackGeneration`], the worker stamps its results with the generation it was
//! working under, and the main thread refuses anything stamped with a generation it has
//! moved on from. Without that stamp a frame from the previous project can be presented
//! over the new one, because clearing the queue cannot reach work already inside the
//! composer.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use time::MediaTime;

use crate::render::ComposedFrame;

/// How many composed frames may wait to be presented before the oldest is dropped.
pub const QUEUE_DEPTH: usize = 3;

/// Identifies one era of playback work.
///
/// The generation advances whenever previously requested frames stop being valid: the
/// project is replaced, the playhead is moved, or the queue is deliberately dropped.
/// Frames stamped with an earlier generation are discarded rather than presented.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct PlaybackGeneration(u64);

impl PlaybackGeneration {
    /// The generation a controller starts in.
    pub const FIRST: Self = Self(0);

    /// The raw counter, for logging and for tests that want to observe an advance.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A composed frame waiting to be shown, together with what it was composed for.
#[derive(Clone, Debug)]
pub struct FrameSlot {
    /// Timeline position this frame represents.
    pub time: MediaTime,
    /// The era this frame was composed for. Stale frames carry an older generation.
    pub generation: PlaybackGeneration,
    /// Monotonic counter over every frame this controller has published, used by the
    /// presenter to notice that the picture has actually changed.
    pub revision: u64,
    pub frame: Arc<ComposedFrame>,
}

/// The shared hand-off point for composed frames.
///
/// Both the worker and the presenting thread hold one of these. The worker only pushes;
/// the presenting thread pops, and is the only side allowed to advance the generation.
#[derive(Clone)]
pub(crate) struct FrameQueue {
    slots: Arc<Mutex<VecDeque<FrameSlot>>>,
    generation: Arc<AtomicU64>,
}

impl FrameQueue {
    pub(crate) fn new() -> Self {
        Self {
            slots: Arc::new(Mutex::new(VecDeque::new())),
            generation: Arc::new(AtomicU64::new(PlaybackGeneration::FIRST.get())),
        }
    }

    /// The era the presenting thread currently accepts frames for.
    pub(crate) fn generation(&self) -> PlaybackGeneration {
        PlaybackGeneration(self.generation.load(Ordering::Acquire))
    }

    /// Ends the current era and drops everything queued for it.
    ///
    /// Work already inside the composer cannot be recalled, so it is left to finish and
    /// is refused on arrival by [`FrameQueue::take_due`]. Returns the new generation,
    /// which the caller must attach to any request it sends next.
    pub(crate) fn invalidate(&self) -> PlaybackGeneration {
        // Bump before clearing: a frame the worker publishes in between carries the old
        // generation and is discarded, rather than surviving in a queue we already emptied.
        let next = PlaybackGeneration(self.generation.fetch_add(1, Ordering::AcqRel) + 1);
        self.lock().clear();
        next
    }

    /// Adds a composed frame, dropping the oldest if the queue is already full.
    pub(crate) fn publish(&self, slot: FrameSlot) {
        let mut slots = self.lock();
        slots.push_back(slot);
        while slots.len() > QUEUE_DEPTH {
            slots.pop_front();
        }
    }

    /// Replaces the whole queue with a single frame. Used for one-off compose requests,
    /// where the newly composed frame supersedes anything still waiting.
    pub(crate) fn replace_with(&self, slot: FrameSlot) {
        let mut slots = self.lock();
        slots.clear();
        slots.push_back(slot);
    }

    /// Drops everything queued without ending the era.
    pub(crate) fn clear(&self) {
        self.lock().clear();
    }

    pub(crate) fn len(&self) -> usize {
        self.lock().len()
    }

    /// Pops frames that are due, discarding any left over from an earlier generation.
    ///
    /// `is_due` decides whether the frame at the front should be presented yet. Stale
    /// frames are dropped regardless of whether they are due.
    pub(crate) fn take_due(
        &self,
        current: PlaybackGeneration,
        mut is_due: impl FnMut(&FrameSlot) -> bool,
    ) -> Option<FrameSlot> {
        let mut slots = self.lock();
        let mut taken = None;
        while let Some(front) = slots.front() {
            if front.generation != current {
                slots.pop_front();
                continue;
            }
            if taken.is_some() && !is_due(front) {
                break;
            }
            taken = slots.pop_front();
        }
        taken
    }

    fn lock(&self) -> MutexGuard<'_, VecDeque<FrameSlot>> {
        self.slots
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameQueue, FrameSlot, PlaybackGeneration, QUEUE_DEPTH};
    use crate::render::ComposedFrame;
    use std::sync::Arc;
    use time::MediaTime;

    fn slot(revision: u64, generation: PlaybackGeneration) -> FrameSlot {
        FrameSlot {
            time: MediaTime::from_ticks(revision as i64),
            generation,
            revision,
            frame: Arc::new(ComposedFrame {
                width: 1,
                height: 1,
                pixels: vec![0, 0, 0, 255],
                skipped: Vec::new(),
                rects: Vec::new(),
            }),
        }
    }

    #[test]
    fn the_queue_never_grows_past_its_depth() {
        let queue = FrameQueue::new();
        let generation = queue.generation();
        for revision in 0..(QUEUE_DEPTH as u64 + 4) {
            queue.publish(slot(revision, generation));
        }
        assert_eq!(queue.len(), QUEUE_DEPTH);
        let front = queue.take_due(generation, |_| false).expect("a due frame");
        assert_eq!(front.revision, 4, "the oldest frames are the ones dropped");
    }

    #[test]
    fn a_frame_composed_for_an_earlier_era_is_never_presented() {
        let queue = FrameQueue::new();
        let old = queue.generation();
        queue.publish(slot(1, old));
        let new = queue.invalidate();
        assert_ne!(new, old);

        // The worker was mid-compose when the era ended and publishes anyway.
        queue.publish(slot(2, old));
        assert!(queue.take_due(new, |_| true).is_none());

        queue.publish(slot(3, new));
        let taken = queue.take_due(new, |_| true).expect("the fresh frame");
        assert_eq!(taken.revision, 3);
    }

    #[test]
    fn invalidating_drops_everything_already_queued() {
        let queue = FrameQueue::new();
        let old = queue.generation();
        queue.publish(slot(1, old));
        queue.publish(slot(2, old));
        assert_eq!(queue.len(), 2);
        queue.invalidate();
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn frames_that_are_not_due_yet_stay_queued() {
        let queue = FrameQueue::new();
        let generation = queue.generation();
        queue.publish(slot(1, generation));
        queue.publish(slot(2, generation));
        queue.publish(slot(3, generation));
        let taken = queue
            .take_due(generation, |_| false)
            .expect("a first frame");
        assert_eq!(taken.revision, 1, "the first frame is always taken");
        assert_eq!(queue.len(), 2, "the rest wait until they are due");
    }

    #[test]
    fn every_due_frame_is_skipped_past_to_the_newest_one() {
        let queue = FrameQueue::new();
        let generation = queue.generation();
        for revision in 1..=3 {
            queue.publish(slot(revision, generation));
        }
        let taken = queue.take_due(generation, |_| true).expect("a due frame");
        assert_eq!(taken.revision, 3, "playback catches up rather than lagging");
        assert_eq!(queue.len(), 0);
    }
}
