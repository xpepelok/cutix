use compositor::{
    BlendMode, EffectPassDescriptor, EffectUniformValueDescriptor, QuadTransformDescriptor,
    SourceRectDescriptor,
};
use cutix_project::model::{
    Crop, Effect, ElementAnimations, ElementTransition, ParamValues, RetimeConfig, TimelineElement,
    Track, Transform,
};
use time::MediaTime;

use crate::animation::{has_channel, local_time, scalar_at};
use crate::effects_map::effect_passes;

pub const MIN_CROP_SPAN: f64 = 0.02;

pub struct VisualParams<'a> {
    pub media_id: &'a str,
    pub transform: &'a Transform,
    pub crop: Option<&'a Crop>,
    pub opacity: f64,
    pub blend_mode: Option<&'a str>,
    pub effects: Option<&'a Vec<Effect>>,
    pub hidden: bool,
    pub retime: Option<&'a RetimeConfig>,
    pub transition: Option<&'a ElementTransition>,
    pub is_video: bool,
}

pub fn visual_params(element: &TimelineElement) -> Option<VisualParams<'_>> {
    match element {
        TimelineElement::Video(video) => Some(VisualParams {
            media_id: &video.media_id,
            transform: &video.transform,
            crop: video.crop.as_ref(),
            opacity: video.opacity,
            blend_mode: video.blend_mode.as_deref(),
            effects: video.effects.as_ref(),
            hidden: video.hidden.unwrap_or(false),
            retime: video.retime.as_ref(),
            transition: video.transition.as_ref(),
            is_video: true,
        }),
        TimelineElement::Image(image) => Some(VisualParams {
            media_id: &image.media_id,
            transform: &image.transform,
            crop: image.crop.as_ref(),
            opacity: image.opacity,
            blend_mode: image.blend_mode.as_deref(),
            effects: image.effects.as_ref(),
            hidden: image.hidden.unwrap_or(false),
            retime: None,
            transition: image.transition.as_ref(),
            is_video: false,
        }),
        _ => None,
    }
}

pub enum RasterSource<'a> {
    Sticker {
        sticker_id: &'a str,
        intrinsic_width: Option<f64>,
        intrinsic_height: Option<f64>,
    },
    Graphic {
        definition_id: &'a str,
        params: &'a ParamValues,
    },
}

pub fn raster_params(element: &TimelineElement) -> Option<(RasterSource<'_>, VisualParams<'_>)> {
    match element {
        TimelineElement::Sticker(sticker) => Some((
            RasterSource::Sticker {
                sticker_id: &sticker.sticker_id,
                intrinsic_width: sticker.intrinsic_width,
                intrinsic_height: sticker.intrinsic_height,
            },
            VisualParams {
                media_id: "",
                transform: &sticker.transform,
                crop: sticker.crop.as_ref(),
                opacity: sticker.opacity,
                blend_mode: sticker.blend_mode.as_deref(),
                effects: sticker.effects.as_ref(),
                hidden: sticker.hidden.unwrap_or(false),
                retime: None,
                transition: None,
                is_video: false,
            },
        )),
        TimelineElement::Graphic(graphic) => Some((
            RasterSource::Graphic {
                definition_id: &graphic.definition_id,
                params: &graphic.params,
            },
            VisualParams {
                media_id: "",
                transform: &graphic.transform,
                crop: graphic.crop.as_ref(),
                opacity: graphic.opacity,
                blend_mode: graphic.blend_mode.as_deref(),
                effects: graphic.effects.as_ref(),
                hidden: graphic.hidden.unwrap_or(false),
                retime: None,
                transition: None,
                is_video: false,
            },
        )),
        _ => None,
    }
}

pub fn contain_scale(
    source_width: f64,
    source_height: f64,
    canvas_width: f64,
    canvas_height: f64,
) -> f64 {
    if source_width <= 0.0 || source_height <= 0.0 {
        return 1.0;
    }
    (canvas_width / source_width).min(canvas_height / source_height)
}

pub fn track_hidden(track: &Track) -> bool {
    match track {
        Track::Video { hidden, .. }
        | Track::Text { hidden, .. }
        | Track::Graphic { hidden, .. }
        | Track::Effect { hidden, .. } => *hidden,
        Track::Audio { .. } => true,
    }
}

