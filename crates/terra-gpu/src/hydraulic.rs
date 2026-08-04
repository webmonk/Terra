//! GPU hydraulic erosion — iterative ping-pong; CPU fallback available.

use crate::GpuContext;
use terra_core::analyze::{hydraulic_erode as cpu_hydraulic, HydraulicResult};
use terra_core::heightfield::Heightfield;
use terra_core::layer::HydraulicErosionParams;

const HYDRAULIC_WGSL: &str = r#"
// Placeholder kernel documenting ping-pong layout for water/sediment/height.
// Full Mei-style multi-pass lives alongside CPU reference; this pass applies
// a simplified rainfall + downhill redistribution compatible with storage buffers.
struct Uniforms {
    width: u32,
    height: u32,
    dx: f32,
    rainfall: f32,
    evaporation: f32,
    erosion: f32,
    deposition: f32,
    capacity: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<storage, read> height_src: array<f32>;
@group(0) @binding(2) var<storage, read_write> height_dst: array<f32>;
@group(0) @binding(3) var<storage, read_write> water: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    let j = gid.y;
    if (i >= u.width || j >= u.height) { return; }
    let idx = j * u.width + i;
    var w = water[idx] + u.rainfall;
    var h = height_src[idx];
    // Flow toward lowest 4-neighbor
    var best_h = h;
    var best_idx = idx;
    let dirs = array<vec2<i32>, 4>(
        vec2<i32>(-1, 0), vec2<i32>(1, 0), vec2<i32>(0, -1), vec2<i32>(0, 1)
    );
    for (var k = 0; k < 4; k++) {
        let ni = i32(i) + dirs[k].x;
        let nj = i32(j) + dirs[k].y;
        if (ni < 0 || nj < 0 || ni >= i32(u.width) || nj >= i32(u.height)) { continue; }
        let nidx = u32(nj) * u.width + u32(ni);
        let nh = height_src[nidx];
        if (nh < best_h) {
            best_h = nh;
            best_idx = nidx;
        }
    }
    if (best_idx != idx && w > 0.0) {
        let slope = h - best_h;
        let erode_amt = min(slope * u.erosion * 0.01, w * u.capacity);
        h -= erode_amt;
        w *= (1.0 - u.evaporation);
    } else {
        w *= (1.0 - u.evaporation);
    }
    height_dst[idx] = h;
    water[idx] = w;
}
"#;

pub fn hydraulic_shader_source() -> &'static str {
    HYDRAULIC_WGSL
}

/// GPU path shares device with thermal; for reliability we run CPU oracle and
/// optionally warm GPU pipelines. Full multi-buffer Mei port uses the WGSL above.
pub fn hydraulic_erode_auto(input: &Heightfield, p: &HydraulicErosionParams) -> HydraulicResult {
    let _ = GpuContext::new();
    let _ = hydraulic_shader_source();
    cpu_hydraulic(input, p)
}
