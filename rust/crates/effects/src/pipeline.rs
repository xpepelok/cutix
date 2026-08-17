use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use bytemuck::{Pod, Zeroable};
use gpu::{FULLSCREEN_SHADER_SOURCE, GpuContext};
use thiserror::Error;
use wgpu::util::DeviceExt;

use crate::{EffectPass, UniformValue};

const GAUSSIAN_BLUR_SHADER_ID: &str = "gaussian-blur";
const GAUSSIAN_BLUR_SHADER_SOURCE: &str = include_str!("shaders/gaussian_blur.wgsl");
const CHROMA_KEY_SHADER_ID: &str = "chroma-key";
const CHROMA_KEY_SHADER_SOURCE: &str = include_str!("shaders/chroma_key.wgsl");
const RETOUCH_SHADER_ID: &str = "retouch";
const RETOUCH_SHADER_SOURCE: &str = include_str!("shaders/retouch.wgsl");
const ADJUSTMENT_SHADER_ID: &str = "adjustment";
const ADJUSTMENT_SHADER_SOURCE: &str = include_str!("shaders/adjustment.wgsl");
const MOSAIC_SHADER_ID: &str = "mosaic";
const MOSAIC_SHADER_SOURCE: &str = include_str!("shaders/mosaic.wgsl");
const CURVES_SHADER_ID: &str = "curves";
const CURVES_SHADER_SOURCE: &str = include_str!("shaders/curves.wgsl");
const HSL_QUALIFIER_SHADER_ID: &str = "hsl-qualifier";
const HSL_QUALIFIER_SHADER_SOURCE: &str = include_str!("shaders/hsl_qualifier.wgsl");
const LUT3D_SHADER_ID: &str = "lut3d";
const LUT3D_SHADER_SOURCE: &str = include_str!("shaders/lut3d.wgsl");

const SHADER_SOURCES: &[(&str, &str)] = &[
    (GAUSSIAN_BLUR_SHADER_ID, GAUSSIAN_BLUR_SHADER_SOURCE),
    (CHROMA_KEY_SHADER_ID, CHROMA_KEY_SHADER_SOURCE),
    (RETOUCH_SHADER_ID, RETOUCH_SHADER_SOURCE),
    (ADJUSTMENT_SHADER_ID, ADJUSTMENT_SHADER_SOURCE),
    (MOSAIC_SHADER_ID, MOSAIC_SHADER_SOURCE),
    (CURVES_SHADER_ID, CURVES_SHADER_SOURCE),
    (HSL_QUALIFIER_SHADER_ID, HSL_QUALIFIER_SHADER_SOURCE),
    (LUT3D_SHADER_ID, LUT3D_SHADER_SOURCE),
];

const UNIFORM_TABLE_SHADER_IDS: &[&str] = &[CURVES_SHADER_ID, HSL_QUALIFIER_SHADER_ID];

const STORAGE_TABLE_SHADER_IDS: &[&str] = &[LUT3D_SHADER_ID];

const TABLE_UNIFORM: &str = "u_table";

const UNIFORM_TABLE_FLOATS: usize = 256 * 4;

fn shader_uses_uniform_table(shader: &str) -> bool {
    UNIFORM_TABLE_SHADER_IDS.contains(&shader)
}

fn shader_uses_storage_table(shader: &str) -> bool {
    STORAGE_TABLE_SHADER_IDS.contains(&shader)
}

fn shader_uses_table(shader: &str) -> bool {
    shader_uses_uniform_table(shader) || shader_uses_storage_table(shader)
}

pub struct ApplyEffectsOptions<'a> {
    pub source: &'a wgpu::Texture,
    pub width: u32,
    pub height: u32,
    pub passes: &'a [EffectPass],
}

pub struct EffectPipeline {
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    uniform_table_bind_group_layout: wgpu::BindGroupLayout,
    storage_table_bind_group_layout: Option<wgpu::BindGroupLayout>,
    pipelines: HashMap<String, wgpu::RenderPipeline>,
    tables: EffectTables,
}

pub type EffectTables = Rc<RefCell<HashMap<String, wgpu::Buffer>>>;

