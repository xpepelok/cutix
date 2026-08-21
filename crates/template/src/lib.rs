use serde::{Deserialize, Serialize};

pub const TEMPLATE_FORMAT_VERSION: u32 = 1;

include!(concat!(env!("OUT_DIR"), "/builtin.rs"));

pub fn builtin_templates() -> Vec<TemplateManifest> {
    BUILTIN
        .iter()
        .filter_map(|(_, raw)| parse(raw).ok())
        .collect()
}

pub fn builtin_names() -> Vec<&'static str> {
    BUILTIN.iter().map(|(name, _)| *name).collect()
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TemplateManifest {
    pub format_version: u32,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub license: String,
    pub canvas: CanvasSpec,
    pub fps: FpsSpec,
    pub duration_seconds: f32,
    pub slots: Vec<TemplateSlot>,
    #[serde(default)]
    pub scenes: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CanvasSpec {
    pub width: u32,
    pub height: u32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FpsSpec {
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TemplateSlot {
    pub id: String,
    pub kind: SlotKind,
    pub label: String,
    pub start_seconds: f32,
    pub duration_seconds: f32,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub placeholder_text: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SlotKind {
    Video,
    Image,
    Audio,
    Text,
}

/// Why a template manifest could not be used.
#[derive(Debug)]
pub enum TemplateError {
    /// The manifest declares a version this build does not know how to read.
    UnsupportedVersion(u32),
    EmptyName,
    NoSlots,
    /// Two slots share an id, so a fill cannot say which one it means.
    DuplicateSlotId(String),
    InvalidCanvas,
    InvalidFps,
    /// A slot starts before zero or has a non-positive duration.
    NegativeTiming(String),
    /// A slot runs past the end of the template's own timeline.
    SlotOutsideTimeline(String),
    /// The manifest is not readable as JSON, or does not have the shape of a template.
    Malformed(serde_json::Error),
}

impl std::fmt::Display for TemplateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported template version {version}")
            }
            Self::EmptyName => write!(formatter, "template name is empty"),
            Self::NoSlots => write!(formatter, "template has no slots"),
            Self::DuplicateSlotId(id) => write!(formatter, "duplicate slot id '{id}'"),
            Self::InvalidCanvas => write!(formatter, "canvas size must be positive"),
            Self::InvalidFps => write!(formatter, "frame rate must be positive"),
            Self::NegativeTiming(id) => {
                write!(formatter, "slot '{id}' has negative timing")
            }
            Self::SlotOutsideTimeline(id) => {
                write!(formatter, "slot '{id}' ends after the template duration")
            }
            Self::Malformed(source) => write!(formatter, "template is not readable: {source}"),
        }
    }
}

impl PartialEq for TemplateError {
    /// Compares by what went wrong. Two malformed manifests are the same failure when
    /// the parser said the same thing about them; `serde_json::Error` has no equality of
    /// its own, and its message is the only part a caller can act on.
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::UnsupportedVersion(left), Self::UnsupportedVersion(right)) => left == right,
            (Self::EmptyName, Self::EmptyName) => true,
            (Self::NoSlots, Self::NoSlots) => true,
            (Self::DuplicateSlotId(left), Self::DuplicateSlotId(right)) => left == right,
            (Self::InvalidCanvas, Self::InvalidCanvas) => true,
            (Self::InvalidFps, Self::InvalidFps) => true,
            (Self::NegativeTiming(left), Self::NegativeTiming(right)) => left == right,
            (Self::SlotOutsideTimeline(left), Self::SlotOutsideTimeline(right)) => left == right,
            (Self::Malformed(left), Self::Malformed(right)) => {
                left.to_string() == right.to_string()
            }
            _ => false,
        }
    }
}

impl std::error::Error for TemplateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Malformed(source) => Some(source),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for TemplateError {
    fn from(source: serde_json::Error) -> Self {
        Self::Malformed(source)
    }
}

pub fn validate(manifest: &TemplateManifest) -> Result<(), TemplateError> {
    if manifest.format_version != TEMPLATE_FORMAT_VERSION {
        return Err(TemplateError::UnsupportedVersion(manifest.format_version));
    }
    if manifest.name.trim().is_empty() {
        return Err(TemplateError::EmptyName);
    }
    if manifest.canvas.width == 0 || manifest.canvas.height == 0 {
        return Err(TemplateError::InvalidCanvas);
    }
    if manifest.fps.numerator == 0 || manifest.fps.denominator == 0 {
        return Err(TemplateError::InvalidFps);
    }
    if manifest.slots.is_empty() {
        return Err(TemplateError::NoSlots);
    }

    let mut seen = std::collections::HashSet::new();
    for slot in &manifest.slots {
        if !seen.insert(slot.id.as_str()) {
            return Err(TemplateError::DuplicateSlotId(slot.id.clone()));
        }
        if slot.start_seconds < 0.0 || slot.duration_seconds <= 0.0 {
            return Err(TemplateError::NegativeTiming(slot.id.clone()));
        }
        if slot.start_seconds + slot.duration_seconds > manifest.duration_seconds + 1e-3 {
            return Err(TemplateError::SlotOutsideTimeline(slot.id.clone()));
        }
    }

    Ok(())
}

