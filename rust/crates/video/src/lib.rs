#[cfg(not(target_arch = "wasm32"))]
pub mod backend;
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
    audio_extensions, backend_for, capabilities, dynamic_range, opens_audio, opens_video, probe,
    video_extensions, BackendKind, Capabilities, DecodeBackend, VideoStream,
};
pub use color::{ColorSpec, Matrix, SpsColor};
#[cfg(not(target_arch = "wasm32"))]
pub use decode::{
    first_frame, frame_at, frame_at_with_color, sps_color, DecodeError, Frame, NativeStream,
    VideoInfo,
};
pub use track::{
    track_region, track_region_scored, track_sequence, track_sequence_scored, Region, TrackOptions,
    TrackStep,
};

pub use reframe::{
    crop_size, crop_window, reframe_path, subject_center, CropWindow, Point, ReframeOptions,
};

pub use stabilize::{
    cumulative_trajectory, estimate_shift, phase_correlate, smooth_trajectory,
    stabilization_offsets, Shift, StabilizeOptions,
};
