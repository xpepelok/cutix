use crate::types::{TWatermarkTiming, WatermarkTimingMode};
use time::TICKS_PER_SECOND;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatermarkWindow {
    pub time_offset: i64,
    pub duration: i64,
    pub fade_in: i64,
    pub fade_out: i64,
}

fn to_ticks(seconds: f64) -> i64 {
    if !seconds.is_finite() || seconds <= 0.0 {
        return 0;
    }
    (seconds * TICKS_PER_SECOND as f64).round() as i64
}

pub fn resolve_watermark_window(
    timing: &TWatermarkTiming,
    duration: i64,
) -> Option<WatermarkWindow> {
    if timing.mode != WatermarkTimingMode::Range {
        return Some(WatermarkWindow {
            time_offset: 0,
            duration,
            fade_in: 0,
            fade_out: 0,
        });
    }

    let start = to_ticks(timing.start).max(0).min(duration);
    let raw_end = to_ticks(timing.end);
    let end = if raw_end > 0 {
        raw_end.min(duration)
    } else {
        duration
    };
    let span = end - start;
    if span <= 0 {
        return None;
    }

    let fade_in = to_ticks(timing.fade_in).min(span);
    let fade_out = to_ticks(timing.fade_out).min(span - fade_in);

    Some(WatermarkWindow {
        time_offset: start,
        duration: span,
        fade_in,
        fade_out: fade_out.max(0),
    })
}

pub fn compute_fade_opacity(local_time: f64, duration: f64, fade_in: f64, fade_out: f64) -> f64 {
    let mut factor: f64 = 1.0;
    if fade_in > 0.0 && local_time < fade_in {
        factor = factor.min(local_time.max(0.0) / fade_in);
    }
    if fade_out > 0.0 {
        let remaining = duration - local_time;
        if remaining < fade_out {
            factor = factor.min(remaining.max(0.0) / fade_out);
        }
    }
    factor.max(0.0).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defaults::default_watermark_timing;

    fn duration() -> i64 {
        10 * TICKS_PER_SECOND
    }

    fn range(start: f64, end: f64, fade_in: f64, fade_out: f64) -> TWatermarkTiming {
        TWatermarkTiming {
            mode: WatermarkTimingMode::Range,
            start,
            end,
            fade_in,
            fade_out,
        }
    }

    #[test]
    fn spans_the_whole_timeline_in_always_mode() {
        let window = resolve_watermark_window(&default_watermark_timing(), duration()).unwrap();
        assert_eq!(
            window,
            WatermarkWindow {
                time_offset: 0,
                duration: duration(),
                fade_in: 0,
                fade_out: 0,
            }
        );
    }

    #[test]
    fn clips_a_range_to_the_timeline_and_converts_fades_to_ticks() {
        let window = resolve_watermark_window(&range(2.0, 6.0, 1.0, 1.0), duration()).unwrap();
        assert_eq!(
            window,
            WatermarkWindow {
                time_offset: 2 * TICKS_PER_SECOND,
                duration: 4 * TICKS_PER_SECOND,
                fade_in: TICKS_PER_SECOND,
                fade_out: TICKS_PER_SECOND,
            }
        );
    }

    #[test]
    fn runs_to_the_end_of_the_timeline_when_end_is_zero() {
        let window = resolve_watermark_window(&range(3.0, 0.0, 0.0, 0.0), duration()).unwrap();
        assert_eq!(window.duration, 7 * TICKS_PER_SECOND);
    }

    #[test]
    fn drops_an_empty_range() {
        assert!(resolve_watermark_window(&range(5.0, 5.0, 0.0, 0.0), duration()).is_none());
    }

    #[test]
    fn ramps_opacity_through_the_fades() {
        let d = 100.0;
        let (fi, fo) = (10.0, 20.0);
        assert_eq!(compute_fade_opacity(0.0, d, fi, fo), 0.0);
        assert!((compute_fade_opacity(5.0, d, fi, fo) - 0.5).abs() < 1e-10);
        assert_eq!(compute_fade_opacity(50.0, d, fi, fo), 1.0);
        assert_eq!(compute_fade_opacity(80.0, d, fi, fo), 1.0);
        assert!((compute_fade_opacity(90.0, d, fi, fo) - 0.5).abs() < 1e-10);
        assert!((compute_fade_opacity(95.0, d, fi, fo) - 0.25).abs() < 1e-10);
        assert_eq!(compute_fade_opacity(100.0, d, fi, fo), 0.0);
    }

    #[test]
    fn is_a_no_op_without_fades() {
        assert_eq!(compute_fade_opacity(3.0, 100.0, 0.0, 0.0), 1.0);
    }
}
