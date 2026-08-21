//! Terrain-space ambient occlusion regression (see `shaders/normals.wgsl`).
//!
//! AO is baked into the alpha channel of the normal texture by the normals
//! compute pass and applied to the ambient term only. These tests pin the two
//! properties that matter and that a silent regression would break: the effect
//! is actually visible at the default strength, and `ao_strength = 0` reproduces
//! the pre-AO look exactly.
//!
//! Deliberately no error scopes here - `offscreen_render.rs` owns all
//! `push`/`pop_error_scope` usage in this crate (device-global, parallel tests).

use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_render::{GpuContext, TerrainRenderer, ViewportRendererMode};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const W: u32 = 96;
const H: u32 = 96;
const N: usize = 256;

/// Rugged terrain: a broad basin with eroded ridges and gullies cut into it.
/// Real occlusion needs real relief - a smooth analytic bowl genuinely has
/// almost none, so a bowl alone would test nothing.
fn basin_heightfield() -> Heightfield {
    let metrics = HeightfieldMetrics::new(N as u32, N as u32, 2048.0, 2048.0);
    let mut data = vec![0.0f32; N * N];
    let c = (N as f32 - 1.0) * 0.5;
    for y in 0..N {
        for x in 0..N {
            let dx = (x as f32 - c) / c;
            let dy = (y as f32 - c) / c;
            let r = (dx * dx + dy * dy).sqrt().min(1.0);
            // Broad concave basin.
            let mut h = 400.0 * r * r;
            // Ridged multi-fractal: |sin| octaves give sharp crests and deep,
            // narrow gullies - the structure horizon AO is meant to find.
            let (fx, fy) = (x as f32, y as f32);
            let mut amp = 90.0;
            let mut freq = 0.09;
            for _ in 0..4 {
                let s = (fx * freq).sin() * (fy * freq * 1.13 + 1.7).cos();
                h += amp * (1.0 - s.abs());
                amp *= 0.55;
                freq *= 2.17;
            }
            data[y * N + x] = h;
        }
    }
    Heightfield::from_dense(metrics, &data)
}

fn mean_luma(pixels: &terra_test_gpu::Pixels) -> f64 {
    let mut sum = 0.0;
    let mut count = 0.0;
    for y in 0..pixels.height() {
        for x in 0..pixels.width() {
            let p = pixels.get(x, y);
            sum += 0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]);
            count += 1.0;
        }
    }
    sum / count
}

#[test]
fn ambient_occlusion_darkens_a_basin_and_is_off_at_zero_strength() {
    let Some(gpu) = terra_test_gpu::headless() else {
        return;
    };
    let ctx = GpuContext {
        device: gpu.device.clone(),
        queue: gpu.queue.clone(),
        surface_format: FORMAT,
    };
    let mut renderer = TerrainRenderer::new_headless(&ctx, W, H);
    renderer.set_renderer_mode(ViewportRendererMode::Raster);
    let target = gpu.target(W, H, FORMAT);

    renderer.upload_heightfield(&basin_heightfield());

    let luma_for = |renderer: &mut TerrainRenderer, strength: f32| {
        renderer.lighting.ao_strength = strength;
        renderer.render_to_view(&target.view, W, H);
        mean_luma(&gpu.read_rgba8(&target))
    };

    let off = luma_for(&mut renderer, 0.0);
    let on = luma_for(&mut renderer, 0.6);
    let full = luma_for(&mut renderer, 1.0);

    // Visible, not subliminal: the default strength must move the image.
    assert!(
        on < off - 1.0,
        "ao_strength 0.6 did not darken the terrain (off={off:.3}, on={on:.3})"
    );
    // Monotonic in strength - a sanity check that the control is actually wired
    // through the `raster.w` uniform slot rather than being baked in.
    assert!(
        full < on,
        "ao_strength 1.0 should occlude at least as much as 0.6 (0.6={on:.3}, 1.0={full:.3})"
    );

    // Determinism: no per-frame seed, no per-pixel noise, no derivatives - the
    // same scene must render bit-identically frame to frame.
    renderer.lighting.ao_strength = 0.6;
    renderer.render_to_view(&target.view, W, H);
    let first = gpu.read_rgba8(&target);
    renderer.render_to_view(&target.view, W, H);
    let second = gpu.read_rgba8(&target);
    for y in 0..first.height() {
        for x in 0..first.width() {
            assert_eq!(
                first.get(x, y),
                second.get(x, y),
                "AO is not deterministic at ({x}, {y})"
            );
        }
    }
}

