use compositor::LayerMaskDescriptor;
use compositor::{
    CanvasClearDescriptor, Compositor, FrameDescriptor, FrameItemDescriptor, LayerDescriptor,
};
use cutix_project::model::{Background, TimelineElement};
use cutix_project::{Project, color::parse_to_linear_rgba};
use gpu::{GpuContext, wgpu};
use time::MediaTime;

use std::sync::Arc;

use crate::budget::MemoryBudget;
use crate::decode_cache::{DecodeCache, DecodeStats, SourceFrame};
use crate::error::{PlaybackError, Result};
use crate::media::MediaResolver;
use crate::raster_cache::RasterCache;
use crate::resolve::{
    RasterSource, bitmap_quad, contain_scale, effect_pass_groups, element_local_time, is_visible,
    parse_blend_mode, quad_transform, raster_params, resolved_crop, resolved_opacity,
    resolved_transform, text_transform, track_hidden, visual_params,
};
use crate::text_render::TextRasterizer;
use crate::transitions::{
    apply_transition_to_layer, build_track_transition_edges, is_visible_with_transition,
    resolve_active_transition,
};

pub struct ComposeRequest<'a> {
    pub project: &'a Project,
    pub scene_id: Option<&'a str>,
    pub time: MediaTime,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ElementRect {
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation_degrees: f32,
}

impl ElementRect {
    pub fn local_from_canvas(&self, x: f32, y: f32) -> (f32, f32) {
        let radians = self.rotation_degrees.to_radians();
        let (sin, cos) = radians.sin_cos();
        let dx = x - self.center_x;
        let dy = y - self.center_y;
        let local_x = dx * cos + dy * sin;
        let local_y = -dx * sin + dy * cos;
        (
            if self.width.abs() > f32::EPSILON {
                local_x / self.width
            } else {
                0.0
            },
            if self.height.abs() > f32::EPSILON {
                local_y / self.height
            } else {
                0.0
            },
        )
    }

    pub fn local_delta(&self, dx: f32, dy: f32) -> (f32, f32) {
        let (ux, uy) = self.local_from_canvas(self.center_x + dx, self.center_y + dy);
        (ux, uy)
    }

    pub fn canvas_from_local(&self, u: f32, v: f32) -> (f32, f32) {
        let radians = self.rotation_degrees.to_radians();
        let (sin, cos) = radians.sin_cos();
        let local_x = u * self.width;
        let local_y = v * self.height;
        (
            self.center_x + local_x * cos - local_y * sin,
            self.center_y + local_x * sin + local_y * cos,
        )
    }

    pub fn local_bounds(&self, u: f32, v: f32, width: f32, height: f32) -> (f32, f32, f32, f32) {
        let corners = [
            (u - width / 2.0, v - height / 2.0),
            (u + width / 2.0, v - height / 2.0),
            (u + width / 2.0, v + height / 2.0),
            (u - width / 2.0, v + height / 2.0),
        ]
        .map(|(cu, cv)| self.canvas_from_local(cu, cv));
        let left = corners.iter().map(|(x, _)| *x).fold(f32::MAX, f32::min);
        let right = corners.iter().map(|(x, _)| *x).fold(f32::MIN, f32::max);
        let top = corners.iter().map(|(_, y)| *y).fold(f32::MAX, f32::min);
        let bottom = corners.iter().map(|(_, y)| *y).fold(f32::MIN, f32::max);
        (left, top, right - left, bottom - top)
    }
}

#[derive(Clone, Debug)]
pub struct ComposedFrame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub skipped: Vec<String>,
    pub rects: Vec<(String, ElementRect)>,
}

impl ComposedFrame {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let offset = ((y * self.width + x) * 4) as usize;
        [
            self.pixels[offset],
            self.pixels[offset + 1],
            self.pixels[offset + 2],
            self.pixels[offset + 3],
        ]
    }
}

pub struct FrameComposer {
    context: GpuContext,
    compositor: Compositor,
    cache: DecodeCache,
    text: TextRasterizer,
    rasters: RasterCache,
    mattes: MatteCache,
    staging: StagingRing,
    budget: MemoryBudget,
    scratch: Scratch,
}

#[derive(Default)]
struct Scratch {
    mask_alpha: Vec<u8>,
    mask_rgba: Vec<u8>,
    upload: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CacheBytes {
    pub decoders: usize,
    pub rasters: usize,
    pub layer_textures: usize,
    pub render_pool: usize,
    pub live_decoders: usize,
    pub layer_texture_entries: usize,
    pub evictions: u64,
}

impl CacheBytes {
    pub fn total(&self) -> usize {
        self.decoders + self.rasters + self.layer_textures + self.render_pool
    }
}

const STAGING_RING_LENGTH: usize = 3;

struct StagingRing {
    buffers: Vec<wgpu::Buffer>,
    length: usize,
    size: u64,
    next: usize,
    allocations: u64,
    reuses: u64,
}

impl Default for StagingRing {
    fn default() -> Self {
        Self {
            buffers: Vec::new(),
            length: STAGING_RING_LENGTH,
            size: 0,
            next: 0,
            allocations: 0,
            reuses: 0,
        }
    }
}

impl StagingRing {
    fn set_length(&mut self, length: usize) {
        let length = length.max(1);
        if length != self.length {
            self.length = length;
            self.buffers.clear();
            self.next = 0;
        }
    }

    fn acquire(&mut self, device: &wgpu::Device, size: u64) -> wgpu::Buffer {
        if self.size != size {
            self.buffers.clear();
            self.next = 0;
            self.size = size;
        }
        if self.buffers.len() < self.length {
            self.buffers
                .push(device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("playback-readback"),
                    size,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }));
            self.allocations += 1;
        } else {
            self.reuses += 1;
        }
        let index = self.next % self.buffers.len();
        self.next = index + 1;
        self.buffers[index].clone()
    }
}

pub struct PendingFrame {
    width: u32,
    height: u32,
    padded: u32,
    buffer: wgpu::Buffer,
    submission: wgpu::SubmissionIndex,
    swap: bool,
    skipped: Vec<String>,
    rects: Vec<(String, ElementRect)>,
}

impl FrameComposer {
    pub fn new() -> Result<Self> {
        Self::with_budget(MemoryBudget::detect())
    }

    pub fn with_budget(budget: MemoryBudget) -> Result<Self> {
        let context = pollster::block_on(GpuContext::new())
            .map_err(|error| PlaybackError::Gpu(error.to_string()))?;
        let mut compositor = Compositor::new(&context);
        compositor.set_texture_budget(budget.layer_textures);
        compositor.set_render_pool_budget(budget.render_pool);
        Ok(Self {
            context,
            compositor,
            cache: DecodeCache::with_budget(budget),
            text: TextRasterizer::new(),
            rasters: RasterCache::with_budget(budget.rasters),
            mattes: MatteCache::default(),
            staging: StagingRing::default(),
            budget,
            scratch: Scratch::default(),
        })
    }

    pub fn budget(&self) -> MemoryBudget {
        self.budget
    }

    pub fn set_budget(&mut self, budget: MemoryBudget) {
        self.budget = budget;
        self.cache.set_budget(budget);
        self.rasters.set_budget(budget.rasters);
        self.compositor.set_texture_budget(budget.layer_textures);
        self.compositor.set_render_pool_budget(budget.render_pool);
    }

    pub fn cache_bytes(&self) -> CacheBytes {
        let (textures, texture_entries, texture_evictions) = self.compositor.texture_stats();
        let (pool_bytes, _, _, pool_drops) = self.compositor.render_pool_stats();
        CacheBytes {
            decoders: self.cache.resident_bytes(),
            rasters: self.rasters.resident_bytes(),
            layer_textures: textures,
            render_pool: pool_bytes,
            live_decoders: self.cache.live_decoders(),
            layer_texture_entries: texture_entries,
            evictions: self.cache.stats().evictions
                + self.rasters.evictions()
                + texture_evictions
                + pool_drops,
        }
    }

    pub fn release_media(&mut self, media_id: &str) {
        self.cache.forget(media_id);
    }

    pub fn clear_caches(&mut self) {
        self.cache.clear();
        self.rasters.clear();
        self.compositor.clear_textures();
    }

    pub fn staging_stats(&self) -> (u64, u64) {
        (self.staging.allocations, self.staging.reuses)
    }

    pub fn stats(&self) -> DecodeStats {
        self.cache.stats()
    }

    pub fn cache_mut(&mut self) -> &mut DecodeCache {
        &mut self.cache
    }

    pub fn seek_count(&self, media_id: &str) -> u64 {
        self.cache.seek_count(media_id)
    }

    pub fn set_matte_root(&mut self, root: Option<std::path::PathBuf>) {
        self.mattes.set_root(root);
    }

    pub fn matte_stats(&self) -> (u64, u64) {
        self.mattes.stats()
    }

