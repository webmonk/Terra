//! GPU compute for Terra (WGSL). CPU references in `terra-core` remain the test oracle.

pub mod buffer_pool;
pub mod engine;
pub mod hydraulic;
pub mod memory;
pub mod parity;
pub mod thermal;

pub use engine::{layer_gpu_supported, GpuEvalResult, GpuTerrainEngine};

use bytemuck::{Pod, Zeroable};
use std::sync::Mutex;
use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use thiserror::Error;
use wgpu::util::DeviceExt;

#[derive(Debug, Error)]
pub enum GpuError {
    #[error("wgpu: {0}")]
    Wgpu(String),
    #[error("no adapter")]
    NoAdapter,
}

pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub buffers: Mutex<buffer_pool::BufferPool>,
}

impl GpuContext {
    pub fn new() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .ok_or(GpuError::NoAdapter)?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("terra-gpu"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .map_err(|e| GpuError::Wgpu(e.to_string()))?;

        Ok(Self {
            buffers: Mutex::new(buffer_pool::BufferPool::new(device.clone())),
            device,
            queue,
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SimUniforms {
    pub width: u32,
    pub height: u32,
    pub dx: f32,
    pub params0: f32,
    pub params1: f32,
    pub params2: f32,
    pub params3: f32,
    pub _pad: f32,
}

pub fn heightfield_to_buffer(device: &wgpu::Device, hf: &Heightfield) -> wgpu::Buffer {
    let data = hf.to_dense();
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("heightfield"),
        contents: bytemuck::cast_slice(&data),
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
    })
}

pub fn readback_f32(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buffer: &wgpu::Buffer,
    n: usize,
) -> Result<Vec<f32>, GpuError> {
    let size = (n * 4) as u64;
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("readback-enc"),
    });
    encoder.copy_buffer_to_buffer(buffer, 0, &staging, 0, size);
    queue.submit(Some(encoder.finish()));
    let slice = staging.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv()
        .map_err(|e| GpuError::Wgpu(e.to_string()))?
        .map_err(|e| GpuError::Wgpu(e.to_string()))?;
    let data = slice.get_mapped_range();
    let out: Vec<f32> = bytemuck::cast_slice(&data).to_vec();
    drop(data);
    staging.unmap();
    Ok(out)
}

pub fn metrics_uniforms(
    metrics: HeightfieldMetrics,
    p0: f32,
    p1: f32,
    p2: f32,
    p3: f32,
) -> SimUniforms {
    SimUniforms {
        width: metrics.width,
        height: metrics.height,
        dx: metrics.dx(),
        params0: p0,
        params1: p1,
        params2: p2,
        params3: p3,
        _pad: 0.0,
    }
}
