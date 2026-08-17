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
@group(2) @binding(0) var<storage, read> table: array<vec4f>;

fn lut_entry(coord: vec3<u32>, size: u32) -> vec3f {
    let index = coord.x + coord.y * size + coord.z * size * size;
    return table[index].xyz;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);
    let size = max(u32(uniforms.scalars.x), 2u);
    let last = size - 1u;
    let scaled = clamp(source.rgb, vec3f(0.0), vec3f(1.0)) * f32(last);
    let base = floor(scaled);
    let fraction = scaled - base;
    let low = vec3<u32>(base);
    let high = min(low + vec3<u32>(1u), vec3<u32>(last));

    let c000 = lut_entry(vec3<u32>(low.x, low.y, low.z), size);
    let c100 = lut_entry(vec3<u32>(high.x, low.y, low.z), size);
    let c010 = lut_entry(vec3<u32>(low.x, high.y, low.z), size);
    let c110 = lut_entry(vec3<u32>(high.x, high.y, low.z), size);
    let c001 = lut_entry(vec3<u32>(low.x, low.y, high.z), size);
    let c101 = lut_entry(vec3<u32>(high.x, low.y, high.z), size);
    let c011 = lut_entry(vec3<u32>(low.x, high.y, high.z), size);
    let c111 = lut_entry(vec3<u32>(high.x, high.y, high.z), size);

    let c00 = mix(c000, c100, fraction.x);
    let c10 = mix(c010, c110, fraction.x);
    let c01 = mix(c001, c101, fraction.x);
    let c11 = mix(c011, c111, fraction.x);
    let c0 = mix(c00, c10, fraction.y);
    let c1 = mix(c01, c11, fraction.y);
    let graded = mix(c0, c1, fraction.z);

    let amount = clamp(uniforms.scalars.y, 0.0, 1.0);
    let color = mix(source.rgb, graded, amount);
    return vec4f(clamp(color, vec3f(0.0), vec3f(1.0)), source.a);
}
