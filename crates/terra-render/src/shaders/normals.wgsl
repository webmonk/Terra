struct NormalUniforms {
    width: u32,
    height: u32,
    dx: f32,
    dz: f32,
    region_x: u32,
    region_y: u32,
    region_w: u32,
    region_h: u32,
};

@group(0) @binding(0) var<uniform> u: NormalUniforms;
@group(0) @binding(1) var height_tex: texture_2d<f32>;
@group(0) @binding(2) var normal_tex: texture_storage_2d<rgba16float, write>;

/// Number of azimuthal horizon slices and march steps per slice for the baked AO.
const AO_DIRS: u32 = 8u;
const AO_STEPS: u32 = 6u;
/// Blend between sky-view factor (0) and tangent-relative HBAO (1). See `horizon_ao`.
const AO_TANGENT_RELIEF: f32 = 0.5;

fn load_height(x: i32, y: i32) -> f32 {
    let cx = clamp(x, 0, i32(u.width) - 1);
    let cy = clamp(y, 0, i32(u.height) - 1);
    return textureLoad(height_tex, vec2<i32>(cx, cy), 0).r;
}

/// Horizon-scan radius in world metres.
///
/// A fraction of the world extent so occlusion reads at valley/basin scale on
/// any project size, with a texel-relative floor so small maps still scan past
/// their immediate neighbourhood.
fn ao_world_radius() -> f32 {
    let extent = max(f32(u.width) * u.dx, f32(u.height) * u.dz);
    return max(extent * 0.045, max(u.dx, u.dz) * 8.0);
}

/// Terrain-space horizon-based ambient occlusion, baked from the heightfield.
///
/// # Why height-field AO rather than SSAO
///
/// The RasterLit backend is a single forward pass straight into the presentation
/// view: no G-buffer, no HDR intermediate, no post stack to hang an SSAO + blur
/// pair off (the depth-export path belongs to the progressive/PT backend only),
/// so screen-space AO would mean restructuring the lit path into a deferred one.
/// Terrain-space AO needs none of that and is strictly better here: it depends
/// only on the heightfield, so it is view-independent (never crawls or flickers
/// as the camera moves), it sees occluders that are off-screen or behind the
/// near plane, and it works at world scale rather than pixel scale — which is
/// what makes basins and valley floors actually read as recessed.
///
/// Being view-independent also means it can be *baked*: it belongs in this pass,
/// next to the normals, recomputed on the same dirty regions when heights change,
/// and costs the terrain fragment shader nothing at draw time (it rides in the
/// unused alpha channel of the normal texture, which the shader already samples).
///
/// # Math
///
/// For each of N azimuthal directions we march outwards and track the largest
/// elevation *tangent* `max_tan = max((h(s) - h0) / dist)`, i.e. the horizon.
/// The cosine-weighted (diffuse) sky fraction of an azimuthal slice whose horizon
/// sits at angle `a` is `cos^2(a)`, and `cos^2(atan(x)) = 1/(1 + x^2)`, so each
/// slice costs one divide. The scan is seeded with the tangent-plane slope in
/// that direction and normalised by it, so an unoccluded tilted surface returns
/// 1.0 instead of being darkened purely by its own slope.
///
/// Fully deterministic: fixed directions, fixed radii, no randomness.
fn horizon_ao(x: i32, y: i32, h0: f32, n: vec3<f32>) -> f32 {
    let radius = ao_world_radius();
    let inv_dx = 1.0 / max(u.dx, 1e-6);
    let inv_dz = 1.0 / max(u.dz, 1e-6);
    let step_scale = 1.0 / f32(AO_STEPS);
    var visibility = 0.0;
    for (var d = 0u; d < AO_DIRS; d = d + 1u) {
        let phi = (f32(d) + 0.5) * (6.2831853 / f32(AO_DIRS));
        let dir = vec2<f32>(cos(phi), sin(phi));
        // Tangent-plane elevation (as a tangent, +up) along `dir`.
        let tan_t = clamp(-(n.x * dir.x + n.z * dir.y) / max(n.y, 1e-3), -4.0, 4.0);
        var max_tan = tan_t;
        for (var s = 0u; s < AO_STEPS; s = s + 1u) {
            // Quadratic spacing: dense near the texel (contact-scale detail),
            // sparse far out (broad valley occlusion).
            let t = (f32(s) + 1.0) * step_scale;
            let dist = radius * t * t;
            let sx = x + i32(round(dir.x * dist * inv_dx));
            let sy = y + i32(round(dir.y * dist * inv_dz));
            max_tan = max(max_tan, (load_height(sx, sy) - h0) / dist);
        }
        // `cos^2(atan(x))` is even in x, so a horizon *below* the horizontal
        // (terrain falling away) must be clamped to 0 rather than fed in signed -
        // otherwise a downslope reports the same occlusion as the matching
        // upslope. Clamped, a slice with nothing above the horizontal is open sky.
        let h_t = max(max_tan, 0.0);
        let r_t = max(tan_t, 0.0);
        let horizon = 1.0 / (1.0 + h_t * h_t);
        let reference = 1.0 / (1.0 + r_t * r_t);
        // AO_TANGENT_RELIEF picks a point on a well-known spectrum:
        //   0 -> pure sky-view factor (cos^2 about the vertical). Standard DEM
        //        shading; darkens every slope by its own tilt, which is what
        //        makes landform read, but is not occlusion.
        //   1 -> pure tangent-relative HBAO. Physically honest (an unoccluded
        //        tilted plane returns 1.0) but nearly a no-op on smooth terrain,
        //        where there genuinely is no occlusion to find.
        // Half-way keeps the full response to real concavity while restoring
        // enough of the sky-view landform cue to be worth the pass.
        let denom = mix(1.0, reference, AO_TANGENT_RELIEF);
        visibility += clamp(horizon / max(denom, 1e-3), 0.0, 1.0);
    }
    return clamp(visibility / f32(AO_DIRS), 0.0, 1.0);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let x = u.region_x + gid.x;
    let y = u.region_y + gid.y;
    if (gid.x >= u.region_w || gid.y >= u.region_h) {
        return;
    }
    if (x >= u.width || y >= u.height) {
        return;
    }

    let xl = max(i32(x) - 1, 0);
    let xr = min(i32(x) + 1, i32(u.width) - 1);
    let yd = max(i32(y) - 1, 0);
    let yu = min(i32(y) + 1, i32(u.height) - 1);

    let h_l = textureLoad(height_tex, vec2<i32>(xl, i32(y)), 0).r;
    let h_r = textureLoad(height_tex, vec2<i32>(xr, i32(y)), 0).r;
    let h_d = textureLoad(height_tex, vec2<i32>(i32(x), yd), 0).r;
    let h_u = textureLoad(height_tex, vec2<i32>(i32(x), yu), 0).r;

    let gx = (h_r - h_l) / (f32(xr - xl) * u.dx);
    let gz = (h_u - h_d) / (f32(yu - yd) * u.dz);
    let n = normalize(vec3<f32>(-gx, 1.0, -gz));
    let h0 = textureLoad(height_tex, vec2<i32>(i32(x), i32(y)), 0).r;
    // Alpha carries baked sky visibility (1 = open sky). Previously a constant
    // 1.0, so a stale/unbaked texel degrades to "no occlusion" - the old look.
    let ao = horizon_ao(i32(x), i32(y), h0, n);
    textureStore(normal_tex, vec2<i32>(i32(x), i32(y)), vec4<f32>(n, ao));
}
