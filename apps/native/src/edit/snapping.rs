//! Snapping a dragged time to whatever is near it.

use super::*;

pub fn snap_time(
    tracks: &SceneTracks,
    candidate: MediaTime,
    playhead: MediaTime,
    exclude: Option<&str>,
    threshold: MediaTime,
) -> MediaTime {
    let mut best = candidate;
    let mut distance = ticks(threshold);
    let mut consider = |target: MediaTime| {
        let delta = (ticks(target) - ticks(candidate)).abs();
        if delta < distance {
            distance = delta;
            best = target;
        }
    };

    consider(playhead);
    for track in tracks.all() {
        for element in track.elements() {
            if exclude == Some(element.base().id.as_str()) {
                continue;
            }
            consider(element.base().start_time);
            consider(element.end_time());
        }
    }
    best
}
