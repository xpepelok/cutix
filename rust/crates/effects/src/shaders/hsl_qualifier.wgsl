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

const BAND_COUNT: u32 = 8u;

fn rgb_to_hsl(color: vec3f) -> vec3f {
    let high = max(color.r, max(color.g, color.b));
    let low = min(color.r, min(color.g, color.b));
    let chroma = high - low;
    let lightness = (high + low) * 0.5;

    if (chroma < 0.000001) {
        return vec3f(0.0, 0.0, lightness);
    }

    let saturation = chroma / (1.0 - abs(2.0 * lightness - 1.0) + 0.000001);
    var hue = 0.0;
    if (high == color.r) {
        hue = 60.0 * (((color.g - color.b) / chroma) % 6.0);
    } else if (high == color.g) {
        hue = 60.0 * (((color.b - color.r) / chroma) + 2.0);
    } else {
        hue = 60.0 * (((color.r - color.g) / chroma) + 4.0);
    }
    if (hue < 0.0) {
        hue = hue + 360.0;
    }
    return vec3f(hue, clamp(saturation, 0.0, 1.0), lightness);
}

fn hue_channel(p: f32, q: f32, offset: f32) -> f32 {
    var t = offset;
    if (t < 0.0) {
        t = t + 1.0;
    }
    if (t > 1.0) {
        t = t - 1.0;
    }
    if (t < 1.0 / 6.0) {
        return p + (q - p) * 6.0 * t;
    }
    if (t < 0.5) {
        return q;
    }
    if (t < 2.0 / 3.0) {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    return p;
}

fn hsl_to_rgb(hsl: vec3f) -> vec3f {
    let lightness = hsl.z;
    if (hsl.y < 0.000001) {
        return vec3f(lightness);
    }
    var q = lightness * (1.0 + hsl.y);
    if (lightness >= 0.5) {
        q = lightness + hsl.y - lightness * hsl.y;
    }
    let p = 2.0 * lightness - q;
    let hue = fract(hsl.x / 360.0);
    return vec3f(
        hue_channel(p, q, hue + 1.0 / 3.0),
        hue_channel(p, q, hue),
        hue_channel(p, q, hue - 1.0 / 3.0),
    );
}

fn signed_hue_delta(hue: f32, center: f32) -> f32 {
    let raw = hue - center;
    return raw - 360.0 * floor((raw + 180.0) / 360.0);
}

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);
    var centers = array<f32, 8>(0.0, 30.0, 60.0, 120.0, 180.0, 240.0, 270.0, 300.0);

    let hsl = rgb_to_hsl(clamp(source.rgb, vec3f(0.0), vec3f(1.0)));
    let gate = smoothstep(0.0, 0.08, hsl.y);
    if (gate <= 0.0) {
        return source;
    }

    var hue_shift = 0.0;
    var saturation_gain = 0.0;
    var luminance_shift = 0.0;

    for (var index = 0u; index < BAND_COUNT; index = index + 1u) {
        let center = centers[index];
        let delta = signed_hue_delta(hsl.x, center);
        var span = 0.0;
        if (delta < 0.0) {
            span = -signed_hue_delta(centers[(index + BAND_COUNT - 1u) % BAND_COUNT], center);
        } else {
            span = signed_hue_delta(centers[(index + 1u) % BAND_COUNT], center);
        }
        if (span <= 0.0) {
            continue;
        }
        let weight = max(0.0, 1.0 - abs(delta) / span);
        if (weight <= 0.0) {
            continue;
        }
        let band = table[index];
        hue_shift = hue_shift + weight * band.x;
        saturation_gain = saturation_gain + weight * band.y;
        luminance_shift = luminance_shift + weight * band.z;
    }

    let adjusted = vec3f(
        hsl.x + hue_shift * gate,
        clamp(hsl.y * (1.0 + saturation_gain * gate), 0.0, 1.0),
        clamp(hsl.z + luminance_shift * gate * 0.5, 0.0, 1.0),
    );
    let color = clamp(hsl_to_rgb(adjusted), vec3f(0.0), vec3f(1.0));
    return vec4f(color, source.a);
}