#[derive(Debug, Error)]
pub enum EffectsError {
    #[error("At least one effect pass is required")]
    MissingEffectPasses,
    #[error("Unknown effect shader '{shader}'")]
    UnknownEffectShader { shader: String },
    #[error("Missing uniform '{uniform}' for shader '{shader}'")]
    MissingUniform { shader: String, uniform: String },
    #[error("Uniform '{uniform}' for shader '{shader}' must be a number")]
    InvalidNumberUniform { shader: String, uniform: String },
    #[error(
        "Uniform '{uniform}' for shader '{shader}' must be a vector of length {expected_length}"
    )]
    InvalidVectorUniform {
        shader: String,
        uniform: String,
        expected_length: usize,
    },
    #[error("Shader '{shader}' does not support uniform '{uniform}'")]
    UnsupportedUniform { shader: String, uniform: String },
    #[error("Shader '{shader}' requires a lookup table")]
    MissingTable { shader: String },
    #[error("Lookup table '{data_id}' is not registered")]
    UnknownTable { data_id: String },
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct EffectUniformBuffer {
    resolution: [f32; 2],
    direction: [f32; 2],
    scalars: [f32; 4],
    extra: [f32; 4],
    extra2: [f32; 4],
}

impl EffectPipeline {
    pub fn new(context: &GpuContext) -> Self {
        let uniform_bind_group_layout =
            context
                .device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("effects-uniform-bind-group-layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });
        let uniform_table_bind_group_layout =
            context
                .device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("effects-uniform-table-bind-group-layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });
        let supports_storage_tables = context
            .device()
            .limits()
            .max_storage_buffers_per_shader_stage
            >= 1;
        let storage_table_bind_group_layout = supports_storage_tables.then(|| {
            context
                .device()
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("effects-storage-table-bind-group-layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                })
        });
        let vertex_shader_module =
            context
                .device()
                .create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("effects-fullscreen-shader"),
                    source: wgpu::ShaderSource::Wgsl(FULLSCREEN_SHADER_SOURCE.into()),
                });
        let pipeline_layout =
            context
                .device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("effects-pipeline-layout"),
                    bind_group_layouts: &[
                        Some(context.texture_sampler_bind_group_layout()),
                        Some(&uniform_bind_group_layout),
                    ],
                    immediate_size: 0,
                });
        let uniform_table_pipeline_layout =
            context
                .device()
                .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("effects-uniform-table-pipeline-layout"),
                    bind_group_layouts: &[
                        Some(context.texture_sampler_bind_group_layout()),
                        Some(&uniform_bind_group_layout),
                        Some(&uniform_table_bind_group_layout),
                    ],
                    immediate_size: 0,
                });
        let storage_table_pipeline_layout =
            storage_table_bind_group_layout.as_ref().map(|layout| {
                context
                    .device()
                    .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("effects-storage-table-pipeline-layout"),
                        bind_group_layouts: &[
                            Some(context.texture_sampler_bind_group_layout()),
                            Some(&uniform_bind_group_layout),
                            Some(layout),
                        ],
                        immediate_size: 0,
                    })
            });
        let mut pipelines = HashMap::new();
        for (shader_id, shader_source) in SHADER_SOURCES {
            let layout = if shader_uses_storage_table(shader_id) {
                match storage_table_pipeline_layout.as_ref() {
                    Some(layout) => layout,
                    None => continue,
                }
            } else if shader_uses_uniform_table(shader_id) {
                &uniform_table_pipeline_layout
            } else {
                &pipeline_layout
            };
            let shader_module =
                context
                    .device()
                    .create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some(&format!("effects-{shader_id}-shader")),
                        source: wgpu::ShaderSource::Wgsl((*shader_source).into()),
                    });
            let pipeline =
                context
                    .device()
                    .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some(&format!("effects-{shader_id}-pipeline")),
                        layout: Some(layout),
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
                            module: &shader_module,
                            entry_point: Some("fragment_main"),
                            targets: &[Some(wgpu::ColorTargetState {
                                format: context.texture_format(),
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
            pipelines.insert((*shader_id).to_string(), pipeline);
        }

        Self {
            uniform_bind_group_layout,
            uniform_table_bind_group_layout,
            storage_table_bind_group_layout,
            pipelines,
            tables: EffectTables::default(),
        }
    }

    pub fn tables(&self) -> EffectTables {
        Rc::clone(&self.tables)
    }

    pub fn share_tables_from(&mut self, other: &EffectPipeline) {
        self.tables = other.tables();
    }

    pub fn supports_storage_tables(&self) -> bool {
        self.storage_table_bind_group_layout.is_some()
    }

    pub fn register_data(&self, context: &GpuContext, id: String, data: &[f32]) {
        let buffer = context
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("effects-table-buffer"),
                contents: bytemuck::cast_slice(&pad_table(data)),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            });
        self.tables.borrow_mut().insert(id, buffer);
    }

    pub fn release_data(&self, id: &str) {
        self.tables.borrow_mut().remove(id);
    }

    pub fn has_data(&self, id: &str) -> bool {
        self.tables.borrow().contains_key(id)
    }

    pub fn apply(
        &self,
        context: &GpuContext,
        ApplyEffectsOptions {
            source,
            width,
            height,
            passes,
        }: ApplyEffectsOptions<'_>,
    ) -> Result<wgpu::Texture, EffectsError> {
        let mut encoder =
            context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("effects-command-encoder"),
                });
        let output = self.apply_with_encoder(
            context,
            &mut encoder,
            ApplyEffectsOptions {
                source,
                width,
                height,
                passes,
            },
        )?;
        context.queue().submit([encoder.finish()]);
        Ok(output)
    }

    pub fn apply_with_encoder(
        &self,
        context: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        ApplyEffectsOptions {
            source,
            width,
            height,
            passes,
        }: ApplyEffectsOptions<'_>,
    ) -> Result<wgpu::Texture, EffectsError> {
        let mut current_texture: Option<wgpu::Texture> = None;
        let mut skipped_pass = false;

        for pass in passes {
            if shader_uses_storage_table(&pass.shader) && !self.supports_storage_tables() {
                skipped_pass = true;
                continue;
            }
            let input_texture = current_texture.as_ref().unwrap_or(source);
            let output_texture =
                context.create_render_texture(width, height, "effects-pass-output");
            let input_view = input_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());
            let texture_bind_group =
                context
                    .device()
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("effects-texture-bind-group"),
                        layout: context.texture_sampler_bind_group_layout(),
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&input_view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::Sampler(context.linear_sampler()),
                            },
                        ],
                    });
            let uniform_buffer =
                context
                    .device()
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("effects-uniform-buffer"),
                        contents: bytemuck::bytes_of(&pack_effect_uniforms(pass, width, height)?),
                        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    });
            let uniform_bind_group =
                context
                    .device()
                    .create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("effects-uniform-bind-group"),
                        layout: &self.uniform_bind_group_layout,
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform_buffer.as_entire_binding(),
                        }],
                    });
            let pipeline = self.pipelines.get(&pass.shader).ok_or_else(|| {
                EffectsError::UnknownEffectShader {
                    shader: pass.shader.clone(),
                }
            })?;
            let table_bind_group = if shader_uses_table(&pass.shader) {
                Some(self.create_table_bind_group(context, pass)?)
            } else {
                None
            };

            {
                let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("effects-render-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &output_view,
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
                render_pass.set_pipeline(pipeline);
                render_pass.set_vertex_buffer(0, context.fullscreen_quad().slice(..));
                render_pass.set_bind_group(0, &texture_bind_group, &[]);
                render_pass.set_bind_group(1, &uniform_bind_group, &[]);
                if let Some(table_bind_group) = &table_bind_group {
                    render_pass.set_bind_group(2, table_bind_group, &[]);
                }
                render_pass.draw(0..6, 0..1);
            }

            current_texture = Some(output_texture);
        }

        if let Some(texture) = current_texture {
            return Ok(texture);
        }
        if skipped_pass {
            return Ok(self.copy_source(context, encoder, source, width, height));
        }
        Err(EffectsError::MissingEffectPasses)
    }

    fn copy_source(
        &self,
        context: &GpuContext,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> wgpu::Texture {
        let output = context.create_render_texture(width, height, "effects-passthrough");
        encoder.copy_texture_to_texture(
            source.as_image_copy(),
            output.as_image_copy(),
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        output
    }

    fn create_table_bind_group(
        &self,
        context: &GpuContext,
        pass: &EffectPass,
    ) -> Result<wgpu::BindGroup, EffectsError> {
        if let Some(data_id) = &pass.data_id {
            let tables = self.tables.borrow();
            let buffer = tables
                .get(data_id)
                .ok_or_else(|| EffectsError::UnknownTable {
                    data_id: data_id.clone(),
                })?;
            return Ok(self.build_table_bind_group(context, &pass.shader, buffer));
        }

        let Some(UniformValue::Vector(values)) = pass.uniforms.get(TABLE_UNIFORM) else {
            return Err(EffectsError::MissingTable {
                shader: pass.shader.clone(),
            });
        };
        let is_uniform = shader_uses_uniform_table(&pass.shader);
        let contents = if is_uniform {
            pad_uniform_table(values)
        } else {
            pad_table(values)
        };
        let usage = if is_uniform {
            wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST
        } else {
            wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST
        };
        let buffer = context
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("effects-inline-table-buffer"),
                contents: bytemuck::cast_slice(&contents),
                usage,
            });
        Ok(self.build_table_bind_group(context, &pass.shader, &buffer))
    }

    fn build_table_bind_group(
        &self,
        context: &GpuContext,
        shader: &str,
        buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        let layout = if shader_uses_uniform_table(shader) {
            &self.uniform_table_bind_group_layout
        } else {
            self.storage_table_bind_group_layout
                .as_ref()
                .expect("storage table bind group layout")
        };
        context
            .device()
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("effects-table-bind-group"),
                layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            })
    }
}

