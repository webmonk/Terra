struct ProgressiveUniforms {
    inv_view_proj: mat4x4<f32>,
    prev_view_proj: mat4x4<f32>,
    // xy = resolution, z = history valid, w = frame index
    resolution: vec4<f32>,
    // x = max history, y = depth threshold, zw reserved
    params: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: ProgressiveUniforms;
@group(0) @binding(1) var scene_tex: texture_2d<f32>;
@group(0) @binding(2) var scene_depth: texture_depth_2d;
@group(0) @binding(3) var history_color: texture_2d<f32>;
@group(0) @binding(4) var history_moments: texture_2d<f32>;
@group(0) @binding(5) var history_depth: texture_2d<f32>;

struct FsOut {
    @location(0) color: vec4<f32>,
    @location(1) moments: vec4<f32>,
    @location(2) depth: f32,
};

@vertex
fn vs_fullscreen(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((vertex_index << 1u) & 2u);
    let y = f32(vertex_index & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

@fragment
fn fs_temporal(@builtin(position) position: vec4<f32>) -> FsOut {
    let size = vec2<i32>(i32(u.resolution.x), i32(u.resolution.y));
    let p = clamp(vec2<i32>(position.xy), vec2<i32>(0), size - vec2<i32>(1));
    let current = textureLoad(scene_tex, p, 0);
    let depth = textureLoad(scene_depth, p, 0);

    var min_color = current.rgb;
    var max_color = current.rgb;
    for (var oy = -1; oy <= 1; oy = oy + 1) {
        for (var ox = -1; ox <= 1; ox = ox + 1) {
            let q = clamp(p + vec2<i32>(ox, oy), vec2<i32>(0), size - vec2<i32>(1));
            let c = textureLoad(scene_tex, q, 0).rgb;
            min_color = min(min_color, c);
            max_color = max(max_color, c);
        }
    }

    let uv = (vec2<f32>(p) + vec2<f32>(0.5)) / u.resolution.xy;
    var previous_uv = uv;
    var expected_depth = depth;
    if (depth < 0.999999) {
        let ndc = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, depth, 1.0);
        let world_h = u.inv_view_proj * ndc;
        let world = world_h / max(abs(world_h.w), 1e-7);
        let previous_clip = u.prev_view_proj * vec4<f32>(world.xyz, 1.0);
        let previous_ndc = previous_clip.xyz / max(abs(previous_clip.w), 1e-7);
        previous_uv = vec2<f32>(previous_ndc.x * 0.5 + 0.5, 0.5 - previous_ndc.y * 0.5);
        expected_depth = previous_ndc.z;
    }

    let inside = all(previous_uv >= vec2<f32>(0.0)) && all(previous_uv <= vec2<f32>(1.0));
    let previous_p = clamp(vec2<i32>(previous_uv * u.resolution.xy), vec2<i32>(0), size - vec2<i32>(1));
    let old_depth = textureLoad(history_depth, previous_p, 0).r;
    let depth_limit = u.params.y * (1.0 + abs(expected_depth) * 4.0);
    let depth_ok = abs(old_depth - expected_depth) <= depth_limit;
    let history_ok = u.resolution.z > 0.5 && inside && depth_ok;

    let old_moments = textureLoad(history_moments, previous_p, 0);
    let previous_count = select(0.0, min(old_moments.z, u.params.x), history_ok);
    let alpha = 1.0 / (previous_count + 1.0);
    let old_color = clamp(textureLoad(history_color, previous_p, 0).rgb, min_color, max_color);
    let accumulated = mix(old_color, current.rgb, select(1.0, alpha, history_ok));

    let l = luminance(current.rgb);
    let moments = mix(vec2<f32>(old_moments.x, old_moments.y), vec2<f32>(l, l * l), select(1.0, alpha, history_ok));

    var out: FsOut;
    out.color = vec4<f32>(accumulated, 1.0);
    out.moments = vec4<f32>(moments, previous_count + 1.0, 0.0);
    out.depth = depth;
    return out;
}
