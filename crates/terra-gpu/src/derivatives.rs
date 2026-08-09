//! GPU preview kernels for local terrain derivatives.
//!
//! CPU [`terra_core::geomorph`] remains the export oracle. These kernels match
//! the finite-difference forms used by `geomorph::derivatives` for slope /
//! gradient / laplacian / aspect at a world-space texel radius.

use bytemuck::{Pod, Zeroable};
use terra_core::heightfield::Heightfield;
use terra_core::mask::MaskField;
use terra_core::terrain_eval::world_radius_texels;

use crate::{readback_f32, GpuContext, GpuError};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct DerivativeUniforms {
    width: u32,
    height: u32,
    radius_texels: u32,
    mode: u32,
    dx: f32,
    dz: f32,
    _pad0: f32,
    _pad1: f32,
}

/// Which derivative field to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuDerivativeMode {
    GradientMagnitude = 0,
    Slope = 1,
    Laplacian = 2,
    Aspect = 3,
}

/// Run a local derivative kernel on `height` (full-field upload / readback).
pub fn run_derivative_gpu(
    ctx: &GpuContext,
    height: &Heightfield,
    radius_m: f32,
    mode: GpuDerivativeMode,
) -> Result<MaskField, GpuError> {
    let m = height.metrics;
    let w = m.width;
    let h = m.height;
    let radius_texels = world_radius_texels(radius_m, m).round().max(1.0) as u32;

    let shader = ctx
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("derivatives"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/derivatives.wgsl").into()),
        });

    let bgl = ctx
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("derivatives-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::R32Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });

    let pipeline_layout = ctx
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("derivatives-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
    let pipeline = ctx
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("derivatives-pipe"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

    let src = create_r32_texture(&ctx.device, "deriv-src", w, h);
    let dst = create_r32_texture_storage(&ctx.device, "deriv-dst", w, h);
    upload_height(&ctx.queue, &src, height);

    let uniforms = DerivativeUniforms {
        width: w,
        height: h,
        radius_texels,
        mode: mode as u32,
        dx: m.dx(),
        dz: m.dz(),
        _pad0: 0.0,
        _pad1: 0.0,
    };
    let ubuf = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("deriv-uniform"),
        size: std::mem::size_of::<DerivativeUniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    ctx.queue
        .write_buffer(&ubuf, 0, bytemuck::bytes_of(&uniforms));

    let src_view = src.create_view(&wgpu::TextureViewDescriptor::default());
    let dst_view = dst.create_view(&wgpu::TextureViewDescriptor::default());
    let bind = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("deriv-bg"),
        layout: &bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: ubuf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&src_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&dst_view),
            },
        ],
    });

    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("deriv-enc"),
        });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("deriv-pass"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bind, &[]);
        pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
    }

    let n = (w * h) as usize;
    let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("deriv-readback"),
        size: (n * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &dst,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    ctx.queue.submit(Some(encoder.finish()));

    let data = readback_f32(&ctx.device, &ctx.queue, &readback, n)?;
    Ok(MaskField::from_raw(m, &data))
}

fn create_r32_texture(device: &wgpu::Device, label: &str, w: u32, h: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
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
    })
}

fn create_r32_texture_storage(device: &wgpu::Device, label: &str, w: u32, h: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R32Float,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn upload_height(queue: &wgpu::Queue, tex: &wgpu::Texture, height: &Heightfield) {
    let dense = height.to_dense();
    let w = height.metrics.width;
    let h = height.metrics.height;
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(&dense),
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
}

/// CPU oracle comparison helper for tests.
pub fn cpu_slope_oracle(height: &Heightfield, radius_m: f32) -> MaskField {
    terra_core::geomorph::slope_magnitude(height, radius_m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GpuError;
    use terra_core::heightfield::HeightfieldMetrics;

    #[test]
    fn derivative_shader_compiles_when_adapter_available() {
        let Ok(ctx) = GpuContext::new() else {
            return;
        };
        let m = HeightfieldMetrics::new(32, 32, 320.0, 320.0);
        let mut hf = Heightfield::zeros(m);
        for j in 0..32 {
            for i in 0..32 {
                hf.set(i, j, i as f32 * 10.0);
            }
        }
        match run_derivative_gpu(&ctx, &hf, 0.0, GpuDerivativeMode::Slope) {
            Ok(gpu) => {
                let cpu = cpu_slope_oracle(&hf, 0.0);
                let mut max_err = 0.0f32;
                for j in 2..30 {
                    for i in 2..30 {
                        max_err = max_err.max((gpu.get(i, j) - cpu.get(i, j)).abs());
                    }
                }
                assert!(max_err < 0.05, "gpu/cpu slope err={max_err}");
            }
            Err(GpuError::NoAdapter) => {}
            Err(e) => panic!("{e}"),
        }
    }
}
