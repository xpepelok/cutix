use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use time::MediaTime;

use crate::render::ComposedFrame;

pub const QUEUE_DEPTH: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct PlaybackGeneration(u64);

impl PlaybackGeneration {
    pub const FIRST: Self = Self(0);

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug)]
pub struct FrameSlot {
    pub time: MediaTime,
    pub generation: PlaybackGeneration,
    pub revision: u64,
    pub frame: Arc<ComposedFrame>,
}

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

    pub(crate) fn generation(&self) -> PlaybackGeneration {
        PlaybackGeneration(self.generation.load(Ordering::Acquire))
    }

    pub(crate) fn invalidate(&self) -> PlaybackGeneration {
        let next = PlaybackGeneration(self.generation.fetch_add(1, Ordering::AcqRel) + 1);
        self.lock().clear();
        next
    }

    pub(crate) fn publish(&self, slot: FrameSlot) {
        let mut slots = self.lock();
        slots.push_back(slot);
        while slots.len() > QUEUE_DEPTH {
            slots.pop_front();
        }
    }

    pub(crate) fn replace_with(&self, slot: FrameSlot) {
        let mut slots = self.lock();
        slots.clear();
        slots.push_back(slot);
    }

    pub(crate) fn clear(&self) {
        self.lock().clear();
    }

    pub(crate) fn len(&self) -> usize {
        self.lock().len()
    }

    pub(crate) fn take_due(
        &self,
        current: PlaybackGeneration,
        force_first: bool,
        mut is_due: impl FnMut(&FrameSlot) -> bool,
    ) -> Option<FrameSlot> {
        let mut slots = self.lock();
        let mut taken = None;
        while let Some(front) = slots.front() {
            if front.generation != current {
                slots.pop_front();
                continue;
            }
            let forced = force_first && taken.is_none();
            if !forced && !is_due(front) {
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
        let front = queue
            .take_due(generation, true, |_| false)
            .expect("a due frame");
        assert_eq!(front.revision, 4, "the oldest frames are the ones dropped");
    }

    #[test]
    fn a_frame_composed_for_an_earlier_era_is_never_presented() {
        let queue = FrameQueue::new();
        let old = queue.generation();
        queue.publish(slot(1, old));
        let new = queue.invalidate();
        assert_ne!(new, old);

        queue.publish(slot(2, old));
        assert!(queue.take_due(new, false, |_| true).is_none());

        queue.publish(slot(3, new));
        let taken = queue
            .take_due(new, false, |_| true)
            .expect("the fresh frame");
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
            .take_due(generation, true, |_| false)
            .expect("a first frame");
        assert_eq!(
            taken.revision, 1,
            "the first frame is taken while nothing is on screen"
        );
        assert_eq!(queue.len(), 2, "the rest wait until they are due");
    }

    #[test]
    fn every_due_frame_is_skipped_past_to_the_newest_one() {
        let queue = FrameQueue::new();
        let generation = queue.generation();
        for revision in 1..=3 {
            queue.publish(slot(revision, generation));
        }
        let taken = queue
            .take_due(generation, false, |_| true)
            .expect("a due frame");
        assert_eq!(taken.revision, 3, "playback catches up rather than lagging");
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn an_early_frame_waits_once_something_is_already_presented() {
        let queue = FrameQueue::new();
        let generation = queue.generation();
        queue.publish(slot(1, generation));
        queue.publish(slot(2, generation));

        assert!(
            queue.take_due(generation, false, |_| false).is_none(),
            "a frame ahead of the clock must not replace the one on screen"
        );
        assert_eq!(queue.len(), 2);

        let taken = queue
            .take_due(generation, false, |slot| slot.revision <= 1)
            .expect("the frame that has become due");
        assert_eq!(taken.revision, 1);
        assert_eq!(queue.len(), 1);
    }
}
