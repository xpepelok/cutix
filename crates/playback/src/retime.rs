use cutix_project::model::RetimeConfig;
use time::{MediaTime, SpeedCurve, SpeedPoint};

pub const MIN_RATE: f64 = 0.01;
pub const MAX_RATE: f64 = 5.0;

pub fn clamp_rate(rate: f64) -> f64 {
    if !rate.is_finite() || rate <= 0.0 {
        return 1.0;
    }
    rate.clamp(MIN_RATE, MAX_RATE)
}

fn speed_curve(retime: &RetimeConfig) -> Option<SpeedCurve> {
    let points = retime.curve.as_ref()?;
    if points.is_empty() {
        return None;
    }
    let mut mapped: Vec<SpeedPoint> = points
        .iter()
        .filter(|point| point.time.is_finite() && point.speed.is_finite())
        .map(|point| SpeedPoint {
            time: point.time.max(0.0),
            speed: point.speed.clamp(0.0, MAX_RATE),
        })
        .collect();
    if mapped.is_empty() {
        return None;
    }
    mapped.sort_by(|a, b| a.time.total_cmp(&b.time));
    if mapped[0].time > 0.0 {
        let speed = mapped[0].speed;
        mapped.insert(0, SpeedPoint { time: 0.0, speed });
    }
    Some(SpeedCurve { points: mapped })
}

pub fn source_offset(retime: Option<&RetimeConfig>, clip_time: MediaTime) -> MediaTime {
    let Some(retime) = retime else {
        return clip_time;
    };
    if let Some(curve) = speed_curve(retime) {
        let seconds = curve.source_offset_at(clip_time.to_seconds_f64());
        return MediaTime::from_seconds_f64(seconds).unwrap_or(clip_time);
    }
    let rate = clamp_rate(retime.rate);
    MediaTime::from_ticks((clip_time.as_ticks() as f64 * rate).round() as i64)
}

pub fn source_offset_seconds(retime: Option<&RetimeConfig>, clip_seconds: f64) -> f64 {
    let Some(retime) = retime else {
        return clip_seconds;
    };
    if let Some(curve) = speed_curve(retime) {
        return curve.source_offset_at(clip_seconds);
    }
    clip_seconds * clamp_rate(retime.rate)
}

pub fn clip_time_at_source(retime: Option<&RetimeConfig>, source_time: f64) -> f64 {
    let Some(retime) = retime else {
        return source_time;
    };
    if let Some(curve) = speed_curve(retime) {
        return curve
            .output_time_for_source(source_time)
            .unwrap_or(source_time);
    }
    source_time / clamp_rate(retime.rate)
}

pub fn effective_rate_at(retime: Option<&RetimeConfig>, clip_time: f64) -> f64 {
    let Some(retime) = retime else {
        return 1.0;
    };
    if let Some(curve) = speed_curve(retime) {
        return curve.speed_at(clip_time);
    }
    clamp_rate(retime.rate)
}

pub fn timeline_duration_for_source_span(retime: Option<&RetimeConfig>, source_span: f64) -> f64 {
    if source_span <= 0.0 {
        return 0.0;
    }
    let Some(retime) = retime else {
        return source_span;
    };
    if let Some(curve) = speed_curve(retime) {
        return curve.output_time_for_source(source_span).unwrap_or(0.0);
    }
    source_span / clamp_rate(retime.rate)
}

pub fn source_span_at_clip_time(retime: Option<&RetimeConfig>, clip_time: f64) -> f64 {
    source_offset_seconds(retime, clip_time).max(0.0)
}

pub fn source_span_ticks(retime: Option<&RetimeConfig>, clip_time: MediaTime) -> MediaTime {
    let offset = source_offset(retime, clip_time);
    if offset.as_ticks() < 0 {
        MediaTime::ZERO
    } else {
        offset
    }
}

pub fn split_retime(retime: Option<&RetimeConfig>) -> (Option<RetimeConfig>, Option<RetimeConfig>) {
    (retime.cloned(), retime.cloned())
}

pub fn is_element_reversed(reversed_from: Option<&serde_json::Value>) -> bool {
    reversed_from.is_some()
}

