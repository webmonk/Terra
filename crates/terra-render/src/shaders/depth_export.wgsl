// Copy hardware depth (Depth32Float) into an R32Float color target for progressive sampling.

@group(0) @binding(0) var depth_tex: texture_depth_2d;

@vertex
fn vs_fullscreen(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((vertex_index << 1u) & 2u);
    let y = f32(vertex_index & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

@fragment
fn fs_export(@builtin(position) position: vec4<f32>) -> @location(0) f32 {
    return textureLoad(depth_tex, vec2<i32>(position.xy), 0);
}
