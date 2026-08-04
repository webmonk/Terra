struct Uniforms {
    width: u32,
    height: u32,
    timestep: f32,
    rainfall: f32,
    evaporation: f32,
    erosion: f32,
    deposition: f32,
    capacity: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var height_src: texture_2d<f32>;
@group(0) @binding(2) var water_src: texture_2d<f32>;
@group(0) @binding(3) var sed_src: texture_2d<f32>;
@group(0) @binding(4) var height_dst: texture_storage_2d<r32float, write>;
@group(0) @binding(5) var water_dst: texture_storage_2d<r32float, write>;
@group(0) @binding(6) var sed_dst: texture_storage_2d<r32float, write>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    let j = gid.y;
    if (i >= u.width || j >= u.height) { return; }
    let p = vec2<i32>(i32(i), i32(j));
    let h0 = textureLoad(height_src, p, 0).r;
    var w = textureLoad(water_src, p, 0).r + u.rainfall;
    var s = textureLoad(sed_src, p, 0).r;
    let total = h0 + w;

    var outflow = 0.0;
    let n0 = vec2<i32>(p.x - 1, p.y);
    if (n0.x >= 0) {
        let d = total - (textureLoad(height_src, n0, 0).r + textureLoad(water_src, n0, 0).r);
        if (d > 0.0) { outflow += d; }
    }
    let n1 = vec2<i32>(p.x + 1, p.y);
    if (n1.x < i32(u.width)) {
        let d = total - (textureLoad(height_src, n1, 0).r + textureLoad(water_src, n1, 0).r);
        if (d > 0.0) { outflow += d; }
    }
    let n2 = vec2<i32>(p.x, p.y - 1);
    if (n2.y >= 0) {
        let d = total - (textureLoad(height_src, n2, 0).r + textureLoad(water_src, n2, 0).r);
        if (d > 0.0) { outflow += d; }
    }
    let n3 = vec2<i32>(p.x, p.y + 1);
    if (n3.y < i32(u.height)) {
        let d = total - (textureLoad(height_src, n3, 0).r + textureLoad(water_src, n3, 0).r);
        if (d > 0.0) { outflow += d; }
    }

    var h = h0;
    if (outflow > 0.0 && w > 0.0) {
        let flow = min(outflow * u.timestep, w);
        w -= flow;
        let slope = outflow * 0.25;
        let cap = u.capacity * slope * flow;
        if (s < cap) {
            let erode_amt = min((cap - s) * u.erosion, max(h, 0.0) * 0.1);
            h -= erode_amt;
            s += erode_amt;
        } else {
            let dep = (s - cap) * u.deposition;
            h += dep;
            s -= dep;
        }
    }
    w *= (1.0 - u.evaporation);
    textureStore(height_dst, p, vec4<f32>(h, 0.0, 0.0, 0.0));
    textureStore(water_dst, p, vec4<f32>(w, 0.0, 0.0, 0.0));
    textureStore(sed_dst, p, vec4<f32>(max(s, 0.0), 0.0, 0.0, 0.0));
}