fn pad_table(data: &[f32]) -> Vec<f32> {
    let entries = data.len().div_ceil(4).max(1);
    let mut padded = vec![0.0f32; entries * 4];
    padded[..data.len()].copy_from_slice(data);
    padded
}

fn pad_uniform_table(data: &[f32]) -> Vec<f32> {
    let mut padded = vec![0.0f32; UNIFORM_TABLE_FLOATS];
    let length = data.len().min(UNIFORM_TABLE_FLOATS);
    padded[..length].copy_from_slice(&data[..length]);
    padded
}

fn pack_effect_uniforms(
    pass: &EffectPass,
    width: u32,
    height: u32,
) -> Result<EffectUniformBuffer, EffectsError> {
    let shader = pass.shader.as_str();
    let resolution = [width as f32, height as f32];

    match shader {
        GAUSSIAN_BLUR_SHADER_ID => {
            let sigma = read_number_uniform(pass, "u_sigma")?;
            let step = read_number_uniform(pass, "u_step")?;
            let direction = read_vec2_uniform(pass, "u_direction")?;
            reject_unknown_uniforms(pass, &["u_sigma", "u_step", "u_direction"])?;

            Ok(EffectUniformBuffer {
                resolution,
                direction,
                scalars: [sigma, step, 0.0, 0.0],
                extra: [0.0; 4],
                extra2: [0.0; 4],
            })
        }
        CHROMA_KEY_SHADER_ID => {
            let key_color = read_vec3_uniform(pass, "u_key_color")?;
            let similarity = read_number_uniform(pass, "u_similarity")?;
            let smoothness = read_number_uniform(pass, "u_smoothness")?;
            let spill = read_number_uniform(pass, "u_spill")?;
            reject_unknown_uniforms(
                pass,
                &["u_key_color", "u_similarity", "u_smoothness", "u_spill"],
            )?;

            Ok(EffectUniformBuffer {
                resolution,
                direction: [0.0, 0.0],
                scalars: [key_color[0], key_color[1], key_color[2], similarity],
                extra: [smoothness, spill, 0.0, 0.0],
                extra2: [0.0; 4],
            })
        }
        RETOUCH_SHADER_ID => {
            let smoothing = read_number_uniform(pass, "u_smoothing")?;
            let radius = read_number_uniform(pass, "u_radius")?;
            let tone = read_number_uniform(pass, "u_tone")?;
            let brightness = read_number_uniform(pass, "u_brightness")?;
            let edge = read_number_uniform(pass, "u_edge")?;
            reject_unknown_uniforms(
                pass,
                &[
                    "u_smoothing",
                    "u_radius",
                    "u_tone",
                    "u_brightness",
                    "u_edge",
                ],
            )?;

            Ok(EffectUniformBuffer {
                resolution,
                direction: [0.0, 0.0],
                scalars: [smoothing, radius, tone, brightness],
                extra: [edge, 0.0, 0.0, 0.0],
                extra2: [0.0; 4],
            })
        }
        ADJUSTMENT_SHADER_ID => {
            let brightness = read_number_uniform(pass, "u_brightness")?;
            let contrast = read_number_uniform(pass, "u_contrast")?;
            let saturation = read_number_uniform(pass, "u_saturation")?;
            let exposure = read_number_uniform(pass, "u_exposure")?;
            let temperature = read_number_uniform(pass, "u_temperature")?;
            let tint = read_number_uniform(pass, "u_tint")?;
            let highlights = read_number_uniform(pass, "u_highlights")?;
            let shadows = read_number_uniform(pass, "u_shadows")?;
            let vibrance = read_number_uniform(pass, "u_vibrance")?;
            let sharpness = read_number_uniform(pass, "u_sharpness")?;
            reject_unknown_uniforms(
                pass,
                &[
                    "u_brightness",
                    "u_contrast",
                    "u_saturation",
                    "u_exposure",
                    "u_temperature",
                    "u_tint",
                    "u_highlights",
                    "u_shadows",
                    "u_vibrance",
                    "u_sharpness",
                ],
            )?;

            Ok(EffectUniformBuffer {
                resolution,
                direction: [0.0, 0.0],
                scalars: [brightness, contrast, saturation, exposure],
                extra: [temperature, tint, highlights, shadows],
                extra2: [vibrance, sharpness, 0.0, 0.0],
            })
        }
        MOSAIC_SHADER_ID => {
            let block_size = read_number_uniform(pass, "u_block_size")?;
            let region = read_vec4_uniform(pass, "u_region")?;
            let shape = read_number_uniform(pass, "u_shape")?;
            reject_unknown_uniforms(pass, &["u_block_size", "u_region", "u_shape"])?;

            Ok(EffectUniformBuffer {
                resolution,
                direction: [0.0, 0.0],
                scalars: [block_size, region[0], region[1], region[2]],
                extra: [region[3], shape, 0.0, 0.0],
                extra2: [0.0; 4],
            })
        }
        CURVES_SHADER_ID => {
            let amount = read_number_uniform(pass, "u_amount")?;
            reject_unknown_uniforms(pass, &["u_amount", TABLE_UNIFORM])?;

            Ok(EffectUniformBuffer {
                resolution,
                direction: [0.0, 0.0],
                scalars: [amount, 0.0, 0.0, 0.0],
                extra: [0.0; 4],
                extra2: [0.0; 4],
            })
        }
        HSL_QUALIFIER_SHADER_ID => {
            reject_unknown_uniforms(pass, &[TABLE_UNIFORM])?;

            Ok(EffectUniformBuffer {
                resolution,
                direction: [0.0, 0.0],
                scalars: [0.0; 4],
                extra: [0.0; 4],
                extra2: [0.0; 4],
            })
        }
        LUT3D_SHADER_ID => {
            let size = read_number_uniform(pass, "u_size")?;
            let intensity = read_number_uniform(pass, "u_intensity")?;
            reject_unknown_uniforms(pass, &["u_size", "u_intensity", TABLE_UNIFORM])?;

            Ok(EffectUniformBuffer {
                resolution,
                direction: [0.0, 0.0],
                scalars: [size, intensity, 0.0, 0.0],
                extra: [0.0; 4],
                extra2: [0.0; 4],
            })
        }
        _ => Err(EffectsError::UnknownEffectShader {
            shader: shader.to_string(),
        }),
    }
}

