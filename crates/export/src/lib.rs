mod backend;
mod capabilities;
mod error;
pub mod fit;
mod presets;

#[cfg(not(target_arch = "wasm32"))]
pub mod aac;
#[cfg(not(target_arch = "wasm32"))]
mod ffmpeg_mp4;
#[cfg(not(target_arch = "wasm32"))]
mod job;
#[cfg(not(target_arch = "wasm32"))]
mod mp4_sink;
#[cfg(not(target_arch = "wasm32"))]
mod openh264_mp4;
#[cfg(not(target_arch = "wasm32"))]
mod package;
#[cfg(not(target_arch = "wasm32"))]
pub mod remux;
#[cfg(not(target_arch = "wasm32"))]
pub mod wav;

pub use backend::{
    AudioSpec, AudioSupport, BackendFactory, EncoderBackend, EncoderKind, ExportArtifacts,
    STREAM_COPY, VideoSpec, backend_named, backends, default_backend, encoder_kind, encoder_label,
    hardware_backend,
};
pub use capabilities::{AudioStrategy, ExportCapabilities};
pub use error::{ExportError, Result};
pub use ffmpeg_mp4::{FFMPEG_MP4, NAME as FFMPEG_MP4_NAME, chosen_encoder};
pub use fit::{
    ContainFit, FitPlan, RgbaFrame, RgbaFrameMut, blit_centre, compute_contain_fit, plan_fit,
};
pub use openh264_mp4::OPENH264_MP4;
pub use presets::{
    EXPORT_PRESETS, EXPORT_QUALITIES, ExportFormat, ExportPreset, ExportPresetId, ExportQuality,
    preset,
};

#[cfg(not(target_arch = "wasm32"))]
pub use job::{
    EXPORT_CHANNELS, EXPORT_SAMPLE_RATE, ExportOutcome, ExportRequest, Progress, Stage,
    frame_count, run, scene_duration,
};
#[cfg(not(target_arch = "wasm32"))]
pub use package::{
    PACKAGE_EXTENSION, PACKAGE_FORMAT_VERSION, PackageManifest, PackageOutcome, PackageProgress,
    PackageStage, export_package, import_package, package_extension,
};
