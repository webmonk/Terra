// Instanced solid props for the 3D viewport.
//
// Deliberately not the vegetation path: that draws crossed alpha cards from a
// density map with its own hashing, which is right for foliage and wrong for a
// boulder or a crate. Props arrive as exact placements - position, scale, yaw
// and an up vector - so this draws a lit solid at each one.

struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexIn {
    @location(0) local_pos: vec3<f32>,
    @location(1) local_normal: vec3<f32>,
    // xyz = world position, w = uniform scale in metres.
    @location(2) position_scale: vec4<f32>,
    // xyz = surface up vector, w = yaw in radians.
    @location(3) up_yaw: vec4<f32>,
    @location(4) color: vec4<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec3<f32>,
};

// Orthonormal basis with `up` as +Y, yawed about it. Props on a slope lean with
// the ground when their class asks to align, and stand upright when it does not
// (the CPU side passes +Y in that case, so there is one path here).
fn basis(up_in: vec3<f32>, yaw: f32) -> mat3x3<f32> {
    let up = normalize(select(vec3<f32>(0.0, 1.0, 0.0), up_in, length(up_in) > 1e-4));
    // Any vector not parallel to `up` seeds the tangent; swap the reference when
    // up is close to +X so the cross product never degenerates.
    let seed = select(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 0.0, 1.0), abs(up.x) > 0.9);
    let t0 = normalize(cross(seed, up));
    let b0 = cross(up, t0);
    let c = cos(yaw);
    let s = sin(yaw);
    let t = t0 * c + b0 * s;
    let b = b0 * c - t0 * s;
    return mat3x3<f32>(t, up, b);
}

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    var o: VertexOut;
    let m = basis(v.up_yaw.xyz, v.up_yaw.w);
    let scale = v.position_scale.w;
    let world = v.position_scale.xyz + m * (v.local_pos * scale);
    o.position = u.view_proj * vec4<f32>(world, 1.0);
    o.normal = normalize(m * v.local_normal);
    o.color = v.color.rgb;
    return o;
}

@fragment
fn fs_main(i: VertexOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    let sun = max(dot(n, normalize(-u.light_dir.xyz)), 0.0);
    // Cheap sky term so faces pointing up never go fully black in shadow.
    let sky = 0.28 + 0.22 * max(n.y, 0.0);
    let lit = i.color * (sky + sun * 0.85 * max(u.light_dir.w, 0.0));
    return vec4<f32>(lit, 1.0);
}
