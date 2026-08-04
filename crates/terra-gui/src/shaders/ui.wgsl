struct Frame {
    // screen_w, screen_h, unused, unused (framebuffer px)
    screen: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Frame;
@group(0) @binding(1) var font_atlas: texture_2d<f32>;
@group(0) @binding(2) var font_samp: sampler;
@group(0) @binding(3) var image_tex: texture_2d<f32>;
@group(0) @binding(4) var image_samp: sampler;

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    // 0 = solid, 1 = font atlas (R), 2 = RGBA image, 3 = rounded solid
    @location(3) mode: f32,
    @location(4) radius: f32,
    @location(5) size: vec2<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) mode: f32,
    @location(3) radius: f32,
    @location(4) size: vec2<f32>,
};

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var o: VsOut;
    let ndc_x = (v.pos.x / max(u.screen.x, 1.0)) * 2.0 - 1.0;
    let ndc_y = 1.0 - (v.pos.y / max(u.screen.y, 1.0)) * 2.0;
    o.clip = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    o.uv = v.uv;
    o.color = v.color;
    o.mode = v.mode;
    o.radius = v.radius;
    o.size = v.size;
    return o;
}

fn sd_rounded_box(p: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let rr = min(r, min(half.x, half.y));
    let q = abs(p) - half + vec2<f32>(rr, rr);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - rr;
}

@fragment
fn fs_main(i: VsOut) -> @location(0) vec4<f32> {
    // Premultiplied alpha output — matches PREMULTIPLIED_ALPHA_BLENDING.
    if (i.mode > 2.5) {
        // Rounded solid: uv is local px from top-left of the quad.
        let half = i.size * 0.5;
        let p = i.uv - half;
        let d = sd_rounded_box(p, half, i.radius);
        let a = clamp(1.0 - smoothstep(-0.75, 0.75, d), 0.0, 1.0) * i.color.a;
        if (a < 0.004) {
            discard;
        }
        return vec4<f32>(i.color.rgb * a, a);
    }
    if (i.mode > 1.5) {
        let texel = textureSampleLevel(image_tex, image_samp, i.uv, 0.0);
        let a = texel.a * i.color.a;
        return vec4<f32>(texel.rgb * a, a);
    }
    if (i.mode > 0.5) {
        // Fontdue already anti-aliases into the R8 atlas. Use coverage as-is —
        // soft-thresholding here double-blurs strokes and makes text look muddy.
        let coverage = textureSampleLevel(font_atlas, font_samp, i.uv, 0.0).r;
        let a = coverage * i.color.a;
        if (a < 0.004) {
            discard;
        }
        return vec4<f32>(i.color.rgb * a, a);
    }
    let a = i.color.a;
    return vec4<f32>(i.color.rgb * a, a);
}
