// Procedural height generation (value / Perlin fBm + ridged). Preview-quality oracle; CPU remains reference.

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
    remap_min: f32,
    remap_max: f32,
    noise_type: u32, // 0 value, 1 perlin
    mode: u32,       // 0 fbm, 1 ridged
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var dst: texture_storage_2d<r32float, write>;

fn hash2(p: vec2<u32>) -> f32 {
    var n = p.x * 374761393u + p.y * 668265263u + u.seed * 2246822519u;
    n = (n ^ (n >> 13u)) * 1274126177u;
    n = n ^ (n >> 16u);
    return f32(n & 0x00FFFFFFu) / f32(0x01000000u);
}

fn fade(t: f32) -> f32 {
    return t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
}

fn value_noise(p: vec2<f32>) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let a = hash2(vec2<u32>(u32(i.x), u32(i.y)));
    let b = hash2(vec2<u32>(u32(i.x + 1), u32(i.y)));
    let c = hash2(vec2<u32>(u32(i.x), u32(i.y + 1)));
    let d = hash2(vec2<u32>(u32(i.x + 1), u32(i.y + 1)));
    let u_x = fade(f.x);
    let u_y = fade(f.y);
    return mix(mix(a, b, u_x), mix(c, d, u_x), u_y) * 2.0 - 1.0;
}

fn grad(h: f32, x: f32, y: f32) -> f32 {
    let ang = h * 6.2831853;
    return cos(ang) * x + sin(ang) * y;
}

fn perlin_noise(p: vec2<f32>) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let aa = hash2(vec2<u32>(u32(i.x), u32(i.y)));
    let ba = hash2(vec2<u32>(u32(i.x + 1), u32(i.y)));
    let ab = hash2(vec2<u32>(u32(i.x), u32(i.y + 1)));
    let bb = hash2(vec2<u32>(u32(i.x + 1), u32(i.y + 1)));
    let u_x = fade(f.x);
    let u_y = fade(f.y);
    let x1 = mix(grad(aa, f.x, f.y), grad(ba, f.x - 1.0, f.y), u_x);
    let x2 = mix(grad(ab, f.x, f.y - 1.0), grad(bb, f.x - 1.0, f.y - 1.0), u_x);
    return mix(x1, x2, u_y);
}

fn sample_noise(p: vec2<f32>) -> f32 {
    if (u.noise_type == 0u) {
        return value_noise(p);
    }
    return perlin_noise(p);
}

fn hash2_seeded(p: vec2<u32>, seed: u32) -> f32 {
    var n = p.x * 374761393u + p.y * 668265263u + seed * 2246822519u;
    n = (n ^ (n >> 13u)) * 1274126177u;
    n = n ^ (n >> 16u);
    return f32(n & 0x00FFFFFFu) / f32(0x01000000u);
}

fn value_noise_seeded(p: vec2<f32>, seed: u32) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let a = hash2_seeded(vec2<u32>(u32(i.x), u32(i.y)), seed);
    let b = hash2_seeded(vec2<u32>(u32(i.x + 1), u32(i.y)), seed);
    let c = hash2_seeded(vec2<u32>(u32(i.x), u32(i.y + 1)), seed);
    let d = hash2_seeded(vec2<u32>(u32(i.x + 1), u32(i.y + 1)), seed);
    let u_x = fade(f.x);
    let u_y = fade(f.y);
    return mix(mix(a, b, u_x), mix(c, d, u_x), u_y) * 2.0 - 1.0;
}

fn perlin_noise_seeded(p: vec2<f32>, seed: u32) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let aa = hash2_seeded(vec2<u32>(u32(i.x), u32(i.y)), seed);
    let ba = hash2_seeded(vec2<u32>(u32(i.x + 1), u32(i.y)), seed);
    let ab = hash2_seeded(vec2<u32>(u32(i.x), u32(i.y + 1)), seed);
    let bb = hash2_seeded(vec2<u32>(u32(i.x + 1), u32(i.y + 1)), seed);
    let u_x = fade(f.x);
    let u_y = fade(f.y);
    let x1 = mix(grad(aa, f.x, f.y), grad(ba, f.x - 1.0, f.y), u_x);
    let x2 = mix(grad(ab, f.x, f.y - 1.0), grad(bb, f.x - 1.0, f.y - 1.0), u_x);
    return mix(x1, x2, u_y);
}

fn sample_noise_seeded(p: vec2<f32>, seed: u32) -> f32 {
    if (u.noise_type == 0u) {
        return value_noise_seeded(p, seed);
    }
    return perlin_noise_seeded(p, seed);
}

fn fbm(p0: vec2<f32>) -> f32 {
    var p = p0;
    var amp = 1.0;
    var sum = 0.0;
    var norm = 0.0;
    let oct = min(u.octaves, 12u);
    for (var o = 0u; o < oct; o++) {
        // Match CPU: seed.wrapping_add(o * 1013)
        let oct_seed = u.seed + o * 1013u;
        var n = sample_noise_seeded(p, oct_seed);
        if (u.mode == 1u) {
            n = 1.0 - abs(n);
            n = n * n;
        }
        sum += n * amp;
        norm += amp;
        amp *= u.persistence;
        p *= u.lacunarity;
    }
    if (norm > 0.0) {
        sum /= norm;
    }
    return sum;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= u.width || gid.y >= u.height) { return; }
    let uv = vec2<f32>(
        (f32(gid.x) + 0.5) / f32(u.width),
        (f32(gid.y) + 0.5) / f32(u.height),
    );
    // CPU applies offset inside each octave sample; match by offsetting world before freq.
    let world = vec2<f32>(uv.x * u.world_x, uv.y * u.world_z);
    let p = (world + vec2<f32>(u.offset_x, u.offset_z)) * u.frequency;
    let n = fbm(p);
    let span = max(u.remap_max - u.remap_min, 1e-5);
    let t = clamp((n - u.remap_min) / span, 0.0, 1.0);
    // CPU remaps to [-1,1] then * amplitude — equivalent to (2t-1)*amplitude when remap is [-1,1].
    let centered = t * 2.0 - 1.0;
    let h = centered * u.amplitude;
    textureStore(dst, vec2<i32>(i32(gid.x), i32(gid.y)), vec4<f32>(h, 0.0, 0.0, 0.0));
}
