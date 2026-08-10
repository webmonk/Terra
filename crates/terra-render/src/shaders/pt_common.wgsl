// Shared path-tracing helpers (reference). Inlined into path_trace.wgsl for naga.

fn pt_hash_u32(v: u32) -> u32 {
    var x = v;
    x = x ^ (x >> 16u);
    x = x * 0x7feb352du;
    x = x ^ (x >> 15u);
    x = x * 0x846ca68bu;
    x = x ^ (x >> 16u);
    return x;
}

fn pt_pcg(state: u32) -> u32 {
    let old = state * 747796405u + 2891336453u;
    let word = ((old >> ((old >> 28u) + 4u)) ^ old) * 277803737u;
    return (word >> 22u) ^ word;
}

// Laine–Karras permutation used for Owen scramble.
fn laine_karras_permutation(x: u32, seed: u32) -> u32 {
    var v = x;
    v = v + seed;
    v = v ^ (v * 0x6c50b47cu);
    v = v ^ (v * 0xb82f1e52u);
    v = v ^ (v * 0xc7afe638u);
    v = v ^ (v * 0x8d22f6e6u);
    return v;
}

fn reverse_bits(x: u32) -> u32 {
    var v = x;
    v = ((v >> 1u) & 0x55555555u) | ((v & 0x55555555u) << 1u);
    v = ((v >> 2u) & 0x33333333u) | ((v & 0x33333333u) << 2u);
    v = ((v >> 4u) & 0x0f0f0f0fu) | ((v & 0x0f0f0f0fu) << 4u);
    v = ((v >> 8u) & 0x00ff00ffu) | ((v & 0x00ff00ffu) << 8u);
    return (v >> 16u) | (v << 16u);
}

// First few Sobol direction numbers (Joe–Kuo / standard tables, 32-bit).
fn sobol_direction(dim: u32, bit: u32) -> u32 {
    // dim 0 = van der Corput (identity powers of 1/2)
    if (dim == 0u) {
        return 1u << (31u - bit);
    }
    // Hardcoded direction tables for dims 1..7 (degree / initial m values compacted).
    var v = 1u << (31u - bit);
    switch (dim) {
        case 1u: {
            // m = 1,1,1,... with poly x^1+1 → alternate XOR shifts
            if (bit >= 1u) { v = v ^ (v >> 1u); }
        }
        case 2u: {
            if (bit >= 1u) { v = v ^ (v >> 1u); }
            if (bit >= 2u) { v = v ^ (v >> 2u); }
        }
        case 3u: {
            if (bit >= 1u) { v = v ^ (v >> 1u); }
            if (bit >= 3u) { v = v ^ (v >> 3u); }
        }
        case 4u: {
            if (bit >= 2u) { v = v ^ (v >> 2u); }
            if (bit >= 3u) { v = v ^ (v >> 3u); }
        }
        case 5u: {
            if (bit >= 1u) { v = v ^ (v >> 1u); }
            if (bit >= 2u) { v = v ^ (v >> 2u); }
            if (bit >= 4u) { v = v ^ (v >> 4u); }
        }
        case 6u: {
            if (bit >= 1u) { v = v ^ (v >> 1u); }
            if (bit >= 4u) { v = v ^ (v >> 4u); }
            if (bit >= 5u) { v = v ^ (v >> 5u); }
        }
        case 7u: {
            if (bit >= 2u) { v = v ^ (v >> 2u); }
            if (bit >= 3u) { v = v ^ (v >> 3u); }
            if (bit >= 5u) { v = v ^ (v >> 5u); }
        }
        default: {
            // Fall back: hash-scrambled van der Corput for higher dims.
            let h = pt_hash_u32(dim * 0x9e3779b1u ^ bit);
            v = reverse_bits(h) & (1u << (31u - min(bit, 31u)));
            if (v == 0u) { v = 1u << (31u - min(bit, 31u)); }
        }
    }
    return v;
}

fn sobol_sample(index: u32, dim: u32) -> u32 {
    var result = 0u;
    var i = index;
    var bit = 0u;
    loop {
        if (i == 0u || bit >= 32u) { break; }
        if ((i & 1u) != 0u) {
            result = result ^ sobol_direction(dim & 7u, bit);
        }
        i = i >> 1u;
        bit = bit + 1u;
    }
    return result;
}

