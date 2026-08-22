//! A partial dirty-region update must not leave stale normals — or stale baked
//! AO, which rides in the normal texture's alpha — outside the dirty rect.
//!
//! `partial_region_upload.rs` covers the *height* half of this: both update
//! paths seed the write slot's height texture from the display slot before
//! patching it. The normal texture got no such seed.
//!
//! The CPU upload path is safe by accident — after seeding it recomputes normals
//! over the *full* field (`upload_regions_and_swap`, "recompute every normal
//! from it"). The GPU-to-GPU handoff does not: it dispatches only over the dirty
//! rect grown by one texel and then by the AO horizon radius
//! (`dilate_region_for_ao`), so every texel outside that dilation kept whatever
//! the write slot held two swaps ago — stale normals and, since AO rides in the
//! normal texture's alpha, a stale bake with it. This test drives that path.
//!
//! One full upload cannot show this: on the very first partial update the write
//! slot's normals are still the zeros it was created with, and a zero normal
//! reads as degenerate rather than as a *different, plausible* surface. This
//! test uploads two genuinely different fields first, so the write slot holds a
//! complete set of normals for the *wrong* terrain, then does a no-op partial
//! update and demands the frame be unchanged.
//!
//! Skips silently without a GPU adapter (the terra-test-gpu convention).

use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::tiling::SampleRect;
use terra_render::{GpuContext, TerrainRenderer, ViewportRendererMode};

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const W: u32 = 128;
const H: u32 = 128;
/// Matches `HeightGpu`'s initial slot size so nothing reallocates mid-test.
const RES: u32 = 256;

/// Two fields with the same min/max (so the camera framing latched on the first
/// present does not move) but opposite relief, giving each slot a full set of
/// normals for a visibly different surface.
fn ridged(res: u32, phase: f32) -> Heightfield {
    let metrics = HeightfieldMetrics::new(res, res, 1024.0, 1024.0);
    let mut field = Heightfield::new(metrics);
    for j in 0..res {
        for i in 0..res {
            let u = i as f32 / (res - 1) as f32;
            let v = j as f32 / (res - 1) as f32;
            let s = ((u * 9.0 + phase).sin() * (v * 7.0 + phase * 1.3).cos()).abs();
            field.set(i, j, s * 300.0);
        }
    }
    field
}

/// Upload a CPU heightfield into an R32Float texture the GPU->GPU path can read,
/// standing in for the compute engine's output.
fn upload_r32(gpu: &terra_test_gpu::TestGpu, hf: &Heightfield) -> wgpu::Texture {
    let (w, h) = (hf.metrics.width, hf.metrics.height);
    let tex = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test-src-height"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let data: Vec<f32> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .map(|(x, y)| hf.get(x, y))
        .collect();
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(&data),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(w * 4),
            rows_per_image: Some(h),
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    tex
}

fn diff_count(a: &terra_test_gpu::Pixels, b: &terra_test_gpu::Pixels) -> (u32, Option<(u32, u32)>) {
    let mut count = 0u32;
    let mut first = None;
    for y in 0..a.height() {
        for x in 0..a.width() {
            if a.get(x, y) != b.get(x, y) {
                count += 1;
                if first.is_none() {
                    first = Some((x, y));
                }
            }
        }
    }
    (count, first)
}

#[test]
fn partial_update_preserves_normals_and_baked_ao_outside_the_dirty_rect() {
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

    // Two full uploads, so *both* double-buffer slots end up holding a complete
    // set of normals — the write slot's belonging to the other terrain.
    let src_a = upload_r32(gpu, &ridged(RES, 0.0));
    let src_b = upload_r32(gpu, &ridged(RES, 2.1));
    let world = (1024.0f32, 1024.0f32);
    let range = (0.0f32, 300.0f32);
    let (dx, dz) = (world.0 / RES as f32, world.1 / RES as f32);

    renderer.present_gpu_height(&src_a, RES, RES, world, range, dx, dz);
    renderer.render_to_view(&target.view, W, H);
    renderer.present_gpu_height(&src_b, RES, RES, world, range, dx, dz);
    renderer.render_to_view(&target.view, W, H);
    let reference = gpu.read_rgba8(&target);

    // Re-upload the *same* field, marking one small rect dirty. Correct
    // behaviour: an identical frame. Without the normal-texture seed copy, every
    // texel outside the AO-dilated rect renders with the first field's normals
    // and its baked AO.
    let dirty = SampleRect {
        x: RES / 2 - 16,
        y: RES / 2 - 16,
        w: 32,
        h: 32,
    };
    renderer.present_gpu_height_region(&src_b, RES, RES, world, range, dx, dz, Some(dirty));
    renderer.render_to_view(&target.view, W, H);
    let partial = gpu.read_rgba8(&target);

    let (count, first) = diff_count(&reference, &partial);
    assert_eq!(
        count, 0,
        "a no-op partial update changed {count} pixel(s) (first at {first:?}); \
         normals and baked AO outside the dirty rect reverted to the write \
         slot's previous terrain"
    );
}
