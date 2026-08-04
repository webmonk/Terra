// Dedicated Mountains / Dunes / Canyons generators (preview; CPU remains bake reference).

struct Uniforms {
    width: u32,
    height: u32,
    world_x: f32,
    world_z: f32,
    seed: u32,
    octaves: u32,
    frequency: f32,
    amplitude: f32,
    lacunarity: f32,
    persistence: f32,
    offset_x: f32,
    offset_z: f32,
    ridge_sharpness: f32,
    range_angle: f32,
    range_width: f32,
    wave_frequency: f32,
    asymmetry: f32,
    depth: f32,
    canyon_width: f32,
    meander: f32,
    shape_mode: u32, // 0 mountains, 1 dunes, 2 canyons
    _pad: u32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var dst: texture_storage_2d<r32float, write>;

fn fade(t: f32) -> f32 {
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

fn hash2_seeded(p: vec2<u32>, seed: u32) -> f32 {
    var n = p.x * 374761393u + p.y * 668265263u + seed * 2246822519u;
    n = (n ^ (n >> 13u)) * 1274126177u;
    n = n ^ (n >> 16u);
    return f32(n & 0x00FFFFFFu) / f32(0x01000000u);
}

fn grad(h: f32, x: f32, y: f32) -> f32 {
    let ang = h * 6.2831853;
    return cos(ang) * x + sin(ang) * y;
}

fn perlin_seeded(p: vec2<f32>, seed: u32) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let aa = hash2_seeded(vec2<u32>(u32(i.x), u32(i.y)), seed);
    let ba = hash2_seeded(vec2<u32>(u32(i.x + 1), u32(i.y)), seed);
    let ab = hash2_seeded(vec2<u32>(u32(i.x), u32(i.y + 1)), seed);
    let bb = hash2_seeded(vec2<u32>(u32(i.x + 1), u32(i.y + 1)), seed);
    let ux = fade(f.x);
    let uy = fade(f.y);
    let x1 = mix(grad(aa, f.x, f.y), grad(ba, f.x - 1.0, f.y), ux);
    let x2 = mix(grad(ab, f.x, f.y - 1.0), grad(bb, f.x - 1.0, f.y - 1.0), ux);
    return mix(x1, x2, uy);
}

/// CPU-style ridged MF (weight cascade, octave seed stride 9173). Returns ≈ [0, amp].
fn ridged_mf(x: f32, z: f32) -> f32 {
    var amp = 1.0;
    var freq = u.frequency;
    var sum = 0.0;
    var weight = 1.0;
    let oct = min(u.octaves, 12u);
    for (var o = 0u; o < oct; o++) {
        let ox = (x + u.offset_x) * freq;
        let oz = (z + u.offset_z) * freq;
        var n = perlin_seeded(vec2<f32>(ox, oz), u.seed + o * 9173u);
        n = 1.0 - abs(n);
        n = n * n;
        n = n * weight;
        weight = clamp(n * 2.0, 0.0, 1.0);
        sum += n * amp;
        amp *= u.persistence;
        freq *= u.lacunarity;
    }
    return sum * u.amplitude;
}

/// CPU-style fBm with remap to [-1,1] then * amplitude.
fn fbm_amp(x: f32, z: f32) -> f32 {
    var amp = 1.0;
    var freq = u.frequency;
    var sum = 0.0;
    var norm = 0.0;
    let oct = min(u.octaves, 12u);
    for (var o = 0u; o < oct; o++) {
        let ox = (x + u.offset_x) * freq;
        let oz = (z + u.offset_z) * freq;
        let n = perlin_seeded(vec2<f32>(ox, oz), u.seed + o * 1013u);
        sum += n * amp;
        norm += amp;
        amp *= u.persistence;
        freq *= u.lacunarity;
    }
    var v = 0.0;
    if (norm > 0.0) {
        v = sum / norm;
    }
    // Default remap [-1,1] → identity for typical NoiseParams.
    return v * u.amplitude;
}

fn mountains(x: f32, z: f32) -> f32 {
    let nx = x / u.world_x - 0.5;
    let nz = z / u.world_z - 0.5;
    let ca = cos(u.range_angle);
    let sa = sin(u.range_angle);
    let axis = nx * ca + nz * sa;
    let cross_axis = nx * (-sa) + nz * ca;
    let dist = abs(cross_axis);
    let range_mask = pow(clamp(1.0 - (dist / max(u.range_width, 1e-3)), 0.0, 1.0), 1.5);
    let ridge_x = axis * u.world_x;
    let ridge_z = cross_axis * u.world_z;
    let amplitude = max(u.amplitude, 0.0);
    let ridges = clamp(ridged_mf(ridge_x, ridge_z) / max(amplitude, 1e-6), 0.0, 1.0);
    let shaped = pow(ridges, max(u.ridge_sharpness, 0.1)) * amplitude;
    return shaped * range_mask;
}

fn dunes(x: f32, z: f32) -> f32 {
    let base = fbm_amp(x, z);
    let tau = 6.2831853;
    var phase = (x * u.wave_frequency) % tau;
    if (phase < 0.0) {
        phase += tau;
    }
    phase = phase / tau;
    let asymmetry = clamp(u.asymmetry, 0.0, 1.0);
    var wave = 0.0;
    if (phase < 0.5) {
        wave = 0.5 * pow(phase * 2.0, 1.0 / (1.0 + asymmetry));
    } else {
        wave = 0.5 + 0.5 * pow((phase - 0.5) * 2.0, 1.0 + asymmetry);
    }
    return base * (0.35 + 0.65 * wave);
}

fn canyons(x: f32, z: f32) -> f32 {
    let seed_f = f32(u.seed);
    let broad = perlin_seeded(vec2<f32>(z * 0.0015, seed_f * 0.017), u.seed);
    let detail = perlin_seeded(vec2<f32>(z * 0.0045, seed_f * 0.043), u.seed ^ 0x9E37u);
    let meander_n = broad * 0.75 + detail * 0.25;
    let center = u.world_x * 0.5 + meander_n * u.meander * u.canyon_width * 1.75;
    let d = abs(x - center);
    let carve = 1.0 - clamp(d / max(u.canyon_width, 1e-3), 0.0, 1.0);
    return -u.depth * carve;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= u.width || gid.y >= u.height) { return; }
    let uv = vec2<f32>(
        (f32(gid.x) + 0.5) / f32(u.width),
        (f32(gid.y) + 0.5) / f32(u.height),
    );
    let x = uv.x * u.world_x;
    let z = uv.y * u.world_z;
    var h = 0.0;
    if (u.shape_mode == 0u) {
        h = mountains(x, z);
    } else if (u.shape_mode == 1u) {
        h = dunes(x, z);
    } else {
        h = canyons(x, z);
    }
    textureStore(dst, vec2<i32>(i32(gid.x), i32(gid.y)), vec4<f32>(h, 0.0, 0.0, 0.0));
}
