//! Instanced solid props (Scatter Objects) for the 3D viewport.
//!
//! Distinct from [`crate::vegetation`] on purpose. Vegetation re-derives
//! billboards from a density raster with its own hashing, which suits foliage;
//! props come from the evaluator as exact [`ObjectInstance`] placements and
//! want a solid that honours their scale, yaw and surface alignment. Sharing
//! the vegetation path would also have meant sharing its aux channel, which is
//! the bug that made props erase the forest.

use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use terra_core::layer::ObjectInstance;
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PropVertex {
    local_pos: [f32; 3],
    local_normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PropInstance {
    position_scale: [f32; 4],
    up_yaw: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ObjectUniforms {
    view_proj: [[f32; 4]; 4],
    light_dir: [f32; 4],
}

/// Upper bound on props uploaded to the viewport in one sync.
///
/// The placer's own budget is 120k instances, and drawing every one of them as
/// a solid is a lot of triangles for a preview. Past this the overlay keeps a
/// deterministic evenly-spaced subset so the distribution still reads correctly
/// rather than showing a dense corner and empty ground. The count is reported
/// so the caller can say what happened instead of silently thinning.
const MAX_DRAWN: usize = 40_000;

pub struct ObjectOverlay {
    pipeline: wgpu::RenderPipeline,
    uniform_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    instance_buf: wgpu::Buffer,
    index_count: u32,
    instance_count: u32,
    /// Instances the last sync was asked to draw, before any thinning.
    requested: usize,
}

impl ObjectOverlay {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("objects-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/objects.wgsl").into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("objects-bgl"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("objects-u"),
            size: 256,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("objects-bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("objects-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("objects-pipe"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<PropVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x3,
                                offset: 0,
                                shader_location: 0,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x3,
                                offset: 12,
                                shader_location: 1,
                            },
                        ],
                    },
                    wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<PropInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &[
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 0,
                                shader_location: 2,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 16,
                                shader_location: 3,
                            },
                            wgpu::VertexAttribute {
                                format: wgpu::VertexFormat::Float32x4,
                                offset: 32,
                                shader_location: 4,
                            },
                        ],
                    },
                ],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let (vertices, indices) = unit_box();
        let vertex_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("objects-v"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("objects-i"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let placeholder = [PropInstance {
            position_scale: [0.0; 4],
            up_yaw: [0.0, 1.0, 0.0, 0.0],
            color: [0.0; 4],
        }];
        let instance_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("objects-instances"),
            contents: bytemuck::cast_slice(&placeholder),
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            pipeline,
            uniform_buf,
            bind_group,
            vertex_buf,
            index_buf,
            instance_buf,
            index_count: indices.len() as u32,
            instance_count: 0,
            requested: 0,
        }
    }

    /// Upload placements. `base_size_m` scales the unit box to world metres.
    pub fn sync(&mut self, device: &wgpu::Device, instances: &[ObjectInstance], base_size_m: f32) {
        self.requested = instances.len();
        if instances.is_empty() {
            self.instance_count = 0;
            return;
        }
        // Even stride rather than a prefix: a truncated prefix would draw one
        // dense region and leave the rest of the world bare, which reads as a
        // placement bug rather than a draw budget.
        let stride = instances.len().div_ceil(MAX_DRAWN).max(1);
        let half = base_size_m.max(0.01) * 0.5;
        let data: Vec<PropInstance> = instances
            .iter()
            .step_by(stride)
            .map(|inst| PropInstance {
                position_scale: [inst.x, inst.y, inst.z, (inst.scale * half).max(0.01)],
                up_yaw: [inst.normal[0], inst.normal[1], inst.normal[2], inst.yaw_rad],
                color: class_color(inst.class_index),
            })
            .collect();
        self.instance_count = data.len() as u32;
        self.instance_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("objects-instances"),
            contents: bytemuck::cast_slice(&data),
            usage: wgpu::BufferUsages::VERTEX,
        });
    }

    pub fn update_uniforms(&self, queue: &wgpu::Queue, view_proj: Mat4, light_dir: [f32; 4]) {
        let u = ObjectUniforms {
            view_proj: view_proj.to_cols_array_2d(),
            light_dir,
        };
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&u));
    }

    /// Instances actually drawn, and how many were placed.
    pub fn counts(&self) -> (usize, usize) {
        (self.instance_count as usize, self.requested)
    }

    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.instance_count == 0 {
            return;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
        pass.set_vertex_buffer(1, self.instance_buf.slice(..));
        pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..self.index_count, 0, 0..self.instance_count);
    }
}