    pub fn submit_frame(
        &mut self,
        request: &ComposeRequest<'_>,
        media: &dyn MediaResolver,
    ) -> Result<PendingFrame> {
        let scene = match request.scene_id {
            Some(id) => request
                .project
                .scenes
                .iter()
                .find(|scene| scene.id == id)
                .ok_or_else(|| PlaybackError::SceneNotFound(id.to_owned()))?,
            None => request
                .project
                .scenes
                .iter()
                .find(|scene| scene.id == request.project.current_scene_id)
                .or_else(|| request.project.scenes.first())
                .ok_or_else(|| PlaybackError::SceneNotFound("<current>".to_owned()))?,
        };

        self.cache.begin_frame();
        self.compositor.begin_frame();

        let mut skipped = Vec::new();
        let mut items = Vec::new();
        let mut rects: Vec<(String, ElementRect)> = Vec::new();
        let canvas_width = request.width as f64;
        let canvas_height = request.height as f64;

        let unit = if request.project.settings.canvas_size.width > 0 {
            canvas_width / request.project.settings.canvas_size.width as f64
        } else {
            1.0
        };

        if let Background::Blur { blur_intensity } = &request.project.settings.background {
            let intensity = *blur_intensity;
            for element in scene.tracks.main.elements() {
                if !is_visible(element, request.time) {
                    continue;
                }
                let Some(params) = visual_params(element) else {
                    continue;
                };
                if params.hidden {
                    continue;
                }
                let Some(path) = media.resolve(params.media_id) else {
                    continue;
                };
                let frame = if params.is_video {
                    let clip_time = request.time - element.base().start_time;
                    let offset = crate::retime::source_offset(params.retime, clip_time);
                    let source = (element.base().trim_start + offset).max(MediaTime::ZERO);
                    self.cache
                        .video_frame(params.media_id, &path, source.to_seconds_f64())
                } else {
                    self.cache.still_frame(params.media_id, &path)
                };
                let Ok(frame) = frame else {
                    continue;
                };
                let texture_id = format!("backdrop:{}", element.base().id);
                let texture = self.upload(&frame, &texture_id);
                self.compositor.upsert_texture(texture_id.clone(), texture);
                items.push(FrameItemDescriptor::Layer(LayerDescriptor {
                    texture_id,
                    transform: cover_quad(
                        frame.width as f64,
                        frame.height as f64,
                        canvas_width,
                        canvas_height,
                    ),
                    opacity: 1.0,
                    blend_mode: parse_blend_mode(None),
                    effect_pass_groups: vec![crate::effects_map::gaussian_blur_passes(
                        crate::effects_map::intensity_to_sigma(
                            intensity as f32,
                            request.width as f32,
                            1920.0,
                        ),
                        crate::effects_map::intensity_to_sigma(
                            intensity as f32,
                            request.height as f32,
                            1080.0,
                        ),
                    )],
                    mask: None,
                }));
            }
        }

        for track in scene.tracks.all() {
            if track_hidden(track) {
                continue;
            }
            let transition_edges = build_track_transition_edges(track);
            for element in track.elements() {
                let edges = transition_edges.get(element.base().id.as_str());
                if !is_visible_with_transition(element, edges, request.time) {
                    continue;
                }
                if let TimelineElement::Text(text) = element {
                    if !is_visible(element, request.time) {
                        continue;
                    }
                    if text.hidden.unwrap_or(false) {
                        continue;
                    }
                    let local = element_local_time(element, request.time);
                    let Some(layer) = self.text.rasterize(text, canvas_height, local) else {
                        continue;
                    };
                    let mut transform = text_transform(&text.transform, element, local);
                    transform.position_x *= unit;
                    transform.position_y *= unit;
                    let quad = bitmap_quad(&transform, &layer, canvas_width, canvas_height);
                    let opacity = crate::animation::scalar_at(
                        element.base().animations.as_ref(),
                        "opacity",
                        text.opacity,
                        local,
                    )
                    .clamp(0.0, 1.0);

                    let texture_id = format!("layer:{}", text.base.id);
                    let frame = SourceFrame {
                        width: layer.width,
                        height: layer.height,
                        rgba: layer.rgba.clone(),
                        timestamp: 0.0,
                    };
                    let texture = self.upload(&frame, &texture_id);
                    self.compositor.upsert_texture(texture_id.clone(), texture);
                    rects.push((text.base.id.clone(), rect_of(&quad)));
                    items.push(FrameItemDescriptor::Layer(LayerDescriptor {
                        texture_id,
                        transform: quad,
                        opacity: opacity as f32,
                        blend_mode: parse_blend_mode(text.blend_mode.as_deref()),
                        effect_pass_groups: effect_pass_groups(
                            text.effects.as_ref(),
                            element.base().animations.as_ref(),
                            local,
                            request.width,
                            request.height,
                        ),
                        mask: None,
                    }));
                    continue;
                }
                if let Some((source, params)) = raster_params(element) {
                    if params.hidden {
                        continue;
                    }
                    if !is_visible(element, request.time) {
                        continue;
                    }
                    let local = element_local_time(element, request.time);
                    let mut transform = resolved_transform(&params, element, local);
                    transform.position_x *= unit;
                    transform.position_y *= unit;
                    let (source_width, source_height) = raster_source_size(&source);
                    let contain =
                        contain_scale(source_width, source_height, canvas_width, canvas_height);
                    let raster_width = raster_extent(source_width * contain * transform.scale_x);
                    let raster_height = raster_extent(source_height * contain * transform.scale_y);

                    let raster = match &source {
                        RasterSource::Sticker { sticker_id, .. } => {
                            match self
                                .rasters
                                .sticker(sticker_id, raster_width, raster_height)
                            {
                                Ok(raster) => raster,
                                Err(error) => {
                                    skipped.push(format!("sticker:{error}"));
                                    continue;
                                }
                            }
                        }
                        RasterSource::Graphic {
                            definition_id,
                            params: shape_params,
                        } => {
                            let animated = animated_graphic_params(
                                shape_params,
                                element.base().animations.as_ref(),
                                local,
                            );
                            match self.rasters.graphic(
                                definition_id,
                                &animated,
                                raster_width,
                                raster_height,
                            ) {
                                Some(raster) => raster,
                                None => {
                                    skipped.push(format!("graphic:{definition_id}"));
                                    continue;
                                }
                            }
                        }
                    };

                    let frame = SourceFrame {
                        width: raster.width,
                        height: raster.height,
                        rgba: raster.rgba.clone(),
                        timestamp: 0.0,
                    };
                    let crop = resolved_crop(&params, element, local);
                    let opacity = resolved_opacity(&params, element, local);
                    let quad = quad_transform(
                        &transform,
                        crop,
                        source_width,
                        source_height,
                        canvas_width,
                        canvas_height,
                    );

                    let base = element.base();
                    let texture_id = format!("layer:{}", base.id);
                    let texture = self.upload(&frame, &texture_id);
                    self.compositor.upsert_texture(texture_id.clone(), texture);
                    let rect = rect_of(&quad);
                    let mask =
                        self.attach_mask(element, &quad, None, request.width, request.height);
                    let groups = effect_pass_groups(
                        params.effects,
                        base.animations.as_ref(),
                        local,
                        request.width,
                        request.height,
                    );
                    rects.push((base.id.clone(), rect));
                    self.push_background_blur(
                        element,
                        &quad,
                        None,
                        &texture_id,
                        opacity as f32,
                        parse_blend_mode(params.blend_mode),
                        &groups,
                        request,
                        &mut items,
                    );
                    items.push(FrameItemDescriptor::Layer(LayerDescriptor {
                        texture_id,
                        transform: quad.clone(),
                        opacity: opacity as f32,
                        blend_mode: parse_blend_mode(params.blend_mode),
                        effect_pass_groups: groups,
                        mask,
                    }));
                    if let Some(stroke) =
                        self.attach_mask_stroke(element, &quad, request.width, request.height)
                    {
                        items.push(stroke);
                    }
                    continue;
                }
                let Some(params) = visual_params(element) else {
                    skipped.push(element_kind(element).to_owned());
                    continue;
                };
                if params.hidden {
                    continue;
                }
                let Some(path) = media.resolve(params.media_id) else {
                    return Err(PlaybackError::MediaNotFound(params.media_id.to_owned()));
                };

                let base = element.base();
                let mut source_ticks = None;
                let frame = if params.is_video {
                    let clip_time = request.time - base.start_time;
                    let offset = crate::retime::source_offset(params.retime, clip_time);
                    let mut source = base.trim_start + offset;
                    if let Some(limit) = base.source_duration
                        && limit.as_ticks() > 0
                    {
                        source = source.min(limit - MediaTime::ONE_TICK);
                    }
                    let source = source.max(MediaTime::ZERO);
                    source_ticks = Some(source.as_ticks() as f64);
                    self.cache
                        .video_frame(params.media_id, &path, source.to_seconds_f64())?
                } else {
                    self.cache.still_frame(params.media_id, &path)?
                };

                let local = element_local_time(element, request.time);
                let mut transform = resolved_transform(&params, element, local);
                transform.position_x *= unit;
                transform.position_y *= unit;
                let crop = resolved_crop(&params, element, local);
                let opacity = resolved_opacity(&params, element, local);
                let quad = quad_transform(
                    &transform,
                    crop,
                    frame.width as f64,
                    frame.height as f64,
                    canvas_width,
                    canvas_height,
                );

                let (quad, opacity) = match resolve_active_transition(edges, request.time) {
                    Some(active) => {
                        let Some(state) = apply_transition_to_layer(
                            &quad,
                            opacity,
                            &active,
                            canvas_width,
                            canvas_height,
                        ) else {
                            continue;
                        };
                        (state.transform, state.opacity)
                    }
                    None => {
                        if !is_visible(element, request.time) {
                            continue;
                        }
                        (quad, opacity)
                    }
                };

                let texture_id = format!("layer:{}", base.id);
                let texture = self.upload(&frame, &texture_id);
                self.compositor.upsert_texture(texture_id.clone(), texture);

                let rect = rect_of(&quad);
                let mask =
                    self.attach_mask(element, &quad, source_ticks, request.width, request.height);

                let groups = effect_pass_groups(
                    params.effects,
                    element.base().animations.as_ref(),
                    local,
                    request.width,
                    request.height,
                );
                rects.push((base.id.clone(), rect));
                self.push_background_blur(
                    element,
                    &quad,
                    source_ticks,
                    &texture_id,
                    opacity as f32,
                    parse_blend_mode(params.blend_mode),
                    &groups,
                    request,
                    &mut items,
                );
                items.push(FrameItemDescriptor::Layer(LayerDescriptor {
                    texture_id,
                    transform: quad.clone(),
                    opacity: opacity as f32,
                    blend_mode: parse_blend_mode(params.blend_mode),
                    effect_pass_groups: groups,
                    mask,
                }));
                if let Some(stroke) =
                    self.attach_mask_stroke(element, &quad, request.width, request.height)
                {
                    items.push(stroke);
                }
            }
        }

        let scene_duration_ticks = scene
            .tracks
            .all()
            .flat_map(|track| track.elements())
            .map(|element| element.end_time().as_ticks())
            .fold(0i64, i64::max);
        self.push_watermark(request, media, scene_duration_ticks, &mut items);

        let clear = match &request.project.settings.background {
            Background::Color { color } => parse_to_linear_rgba(color)
                .map(|rgba| {
                    [
                        rgba[0] as f32,
                        rgba[1] as f32,
                        rgba[2] as f32,
                        rgba[3] as f32,
                    ]
                })
                .unwrap_or([0.0, 0.0, 0.0, 1.0]),

            Background::Blur { .. } => [0.0, 0.0, 0.0, 1.0],
        };

        let descriptor = FrameDescriptor {
            width: request.width,
            height: request.height,
            clear: CanvasClearDescriptor { color: clear },
            items,
        };

        let texture = self
            .compositor
            .render_frame_to_texture(&self.context, &descriptor)?;
        let pending = self.begin_readback(&texture, request.width, request.height);

        Ok(PendingFrame {
            skipped,
            rects,
            ..pending
        })
    }

