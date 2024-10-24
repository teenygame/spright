@group(0) @binding(0)
var t: texture_2d<f32>;
@group(0) @binding(1)
var s: sampler;

@group(1) @binding(0)
var<uniform> screen_size: vec2<f32>;

@group(2) @binding(0)
var<uniform> texture_size: vec2<f32>;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) tint: vec4<f32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) tint: vec4<f32>,
};

@vertex
fn vs_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    out.tint = model.tint;

    // Normalize pixel texture position to texture coordinates.
    out.tex_coords = model.tex_coords / texture_size;

    // Normalize screen position to NDC position.
    var pos = model.position.xy / screen_size - 1.0;
    pos.y = -pos.y;

    out.position = vec4<f32>(pos, model.position.z, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t, s, in.tex_coords) * in.tint;
}
