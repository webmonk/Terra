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
    textureStore(normal_tex, vec2<i32>(i32(x), i32(y)), vec4<f32>(n, 1.0));
}