    fn push_watermark(
        &mut self,
        request: &ComposeRequest<'_>,
        media: &dyn MediaResolver,
        scene_duration_ticks: i64,
        items: &mut Vec<FrameItemDescriptor>,
    ) {
        let Some(raw) = request.project.settings.watermark.as_ref() else {
            return;
        };
        let Ok(mark) = serde_json::from_value::<watermark::TWatermark>(raw.clone()) else {
            return;
        };
        if !mark.enabled {
            return;
        }
        let Some(source) = mark.source.as_ref() else {
            return;
        };

        let Some(window) = watermark::resolve_watermark_window(&mark.timing, scene_duration_ticks)
        else {
            return;
        };
        let local_time = (request.time.as_ticks() - window.time_offset) as f64;
        if local_time < 0.0 || local_time > window.duration as f64 {
            return;
        }
        let fade = watermark::compute_fade_opacity(
            local_time,
            window.duration as f64,
            window.fade_in as f64,
            window.fade_out as f64,
        );
        let opacity = (mark.opacity * fade).clamp(0.0, 1.0) as f32;
        if opacity <= 0.0 {
            return;
        }

        let (frame, texture_key) = match source {
            watermark::TWatermarkSource::Image { media_id } => {
                let Some(path) = media.resolve(media_id) else {
                    return;
                };
                let Ok(frame) = self.cache.still_frame(media_id, &path) else {
                    return;
                };
                (frame, format!("watermark-image:{media_id}"))
            }
            watermark::TWatermarkSource::Text {
                text,
                color,
                font_weight,
                stroke,
                shadow,
                font_family,
            } => {
                let Some(layer) = self.rasterize_watermark_text(
                    text,
                    color,
                    *font_weight,
                    font_family,
                    stroke,
                    shadow,
                    request.height as f64,
                ) else {
                    return;
                };
                (
                    SourceFrame {
                        width: layer.width,
                        height: layer.height,
                        rgba: layer.rgba,
                        timestamp: 0.0,
                    },
                    format!("watermark-text:{}", request.project.metadata.id),
                )
            }
        };

        if frame.width == 0 || frame.height == 0 {
            return;
        }

        let canvas = watermark::CanvasSize {
            width: request.width as f64,
            height: request.height as f64,
        };
        let source_size = watermark::SourceSize {
            width: frame.width as f64,
            height: frame.height as f64,
        };
        let rects = watermark::compute_watermark_rects(canvas, source_size, &mark);
        if rects.is_empty() {
            return;
        }

        let texture = self.upload(&frame, &texture_key);
        self.compositor.upsert_texture(texture_key.clone(), texture);
        let blend_mode = parse_blend_mode(Some(mark.blend_mode.key()));

        for rect in rects {
            if rect.width <= 0.0 || rect.height <= 0.0 {
                continue;
            }
            let quad = compositor::QuadTransformDescriptor {
                center_x: (rect.x + rect.width / 2.0) as f32,
                center_y: (rect.y + rect.height / 2.0) as f32,
                width: rect.width as f32,
                height: rect.height as f32,
                rotation_degrees: rect.rotation as f32,
                flip_x: false,
                flip_y: false,
                source_rect: compositor::SourceRectDescriptor::default(),
            };
            items.push(FrameItemDescriptor::Layer(LayerDescriptor {
                texture_id: texture_key.clone(),
                transform: quad,
                opacity,
                blend_mode,
                effect_pass_groups: Vec::new(),
                mask: None,
            }));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rasterize_watermark_text(
        &mut self,
        text: &str,
        color: &str,
        font_weight: f64,
        font_family: &str,
        stroke: &watermark::TextStroke,
        shadow: &watermark::TextShadow,
        canvas_height: f64,
    ) -> Option<crate::text_render::TextLayer> {
        if text.trim().is_empty() {
            return None;
        }

        const TARGET_GLYPH_PX: f64 = 96.0;
        let font_size = if canvas_height > 0.0 {
            TARGET_GLYPH_PX * crate::text_render::FONT_SIZE_SCALE_REFERENCE / canvas_height
        } else {
            TARGET_GLYPH_PX
        };
        let weight = if font_weight >= 600.0 {
            "bold"
        } else {
            "normal"
        };
        let document = serde_json::json!({
            "id": "watermark-text",
            "type": "text",
            "name": "watermark",
            "content": text,
            "startTime": 0,
            "duration": 1,
            "trimStart": 0,
            "trimEnd": 0,
            "fontSize": font_size,
            "fontFamily": font_family,
            "color": color,
            "fontWeight": weight,
            "fontStyle": "normal",
            "textAlign": "center",
            "textDecoration": "none",
            "opacity": 1.0,
            "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
            "background": { "enabled": false, "color": "#000000" },
            "stroke": {
                "enabled": stroke.enabled,
                "color": stroke.color,
                "width": stroke.width,
            },
            "shadow": {
                "enabled": shadow.enabled,
                "color": shadow.color,
                "blur": shadow.blur,
                "offsetX": shadow.offset_x,
                "offsetY": shadow.offset_y,
            },
        });
        let element: cutix_project::model::TextElement = serde_json::from_value(document).ok()?;
        self.text
            .rasterize(&element, canvas_height, MediaTime::ZERO)
    }

    fn attach_mask(
        &mut self,
        element: &TimelineElement,
        quad: &compositor::QuadTransformDescriptor,
        source_ticks: Option<f64>,
        canvas_width: u32,
        canvas_height: u32,
    ) -> Option<LayerMaskDescriptor> {
        self.attach_mask_with(
            element,
            quad,
            source_ticks,
            canvas_width,
            canvas_height,
            false,
        )
    }

    fn attach_mask_with(
        &mut self,
        element: &TimelineElement,
        quad: &compositor::QuadTransformDescriptor,
        source_ticks: Option<f64>,
        canvas_width: u32,
        canvas_height: u32,
        invert_cutout: bool,
    ) -> Option<LayerMaskDescriptor> {
        let entries = masks_of(element);
        let cutout = resolved_cutout(element);
        if (entries.is_empty() && cutout.is_none()) || canvas_width == 0 || canvas_height == 0 {
            return None;
        }

        let width = canvas_width as usize;
        let height = canvas_height as usize;
        let pixels = width * height;
        let mut combined = std::mem::take(&mut self.scratch.mask_alpha);
        combined.clear();
        combined.resize(pixels, 255u8);
        let mut feather: f32 = 0.0;
        let mut applied = false;

        let mapper = QuadMapper::new(quad);

        for entry in entries {
            let Some(shape) = masks::MaskShape::from_key(&entry.mask_type) else {
                continue;
            };
            let Some(mapper) = mapper.as_ref() else {
                continue;
            };
            let stored = mask_params(entry);
            let Some(alpha) = mask_alpha(shape, &stored, mapper, width, height) else {
                continue;
            };
            for (target, value) in combined.iter_mut().zip(alpha) {
                *target = ((*target as u16 * value as u16) / 255) as u8;
            }
            feather = feather.max(stored.feather as f32);
            applied = true;
        }

        if let Some(mut cutout) = cutout {
            if invert_cutout {
                cutout.invert = !cutout.invert;
            }
            if let Some(matte) =
                cutout_alpha(&cutout, source_ticks, quad, width, height, &mut self.mattes)
            {
                for (target, value) in combined.iter_mut().zip(matte) {
                    *target = ((*target as u16 * value as u16) / 255) as u8;
                }
                applied = true;
            }
        }

        if !applied {
            self.scratch.mask_alpha = combined;
            return None;
        }

        let mut rgba = std::mem::take(&mut self.scratch.mask_rgba);
        rgba.clear();
        rgba.reserve(pixels * 4);
        for value in &combined {
            rgba.extend_from_slice(&[*value, *value, *value, *value]);
        }
        self.scratch.mask_alpha = combined;

        let texture_id = if invert_cutout {
            format!("mask:{}:inverted", element.base().id)
        } else {
            format!("mask:{}", element.base().id)
        };
        let mut frame = SourceFrame {
            width: canvas_width,
            height: canvas_height,
            rgba,
            timestamp: 0.0,
        };
        let texture = self.upload(&frame, &texture_id);
        self.scratch.mask_rgba = std::mem::take(&mut frame.rgba);
        self.compositor.upsert_texture(texture_id.clone(), texture);

        Some(LayerMaskDescriptor {
            texture_id,
            feather,
            inverted: false,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn push_background_blur(
        &mut self,
        element: &TimelineElement,
        quad: &compositor::QuadTransformDescriptor,
        source_ticks: Option<f64>,
        texture_id: &str,
        opacity: f32,
        blend_mode: compositor::BlendMode,
        groups: &[Vec<compositor::EffectPassDescriptor>],
        request: &ComposeRequest<'_>,
        items: &mut Vec<FrameItemDescriptor>,
    ) {
        let passes = background_blur_passes(element, request.width, request.height);
        if passes.is_empty() || resolved_cutout(element).is_none() {
            return;
        }
        let Some(mask) = self.attach_mask_with(
            element,
            quad,
            source_ticks,
            request.width,
            request.height,
            true,
        ) else {
            return;
        };
        let mut background: Vec<Vec<compositor::EffectPassDescriptor>> = groups.to_vec();
        background.push(passes);
        items.push(FrameItemDescriptor::Layer(LayerDescriptor {
            texture_id: texture_id.to_owned(),
            transform: quad.clone(),
            opacity,
            blend_mode,
            effect_pass_groups: background,
            mask: Some(mask),
        }));
    }

    fn attach_mask_stroke(
        &mut self,
        element: &TimelineElement,
        quad: &compositor::QuadTransformDescriptor,
        canvas_width: u32,
        canvas_height: u32,
    ) -> Option<FrameItemDescriptor> {
        if canvas_width == 0 || canvas_height == 0 {
            return None;
        }
        let entry = masks_of(element).first()?;
        let shape = masks::MaskShape::from_key(&entry.mask_type)?;
        let stroke = mask_stroke(entry)?;
        let mapper = QuadMapper::new(quad)?;
        let stored = mask_params(entry);

        let width = canvas_width as usize;
        let height = canvas_height as usize;
        let (grid_width, grid_height) = mapper.source_grid();
        let params = source_space_params(&stored, &mapper);

        let band = masks::shapes::stroke_alpha(
            shape,
            &params,
            stroke.width,
            stroke.align,
            grid_width,
            grid_height,
        );
        if band.iter().all(|value| *value == 0) {
            return None;
        }

        let [red, green, blue, alpha] = stroke.color;
        let mut rgba = std::mem::take(&mut self.scratch.mask_rgba);
        rgba.clear();
        rgba.resize(width * height * 4, 0u8);
        for y in 0..height {
            for x in 0..width {
                let Some((source_u, source_v)) = mapper.source_uv(x, y) else {
                    continue;
                };
                let sample_x = ((source_u * grid_width as f64) as isize)
                    .clamp(0, grid_width as isize - 1) as usize;
                let sample_y = ((source_v * grid_height as f64) as isize)
                    .clamp(0, grid_height as isize - 1) as usize;
                let coverage = band[sample_y * grid_width + sample_x];
                if coverage == 0 {
                    continue;
                }
                let out = (y * width + x) * 4;
                let combined = ((coverage as u16 * alpha as u16) / 255) as u8;
                rgba[out] = red;
                rgba[out + 1] = green;
                rgba[out + 2] = blue;
                rgba[out + 3] = combined;
            }
        }

        let texture_id = format!("mask-stroke:{}", element.base().id);
        let mut frame = SourceFrame {
            width: canvas_width,
            height: canvas_height,
            rgba,
            timestamp: 0.0,
        };
        let texture = self.upload(&frame, &texture_id);
        self.scratch.mask_rgba = std::mem::take(&mut frame.rgba);
        self.compositor.upsert_texture(texture_id.clone(), texture);

        Some(FrameItemDescriptor::Layer(LayerDescriptor {
            texture_id,
            transform: full_canvas_quad(canvas_width, canvas_height),
            opacity: 1.0,
            blend_mode: parse_blend_mode(None),
            effect_pass_groups: Vec::new(),
            mask: None,
        }))
    }

    fn upload(&mut self, frame: &SourceFrame, label: &str) -> wgpu::Texture {
        let format = self.context.texture_format();
        let texture = self
            .context
            .device()
            .create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: frame.width.max(1),
                    height: frame.height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });

        let data: &[u8] = if format == wgpu::TextureFormat::Bgra8Unorm {
            self.scratch.upload.clear();
            self.scratch.upload.extend_from_slice(&frame.rgba);
            for pixel in self.scratch.upload.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            &self.scratch.upload
        } else {
            &frame.rgba
        };

        self.context.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(frame.width.max(1) * 4),
                rows_per_image: Some(frame.height.max(1)),
            },
            wgpu::Extent3d {
                width: frame.width.max(1),
                height: frame.height.max(1),
                depth_or_array_layers: 1,
            },
        );
        texture
    }

    fn begin_readback(&mut self, texture: &wgpu::Texture, width: u32, height: u32) -> PendingFrame {
        let Self {
            context, staging, ..
        } = self;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let unpadded = width * 4;
        let padded = unpadded.div_ceil(align) * align;
        let buffer = staging.acquire(context.device(), (padded as u64) * (height as u64));
        let mut encoder =
            context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("playback-readback-encoder"),
                });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let submission = context.queue().submit([encoder.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});

