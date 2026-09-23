use std::time::Instant;

use time::MediaTime;

pub(crate) struct Clock {
    anchor_time: MediaTime,
    anchor_instant: Instant,
    rate: f64,
    playing: bool,
}

impl Clock {
    pub(crate) fn stopped_at_start() -> Self {
        Self {
            anchor_time: MediaTime::ZERO,
            anchor_instant: Instant::now(),
            rate: 1.0,
            playing: false,
        }
    }

    pub(crate) fn now(&self) -> MediaTime {
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

    pub(crate) fn play(&mut self) {
        self.reanchor();
        self.playing = true;
    }

    pub(crate) fn pause(&mut self) {
        self.reanchor();
        self.playing = false;
    }

    pub(crate) fn is_playing(&self) -> bool {
        self.playing
    }

    pub(crate) fn seek(&mut self, time: MediaTime) {
        self.anchor_time = time.max(MediaTime::ZERO);
        self.anchor_instant = Instant::now();
    }

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
