//! The editor: turning user intent into changes to a project document.
//!
//! Every mutation goes through [`Editor`], which snapshots the document for the undo stack
//! before it touches anything. The submodules are the areas that mutation falls into —
//! commands, history, fields, effects, text, tracks, captions — and each is re-exported
//! here, so a consumer writes `edit::something` without caring which one it came from.
//!
//! Nothing in here imports GPUI. The editor is domain logic; the panels that drive it are
//! the layer above.

use cutix_project::model::{
    AnimationChannel, BaseElementFields, Crop, Effect, ElementAnimations, ElementTransition,
    JsonMap, Mask, MotionSettings, ParamValues, RetimeConfig, ScalarAnimationKey, TextBackground,
    Transform, Vector2,
};
use cutix_project::{
    AudioElement, ImageElement, MediaAssetData, MediaType, Project, Scene, SceneTracks,
    TextElement, TimelineElement, Track, VideoElement,
};
use serde_json::{json, Value};
use time::MediaTime;

pub const SNAP_THRESHOLD_PX: f32 = 10.0;

pub const DEFAULT_NEW_ELEMENT_SECONDS: f64 = 5.0;

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn ticks(time: MediaTime) -> i64 {
    time.as_ticks()
}

fn add(left: MediaTime, right: MediaTime) -> MediaTime {
    MediaTime::from_ticks(ticks(left) + ticks(right))
}

fn sub(left: MediaTime, right: MediaTime) -> MediaTime {
    MediaTime::from_ticks(ticks(left) - ticks(right))
}

pub fn seconds(value: f64) -> MediaTime {
    MediaTime::from_seconds_f64(value).unwrap_or(MediaTime::ZERO)
}

pub fn min_duration(fps: f32) -> MediaTime {
    let fps = if fps > 0.0 { fps as f64 } else { 30.0 };
    MediaTime::from_ticks(((time::TICKS_PER_SECOND as f64) / fps).round() as i64)
}

pub fn snap_to_frame(time: MediaTime, fps: f32) -> MediaTime {
    let step = ticks(min_duration(fps)).max(1);
    let rounded = ((ticks(time) as f64) / step as f64).round() as i64 * step;
    MediaTime::from_ticks(rounded.max(0))
}

mod history;
pub use history::*;

mod commands;
pub use commands::*;

mod effects;
pub use effects::*;

mod fields;
pub use fields::*;

mod text;
pub use text::*;

mod settings;
pub use settings::*;

mod elements;
pub use elements::*;

mod tracks;
pub use tracks::*;

mod captions;
pub use captions::*;

mod construct;
pub use construct::*;

mod snapping;
pub use snapping::*;

#[cfg(test)]
mod test_support;

#[cfg(test)]
pub(crate) use test_support::*;

#[cfg(test)]
mod tests;
