struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) tex_coord: vec2f,
}

struct EffectUniforms {
    resolution: vec2f,
    direction: vec2f,
    scalars: vec4f,
    extra: vec4f,
    extra2: vec4f,
}

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var input_sampler: sampler;
@group(1) @binding(0) var<uniform> uniforms: EffectUniforms;
@group(2) @binding(0) var<uniform> table: array<vec4f, 256>;

fn curve_at(value: f32) -> vec3f {
    let position = clamp(value, 0.0, 1.0) * 255.0;
    let lower = floor(position);
    let low_index = u32(lower);
    let high_index = min(low_index + 1u, 255u);
    return mix(table[low_index].xyz, table[high_index].xyz, position - lower);
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);
    let graded = vec3f(
        curve_at(source.r).x,
        curve_at(source.g).y,
        curve_at(source.b).z,
    );
    let amount = clamp(uniforms.scalars.x, 0.0, 1.0);
    let color = mix(source.rgb, graded, amount);
    return vec4f(clamp(color, vec3f(0.0), vec3f(1.0)), source.a);
}