        PendingFrame {
            width,
            height,
            padded,
            buffer,
            submission,
            swap: context.texture_format() == wgpu::TextureFormat::Bgra8Unorm,
            skipped: Vec::new(),
            rects: Vec::new(),
        }
    }

    pub fn resolve(&self, pending: PendingFrame) -> ComposedFrame {
        let PendingFrame {
            width,
            height,
            padded,
            buffer,
            submission,
            swap,
            skipped,
            rects,
        } = pending;

        let _ = self.context.device().poll(wgpu::PollType::Wait {
            submission_index: Some(submission),
            timeout: None,
        });

        let unpadded = (width * 4) as usize;
        let slice = buffer.slice(..);
        let mapped = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity(unpadded * height as usize);
        for row in 0..height as usize {
            let start = row * padded as usize;
            pixels.extend_from_slice(&mapped[start..start + unpadded]);
        }
        drop(mapped);
        buffer.unmap();

        if swap {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }

        ComposedFrame {
            width,
            height,
            pixels,
            skipped,
            rects,
        }
    }

    pub fn compose(
        &mut self,
        request: &ComposeRequest<'_>,
        media: &dyn MediaResolver,
    ) -> Result<ComposedFrame> {
        let pending = self.submit_frame(request, media)?;
        Ok(self.resolve(pending))
    }

    pub fn set_pipeline_depth(&mut self, depth: usize) {
        self.staging.set_length(depth + 1);
    }
}

const MAX_RASTER_EXTENT: u32 = 4096;

fn raster_extent(value: f64) -> u32 {
    let extent = value.abs().round();
    if !extent.is_finite() || extent < 1.0 {
        return 1;
    }
    (extent as u32).min(MAX_RASTER_EXTENT)
}

fn raster_source_size(source: &RasterSource<'_>) -> (f64, f64) {
    match source {
        RasterSource::Sticker {
            sticker_id,
            intrinsic_width,
            intrinsic_height,
        } => {
            let intrinsic = match (intrinsic_width, intrinsic_height) {
                (Some(width), Some(height)) if *width > 0.0 && *height > 0.0 => {
                    Some((*width, *height))
                }
                _ => None,
            };
            intrinsic
                .or_else(|| stickers::sticker_intrinsic_size(sticker_id))
                .unwrap_or((
                    stickers::DEFAULT_INTRINSIC_SIZE,
                    stickers::DEFAULT_INTRINSIC_SIZE,
                ))
        }
        RasterSource::Graphic { .. } => (
            stickers::DEFAULT_GRAPHIC_SOURCE_SIZE as f64,
            stickers::DEFAULT_GRAPHIC_SOURCE_SIZE as f64,
        ),
    }
}

fn animated_graphic_params(
    params: &cutix_project::model::ParamValues,
    animations: Option<&cutix_project::model::ElementAnimations>,
    local: MediaTime,
) -> cutix_project::model::ParamValues {
    let mut resolved = params.clone();
    for (key, value) in params {
        let Some(number) = value.as_f64() else {
            continue;
        };
        let channel = format!("params.{key}");
        let animated = crate::animation::scalar_at(animations, &channel, number, local);
        if animated != number {
            resolved.insert(key.clone(), serde_json::Value::from(animated));
        }
    }
    resolved
}

fn rect_of(quad: &compositor::QuadTransformDescriptor) -> ElementRect {
    ElementRect {
        center_x: quad.center_x,
        center_y: quad.center_y,
        width: quad.width,
        height: quad.height,
        rotation_degrees: quad.rotation_degrees,
    }
}

