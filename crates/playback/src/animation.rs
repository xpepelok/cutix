use cutix_project::model::{AnimationChannel, CurveHandle, ElementAnimations, ScalarAnimationKey};
use time::MediaTime;

fn channel_id_for(animations: &ElementAnimations, path: &str, component: &str) -> Option<String> {
    let binding = animations.bindings.get(path)?;
    let components = binding.get("components")?.as_array()?;
    for entry in components {
        let key = entry.get("key").and_then(|value| value.as_str())?;
        if key == component {
            return entry
                .get("channelId")
                .and_then(|value| value.as_str())
                .map(str::to_owned);
        }
    }
    None
}

fn lookup<'a>(
    animations: &'a ElementAnimations,
    path: &str,
    component: &str,
) -> Option<&'a AnimationChannel> {
    if let Some(id) = channel_id_for(animations, path, component)
        && let Some(channel) = animations.channels.get(&id)
    {
        return Some(channel);
    }
    animations
        .channels
        .get(&format!("{path}:{component}"))
        .or_else(|| animations.channels.get(path))
}

fn sorted_keys(keys: &[ScalarAnimationKey]) -> Vec<&ScalarAnimationKey> {
    let mut sorted: Vec<&ScalarAnimationKey> = keys.iter().collect();
    sorted.sort_by_key(|key| key.time.as_ticks());
    sorted
}

fn default_right_handle(left: &ScalarAnimationKey, right: &ScalarAnimationKey) -> CurveHandle {
    let span = right.time.as_ticks() - left.time.as_ticks();
    CurveHandle {
        dt: MediaTime::from_ticks(span / 3),
        dv: (right.value - left.value) / 3.0,
    }
}

fn default_left_handle(left: &ScalarAnimationKey, right: &ScalarAnimationKey) -> CurveHandle {
    let span = right.time.as_ticks() - left.time.as_ticks();
    CurveHandle {
        dt: MediaTime::from_ticks(-(span / 3)),
        dv: -(right.value - left.value) / 3.0,
    }
}

fn cubic(progress: f64, p0: f64, p1: f64, p2: f64, p3: f64) -> f64 {
    let inverse = 1.0 - progress;
    inverse * inverse * inverse * p0
        + 3.0 * inverse * inverse * progress * p1
        + 3.0 * inverse * progress * progress * p2
        + progress * progress * progress * p3
}

fn solve_progress_for_time(
    time: f64,
    left: &ScalarAnimationKey,
    right: &ScalarAnimationKey,
    right_handle: &CurveHandle,
    left_handle: &CurveHandle,
) -> f64 {
    let t0 = left.time.as_ticks() as f64;
    let t3 = right.time.as_ticks() as f64;
    let t1 = t0 + right_handle.dt.as_ticks() as f64;
    let t2 = t3 + left_handle.dt.as_ticks() as f64;
    let mut lower = 0.0;
    let mut upper = 1.0;
    for _ in 0..20 {
        let middle = (lower + upper) / 2.0;
        if cubic(middle, t0, t1, t2, t3) < time {
            lower = middle;
        } else {
            upper = middle;
        }
    }
    (lower + upper) / 2.0
}

fn extrapolate(
    edge: &ScalarAnimationKey,
    neighbor: Option<&ScalarAnimationKey>,
    mode: &str,
    time: f64,
) -> f64 {
    if mode != "linear" {
        return edge.value;
    }
    let Some(neighbor) = neighbor else {
        return edge.value;
    };
    let span = neighbor.time.as_ticks() as f64 - edge.time.as_ticks() as f64;
    if span == 0.0 {
        return edge.value;
    }
    edge.value + ((time - edge.time.as_ticks() as f64) / span) * (neighbor.value - edge.value)
}

fn extrapolation_mode(extrapolation: Option<&serde_json::Value>, side: &str) -> String {
    extrapolation
        .and_then(|value| value.get(side))
        .and_then(|value| value.as_str())
        .unwrap_or("hold")
        .to_owned()
}

