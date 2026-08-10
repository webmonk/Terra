struct CompositeUniforms {
    mode: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
};

@group(0) @binding(0) var<uniform> u: CompositeUniforms;
@group(0) @binding(1) var filtered_tex: texture_2d<f32>;
@group(0) @binding(2) var scene_tex: texture_2d<f32>;
@group(0) @binding(3) var moments_tex: texture_2d<f32>;
@group(0) @binding(4) var history_tex: texture_2d<f32>;

@vertex
fn vs_fullscreen(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((vertex_index << 1u) & 2u);
    let y = f32(vertex_index & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

@fragment
fn fs_composite(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let p = vec2<i32>(position.xy);
    switch (u.mode) {
        case 1u: {
            // Noisy scene input (before temporal)
            return textureLoad(scene_tex, p, 0);
        }
        case 2u: {
            // Moments.y (second moment) as variance proxy grayscale
            let m = textureLoad(moments_tex, p, 0);
            let variance = max(m.y - m.x * m.x, 0.0);
            let g = clamp(sqrt(variance) * 8.0, 0.0, 1.0);
            return vec4<f32>(g, g, g, 1.0);
        }
        case 3u: {
            // History length / sample count
            let m = textureLoad(moments_tex, p, 0);
            let g = clamp(m.z / 64.0, 0.0, 1.0);
            return vec4<f32>(g, g, g, 1.0);
        }
        case 4u: {
            // Temporal history color
            return textureLoad(history_tex, p, 0);
        }
        default: {
            return textureLoad(filtered_tex, p, 0);
        }
    }
}
