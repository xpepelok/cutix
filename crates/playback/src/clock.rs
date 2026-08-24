//! The playback clock: where the playhead is, and how fast it is moving.
//!
//! The clock is the only thing that knows the current timeline position. It holds an
//! anchor — a timeline time paired with the wall-clock instant it was taken — and derives
//! the present position from how long ago that anchor was set. Nothing here composes,
//! decodes or queues; the controller owns the clock and asks it for the time.

use std::time::Instant;

use time::MediaTime;

/// A playhead that advances with wall-clock time while playing and holds still while paused.
pub(crate) struct Clock {
    /// Timeline position at the moment `anchor_instant` was taken.
    anchor_time: MediaTime,
    /// Wall-clock instant `anchor_time` was captured.
    anchor_instant: Instant,
    /// Timeline seconds advanced per wall-clock second. Always finite and positive.
    rate: f64,
    playing: bool,
}

impl Clock {
    /// A paused clock parked at the start of the timeline, running at normal speed.
    pub(crate) fn stopped_at_start() -> Self {
        Self {
            anchor_time: MediaTime::ZERO,
            anchor_instant: Instant::now(),
            rate: 1.0,
            playing: false,
        }
    }

    /// The timeline position right now.
    pub(crate) fn now(&self) -> MediaTime {
        if !self.playing {
            return self.anchor_time;
        }
        let elapsed = self.anchor_instant.elapsed().as_secs_f64() * self.rate;
        self.anchor_time + MediaTime::from_seconds_f64(elapsed).unwrap_or(MediaTime::ZERO)
    }

    /// Moves the anchor to the present, so a change in rate or play state does not make the
    /// playhead jump.
    fn reanchor(&mut self) {
        self.anchor_time = self.now();
        self.anchor_instant = Instant::now();
    }

    /// Starts advancing from wherever the playhead is now.
    pub(crate) fn play(&mut self) {
        self.reanchor();
        self.playing = true;
    }

    /// Holds the playhead at its present position.
    pub(crate) fn pause(&mut self) {
        self.reanchor();
        self.playing = false;
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.playing
    }

    /// Jumps the playhead. Times before the start of the timeline clamp to zero.
    pub(crate) fn seek(&mut self, time: MediaTime) {
        self.anchor_time = time.max(MediaTime::ZERO);
        self.anchor_instant = Instant::now();
    }

    /// Sets the playback rate. A rate that is not finite and positive resets to normal speed.
    pub(crate) fn set_rate(&mut self, rate: f64) {
        self.reanchor();
        self.rate = if rate.is_finite() && rate > 0.0 {
            rate
        } else {
            1.0
        };
    }

    pub(crate) fn rate(&self) -> f64 {
        self.rate
    }
}

#[cfg(test)]
mod tests {
    use super::Clock;
    use time::MediaTime;

    #[test]
    fn a_paused_clock_stays_where_it_was_put() {
        let mut clock = Clock::stopped_at_start();
        clock.seek(MediaTime::from_ticks(5_000));
        assert_eq!(clock.now(), MediaTime::from_ticks(5_000));
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert_eq!(clock.now(), MediaTime::from_ticks(5_000));
    }

    #[test]
    fn seeking_before_the_start_clamps_to_zero() {
        let mut clock = Clock::stopped_at_start();
        clock.seek(MediaTime::from_ticks(-1_000));
        assert_eq!(clock.now(), MediaTime::ZERO);
    }

    #[test]
    fn a_playing_clock_advances() {
        let mut clock = Clock::stopped_at_start();
        clock.seek(MediaTime::from_ticks(1_000));
        clock.play();
        std::thread::sleep(std::time::Duration::from_millis(20));
        assert!(clock.now() > MediaTime::from_ticks(1_000));
    }

    #[test]
    fn pausing_does_not_make_the_playhead_jump() {
        let mut clock = Clock::stopped_at_start();
        clock.play();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let before = clock.now();
        clock.pause();
        assert!(clock.now() >= before);
        assert_eq!(clock.now(), clock.now());
    }

    #[test]
    fn a_rate_that_is_not_a_rate_falls_back_to_normal_speed() {
        let mut clock = Clock::stopped_at_start();
        clock.set_rate(f64::NAN);
        assert_eq!(clock.rate(), 1.0);
        clock.set_rate(-2.0);
        assert_eq!(clock.rate(), 1.0);
        clock.set_rate(2.0);
        assert_eq!(clock.rate(), 2.0);
    }
}
