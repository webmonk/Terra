// Compute megakernel heightfield path tracer — no HW RT/BLAS in Terra wgpu path

struct PathTraceUniforms {
    view_inv: mat4x4<f32>,
    proj_params: vec4<f32>,       // x=aspect, y=tan_half_fov_y, z=near, w=far
    sun_dir_intensity: vec4<f32>,  // xyz toward scene, w intensity
    clear_exposure: vec4<f32>,      // rgb clear, w exposure
    world_height: vec4<f32>,        // world_x, world_z, min_h, max_h
    trace_params: vec4<f32>,        // spp, max_bounces, frame_seed, accum_index
    clamps_radius: vec4<f32>,       // direct_clamp, indirect_clamp, sun_angular_radius, _
    resolution_scale: vec4<f32>,    // out_w, out_h, internal_w, internal_h
    tex_dims: vec4<f32>,            // tex_w, tex_h, dx, dz
};

@group(0) @binding(0) var<uniform> u: PathTraceUniforms;
@group(0) @binding(1) var height_tex: texture_2d<f32>;
@group(0) @binding(2) var normal_tex: texture_2d<f32>;
@group(0) @binding(3) var material_tex: texture_2d<f32>;
// Write-only storage (≤4 for default WebGPU) + sampled previous radiance (ping-pong).
// Radiance.a holds sample count; normal.w = roughness; albedo.a = material id / 255.
@group(0) @binding(4) var radiance_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(5) var depth_out: texture_storage_2d<r32float, write>;
@group(0) @binding(6) var normal_out: texture_storage_2d<rgba16float, write>;
@group(0) @binding(7) var albedo_out: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(8) var sample_mask: texture_2d<f32>;
@group(0) @binding(9) var radiance_prev: texture_2d<f32>;

// --- pt_common (inlined) ---

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

