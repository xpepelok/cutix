use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use time::{FrameRate, MediaTime};

pub type JsonMap = Map<String, Value>;
pub type ParamValues = Map<String, Value>;

fn is_false(value: &bool) -> bool {
    !*value
}

fn explicit_null<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub metadata: ProjectMetadata,
    pub scenes: Vec<Scene>,
    pub current_scene_id: String,
    pub settings: ProjectSettings,
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub timeline_view_state: Option<TimelineViewState>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub attributions: Option<Vec<Attribution>>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMetadata {
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub thumbnail: Option<String>,
    pub duration: MediaTime,
    pub created_at: String,
    pub updated_at: String,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanvasSize {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CanvasSizeMode {
    Preset,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Background {
    #[serde(rename = "color")]
    Color { color: String },
    #[serde(rename = "blur")]
    Blur {
        #[serde(rename = "blurIntensity")]
        blur_intensity: f64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettings {
    pub fps: FrameRate,
    pub canvas_size: CanvasSize,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub canvas_size_mode: Option<CanvasSizeMode>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "explicit_null"
    )]
    pub last_custom_canvas_size: Option<Option<CanvasSize>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "explicit_null"
    )]
    pub original_canvas_size: Option<Option<CanvasSize>>,
    pub background: Background,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub watermark: Option<Value>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineViewState {
    pub zoom_level: f64,
    pub scroll_left: f64,
    pub playhead_time: MediaTime,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attribution {
    pub id: String,
    pub title: String,
    pub creator: String,
    pub license: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub license_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub provider: Option<String>,
    pub added_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub time: MediaTime,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub duration: Option<MediaTime>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub id: String,
    pub name: String,
    pub is_main: bool,
    pub tracks: SceneTracks,
    pub bookmarks: Vec<Bookmark>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SceneTracks {
    pub overlay: Vec<Track>,
    pub main: Track,
    pub audio: Vec<Track>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Track {
    #[serde(rename = "video")]
    Video {
        id: String,
        name: String,
        elements: Vec<TimelineElement>,
        muted: bool,
        hidden: bool,
    },
    #[serde(rename = "text")]
    Text {
        id: String,
        name: String,
        elements: Vec<TimelineElement>,
        hidden: bool,
    },
    #[serde(rename = "audio")]
    Audio {
        id: String,
        name: String,
        elements: Vec<TimelineElement>,
        muted: bool,
    },
    #[serde(rename = "graphic")]
    Graphic {
        id: String,
        name: String,
        elements: Vec<TimelineElement>,
        hidden: bool,
    },
    #[serde(rename = "effect")]
    Effect {
        id: String,
        name: String,
        elements: Vec<TimelineElement>,
        hidden: bool,
    },
}

impl Track {
    pub fn id(&self) -> &str {
        match self {
            Track::Video { id, .. }
            | Track::Text { id, .. }
            | Track::Audio { id, .. }
            | Track::Graphic { id, .. }
            | Track::Effect { id, .. } => id,
        }
    }

    pub fn elements(&self) -> &[TimelineElement] {
        match self {
            Track::Video { elements, .. }
            | Track::Text { elements, .. }
            | Track::Audio { elements, .. }
            | Track::Graphic { elements, .. }
            | Track::Effect { elements, .. } => elements,
        }
    }

    pub fn elements_mut(&mut self) -> &mut Vec<TimelineElement> {
        match self {
            Track::Video { elements, .. }
            | Track::Text { elements, .. }
            | Track::Audio { elements, .. }
            | Track::Graphic { elements, .. }
            | Track::Effect { elements, .. } => elements,
        }
    }

    pub fn empty_video(id: String, name: String) -> Self {
        Track::Video {
            id,
            name,
            elements: Vec::new(),
            muted: false,
            hidden: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transform {
    pub scale_x: f64,
    pub scale_y: f64,
    pub position: Vector2,
    pub rotate: f64,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            scale_x: 1.0,
            scale_y: 1.0,
            position: Vector2 { x: 0.0, y: 0.0 },
            rotate: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Vector2 {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Crop {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Effect {
    pub id: String,
    #[serde(rename = "type")]
    pub effect_type: String,
    pub params: ParamValues,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mask {
    pub id: String,
    #[serde(rename = "type")]
    pub mask_type: String,
    pub params: ParamValues,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetimeConfig {
    pub rate: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub maintain_pitch: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub curve: Option<Vec<RetimeSpeedPoint>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub blend_frames: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetimeSpeedPoint {
    pub time: f64,
    pub speed: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementTransition {
    #[serde(rename = "type")]
    pub transition_type: String,
    pub duration: MediaTime,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub easing: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MotionSettings {
    pub preset_id: String,
    pub duration: MediaTime,
    pub intensity: f64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElementAnimations {
    pub bindings: std::collections::BTreeMap<String, Value>,
    pub channels: std::collections::BTreeMap<String, AnimationChannel>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AnimationChannel {
    #[serde(rename = "scalar")]
    Scalar {
        keys: Vec<ScalarAnimationKey>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        extrapolation: Option<Value>,
    },
    #[serde(rename = "discrete")]
    Discrete { keys: Vec<DiscreteAnimationKey> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScalarAnimationKey {
    pub id: String,
    pub time: MediaTime,
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub left_handle: Option<CurveHandle>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub right_handle: Option<CurveHandle>,
    pub segment_to_next: String,
    pub tangent_mode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurveHandle {
    pub dt: MediaTime,
    pub dv: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscreteAnimationKey {
    pub id: String,
    pub time: MediaTime,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseElementFields {
    pub id: String,
    pub name: String,
    pub duration: MediaTime,
    pub start_time: MediaTime,
    pub trim_start: MediaTime,
    pub trim_end: MediaTime,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source_duration: Option<MediaTime>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub animations: Option<ElementAnimations>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TimelineElement {
    #[serde(rename = "video")]
    Video(VideoElement),
    #[serde(rename = "image")]
    Image(ImageElement),
    #[serde(rename = "audio")]
    Audio(AudioElement),
    #[serde(rename = "text")]
    Text(TextElement),
    #[serde(rename = "sticker")]
    Sticker(StickerElement),
    #[serde(rename = "graphic")]
    Graphic(GraphicElement),
    #[serde(rename = "effect")]
    Effect(EffectElement),
}

impl TimelineElement {
    pub fn base(&self) -> &BaseElementFields {
        match self {
            TimelineElement::Video(element) => &element.base,
            TimelineElement::Image(element) => &element.base,
            TimelineElement::Audio(element) => &element.base,
            TimelineElement::Text(element) => &element.base,
            TimelineElement::Sticker(element) => &element.base,
            TimelineElement::Graphic(element) => &element.base,
            TimelineElement::Effect(element) => &element.base,
        }
    }

    pub fn end_time(&self) -> MediaTime {
        let base = self.base();
        MediaTime::from_ticks(base.start_time.as_ticks() + base.duration.as_ticks())
    }

    pub fn media_id(&self) -> Option<&str> {
        match self {
            TimelineElement::Video(element) => Some(element.media_id.as_str()),
            TimelineElement::Image(element) => Some(element.media_id.as_str()),
            TimelineElement::Audio(element) => element.media_id.as_deref(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoElement {
    #[serde(flatten)]
    pub base: BaseElementFields,
    pub media_id: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub volume: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub muted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub is_source_audio_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hidden: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub retime: Option<RetimeConfig>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub reversed_from: Option<Value>,
    pub transform: Transform,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub crop: Option<Crop>,
    pub opacity: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub blend_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effects: Option<Vec<Effect>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub masks: Option<Vec<Mask>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cutout: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub transition: Option<ElementTransition>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub motion: Option<MotionSettings>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageElement {
    #[serde(flatten)]
    pub base: BaseElementFields,
    pub media_id: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hidden: Option<bool>,
    pub transform: Transform,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub crop: Option<Crop>,
    pub opacity: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub blend_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effects: Option<Vec<Effect>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub masks: Option<Vec<Mask>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cutout: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub transition: Option<ElementTransition>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub motion: Option<MotionSettings>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioElement {
    #[serde(flatten)]
    pub base: BaseElementFields,
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub media_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub source_url: Option<String>,
    pub volume: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub muted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub retime: Option<RetimeConfig>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextBackground {
    pub enabled: bool,
    pub color: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub corner_radius: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub padding_x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub padding_y: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub offset_x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub offset_y: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextElement {
    #[serde(flatten)]
    pub base: BaseElementFields,
    pub content: String,
    pub font_size: f64,
    pub font_family: String,
    pub color: String,
    pub background: TextBackground,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stroke: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub shadow: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gradient: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub text_animations: Option<Value>,
    pub text_align: String,
    pub font_weight: String,
    pub font_style: String,
    pub text_decoration: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub letter_spacing: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub line_height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hidden: Option<bool>,
    pub transform: Transform,
    pub opacity: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub blend_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effects: Option<Vec<Effect>>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StickerElement {
    #[serde(flatten)]
    pub base: BaseElementFields,
    pub sticker_id: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub intrinsic_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub intrinsic_height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hidden: Option<bool>,
    pub transform: Transform,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub crop: Option<Crop>,
    pub opacity: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub blend_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effects: Option<Vec<Effect>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub cutout: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub motion: Option<MotionSettings>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphicElement {
    #[serde(flatten)]
    pub base: BaseElementFields,
    pub definition_id: String,
    pub params: ParamValues,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub hidden: Option<bool>,
    pub transform: Transform,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub crop: Option<Crop>,
    pub opacity: f64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub blend_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub effects: Option<Vec<Effect>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub masks: Option<Vec<Mask>>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectElement {
    #[serde(flatten)]
    pub base: BaseElementFields,
    pub effect_type: String,
    pub params: ParamValues,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Image,
    Video,
    Audio,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaAssetData {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub media_type: MediaType,
    pub size: u64,
    pub last_modified: i64,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub duration: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub fps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub has_audio: Option<bool>,
    #[serde(skip_serializing_if = "is_false", default)]
    pub ephemeral: bool,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub thumbnail_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub file_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub thumbnail: Option<String>,
    pub duration: MediaTime,
    pub created_at: String,
    pub updated_at: String,
}

pub const DEFAULT_CANVAS_WIDTH: u32 = 1920;
pub const DEFAULT_CANVAS_HEIGHT: u32 = 1080;
pub const DEFAULT_BACKGROUND_COLOR: &str = "#000000";

impl Project {
    pub fn new(name: impl Into<String>, now_iso: String) -> Self {
        let project_id = uuid::Uuid::new_v4().to_string();
        let scene_id = uuid::Uuid::new_v4().to_string();
        let main_track_id = uuid::Uuid::new_v4().to_string();

        let scene = Scene {
            id: scene_id.clone(),
            name: "Main scene".to_string(),
            is_main: true,
            tracks: SceneTracks {
                overlay: Vec::new(),
                main: Track::empty_video(main_track_id, "Main".to_string()),
                audio: Vec::new(),
            },
            bookmarks: Vec::new(),
            created_at: now_iso.clone(),
            updated_at: now_iso.clone(),
            extra: JsonMap::new(),
        };

        Self {
            metadata: ProjectMetadata {
                id: project_id,
                name: name.into(),
                thumbnail: None,
                duration: MediaTime::ZERO,
                created_at: now_iso.clone(),
                updated_at: now_iso,
                extra: JsonMap::new(),
            },
            scenes: vec![scene.clone()],
            current_scene_id: scene_id,
            settings: ProjectSettings {
                fps: FrameRate::FPS_30,
                canvas_size: CanvasSize {
                    width: DEFAULT_CANVAS_WIDTH,
                    height: DEFAULT_CANVAS_HEIGHT,
                },
                canvas_size_mode: Some(CanvasSizeMode::Preset),
                last_custom_canvas_size: Some(None),
                original_canvas_size: Some(None),
                background: Background::Color {
                    color: DEFAULT_BACKGROUND_COLOR.to_string(),
                },
                watermark: None,
                extra: JsonMap::new(),
            },
            version: crate::migrate::CURRENT_PROJECT_VERSION,
            timeline_view_state: None,
            attributions: None,
            extra: JsonMap::new(),
        }
    }

    pub fn main_scene(&self) -> Option<&Scene> {
        self.scenes
            .iter()
            .find(|scene| scene.is_main)
            .or_else(|| self.scenes.first())
    }

    pub fn computed_duration(&self) -> MediaTime {
        let Some(scene) = self.main_scene() else {
            return MediaTime::ZERO;
        };

        let mut max_end = 0i64;
        for track in scene.tracks.all() {
            for element in track.elements() {
                max_end = max_end.max(element.end_time().as_ticks());
            }
        }

        MediaTime::from_ticks(max_end)
    }

    pub fn summary(&self) -> ProjectSummary {
        ProjectSummary {
            id: self.metadata.id.clone(),
            name: self.metadata.name.clone(),
            thumbnail: self.metadata.thumbnail.clone(),
            duration: self.metadata.duration,
            created_at: self.metadata.created_at.clone(),
            updated_at: self.metadata.updated_at.clone(),
        }
    }
}

impl SceneTracks {
    pub fn all(&self) -> impl Iterator<Item = &Track> {
        std::iter::once(&self.main)
            .chain(self.overlay.iter())
            .chain(self.audio.iter())
    }
}
