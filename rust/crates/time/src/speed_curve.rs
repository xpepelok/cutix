use bridge::export;
use serde::{Deserialize, Serialize};

use crate::media_time::MediaTime;

const MIN_SPEED: f64 = 0.0;
const MAX_SPEED: f64 = 100.0;
const INVERSE_EPSILON: f64 = 1e-9;
const INVERSE_ITERATIONS: usize = 64;

#[cfg_attr(feature = "wasm", derive(tsify_next::Tsify))]
#[cfg_attr(feature = "wasm", tsify(from_wasm_abi, into_wasm_abi))]
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpeedPoint {
    pub time: f64,
    pub speed: f64,
}

#[cfg_attr(feature = "wasm", derive(tsify_next::Tsify))]
#[cfg_attr(feature = "wasm", tsify(from_wasm_abi, into_wasm_abi))]
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SpeedCurve {
    pub points: Vec<SpeedPoint>,
}

impl SpeedCurve {
    pub fn constant(speed: f64) -> Self {
        Self {
            points: vec![SpeedPoint {
                time: 0.0,
                speed: clamp_speed(speed),
            }],
        }
    }

    pub fn normalized(&self) -> Vec<SpeedPoint> {
        let mut points: Vec<SpeedPoint> = self
            .points
            .iter()
            .filter(|point| point.time.is_finite() && point.speed.is_finite())
            .map(|point| SpeedPoint {
                time: point.time.max(0.0),
                speed: clamp_speed(point.speed),
            })
            .collect();

        points.sort_by(|left, right| left.time.partial_cmp(&right.time).unwrap());
        points.dedup_by(|left, right| left.time == right.time && left.speed == right.speed);

        if points.is_empty() {
            return vec![SpeedPoint {
                time: 0.0,
                speed: 1.0,
            }];
        }
        if points[0].time > 0.0 {
            points.insert(
                0,
                SpeedPoint {
                    time: 0.0,
                    speed: points[0].speed,
                },
            );
        }
        points
    }

    pub fn speed_at(&self, output_time: f64) -> f64 {
        let points = self.normalized();
        if output_time <= points[0].time {
            return points[0].speed;
        }
        for pair in points.windows(2) {
            let (left, right) = (pair[0], pair[1]);
            if right.time <= output_time {
                continue;
            }
            if left.time > output_time {
                break;
            }
            let span = right.time - left.time;
            if span <= 0.0 {
                return right.speed;
            }
            let ratio = (output_time - left.time) / span;
            return left.speed + (right.speed - left.speed) * ratio;
        }
        points[points.len() - 1].speed
    }

    pub fn source_offset_at(&self, output_time: f64) -> f64 {
        if !output_time.is_finite() || output_time <= 0.0 {
            return 0.0;
        }

        let points = self.normalized();
        let mut consumed = 0.0;

        for pair in points.windows(2) {
            let (left, right) = (pair[0], pair[1]);
            if output_time <= left.time {
                break;
            }
            let end = output_time.min(right.time);
            let span = end - left.time;
            if span <= 0.0 {
                continue;
            }
            let speed_at_end = if right.time > left.time {
                let ratio = span / (right.time - left.time);
                left.speed + (right.speed - left.speed) * ratio
            } else {
                right.speed
            };
            consumed += (left.speed + speed_at_end) * 0.5 * span;
        }

        let last = points[points.len() - 1];
        if output_time > last.time {
            consumed += last.speed * (output_time - last.time);
        }

        consumed
    }

    pub fn output_time_for_source(&self, source_offset: f64) -> Option<f64> {
        if !source_offset.is_finite() || source_offset < 0.0 {
            return None;
        }
        if source_offset == 0.0 {
            return Some(0.0);
        }

        let mut low = 0.0;
        let mut high = 1.0;
        while self.source_offset_at(high) < source_offset {
            high *= 2.0;
            if high > 1e9 {
                return None;
            }
        }

        for _ in 0..INVERSE_ITERATIONS {
            let middle = (low + high) * 0.5;
            let value = self.source_offset_at(middle);
            if (value - source_offset).abs() <= INVERSE_EPSILON {
                return Some(middle);
            }
            if value < source_offset {
                low = middle;
            } else {
                high = middle;
            }
        }

        Some((low + high) * 0.5)
    }

    pub fn is_frozen_at(&self, output_time: f64) -> bool {
        self.speed_at(output_time) <= f64::EPSILON
    }
}

fn clamp_speed(speed: f64) -> f64 {
    if !speed.is_finite() {
        return 1.0;
    }
    speed.clamp(MIN_SPEED, MAX_SPEED)
}