fn sobol_direction(dim: u32, bit: u32) -> u32 {
    if (dim == 0u) {
        return 1u << (31u - bit);
    }
    var v = 1u << (31u - bit);
    switch (dim) {
        case 1u: {
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
    return reverse_bits(laine_karras_permutation(reverse_bits(x), seed));
}

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

// --- heightfield ---

fn world_to_uv(p: vec3<f32>) -> vec2<f32> {
    let wx = max(u.world_height.x, 1e-3);
    let wz = max(u.world_height.y, 1e-3);
    return clamp(vec2<f32>(p.x / wx, p.z / wz), vec2<f32>(0.0), vec2<f32>(1.0));
}

fn sample_height_uv(uv: vec2<f32>) -> f32 {
    let dim = textureDimensions(height_tex);
    let fx = clamp(uv.x * f32(dim.x - 1u), 0.0, f32(dim.x - 1u));
    let fy = clamp(uv.y * f32(dim.y - 1u), 0.0, f32(dim.y - 1u));
    let p0 = vec2<i32>(i32(floor(fx)), i32(floor(fy)));
    let p1 = vec2<i32>(min(p0.x + 1, i32(dim.x) - 1), min(p0.y + 1, i32(dim.y) - 1));
    let tx = fx - f32(p0.x);
    let ty = fy - f32(p0.y);
    let h00 = textureLoad(height_tex, p0, 0).r;
    let h10 = textureLoad(height_tex, vec2<i32>(p1.x, p0.y), 0).r;
    let h01 = textureLoad(height_tex, vec2<i32>(p0.x, p1.y), 0).r;
    let h11 = textureLoad(height_tex, p1, 0).r;
    return mix(mix(h00, h10, tx), mix(h01, h11, tx), ty);
}

fn sample_height_world(p: vec3<f32>) -> f32 {
    return sample_height_uv(world_to_uv(p));
}

fn sample_normal_world(p: vec3<f32>) -> vec3<f32> {
    let uv = world_to_uv(p);
    let dim = textureDimensions(normal_tex);
    let x = i32(clamp(uv.x * f32(dim.x - 1u), 0.0, f32(dim.x - 1u)));
    let y = i32(clamp(uv.y * f32(dim.y - 1u), 0.0, f32(dim.y - 1u)));
    return normalize(textureLoad(normal_tex, vec2<i32>(x, y), 0).xyz);
}

fn sample_material(p: vec3<f32>) -> vec2<f32> {
    let uv = world_to_uv(p);
    let dim = textureDimensions(material_tex);
    let x = i32(clamp(uv.x * f32(dim.x - 1u), 0.0, f32(dim.x - 1u)));
    let y = i32(clamp(uv.y * f32(dim.y - 1u), 0.0, f32(dim.y - 1u)));
    let m = textureLoad(material_tex, vec2<i32>(x, y), 0).r;
    let id = clamp(round(m * 16.0), 0.0, 16.0) / 16.0;
    return vec2<f32>(id, 0.82);
}

fn material_albedo(id_norm: f32) -> vec3<f32> {
    let id = u32(clamp(round(id_norm * 16.0), 0.0, 16.0));
    switch (id % 5u) {
        case 0u: { return vec3<f32>(0.42, 0.40, 0.36); }
        case 1u: { return vec3<f32>(0.48, 0.36, 0.24); }
        case 2u: { return vec3<f32>(0.30, 0.42, 0.22); }
        case 3u: { return vec3<f32>(0.62, 0.54, 0.36); }
        default: { return vec3<f32>(0.78, 0.80, 0.82); }
    }
}

struct HeightHit {
    hit: bool,
    t: f32,
    pos: vec3<f32>,
    normal: vec3<f32>,
    uv: vec2<f32>,
    material_id: f32,
    roughness: f32,
};

fn intersect_heightfield(ro: vec3<f32>, rd: vec3<f32>) -> HeightHit {
    var miss = HeightHit(false, 0.0, ro, vec3<f32>(0.0, 1.0, 0.0), vec2<f32>(0.0), 0.0, 0.82);

    let wx = max(u.world_height.x, 1e-3);
    let wz = max(u.world_height.y, 1e-3);
    let min_h = u.world_height.z;
    let max_h = u.world_height.w;
    let h_pad = max((max_h - min_h) * 0.05, 1.0);

    // Intersect ray with world XZ slab expanded by height bounds.
    var t_min = u.proj_params.z;
    var t_max = u.proj_params.w;
    if (abs(rd.x) > 1e-8) {
        let t0 = (0.0 - ro.x) / rd.x;
        let t1 = (wx - ro.x) / rd.x;
        t_min = max(t_min, min(t0, t1));
        t_max = min(t_max, max(t0, t1));
    } else if (ro.x < 0.0 || ro.x > wx) {
        return miss;
    }
    if (abs(rd.z) > 1e-8) {
        let t0 = (0.0 - ro.z) / rd.z;
        let t1 = (wz - ro.z) / rd.z;
        t_min = max(t_min, min(t0, t1));
        t_max = min(t_max, max(t0, t1));
    } else if (ro.z < 0.0 || ro.z > wz) {
        return miss;
    }
    if (abs(rd.y) > 1e-8) {
        let t0 = ((min_h - h_pad) - ro.y) / rd.y;
        let t1 = ((max_h + h_pad) - ro.y) / rd.y;
        t_min = max(t_min, min(t0, t1));
        t_max = min(t_max, max(t0, t1));
    }
    if (t_min > t_max) {
        return miss;
    }

    let steps = 64;
    var t_prev = t_min;
    var y_prev = ro.y + t_prev * rd.y - sample_height_world(ro + rd * t_prev);
    var found = false;
    var t_hit = t_max;

    for (var i = 1; i <= steps; i = i + 1) {
        let t = mix(t_min, t_max, f32(i) / f32(steps));
        let p = ro + rd * t;
        let y = p.y - sample_height_world(p);
        if (y <= 0.0 && y_prev > 0.0) {
            t_hit = t;
            found = true;
            break;
        }
        y_prev = y;
        t_prev = t;
    }
    if (!found) {
        return miss;
    }

    var lo = t_prev;
    var hi = t_hit;
    for (var j = 0; j < 8; j = j + 1) {
        let tm = 0.5 * (lo + hi);
        let pm = ro + rd * tm;
        let ym = pm.y - sample_height_world(pm);
        if (ym > 0.0) {
            lo = tm;
        } else {
            hi = tm;
        }
    }
    let t = hi;
    let pos = ro + rd * t;
    let uv = world_to_uv(pos);
    let mat = sample_material(pos);
    let n = sample_normal_world(pos);
    return HeightHit(true, t, pos, n, uv, mat.x, mat.y);
}

fn camera_ray(pixel: vec2<u32>, jitter: vec2<f32>) -> vec3<f32> {
    // proj_params: x=aspect, y=tan_half_fov_y, z=near, w=far
    // view_inv: view → world (glam look_at_rh inverse). View looks down -Z.
    let res = vec2<f32>(u.resolution_scale.z, u.resolution_scale.w);
    let uv = (vec2<f32>(pixel) + jitter) / max(res, vec2<f32>(1.0));
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let aspect = max(u.proj_params.x, 1e-4);
    let tan_half = max(u.proj_params.y, 1e-4);
    let dir_view = normalize(vec3<f32>(
        ndc.x * aspect * tan_half,
        ndc.y * tan_half,
        -1.0,
    ));
    return normalize((u.view_inv * vec4<f32>(dir_view, 0.0)).xyz);
}

fn camera_origin() -> vec3<f32> {
    let eye_h = u.view_inv * vec4<f32>(0.0, 0.0, 0.0, 1.0);
    return eye_h.xyz / max(abs(eye_h.w), 1e-6);
}

fn trace_path(ro: vec3<f32>, rd: vec3<f32>, pixel: vec2<u32>, spp_index: u32) -> vec3<f32> {
    var radiance = vec3<f32>(0.0);
    var throughput = vec3<f32>(1.0);
    var ray_o = ro;
    var ray_d = rd;
    let max_b = u32(max(u.trace_params.y, 1.0));
    let sun_dir = u.sun_dir_intensity.xyz;
    let sun_i = u.sun_dir_intensity.w;
    let clear = u.clear_exposure.xyz;
    let sun_r = u.clamps_radius.z;
    let pdf_light = sun_solid_angle_pdf(sun_r);
    let inv_pi = 1.0 / 3.14159265;

    var first_hit = true;
    var primary: HeightHit;

    for (var bounce = 0u; bounce < max_b; bounce = bounce + 1u) {
        let hit = intersect_heightfield(ray_o, ray_d);
        if (!hit.hit) {
            radiance = radiance + throughput * evaluate_environment(ray_d, sun_dir, clear);
            break;
        }
        if (first_hit) {
            primary = hit;
            first_hit = false;
        }

        let n = hit.normal;
        let albedo = material_albedo(hit.material_id);
        let rough = hit.roughness;
        let wo = -ray_d;

        // NEE — sun disc with solid-angle PDF + MIS vs cosine hemisphere
        let su = vec2<f32>(
            sample_dim(pixel, spp_index, bounce, 10u),
            sample_dim(pixel, spp_index, bounce, 11u),
        );
        let wi_light = sample_sun_disc(su, sun_dir, sun_r);
        let shadow_hit = intersect_heightfield(offset_ray_origin(hit.pos, n), wi_light);
        if (!shadow_hit.hit) {
            let ndotl = max(dot(n, wi_light), 0.0);
            let pdf_bsdf = ndotl * inv_pi;
            let mis_w = power_heuristic(pdf_light, pdf_bsdf);
            let sun_col = vec3<f32>(1.0, 0.96, 0.88) * sun_i;
            let direct = albedo * sun_col * ndotl * inv_pi * mis_w / pdf_light;
            radiance = radiance + throughput * min(direct, vec3<f32>(u.clamps_radius.x));
        }

        // Indirect bounce — GGX VNDF specular mix when smooth, else cosine
        let bu = vec2<f32>(
            sample_dim(pixel, spp_index, bounce, 20u),
            sample_dim(pixel, spp_index, bounce, 21u),
        );
        var wi: vec3<f32>;
        var pdf: f32;
        var brdf: vec3<f32>;
        if (rough < 0.35) {
            let mix_t = clamp(rough / 0.35, 0.0, 1.0);
            let lobe = sample_dim(pixel, spp_index, bounce, 22u);
            if (lobe > mix_t) {
                let alpha = max(rough * rough, 1e-4);
                let h = ggx_vndf_sample(bu, alpha, wo);
                wi = normalize(reflect(-wo, h));
                if (dot(n, wi) <= 0.0) {
                    wi = normalize(build_tangent_basis(n) * cosine_sample_hemisphere(bu));
                    pdf = max(dot(n, wi), 1e-4) * inv_pi;
                    brdf = albedo * inv_pi;
                } else {
                    let ndoth = max(dot(n, h), 1e-4);
                    let vdoth = max(dot(wo, h), 1e-4);
                    pdf = max(ndoth / (4.0 * vdoth), 1e-4);
                    // Specular lobe tinted by albedo (simple dielectric stub)
                    brdf = albedo * (0.04 + 0.96 * pow(1.0 - vdoth, 5.0)) / max(4.0 * vdoth, 1e-4);
                }
            } else {
                wi = normalize(build_tangent_basis(n) * cosine_sample_hemisphere(bu));
                pdf = max(dot(n, wi), 1e-4) * inv_pi;
                brdf = albedo * inv_pi;
            }
        } else {
            wi = normalize(build_tangent_basis(n) * cosine_sample_hemisphere(bu));
            pdf = max(dot(n, wi), 1e-4) * inv_pi;
            brdf = albedo * inv_pi;
        }

        let ndot_wi = max(dot(n, wi), 0.0);
        // MIS weight vs sun NEE when BSDF sample lands near the sun disc
        let sun_axis = normalize(-sun_dir);
        let cos_to_sun = dot(wi, sun_axis);
        let in_sun = cos_to_sun > cos(max(sun_r, 1e-6));
        let mis_bsdf = select(1.0, power_heuristic(pdf, pdf_light), in_sun);
        throughput = throughput * brdf * ndot_wi * mis_bsdf / max(pdf, 1e-6);

        let rr = sample_dim(pixel, spp_index, bounce, 30u);
        if (!russian_roulette_throughput(throughput, bounce, rr)) {
            break;
        }
        let p_survive = clamp(max(throughput.x, max(throughput.y, throughput.z)), 0.05, 0.95);
        throughput = throughput / p_survive;

        ray_o = offset_ray_origin(hit.pos, n);
        ray_d = wi;
    }

    return min(radiance, vec3<f32>(u.clamps_radius.y));
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let out_w = u32(u.resolution_scale.x);
    let out_h = u32(u.resolution_scale.y);
    if (gid.x >= out_w || gid.y >= out_h) {
        return;
    }
    let pixel = vec2<u32>(gid.xy);
    let p = vec2<i32>(i32(gid.x), i32(gid.y));

    let prev = textureLoad(radiance_prev, p, 0);
    let prev_rgb = prev.rgb;
    let prev_count = prev.a;

    // Adaptive mask: converged tiles copy previous radiance into the write buffer.
    let mask = textureLoad(sample_mask, p, 0).r;
    if (mask < 0.01) {
        textureStore(radiance_out, p, prev);
        return;
    }

    let internal = vec2<u32>(u32(u.resolution_scale.z), u32(u.resolution_scale.w));
    let trace_pixel = vec2<u32>(
        min(pixel.x * internal.x / max(out_w, 1u), internal.x - 1u),
        min(pixel.y * internal.y / max(out_h, 1u), internal.y - 1u),
    );

    let spp = u32(max(u.trace_params.x, 1.0));
    let frame_seed = u32(u.trace_params.z);
    let accum_base = u32(u.trace_params.w);

    var sample_rgb = vec3<f32>(0.0);
    for (var s = 0u; s < spp; s = s + 1u) {
        let accum = accum_base + s;
        let jitter = vec2<f32>(
            sample_dim(trace_pixel, accum + frame_seed, 0u, 0u),
            sample_dim(trace_pixel, accum + frame_seed, 0u, 1u),
        ) - 0.5;
        let rd = camera_ray(trace_pixel, jitter);
        let ro = camera_origin();
        sample_rgb = sample_rgb + trace_path(ro, rd, trace_pixel, accum);
    }
    sample_rgb = sample_rgb / f32(spp);
    sample_rgb = sample_rgb * u.clear_exposure.w;

    let new_count = prev_count + f32(spp);
    let alpha = f32(spp) / max(new_count, 1e-3);
    let accum_rgb = mix(prev_rgb, sample_rgb, alpha);

    textureStore(radiance_out, p, vec4<f32>(accum_rgb, new_count));

    let ro = camera_origin();
    let center_rd = camera_ray(trace_pixel, vec2<f32>(0.0));
    let primary = intersect_heightfield(ro, center_rd);
    if (primary.hit) {
        textureStore(depth_out, p, vec4<f32>(primary.t, 0.0, 0.0, 0.0));
        textureStore(normal_out, p, vec4<f32>(primary.normal, primary.roughness));
        let alb = material_albedo(primary.material_id);
        textureStore(albedo_out, p, vec4<f32>(alb, primary.material_id / 255.0));
    } else {
        textureStore(depth_out, p, vec4<f32>(1.0e30, 0.0, 0.0, 0.0));
        textureStore(normal_out, p, vec4<f32>(0.0, 1.0, 0.0, 0.82));
        textureStore(albedo_out, p, vec4<f32>(u.clear_exposure.xyz, 0.0));
    }
}
