#![cfg(not(target_arch = "wasm32"))]

use std::collections::HashMap;

use effects::{ApplyEffectsOptions, EffectPass, EffectPipeline, UniformValue};
use gpu::{GpuContext, wgpu};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;

struct Harness {
    context: GpuContext,
    pipeline: EffectPipeline,
}

impl Harness {
    fn new() -> Option<Self> {
        let context = pollster::block_on(GpuContext::new()).ok()?;
        let pipeline = EffectPipeline::new(&context);
        Some(Self { context, pipeline })
    }

    fn run(&self, source: &[u8], passes: &[EffectPass]) -> Vec<u8> {
        let texture = self
            .context
            .create_render_texture(WIDTH, HEIGHT, "test-source");
        self.context.queue().write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &to_native(source, self.context.texture_format()),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(WIDTH * 4),
                rows_per_image: Some(HEIGHT),
            },
            wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
        );

        let output = self
            .pipeline
            .apply(
                &self.context,
                ApplyEffectsOptions {
                    source: &texture,
                    width: WIDTH,
                    height: HEIGHT,
                    passes,
                },
            )
            .expect("effect passes applied");
        to_native(&self.read_back(&output), self.context.texture_format())
    }

    fn read_back(&self, texture: &wgpu::Texture) -> Vec<u8> {
        let size = (WIDTH * HEIGHT * 4) as u64;
        let buffer = self
            .context
            .device()
            .create_buffer(&wgpu::BufferDescriptor {
                label: Some("test-readback"),
                size,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });
        let mut encoder =
            self.context
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("test-readback-encoder"),
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
                    bytes_per_row: Some(WIDTH * 4),
                    rows_per_image: Some(HEIGHT),
                },
            },
            wgpu::Extent3d {
                width: WIDTH,
                height: HEIGHT,
                depth_or_array_layers: 1,
            },
        );
        self.context.queue().submit([encoder.finish()]);

        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        self.context
            .device()
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("device polled");
        let data = slice.get_mapped_range().to_vec();
        buffer.unmap();
        data
    }
}

fn to_native(rgba: &[u8], format: wgpu::TextureFormat) -> Vec<u8> {
    if format == wgpu::TextureFormat::Rgba8Unorm {
        return rgba.to_vec();
    }
    let mut out = rgba.to_vec();
    for pixel in out.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    out
}

fn pass(shader: &str, uniforms: &[(&str, UniformValue)]) -> EffectPass {
    EffectPass {
        shader: shader.to_string(),
        uniforms: uniforms
            .iter()
            .map(|(name, value)| ((*name).to_string(), value.clone()))
            .collect::<HashMap<_, _>>(),
        data_id: None,
    }
}

fn gradient_image() -> Vec<u8> {
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for _ in 0..HEIGHT {
        for x in 0..WIDTH {
            let level = (x * 255 / (WIDTH - 1)) as u8;
            pixels.extend_from_slice(&[level, level, level, 255]);
        }
    }
    pixels
}

fn identity_curve_table() -> Vec<f32> {
    let mut table = vec![0.0f32; 256 * 4];
    for i in 0..256 {
        let value = i as f32 / 255.0;
        table[i * 4] = value;
        table[i * 4 + 1] = value;
        table[i * 4 + 2] = value;
    }
    table
}

fn s_curve_table() -> Vec<f32> {
    let mut table = vec![0.0f32; 256 * 4];
    for i in 0..256 {
        let x = i as f32 / 255.0;
        let y = (x * x * (3.0 - 2.0 * x)).clamp(0.0, 1.0);
        table[i * 4] = y;
        table[i * 4 + 1] = y;
        table[i * 4 + 2] = y;
    }
    table
}

fn channel_stats(pixels: &[u8], channel: usize) -> (f64, f64) {
    let values: Vec<f64> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[channel] as f64)
        .collect();
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance =
        values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / values.len() as f64;
    (mean, variance.sqrt())
}

fn pixel_at(pixels: &[u8], x: u32, y: u32) -> [u8; 4] {
    let index = ((y * WIDTH + x) * 4) as usize;
    [
        pixels[index],
        pixels[index + 1],
        pixels[index + 2],
        pixels[index + 3],
    ]
}

macro_rules! harness_or_skip {
    () => {
        match Harness::new() {
            Some(harness) => harness,
            None => {
                eprintln!("skipping: no GPU adapter available");
                return;
            }
        }
    };
}

#[test]
fn identity_curve_table_reproduces_the_source() {
    let harness = harness_or_skip!();
    let source = gradient_image();
    let output = harness.run(
        &source,
        &[pass(
            "curves",
            &[
                ("u_amount", UniformValue::Number(1.0)),
                ("u_table", UniformValue::Vector(identity_curve_table())),
            ],
        )],
    );
    assert_eq!(output, source, "identity curve changed the image");
}