#[cfg_attr(feature = "wasm", derive(tsify_next::Tsify))]
#[cfg_attr(feature = "wasm", tsify(from_wasm_abi))]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeedAtOptions {
    pub curve: SpeedCurve,
    pub output_time: f64,
}

#[export]
pub fn speed_at(SpeedAtOptions { curve, output_time }: SpeedAtOptions) -> f64 {
    curve.speed_at(output_time)
}

#[cfg_attr(feature = "wasm", derive(tsify_next::Tsify))]
#[cfg_attr(feature = "wasm", tsify(from_wasm_abi))]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceTimeAtOptions {
    pub curve: SpeedCurve,
    pub output_time: MediaTime,
}

#[export]
pub fn source_time_at(
    SourceTimeAtOptions { curve, output_time }: SourceTimeAtOptions,
) -> Option<MediaTime> {
    MediaTime::from_seconds_f64(curve.source_offset_at(output_time.to_seconds_f64()))
}

#[cfg_attr(feature = "wasm", derive(tsify_next::Tsify))]
#[cfg_attr(feature = "wasm", tsify(from_wasm_abi))]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputDurationOptions {
    pub curve: SpeedCurve,
    pub source_duration: MediaTime,
}

#[export]
pub fn output_duration_for_source(
    OutputDurationOptions {
        curve,
        source_duration,
    }: OutputDurationOptions,
) -> Option<MediaTime> {
    let seconds = curve.output_time_for_source(source_duration.to_seconds_f64())?;
    MediaTime::from_seconds_f64(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curve(points: &[(f64, f64)]) -> SpeedCurve {
        SpeedCurve {
            points: points
                .iter()
                .map(|(time, speed)| SpeedPoint {
                    time: *time,
                    speed: *speed,
                })
                .collect(),
        }
    }

    #[test]
    fn constant_speed_scales_linearly() {
        let curve = SpeedCurve::constant(2.0);
        assert!((curve.source_offset_at(3.0) - 6.0).abs() < 1e-9);
    }

    #[test]
    fn empty_curve_plays_at_normal_speed() {
        let curve = SpeedCurve::default();
        assert!((curve.source_offset_at(5.0) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn zero_speed_freezes_the_frame() {
        let curve = SpeedCurve::constant(0.0);
        assert_eq!(curve.source_offset_at(10.0), 0.0);
        assert!(curve.is_frozen_at(4.0));
    }

    #[test]
    fn freeze_segment_holds_source_position() {
        let curve = curve(&[(0.0, 1.0), (2.0, 1.0), (2.0, 0.0)]);
        let before = curve.source_offset_at(2.0);
        let during = curve.source_offset_at(5.0);
        assert!((before - during).abs() < 1e-9, "{before} vs {during}");
    }

    #[test]
    fn ramp_integrates_average_speed() {
        let curve = curve(&[(0.0, 1.0), (2.0, 3.0)]);
        assert!((curve.source_offset_at(2.0) - 4.0).abs() < 1e-9);
    }

    #[test]
    fn source_offset_is_monotonic() {
        let curve = curve(&[(0.0, 0.5), (1.0, 2.0), (3.0, 0.25)]);
        let mut previous = 0.0;
        for step in 0..=60 {
            let value = curve.source_offset_at(step as f64 * 0.1);
            assert!(value >= previous - 1e-12, "not monotonic at {step}");
            previous = value;
        }
    }

    #[test]
    fn inverse_round_trips() {
        let curve = curve(&[(0.0, 0.5), (2.0, 2.0), (4.0, 1.0)]);
        let output = 3.7;
        let source = curve.source_offset_at(output);
        let recovered = curve.output_time_for_source(source).expect("inverse");
        assert!((recovered - output).abs() < 1e-4, "{recovered} vs {output}");
    }

    #[test]
    fn speed_is_clamped_to_supported_range() {
        let curve = curve(&[(0.0, -5.0), (1.0, 1e9)]);
        assert_eq!(curve.speed_at(0.0), 0.0);
        assert_eq!(curve.speed_at(1.0), MAX_SPEED);
    }

    #[test]
    fn unsorted_points_are_normalized() {
        let curve = curve(&[(3.0, 2.0), (0.0, 1.0)]);
        let points = curve.normalized();
        assert_eq!(points[0].time, 0.0);
        assert_eq!(points[1].time, 3.0);
    }

    #[test]
    fn fully_frozen_curve_has_no_inverse() {
        let curve = SpeedCurve::constant(0.0);
        assert!(curve.output_time_for_source(1.0).is_none());
    }
}