fn element_kind(element: &TimelineElement) -> &'static str {
    match element {
        TimelineElement::Video(_) => "video",
        TimelineElement::Image(_) => "image",
        TimelineElement::Audio(_) => "audio",
        TimelineElement::Text(_) => "text",
        TimelineElement::Sticker(_) => "sticker",
        TimelineElement::Graphic(_) => "graphic",
        TimelineElement::Effect(_) => "effect",
    }
}

fn resolved_cutout(element: &TimelineElement) -> Option<ml::ElementCutout> {
    let raw = match element {
        TimelineElement::Video(inner) => inner.cutout.as_ref(),
        TimelineElement::Image(inner) => inner.cutout.as_ref(),
        _ => None,
    }?;
    let cutout: ml::ElementCutout = serde_json::from_value(raw.clone()).ok()?;
    cutout.enabled.then_some(cutout)
}

struct QuadMapper {
    cos: f64,
    sin: f64,
    center_x: f64,
    center_y: f64,
    half_width: f64,
    half_height: f64,
    flip_x: bool,
    flip_y: bool,
    source_x: f64,
    source_y: f64,
    source_width: f64,
    source_height: f64,
}

impl QuadMapper {
    fn new(quad: &compositor::QuadTransformDescriptor) -> Option<Self> {
        let half_width = (quad.width as f64) / 2.0;
        let half_height = (quad.height as f64) / 2.0;
        if half_width.abs() < f64::EPSILON || half_height.abs() < f64::EPSILON {
            return None;
        }
        let radians = (quad.rotation_degrees as f64).to_radians();
        let source = &quad.source_rect;
        if !(source.width as f64).is_finite() || source.width.abs() < f32::EPSILON {
            return None;
        }
        if !(source.height as f64).is_finite() || source.height.abs() < f32::EPSILON {
            return None;
        }
        Some(Self {
            cos: radians.cos(),
            sin: radians.sin(),
            center_x: quad.center_x as f64,
            center_y: quad.center_y as f64,
            half_width,
            half_height,
            flip_x: quad.flip_x,
            flip_y: quad.flip_y,
            source_x: source.x as f64,
            source_y: source.y as f64,
            source_width: source.width as f64,
            source_height: source.height as f64,
        })
    }

    fn source_uv(&self, x: usize, y: usize) -> Option<(f64, f64)> {
        let dx = x as f64 + 0.5 - self.center_x;
        let dy = y as f64 + 0.5 - self.center_y;
        let local_x = dx * self.cos + dy * self.sin;
        let local_y = -dx * self.sin + dy * self.cos;
        let mut u = local_x / (self.half_width * 2.0) + 0.5;
        let mut v = local_y / (self.half_height * 2.0) + 0.5;
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return None;
        }
        if self.flip_x {
            u = 1.0 - u;
        }
        if self.flip_y {
            v = 1.0 - v;
        }
        Some((
            self.source_x + u * self.source_width,
            self.source_y + v * self.source_height,
        ))
    }

    fn source_grid(&self) -> (usize, usize) {
        let width = (self.half_width * 2.0 / self.source_width).abs().round();
        let height = (self.half_height * 2.0 / self.source_height).abs().round();
        (
            (width as usize).clamp(1, MAX_SOURCE_GRID),
            (height as usize).clamp(1, MAX_SOURCE_GRID),
        )
    }
}

const MAX_SOURCE_GRID: usize = 4096;

fn mask_alpha(
    shape: masks::MaskShape,
    stored: &masks::MaskParams,
    mapper: &QuadMapper,
    canvas_width: usize,
    canvas_height: usize,
) -> Option<Vec<u8>> {
    let (grid_width, grid_height) = mapper.source_grid();
    let params = source_space_params(stored, mapper);

    let grid = masks::shapes::rasterize(shape, &params, grid_width, grid_height);
    if grid.len() != grid_width * grid_height {
        return None;
    }

    let mut out = vec![0u8; canvas_width * canvas_height];
    for y in 0..canvas_height {
        for x in 0..canvas_width {
            let Some((source_u, source_v)) = mapper.source_uv(x, y) else {
                continue;
            };
            let sample_x = ((source_u * grid_width as f64) as isize)
                .clamp(0, grid_width as isize - 1) as usize;
            let sample_y = ((source_v * grid_height as f64) as isize)
                .clamp(0, grid_height as isize - 1) as usize;
            let value = grid[sample_y * grid_width + sample_x];
            out[y * canvas_width + x] = if stored.inverted { 255 - value } else { value };
        }
    }

    Some(out)
}

fn source_space_params(stored: &masks::MaskParams, mapper: &QuadMapper) -> masks::MaskParams {
    masks::MaskParams {
        center_x: mapper.source_x + (0.5 + stored.center_x) * mapper.source_width - 0.5,
        center_y: mapper.source_y + (0.5 + stored.center_y) * mapper.source_height - 0.5,
        width: stored.width * mapper.source_width,
        height: stored.height * mapper.source_height,
        rotation: stored.rotation,
        feather: stored.feather,
        inverted: stored.inverted,
    }
}

fn cover_quad(
    source_width: f64,
    source_height: f64,
    canvas_width: f64,
    canvas_height: f64,
) -> compositor::QuadTransformDescriptor {
    let scale = if source_width > 0.0 && source_height > 0.0 {
        (canvas_width / source_width).max(canvas_height / source_height)
    } else {
        1.0
    };
    compositor::QuadTransformDescriptor {
        center_x: canvas_width as f32 / 2.0,
        center_y: canvas_height as f32 / 2.0,
        width: (source_width * scale) as f32,
        height: (source_height * scale) as f32,
        rotation_degrees: 0.0,
        flip_x: false,
        flip_y: false,
        source_rect: compositor::SourceRectDescriptor::default(),
    }
}

fn full_canvas_quad(width: u32, height: u32) -> compositor::QuadTransformDescriptor {
    compositor::QuadTransformDescriptor {
        center_x: width as f32 / 2.0,
        center_y: height as f32 / 2.0,
        width: width as f32,
        height: height as f32,
        rotation_degrees: 0.0,
        flip_x: false,
        flip_y: false,
        source_rect: compositor::SourceRectDescriptor::default(),
    }
}

struct MaskStroke {
    width: f64,
    align: masks::StrokeAlign,
    color: [u8; 4],
}

fn mask_stroke(mask: &cutix_project::model::Mask) -> Option<MaskStroke> {
    let width = mask_number(&mask.params, "strokeWidth", 0.0);
    if width <= 0.0 {
        return None;
    }
    let color = mask
        .params
        .get("strokeColor")
        .and_then(serde_json::Value::as_str)
        .and_then(parse_to_linear_rgba)
        .map(|rgba| {
            [
                srgb_byte(rgba[0]),
                srgb_byte(rgba[1]),
                srgb_byte(rgba[2]),
                (rgba[3].clamp(0.0, 1.0) * 255.0).round() as u8,
            ]
        })
        .unwrap_or([255, 255, 255, 255]);
    if color[3] == 0 {
        return None;
    }
    let align = mask
        .params
        .get("strokeAlign")
        .and_then(serde_json::Value::as_str)
        .map(masks::StrokeAlign::from_key)
        .unwrap_or_default();
    Some(MaskStroke {
        width,
        align,
        color,
    })
}

fn srgb_byte(linear: f64) -> u8 {
    let value = linear.clamp(0.0, 1.0);
    let encoded = if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055 * value.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
}

fn background_blur_passes(
    element: &TimelineElement,
    width: u32,
    height: u32,
) -> Vec<compositor::EffectPassDescriptor> {
    let effects = match element {
        TimelineElement::Video(inner) => inner.effects.as_ref(),
        TimelineElement::Image(inner) => inner.effects.as_ref(),
        TimelineElement::Graphic(inner) => inner.effects.as_ref(),
        _ => None,
    };
    let Some(effects) = effects else {
        return Vec::new();
    };
    let Some(effect) = effects
        .iter()
        .find(|effect| effect.enabled && effect.effect_type == "background-blur")
    else {
        return Vec::new();
    };
    let strength = effect
        .params
        .get("strength")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(0.0)
        .clamp(0.0, 100.0) as f32;
    if strength <= 0.0 {
        return Vec::new();
    }
    crate::effects_map::gaussian_blur_passes(
        crate::effects_map::intensity_to_sigma(strength, width as f32, 1920.0),
        crate::effects_map::intensity_to_sigma(strength, height as f32, 1080.0),
    )
}

struct DecodedMatte {
    width: u32,
    height: u32,
    alpha: Vec<u8>,
}

#[derive(Default)]
pub struct MatteCache {
    entries: Vec<(u64, Arc<DecodedMatte>)>,
    hits: u64,
    misses: u64,
    root: Option<std::path::PathBuf>,
}

const MATTE_CACHE_CAPACITY: usize = 32;

impl MatteCache {
    pub fn set_root(&mut self, root: Option<std::path::PathBuf>) {
        if self.root != root {
            self.entries.clear();
        }
        self.root = root;
    }

