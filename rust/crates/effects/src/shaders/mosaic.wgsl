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

@fragment
fn fragment_main(input: VertexOutput) -> @location(0) vec4f {
    let source = textureSample(input_texture, input_sampler, input.tex_coord);

    let block_size = max(uniforms.scalars.x, 1.0);
    let region_min = vec2f(uniforms.scalars.y, uniforms.scalars.z);
    let region_max = region_min + vec2f(uniforms.scalars.w, uniforms.extra.x);
    let shape = uniforms.extra.y;

    let resolution = max(uniforms.resolution, vec2f(1.0, 1.0));
    let pixel = input.tex_coord * resolution;
    let snapped = (floor(pixel / block_size) + vec2f(0.5)) * block_size;
    let mosaic = textureSample(input_texture, input_sampler, snapped / resolution);

    var inside = 0.0;
    if (shape < 0.5) {
        let in_x = step(region_min.x, input.tex_coord.x) * step(input.tex_coord.x, region_max.x);
        let in_y = step(region_min.y, input.tex_coord.y) * step(input.tex_coord.y, region_max.y);
        inside = in_x * in_y;
    } else {
        let center = (region_min + region_max) * 0.5;
        let radius = max((region_max - region_min) * 0.5, vec2f(0.0001));
        let normalized = (input.tex_coord - center) / radius;
        inside = 1.0 - step(1.0, length(normalized));
    }

    return vec4f(mix(source.rgb, mosaic.rgb, inside), source.a);
}