pub fn parse_blend_mode(value: Option<&str>) -> BlendMode {
    match value.unwrap_or("normal") {
        "darken" => BlendMode::Darken,
        "multiply" => BlendMode::Multiply,
        "color-burn" => BlendMode::ColorBurn,
        "lighten" => BlendMode::Lighten,
        "screen" => BlendMode::Screen,
        "plus-lighter" => BlendMode::PlusLighter,
        "color-dodge" => BlendMode::ColorDodge,
        "overlay" => BlendMode::Overlay,
        "soft-light" => BlendMode::SoftLight,
        "hard-light" => BlendMode::HardLight,
        "difference" => BlendMode::Difference,
        "exclusion" => BlendMode::Exclusion,
        "hue" => BlendMode::Hue,
        "saturation" => BlendMode::Saturation,
        "color" => BlendMode::Color,
        "luminosity" => BlendMode::Luminosity,
        _ => BlendMode::Normal,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NormalizedCrop {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

fn clamp_inset(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0 - MIN_CROP_SPAN)
    } else {
        0.0
    }
}

pub fn normalize_crop(crop: Option<&Crop>) -> NormalizedCrop {
    let Some(crop) = crop else {
        return NormalizedCrop::default();
    };
    let left = clamp_inset(crop.left);
    let top = clamp_inset(crop.top);
    let right = clamp_inset(crop.right).min(1.0 - MIN_CROP_SPAN - left);
    let bottom = clamp_inset(crop.bottom).min(1.0 - MIN_CROP_SPAN - top);
    NormalizedCrop {
        left,
        top,
        right,
        bottom,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ResolvedTransform {
    pub scale_x: f64,
    pub scale_y: f64,
    pub position_x: f64,
    pub position_y: f64,
    pub rotate: f64,
}

pub fn resolved_transform(
    params: &VisualParams<'_>,
    element: &TimelineElement,
    local: MediaTime,
) -> ResolvedTransform {
    let animations = element.base().animations.as_ref();
    ResolvedTransform {
        scale_x: scalar_at(
            animations,
            "transform.scaleX",
            params.transform.scale_x,
            local,
        ),
        scale_y: scalar_at(
            animations,
            "transform.scaleY",
            params.transform.scale_y,
            local,
        ),
        position_x: scalar_at(
            animations,
            "transform.positionX",
            params.transform.position.x,
            local,
        ),
        position_y: scalar_at(
            animations,
            "transform.positionY",
            params.transform.position.y,
            local,
        ),
        rotate: scalar_at(
            animations,
            "transform.rotate",
            params.transform.rotate,
            local,
        ),
    }
}

pub fn resolved_crop(
    params: &VisualParams<'_>,
    element: &TimelineElement,
    local: MediaTime,
) -> NormalizedCrop {
    let animations = element.base().animations.as_ref();
    let base = normalize_crop(params.crop);
    let animated = Crop {
        left: scalar_at(animations, "crop.left", base.left, local),
        top: scalar_at(animations, "crop.top", base.top, local),
        right: scalar_at(animations, "crop.right", base.right, local),
        bottom: scalar_at(animations, "crop.bottom", base.bottom, local),
    };
    normalize_crop(Some(&animated))
}

pub fn quad_transform(
    transform: &ResolvedTransform,
    crop: NormalizedCrop,
    source_width: f64,
    source_height: f64,
    canvas_width: f64,
    canvas_height: f64,
) -> QuadTransformDescriptor {
    let contain = (canvas_width / source_width).min(canvas_height / source_height);
    let scaled_width = source_width * contain * transform.scale_x;
    let scaled_height = source_height * contain * transform.scale_y;
    let abs_width = scaled_width.abs();
    let abs_height = scaled_height.abs();
    let flip_x = scaled_width < 0.0;
    let flip_y = scaled_height < 0.0;

    let crop_width = 1.0 - crop.left - crop.right;
    let crop_height = 1.0 - crop.top - crop.bottom;
    let local_offset_x =
        ((crop.left - crop.right) / 2.0) * abs_width * if flip_x { -1.0 } else { 1.0 };
    let local_offset_y =
        ((crop.top - crop.bottom) / 2.0) * abs_height * if flip_y { -1.0 } else { 1.0 };
    let radians = transform.rotate.to_radians();
    let (sin, cos) = radians.sin_cos();

    QuadTransformDescriptor {
        center_x: (canvas_width / 2.0 + transform.position_x + local_offset_x * cos
            - local_offset_y * sin) as f32,
        center_y: (canvas_height / 2.0
            + transform.position_y
            + local_offset_x * sin
            + local_offset_y * cos) as f32,
        width: (abs_width * crop_width) as f32,
        height: (abs_height * crop_height) as f32,
        rotation_degrees: transform.rotate as f32,
        flip_x,
        flip_y,
        source_rect: SourceRectDescriptor {
            x: crop.left as f32,
            y: crop.top as f32,
            width: crop_width as f32,
            height: crop_height as f32,
        },
    }
}

pub fn text_transform(
    transform: &Transform,
    element: &TimelineElement,
    local: MediaTime,
) -> ResolvedTransform {
    let animations = element.base().animations.as_ref();
    ResolvedTransform {
        scale_x: scalar_at(animations, "transform.scaleX", transform.scale_x, local),
        scale_y: scalar_at(animations, "transform.scaleY", transform.scale_y, local),
        position_x: scalar_at(
            animations,
            "transform.positionX",
            transform.position.x,
            local,
        ),
        position_y: scalar_at(
            animations,
            "transform.positionY",
            transform.position.y,
            local,
        ),
        rotate: scalar_at(animations, "transform.rotate", transform.rotate, local),
    }
}

pub fn bitmap_quad(
    transform: &ResolvedTransform,
    layer: &crate::text_render::TextLayer,
    canvas_width: f64,
    canvas_height: f64,
) -> QuadTransformDescriptor {
    let width = layer.width as f64 * transform.scale_x;
    let height = layer.height as f64 * transform.scale_y;
    let flip_x = width < 0.0;
    let flip_y = height < 0.0;
    let offset_x = (layer.width as f64 / 2.0 - layer.anchor_x) * transform.scale_x;
    let offset_y = (layer.height as f64 / 2.0 - layer.anchor_y) * transform.scale_y;
    let radians = transform.rotate.to_radians();
    let (sin, cos) = radians.sin_cos();

    QuadTransformDescriptor {
        center_x: (canvas_width / 2.0 + transform.position_x + offset_x * cos - offset_y * sin)
            as f32,
        center_y: (canvas_height / 2.0 + transform.position_y + offset_x * sin + offset_y * cos)
            as f32,
        width: width.abs() as f32,
        height: height.abs() as f32,
        rotation_degrees: transform.rotate as f32,
        flip_x,
        flip_y,
        source_rect: SourceRectDescriptor {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        },
    }
}

pub fn apply_transition_easing(progress: f64, easing: Option<&str>) -> f64 {
    let progress = progress.clamp(0.0, 1.0);
    match easing.unwrap_or("linear") {
        "easeIn" => progress * progress,
        "easeOut" => 1.0 - (1.0 - progress) * (1.0 - progress),
        "easeInOut" => {
            if progress < 0.5 {
                2.0 * progress * progress
            } else {
                1.0 - 2.0 * (1.0 - progress) * (1.0 - progress)
            }
        }
        _ => progress,
    }
}

pub fn resolved_opacity(
    params: &VisualParams<'_>,
    element: &TimelineElement,
    local: MediaTime,
) -> f64 {
    let animations = element.base().animations.as_ref();
    scalar_at(animations, "opacity", params.opacity, local).clamp(0.0, 1.0)
}

fn resolved_effect_params(
    effect: &Effect,
    animations: Option<&ElementAnimations>,
    local: MediaTime,
) -> ParamValues {
    let mut resolved = effect.params.clone();
    if animations.is_none() {
        return resolved;
    }
    for (key, value) in effect.params.iter() {
        let Some(base) = value.as_f64() else {
            continue;
        };
        let path = format!("effects.{}.params.{}", effect.id, key);
        if !has_channel(animations, &path) {
            continue;
        }
        let animated = scalar_at(animations, &path, base, local);
        resolved.insert(key.clone(), serde_json::json!(animated));
    }
    resolved
}

pub fn effect_pass_groups(
    effects: Option<&Vec<Effect>>,
    animations: Option<&ElementAnimations>,
    local: MediaTime,
    width: u32,
    height: u32,
) -> Vec<Vec<EffectPassDescriptor>> {
    let Some(effects) = effects else {
        return Vec::new();
    };
    effects
        .iter()
        .filter(|effect| effect.enabled)
        .map(|effect| {
            let params = resolved_effect_params(effect, animations, local);
            effect_passes(&effect.effect_type, &params, width, height)
        })
        .filter(|passes| !passes.is_empty())
        .collect()
}

pub fn uniform_number(value: f32) -> EffectUniformValueDescriptor {
    EffectUniformValueDescriptor::Number(value)
}

pub fn element_local_time(element: &TimelineElement, time: MediaTime) -> MediaTime {
    local_time(time, element.base().start_time, element.base().duration)
}

pub fn is_visible(element: &TimelineElement, time: MediaTime) -> bool {
    time >= element.base().start_time && time < element.end_time()
}