    fn key(source: ml::MatteSource<'_>) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        match source {
            ml::MatteSource::Inline(text) => {
                0u8.hash(&mut hasher);
                text.len().hash(&mut hasher);
                text.hash(&mut hasher);
            }
            ml::MatteSource::File(path) => {
                1u8.hash(&mut hasher);
                path.hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    fn bytes_of(&self, source: ml::MatteSource<'_>) -> Option<Vec<u8>> {
        match source {
            ml::MatteSource::Inline(text) => ml::base64_decode(text),
            ml::MatteSource::File(relative) => {
                let root = self.root.as_ref()?;
                std::fs::read(root.join(relative)).ok()
            }
        }
    }

    fn decode(&mut self, source: ml::MatteSource<'_>) -> Option<Arc<DecodedMatte>> {
        let key = Self::key(source);
        if let Some(index) = self.entries.iter().position(|entry| entry.0 == key) {
            let entry = self.entries.remove(index);
            let matte = Arc::clone(&entry.1);
            self.entries.push(entry);
            self.hits += 1;
            return Some(matte);
        }

        self.misses += 1;
        let bytes = self.bytes_of(source)?;
        let (width, height, alpha) = ml::decode_matte_png(&bytes).ok()?;
        let matte = Arc::new(DecodedMatte {
            width,
            height,
            alpha,
        });
        self.entries.push((key, Arc::clone(&matte)));
        if self.entries.len() > MATTE_CACHE_CAPACITY {
            self.entries.remove(0);
        }
        Some(matte)
    }

    pub fn stats(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }
}

fn cutout_alpha(
    cutout: &ml::ElementCutout,
    source_ticks: Option<f64>,
    quad: &compositor::QuadTransformDescriptor,
    canvas_width: usize,
    canvas_height: usize,
    cache: &mut MatteCache,
) -> Option<Vec<u8>> {
    let decoded = cache.decode(cutout.matte_for(source_ticks))?;
    let matte_width = decoded.width;
    let matte_height = decoded.height;
    let matte = &decoded.alpha;
    if matte_width == 0 || matte_height == 0 {
        return None;
    }
    let mapper = QuadMapper::new(quad)?;

    let mut out = vec![0u8; canvas_width * canvas_height];
    for y in 0..canvas_height {
        for x in 0..canvas_width {
            let Some((source_u, source_v)) = mapper.source_uv(x, y) else {
                continue;
            };
            let sample_x = ((source_u * matte_width as f64) as isize)
                .clamp(0, matte_width as isize - 1) as usize;
            let sample_y = ((source_v * matte_height as f64) as isize)
                .clamp(0, matte_height as isize - 1) as usize;
            let value = matte[sample_y * matte_width as usize + sample_x];
            out[y * canvas_width + x] = if cutout.invert { 255 - value } else { value };
        }
    }

    Some(out)
}

fn masks_of(element: &TimelineElement) -> &[cutix_project::model::Mask] {
    let masks = match element {
        TimelineElement::Video(inner) => inner.masks.as_ref(),
        TimelineElement::Image(inner) => inner.masks.as_ref(),
        TimelineElement::Graphic(inner) => inner.masks.as_ref(),
        _ => None,
    };
    masks.map(Vec::as_slice).unwrap_or(&[])
}

fn mask_number(params: &cutix_project::model::ParamValues, key: &str, fallback: f64) -> f64 {
    params
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .filter(|value| value.is_finite())
        .unwrap_or(fallback)
}

fn mask_params(mask: &cutix_project::model::Mask) -> masks::MaskParams {
    let defaults = masks::MaskParams::default();
    masks::MaskParams {
        center_x: mask_number(&mask.params, "centerX", defaults.center_x),
        center_y: mask_number(&mask.params, "centerY", defaults.center_y),
        width: mask_number(&mask.params, "width", defaults.width),
        height: mask_number(&mask.params, "height", defaults.height),
        rotation: mask_number(&mask.params, "rotation", defaults.rotation),
        feather: mask_number(&mask.params, "feather", defaults.feather).max(0.0),
        inverted: mask
            .params
            .get("inverted")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
    }
}

#[cfg(test)]
mod cutout_tests {
    use super::*;
    use compositor::{QuadTransformDescriptor, SourceRectDescriptor};

    fn split_matte() -> ml::ElementCutout {
        let width = 8usize;
        let height = 8usize;
        let alpha: Vec<u8> = (0..width * height)
            .map(|index| if index % width < width / 2 { 0 } else { 255 })
            .collect();
        let png = ml::encode_matte_png(&alpha, width as u32, height as u32).expect("png");
        ml::ElementCutout {
            enabled: true,
            mode: ml::CutoutMode::Static,
            width: width as u32,
            height: height as u32,
            png: ml::base64_encode(&png),
            png_path: None,
            invert: false,
            reference_time: 0.0,
            coverage: 0.5,
            frames: None,
            sample_interval: None,
        }
    }

    fn quad(rotation: f32, flip_x: bool, source: SourceRectDescriptor) -> QuadTransformDescriptor {
        QuadTransformDescriptor {
            center_x: 50.0,
            center_y: 50.0,
            width: 40.0,
            height: 40.0,
            rotation_degrees: rotation,
            flip_x,
            flip_y: false,
            source_rect: source,
        }
    }

    #[test]
    fn the_matte_lands_inside_the_element_rect_and_nowhere_else() {
        let alpha = cutout_alpha(
            &split_matte(),
            None,
            &quad(0.0, false, SourceRectDescriptor::default()),
            100,
            100,
            &mut MatteCache::default(),
        )
        .expect("alpha");

        assert_eq!(alpha[50 * 100 + 40], 0);
        assert_eq!(alpha[50 * 100 + 60], 255);

        assert_eq!(alpha[50 * 100 + 5], 0);
        assert_eq!(alpha[5 * 100 + 50], 0);
    }

    #[test]
    fn inverting_swaps_which_half_survives() {
        let mut cutout = split_matte();
        cutout.invert = true;
        let alpha = cutout_alpha(
            &cutout,
            None,
            &quad(0.0, false, SourceRectDescriptor::default()),
            100,
            100,
            &mut MatteCache::default(),
        )
        .expect("alpha");
        assert_eq!(alpha[50 * 100 + 40], 255);
        assert_eq!(alpha[50 * 100 + 60], 0);
    }

    #[test]
    fn a_ninety_degree_rotation_turns_the_split_from_vertical_to_horizontal() {
        let alpha = cutout_alpha(
            &split_matte(),
            None,
            &quad(90.0, false, SourceRectDescriptor::default()),
            100,
            100,
            &mut MatteCache::default(),
        )
        .expect("alpha");
        assert_eq!(alpha[60 * 100 + 50], 255);
        assert_eq!(alpha[40 * 100 + 50], 0);
    }

    #[test]
    fn a_horizontal_flip_mirrors_the_matte() {
        let alpha = cutout_alpha(
            &split_matte(),
            None,
            &quad(0.0, true, SourceRectDescriptor::default()),
            100,
            100,
            &mut MatteCache::default(),
        )
        .expect("alpha");
        assert_eq!(alpha[50 * 100 + 40], 255);
        assert_eq!(alpha[50 * 100 + 60], 0);
    }

    #[test]
    fn a_crop_selects_the_matching_slice_of_the_matte() {
        let source = SourceRectDescriptor {
            x: 0.5,
            y: 0.0,
            width: 0.5,
            height: 1.0,
        };
        let alpha = cutout_alpha(
            &split_matte(),
            None,
            &quad(0.0, false, source),
            100,
            100,
            &mut MatteCache::default(),
        )
        .expect("alpha");

        assert_eq!(alpha[50 * 100 + 35], 255);
        assert_eq!(alpha[50 * 100 + 65], 255);
    }

    #[test]
    fn the_matte_is_decoded_once_and_then_served_from_the_cache() {
        let cutout = split_matte();
        let quad = quad(0.0, false, SourceRectDescriptor::default());
        let mut cache = MatteCache::default();
        let first = cutout_alpha(&cutout, None, &quad, 100, 100, &mut cache).expect("alpha");
        assert_eq!(cache.stats(), (0, 1));
        for _ in 0..29 {
            let again = cutout_alpha(&cutout, None, &quad, 100, 100, &mut cache).expect("alpha");
            assert_eq!(again, first);
        }
        assert_eq!(cache.stats(), (29, 1));
    }

    #[test]
    fn a_disabled_cutout_is_ignored() {
        let mut cutout = split_matte();
        cutout.enabled = false;
        let mut document = serde_json::json!({
            "id": "e1",
            "type": "image",
            "name": "image",
            "mediaId": "m",
            "startTime": 0,
            "duration": 120000,
            "trimStart": 0,
            "trimEnd": 0,
            "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
            "opacity": 1.0,
        });
        document["cutout"] = serde_json::to_value(&cutout).expect("json");
        let element: TimelineElement = serde_json::from_value(document.clone()).expect("element");
        assert!(resolved_cutout(&element).is_none());

        cutout.enabled = true;
        document["cutout"] = serde_json::to_value(&cutout).expect("json");
        let element: TimelineElement = serde_json::from_value(document).expect("element");
        assert!(resolved_cutout(&element).is_some());
    }
}

#[cfg(test)]
mod mask_tests {
    use super::*;
    use compositor::{QuadTransformDescriptor, SourceRectDescriptor};

    const CANVAS: usize = 100;

    fn quad(
        size: f32,
        rotation: f32,
        flip_x: bool,
        flip_y: bool,
        source: SourceRectDescriptor,
    ) -> QuadTransformDescriptor {
        QuadTransformDescriptor {
            center_x: 50.0,
            center_y: 50.0,
            width: size,
            height: size,
            rotation_degrees: rotation,
            flip_x,
            flip_y,
            source_rect: source,
        }
    }

    fn params(center_x: f64, width: f64, inverted: bool) -> masks::MaskParams {
        masks::MaskParams {
            center_x,
            center_y: 0.0,
            width,
            height: width,
            rotation: 0.0,
            feather: 0.0,
            inverted,
        }
    }

    fn alpha_for(quad: &QuadTransformDescriptor, params: &masks::MaskParams) -> Vec<u8> {
        let mapper = QuadMapper::new(quad).expect("mapper");
        mask_alpha(masks::MaskShape::Rectangle, params, &mapper, CANVAS, CANVAS).expect("alpha")
    }

    fn centroid(alpha: &[u8]) -> (f64, f64) {
        let mut weight = 0.0;
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        for y in 0..CANVAS {
            for x in 0..CANVAS {
                let value = alpha[y * CANVAS + x] as f64 / 255.0;
                weight += value;
                sum_x += value * (x as f64 + 0.5);
                sum_y += value * (y as f64 + 0.5);
            }
        }
        assert!(weight > 0.0, "mask kept nothing");
        (sum_x / weight, sum_y / weight)
    }

    fn kept(alpha: &[u8]) -> f64 {
        alpha.iter().map(|value| *value as f64).sum::<f64>() / 255.0
    }

    #[test]
    fn a_half_sized_rectangle_keeps_the_middle_of_the_quad() {
        let alpha = alpha_for(
            &quad(40.0, 0.0, false, false, SourceRectDescriptor::default()),
            &params(0.0, 0.5, false),
        );
        assert_eq!(alpha[50 * CANVAS + 50], 255, "centre");
        assert_eq!(
            alpha[50 * CANVAS + 35],
            0,
            "inside the quad but outside the mask"
        );
        assert_eq!(alpha[50 * CANVAS + 5], 0, "outside the quad");
        assert!((kept(&alpha) - 400.0).abs() < 2.0, "{}", kept(&alpha));
    }

    #[test]
    fn an_inverted_mask_keeps_exactly_the_complement_inside_the_quad() {
        let quad = quad(40.0, 0.0, false, false, SourceRectDescriptor::default());
        let plain = alpha_for(&quad, &params(0.0, 0.5, false));
        let inverted = alpha_for(&quad, &params(0.0, 0.5, true));

        let mapper = QuadMapper::new(&quad).expect("mapper");
        let mut inside = 0;
        for y in 0..CANVAS {
            for x in 0..CANVAS {
                let index = y * CANVAS + x;
                if mapper.source_uv(x, y).is_none() {
                    assert_eq!(inverted[index], 0, "outside the quad stays cut");
                    continue;
                }
                inside += 1;
                assert_eq!(
                    plain[index] as u16 + inverted[index] as u16,
                    255,
                    "pixel {x},{y}"
                );
            }
        }
        assert_eq!(inside, 40 * 40);
        assert!((kept(&plain) + kept(&inverted) - 1600.0).abs() < 2.0);
    }

    #[test]
    fn inverting_one_of_two_masks_leaves_the_other_alone() {
        let quad = quad(40.0, 0.0, false, false, SourceRectDescriptor::default());
        let outer = alpha_for(&quad, &params(0.0, 0.8, false));
        let hole = alpha_for(&quad, &params(0.0, 0.4, true));

        let mut combined = vec![255u8; CANVAS * CANVAS];
        for (target, value) in combined.iter_mut().zip(outer.iter().zip(hole.iter())) {
            *target = ((*value.0 as u16 * *value.1 as u16) / 255) as u8;
        }
        assert_eq!(combined[50 * CANVAS + 50], 0, "hole");
        assert_eq!(combined[50 * CANVAS + 38], 255, "ring");
        assert_eq!(combined[50 * CANVAS + 31], 0, "outside the big mask");
        let expected = (0.8 * 40.0f64).powi(2) - (0.4 * 40.0f64).powi(2);
        assert!(
            (kept(&combined) - expected).abs() < 4.0,
            "{}",
            kept(&combined)
        );
    }

    #[test]
    fn a_mask_stays_on_the_same_fraction_of_the_layer_when_it_scales() {
        let small = alpha_for(
            &quad(40.0, 0.0, false, false, SourceRectDescriptor::default()),
            &params(0.25, 0.4, false),
        );
        let large = alpha_for(
            &quad(80.0, 0.0, false, false, SourceRectDescriptor::default()),
            &params(0.25, 0.4, false),
        );

        let (small_x, small_y) = centroid(&small);
        let (large_x, large_y) = centroid(&large);
        assert!((small_x - 60.0).abs() < 0.6, "{small_x}");
        assert!((large_x - 70.0).abs() < 0.6, "{large_x}");
        assert!((small_y - 50.0).abs() < 0.6 && (large_y - 50.0).abs() < 0.6);
        assert!((kept(&large) / kept(&small) - 4.0).abs() < 0.1);
    }

    #[test]
    fn a_rotated_layer_rotates_its_mask_with_it() {
        let upright = alpha_for(
            &quad(40.0, 0.0, false, false, SourceRectDescriptor::default()),
            &params(0.25, 0.4, false),
        );
        let turned = alpha_for(
            &quad(40.0, 90.0, false, false, SourceRectDescriptor::default()),
            &params(0.25, 0.4, false),
        );

        let (x0, y0) = centroid(&upright);
        let (x1, y1) = centroid(&turned);
        assert!(
            (x0 - 60.0).abs() < 0.6 && (y0 - 50.0).abs() < 0.6,
            "{x0},{y0}"
        );
        assert!(
            (x1 - 50.0).abs() < 0.6 && (y1 - 60.0).abs() < 0.6,
            "{x1},{y1}"
        );
        assert!((kept(&turned) - kept(&upright)).abs() < 8.0);
    }

    #[test]
    fn a_flipped_layer_mirrors_its_mask() {
        let plain = alpha_for(
            &quad(40.0, 0.0, false, false, SourceRectDescriptor::default()),
            &params(0.25, 0.4, false),
        );
        let flipped = alpha_for(
            &quad(40.0, 0.0, true, false, SourceRectDescriptor::default()),
            &params(0.25, 0.4, false),
        );

        let (x0, _) = centroid(&plain);
        let (x1, _) = centroid(&flipped);
        assert!((x0 - 60.0).abs() < 0.6, "{x0}");
        assert!((x1 - 40.0).abs() < 0.6, "{x1}");
        assert!((kept(&flipped) - kept(&plain)).abs() < 2.0);
    }

    #[test]
    fn a_crop_keeps_the_mask_over_the_same_visible_pixels() {
        let full = alpha_for(
            &quad(40.0, 0.0, false, false, SourceRectDescriptor::default()),
            &params(0.25, 0.4, false),
        );
        let cropped = alpha_for(
            &quad(
                40.0,
                0.0,
                false,
                false,
                SourceRectDescriptor {
                    x: 0.5,
                    y: 0.25,
                    width: 0.5,
                    height: 0.5,
                },
            ),
            &params(0.25, 0.4, false),
        );
        assert_eq!(full, cropped);
    }

    #[test]
    fn a_degenerate_quad_yields_no_mask() {
        let mut degenerate = quad(40.0, 0.0, false, false, SourceRectDescriptor::default());
        degenerate.width = 0.0;
        assert!(QuadMapper::new(&degenerate).is_none());
        let mut no_source = quad(40.0, 0.0, false, false, SourceRectDescriptor::default());
        no_source.source_rect.width = 0.0;
        assert!(QuadMapper::new(&no_source).is_none());
    }
}

#[cfg(test)]
mod stroke_and_blur_tests {
    use super::*;
    use compositor::{QuadTransformDescriptor, SourceRectDescriptor};
    use cutix_project::model::Mask;

    fn mask_with(params: serde_json::Value) -> Mask {
        serde_json::from_value(serde_json::json!({
            "id": "mask-1",
            "type": "rectangle",
            "params": params,
        }))
        .expect("mask")
    }

    fn quad() -> QuadTransformDescriptor {
        QuadTransformDescriptor {
            center_x: 100.0,
            center_y: 100.0,
            width: 200.0,
            height: 200.0,
            rotation_degrees: 0.0,
            flip_x: false,
            flip_y: false,
            source_rect: SourceRectDescriptor::default(),
        }
    }

    #[test]
    fn a_stroke_is_read_off_the_stored_params() {
        let mask = mask_with(serde_json::json!({
            "strokeWidth": 6.0,
            "strokeColor": "#ff0000",
            "strokeAlign": "outside",
        }));
        let stroke = mask_stroke(&mask).expect("stroke");
        assert_eq!(stroke.width, 6.0);
        assert_eq!(stroke.align, masks::StrokeAlign::Outside);
        assert_eq!(stroke.color, [255, 0, 0, 255]);
    }

    #[test]
    fn a_stroke_defaults_to_a_centred_white_line() {
        let stroke =
            mask_stroke(&mask_with(serde_json::json!({ "strokeWidth": 2.0 }))).expect("stroke");
        assert_eq!(stroke.align, masks::StrokeAlign::Center);
        assert_eq!(stroke.color, [255, 255, 255, 255]);
    }

    #[test]
    fn a_zero_width_or_transparent_stroke_is_not_drawn() {
        assert!(mask_stroke(&mask_with(serde_json::json!({}))).is_none());
        assert!(mask_stroke(&mask_with(serde_json::json!({ "strokeWidth": 0.0 }))).is_none());
        assert!(
            mask_stroke(&mask_with(serde_json::json!({
                "strokeWidth": 4.0,
                "strokeColor": "rgba(255,255,255,0)",
            })))
            .is_none()
        );
    }

    #[test]
    fn the_stroke_band_lands_on_the_mask_edge_in_source_space() {
        let mapper = QuadMapper::new(&quad()).expect("mapper");
        assert_eq!(mapper.source_grid(), (200, 200));
        let stored = mask_params(&mask_with(serde_json::json!({
            "width": 0.5,
            "height": 0.5,
        })));
        let params = source_space_params(&stored, &mapper);
        let band = masks::shapes::stroke_alpha(
            masks::MaskShape::Rectangle,
            &params,
            20.0,
            masks::StrokeAlign::Center,
            200,
            200,
        );
        assert_eq!(band[100 * 200 + 39], 0);
        assert_eq!(band[100 * 200 + 45], 255);
        assert_eq!(band[100 * 200 + 60], 0);
        assert_eq!(band[100 * 200 + 100], 0);
    }

    fn element_with(effects: serde_json::Value) -> TimelineElement {
        serde_json::from_value(serde_json::json!({
            "id": "e1",
            "type": "image",
            "name": "image",
            "mediaId": "m",
            "startTime": 0,
            "duration": 120_000,
            "trimStart": 0,
            "trimEnd": 0,
            "transform": { "scaleX": 1.0, "scaleY": 1.0, "position": { "x": 0.0, "y": 0.0 }, "rotate": 0.0 },
            "opacity": 1.0,
            "effects": effects,
        }))
        .expect("element")
    }

    #[test]
    fn background_blur_builds_passes_that_scale_with_the_canvas() {
        let element = element_with(serde_json::json!([
            { "id": "fx", "type": "background-blur", "enabled": true, "params": { "strength": 40 } }
        ]));
        let full = background_blur_passes(&element, 1920, 1080);
        let half = background_blur_passes(&element, 960, 540);
        assert!(!full.is_empty());
        assert_eq!(full.len(), half.len());
        let describe = |passes: &[compositor::EffectPassDescriptor]| {
            serde_json::to_value(passes).expect("json")
        };
        assert_eq!(
            describe(&full),
            describe(&crate::effects_map::gaussian_blur_passes(
                crate::effects_map::intensity_to_sigma(40.0, 1920.0, 1920.0),
                crate::effects_map::intensity_to_sigma(40.0, 1080.0, 1080.0),
            ))
        );
        assert_ne!(describe(&full), describe(&half));
        assert!(full.iter().all(|pass| pass.shader == "gaussian-blur"));
    }

    #[test]
    fn background_blur_is_silent_when_off_or_at_zero_strength() {
        let disabled = element_with(serde_json::json!([
            { "id": "fx", "type": "background-blur", "enabled": false, "params": { "strength": 40 } }
        ]));
        assert!(background_blur_passes(&disabled, 1920, 1080).is_empty());
        let zero = element_with(serde_json::json!([
            { "id": "fx", "type": "background-blur", "enabled": true, "params": { "strength": 0 } }
        ]));
        assert!(background_blur_passes(&zero, 1920, 1080).is_empty());
        assert!(
            background_blur_passes(&element_with(serde_json::json!([])), 1920, 1080).is_empty()
        );
    }

    #[test]
    fn a_file_backed_matte_decodes_from_the_project_directory() {
        let directory = tempfile::tempdir().expect("temp");
        std::fs::create_dir_all(directory.path().join("mattes")).expect("dir");
        let alpha: Vec<u8> = (0..64)
            .map(|index| if index % 8 < 4 { 0 } else { 255 })
            .collect();
        let png = ml::encode_matte_png(&alpha, 8, 8).expect("png");
        std::fs::write(directory.path().join("mattes/a.png"), &png).expect("write");

        let mut cache = MatteCache::default();
        cache.set_root(Some(directory.path().to_path_buf()));
        let cutout = ml::ElementCutout {
            enabled: true,
            mode: ml::CutoutMode::Static,
            width: 8,
            height: 8,
            png: String::new(),
            png_path: Some(String::from("mattes/a.png")),
            invert: false,
            reference_time: 0.0,
            coverage: 0.5,
            frames: None,
            sample_interval: None,
        };
        let composed = cutout_alpha(&cutout, None, &quad(), 200, 200, &mut cache).expect("alpha");
        assert_eq!(composed[100 * 200 + 50], 0);
        assert_eq!(composed[100 * 200 + 150], 255);
        assert_eq!(cache.stats(), (0, 1));
        let again = cutout_alpha(&cutout, None, &quad(), 200, 200, &mut cache).expect("alpha");
        assert_eq!(again, composed);
        assert_eq!(cache.stats(), (1, 1));
    }

    #[test]
    fn a_missing_matte_file_yields_no_mask_rather_than_a_panic() {
        let mut cache = MatteCache::default();
        cache.set_root(Some(std::path::PathBuf::from("/nowhere")));
        let cutout = ml::ElementCutout {
            enabled: true,
            mode: ml::CutoutMode::Static,
            width: 8,
            height: 8,
            png: String::new(),
            png_path: Some(String::from("mattes/missing.png")),
            invert: false,
            reference_time: 0.0,
            coverage: 0.5,
            frames: None,
            sample_interval: None,
        };
        assert!(cutout_alpha(&cutout, None, &quad(), 100, 100, &mut cache).is_none());
    }
}

#[cfg(test)]
mod backdrop_tests {
    use super::*;

    #[test]
    fn a_wide_source_overflows_sideways_and_a_tall_one_vertically() {
        let wide = cover_quad(1920.0, 1080.0, 1000.0, 1000.0);
        assert_eq!((wide.width, wide.height), (1777.7778, 1000.0));
        assert_eq!((wide.center_x, wide.center_y), (500.0, 500.0));

        let tall = cover_quad(1080.0, 1920.0, 1000.0, 1000.0);
        assert_eq!((tall.width, tall.height), (1000.0, 1777.7778));

        let square = cover_quad(100.0, 100.0, 640.0, 360.0);
        assert_eq!((square.width, square.height), (640.0, 640.0));
    }

    #[test]
    fn a_degenerate_source_falls_back_to_its_own_size() {
        let quad = cover_quad(0.0, 0.0, 640.0, 360.0);
        assert_eq!((quad.width, quad.height), (0.0, 0.0));
    }
}

#[cfg(test)]
mod element_rect_tests {
    use super::*;
    use compositor::{QuadTransformDescriptor, SourceRectDescriptor};

    fn rect(rotation: f32) -> ElementRect {
        ElementRect {
            center_x: 100.0,
            center_y: 80.0,
            width: 200.0,
            height: 120.0,
            rotation_degrees: rotation,
        }
    }

    #[test]
    fn the_centre_and_corners_map_to_the_expected_fractions() {
        let rect = rect(0.0);
        assert_eq!(rect.local_from_canvas(100.0, 80.0), (0.0, 0.0));
        assert_eq!(rect.local_from_canvas(200.0, 140.0), (0.5, 0.5));
        assert_eq!(rect.canvas_from_local(-0.5, -0.5), (0.0, 20.0));
    }

    #[test]
    fn the_two_directions_are_inverses_at_every_rotation() {
        for rotation in [0.0, 30.0, 90.0, 137.0, -45.0] {
            let rect = rect(rotation);
            for (u, v) in [(0.0, 0.0), (0.5, 0.25), (-0.3, 0.4), (0.5, -0.5)] {
                let (x, y) = rect.canvas_from_local(u, v);
                let (back_u, back_v) = rect.local_from_canvas(x, y);
                assert!((back_u - u).abs() < 1e-4, "{rotation}: {back_u} vs {u}");
                assert!((back_v - v).abs() < 1e-4, "{rotation}: {back_v} vs {v}");
            }
        }
    }

    #[test]
    fn it_agrees_with_the_mask_rasterisers_inverse() {
        for rotation in [0.0, 25.0, 90.0, -60.0] {
            let quad = QuadTransformDescriptor {
                center_x: 100.0,
                center_y: 80.0,
                width: 200.0,
                height: 120.0,
                rotation_degrees: rotation,
                flip_x: false,
                flip_y: false,
                source_rect: SourceRectDescriptor::default(),
            };
            let mapper = QuadMapper::new(&quad).expect("mapper");
            let rect = rect(rotation);
            for (x, y) in [(100usize, 80usize), (120, 60), (40, 110), (170, 30)] {
                let Some((u, v)) = mapper.source_uv(x, y) else {
                    continue;
                };
                let (local_u, local_v) = rect.local_from_canvas(x as f32 + 0.5, y as f32 + 0.5);
                assert!(
                    ((local_u + 0.5) as f64 - u).abs() < 1e-3,
                    "{rotation} at {x},{y}: {local_u} vs {u}"
                );
                assert!(
                    ((local_v + 0.5) as f64 - v).abs() < 1e-3,
                    "{rotation} at {x},{y}: {local_v} vs {v}"
                );
            }
        }
    }

    #[test]
    fn a_rotated_sub_rect_reports_a_larger_axis_aligned_box() {
        let straight = rect(0.0).local_bounds(0.0, 0.0, 0.5, 0.5);
        assert_eq!(straight, (50.0, 50.0, 100.0, 60.0));

        let turned = rect(90.0).local_bounds(0.0, 0.0, 0.5, 0.5);
        assert!((turned.2 - 60.0).abs() < 1e-3, "{turned:?}");
        assert!((turned.3 - 100.0).abs() < 1e-3, "{turned:?}");

        let diagonal = rect(45.0).local_bounds(0.0, 0.0, 0.5, 0.5);
        assert!(diagonal.2 > straight.2, "{diagonal:?}");
    }

    #[test]
    fn a_degenerate_rect_reports_the_centre_rather_than_dividing_by_zero() {
        let flat = ElementRect {
            center_x: 10.0,
            center_y: 10.0,
            width: 0.0,
            height: 0.0,
            rotation_degrees: 0.0,
        };
        assert_eq!(flat.local_from_canvas(50.0, 50.0), (0.0, 0.0));
    }
}