pub fn mirror_trim(
    trim_start: f64,
    trim_end: f64,
    source_duration: f64,
    next_source_duration: f64,
) -> (f64, f64) {
    let total = source_duration.max(0.0);
    let next_total = next_source_duration.max(0.0);
    let start = trim_start.max(0.0).min(total);
    let end = (total - trim_end.max(0.0)).max(start).min(total);
    let span = (end - start).min(next_total);
    let next_start = (next_total - end).max(0.0).min(next_total - span);
    let next_end = (next_total - next_start - span).max(0.0);
    (next_start, next_end)
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReverseLink {
    pub media_id: String,
    pub source_duration: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReversedFromPatch {
    Set(ReverseLink),
    Clear,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReverseSwap {
    pub media_id: String,
    pub source_duration: f64,
    pub trim_start: f64,
    pub trim_end: f64,
    pub duration: f64,
    pub reversed_from: ReversedFromPatch,
}

#[allow(clippy::too_many_arguments)]
pub fn build_reverse_swap(
    media_id: &str,
    trim_start: f64,
    trim_end: f64,
    source_duration: Option<f64>,
    duration: f64,
    retime: Option<&RetimeConfig>,
    was_reversed: bool,
    next_media_id: &str,
    next_source_duration: f64,
) -> ReverseSwap {
    let source_duration = source_duration.unwrap_or(trim_start + trim_end + duration);
    let (next_trim_start, next_trim_end) =
        mirror_trim(trim_start, trim_end, source_duration, next_source_duration);
    let span = (next_source_duration - next_trim_start - next_trim_end).max(0.0);
    let duration = timeline_duration_for_source_span(retime, span);
    let reversed_from = if was_reversed {
        ReversedFromPatch::Clear
    } else {
        ReversedFromPatch::Set(ReverseLink {
            media_id: media_id.to_string(),
            source_duration,
        })
    };
    ReverseSwap {
        media_id: next_media_id.to_string(),
        source_duration: next_source_duration,
        trim_start: next_trim_start,
        trim_end: next_trim_end,
        duration,
        reversed_from,
    }
}

pub fn build_reversed_asset_name(name: &str, suffix: &str) -> String {
    match name.rfind('.') {
        Some(dot) if dot > 0 => format!("{} {}{}", &name[..dot], suffix, &name[dot..]),
        _ => format!("{name} {suffix}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cutix_project::model::{RetimeConfig, RetimeSpeedPoint};

    fn rate(rate: f64) -> RetimeConfig {
        RetimeConfig {
            rate,
            maintain_pitch: None,
            curve: None,
            blend_frames: None,
        }
    }

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "{a} vs {b}");
    }

    #[test]
    fn maps_clip_time_to_source_time_at_2x() {
        approx(source_offset_seconds(Some(&rate(2.0)), 5.0), 10.0);
    }

    #[test]
    fn maps_clip_time_to_source_time_at_half_x() {
        approx(source_offset_seconds(Some(&rate(0.5)), 4.0), 2.0);
    }

    #[test]
    fn returns_clip_time_unchanged_when_no_retime() {
        approx(source_offset_seconds(None, 7.0), 7.0);
    }

    #[test]
    fn inverts_source_time_back_to_clip_time_at_2x() {
        approx(clip_time_at_source(Some(&rate(2.0)), 10.0), 5.0);
    }

    #[test]
    fn returns_effective_rate() {
        approx(effective_rate_at(Some(&rate(2.0)), 0.0), 2.0);
        approx(effective_rate_at(None, 0.0), 1.0);
    }

    #[test]
    fn derives_timeline_duration_for_a_visible_source_span() {
        approx(
            timeline_duration_for_source_span(Some(&rate(2.0)), 10.0),
            5.0,
        );
        approx(
            timeline_duration_for_source_span(Some(&rate(0.5)), 10.0),
            20.0,
        );
    }

    #[test]
    fn clamps_invalid_rates_to_1() {
        approx(source_offset_seconds(Some(&rate(0.0)), 5.0), 5.0);
        approx(source_offset_seconds(Some(&rate(-1.0)), 5.0), 5.0);
    }

    #[test]
    fn caps_retime_rates_above_5x() {
        approx(source_offset_seconds(Some(&rate(100.0)), 5.0), 25.0);
        approx(
            timeline_duration_for_source_span(Some(&rate(100.0)), 10.0),
            2.0,
        );
    }

    #[test]
    fn measures_source_span_at_a_clip_time() {
        approx(source_span_at_clip_time(Some(&rate(2.0)), 5.0), 10.0);
    }

    #[test]
    fn returns_zero_for_non_positive_clip_time() {
        approx(source_span_at_clip_time(None, 0.0), 0.0);
        approx(source_span_at_clip_time(None, -1.0), 0.0);
    }

    #[test]
    fn passes_the_same_retime_to_both_halves_when_splitting() {
        let retime = RetimeConfig {
            rate: 1.5,
            maintain_pitch: None,
            curve: None,
            blend_frames: None,
        };
        let (left, right) = split_retime(Some(&retime));
        assert_eq!(left.as_ref(), Some(&retime));
        assert_eq!(right.as_ref(), Some(&retime));
    }

    #[test]
    fn returns_none_on_both_sides_when_no_retime() {
        let (left, right) = split_retime(None);
        assert!(left.is_none());
        assert!(right.is_none());
    }

    #[test]
    fn split_source_spans_partition_the_curve() {
        let retime = RetimeConfig {
            rate: 1.0,
            maintain_pitch: None,
            curve: Some(vec![
                RetimeSpeedPoint {
                    time: 0.0,
                    speed: 0.5,
                },
                RetimeSpeedPoint {
                    time: 4.0,
                    speed: 2.0,
                },
            ]),
            blend_frames: None,
        };
        let total = source_span_at_clip_time(Some(&retime), 4.0);
        let left = source_span_at_clip_time(Some(&retime), 1.5);
        let right = total - left;

        approx(source_span_at_clip_time(Some(&retime), 1.5) + right, total);
        assert!(left > 0.0 && right > 0.0);
    }

    const SOURCE: f64 = 1000.0;

    #[test]
    fn mirror_swaps_head_and_tail_trims_for_equal_length_source() {
        assert_eq!(mirror_trim(100.0, 300.0, SOURCE, SOURCE), (300.0, 100.0));
    }

    #[test]
    fn mirror_preserves_the_visible_span_when_derived_source_is_shorter() {
        let (start, end) = mirror_trim(100.0, 300.0, SOURCE, 900.0);
        approx(900.0 - start - end, 600.0);
        assert!(start >= 0.0 && end >= 0.0);
    }

    #[test]
    fn mirror_is_an_involution() {
        let (start, end) = mirror_trim(100.0, 300.0, SOURCE, SOURCE);
        assert_eq!(mirror_trim(start, end, SOURCE, SOURCE), (100.0, 300.0));
    }

    #[test]
    fn mirror_clamps_when_derived_source_cannot_hold_the_span() {
        assert_eq!(mirror_trim(100.0, 300.0, SOURCE, 400.0), (0.0, 0.0));
    }

    fn swap(retime: Option<&RetimeConfig>, duration: f64, was_reversed: bool) -> ReverseSwap {
        build_reverse_swap(
            "original",
            100.0,
            300.0,
            Some(SOURCE),
            duration,
            retime,
            was_reversed,
            "reversed",
            SOURCE,
        )
    }

    #[test]
    fn swap_swaps_media_mirrors_trims_and_records_provenance() {
        let patch = swap(None, 600.0, false);
        assert_eq!(patch.media_id, "reversed");
        approx(patch.trim_start, 300.0);
        approx(patch.trim_end, 100.0);
        approx(patch.duration, 600.0);
        assert_eq!(
            patch.reversed_from,
            ReversedFromPatch::Set(ReverseLink {
                media_id: "original".into(),
                source_duration: SOURCE,
            })
        );
    }

    #[test]
    fn swap_keeps_the_timeline_duration_under_a_retime_rate() {
        let patch = swap(Some(&rate(2.0)), 300.0, false);
        approx(patch.duration, 300.0);
    }

    #[test]
    fn swap_round_trips_back_to_original_media_in_one_step() {
        let forward = swap(None, 600.0, false);

        let restore = build_reverse_swap(
            &forward.media_id,
            forward.trim_start,
            forward.trim_end,
            Some(forward.source_duration),
            forward.duration,
            None,
            true,
            "original",
            SOURCE,
        );
        approx(restore.trim_start, 100.0);
        approx(restore.trim_end, 300.0);
        approx(restore.duration, 600.0);
        assert_eq!(restore.media_id, "original");
        assert_eq!(restore.reversed_from, ReversedFromPatch::Clear);
    }

    #[test]
    fn reversed_asset_name_inserts_suffix_before_extension() {
        assert_eq!(
            build_reversed_asset_name("clip.mp4", "(reversed)"),
            "clip (reversed).mp4"
        );
    }

    #[test]
    fn reversed_asset_name_appends_when_no_extension() {
        assert_eq!(build_reversed_asset_name("clip", "(rev)"), "clip (rev)");
    }

    #[test]
    fn is_reversed_reflects_presence_of_link() {
        assert!(!is_element_reversed(None));
        assert!(is_element_reversed(Some(
            &serde_json::json!({ "mediaId": "x" })
        )));
    }
}
