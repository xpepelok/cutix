//! Frame rates and the exact tick duration of a single frame.
//!
//! A [`FrameRate`] is a rational number of frames per second. Broadcast rates such as
//! 29.97 are `30000/1001`, not a rounded float, so the rational form is the only one
//! that survives a long timeline without drifting.
//!
//! [`FrameRate::ticks_per_frame`] only answers when one frame happens to be a whole
//! number of ticks. Most of the pipeline needs an answer for *every* valid rate, so it
//! should ask for a [`FrameDuration`] instead: that carries the frame length as an exact
//! fraction of ticks and converts between frame indices and timeline ticks without ever
//! accumulating error.

use serde::{Deserialize, Serialize};

use crate::media_time::TICKS_PER_SECOND;

/// A frame rate expressed as the exact fraction `numerator / denominator` frames per second.
///
/// The fraction is stored as given and is not normalised on construction, so `50/2` and
/// `25/1` are different values that describe the same rate. Compare rates through
/// [`FrameRate::reduced`] when the distinction does not matter.
#[cfg_attr(feature = "wasm", derive(tsify_next::Tsify))]
#[cfg_attr(feature = "wasm", tsify(from_wasm_abi, into_wasm_abi))]
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameRate {
    /// Frames counted over `denominator` seconds. Must be non-zero for the rate to be valid.
    pub numerator: u32,
    /// Seconds the `numerator` frames are spread over. Must be non-zero for the rate to be valid.
    pub denominator: u32,
}

impl FrameRate {
    /// 24000/1001 — film transferred to NTSC.
    pub const FPS_23_976: Self = Self {
        numerator: 24_000,
        denominator: 1_001,
    };
    /// 24/1 — film.
    pub const FPS_24: Self = Self {
        numerator: 24,
        denominator: 1,
    };
    /// 25/1 — PAL.
    pub const FPS_25: Self = Self {
        numerator: 25,
        denominator: 1,
    };
    /// 30000/1001 — NTSC.
    pub const FPS_29_97: Self = Self {
        numerator: 30_000,
        denominator: 1_001,
    };
    /// 30/1.
    pub const FPS_30: Self = Self {
        numerator: 30,
        denominator: 1,
    };
    /// 48/1 — high frame rate film.
    pub const FPS_48: Self = Self {
        numerator: 48,
        denominator: 1,
    };
    /// 50/1 — PAL double rate.
    pub const FPS_50: Self = Self {
        numerator: 50,
        denominator: 1,
    };
    /// 60000/1001 — NTSC double rate.
    pub const FPS_59_94: Self = Self {
        numerator: 60_000,
        denominator: 1_001,
    };
    /// 60/1.
    pub const FPS_60: Self = Self {
        numerator: 60,
        denominator: 1,
    };
    /// 120/1 — high frame rate capture.
    pub const FPS_120: Self = Self {
        numerator: 120,
        denominator: 1,
    };

    /// The rates a measured frame rate is allowed to snap onto in [`FrameRate::nearest`].
    pub const KNOWN: [Self; 10] = [
        Self::FPS_23_976,
        Self::FPS_24,
        Self::FPS_25,
        Self::FPS_29_97,
        Self::FPS_30,
        Self::FPS_48,
        Self::FPS_50,
        Self::FPS_59_94,
        Self::FPS_60,
        Self::FPS_120,
    ];

    /// Interprets a rate measured from a container as an exact rational rate.
    ///
    /// A measurement within 1% of a rate in [`FrameRate::KNOWN`] snaps onto it, so a
    /// container reporting `29.970029` becomes exactly `30000/1001`. Anything else is kept
    /// as measured, rounded to a thousandth of a frame per second and reduced.
    ///
    /// Returns `None` for a measurement that is not a rate at all: non-finite, zero,
    /// negative, or above 1000 fps.
    pub fn nearest(fps: f64) -> Option<Self> {
        if !fps.is_finite() || fps <= 0.0 || fps > 1_000.0 {
            return None;
        }
        let mut best: Option<(f64, Self)> = None;
        for candidate in Self::KNOWN {
            let Some(rate) = candidate.as_f64() else {
                continue;
            };
            let distance = (rate - fps).abs();
            if distance / fps > 0.01 {
                continue;
            }
            if best.is_none_or(|(closest, _)| distance < closest) {
                best = Some((distance, candidate));
            }
        }
        Some(match best {
            Some((_, rate)) => rate,
            // Reduce the measured fraction: `37.5` becomes `75/2` rather than `37500/1000`,
            // which is what lets a whole frame stay a whole number of ticks.
            None => Self::new((fps * 1_000.0).round() as u32, 1_000).reduced(),
        })
    }

