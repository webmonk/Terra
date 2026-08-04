struct FrameUniforms {
    view_proj: mat4x4<f32>,
    // xyz = light direction (from light toward scene), w = sun intensity
    light_dir: vec4<f32>,
    // x=world_size_x, y=world_size_z, z=height_min, w=height_max
    world: vec4<f32>,
    // x=tex_w, y=tex_h, z=unused, w=unused
    grid: vec4<f32>,
    // x=origin_x, y=origin_z, z=spacing, w=grid_size
    clipmap: vec4<f32>,
    // xyz = camera eye, w = exposure
    eye: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: FrameUniforms;
@group(0) @binding(1) var height_tex: texture_2d<f32>;
@group(0) @binding(2) var normal_tex: texture_2d<f32>;
@group(0) @binding(3) var normal_samp: sampler;
@group(0) @binding(4) var materials_tex: texture_2d<f32>;
@group(0) @binding(5) var wetness_tex: texture_2d<f32>;
@group(0) @binding(6) var vegetation_tex: texture_2d<f32>;

struct VsIn {
    @location(0) uv: vec2<f32>,
};

struct VsOut {
    @builtin(position) position: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) terrain_uv: vec2<f32>,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var o: VsOut;
    let cells = max(u.clipmap.w - 1.0, 1.0);
    let wx = u.clipmap.x + v.uv.x * cells * u.clipmap.z;
    let wz = u.clipmap.y + v.uv.y * cells * u.clipmap.z;
    // Unclamped UV — FS discards outside the heightfield so LOD skirts don't form a
    // sliding "apron" when the camera pans past the world edge.
    let huv_raw = vec2<f32>(
        wx / max(u.world.x, 1e-3),
        wz / max(u.world.y, 1e-3),
    );
    let huv = clamp(huv_raw, vec2<f32>(0.0), vec2<f32>(1.0));

    // Bilinear height sample via textureLoad (R32Float is typically non-filterable).
    let tw = max(u.grid.x - 1.0, 1.0);
    let th = max(u.grid.y - 1.0, 1.0);
    let fx = huv.x * tw;
    let fy = huv.y * th;
    let x0 = i32(floor(fx));
    let y0 = i32(floor(fy));
    let x1 = min(x0 + 1, i32(tw));
    let y1 = min(y0 + 1, i32(th));
    let tx = fx - f32(x0);
    let ty = fy - f32(y0);
    let h00 = textureLoad(height_tex, vec2<i32>(x0, y0), 0).r;
    let h10 = textureLoad(height_tex, vec2<i32>(x1, y0), 0).r;
    let h01 = textureLoad(height_tex, vec2<i32>(x0, y1), 0).r;
    let h11 = textureLoad(height_tex, vec2<i32>(x1, y1), 0).r;
    let h = mix(mix(h00, h10, tx), mix(h01, h11, tx), ty);

    let world_pos = vec3<f32>(wx, h, wz);
    o.position = u.view_proj * vec4<f32>(world_pos, 1.0);
    o.world_pos = world_pos;
    // Prefer textureLoad in VS for broader driver compatibility.
    let ntex = textureDimensions(normal_tex);
    let nx = i32(clamp(huv.x * f32(ntex.x - 1u), 0.0, f32(ntex.x - 1u)));
    let ny = i32(clamp(huv.y * f32(ntex.y - 1u), 0.0, f32(ntex.y - 1u)));
    o.normal = textureLoad(normal_tex, vec2<i32>(nx, ny), 0).xyz;
    o.terrain_uv = huv_raw;
    return o;
}

fn sample_map(map: texture_2d<f32>, uv: vec2<f32>) -> f32 {
    let dim = textureDimensions(map);
    let x = i32(clamp(uv.x * f32(dim.x - 1u), 0.0, f32(dim.x - 1u)));
    let y = i32(clamp(uv.y * f32(dim.y - 1u), 0.0, f32(dim.y - 1u)));
    return textureLoad(map, vec2<i32>(x, y), 0).r;
}

