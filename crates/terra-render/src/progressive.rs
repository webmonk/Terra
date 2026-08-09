//! Progressive presentation renderer: temporal reprojection, moments, and ?-trous filtering.
//!
//! The terrain pass still produces an immediate raster sample. In progressive mode that
//! sample contains stochastic heightfield visibility rays; this module converges those
//! samples over time and rejects history when depth reprojection no longer matches.

use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use wgpu::util::DeviceExt;

const FILTER_PASSES: usize = 4;
const MAX_HISTORY: f32 = 256.0;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ProgressiveUniforms {
    inv_view_proj: [[f32; 4]; 4],
    prev_view_proj: [[f32; 4]; 4],
    resolution: [f32; 4],
    params: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FilterUniforms {
    params: [f32; 4],
}

fn target(format: wgpu::TextureFormat) -> Option<wgpu::ColorTargetState> {
    Some(wgpu::ColorTargetState {
        format,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    })
}

fn sampled_texture_entry(
    binding: u32,
    sample_type: wgpu::TextureSampleType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type,
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn make_view(
    device: &wgpu::Device,
    label: &str,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: width.max(1),
                height: height.max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// Owns the off-screen sample, temporal history, and edge-aware denoising chain.
pub struct ProgressiveRenderer {
    enabled: bool,
    width: u32,
    height: u32,
    surface_format: wgpu::TextureFormat,
    scene: wgpu::TextureView,
    history_color: [wgpu::TextureView; 2],
    history_moments: [wgpu::TextureView; 2],
    history_depth: [wgpu::TextureView; 2],
    denoise: [wgpu::TextureView; 2],
    temporal_layout: wgpu::BindGroupLayout,
    atrous_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    temporal_pipeline: wgpu::RenderPipeline,
    atrous_pipeline: wgpu::RenderPipeline,
    composite_pipeline: wgpu::RenderPipeline,
    temporal_uniform: wgpu::Buffer,
    filter_uniforms: [wgpu::Buffer; FILTER_PASSES],
    history_index: usize,
    history_valid: bool,
    samples: u32,
    frame_index: u32,
    previous_view_proj: Mat4,
    last_signature: Option<[f32; 8]>,
}

impl ProgressiveRenderer {
    pub fn new(
        device: &wgpu::Device,
        width: u32,
        height: u32,
        surface_format: wgpu::TextureFormat,
    ) -> Self {
        let render_sampled =
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let scene = make_view(
            device,
            "progressive-scene",
            width,
            height,
            surface_format,
            render_sampled,
        );
        let history_color = std::array::from_fn(|i| {
            make_view(
                device,
                &format!("progressive-history-color-{i}"),
                width,
                height,
                wgpu::TextureFormat::Rgba16Float,
                render_sampled,
            )
        });
        let history_moments = std::array::from_fn(|i| {
            make_view(
                device,
                &format!("progressive-history-moments-{i}"),
                width,
                height,
                wgpu::TextureFormat::Rgba16Float,
                render_sampled,
            )
        });
        let history_depth = std::array::from_fn(|i| {
            make_view(
                device,
                &format!("progressive-history-depth-{i}"),
                width,
                height,
                wgpu::TextureFormat::R32Float,
                render_sampled,
            )
        });
        let denoise = std::array::from_fn(|i| {
            make_view(
                device,
                &format!("progressive-denoise-{i}"),
                width,
                height,
                wgpu::TextureFormat::Rgba16Float,
                render_sampled,
            )
        });

        let temporal_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("progressive-temporal-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                sampled_texture_entry(1, wgpu::TextureSampleType::Float { filterable: false }),
                sampled_texture_entry(2, wgpu::TextureSampleType::Depth),
                sampled_texture_entry(3, wgpu::TextureSampleType::Float { filterable: false }),
                sampled_texture_entry(4, wgpu::TextureSampleType::Float { filterable: false }),
                sampled_texture_entry(5, wgpu::TextureSampleType::Float { filterable: false }),
            ],
        });
        let atrous_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("progressive-atrous-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                sampled_texture_entry(1, wgpu::TextureSampleType::Float { filterable: false }),
                sampled_texture_entry(2, wgpu::TextureSampleType::Float { filterable: false }),
                sampled_texture_entry(3, wgpu::TextureSampleType::Float { filterable: false }),
            ],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("progressive-composite-bgl"),
            entries: &[sampled_texture_entry(
                0,
                wgpu::TextureSampleType::Float { filterable: false },
            )],
        });

        let temporal_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("progressive-temporal"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("shaders/progressive_temporal.wgsl").into(),
            ),
        });
        let atrous_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("progressive-atrous"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("shaders/progressive_atrous.wgsl").into(),
            ),
        });
        let composite_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("progressive-composite"),
            source: wgpu::ShaderSource::Wgsl(
                include_str!("shaders/progressive_composite.wgsl").into(),
            ),
        });

        let temporal_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("progressive-temporal-pl"),
            bind_group_layouts: &[&temporal_layout],
            push_constant_ranges: &[],
        });
        let atrous_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("progressive-atrous-pl"),
            bind_group_layouts: &[&atrous_layout],
            push_constant_ranges: &[],
        });
        let composite_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("progressive-composite-pl"),
            bind_group_layouts: &[&composite_layout],
            push_constant_ranges: &[],
        });

        let temporal_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("progressive-temporal-pipeline"),
            layout: Some(&temporal_pl),
            vertex: wgpu::VertexState {
                module: &temporal_shader,
                entry_point: Some("vs_fullscreen"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &temporal_shader,
                entry_point: Some("fs_temporal"),
                targets: &[
                    target(wgpu::TextureFormat::Rgba16Float),
                    target(wgpu::TextureFormat::Rgba16Float),
                    target(wgpu::TextureFormat::R32Float),
                ],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let atrous_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("progressive-atrous-pipeline"),
            layout: Some(&atrous_pl),
            vertex: wgpu::VertexState {
                module: &atrous_shader,
                entry_point: Some("vs_fullscreen"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &atrous_shader,
                entry_point: Some("fs_atrous"),
                targets: &[target(wgpu::TextureFormat::Rgba16Float)],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let composite_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("progressive-composite-pipeline"),
            layout: Some(&composite_pl),
            vertex: wgpu::VertexState {
                module: &composite_shader,
                entry_point: Some("vs_fullscreen"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &composite_shader,
                entry_point: Some("fs_composite"),
                targets: &[target(surface_format)],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let temporal_uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("progressive-temporal-uniform"),
            contents: bytemuck::bytes_of(&ProgressiveUniforms::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let filter_uniforms = std::array::from_fn(|i| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(&format!("progressive-filter-uniform-{i}")),
                contents: bytemuck::bytes_of(&FilterUniforms::zeroed()),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            })
        });

        Self {
            enabled: false,
            width: width.max(1),
            height: height.max(1),
            surface_format,
            scene,
            history_color,
            history_moments,
            history_depth,
            denoise,
            temporal_layout,
            atrous_layout,
            composite_layout,
            temporal_pipeline,
            atrous_pipeline,
            composite_pipeline,
            temporal_uniform,
            filter_uniforms,
            history_index: 0,
            history_valid: false,
            samples: 0,
            frame_index: 0,
            previous_view_proj: Mat4::IDENTITY,
            last_signature: None,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let enabled = self.enabled;
        *self = Self::new(device, width, height, self.surface_format);
        self.enabled = enabled;
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        if self.enabled != enabled {
            self.enabled = enabled;
            self.invalidate();
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn samples(&self) -> u32 {
        if self.enabled {
            self.samples
        } else {
            0
        }
    }

    pub fn frame_seed(&self) -> u32 {
        self.frame_index
    }

    pub fn scene_view(&self) -> &wgpu::TextureView {
        &self.scene
    }

    pub fn invalidate(&mut self) {
        self.history_valid = false;
        self.samples = 0;
        self.frame_index = 0;
        self.last_signature = None;
    }

    pub fn prepare_signature(&mut self, signature: [f32; 8]) {
        if let Some(previous) = self.last_signature {
            if previous
                .iter()
                .zip(signature.iter())
                .any(|(a, b)| (a - b).abs() > 1e-5)
            {
                self.history_valid = false;
                self.samples = 0;
                self.frame_index = 0;
            }
        }
        self.last_signature = Some(signature);
    }

    pub fn resolve_to(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        current_depth: &wgpu::TextureView,
        surface_view: &wgpu::TextureView,
        view_proj: Mat4,
    ) {
        let inverse = view_proj.inverse();
        let inverse_finite = inverse.to_cols_array().iter().all(|v| v.is_finite());
        if !inverse_finite {
            self.invalidate();
            return;
        }

        let write = 1 - self.history_index;
        let temporal = ProgressiveUniforms {
            inv_view_proj: inverse.to_cols_array_2d(),
            prev_view_proj: if self.history_valid {
                self.previous_view_proj.to_cols_array_2d()
            } else {
                view_proj.to_cols_array_2d()
            },
            resolution: [
                self.width as f32,
                self.height as f32,
                if self.history_valid { 1.0 } else { 0.0 },
                self.samples as f32,
            ],
            params: [MAX_HISTORY, 0.0025, 0.0, 0.0],
        };
        queue.write_buffer(&self.temporal_uniform, 0, bytemuck::bytes_of(&temporal));

        let temporal_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("progressive-temporal-bg"),
            layout: &self.temporal_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.temporal_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.scene),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(current_depth),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(
                        &self.history_color[self.history_index],
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(
                        &self.history_moments[self.history_index],
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(
                        &self.history_depth[self.history_index],
                    ),
                },
            ],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("progressive-temporal-pass"),
                color_attachments: &[
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.history_color[write],
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.history_moments[write],
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                    Some(wgpu::RenderPassColorAttachment {
                        view: &self.history_depth[write],
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                            store: wgpu::StoreOp::Store,
                        },
                    }),
                ],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.temporal_pipeline);
            pass.set_bind_group(0, &temporal_bg, &[]);
            pass.draw(0..3, 0..1);
        }

        for pass_index in 0..FILTER_PASSES {
            let filter = FilterUniforms {
                params: [
                    self.width as f32,
                    self.height as f32,
                    (1u32 << pass_index) as f32,
                    3.0,
                ],
            };
            queue.write_buffer(
                &self.filter_uniforms[pass_index],
                0,
                bytemuck::bytes_of(&filter),
            );
            let input = if pass_index == 0 {
                &self.history_color[write]
            } else {
                &self.denoise[(pass_index - 1) & 1]
            };
            let output = &self.denoise[pass_index & 1];
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("progressive-atrous-bg"),
                layout: &self.atrous_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: self.filter_uniforms[pass_index].as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(input),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&self.history_moments[write]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&self.history_depth[write]),
                    },
                ],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("progressive-atrous-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: output,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.atrous_pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.draw(0..3, 0..1);
        }

        let filtered = &self.denoise[(FILTER_PASSES - 1) & 1];
        let composite_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("progressive-composite-bg"),
            layout: &self.composite_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(filtered),
            }],
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("progressive-composite-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: surface_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &composite_bg, &[]);
            pass.draw(0..3, 0..1);
        }

        self.history_index = write;
        self.history_valid = true;
        self.samples = self.samples.saturating_add(1).min(MAX_HISTORY as u32);
        self.frame_index = self.frame_index.wrapping_add(1);
        self.previous_view_proj = view_proj;
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// Exercises the complete post stack on a real adapter. Ignored by default so
    /// headless CI is not required to expose a GPU; run explicitly during renderer work.
    #[test]
    #[ignore = "requires a graphics adapter"]
    fn progressive_post_stack_executes() {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
        }))
        .expect("graphics adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("progressive-test"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: Default::default(),
            },
            None,
        ))
        .expect("device");

        let mut progressive =
            ProgressiveRenderer::new(&device, 32, 24, wgpu::TextureFormat::Rgba8Unorm);
        progressive.set_enabled(true);
        let depth = make_view(
            &device,
            "progressive-test-depth",
            32,
            24,
            wgpu::TextureFormat::Depth32Float,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let output = make_view(
            &device,
            "progressive-test-output",
            32,
            24,
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("progressive-test-encoder"),
        });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("progressive-test-clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: progressive.scene_view(),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        progressive.resolve_to(
            &device,
            &queue,
            &mut encoder,
            &depth,
            &output,
            Mat4::IDENTITY,
        );
        queue.submit(Some(encoder.finish()));
        let _ = device.poll(wgpu::Maintain::Wait);
        assert_eq!(progressive.samples(), 1);
    }
}