    /// Builds a rate from an exact fraction. The fraction is stored as given.
    pub const fn new(numerator: u32, denominator: u32) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    /// True when both parts of the fraction are non-zero.
    ///
    /// Every other method on this type answers `None` for an invalid rate.
    pub const fn is_valid(self) -> bool {
        self.numerator > 0 && self.denominator > 0
    }

    /// The same rate with the fraction reduced to lowest terms.
    ///
    /// An invalid rate is returned unchanged.
    pub const fn reduced(self) -> Self {
        if !self.is_valid() {
            return self;
        }
        let divisor = gcd_u32(self.numerator, self.denominator);
        Self {
            numerator: self.numerator / divisor,
            denominator: self.denominator / divisor,
        }
    }

    /// The rate as frames per second. Lossy — prefer the rational form for any arithmetic.
    pub fn as_f64(self) -> Option<f64> {
        if !self.is_valid() {
            return None;
        }

        Some(f64::from(self.numerator) / f64::from(self.denominator))
    }

    /// One past the highest frame number that can appear within a single second.
    ///
    /// This is the modulus for the frame field of a `HH:MM:SS:FF` timecode.
    pub fn frame_number_upper_bound(self) -> Option<u32> {
        if !self.is_valid() {
            return None;
        }

        Some(self.numerator.div_ceil(self.denominator))
    }

    /// The exact duration of one frame, for any valid rate.
    ///
    /// This is the conversion the playback and export pipelines should use. It never
    /// rounds a frame length down to a usable-looking integer, so a rate whose frame is
    /// not a whole number of ticks still steps at the right speed.
    pub const fn frame_duration(self) -> Option<FrameDuration> {
        if !self.is_valid() {
            return None;
        }

        Some(FrameDuration {
            ticks_numerator: TICKS_PER_SECOND * self.denominator as i64,
            ticks_denominator: self.numerator as i64,
        })
    }

    /// The duration of one frame in whole ticks, when it happens to be whole.
    ///
    /// Returns `None` both for an invalid rate and for a valid rate whose frame is a
    /// fractional number of ticks — `1000/7` fps, for instance. A caller that needs an
    /// answer for every rate wants [`FrameRate::frame_duration`]; substituting a fallback
    /// integer here silently changes playback speed.
    pub fn ticks_per_frame(self) -> Option<i64> {
        self.frame_duration()?.whole_ticks()
    }
}

/// The exact duration of one frame, as a fraction of timeline ticks.
///
/// Frame index and tick conversions go through this type so that stepping over a long
/// timeline never accumulates rounding error: frame `n` is computed from `n` directly
/// rather than by adding an approximate frame length `n` times.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameDuration {
    /// Ticks covered by `ticks_denominator` frames. Always positive.
    ticks_numerator: i64,
    /// Frames the `ticks_numerator` ticks are spread over. Always positive.
    ticks_denominator: i64,
}

impl FrameDuration {
    /// The tick offset of frame `index`, measured from frame zero.
    ///
    /// Exact for every rate: the index is multiplied before it is divided, so the result
    /// is the nearest tick to the true frame boundary and successive frames do not drift.
    /// Ties round away from zero's side consistently with [`FrameDuration::frame_round`].
    ///
    /// Returns `None` only if the offset does not fit in an `i64`.
    pub fn ticks_at_frame(self, index: i64) -> Option<i64> {
        let scaled = i128::from(index).checked_mul(i128::from(self.ticks_numerator))?;
        i64::try_from(round_div_i128(scaled, i128::from(self.ticks_denominator))).ok()
    }

    /// The index of the frame that is on screen at `ticks`, rounding down.
    ///
    /// Negative tick positions floor towards negative infinity, so frame boundaries stay
    /// evenly spaced on both sides of zero.
    pub fn frame_floor(self, ticks: i64) -> Option<i64> {
        let scaled = i128::from(ticks).checked_mul(i128::from(self.ticks_denominator))?;
        i64::try_from(scaled.div_euclid(i128::from(self.ticks_numerator))).ok()
    }

