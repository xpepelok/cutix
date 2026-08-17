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

fn luma_of(color: vec3f) -> f32 {
    return dot(color, vec3f(0.2126, 0.7152, 0.0722));
}

fn sharpened(tex_coord: vec2f, amount: f32) -> vec3f {
    let center = textureSample(input_texture, input_sampler, tex_coord).rgb;
    if (abs(amount) < 0.0001) {
        return center;
    }
    let texel = vec2f(1.0, 1.0) / max(uniforms.resolution, vec2f(1.0, 1.0));
    var blurred = vec3f(0.0);
    var weight_sum = 0.0;
    for (var y = -1; y <= 1; y = y + 1) {
        for (var x = -1; x <= 1; x = x + 1) {
            let weight = select(select(1.0, 2.0, x == 0 || y == 0), 4.0, x == 0 && y == 0);
            let offset = vec2f(f32(x), f32(y)) * texel;
            blurred = blurred + textureSample(input_texture, input_sampler, tex_coord + offset).rgb * weight;
            weight_sum = weight_sum + weight;
        }
    }
    blurred = blurred / weight_sum;
    return center + (center - blurred) * amount * 3.0;
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);

    let brightness = uniforms.scalars.x;
    let contrast = uniforms.scalars.y;
    let saturation = uniforms.scalars.z;
    let exposure = uniforms.scalars.w;
    let temperature = uniforms.extra.x;
    let tint = uniforms.extra.y;
    let highlights = uniforms.extra.z;
    let shadows = uniforms.extra.w;
    let vibrance = uniforms.extra2.x;
    let sharpness = uniforms.extra2.y;

    var color = sharpened(input.tex_coord, sharpness);

    color = color * exp2(exposure);
    color = color + brightness;
    color = (color - vec3f(0.5)) * (1.0 + contrast) + vec3f(0.5);

    color.r = color.r + temperature * 0.12;
    color.b = color.b - temperature * 0.12;
    color.g = color.g + tint * 0.12;
    color.r = color.r - tint * 0.06;
    color.b = color.b - tint * 0.06;

    color = clamp(color, vec3f(0.0), vec3f(1.0));

    let luminance = luma_of(color);
    let highlight_mask = smoothstep(0.4, 1.0, luminance);
    let shadow_mask = 1.0 - smoothstep(0.0, 0.6, luminance);
    color = color + highlights * 0.5 * highlight_mask;
    color = color + shadows * 0.5 * shadow_mask;
    color = clamp(color, vec3f(0.0), vec3f(1.0));

    let gray = vec3f(luma_of(color));
    color = mix(gray, color, 1.0 + saturation);

    let max_channel = max(color.r, max(color.g, color.b));
    let min_channel = min(color.r, min(color.g, color.b));
    let current_saturation = max_channel - min_channel;
    let vibrance_weight = 1.0 - current_saturation;
    color = mix(vec3f(luma_of(color)), color, 1.0 + vibrance * vibrance_weight);

    color = clamp(color, vec3f(0.0), vec3f(1.0));

    return vec4f(color, source.a);
}