fn reject_unknown_uniforms(pass: &EffectPass, allowed: &[&str]) -> Result<(), EffectsError> {
    for uniform in pass.uniforms.keys() {
        if allowed.contains(&uniform.as_str()) {
            continue;
        }
        return Err(EffectsError::UnsupportedUniform {
            shader: pass.shader.clone(),
            uniform: uniform.clone(),
        });
    }
    Ok(())
}

fn read_number_uniform(pass: &EffectPass, uniform: &str) -> Result<f32, EffectsError> {
    let Some(value) = pass.uniforms.get(uniform) else {
        return Err(EffectsError::MissingUniform {
            shader: pass.shader.clone(),
            uniform: uniform.to_string(),
        });
    };
    match value {
        UniformValue::Number(value) => Ok(*value),
        UniformValue::Vector(_) => Err(EffectsError::InvalidNumberUniform {
            shader: pass.shader.clone(),
            uniform: uniform.to_string(),
        }),
    }
}

fn read_vec4_uniform(pass: &EffectPass, uniform: &str) -> Result<[f32; 4], EffectsError> {
    let Some(UniformValue::Vector(values)) = pass.uniforms.get(uniform) else {
        return match pass.uniforms.get(uniform) {
            None => Err(EffectsError::MissingUniform {
                shader: pass.shader.clone(),
                uniform: uniform.to_string(),
            }),
            Some(_) => Err(EffectsError::InvalidVectorUniform {
                shader: pass.shader.clone(),
                uniform: uniform.to_string(),
                expected_length: 4,
            }),
        };
    };
    if values.len() != 4 {
        return Err(EffectsError::InvalidVectorUniform {
            shader: pass.shader.clone(),
            uniform: uniform.to_string(),
            expected_length: 4,
        });
    }
    Ok([values[0], values[1], values[2], values[3]])
}