    /// The index of the frame boundary nearest `ticks`, with ties going to the later frame.
    pub fn frame_round(self, ticks: i64) -> Option<i64> {
        let scaled = i128::from(ticks).checked_mul(i128::from(self.ticks_denominator))?;
        i64::try_from(round_div_i128(scaled, i128::from(self.ticks_numerator))).ok()
    }

    /// True when `ticks` sits exactly on a frame boundary.
    pub fn is_aligned(self, ticks: i64) -> bool {
        let Some(scaled) = i128::from(ticks).checked_mul(i128::from(self.ticks_denominator)) else {
            return false;
        };
        scaled.rem_euclid(i128::from(self.ticks_numerator)) == 0
    }

    /// The frame duration in whole ticks, when the frame is a whole number of ticks.
    ///
    /// Returns `None` for a rate whose frame length is fractional.
    pub const fn whole_ticks(self) -> Option<i64> {
        if self.ticks_numerator % self.ticks_denominator != 0 {
            return None;
        }
        Some(self.ticks_numerator / self.ticks_denominator)
    }

    /// The frame duration rounded to the nearest whole tick, never below one tick.
    ///
    /// Only for sizing hints such as cache keys and buffer capacities. Timeline positions
    /// must come from [`FrameDuration::ticks_at_frame`], which does not round per frame.
    pub fn approximate_ticks(self) -> i64 {
        round_div_i128(
            i128::from(self.ticks_numerator),
            i128::from(self.ticks_denominator),
        )
        .try_into()
        .unwrap_or(i64::MAX)
        .max(1)
    }

    /// The frame duration in seconds. Lossy — for display and for float-based APIs only.
    pub fn as_seconds_f64(self) -> f64 {
        self.ticks_numerator as f64 / (self.ticks_denominator as f64 * TICKS_PER_SECOND as f64)
    }
}

/// Divides and rounds to the nearest integer, ties going towards positive infinity.
///
/// `divisor` must be positive; every caller here builds it from a validated rate.
const fn round_div_i128(dividend: i128, divisor: i128) -> i128 {
    let doubled = dividend * 2 + divisor;
    doubled.div_euclid(divisor * 2)
}

const fn gcd_u32(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let next = a % b;
        a = b;
        b = next;
    }
    if a == 0 { 1 } else { a }
}

#[cfg(test)]
mod tests {
    use super::FrameRate;
    use crate::media_time::TICKS_PER_SECOND;

    #[test]
    fn a_measured_rate_snaps_onto_the_broadcast_rate_it_is_trying_to_be() {
        assert_eq!(FrameRate::nearest(29.97), Some(FrameRate::FPS_29_97));
        assert_eq!(FrameRate::nearest(30.0), Some(FrameRate::FPS_30));
        assert_eq!(FrameRate::nearest(59.94006), Some(FrameRate::FPS_59_94));
        assert_eq!(FrameRate::nearest(60.0), Some(FrameRate::FPS_60));
        assert_eq!(FrameRate::nearest(119.88), Some(FrameRate::FPS_120));
    }

    #[test]
    fn a_rate_that_is_nothing_standard_is_kept_as_measured() {
        let odd = FrameRate::nearest(37.5).expect("a real rate");
        assert_eq!(odd.as_f64(), Some(37.5));
    }

    #[test]
    fn a_rate_that_is_not_a_rate_at_all_is_refused() {
        assert_eq!(FrameRate::nearest(0.0), None);
        assert_eq!(FrameRate::nearest(-30.0), None);
        assert_eq!(FrameRate::nearest(f64::NAN), None);
        assert_eq!(FrameRate::nearest(f64::INFINITY), None);
    }

    #[test]
    fn resolves_ticks_per_standard_frame_rate() {
        assert_eq!(FrameRate::FPS_23_976.ticks_per_frame(), Some(5_005));
        assert_eq!(FrameRate::FPS_24.ticks_per_frame(), Some(5_000));
        assert_eq!(FrameRate::FPS_25.ticks_per_frame(), Some(4_800));
        assert_eq!(FrameRate::FPS_29_97.ticks_per_frame(), Some(4_004));
        assert_eq!(FrameRate::FPS_30.ticks_per_frame(), Some(4_000));
        assert_eq!(FrameRate::FPS_48.ticks_per_frame(), Some(2_500));
        assert_eq!(FrameRate::FPS_50.ticks_per_frame(), Some(2_400));
        assert_eq!(FrameRate::FPS_59_94.ticks_per_frame(), Some(2_002));
        assert_eq!(FrameRate::FPS_60.ticks_per_frame(), Some(2_000));
        assert_eq!(FrameRate::FPS_120.ticks_per_frame(), Some(1_000));
    }

