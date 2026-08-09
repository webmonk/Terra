//! GPU compute for Terra (WGSL). CPU references in `terra-core` remain the test oracle.
//!
//! Interactive hard rules: GPU-resident heightfields, no UI-thread readback, no mesh rebuild,
//! dirty-tile compute, incomplete GPU prefixes are never treated as finished Draft.

pub mod derivatives;
pub mod effect_filter;
pub mod engine;
pub mod graph;
pub mod memory;
pub mod parity;
pub mod tile_cache;

pub use derivatives::{cpu_slope_oracle, run_derivative_gpu, GpuDerivativeMode};
pub use engine::{layer_gpu_supported, GpuEvalResult, GpuTerrainEngine};
pub use graph::{compile_gpu_graph, expand_dirty_rect, GpuComputeGraph, GpuPass, GpuPassKind};
pub use tile_cache::{GpuPageTableEntry, GpuTileAtlas, GpuTileCacheError, GpuTileUpload};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GpuError {
    #[error("wgpu: {0}")]
    Wgpu(String),
    #[error("no adapter")]
    NoAdapter,
    /// Stack needs the CPU tree evaluator (scoped groups / unsupported layers).
    #[error("cpu evaluation required")]
    RequiresCpu,
}

pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

fn preferred_backends() -> wgpu::Backends {
    if let Ok(raw) = std::env::var("WGPU_BACKEND") {
        let mut backends = wgpu::Backends::empty();
        for part in raw.split([',', '|']).map(|p| p.trim().to_ascii_lowercase()) {
            match part.as_str() {
                "vulkan" | "vk" => backends |= wgpu::Backends::VULKAN,
                "dx12" | "d3d12" => backends |= wgpu::Backends::DX12,
                "metal" => backends |= wgpu::Backends::METAL,
                "gl" | "gles" => backends |= wgpu::Backends::GL,
                "primary" => return wgpu::Backends::PRIMARY,
                "all" => return wgpu::Backends::all(),
                _ => {}
            }
        }
        if !backends.is_empty() {
            return backends;
        }
    }
    if cfg!(target_os = "windows") {
        wgpu::Backends::DX12
    } else {
        wgpu::Backends::PRIMARY
    }
}

impl GpuContext {
    pub fn new() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: preferred_backends(),
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

        Ok(Self { device, queue })
    }
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