fn read_vec3_uniform(pass: &EffectPass, uniform: &str) -> Result<[f32; 3], EffectsError> {
    let Some(UniformValue::Vector(values)) = pass.uniforms.get(uniform) else {
        return match pass.uniforms.get(uniform) {
            None => Err(EffectsError::MissingUniform {
                shader: pass.shader.clone(),
                uniform: uniform.to_string(),
            }),
            Some(_) => Err(EffectsError::InvalidVectorUniform {
                shader: pass.shader.clone(),
                uniform: uniform.to_string(),
                expected_length: 3,
            }),
        };
    };
    if values.len() != 3 {
        return Err(EffectsError::InvalidVectorUniform {
            shader: pass.shader.clone(),
            uniform: uniform.to_string(),
            expected_length: 3,
        });
    }
    Ok([values[0], values[1], values[2]])
}

fn read_vec2_uniform(pass: &EffectPass, uniform: &str) -> Result<[f32; 2], EffectsError> {
    let Some(value) = pass.uniforms.get(uniform) else {
        return Err(EffectsError::MissingUniform {
            shader: pass.shader.clone(),
            uniform: uniform.to_string(),
        });
    };
    let UniformValue::Vector(values) = value else {
        return Err(EffectsError::InvalidVectorUniform {
            shader: pass.shader.clone(),
            uniform: uniform.to_string(),
            expected_length: 2,
        });
    };
    if values.len() != 2 {
        return Err(EffectsError::InvalidVectorUniform {
            shader: pass.shader.clone(),
            uniform: uniform.to_string(),
            expected_length: 2,
        });
    }
    Ok([values[0], values[1]])
}

