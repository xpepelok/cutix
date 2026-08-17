pub mod defaults;
pub mod layout;
pub mod presets;
pub mod timing;
pub mod types;

pub use defaults::*;
pub use layout::{
    clamp_watermark_opacity, clamp_watermark_size, clamp_watermark_tile_spacing,
    compute_watermark_rect, compute_watermark_rects, compute_watermark_size,
    compute_watermark_tile_rects, compute_watermark_transform, watermark_rect_to_transform,
    CanvasSize, Position, SourceSize, Transform, WatermarkRect,
};
pub use presets::{
    apply_watermark_preset, list_watermark_presets, remove_watermark_preset,
    rename_watermark_preset, save_watermark_preset, StoredWatermarkPreset,
};
pub use timing::{compute_fade_opacity, resolve_watermark_window, WatermarkWindow};
pub use types::*;
