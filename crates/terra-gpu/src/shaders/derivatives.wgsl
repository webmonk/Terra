//! GPU terrain derivative kernels (preview). CPU geomorph remains the oracle.
//!
//! World-space radius is passed as texel radius (`radius_texels`).

struct Uniforms {
    width: u32,
    height: u32,
    /// Sample offset in texels (≥ 1).
    radius_texels: u32,
    /// 0=gradient_mag, 1=slope[0,1], 2=laplacian[0,1], 3=aspect[0,1]
    mode: u32,
    dx: f32,
    dz: f32,
    _pad0: f32,
    _pad1: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var src: texture_2d<f32>;
@group(0) @binding(2) var dst: texture_storage_2d<r32float, write>;

fn sample_h(i: i32, j: i32) -> f32 {
    let ii = clamp(i, 0, i32(u.width) - 1);
    let jj = clamp(j, 0, i32(u.height) - 1);
    return textureLoad(src, vec2<i32>(ii, jj), 0).r;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= u.width || gid.y >= u.height) { return; }
    let i = i32(gid.x);
    let j = i32(gid.y);
    let r = max(i32(u.radius_texels), 1);
    let dx = max(u.dx * f32(r), 1e-6);
    let dz = max(u.dz * f32(r), 1e-6);

    let gx = (sample_h(i + r, j) - sample_h(i - r, j)) / (2.0 * dx);
    let gz = (sample_h(i, j + r) - sample_h(i, j - r)) / (2.0 * dz);
    let mag = sqrt(gx * gx + gz * gz);

    var out_v = 0.0;
    if (u.mode == 0u) {
        out_v = mag;
    } else if (u.mode == 1u) {
        out_v = clamp(atan(mag) * 57.2957795 / 90.0, 0.0, 1.0);
    } else if (u.mode == 2u) {
        let z = sample_h(i, j);
        let lap = (sample_h(i + r, j) + sample_h(i - r, j) + sample_h(i, j + r) + sample_h(i, j - r) - 4.0 * z)
            / (dx * dx);
        // Soft map around 0.5 (CPU normalises by max abs; preview uses fixed scale).
        out_v = clamp(0.5 + lap * 5.0, 0.0, 1.0);
    } else {
        let aspect = atan2(gz, -gx);
        let two_pi = 6.2831853;
        var a = aspect;
        if (a < 0.0) { a = a + two_pi; }
        out_v = clamp(a / two_pi, 0.0, 1.0);
    }

    textureStore(dst, vec2<i32>(i, j), vec4<f32>(out_v, 0.0, 0.0, 0.0));
}