#[test]
fn s_curve_increases_contrast() {
    let harness = harness_or_skip!();
    let source = gradient_image();
    let output = harness.run(
        &source,
        &[pass(
            "curves",
            &[
                ("u_amount", UniformValue::Number(1.0)),
                ("u_table", UniformValue::Vector(s_curve_table())),
            ],
        )],
    );
    let (before_mean, before_sd) = channel_stats(&source, 1);
    let (after_mean, after_sd) = channel_stats(&output, 1);
    eprintln!(
        "s-curve: mean {before_mean:.2} -> {after_mean:.2}, sd {before_sd:.2} -> {after_sd:.2}"
    );
    assert!(
        after_sd > before_sd * 1.15,
        "contrast did not increase: {before_sd} -> {after_sd}"
    );
    assert!((after_mean - before_mean).abs() < 6.0, "mean drifted");
    assert!(pixel_at(&output, 8, 0)[1] < pixel_at(&source, 8, 0)[1]);
    assert!(pixel_at(&output, 56, 0)[1] > pixel_at(&source, 56, 0)[1]);
}

fn primaries_image() -> Vec<u8> {
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for _ in 0..HEIGHT {
        for x in 0..WIDTH {
            let color = match x * 3 / WIDTH {
                0 => [220u8, 30, 30],
                1 => [30, 200, 30],
                _ => [30, 30, 210],
            };
            pixels.extend_from_slice(&[color[0], color[1], color[2], 255]);
        }
    }
    pixels
}

fn hsl_table(band_index: usize, values: [f32; 3]) -> Vec<f32> {
    let mut table = vec![0.0f32; 8 * 4];
    table[band_index * 4] = values[0];
    table[band_index * 4 + 1] = values[1];
    table[band_index * 4 + 2] = values[2];
    table
}

#[test]
fn hsl_qualifier_only_moves_the_targeted_band() {
    let harness = harness_or_skip!();
    let source = primaries_image();
    let output = harness.run(
        &source,
        &[pass(
            "hsl-qualifier",
            &[(
                "u_table",
                UniformValue::Vector(hsl_table(5, [0.0, -0.8, 0.3])),
            )],
        )],
    );

    let red_in = pixel_at(&source, 10, 32);
    let red_out = pixel_at(&output, 10, 32);
    let green_in = pixel_at(&source, 32, 32);
    let green_out = pixel_at(&output, 32, 32);
    let blue_in = pixel_at(&source, 54, 32);
    let blue_out = pixel_at(&output, 54, 32);
    eprintln!("hsl red {red_in:?} -> {red_out:?}");
    eprintln!("hsl green {green_in:?} -> {green_out:?}");
    eprintln!("hsl blue {blue_in:?} -> {blue_out:?}");

    assert_eq!(red_in, red_out, "red band moved");
    assert_eq!(green_in, green_out, "green band moved");
    let delta = blue_out
        .iter()
        .zip(blue_in.iter())
        .map(|(a, b)| (*a as i32 - *b as i32).abs())
        .max()
        .unwrap_or(0);
    assert!(delta > 40, "blue band barely moved ({delta})");
}

#[test]
fn neutral_hsl_table_is_a_no_op() {
    let harness = harness_or_skip!();
    let source = primaries_image();
    let output = harness.run(
        &source,
        &[pass(
            "hsl-qualifier",
            &[("u_table", UniformValue::Vector(vec![0.0; 32]))],
        )],
    );
    assert_eq!(output, source);
}

fn channel_swap_lut() -> Vec<f32> {
    let mut table = vec![0.0f32; 8 * 4];
    for index in 0..8usize {
        let r = (index & 1) as f32;
        let g = ((index >> 1) & 1) as f32;
        let b = ((index >> 2) & 1) as f32;
        table[index * 4] = b;
        table[index * 4 + 1] = g;
        table[index * 4 + 2] = r;
    }
    table
}

#[test]
fn lut3d_applies_a_channel_swap_exactly() {
    let harness = harness_or_skip!();
    let mut source = Vec::new();
    for _ in 0..HEIGHT {
        for x in 0..WIDTH {
            let level = (x * 255 / (WIDTH - 1)) as u8;
            source.extend_from_slice(&[level, 128, 255 - level, 255]);
        }
    }
    let output = harness.run(
        &source,
        &[pass(
            "lut3d",
            &[
                ("u_size", UniformValue::Number(2.0)),
                ("u_intensity", UniformValue::Number(1.0)),
                ("u_table", UniformValue::Vector(channel_swap_lut())),
            ],
        )],
    );

    for x in [0u32, 16, 32, 48, 63] {
        let input = pixel_at(&source, x, 10);
        let result = pixel_at(&output, x, 10);
        eprintln!("lut x={x}: {input:?} -> {result:?}");
        assert!(
            (result[0] as i32 - input[2] as i32).abs() <= 1,
            "red should equal source blue at x={x}"
        );
        assert!(
            (result[2] as i32 - input[0] as i32).abs() <= 1,
            "blue should equal source red at x={x}"
        );
        assert!((result[1] as i32 - input[1] as i32).abs() <= 1);
    }
}

