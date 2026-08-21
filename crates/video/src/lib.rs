#[cfg(not(target_arch = "wasm32"))]
mod backend;
pub mod color;
#[cfg(not(target_arch = "wasm32"))]
pub mod decode;
#[cfg(not(target_arch = "wasm32"))]
pub mod ffmpeg;
pub mod reframe;
pub mod stabilize;
pub mod track;

#[cfg(not(target_arch = "wasm32"))]
pub use backend::{
    BackendKind, Capabilities, DecodeBackend, FfmpegBackend, VideoStream, audio_extensions,
    backend_for, backends_for, capabilities, dynamic_range, opens_audio, opens_video, probe,
    video_extensions,
};
pub use color::{ColorSpec, Matrix, SpsColor};
#[cfg(not(target_arch = "wasm32"))]
pub use decode::{
    DecodeError, Frame, NativeStream, VideoInfo, first_frame, frame_at, frame_at_with_color,
    sps_color,
};
pub use track::{
    Region, TrackOptions, TrackStep, track_region, track_region_scored, track_sequence,
    track_sequence_scored,
};

pub use reframe::{
    CropWindow, Point, ReframeOptions, crop_size, crop_window, reframe_path, subject_center,
};

pub use stabilize::{
    Shift, StabilizeOptions, cumulative_trajectory, estimate_shift, phase_correlate,
    smooth_trajectory, stabilization_offsets,
};
