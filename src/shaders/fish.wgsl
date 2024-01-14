struct CameraUniform {
    view_proj: mat4x4<f32>,
};

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) tex_coords: vec2<f32>,
    @location(2) normal: vec3<f32>,
};

struct InstanceInput {
    @builtin(instance_index) index: u32,

    // Model Matrix
    @location(5) model_matrix_0: vec4<f32>,
    @location(6) model_matrix_1: vec4<f32>,
    @location(7) model_matrix_2: vec4<f32>,
    @location(8) model_matrix_3: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
    @location(1) index: u32,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

@vertex
fn vs_main(
    model: VertexInput,
    instance: InstanceInput
) -> VertexOutput {
    let model_matrix = mat4x4<f32>(
        instance.model_matrix_0,
        instance.model_matrix_1,
        instance.model_matrix_2,
        instance.model_matrix_3,
    );

    var out: VertexOutput;
    out.tex_coords = model.tex_coords;
    out.clip_position = camera.view_proj * model_matrix * vec4<f32>(model.position, 1.0);
    out.index = instance.index;
    return out;
}

// === Fragment ===

@group(1) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(1)
var s_diffuse: sampler;

@group(2) @binding(0)
var<storage, read> tints: array<vec3<f32>, 50>;

const TINT = 0.05;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tnt_color = tints[in.index] * TINT;
    let tex_color = textureSample(t_diffuse, s_diffuse, in.tex_coords);
    let fin_color = tex_color.xyz * (1.0 - TINT) + tnt_color;
    return vec4(fin_color, 1.0);
}