fn scalar_channel_value(channel: &AnimationChannel, time: MediaTime, fallback: f64) -> f64 {
    let AnimationChannel::Scalar {
        keys,
        extrapolation,
    } = channel
    else {
        return discrete_channel_value(channel, time)
            .and_then(|value| value.as_f64())
            .unwrap_or(fallback);
    };
    let keys = sorted_keys(keys);
    if keys.is_empty() {
        return fallback;
    }
    let ticks = time.as_ticks() as f64;
    let first = keys[0];
    let last = keys[keys.len() - 1];
    if ticks <= first.time.as_ticks() as f64 {
        if ticks == first.time.as_ticks() as f64 {
            return first.value;
        }
        let mode = extrapolation_mode(extrapolation.as_ref(), "before");
        return extrapolate(first, keys.get(1).copied(), &mode, ticks);
    }
    if ticks >= last.time.as_ticks() as f64 {
        if ticks == last.time.as_ticks() as f64 {
            return last.value;
        }
        let mode = extrapolation_mode(extrapolation.as_ref(), "after");
        let neighbor = if keys.len() >= 2 {
            Some(keys[keys.len() - 2])
        } else {
            None
        };
        return extrapolate(last, neighbor, &mode, ticks);
    }

    let mut left = keys[0];
    let mut right = keys[1];
    for window in keys.windows(2) {
        if window[0].time.as_ticks() as f64 <= ticks && ticks <= window[1].time.as_ticks() as f64 {
            left = window[0];
            right = window[1];
            break;
        }
    }

    let span = right.time.as_ticks() as f64 - left.time.as_ticks() as f64;
    if span == 0.0 {
        return right.value;
    }
    if ticks == right.time.as_ticks() as f64 {
        return right.value;
    }
    match left.segment_to_next.as_str() {
        "step" => left.value,
        "linear" => {
            let progress = (ticks - left.time.as_ticks() as f64) / span;
            left.value + (right.value - left.value) * progress
        }
        _ => {
            let right_handle = left
                .right_handle
                .clone()
                .unwrap_or_else(|| default_right_handle(left, right));
            let left_handle = right
                .left_handle
                .clone()
                .unwrap_or_else(|| default_left_handle(left, right));
            let progress = solve_progress_for_time(ticks, left, right, &right_handle, &left_handle);
            cubic(
                progress,
                left.value,
                left.value + right_handle.dv,
                right.value + left_handle.dv,
                right.value,
            )
        }
    }
}

fn discrete_channel_value(
    channel: &AnimationChannel,
    time: MediaTime,
) -> Option<serde_json::Value> {
    let AnimationChannel::Discrete { keys } = channel else {
        return None;
    };
    let mut result = None;
    for key in keys {
        if key.time <= time {
            result = Some(key.value.clone());
        }
    }
    result
}

pub fn scalar_at(
    animations: Option<&ElementAnimations>,
    path: &str,
    base: f64,
    local_time: MediaTime,
) -> f64 {
    let Some(animations) = animations else {
        return base;
    };
    let Some(channel) = lookup(animations, path, "value") else {
        return base;
    };
    scalar_channel_value(channel, local_time, base)
}

pub fn color_at(
    animations: Option<&ElementAnimations>,
    path: &str,
    base: &str,
    local_time: MediaTime,
) -> [f64; 4] {
    let fallback = cutix_project::color::parse_to_srgb_rgba(base).unwrap_or([1.0, 1.0, 1.0, 1.0]);
    let Some(animations) = animations else {
        return fallback;
    };
    let components = ["r", "g", "b", "a"];
    if components
        .iter()
        .all(|component| lookup(animations, path, component).is_none())
    {
        return fallback;
    }

    let mut resolved = fallback;
    for (index, component) in components.iter().enumerate() {
        let linear_default = if index == 3 {
            fallback[3]
        } else {
            cutix_project::color::srgb_to_linear_channel(fallback[index])
        };
        let value = match lookup(animations, path, component) {
            Some(channel) => scalar_channel_value(channel, local_time, linear_default),
            None => linear_default,
        };
        resolved[index] = if index == 3 {
            value.clamp(0.0, 1.0)
        } else {
            cutix_project::color::linear_to_srgb_channel(value)
        };
    }
    resolved
}

pub fn has_channel(animations: Option<&ElementAnimations>, path: &str) -> bool {
    animations
        .map(|animations| lookup(animations, path, "value").is_some())
        .unwrap_or(false)
}

pub fn local_time(time: MediaTime, start: MediaTime, duration: MediaTime) -> MediaTime {
    let local = time - start;
    local.clamp(MediaTime::ZERO, duration)
}
