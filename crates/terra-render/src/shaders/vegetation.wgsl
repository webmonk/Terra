struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexIn {
    @location(0) local_pos: vec3<f32>,
    @location(1) local_uv: vec2<f32>,
    @location(2) position_scale: vec4<f32>,
    @location(3) color_phase: vec4<f32>,
};

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec3<f32>,
    @location(2) world_pos: vec3<f32>,
};

@vertex
fn vs_main(v: VertexIn) -> VertexOut {
    var o: VertexOut;
    let scale = v.position_scale.w;
    let width = scale * 0.52;
    let lx = v.local_pos.x * width;
    let lz = v.local_pos.z * width;
    let yaw = v.color_phase.w;
    let c = cos(yaw);
    let s = sin(yaw);
    let local = vec3<f32>(lx * c - lz * s, v.local_pos.y * scale, lx * s + lz * c);
    let world = v.position_scale.xyz + local;
    o.position = u.view_proj * vec4<f32>(world, 1.0);
    o.uv = v.local_uv;
    o.color = v.color_phase.rgb;
    o.world_pos = world;
    return o;
}

@fragment
fn fs_main(i: VertexOut) -> @location(0) vec4<f32> {
    // Soft triangular crown with a darker trunk/understory base.
    let edge = 1.0 - abs(i.uv.x * 2.0 - 1.0);
    let crown = smoothstep(0.0, 0.18, edge - i.uv.y * 0.08);
    if (crown < 0.04) { discard; }
    let n = normalize(vec3<f32>((i.uv.x - 0.5) * 0.8, 0.75, 0.25));
    let sun = max(dot(n, normalize(-u.light_dir.xyz)), 0.0);
    let vertical = mix(0.55, 1.08, smoothstep(0.0, 1.0, i.uv.y));
    let color = i.color * vertical * (0.42 + sun * 0.72 * max(u.light_dir.w, 0.0));
    return vec4<f32>(color, crown);
}
