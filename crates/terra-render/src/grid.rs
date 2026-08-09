//! Static XZ terrain grid — never rebuilt when heights change.
//!
//! Includes vertical skirt walls and a flat underside so the heightfield reads as a
//! solid block (World Creator–style) rather than a paper-thin sheet.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GridVertex {
    /// Terrain UV in 0..1 (maps to world XZ).
    pub uv: [f32; 2],
    /// 0 = top surface, 1 = skirt wall, 2 = underside.
    pub face: f32,
    /// 0 = sample heightfield, 1 = use slab base (`grid.w`).
    pub use_base: f32,
}

pub struct TerrainGrid {
    pub vertex_buf: wgpu::Buffer,
    pub index_buf: wgpu::Buffer,
    /// Surface + skirts + underside (opaque terrain draw).
    pub index_count: u32,
    /// Top surface only (ocean / water plane).
    pub surface_index_count: u32,
    pub edge_index_buf: wgpu::Buffer,
    pub edge_index_count: u32,
    pub resolution: u32,
}

impl TerrainGrid {
    pub fn new(device: &wgpu::Device, resolution: u32) -> Self {
        let res = resolution.max(2);
        let (verts, indices, surface_index_count, edges) = build_solid_grid(res);
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
            surface_index_count,
            edge_index_buf: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("terrain-grid-edges"),
                contents: bytemuck::cast_slice(&edges),
                usage: wgpu::BufferUsages::INDEX,
            }),
            edge_index_count: edges.len() as u32,
            resolution: res,
        }
    }

    pub fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GridVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: 8,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: 12,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32,
                },
            ],
        }
    }
}

fn build_solid_grid(res: u32) -> (Vec<GridVertex>, Vec<u32>, u32, Vec<u32>) {
    let mut verts = Vec::with_capacity((res * res + 8 * res + 4) as usize);
    for z in 0..res {
        for x in 0..res {
            verts.push(GridVertex {
                uv: [x as f32 / (res - 1) as f32, z as f32 / (res - 1) as f32],
                face: 0.0,
                use_base: 0.0,
            });
        }
    }

    let mut indices = grid_indices(res);
    let surface_index_count = indices.len() as u32;

    // Dedicated wall verts (do not share with the surface) so face/normals stay correct.
    let mut wall_top_of = vec![u32::MAX; (res * res) as usize];
    let mut wall_bot_of = vec![u32::MAX; (res * res) as usize];
    let mark_wall = |verts: &mut Vec<GridVertex>,
                     idx: u32,
                     cache: &mut [u32],
                     use_base: f32|
     -> u32 {
        let i = idx as usize;
        if cache[i] != u32::MAX {
            return cache[i];
        }
        let uv = verts[i].uv;
        let sid = verts.len() as u32;
        verts.push(GridVertex {
            uv,
            face: 1.0,
            use_base,
        });
        cache[i] = sid;
        sid
    };

    // Walk each border CCW (from +Y) so outward = right of travel → correct backface winding.
    let edge_chains: [[(u32, u32); 2]; 4] = [
        // -Z (z = 0): x 0 → res-1
        [(0, 0), (res - 1, 0)],
        // +X (x = res-1): z 0 → res-1
        [(res - 1, 0), (res - 1, res - 1)],
        // +Z (z = res-1): x res-1 → 0
        [(res - 1, res - 1), (0, res - 1)],
        // -X (x = 0): z res-1 → 0
        [(0, res - 1), (0, 0)],
    ];

    for chain in &edge_chains {
        let (ax, az) = chain[0];
        let (bx, bz) = chain[1];
        let steps = (ax.abs_diff(bx)).max(az.abs_diff(bz));
        for s in 0..steps {
            let t0 = s as f32 / steps as f32;
            let t1 = (s + 1) as f32 / steps as f32;
            let x0 = ax as f32 + (bx as f32 - ax as f32) * t0;
            let z0 = az as f32 + (bz as f32 - az as f32) * t0;
            let x1 = ax as f32 + (bx as f32 - ax as f32) * t1;
            let z1 = az as f32 + (bz as f32 - az as f32) * t1;
            let i0 = (z0.round() as u32) * res + x0.round() as u32;
            let i1 = (z1.round() as u32) * res + x1.round() as u32;
            let t0i = mark_wall(&mut verts, i0, &mut wall_top_of, 0.0);
            let t1i = mark_wall(&mut verts, i1, &mut wall_top_of, 0.0);
            let b0 = mark_wall(&mut verts, i0, &mut wall_bot_of, 1.0);
            let b1 = mark_wall(&mut verts, i1, &mut wall_bot_of, 1.0);
            // Outward winding: top0, top1, bot1 / top0, bot1, bot0
            indices.extend_from_slice(&[t0i, t1i, b1, t0i, b1, b0]);
        }
    }

    // Underside — flat quad at slab base.
    let u0 = verts.len() as u32;
    for uv in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
        verts.push(GridVertex {
            uv,
            face: 2.0,
            use_base: 1.0,
        });
    }
    // Bottom face winding so the underside faces downward (outward −Y).
    indices.extend_from_slice(&[u0, u0 + 1, u0 + 2, u0, u0 + 2, u0 + 3]);

    let edges = grid_edge_indices(res);
    (verts, indices, surface_index_count, edges)
}

fn grid_indices(resolution: u32) -> Vec<u32> {
    let res = resolution.max(2);
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
    indices
}

/// Unique horizontal + vertical edges as a LineList index buffer (top surface only).
fn grid_edge_indices(resolution: u32) -> Vec<u32> {
    let res = resolution.max(2);
    let mut indices = Vec::with_capacity(((res * (res - 1) * 2) * 2) as usize);
    for z in 0..res {
        for x in 0..res - 1 {
            let i0 = z * res + x;
            let i1 = i0 + 1;
            indices.extend_from_slice(&[i0, i1]);
        }
    }
    for z in 0..res - 1 {
        for x in 0..res {
            let i0 = z * res + x;
            let i1 = i0 + res;
            indices.extend_from_slice(&[i0, i1]);
        }
    }
    indices
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_grid_has_skirts_and_bottom() {
        let (verts, indices, surface, _) = build_solid_grid(17);
        assert_eq!(surface, 16 * 16 * 6);
        assert!(indices.len() as u32 > surface);
        assert!(verts.iter().any(|v| v.face > 0.5 && v.use_base < 0.5));
        assert!(verts.iter().any(|v| v.face > 0.5 && v.use_base > 0.5));
        assert!(verts.iter().any(|v| v.face > 1.5));
        assert_eq!(indices.len() % 6, 0);
        // Walls must not reuse surface verts (face stays 0 on the top grid).
        assert!(verts.iter().take(17 * 17).all(|v| v.face < 0.5));
    }

    #[test]
    fn edge_indices_are_line_pairs() {
        let edges = grid_edge_indices(17);
        assert!(!edges.is_empty());
        assert_eq!(edges.len() % 2, 0);
    }
}
