// Porter-Duff "src over dst" alpha compositing.
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) vertex_idx: u32) -> VertexOutput {
    var positions = array<vec2<f32>, 6>(
        vec2<f32>(-1.0,  1.0), vec2<f32>( 1.0,  1.0), vec2<f32>(-1.0, -1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>( 1.0,  1.0), vec2<f32>( 1.0, -1.0),
    );
    var uvs = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(0.0, 1.0), vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0),
    );
    var out: VertexOutput;
    out.position = vec4<f32>(positions[vertex_idx], 0.0, 1.0);
    out.uv = uvs[vertex_idx];
    return out;
}

@group(0) @binding(0) var tex_base:    texture_2d<f32>;
@group(0) @binding(1) var tex_overlay: texture_2d<f32>;
@group(0) @binding(2) var tex_sampler: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let base    = textureSample(tex_base,    tex_sampler, in.uv);
    let overlay = textureSample(tex_overlay, tex_sampler, in.uv);
    // src over dst: out_rgb = overlay.rgb * overlay.a + base.rgb * (1 - overlay.a)
    let alpha_ov = overlay.a;
    let out_rgb  = overlay.rgb * alpha_ov + base.rgb * (1.0 - alpha_ov);
    let out_a    = alpha_ov + base.a * (1.0 - alpha_ov);
    return vec4<f32>(out_rgb, out_a);
}
