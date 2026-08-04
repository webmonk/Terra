//! wgpu terrain viewport — GPU height textures + clipmap LOD displacement.

pub mod brush;
pub mod camera;
pub mod clipmap;
pub mod grid;
pub mod height_gpu;
pub mod terrain_mesh;

pub use brush::{pick_terrain_uv, pick_terrain_uv_on_surface, BrushGizmo, BrushOverlay};
pub use camera::OrbitCamera;
pub use clipmap::ClipmapConfig;
pub use grid::TerrainGrid;
pub use height_gpu::HeightGpu;

use bytemuck::{Pod, Zeroable};
use glam::Vec2;
use terra_core::heightfield::Heightfield;
use terra_core::mask::MaskField;
use terra_core::tiling::SampleRect;
use thiserror::Error;
use winit::window::Window;

#[derive(Debug, Error)]
pub enum RenderError {
    #[error("{0}")]
    Msg(String),
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameUniforms {
    view_proj: [[f32; 4]; 4],
    /// xyz = light direction (from light toward scene), w = sun intensity
    light_dir: [f32; 4],
    world: [f32; 4],
    grid: [f32; 4],
    /// origin_x, origin_z, spacing, grid_size
    clipmap: [f32; 4],
    /// xyz = camera eye, w = exposure
    eye: [f32; 4],
}

/// wgpu/Vulkan drivers often expect uniform bindings sized to 256-byte alignment.
const FRAME_UNIFORM_BUF_SIZE: u64 = 256;

pub struct TerrainRenderer {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub pipeline: wgpu::RenderPipeline,
    /// One frame-uniform buffer per clipmap level (avoids mid-pass buffer writes).
    pub uniform_bufs: Vec<wgpu::Buffer>,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_groups: Vec<wgpu::BindGroup>,
    pub depth: wgpu::TextureView,
    /// Unit UV grid instanced per clipmap level.
    pub grid: TerrainGrid,
    pub clipmap: ClipmapConfig,
    pub heights: HeightGpu,
    pub camera: OrbitCamera,
    pub size: winit::dpi::PhysicalSize<u32>,
    /// Last CPU→GPU height upload microseconds.
    pub last_upload_us: u64,
    /// Clipmap levels drawn last frame (profiler).
    pub last_clipmap_levels: u32,
    /// After first height present, leave orbit target alone so uploads don't fight the user.
    camera_framed: bool,
    /// Sculpt / mask brush ring drawn on the height surface.
    pub brush: BrushOverlay,
    /// Presentation lighting (does not affect height data).
    pub lighting: EnvironmentLighting,
}

/// Environment lighting used for Lit viewport presentation.
#[derive(Debug, Clone, Copy)]
pub struct EnvironmentLighting {
    pub light_dir: [f32; 4],
    pub exposure: f32,
    pub clear: [f32; 3],
}

impl Default for EnvironmentLighting {
    fn default() -> Self {
        Self {
            light_dir: [-0.35, -0.90, -0.20, 1.00],
            exposure: 1.00,
            clear: [0.28, 0.32, 0.38],
        }
    }
}

impl TerrainRenderer {
    pub async fn new(window: std::sync::Arc<Window>) -> Result<Self, RenderError> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| RenderError::Msg(e.to_string()))?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| RenderError::Msg("no adapter".into()))?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("terra-render"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: Default::default(),
                },
                None,
            )
            .await
            .map_err(|e| RenderError::Msg(e.to_string()))?;

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .or_else(|| caps.formats.iter().copied().find(|f| !f.is_srgb()))
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("terrain"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/terrain.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    // Vertex displace + fragment height-AO both sample the height field.
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let heights = HeightGpu::new(&device, 256);
        debug_assert!(std::mem::size_of::<FrameUniforms>() as u64 <= FRAME_UNIFORM_BUF_SIZE);
        let max_levels = 8usize;
        let uniform_bufs: Vec<wgpu::Buffer> = (0..max_levels)
            .map(|i| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(&format!("frame-u-{i}")),
                    size: FRAME_UNIFORM_BUF_SIZE,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })
            .collect();
        let bind_groups: Vec<wgpu::BindGroup> = uniform_bufs
            .iter()
            .map(|buf| Self::make_bind_group(&device, &bind_group_layout, buf, &heights))
            .collect();

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("terrain-pl"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("terrain-pipe"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[TerrainGrid::vertex_layout()],
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
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let depth = create_depth(&device, config.width, config.height);
        let clipmap = ClipmapConfig::for_world(4096.0, 4, 96);
        // One unit grid; each clipmap level remaps UV → world via uniforms.
        let grid = TerrainGrid::new(&device, clipmap.grid_size);
        let camera = OrbitCamera::default();
        let brush = BrushOverlay::new(&device, format);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            uniform_bufs,
            bind_group_layout,
            bind_groups,
            depth,
            grid,
            clipmap,
            heights,
            camera,
            size,
            last_upload_us: 0,
            last_clipmap_levels: 0,
            camera_framed: false,
            brush,
            lighting: EnvironmentLighting::default(),
        })
    }

    fn make_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform_buf: &wgpu::Buffer,
        heights: &HeightGpu,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain-bg"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(heights.display_height_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(heights.display_normal_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(heights.sampler()),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(heights.materials_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(heights.wetness_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(heights.vegetation_view()),
                },
            ],
        })
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }
        self.size = new_size;
        self.config.width = new_size.width;
        self.config.height = new_size.height;
        self.surface.configure(&self.device, &self.config);
        self.depth = create_depth(&self.device, self.config.width, self.config.height);
    }

    /// Upload heightfield to GPU textures (no mesh rebuild). Swaps display buffer when done.
    pub fn upload_heightfield(&mut self, hf: &Heightfield) {
        self.upload_heightfield_regions(hf, None);
    }

    /// Upload only dirty sample regions (Wave D); `None` = full field.
    pub fn upload_heightfield_regions(&mut self, hf: &Heightfield, regions: Option<&[SampleRect]>) {
        profiling::scope!("upload_heightfield");
        let t0 = std::time::Instant::now();
        self.heights
            .upload_regions_and_swap(&self.device, &self.queue, hf, regions);
        self.finish_height_present(t0);
    }

    /// Upload material-ID and wetness fields produced by the surface layers.
    /// Rebuilds terrain bind groups because the sampled views may have resized.
    pub fn upload_aux_maps(
        &mut self,
        materials: Option<&MaskField>,
        wetness: Option<&MaskField>,
        vegetation: Option<&MaskField>,
    ) {
        self.heights
            .upload_aux_maps(&self.device, &self.queue, materials, wetness, vegetation);
        self.recreate_bind_groups();
    }

    /// Present a GPU-resident height texture (Wave C — no CPU readback).
    pub fn present_gpu_height(
        &mut self,
        src: &wgpu::Texture,
        width: u32,
        height: u32,
        world_size: (f32, f32),
        height_range: (f32, f32),
        dx: f32,
        dz: f32,
    ) {
        self.present_gpu_height_region(src, width, height, world_size, height_range, dx, dz, None);
    }

    pub fn present_gpu_height_region(
        &mut self,
        src: &wgpu::Texture,
        width: u32,
        height: u32,
        world_size: (f32, f32),
        height_range: (f32, f32),
        dx: f32,
        dz: f32,
        region: Option<SampleRect>,
    ) {
        profiling::scope!("present_gpu_height");
        let t0 = std::time::Instant::now();
        self.heights.copy_from_texture_region_and_swap(
            &self.device,
            &self.queue,
            src,
            width,
            height,
            world_size,
            height_range,
            dx,
            dz,
            region,
        );
        self.finish_height_present(t0);
    }

    fn finish_height_present(&mut self, t0: std::time::Instant) {
        self.recreate_bind_groups();
        let extent = self.heights.world_size.0.max(self.heights.world_size.1);
        self.clipmap =
            ClipmapConfig::for_world(extent, self.clipmap.levels, self.clipmap.grid_size);
        if self.grid.resolution != self.clipmap.grid_size {
            self.grid = TerrainGrid::new(&self.device, self.clipmap.grid_size);
        }
        // Frame once (or after explicit reset). Continuous retargeting fights orbit/pan.
        if !self.camera_framed {
            self.frame_camera_to_terrain();
        } else if self.camera.distance < 10.0 || self.camera.distance > extent * 4.0 {
            self.camera.distance = extent * 1.1;
        }
        self.last_upload_us = t0.elapsed().as_micros() as u64;
    }

    /// Center orbit on the current heightfield extents (camera-reset / first present).
    pub fn frame_camera_to_terrain(&mut self) {
        let (min_h, max_h) = self.heights.height_range;
        let extent = self.heights.world_size.0.max(self.heights.world_size.1);
        self.camera.target = glam::Vec3::new(
            self.heights.world_size.0 * 0.5,
            (min_h + max_h) * 0.5,
            self.heights.world_size.1 * 0.5,
        );
        self.camera.distance = extent * 1.1;
        self.camera_framed = true;
    }

    pub fn request_camera_reframe(&mut self) {
        self.camera_framed = false;
    }

    /// Move the orbit target to a normalized location in the terrain footprint.
    pub fn focus_camera_uv(&mut self, u: f32, v: f32) {
        self.camera.target.x = u.clamp(0.0, 1.0) * self.heights.world_size.0;
        self.camera.target.z = v.clamp(0.0, 1.0) * self.heights.world_size.1;
        self.camera.clamp_to_world(self.heights.world_size);
    }

    /// Switch to a near-vertical overview while retaining the current target.
    pub fn camera_top_view(&mut self) {
        self.camera.pitch = 1.45;
        let extent = self.heights.world_size.0.max(self.heights.world_size.1);
        self.camera.distance = self.camera.distance.max(extent * 0.9);
    }

    /// The terrain document has no spatial selection bounds yet, so frame its
    /// full footprint until selected layers expose one.
    pub fn frame_camera_to_selection(&mut self) {
        self.frame_camera_to_terrain();
    }

    fn recreate_bind_groups(&mut self) {
        self.bind_groups = self
            .uniform_bufs
            .iter()
            .map(|buf| {
                Self::make_bind_group(&self.device, &self.bind_group_layout, buf, &self.heights)
            })
            .collect();
    }

    pub fn set_brush_gizmo(&mut self, gizmo: Option<BrushGizmo>) {
        self.brush.set_gizmo(gizmo);
    }

    /// Update brush ring mesh to match current gizmo + optional ring height samples.
    pub fn sync_brush_geometry(&mut self, ring_heights: Option<&[f32]>) {
        self.brush
            .sync_geometry(&self.queue, self.heights.world_size, ring_heights);
    }

    pub fn render_terrain(&mut self) -> Result<wgpu::SurfaceTexture, RenderError> {
        let frame = self
            .surface
            .get_current_texture()
            .map_err(|e| RenderError::Msg(e.to_string()))?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        let view_proj = self.camera.view_proj(aspect);
        let (min_h, max_h) = self.heights.height_range;
        let (tw, th) = self.heights.tex_size;
        let world_xz = Vec2::new(self.heights.world_size.0, self.heights.world_size.1);
        self.camera.clamp_to_world(self.heights.world_size);
        let center = Vec2::new(self.camera.target.x, self.camera.target.z);
        let levels = self.clipmap.levels_for_center(center, world_xz);
        self.last_clipmap_levels = levels.len() as u32;

        let eye = self.camera.eye();
        // Upload per-level uniforms before the pass.
        for (i, level) in levels.iter().rev().enumerate() {
            let uniforms = FrameUniforms {
                view_proj: view_proj.to_cols_array_2d(),
                // Direction from light toward the scene; w = sun intensity.
                light_dir: self.lighting.light_dir,
                world: [
                    self.heights.world_size.0,
                    self.heights.world_size.1,
                    min_h,
                    max_h.max(min_h + 1e-3),
                ],
                grid: [tw as f32, th as f32, 0.0, 0.0],
                clipmap: [
                    level.origin.x,
                    level.origin.y,
                    level.spacing,
                    level.grid_size as f32,
                ],
                eye: [eye.x, eye.y, eye.z, self.lighting.exposure],
            };
            if let Some(buf) = self.uniform_bufs.get(i) {
                self.queue
                    .write_buffer(buf, 0, bytemuck::bytes_of(&uniforms));
            }
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("terrain-enc"),
            });
        self.brush.upload_view_proj(&self.queue, view_proj);
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terrain-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: self.lighting.clear[0] as f64,
                            g: self.lighting.clear[1] as f64,
                            b: self.lighting.clear[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_vertex_buffer(0, self.grid.vertex_buf.slice(..));
            pass.set_index_buffer(self.grid.index_buf.slice(..), wgpu::IndexFormat::Uint32);

            // Coarse → fine so denser LODs win the depth test near the camera.
            for i in 0..levels.len() {
                if let Some(bg) = self.bind_groups.get(i) {
                    pass.set_bind_group(0, bg, &[]);
                    pass.draw_indexed(0..self.grid.index_count, 0, 0..1);
                }
            }

            self.brush.draw(&mut pass);
        }
        self.queue.submit(Some(encoder.finish()));
        Ok(frame)
    }
}

fn create_depth(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}
