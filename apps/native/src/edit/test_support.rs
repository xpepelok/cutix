//! Helpers the tests in this crate use and the shipping binary does not.

use super::*;

/// Only the tests in this file ask for this; compiled for them alone so the shipping
/// binary does not carry something nothing calls.
#[cfg(test)]
pub fn text_animation_duration(element: &TimelineElement, key: &str) -> Option<MediaTime> {
    text_animation_settings(element)
        .get(key)
        .and_then(|entry| entry.get("duration"))
        .and_then(Value::as_i64)
        .map(MediaTime::from_ticks)
}

/// Only the tests in this file ask for this; compiled for them alone so the shipping
/// binary does not carry something nothing calls.
#[cfg(test)]
pub(crate) fn apply_reverse_swap(
    video: &mut VideoElement,
    next_media_id: &str,
    next_source_ticks: i64,
) {
    use cutix_playback::retime::{build_reverse_swap, ReversedFromPatch};
    let was_reversed = video.reversed_from.is_some();
    let source_duration = video
        .base
        .source_duration
        .map(|value| value.as_ticks() as f64);
    let swap = build_reverse_swap(
        &video.media_id,
        video.base.trim_start.as_ticks() as f64,
        video.base.trim_end.as_ticks() as f64,
        source_duration,
        video.base.duration.as_ticks() as f64,
        video.retime.as_ref(),
        was_reversed,
        next_media_id,
        next_source_ticks as f64,
    );
    video.media_id = swap.media_id;
    video.base.source_duration = Some(MediaTime::from_ticks(swap.source_duration.round() as i64));
    video.base.trim_start = MediaTime::from_ticks(swap.trim_start.round() as i64);
    video.base.trim_end = MediaTime::from_ticks(swap.trim_end.round() as i64);
    video.base.duration = MediaTime::from_ticks(swap.duration.round() as i64);
    video.reversed_from = match swap.reversed_from {
        ReversedFromPatch::Clear => None,
        ReversedFromPatch::Set(link) => Some(json!({
            "mediaId": link.media_id,
            "sourceDuration": link.source_duration.round() as i64,
        })),
    };
}
