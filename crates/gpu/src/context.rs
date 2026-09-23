use wgpu::util::DeviceExt;

#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
use wasm_bindgen::JsCast;

use crate::{FULLSCREEN_SHADER_SOURCE, GpuError};

#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
#[derive(Debug)]
struct WebDisplay;

#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
impl wgpu::rwh::HasDisplayHandle for WebDisplay {
    fn display_handle(&self) -> Result<wgpu::rwh::DisplayHandle<'_>, wgpu::rwh::HandleError> {
        let raw = wgpu::rwh::WebDisplayHandle::new();
        Ok(unsafe { wgpu::rwh::DisplayHandle::borrow_raw(raw.into()) })
    }
}

const BLIT_SHADER_SOURCE: &str = include_str!("shaders/blit.wgsl");

const FULLSCREEN_QUAD_POSITIONS: [[f32; 2]; 6] = [
    [-1.0, -1.0],
    [1.0, -1.0],
    [-1.0, 1.0],
    [-1.0, 1.0],
    [1.0, -1.0],
    [1.0, 1.0],
];

#[cfg_attr(not(all(feature = "wasm", target_arch = "wasm32")), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReadbackLayout {
    unpadded_bytes_per_row: u32,
    padded_bytes_per_row: u32,
    height: u32,
}

#[cfg_attr(not(all(feature = "wasm", target_arch = "wasm32")), allow(dead_code))]
impl ReadbackLayout {
    const BYTES_PER_PIXEL: u32 = 4;

    fn new(width: u32, height: u32) -> Self {
        let unpadded_bytes_per_row = width * Self::BYTES_PER_PIXEL;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        Self {
            unpadded_bytes_per_row,
            padded_bytes_per_row,
            height,
        }
    }

    fn buffer_size(&self) -> u64 {
        u64::from(self.padded_bytes_per_row) * u64::from(self.height)
    }

    fn strip_padding(&self, padded: &[u8]) -> Vec<u8> {
        let row = self.unpadded_bytes_per_row as usize;
        let mut tight = Vec::with_capacity(row * self.height as usize);
        for chunk in padded
            .chunks(self.padded_bytes_per_row as usize)
            .take(self.height as usize)
        {
            tight.extend_from_slice(&chunk[..row]);
        }
        tight
    }
}

pub struct GpuContext {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture_format: wgpu::TextureFormat,
    fullscreen_quad: wgpu::Buffer,
    linear_sampler: wgpu::Sampler,
    nearest_sampler: wgpu::Sampler,
    texture_sampler_bind_group_layout: wgpu::BindGroupLayout,
    blit_pipeline: wgpu::RenderPipeline,
    #[cfg(target_arch = "wasm32")]
    supports_external_texture_copies: bool,
}

