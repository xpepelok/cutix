pub mod backend;
pub mod error;
pub mod fit;
pub mod presets;

#[cfg(not(target_arch = "wasm32"))]
pub mod aac;
#[cfg(not(target_arch = "wasm32"))]
pub mod job;
#[cfg(not(target_arch = "wasm32"))]
pub mod openh264_mp4;
#[cfg(not(target_arch = "wasm32"))]
pub mod package;
#[cfg(not(target_arch = "wasm32"))]
pub mod wav;

pub use backend::{
    backend_named, backends, default_backend, AudioSpec, AudioSupport, BackendFactory,
    EncoderBackend, ExportArtifacts, VideoSpec,
};
pub use error::{ExportError, Result};
pub use fit::{blit_centre, compute_contain_fit, plan_fit, ContainFit, FitPlan};
pub use presets::{
    preset, ExportFormat, ExportPreset, ExportPresetId, ExportQuality, EXPORT_PRESETS,
    EXPORT_QUALITIES,
};

#[cfg(not(target_arch = "wasm32"))]
pub use job::{
    frame_count, run, scene_duration, ExportOutcome, ExportRequest, Progress, Stage,
    EXPORT_CHANNELS, EXPORT_SAMPLE_RATE,
};
#[cfg(not(target_arch = "wasm32"))]
pub use package::{
    export_package, import_package, package_extension, PackageManifest, PackageOutcome,
    PackageProgress, PackageStage, PACKAGE_EXTENSION, PACKAGE_FORMAT_VERSION,
};
