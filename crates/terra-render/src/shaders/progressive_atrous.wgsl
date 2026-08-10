struct FilterUniforms {
    // x = width, y = height, z = step size, w = variance scale
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: FilterUniforms;
@group(0) @binding(1) var input_color: texture_2d<f32>;
@group(0) @binding(2) var moments_tex: texture_2d<f32>;
@group(0) @binding(3) var depth_tex: texture_2d<f32>;

@vertex
fn vs_fullscreen(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((vertex_index << 1u) & 2u);
    let y = f32(vertex_index & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn kernel_weight(i: i32) -> f32 {
    let a = abs(i);
    return select(select(1.0, 4.0, a == 1), 6.0, a == 0);
}

@fragment
fn fs_atrous(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let size = vec2<i32>(i32(u.params.x), i32(u.params.y));
    let p = clamp(vec2<i32>(position.xy), vec2<i32>(0), size - vec2<i32>(1));
    let step_size = i32(u.params.z);
    let center = textureLoad(input_color, p, 0).rgb;
    let center_depth = textureLoad(depth_tex, p, 0).r;
    let moments = textureLoad(moments_tex, p, 0);
    let variance = max(moments.y - moments.x * moments.x, 1e-5);
    let sigma_l = sqrt(variance) * u.params.w + 0.0125;

    var total = vec3<f32>(0.0);
    var weight_sum = 0.0;
    for (var oy = -2; oy <= 2; oy = oy + 1) {
        for (var ox = -2; ox <= 2; ox = ox + 1) {
            let q = clamp(p + vec2<i32>(ox, oy) * step_size, vec2<i32>(0), size - vec2<i32>(1));
            let sample_color = textureLoad(input_color, q, 0).rgb;
            let sample_depth = textureLoad(depth_tex, q, 0).r;
            let spatial = kernel_weight(ox) * kernel_weight(oy);
            let depth_tol = max(abs(center_depth) * 0.02 + 0.002, 1e-5);
            let depth_weight = exp(-abs(sample_depth - center_depth) / depth_tol);
            let luma_weight = exp(-abs(luminance(sample_color) - luminance(center)) / sigma_l);
            let sky_guard = select(1.0, select(0.0, 1.0, sample_depth > 0.9999), center_depth > 0.9999);
            let weight = spatial * depth_weight * luma_weight * sky_guard;
            total += sample_color * weight;
            weight_sum += weight;
        }
    }
    return vec4<f32>(total / max(weight_sum, 1e-5), 1.0);
}
