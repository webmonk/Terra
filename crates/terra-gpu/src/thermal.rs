//! GPU thermal erosion (talus). Falls back to CPU if device init fails in tests.

use crate::{metrics_uniforms, readback_f32, GpuContext, GpuError};
use terra_core::analyze::thermal_erode as cpu_thermal;
use terra_core::heightfield::Heightfield;
use terra_core::layer::ThermalErosionParams;
use terra_core::mask::MaskField;
use wgpu::util::DeviceExt;

const THERMAL_WGSL: &str = r#"
struct Uniforms {
    width: u32,
    height: u32,
    dx: f32,
    talus: f32,
    strength: f32,
    _p2: f32,
    _p3: f32,
    _pad: f32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<storage, read> src: array<f32>;
@group(0) @binding(2) var<storage, read_write> dst: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    let j = gid.y;
    if (i >= u.width || j >= u.height) { return; }
    let idx = j * u.width + i;
    let h0 = src[idx];
    var sum = 0.0;
    var count = 0.0;
    let dirs = array<vec2<i32>, 4>(
        vec2<i32>(-1, 0), vec2<i32>(1, 0), vec2<i32>(0, -1), vec2<i32>(0, 1)
    );
    for (var k = 0; k < 4; k++) {
        let ni = i32(i) + dirs[k].x;
        let nj = i32(j) + dirs[k].y;
        if (ni < 0 || nj < 0 || ni >= i32(u.width) || nj >= i32(u.height)) { continue; }
        let nidx = u32(nj) * u.width + u32(ni);
        let diff = h0 - src[nidx] - u.talus;
        if (diff > 0.0) {
            sum += diff;
            count += 1.0;
        }
    }
    var h = h0;
    if (sum > 0.0 && count > 0.0) {
        let move_amt = sum * u.strength * 0.25;
        h -= move_amt;
    }
    dst[idx] = h;
}
"#;

pub fn thermal_erode_gpu(
    ctx: &GpuContext,
    input: &Heightfield,
    p: &ThermalErosionParams,
) -> Result<(Heightfield, MaskField, MaskField), GpuError> {
    let n = (input.metrics.width * input.metrics.height) as usize;
    let talus = p.talus_angle_deg.to_radians().tan() * input.metrics.dx();
    let uniforms = metrics_uniforms(input.metrics, talus, p.strength, 0.0, 0.0);

    let uniform_buf = ctx
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("thermal-u"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });

    let buffer_size = (n * 4) as u64;
    let usage =
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::COPY_DST;
    let (mut a, mut b) = {
        let mut pool = ctx.buffers.lock().expect("GPU buffer pool poisoned");
        (
            pool.acquire(buffer_size, usage),
            pool.acquire(buffer_size, usage),
        )
    };
    ctx.queue
        .write_buffer(&a, 0, bytemuck::cast_slice(&input.to_dense()));

    let shader = ctx
        .device
        .create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("thermal"),
            source: wgpu::ShaderSource::Wgsl(THERMAL_WGSL.into()),
        });

    let layout = ctx
        .device
        .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("thermal-bgl"),
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
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: false },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

    let pipeline_layout = ctx
        .device
        .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("thermal-pl"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

    let pipeline = ctx
        .device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("thermal-pipe"),
            layout: Some(&pipeline_layout),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

    let wx = (input.metrics.width + 7) / 8;
    let wy = (input.metrics.height + 7) / 8;

    for _ in 0..p.iterations {
        let bind = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("thermal-bg"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: a.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: b.as_entire_binding(),
                },
            ],
        });
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("thermal-enc"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("thermal-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(wx, wy, 1);
        }
        ctx.queue.submit(Some(encoder.finish()));
        std::mem::swap(&mut a, &mut b);
    }

    let data = readback_f32(&ctx.device, &ctx.queue, &a, n);
    {
        let mut pool = ctx.buffers.lock().expect("GPU buffer pool poisoned");
        pool.release(buffer_size, a);
        pool.release(buffer_size, b);
    }
    let data = data?;
    let height = Heightfield::from_dense(input.metrics, &data);
    // Masks from CPU comparison path for reuse
    let (_, _erosion, _deposit) = cpu_thermal(input, p);
    // Derive simple erosion mask from delta
    let mut e = MaskField::zeros(input.metrics);
    let mut d = MaskField::zeros(input.metrics);
    for j in 0..input.metrics.height {
        for i in 0..input.metrics.width {
            let delta = input.get(i, j) - height.get(i, j);
            if delta > 0.0 {
                e.set(i, j, delta);
            } else {
                d.set(i, j, -delta);
            }
        }
    }
    Ok((height, e, d))
}

/// Prefer GPU; on failure use CPU reference.
pub fn thermal_erode_auto(
    input: &Heightfield,
    p: &ThermalErosionParams,
) -> (Heightfield, MaskField, MaskField) {
    match GpuContext::new().and_then(|ctx| thermal_erode_gpu(&ctx, input, p)) {
        Ok(v) => v,
        Err(_) => cpu_thermal(input, p),
    }
}