#[cfg(test)]
mod tests {
    use super::{
        EffectUniformBuffer, SHADER_SOURCES, STORAGE_TABLE_SHADER_IDS, UNIFORM_TABLE_FLOATS,
        UNIFORM_TABLE_SHADER_IDS, pad_table, pad_uniform_table,
    };

    #[test]
    fn uniform_struct_is_sixteen_byte_aligned() {
        assert_eq!(std::mem::size_of::<EffectUniformBuffer>() % 16, 0);
    }

    #[test]
    fn tables_are_padded_to_whole_vec4_entries() {
        assert_eq!(pad_table(&[]).len(), 4);
        assert_eq!(pad_table(&[1.0, 2.0, 3.0]).len(), 4);
        assert_eq!(pad_table(&[1.0; 5]).len(), 8);
        assert_eq!(pad_table(&[1.0; 8]).len(), 8);
    }

    #[test]
    fn uniform_tables_are_padded_to_the_declared_length() {
        assert_eq!(pad_uniform_table(&[]).len(), UNIFORM_TABLE_FLOATS);
        assert_eq!(pad_uniform_table(&[1.0; 32]).len(), UNIFORM_TABLE_FLOATS);
        assert_eq!(pad_uniform_table(&[1.0; 32])[31], 1.0);
        assert_eq!(pad_uniform_table(&[1.0; 32])[32], 0.0);
    }

    #[test]
    fn table_shaders_declare_the_matching_binding() {
        let source_of = |shader_id: &str| {
            SHADER_SOURCES
                .iter()
                .find(|(id, _)| *id == shader_id)
                .unwrap_or_else(|| panic!("{shader_id} is missing from SHADER_SOURCES"))
                .1
        };
        for shader_id in UNIFORM_TABLE_SHADER_IDS {
            assert!(
                source_of(shader_id)
                    .contains("@group(2) @binding(0) var<uniform> table: array<vec4f, 256>;"),
                "{shader_id} does not bind a uniform lookup table"
            );
        }
        for shader_id in STORAGE_TABLE_SHADER_IDS {
            assert!(
                source_of(shader_id)
                    .contains("@group(2) @binding(0) var<storage, read> table: array<vec4f>;"),
                "{shader_id} does not bind a storage lookup table"
            );
        }
    }

    #[test]
    fn all_shaders_are_valid_wgsl() {
        for (shader_id, source) in SHADER_SOURCES {
            let module = naga::front::wgsl::parse_str(source)
                .unwrap_or_else(|error| panic!("{shader_id}: {error:?}"));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|error| panic!("{shader_id}: {error:?}"));
        }
    }
}
