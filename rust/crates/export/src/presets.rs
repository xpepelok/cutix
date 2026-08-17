#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportPresetId {
    Project,
    Youtube1080p,
    Vertical1080p,
    Square1080,
    Portrait4x5,
}

#[derive(Clone, Copy, Debug)]
pub struct ExportPreset {
    pub id: ExportPresetId,
    pub label_key: &'static str,
    pub resolution: Option<(u32, u32)>,
}

pub const EXPORT_PRESETS: [ExportPreset; 5] = [
    ExportPreset {
        id: ExportPresetId::Project,
        label_key: "export.preset.project",
        resolution: None,
    },
    ExportPreset {
        id: ExportPresetId::Youtube1080p,
        label_key: "export.preset.youtube",
        resolution: Some((1920, 1080)),
    },
    ExportPreset {
        id: ExportPresetId::Vertical1080p,
        label_key: "export.preset.vertical",
        resolution: Some((1080, 1920)),
    },
    ExportPreset {
        id: ExportPresetId::Square1080,
        label_key: "export.preset.square",
        resolution: Some((1080, 1080)),
    },
    ExportPreset {
        id: ExportPresetId::Portrait4x5,
        label_key: "export.preset.portrait",
        resolution: Some((1080, 1350)),
    },
];

impl ExportPresetId {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Youtube1080p => "1920x1080",
            Self::Vertical1080p => "1080x1920",
            Self::Square1080 => "1080x1080",
            Self::Portrait4x5 => "1080x1350",
        }
    }
}

pub fn preset(id: ExportPresetId) -> ExportPreset {
    EXPORT_PRESETS
        .iter()
        .copied()
        .find(|candidate| candidate.id == id)
        .unwrap_or(EXPORT_PRESETS[0])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportQuality {
    Low,
    Medium,
    High,
    VeryHigh,
}

pub const EXPORT_QUALITIES: [ExportQuality; 4] = [
    ExportQuality::Low,
    ExportQuality::Medium,
    ExportQuality::High,
    ExportQuality::VeryHigh,
];

impl ExportQuality {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Low => "export.quality.low",
            Self::Medium => "export.quality.medium",
            Self::High => "export.quality.high",
            Self::VeryHigh => "export.quality.veryHigh",
        }
    }

    fn bits_per_pixel(self) -> f64 {
        match self {
            Self::Low => 0.04,
            Self::Medium => 0.08,
            Self::High => 0.14,
            Self::VeryHigh => 0.24,
        }
    }

    pub fn bitrate_bps(self, width: u32, height: u32, fps: f64) -> u32 {
        let pixels = (width as f64) * (height as f64) * fps.max(1.0);
        (pixels * self.bits_per_pixel()).clamp(200_000.0, 120_000_000.0) as u32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Mp4,
}

impl ExportFormat {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mp4 => "mp4",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Mp4 => "export.format.mp4",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_bitrates_are_ordered() {
        let rates: Vec<u32> = EXPORT_QUALITIES
            .iter()
            .map(|quality| quality.bitrate_bps(1920, 1080, 30.0))
            .collect();
        assert!(rates.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn a_tiny_output_still_gets_a_usable_bitrate() {
        assert_eq!(ExportQuality::Low.bitrate_bps(16, 16, 1.0), 200_000);
    }

    #[test]
    fn every_preset_slug_is_distinct() {
        let mut slugs: Vec<&str> = EXPORT_PRESETS.iter().map(|entry| entry.id.slug()).collect();
        slugs.sort_unstable();
        let count = slugs.len();
        slugs.dedup();
        assert_eq!(slugs.len(), count);
        assert!(slugs.iter().all(|slug| !slug.is_empty()));
    }

    #[test]
    fn the_project_preset_has_no_fixed_resolution() {
        assert!(preset(ExportPresetId::Project).resolution.is_none());
        assert_eq!(
            preset(ExportPresetId::Vertical1080p).resolution,
            Some((1080, 1920))
        );
    }
}
