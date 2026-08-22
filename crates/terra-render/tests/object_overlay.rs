//! Placed props must actually reach the viewport.
//!
//! The Scatter Objects layer shipped able to place and export props that were
//! never drawn: the instance list stopped at the evaluator, and the only thing
//! on screen was the vegetation billboard overlay reading a density raster. So
//! the regression worth guarding is not "the shader compiles" but "syncing a
//! placement changes the rendered frame, at the place the prop was put".
//!
//! Skips silently without a GPU adapter (the terra-test-gpu convention).

use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::layer::ObjectInstance;
use terra_render::{GpuContext, TerrainRenderer, ViewportRendererMode};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const W: u32 = 160;
const H: u32 = 120;
const RES: u32 = 128;
const WORLD: f32 = 1024.0;

fn flat_terrain() -> Heightfield {
    Heightfield::filled(HeightfieldMetrics::new(RES, RES, WORLD, WORLD), 100.0)
}

fn prop(x: f32, z: f32, class_index: u32) -> ObjectInstance {
    ObjectInstance {
        class_index,
        class: format!("class{class_index}"),
        x,
        y: 100.0,
        z,
        scale: 1.0,
        yaw_rad: 0.4,
        normal: [0.0, 1.0, 0.0],
    }
}

fn diff_count(a: &terra_test_gpu::Pixels, b: &terra_test_gpu::Pixels) -> u32 {
    let mut n = 0;
    for y in 0..a.height() {
        for x in 0..a.width() {
            if a.get(x, y) != b.get(x, y) {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn syncing_props_changes_the_rendered_frame() {
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

    renderer.upload_heightfield(&flat_terrain());
    renderer.render_to_view(&target.view, W, H);
    let bare = gpu.read_rgba8(&target);

    // A cluster near the middle of the world, large enough to be unmissable.
    let props: Vec<ObjectInstance> = (0..24)
        .map(|k| {
            let a = k as f32 * 0.26;
            prop(
                WORLD * 0.5 + a.cos() * 60.0,
                WORLD * 0.5 + a.sin() * 60.0,
                k % 3,
            )
        })
        .collect();
    let (drawn, placed) = renderer.sync_object_instances(&props, 40.0);
    assert_eq!((drawn, placed), (props.len(), props.len()));

    renderer.render_to_view(&target.view, W, H);
    let with_props = gpu.read_rgba8(&target);

    assert!(
        diff_count(&bare, &with_props) > 40,
        "syncing {} props changed almost nothing on screen - the placements are \
         not reaching the viewport",
        props.len()
    );

    // ...and clearing them puts the frame back, so the overlay is not leaving
    // stale geometry resident.
    renderer.sync_object_instances(&[], 40.0);
    renderer.render_to_view(&target.view, W, H);
    let cleared = gpu.read_rgba8(&target);
    assert_eq!(
        diff_count(&bare, &cleared),
        0,
        "clearing the prop list left geometry on screen"
    );
}

/// The draw budget thins rather than truncates, and reports what it did.
#[test]
fn oversized_prop_sets_are_thinned_and_reported() {
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
    renderer.upload_heightfield(&flat_terrain());

    // Spread across the whole world so a prefix-truncating budget would leave
    // one edge populated and the rest bare.
    let n = 100_000usize;
    let props: Vec<ObjectInstance> = (0..n)
        .map(|k| {
            let u = k as f32 / n as f32;
            prop(u * WORLD, (u * 37.0).fract() * WORLD, (k % 3) as u32)
        })
        .collect();
    let (drawn, placed) = renderer.sync_object_instances(&props, 4.0);

    assert_eq!(placed, n, "the placed count must report every prop");
    assert!(drawn < placed, "this set is above the draw budget");
    assert!(
        drawn > n / 4,
        "thinning kept only {drawn} of {placed}; that is a truncation, not a stride"
    );
}