#[test]
fn lut3d_at_zero_intensity_is_a_no_op() {
    let harness = harness_or_skip!();
    let source = primaries_image();
    let output = harness.run(
        &source,
        &[pass(
            "lut3d",
            &[
                ("u_size", UniformValue::Number(2.0)),
                ("u_intensity", UniformValue::Number(0.0)),
                ("u_table", UniformValue::Vector(channel_swap_lut())),
            ],
        )],
    );
    assert_eq!(output, source);
}

fn stripes_image() -> Vec<u8> {
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for _ in 0..HEIGHT {
        for x in 0..WIDTH {
            let level = if x % 4 < 2 { 20u8 } else { 235u8 };
            pixels.extend_from_slice(&[level, level, level, 255]);
        }
    }
    pixels
}

fn local_variance(pixels: &[u8], x0: u32, x1: u32) -> f64 {
    let mut values = Vec::new();
    for y in 0..HEIGHT {
        for x in x0..x1 {
            values.push(pixel_at(pixels, x, y)[1] as f64);
        }
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / values.len() as f64
}

#[test]
fn background_blur_passes_destroy_high_frequency_detail() {
    let harness = harness_or_skip!();
    let source = stripes_image();
    let blur = |direction: [f32; 2]| {
        pass(
            "gaussian-blur",
            &[
                ("u_sigma", UniformValue::Number(6.0)),
                ("u_step", UniformValue::Number(1.0)),
                ("u_direction", UniformValue::Vector(direction.to_vec())),
            ],
        )
    };
    let output = harness.run(&source, &[blur([1.0, 0.0]), blur([0.0, 1.0])]);

    let before = local_variance(&source, 0, WIDTH);
    let after = local_variance(&output, 0, WIDTH);
    eprintln!("blur variance: {before:.1} -> {after:.1}");
    assert!(
        after < before * 0.05,
        "detail survived: {before} -> {after}"
    );
}

#[test]
fn shared_tables_resolve_across_pipelines() {
    let Some(harness) = Harness::new() else {
        eprintln!("skipping: no GPU adapter available");
        return;
    };
    let mut consumer = EffectPipeline::new(&harness.context);
    consumer.share_tables_from(&harness.pipeline);

    harness
        .pipeline
        .register_data(&harness.context, "swap".to_string(), &channel_swap_lut());

    assert!(
        consumer.has_data("swap"),
        "a table registered on one pipeline is invisible to the pipeline sharing its registry"
    );
}

fn flat_image(level: u8) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for _ in 0..WIDTH * HEIGHT {
        pixels.extend_from_slice(&[level, level, level, 255]);
    }
    pixels
}

fn adjustment_pass(name: &str, value: f32) -> EffectPass {
    let keys = [
        "u_brightness",
        "u_contrast",
        "u_exposure",
        "u_saturation",
        "u_vibrance",
        "u_temperature",
        "u_tint",
        "u_highlights",
        "u_shadows",
        "u_sharpness",
    ];
    let uniforms: Vec<(&str, UniformValue)> = keys
        .iter()
        .map(|key| {
            (
                *key,
                UniformValue::Number(if *key == name { value } else { 0.0 }),
            )
        })
        .collect();
    pass("adjustment", &uniforms)
}

#[test]
fn swapping_two_effects_changes_the_result_predictably() {
    let harness = harness_or_skip!();
    let source = flat_image(128);
    let warm = adjustment_pass("u_temperature", 0.5);
    let grey = adjustment_pass("u_saturation", -1.0);

    let warm_then_grey = harness.run(&source, &[warm.clone(), grey.clone()]);
    let grey_then_warm = harness.run(&source, &[grey, warm]);

    let a = pixel_at(&warm_then_grey, 32, 32);
    let b = pixel_at(&grey_then_warm, 32, 32);

    assert!(
        a[0].abs_diff(a[2]) <= 1,
        "expected a neutral pixel, got {a:?}"
    );

    let spread = b[0] as i32 - b[2] as i32;
    let expected = (2.0f64 * 0.12 * 0.5 * 255.0).round() as i32;
    assert!(
        (spread - expected).abs() <= 2,
        "expected a spread near {expected}, got {b:?} (spread {spread})"
    );
    assert_ne!(a, b, "the two orders must not produce the same pixel");
}