fn owen_scramble(x: u32, seed: u32) -> u32 {
    // Nested uniform / Owen-style scramble via Laine–Karras.
    return reverse_bits(laine_karras_permutation(reverse_bits(x), seed));
}

// Owen-scrambled Sobol — deterministic per (pixel, accum, bounce, dim).
fn sample_dim(pixel: vec2<u32>, accum: u32, bounce: u32, dim: u32) -> f32 {
    let pixel_seed = pt_hash_u32(pixel.x * 0x9e3779b1u ^ pixel.y * 0x85ebca6bu);
    let index = accum + 1u;
    let scramble_seed = pt_pcg(pixel_seed ^ bounce * 0x6c078965u ^ dim * 0x1b873593u);
    let sobol_dim = dim & 7u;
    let raw = sobol_sample(index, sobol_dim);
    let scrambled = owen_scramble(raw, scramble_seed);
    return f32(scrambled) * (1.0 / 4294967296.0);
}

fn power_heuristic(a: f32, b: f32) -> f32 {
    let a2 = a * a;
    return a2 / max(a2 + b * b, 1e-8);
}

fn sun_solid_angle_pdf(angular_radius: f32) -> f32 {
    let r = max(angular_radius, 1e-6);
    let solid = max(6.2831853 * (1.0 - cos(r)), 1e-8);
    return 1.0 / solid;
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn offset_ray_origin(p: vec3<f32>, n: vec3<f32>) -> vec3<f32> {
    let origin = 1e-4 * (1.0 + abs(p.y) * 0.01);
    return p + n * origin;
}

fn cosine_sample_hemisphere(u: vec2<f32>) -> vec3<f32> {
    let r = sqrt(u.x);
    let phi = 6.2831853 * u.y;
    return vec3<f32>(r * cos(phi), r * sin(phi), sqrt(max(1.0 - u.x, 0.0)));
}

fn build_tangent_basis(n: vec3<f32>) -> mat3x3<f32> {
    let up = select(vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(1.0, 0.0, 0.0), abs(n.y) > 0.999);
    let t = normalize(cross(up, n));
    let b = cross(n, t);
    return mat3x3<f32>(t, b, n);
}

fn ggx_vndf_sample(u: vec2<f32>, alpha: f32, wo: vec3<f32>) -> vec3<f32> {
    let a = max(alpha, 1e-4);
    let a2 = a * a;
    let phi = 6.2831853 * u.x;
    let cos_theta = sqrt((1.0 - u.y) / max(1.0 + (a2 - 1.0) * u.y, 1e-6));
    let sin_theta = sqrt(max(1.0 - cos_theta * cos_theta, 0.0));
    let h_local = vec3<f32>(sin_theta * cos(phi), sin_theta * sin(phi), cos_theta);
    let basis = build_tangent_basis(normalize(wo));
    return normalize(basis * h_local);
}

fn russian_roulette_throughput(tp: vec3<f32>, bounce: u32, rng: f32) -> bool {
    if (bounce < 2u) {
        return true;
    }
    let p = clamp(max(tp.x, max(tp.y, tp.z)), 0.05, 0.95);
    return rng < p;
}

fn evaluate_environment(dir: vec3<f32>, sun_dir: vec3<f32>, clear_color: vec3<f32>) -> vec3<f32> {
    let up = clamp(dir.y * 0.5 + 0.5, 0.0, 1.0);
    var sky = mix(clear_color * 0.35, clear_color * 1.15, up);
    let sun_w = max(dot(normalize(sun_dir), normalize(dir)), 0.0);
    let sun_disk = pow(sun_w, 512.0);
    let sun_glow = pow(sun_w, 32.0) * 0.25;
    sky = sky + vec3<f32>(1.0, 0.95, 0.85) * (sun_disk * 8.0 + sun_glow);
    return sky;
}

fn sample_sun_disc(u: vec2<f32>, sun_dir: vec3<f32>, angular_radius: f32) -> vec3<f32> {
    let r = angular_radius * sqrt(u.x);
    let phi = 6.2831853 * u.y;
    let basis = build_tangent_basis(normalize(-sun_dir));
    let offset = basis * vec3<f32>(r * cos(phi), r * sin(phi), 0.0);
    return normalize(-sun_dir + offset);
}
