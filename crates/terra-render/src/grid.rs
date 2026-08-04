//! Static XZ terrain grid — never rebuilt when heights change.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GridVertex {
    pub uv: [f32; 2],
}

pub struct TerrainGrid {
    pub vertex_buf: wgpu::Buffer,
    pub index_buf: wgpu::Buffer,
    pub index_count: u32,
    pub resolution: u32,
}

impl TerrainGrid {
    pub fn new(device: &wgpu::Device, resolution: u32) -> Self {
        let res = resolution.max(2);
        let mut verts = Vec::with_capacity((res * res) as usize);
        for z in 0..res {
            for x in 0..res {
                verts.push(GridVertex {
                    uv: [x as f32 / (res - 1) as f32, z as f32 / (res - 1) as f32],
                });
            }
        }
        let mut indices = Vec::with_capacity(((res - 1) * (res - 1) * 6) as usize);
        for z in 0..res - 1 {
            for x in 0..res - 1 {
                let i0 = z * res + x;
                let i1 = i0 + 1;
                let i2 = i0 + res;
                let i3 = i2 + 1;
                indices.extend_from_slice(&[i0, i2, i1, i1, i2, i3]);
            }
        }
        Self {
            vertex_buf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("terrain-grid-v"),
                contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX,
            }),
            index_buf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("terrain-grid-i"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            index_count: indices.len() as u32,
            resolution: res,
        }
    }

    pub fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GridVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[wgpu::VertexAttribute {
                offset: 0,
                shader_location: 0,
                format: wgpu::VertexFormat::Float32x2,
            }],
        }
    }
}