/// Unit box spanning [-0.5, 0.5] in X/Z and [0, 1] in Y, so an instance sits on
/// the surface rather than half-buried in it. Per-face normals, so the faces
/// shade distinctly and the silhouette reads at a distance.
fn unit_box() -> (Vec<PropVertex>, Vec<u16>) {
    let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
        // +Y (top)
        (
            [0.0, 1.0, 0.0],
            [
                [-0.5, 1.0, -0.5],
                [0.5, 1.0, -0.5],
                [0.5, 1.0, 0.5],
                [-0.5, 1.0, 0.5],
            ],
        ),
        // -Y (bottom)
        (
            [0.0, -1.0, 0.0],
            [
                [-0.5, 0.0, 0.5],
                [0.5, 0.0, 0.5],
                [0.5, 0.0, -0.5],
                [-0.5, 0.0, -0.5],
            ],
        ),
        // +X
        (
            [1.0, 0.0, 0.0],
            [
                [0.5, 0.0, -0.5],
                [0.5, 0.0, 0.5],
                [0.5, 1.0, 0.5],
                [0.5, 1.0, -0.5],
            ],
        ),
        // -X
        (
            [-1.0, 0.0, 0.0],
            [
                [-0.5, 0.0, 0.5],
                [-0.5, 0.0, -0.5],
                [-0.5, 1.0, -0.5],
                [-0.5, 1.0, 0.5],
            ],
        ),
        // +Z
        (
            [0.0, 0.0, 1.0],
            [
                [0.5, 0.0, 0.5],
                [-0.5, 0.0, 0.5],
                [-0.5, 1.0, 0.5],
                [0.5, 1.0, 0.5],
            ],
        ),
        // -Z
        (
            [0.0, 0.0, -1.0],
            [
                [-0.5, 0.0, -0.5],
                [0.5, 0.0, -0.5],
                [0.5, 1.0, -0.5],
                [-0.5, 1.0, -0.5],
            ],
        ),
    ];
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (normal, corners) in faces {
        let base = vertices.len() as u16;
        for corner in corners {
            vertices.push(PropVertex {
                local_pos: corner,
                local_normal: normal,
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    (vertices, indices)
}

/// Stable per-class tint so different prop families are distinguishable in the
/// viewport without any material authoring.
fn class_color(class_index: u32) -> [f32; 4] {
    const PALETTE: [[f32; 3]; 6] = [
        [0.62, 0.58, 0.52], // stone
        [0.45, 0.36, 0.28], // timber
        [0.55, 0.52, 0.46], // gravel
        [0.38, 0.42, 0.36], // mossy
        [0.66, 0.62, 0.55], // pale rock
        [0.48, 0.44, 0.40], // dark rock
    ];
    let c = PALETTE[(class_index as usize) % PALETTE.len()];
    [c[0], c[1], c[2], 1.0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::{Mat3, Vec3};

    /// Rust mirror of `basis()` in objects.wgsl. Kept here so the winding
    /// invariant below is checkable on CPU; it must track the shader.
    fn basis(up_in: Vec3, yaw: f32) -> Mat3 {
        let up = if up_in.length() > 1e-4 {
            up_in.normalize()
        } else {
            Vec3::Y
        };
        let seed = if up.x.abs() > 0.9 { Vec3::Z } else { Vec3::X };
        let t0 = seed.cross(up).normalize();
        let b0 = up.cross(t0);
        let (s, c) = yaw.sin_cos();
        Mat3::from_cols(t0 * c + b0 * s, up, b0 * c - t0 * s)
    }

    /// The unit box is wound clockwise-outward in local space, which on its own
    /// would be back-facing under wgpu's default `FrontFace::Ccw` and get culled
    /// by `cull_mode: Back`. It survives because `basis()` is deliberately
    /// left-handed (`t0 x up = -b0`, so det = -1), and that mirror flips the
    /// winding back. Two wrongs that must stay wrong together: "fixing" either
    /// one alone turns every prop inside out, and the frame would still change
    /// enough for `object_overlay.rs` to pass. So assert the composition.
    #[test]
    fn box_faces_are_front_facing_after_the_instance_basis() {
        let (vertices, indices) = unit_box();
        // Upright, tilted, and steeply tilted, each at a few yaws: the basis is
        // rebuilt per instance, so handedness must hold for all of them.
        let ups = [
            Vec3::Y,
            Vec3::new(0.3, 0.9, 0.1).normalize(),
            Vec3::new(0.95, 0.3, 0.0).normalize(),
            Vec3::new(-0.4, 0.7, 0.6).normalize(),
        ];
        for up in ups {
            for yaw in [0.0, 0.7, 2.5, 5.9] {
                let m = basis(up, yaw);
                assert!(
                    m.determinant() < 0.0,
                    "basis must stay left-handed (det {}); the box winding \
                     depends on the mirror",
                    m.determinant()
                );
                for tri in indices.chunks(3) {
                    let v: Vec<&PropVertex> =
                        tri.iter().map(|&i| &vertices[i as usize]).collect();
                    let p: Vec<Vec3> = v.iter().map(|x| m * Vec3::from(x.local_pos)).collect();
                    let geometric = (p[1] - p[0]).cross(p[2] - p[0]).normalize();
                    let declared = (m * Vec3::from(v[0].local_normal)).normalize();
                    assert!(
                        geometric.dot(declared) > 0.99,
                        "face normal {declared:?} disagrees with its winding \
                         {geometric:?} (up {up:?}, yaw {yaw}); the face would be \
                         culled and the prop would render inside out"
                    );
                }
            }
        }
    }

    /// Every corner of the box must sit on or above the instance origin in
    /// local Y, so a prop stands on the surface instead of being half-buried.
    #[test]
    fn unit_box_sits_on_the_surface() {
        let (vertices, _) = unit_box();
        assert!(vertices.iter().all(|v| v.local_pos[1] >= 0.0));
        assert!(vertices.iter().any(|v| v.local_pos[1] == 0.0));
        assert!(vertices.iter().any(|v| v.local_pos[1] == 1.0));
    }

    /// The budget thins by stride and must never exceed `MAX_DRAWN`, including
    /// at the boundaries where `div_ceil` changes step.
    #[test]
    fn thinning_stride_respects_the_budget() {
        for len in [
            1,
            MAX_DRAWN - 1,
            MAX_DRAWN,
            MAX_DRAWN + 1,
            2 * MAX_DRAWN,
            2 * MAX_DRAWN + 1,
            120_000,
        ] {
            let stride = len.div_ceil(MAX_DRAWN).max(1);
            let drawn = len.div_ceil(stride);
            assert!(drawn <= MAX_DRAWN, "len {len} drew {drawn}");
            assert!(drawn > 0);
            if len <= MAX_DRAWN {
                assert_eq!(drawn, len, "len {len} is under budget and must not thin");
            }
        }
    }
}