impl GpuContext {
    pub async fn new() -> Result<Self, GpuError> {
        let (instance, adapter, device, queue) = Self::acquire_device().await?;
        let texture_format = if adapter.get_info().backend == wgpu::Backend::Gl {
            wgpu::TextureFormat::Rgba8Unorm
        } else {
            wgpu::TextureFormat::Bgra8Unorm
        };
        let fullscreen_quad = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gpu-fullscreen-quad-buffer"),
            contents: bytemuck::cast_slice(&FULLSCREEN_QUAD_POSITIONS),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let linear_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("gpu-linear-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("gpu-nearest-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let texture_sampler_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("gpu-texture-sampler-bind-group-layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });
        let vertex_shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gpu-fullscreen-shader"),
            source: wgpu::ShaderSource::Wgsl(FULLSCREEN_SHADER_SOURCE.into()),
        });
        let blit_shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gpu-blit-shader"),
            source: wgpu::ShaderSource::Wgsl(BLIT_SHADER_SOURCE.into()),
        });
        let blit_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu-blit-pipeline-layout"),
            bind_group_layouts: &[Some(&texture_sampler_bind_group_layout)],
            immediate_size: 0,
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gpu-blit-pipeline"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_shader_module,
                entry_point: Some("vertex_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<[f32; 2]>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x2,
                        offset: 0,
                        shader_location: 0,
                    }],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader_module,
                entry_point: Some("fragment_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: texture_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        #[cfg(target_arch = "wasm32")]
        let supports_external_texture_copies = adapter
            .get_downlevel_capabilities()
            .flags
            .contains(wgpu::DownlevelFlags::UNRESTRICTED_EXTERNAL_TEXTURE_COPIES);

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            texture_format,
            fullscreen_quad,
            linear_sampler,
            nearest_sampler,
            texture_sampler_bind_group_layout,
            blit_pipeline,
            #[cfg(target_arch = "wasm32")]
            supports_external_texture_copies,
        })
    }

    async fn acquire_device()
    -> Result<(wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue), GpuError> {
        let instance = wgpu::util::new_instance_with_webgpu_detection(
            wgpu::InstanceDescriptor::new_without_display_handle(),
        )
        .await;

        if let Ok((adapter, device, queue)) = Self::try_request_device(&instance, None).await {
            return Ok((instance, adapter, device, queue));
        }

        Self::try_gl_fallback().await
    }

    #[cfg(all(feature = "wasm", target_arch = "wasm32"))]
    async fn try_gl_fallback()
    -> Result<(wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue), GpuError> {
        let mut gl_desc = wgpu::InstanceDescriptor::new_without_display_handle();
        gl_desc.backends = wgpu::Backends::GL;
        gl_desc.display = Some(Box::new(WebDisplay));
        let gl_instance = wgpu::Instance::new(gl_desc);

        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or(GpuError::AdapterUnavailable)?;
        let canvas: web_sys::HtmlCanvasElement = document
            .create_element("canvas")
            .map_err(|_| GpuError::AdapterUnavailable)?
            .unchecked_into();
        canvas.set_width(1);
        canvas.set_height(1);
        let surface = gl_instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas))?;

        let (adapter, device, queue) =
            Self::try_request_device(&gl_instance, Some(&surface)).await?;
        Ok((gl_instance, adapter, device, queue))
    }

    #[cfg(not(all(feature = "wasm", target_arch = "wasm32")))]
    async fn try_gl_fallback()
    -> Result<(wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue), GpuError> {
        Err(GpuError::AdapterUnavailable)
    }

    fn device_limits(adapter: &wgpu::Adapter) -> wgpu::Limits {
        let adapter_limits = adapter.limits();
        let mut limits =
            wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter_limits.clone());
        limits.max_storage_buffers_per_shader_stage = limits
            .max_storage_buffers_per_shader_stage
            .max(adapter_limits.max_storage_buffers_per_shader_stage.min(1));
        limits.max_storage_buffer_binding_size = limits
            .max_storage_buffer_binding_size
            .max(adapter_limits.max_storage_buffer_binding_size);
        limits
    }

    async fn try_request_device(
        instance: &wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue), GpuError> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface,
                force_fallback_adapter: false,
            })
            .await
            .map_err(|_| GpuError::AdapterUnavailable)?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("gpu-device"),
                required_features: wgpu::Features::empty(),
                required_limits: Self::device_limits(&adapter),
                memory_hints: wgpu::MemoryHints::Performance,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                trace: wgpu::Trace::Off,
            })
            .await?;

        Ok((adapter, device, queue))
    }

    pub fn create_render_texture(
        &self,
        width: u32,
        height: u32,
        label: &'static str,
    ) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.texture_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
    }

    pub fn instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    pub fn texture_format(&self) -> wgpu::TextureFormat {
        self.texture_format
    }

    pub fn fullscreen_quad(&self) -> &wgpu::Buffer {
        &self.fullscreen_quad
    }

    pub fn linear_sampler(&self) -> &wgpu::Sampler {
        &self.linear_sampler
    }

    pub fn nearest_sampler(&self) -> &wgpu::Sampler {
        &self.nearest_sampler
    }

    pub fn texture_sampler_bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.texture_sampler_bind_group_layout
    }

    pub fn blit_pipeline(&self) -> &wgpu::RenderPipeline {
        &self.blit_pipeline
    }

    pub fn render_texture_to_surface(
        &self,
        texture: &wgpu::Texture,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
    ) -> Result<(), GpuError> {
        self.configure_surface(surface, width, height)?;
        let surface_texture = self.acquire_surface_texture(surface)?;
        let target_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gpu-surface-blit-encoder"),
            });
        self.encode_texture_blit_to_view(&mut encoder, texture, &target_view, "gpu-surface-blit");
        self.queue.submit([encoder.finish()]);
        surface_texture.present();
        Ok(())
    }

    pub fn configure_surface(
        &self,
        surface: &wgpu::Surface<'_>,
        width: u32,
        height: u32,
    ) -> Result<(), GpuError> {
        let Some(mut config) = surface.get_default_config(&self.adapter, width, height) else {
            return Err(GpuError::UnsupportedSurfaceFormat);
        };
        let capabilities = surface.get_capabilities(&self.adapter);
        if !capabilities.formats.contains(&self.texture_format) {
            return Err(GpuError::UnsupportedSurfaceFormat);
        }
        config.format = self.texture_format;
        surface.configure(&self.device, &config);
        Ok(())
    }

    pub fn acquire_surface_texture(
        &self,
        surface: &wgpu::Surface<'_>,
    ) -> Result<wgpu::SurfaceTexture, GpuError> {
        match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(surface_texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => Ok(surface_texture),
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Lost
            | wgpu::CurrentSurfaceTexture::Validation => Err(GpuError::UnsupportedSurfaceFormat),
        }
    }

    pub fn encode_texture_blit_to_view(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        texture: &wgpu::Texture,
        target_view: &wgpu::TextureView,
        label: &'static str,
    ) {
        let source_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("gpu-blit-bind-group"),
            layout: &self.texture_sampler_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.linear_sampler),
                },
            ],
        });

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            occlusion_query_set: None,
            timestamp_writes: None,
            multiview_mask: None,
        });
        render_pass.set_pipeline(&self.blit_pipeline);
        render_pass.set_vertex_buffer(0, self.fullscreen_quad.slice(..));
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..6, 0..1);
    }

    #[cfg(all(feature = "wasm", target_arch = "wasm32"))]
    pub fn import_offscreen_canvas_texture(
        &self,
        canvas: &wgpu::web_sys::OffscreenCanvas,
        width: u32,
        height: u32,
        label: &'static str,
    ) -> wgpu::Texture {
        let texture = self.create_render_texture(width, height, label);

        if self.supports_external_texture_copies {
            self.queue.copy_external_image_to_texture(
                &wgpu::CopyExternalImageSourceInfo {
                    source: wgpu::ExternalImageSource::OffscreenCanvas(canvas.clone()),
                    origin: wgpu::Origin2d::ZERO,
                    flip_y: false,
                },
                wgpu::CopyExternalImageDestInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                    color_space: wgpu::PredefinedColorSpace::Srgb,
                    premultiplied_alpha: false,
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        } else {
            let ctx: web_sys::OffscreenCanvasRenderingContext2d = canvas
                .get_context("2d")
                .ok()
                .flatten()
                .expect("Failed to get 2d context for texture import")
                .unchecked_into();
            let image_data = ctx
                .get_image_data(0.0, 0.0, width as f64, height as f64)
                .expect("Failed to read pixel data from canvas");
            let rgba_bytes = image_data.data();

            let pixel_bytes = if self.texture_format == wgpu::TextureFormat::Bgra8Unorm {
                let mut bytes = rgba_bytes.to_vec();
                for pixel in bytes.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
                bytes
            } else {
                rgba_bytes.to_vec()
            };

            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &pixel_bytes,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }

        texture
    }

    #[cfg(all(feature = "wasm", target_arch = "wasm32"))]
    pub fn render_texture_to_offscreen_canvas(
        &self,
        texture: &wgpu::Texture,
        canvas: &wgpu::web_sys::OffscreenCanvas,
        width: u32,
        height: u32,
    ) -> Result<(), GpuError> {
        if self.supports_external_texture_copies {
            let surface = self
                .instance
                .create_surface(wgpu::SurfaceTarget::OffscreenCanvas(canvas.clone()))?;
            return self.render_texture_to_surface(texture, &surface, width, height);
        }

        self.readback_texture_to_offscreen_canvas(texture, canvas, width, height)
    }

    #[cfg(all(feature = "wasm", target_arch = "wasm32"))]
    fn readback_texture_to_offscreen_canvas(
        &self,
        texture: &wgpu::Texture,
        canvas: &wgpu::web_sys::OffscreenCanvas,
        width: u32,
        height: u32,
    ) -> Result<(), GpuError> {
        let layout = ReadbackLayout::new(width, height);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu-readback-buffer"),
            size: layout.buffer_size(),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gpu-readback-encoder"),
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
                    bytes_per_row: Some(layout.padded_bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);

        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());

        let data = slice.get_mapped_range();
        let mut rgba_bytes = layout.strip_padding(&data);
        drop(data);
        buffer.unmap();

        if self.texture_format == wgpu::TextureFormat::Bgra8Unorm {
            for pixel in rgba_bytes.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }

        let clamped = wasm_bindgen::Clamped(&rgba_bytes[..]);
        let image_data =
            web_sys::ImageData::new_with_u8_clamped_array_and_sh(clamped, width, height)
                .map_err(|_| GpuError::AdapterUnavailable)?;

        let ctx: web_sys::OffscreenCanvasRenderingContext2d = canvas
            .get_context("2d")
            .ok()
            .flatten()
            .ok_or(GpuError::AdapterUnavailable)?
            .unchecked_into();
        ctx.put_image_data(&image_data, 0.0, 0.0)
            .map_err(|_| GpuError::AdapterUnavailable)?;

        Ok(())
    }
}