/// Reads a template manifest and checks it describes something usable.
///
/// Both failures — unreadable JSON and a manifest whose slots do not make sense — come
/// back as [`TemplateError`], so a caller can tell a corrupt file from a valid file
/// describing an impossible template.
pub fn parse(raw: &str) -> Result<TemplateManifest, TemplateError> {
    let manifest: TemplateManifest = serde_json::from_str(raw)?;
    validate(&manifest)?;
    Ok(manifest)
}

pub fn to_json(manifest: &TemplateManifest) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(manifest)
}

pub fn required_slots(manifest: &TemplateManifest) -> Vec<&TemplateSlot> {
    manifest.slots.iter().filter(|slot| slot.required).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(id: &str, start: f32, duration: f32) -> TemplateSlot {
        TemplateSlot {
            id: id.to_string(),
            kind: SlotKind::Video,
            label: format!("Slot {id}"),
            start_seconds: start,
            duration_seconds: duration,
            required: true,
            placeholder_text: None,
        }
    }

    fn manifest() -> TemplateManifest {
        TemplateManifest {
            format_version: TEMPLATE_FORMAT_VERSION,
            name: "Demo".to_string(),
            description: String::new(),
            author: String::new(),
            license: "CC0".to_string(),
            canvas: CanvasSpec {
                width: 1080,
                height: 1920,
            },
            fps: FpsSpec {
                numerator: 30,
                denominator: 1,
            },
            duration_seconds: 10.0,
            slots: vec![slot("a", 0.0, 4.0), slot("b", 4.0, 6.0)],
            scenes: serde_json::Value::Null,
        }
    }

    #[test]
    fn accepts_a_valid_manifest() {
        assert!(validate(&manifest()).is_ok());
    }

    #[test]
    fn round_trips_through_json() {
        let original = manifest();
        let encoded = to_json(&original).expect("encode");
        let decoded = parse(&encoded).expect("decode");
        assert_eq!(original, decoded);
    }

    #[test]
    fn rejects_a_future_version() {
        let mut broken = manifest();
        broken.format_version = 99;
        assert_eq!(
            validate(&broken),
            Err(TemplateError::UnsupportedVersion(99))
        );
    }

    #[test]
    fn rejects_duplicate_slot_ids() {
        let mut broken = manifest();
        broken.slots = vec![slot("a", 0.0, 2.0), slot("a", 2.0, 2.0)];
        assert_eq!(
            validate(&broken),
            Err(TemplateError::DuplicateSlotId("a".to_string()))
        );
    }

    #[test]
    fn rejects_slots_past_the_end() {
        let mut broken = manifest();
        broken.slots = vec![slot("a", 8.0, 5.0)];
        assert_eq!(
            validate(&broken),
            Err(TemplateError::SlotOutsideTimeline("a".to_string()))
        );
    }

    #[test]
    fn rejects_empty_slot_list() {
        let mut broken = manifest();
        broken.slots = Vec::new();
        assert_eq!(validate(&broken), Err(TemplateError::NoSlots));
    }

    #[test]
    fn rejects_zero_canvas() {
        let mut broken = manifest();
        broken.canvas = CanvasSpec {
            width: 0,
            height: 100,
        };
        assert_eq!(validate(&broken), Err(TemplateError::InvalidCanvas));
    }

    #[test]
    fn ships_no_builtin_templates() {
        let names = builtin_names();
        assert!(names.is_empty(), "the app ships no templates of its own");
    }

    #[test]
    fn builtin_templates_are_valid() {
        let templates = builtin_templates();
        assert_eq!(templates.len(), builtin_names().len());
        for template in &templates {
            validate(template).unwrap_or_else(|error| panic!("{}: {error}", template.name));
        }
    }

    #[test]
    fn lists_required_slots() {
        let mut sample = manifest();
        sample.slots[1].required = false;
        let required = required_slots(&sample);
        assert_eq!(required.len(), 1);
        assert_eq!(required[0].id, "a");
    }
}