    #[test]
    fn rejects_invalid_or_unsupported_rates() {
        assert_eq!(FrameRate::new(0, 1).ticks_per_frame(), None);
        assert_eq!(FrameRate::new(1, 0).ticks_per_frame(), None);
        // 7/3 fps is a real rate, but one frame is not a whole number of ticks.
        assert_eq!(FrameRate::new(7, 3).ticks_per_frame(), None);
        assert!(FrameRate::new(7, 3).frame_duration().is_some());
        assert_eq!(FrameRate::new(0, 1).frame_duration(), None);
        assert_eq!(FrameRate::new(1, 0).frame_duration(), None);
    }

    #[test]
    fn a_measured_rate_is_reduced_so_its_frame_stays_whole() {
        let odd = FrameRate::nearest(37.5).expect("a real rate");
        assert_eq!(odd, FrameRate::new(75, 2));
        assert_eq!(odd.ticks_per_frame(), Some(3_200));
    }

    #[test]
    fn every_valid_rate_has_a_frame_duration_even_when_it_is_not_whole() {
        // 23 fps snaps onto nothing and 120000/23 is not an integer. Before frame
        // durations existed this fell back to one tick per frame and played back
        // roughly five thousand times too fast.
        let rate = FrameRate::nearest(23.0).expect("a real rate");
        assert_eq!(rate.ticks_per_frame(), None);
        let frame = rate.frame_duration().expect("a valid rate has a duration");
        assert_eq!(frame.ticks_at_frame(0), Some(0));
        assert_eq!(frame.ticks_at_frame(23), Some(TICKS_PER_SECOND));
    }

    #[test]
    fn stepping_frames_does_not_drift_over_a_long_timeline() {
        for rate in [
            FrameRate::FPS_23_976,
            FrameRate::FPS_24,
            FrameRate::FPS_29_97,
            FrameRate::FPS_59_94,
            FrameRate::nearest(37.5).expect("a real rate"),
            FrameRate::nearest(23.0).expect("a real rate"),
            FrameRate::new(1_000, 7),
        ] {
            let frame = rate.frame_duration().expect("a valid rate has a duration");
            let fps = rate.as_f64().expect("a valid rate has a float form");
            // One hour of frames. The error against the true boundary must stay within
            // half a tick no matter how far along the timeline we are.
            let last = (fps * 3_600.0) as i64;
            for index in [1, 2, last / 2, last - 1, last] {
                let ticks = frame.ticks_at_frame(index).expect("an in-range offset");
                let exact = index as f64 * TICKS_PER_SECOND as f64 / fps;
                assert!(
                    (ticks as f64 - exact).abs() <= 0.5 + 1e-6,
                    "{rate:?} frame {index}: {ticks} ticks vs {exact} exact"
                );
            }
        }
    }

    #[test]
    fn a_frame_index_survives_a_round_trip_through_ticks() {
        for rate in [
            FrameRate::FPS_23_976,
            FrameRate::FPS_29_97,
            FrameRate::nearest(23.0).expect("a real rate"),
            FrameRate::new(1_000, 7),
        ] {
            let frame = rate.frame_duration().expect("a valid rate has a duration");
            for index in [0, 1, 7, 100, 100_000] {
                let ticks = frame.ticks_at_frame(index).expect("an in-range offset");
                assert_eq!(frame.frame_round(ticks), Some(index), "{rate:?} {index}");
            }
        }
    }

    #[test]
    fn an_approximate_frame_is_never_shorter_than_a_tick() {
        let frame = FrameRate::new(1_000, 1)
            .frame_duration()
            .expect("a valid rate has a duration");
        assert_eq!(frame.approximate_ticks(), 120);
        let dense = FrameRate::new(1_000_000, 1)
            .frame_duration()
            .expect("a valid rate has a duration");
        assert_eq!(dense.approximate_ticks(), 1);
    }

    #[test]
    fn a_reduced_rate_describes_the_same_speed() {
        assert_eq!(FrameRate::new(50, 2).reduced(), FrameRate::new(25, 1));
        assert_eq!(
            FrameRate::new(24_000, 1_001).reduced(),
            FrameRate::FPS_23_976
        );
        assert_eq!(FrameRate::new(0, 1).reduced(), FrameRate::new(0, 1));
    }
}
