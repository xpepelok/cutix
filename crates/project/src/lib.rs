mod base64;
pub mod color;
mod error;
mod mattes;
mod media;
pub mod migrate;
pub mod model;
pub mod probe;
pub mod store;
mod subtitle_ass;
mod subtitle_vtt;
mod subtitles;
pub mod template_project;

pub use base64::encode as encode_base64;
pub use error::{ProjectError, Result};
pub use mattes::{
    MATTE_DIRECTORY_NAME, referenced_files as matte_referenced_files,
    sweep_orphans as sweep_matte_orphans,
};
pub use media::MediaStore;
pub use migrate::{CURRENT_PROJECT_VERSION, detect_version, migrate_to_current};
pub use model::{
    Attribution, AudioElement, Background, CanvasSize, Crop, EffectElement, ElementAnimations,
    GraphicElement, ImageElement, MediaAssetData, MediaType, Project, ProjectMetadata,
    ProjectSettings, ProjectSummary, Scene, SceneTracks, StickerElement, TextElement,
    TimelineElement, TimelineViewState, Track, Transform, VideoElement,
};
pub use probe::{ProbeResult, probe};
pub use store::{LoadedProject, ProjectStore, now_iso};
pub use subtitle_ass::parse_ass;
pub use subtitle_vtt::parse_vtt;
pub use subtitles::{
    ParseSubtitleResult, SubtitleBackground, SubtitleCue, SubtitleFormat, SubtitlePlacement,
    SubtitleStyleOverrides, SubtitleWarning, parse_srt, parse_subtitle_file, serialize_srt,
};
pub use template_project::{
    TEMPLATE_SCENES_VERSION, TemplateInstantiation, TemplateMediaRef, TemplateProjectError,
    TemplateScenes, build_template_from_project, instantiate_template,
};
