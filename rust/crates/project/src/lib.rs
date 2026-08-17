pub mod base64;
pub mod color;
pub mod error;
pub mod mattes;
pub mod media;
pub mod migrate;
pub mod model;
pub mod probe;
pub mod store;
pub mod subtitle_ass;
pub mod subtitle_vtt;
pub mod subtitles;
pub mod template_project;

pub use error::{ProjectError, Result};
pub use mattes::{
    referenced_files as matte_referenced_files, sweep_orphans as sweep_matte_orphans,
    MATTE_DIRECTORY_NAME,
};
pub use media::MediaStore;
pub use migrate::{detect_version, migrate_to_current, CURRENT_PROJECT_VERSION};
pub use model::{
    Attribution, AudioElement, Background, CanvasSize, Crop, EffectElement, ElementAnimations,
    GraphicElement, ImageElement, MediaAssetData, MediaType, Project, ProjectMetadata,
    ProjectSettings, ProjectSummary, Scene, SceneTracks, StickerElement, TextElement,
    TimelineElement, TimelineViewState, Track, Transform, VideoElement,
};
pub use probe::{probe, ProbeResult};
pub use store::{now_iso, LoadedProject, ProjectStore};
pub use subtitle_ass::parse_ass;
pub use subtitle_vtt::parse_vtt;
pub use subtitles::{
    parse_srt, parse_subtitle_file, serialize_srt, ParseSubtitleResult, SubtitleBackground,
    SubtitleCue, SubtitleFormat, SubtitlePlacement, SubtitleStyleOverrides, SubtitleWarning,
};
pub use template_project::{
    build_template_from_project, instantiate_template, TemplateInstantiation, TemplateMediaRef,
    TemplateProjectError, TemplateScenes, TEMPLATE_SCENES_VERSION,
};
