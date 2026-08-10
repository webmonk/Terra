//! Depth-only shadow pass: displace the terrain grid in light clip space.

struct ShadowUniforms {
    light_view_proj: mat4x4<f32>,
    params: vec4<f32>,
}

struct VsIn {
    @location(0) uv: vec2<f32>,
    @location(1) face: f32,
    @location(2) use_base: f32,
}

@group(0) @binding(0) var<uniform> u: ShadowUniforms;
@group(0) @binding(1) var height_tex: texture_2d<f32>;

@vertex
fn vs_shadow(v: VsIn) -> @builtin(position) vec4<f32> {
    // Shadow pass uses a unit UV grid spanning the world; world size is baked into
    // the light view-projection fit, so we recover world XZ from UV via the
    // orthographic fit centered on the terrain (UV * large extent handled on CPU).
    // The CPU orthographic frustum matches UV∈[0,1] → world XZ via the same
    // mapping as the main pass: we pass world through clipmap-style UV*extent
    // by encoding extent into the light matrix. Here UV maps to [0,1] and the
    // light matrix was built in world metres — reconstruct using texture size
    // as a stand-in: store world in UV by drawing the same TerrainGrid as main.
    //
    // TerrainGrid vertices are UV in [0,1]. Main pass does wx = uv * world_size.
    // Shadow uniforms' light_view_proj expects world metres, so the CPU must
    // encode that. We sample height and build world_pos with a fixed convention:
    // the light matrix includes the world fit; vertex UV is treated as normalised
    // world XZ and scaled by packing world size into params.zw when needed.
    // For simplicity params.z/w carry world_size_x / world_size_z.
    let world_x = v.uv.x * max(u.params.z, 1.0);
    let world_z = v.uv.y * max(u.params.w, 1.0);
    let dim = textureDimensions(height_tex);
    let tw = max(f32(dim.x) - 1.0, 1.0);
    let th = max(f32(dim.y) - 1.0, 1.0);
    let fx = clamp(v.uv.x, 0.0, 1.0) * tw;
    let fy = clamp(v.uv.y, 0.0, 1.0) * th;
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
    let surface_h = mix(mix(h00, h10, tx), mix(h01, h11, tx), ty);
    let h = select(surface_h, surface_h - 40.0, v.use_base > 0.5);
    // Skip underside faces in the shadow caster.
    if (v.face > 1.5) {
        return vec4<f32>(0.0, 0.0, -2.0, 1.0);
    }
    let world = vec3<f32>(world_x, h, world_z);
    return u.light_view_proj * vec4<f32>(world, 1.0);
}
