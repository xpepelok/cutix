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

fn luma_of(color: vec3f) -> f32 {
    return dot(color, vec3f(0.299, 0.587, 0.114));
}

fn skin_weight(color: vec3f) -> f32 {
    let total = color.r + color.g + color.b + 0.0001;
    let red_ratio = color.r / total;
    let green_ratio = color.g / total;
    let in_red = smoothstep(0.32, 0.38, red_ratio) * (1.0 - smoothstep(0.48, 0.56, red_ratio));
    let in_green = smoothstep(0.26, 0.30, green_ratio) * (1.0 - smoothstep(0.38, 0.44, green_ratio));
    let bright = smoothstep(0.15, 0.25, luma_of(color));
    return clamp(in_red * in_green * bright, 0.0, 1.0);
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);
    let smooth_amount = clamp(uniforms.scalars.x, 0.0, 1.0);
    let radius = max(uniforms.scalars.y, 0.0);
    let tone_amount = clamp(uniforms.scalars.z, 0.0, 1.0);
    let brightness = uniforms.scalars.w;

    let texel = vec2f(1.0, 1.0) / uniforms.resolution;
    let center_luma = luma_of(source.rgb);
    let range_sigma = max(uniforms.extra.x, 0.0001);

    var accumulated = vec3f(0.0, 0.0, 0.0);
    var total_weight = 0.0;

    for (var y = -3; y <= 3; y = y + 1) {
        for (var x = -3; x <= 3; x = x + 1) {
            let offset = vec2f(f32(x), f32(y)) * texel * radius;
            let sample = textureSample(input_texture, input_sampler, input.tex_coord + offset);

            let spatial = exp(-(f32(x * x + y * y)) / 8.0);
            let difference = luma_of(sample.rgb) - center_luma;
            let range = exp(-(difference * difference) / (2.0 * range_sigma * range_sigma));
            let weight = spatial * range;

            accumulated = accumulated + sample.rgb * weight;
            total_weight = total_weight + weight;
        }
    }

    let blurred = accumulated / max(total_weight, 0.0001);
    let mask = skin_weight(source.rgb) * smooth_amount;
    var color = mix(source.rgb, blurred, mask);

    let warm = vec3f(1.03, 1.0, 0.97);
    color = mix(color, clamp(color * warm, vec3f(0.0), vec3f(1.0)), tone_amount * mask);
    color = clamp(color + brightness * mask, vec3f(0.0), vec3f(1.0));

    return vec4f(color, source.a);
}