#[cfg(test)]
mod readback_layout_tests {
    use super::ReadbackLayout;

    #[test]
    fn readback_rows_are_padded_to_copy_alignment() {
        let layout = ReadbackLayout::new(100, 3);
        assert_eq!(layout.unpadded_bytes_per_row, 400);
        assert_eq!(layout.padded_bytes_per_row, 512);
        assert_eq!(layout.buffer_size(), 512 * 3);
    }

    #[test]
    fn readback_rows_already_aligned_are_not_padded() {
        let layout = ReadbackLayout::new(64, 2);
        assert_eq!(layout.padded_bytes_per_row, 256);
        assert_eq!(layout.buffer_size(), 512);
    }

    #[test]
    fn readback_buffer_size_does_not_overflow_u32() {
        let layout = ReadbackLayout::new(16_384, 16_384);
        assert_eq!(layout.buffer_size(), 16_384u64 * 4 * 16_384);
    }

    #[test]
    fn readback_padding_is_stripped_per_row() {
        let layout = ReadbackLayout::new(2, 2);
        let mut padded = vec![0xEEu8; layout.buffer_size() as usize];
        padded[..8].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
        padded[256..264].copy_from_slice(&[9, 10, 11, 12, 13, 14, 15, 16]);
        assert_eq!(layout.strip_padding(&padded), (1..=16).collect::<Vec<u8>>());
    }
}
