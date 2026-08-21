struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) tex_coord: vec2f,
}

struct EffectUniforms {
    resolution: vec2f,
    direction: vec2f,
    scalars: vec4f,
    extra: vec4f,
}

@group(0) @binding(0) var input_texture: texture_2d<f32>;
@group(0) @binding(1) var input_sampler: sampler;
@group(1) @binding(0) var<uniform> uniforms: EffectUniforms;

fn chroma_of(color: vec3f) -> vec2f {
    let u = dot(color, vec3f(-0.168736, -0.331264, 0.5));
    let v = dot(color, vec3f(0.5, -0.418688, -0.081312));
    return vec2f(u, v);
}

fn luma_of(color: vec3f) -> f32 {
    return dot(color, vec3f(0.299, 0.587, 0.114));
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);

    let key_color = uniforms.scalars.xyz;
    let similarity = max(uniforms.scalars.w, 0.0001);
    let smoothness = max(uniforms.extra.x, 0.0001);
    let spill_strength = clamp(uniforms.extra.y, 0.0, 1.0);

    let key_chroma = chroma_of(key_color);
    let pixel_chroma = chroma_of(source.rgb);
    let chroma_distance = distance(pixel_chroma, key_chroma);

    let alpha = smoothstep(similarity, similarity + smoothness, chroma_distance);

    var color = source.rgb;
    if (spill_strength > 0.0) {
        let spill = clamp(1.0 - chroma_distance / (similarity + smoothness), 0.0, 1.0);
        let neutral = vec3f(luma_of(color));
        color = mix(color, neutral, spill * spill_strength);
    }

    return vec4f(color * alpha, source.a * alpha);
}