fn sample_height_uv(uv: vec2<f32>) -> f32 {
    let dim = textureDimensions(height_tex);
    let x = i32(clamp(uv.x * f32(dim.x - 1u), 0.0, f32(dim.x - 1u)));
    let y = i32(clamp(uv.y * f32(dim.y - 1u), 0.0, f32(dim.y - 1u)));
    return textureLoad(height_tex, vec2<i32>(x, y), 0).r;
}

fn material_color(id: u32) -> vec3<f32> {
    // Neutral editor palette — rock/dirt/grass/sand/snow without blowing out.
    switch (id % 5u) {
        case 0u: { return vec3<f32>(0.42, 0.40, 0.36); } // rock
        case 1u: { return vec3<f32>(0.48, 0.36, 0.24); } // dirt
        case 2u: { return vec3<f32>(0.30, 0.42, 0.22); } // grass
        case 3u: { return vec3<f32>(0.62, 0.54, 0.36); } // sand
        default: { return vec3<f32>(0.78, 0.80, 0.82); } // snow
    }
}

fn hash21(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

/// Local height-field AO — samples a few texels as a cheap GI occlusion term.
/// Offsets are unrolled (no WGSL array ctors — some NVIDIA drivers crash on those).
fn height_ao(uv: vec2<f32>, h: f32) -> f32 {
    let dim = textureDimensions(height_tex);
    let texel = vec2<f32>(1.0 / f32(dim.x), 1.0 / f32(dim.y));
    var occl = 0.0;
    occl += max(sample_height_uv(clamp(uv + vec2<f32>( 2.0,  0.0) * texel, vec2<f32>(0.0), vec2<f32>(1.0))) - h, 0.0);
    occl += max(sample_height_uv(clamp(uv + vec2<f32>(-2.0,  0.0) * texel, vec2<f32>(0.0), vec2<f32>(1.0))) - h, 0.0);
    occl += max(sample_height_uv(clamp(uv + vec2<f32>( 0.0,  2.0) * texel, vec2<f32>(0.0), vec2<f32>(1.0))) - h, 0.0);
    occl += max(sample_height_uv(clamp(uv + vec2<f32>( 0.0, -2.0) * texel, vec2<f32>(0.0), vec2<f32>(1.0))) - h, 0.0);
    occl += max(sample_height_uv(clamp(uv + vec2<f32>( 4.0,  4.0) * texel, vec2<f32>(0.0), vec2<f32>(1.0))) - h, 0.0);
    occl += max(sample_height_uv(clamp(uv + vec2<f32>(-4.0,  4.0) * texel, vec2<f32>(0.0), vec2<f32>(1.0))) - h, 0.0);
    occl += max(sample_height_uv(clamp(uv + vec2<f32>( 4.0, -4.0) * texel, vec2<f32>(0.0), vec2<f32>(1.0))) - h, 0.0);
    occl += max(sample_height_uv(clamp(uv + vec2<f32>(-4.0, -4.0) * texel, vec2<f32>(0.0), vec2<f32>(1.0))) - h, 0.0);
    let span = max(u.world.w - u.world.z, 1.0);
    return clamp(1.0 - (occl / 8.0) / (span * 0.08), 0.35, 1.0);
}

fn aces_tonemap(x: vec3<f32>) -> vec3<f32> {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    // Outside the authored heightfield — leave sky (no edge-height skirt).
    if (i.terrain_uv.x < 0.0 || i.terrain_uv.x > 1.0 || i.terrain_uv.y < 0.0 || i.terrain_uv.y > 1.0) {
        discard;
    }

    var n = i.normal;
    if (dot(n, n) < 1e-6) {
        n = vec3<f32>(0.0, 1.0, 0.0);
    } else {
        n = normalize(n);
    }

    let h_span = max(u.world.w - u.world.z, 1e-3);
    let h_norm = clamp((i.world_pos.y - u.world.z) / h_span, 0.0, 1.0);
    let slope = clamp(1.0 - n.y, 0.0, 1.0);

    // Height + slope tint — neutral grey-brown, avoids washed-out white highs.
    let low = vec3<f32>(0.32, 0.30, 0.24);
    let mid = vec3<f32>(0.42, 0.38, 0.32);
    let high = vec3<f32>(0.52, 0.50, 0.46);
    let height_albedo = mix(mix(low, mid, smoothstep(0.0, 0.55, h_norm)), high, smoothstep(0.55, 1.0, h_norm));
    let rock_tint = vec3<f32>(0.48, 0.45, 0.40);
    let base_tint = mix(height_albedo, rock_tint, slope * 0.70);

    let material_id = u32(clamp(round(sample_map(materials_tex, i.terrain_uv) * 16.0), 0.0, 16.0));
    let mat_albedo = material_color(material_id);
    // Prefer materials when present; otherwise keep the readable height tint.
    let has_materials = sample_map(materials_tex, i.terrain_uv) > 1e-4;
    var albedo = mix(base_tint, mat_albedo, select(0.35, 0.85, has_materials));

    let wetness = clamp(sample_map(wetness_tex, i.terrain_uv), 0.0, 1.0);
    albedo *= 1.0 - wetness * 0.18;

    let density = clamp(sample_map(vegetation_tex, i.terrain_uv), 0.0, 1.0);
    let speck = step(1.0 - density * 0.28, hash21(floor(i.world_pos.xz * 0.7)));
    albedo = mix(albedo, vec3<f32>(0.14, 0.42, 0.12), speck * density);

    // --- Lighting / approximate GI ---
    let sun_dir = normalize(-u.light_dir.xyz);
    let sun_intensity = max(u.light_dir.w, 0.0);
    // Wrap lighting softens self-shadow so valleys stay readable.
    let wrap = 0.22;
    let ndl = clamp((dot(n, sun_dir) + wrap) / (1.0 + wrap), 0.0, 1.0);
    let sun_color = vec3<f32>(1.0, 0.96, 0.88) * sun_intensity;

    // Hemispheric ambient (sky vs ground bounce) — softer fill for form readability.
    let sky_col = vec3<f32>(0.38, 0.44, 0.55);
    let ground_col = vec3<f32>(0.18, 0.16, 0.12);
    let hemi = mix(ground_col, sky_col, n.y * 0.5 + 0.5);

    // Single-bounce fill: albedo bleeds into ambient (terrain radiosity approx).
    let bounce = albedo * 0.22 * (0.40 + 0.60 * h_norm);

    let ao = height_ao(i.terrain_uv, i.world_pos.y);
    let ambient = (hemi * 0.38 + bounce) * ao;
    let diffuse = sun_color * ndl * ao;

    // Soft specular for wet / snow readability (Blinn-Phong).
    let view_dir = normalize(u.eye.xyz - i.world_pos);
    let half_v = normalize(sun_dir + view_dir);
    let spec_power = mix(24.0, 64.0, wetness);
    let spec = pow(max(dot(n, half_v), 0.0), spec_power) * (0.03 + wetness * 0.14 + select(0.0, 0.10, material_id == 4u));

    var color = albedo * (ambient + diffuse) + sun_color * spec;

    // Camera-distance atmospheric haze — subtle, preserves ridge/valley contrast.
    let cam_dist = length(i.world_pos - u.eye.xyz);
    let fog_amount = 1.0 - exp(-cam_dist * 0.00028);
    let fog_col = vec3<f32>(0.40, 0.46, 0.54);
    color = mix(color, fog_col, clamp(fog_amount, 0.0, 0.35));

    let exposure = max(u.eye.w, 0.1);
    color = aces_tonemap(color * exposure);
    return vec4<f32>(color, 1.0);
}
