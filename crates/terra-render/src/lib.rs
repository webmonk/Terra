//! wgpu terrain viewport — GPU height textures + world-fixed grid displacement.

pub mod brush;
pub mod camera;
pub mod clipmap;
pub mod grid;
pub mod guides;
pub mod height_gpu;
pub mod overhang;
pub mod progressive;
pub mod terrain_mesh;
pub mod vegetation;

pub use brush::{pick_terrain_uv, pick_terrain_uv_on_surface, BrushGizmo, BrushOverlay};
pub use camera::OrbitCamera;
pub use clipmap::{
    plan_resident_tiles, ClipmapConfig, ClipmapRingLevel, ResidentTileSelection,
    ViewportTilePlan, WorldGridConfig,
};
pub use grid::TerrainGrid;
pub use guides::{GuideOverlay, GuideState};
pub use height_gpu::HeightGpu;
pub use overhang::OverhangOverlay;
pub use vegetation::VegetationOverlay;

use bytemuck::{Pod, Zeroable};
use terra_core::heightfield::Heightfield;
use terra_core::layer::MaterialsParams;
use terra_core::mask::MaskField;
use terra_core::tiling::SampleRect;
use terra_core::{FieldId, NormalizedRect, TerrainPyramid};
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
    /// x=tex_w, y=tex_h, z=ocean_level, w=slab base height
    grid: [f32; 4],
    /// World-fixed mesh: origin_x, origin_z, spacing_x (unused when UV*world), grid_size
    clipmap: [f32; 4],
    /// xyz = camera eye, w = exposure
    eye: [f32; 4],
    /// x = stochastic frame seed, y = progressive enabled, z = accumulated samples, w = biome tint
    render: [f32; 4],
    /// x = shading_mode, y = contours on, z = contour interval m
    viz: [f32; 4],
}

/// Viewport false-color / analysis shading (mode bar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum ViewportShadingMode {
    #[default]
    Lit = 0,
    Height = 1,
    Slope = 2,
    Flow = 3,
}

/// Display-aid flags pushed from the editor chrome each frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct ViewportDisplayAids {
    pub wireframe: bool,
    pub grid: bool,
    pub world_bounds: bool,
    pub contours: bool,
    pub shading: ViewportShadingMode,
}

const MATERIAL_SLOT_COUNT: usize = 17;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MaterialGpu {
    albedo_roughness: [f32; 4],
    metalness_valid: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MaterialPalette {
    entries: [MaterialGpu; MATERIAL_SLOT_COUNT],
}

impl MaterialPalette {
    fn from_params(
        params: Option<&terra_core::layer::MaterialsParams>,
        albedo_layers: &[i32; MATERIAL_SLOT_COUNT],
    ) -> Self {
        let colors = [
            [0.42, 0.40, 0.36],
            [0.48, 0.36, 0.24],
            [0.30, 0.42, 0.22],
            [0.62, 0.54, 0.36],
            [0.78, 0.80, 0.82],
        ];
        let mut entries = [MaterialGpu {
            albedo_roughness: [0.45, 0.42, 0.38, 0.85],
            metalness_valid: [0.0, 0.0, 0.0, 0.0],
        }; MATERIAL_SLOT_COUNT];
        for (id, color) in colors.into_iter().enumerate() {
            entries[id] = MaterialGpu {
                albedo_roughness: [color[0], color[1], color[2], 0.82],
                metalness_valid: [0.0, 1.0, (albedo_layers[id] + 1) as f32, 0.0],
            };
        }
        if let Some(params) = params {
            for rule in &params.rules {
                let id = (rule.id as usize).min(MATERIAL_SLOT_COUNT - 1);
                entries[id] = MaterialGpu {
                    albedo_roughness: [
                        rule.tint[0].max(0.0),
                        rule.tint[1].max(0.0),
                        rule.tint[2].max(0.0),
                        rule.roughness.clamp(0.04, 1.0),
                    ],
                    metalness_valid: [
                        rule.metalness.clamp(0.0, 1.0),
                        1.0,
                        (albedo_layers[id] + 1) as f32,
                        0.0,
                    ],
                };
            }
        }
        Self { entries }
    }
}

/// wgpu/Vulkan drivers often expect uniform bindings sized to 256-byte alignment.
const FRAME_UNIFORM_BUF_SIZE: u64 = 256;

pub struct TerrainRenderer {
    pub surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub config: wgpu::SurfaceConfiguration,
    pub pipeline: wgpu::RenderPipeline,
    /// Grid edge LineList overlay for the Wireframe display aid.
    pub wireframe_pipeline: wgpu::RenderPipeline,
    /// Single frame-uniform buffer for the world-fixed terrain grid.
    pub uniform_buf: wgpu::Buffer,
    /// Transparent sea-level surface rendered after opaque terrain.
    pub ocean_pipeline: wgpu::RenderPipeline,
    pub bind_group_layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
    /// Authored tint/roughness/metalness indexed by the material-ID map.
    material_palette_buf: wgpu::Buffer,
    /// RGBA8 albedo texture array — one layer per material slot (neutral grey when unused).
    albedo_array: wgpu::Texture,
    albedo_array_view: wgpu::TextureView,
    albedo_sampler: wgpu::Sampler,
    /// Per material-ID albedo array layer (-1 = tint only).
    albedo_layers: [i32; MATERIAL_SLOT_COUNT],
    pub depth: wgpu::TextureView,
    /// Unit UV grid spanning the full world (coarse fallback).
    pub grid: TerrainGrid,
    /// Camera-centered nested LOD rings (fine → coarse draw order).
    pub ring_grids: Vec<TerrainGrid>,
    pub clipmap: ClipmapConfig,
    /// Backward-compatible alias for [`Self::clipmap`].fallback.
    pub world_grid: WorldGridConfig,
    pub heights: HeightGpu,
    pub camera: OrbitCamera,
    pub size: winit::dpi::PhysicalSize<u32>,
    /// Last CPU→GPU height upload microseconds.
    pub last_upload_us: u64,
    /// Terrain mesh resolution drawn last frame (profiler).
    pub last_grid_resolution: u32,
    /// Desired visible pages resident at the requested pyramid level.
    pub last_tile_plan_exact: usize,
    /// Desired visible pages currently covered by a coarser resident ancestor.
    pub last_tile_plan_fallback: usize,
    /// Desired visible pages with no resident ancestor yet.
    pub last_tile_plan_missing: usize,
    /// After first height present, leave orbit target alone so uploads don't fight the user.
    camera_framed: bool,
    /// Sculpt / mask brush ring drawn on the height surface.
    pub brush: BrushOverlay,
    /// World grid + AABB bounds line guides.
    pub guides: GuideOverlay,
    /// Phase J dual-height overhang / cave roof proxy (opt-in layers).
    pub overhang: OverhangOverlay,
    /// Instanced vegetation driven by the evaluated vegetation-density field.
    pub vegetation: VegetationOverlay,
    /// Presentation lighting (does not affect height data).
    pub lighting: EnvironmentLighting,
    /// Active ocean height; None disables the water surface.
    ocean_level: Option<f32>,
    /// Strength of painted biome placement colour overlay (0 = off).
    biome_tint_strength: f32,
    /// Display aids / analysis shading from the viewport chrome.
    display_aids: ViewportDisplayAids,
    /// Progressive stochastic lighting, temporal reprojection, and denoising.
    progressive: progressive::ProgressiveRenderer,
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
            // Windows: DX12 avoids OBS/Overwolf/Medal Vulkan implicit layers that
            // STATUS_STACK_OVERFLOW in vkCreateDevice (esp. debug + large shaders).
            backends: preferred_backends(),
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

        log::info!("terra-render: compiling terrain shader/pipelines…");
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
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Climate snow / temperature / rainfall (R32Float, non-filterable).
                wgpu::BindGroupLayoutEntry {
                    binding: 9,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 10,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 11,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Material albedo texture array (RGBA8) + filtering sampler.
                wgpu::BindGroupLayoutEntry {
                    binding: 12,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 13,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                // Painted biome placement colour overlay (RGBA8).
                wgpu::BindGroupLayoutEntry {
                    binding: 14,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });

        let heights = HeightGpu::new(&device, 256);
        debug_assert!(std::mem::size_of::<FrameUniforms>() as u64 <= FRAME_UNIFORM_BUF_SIZE);
        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("frame-u"),
            size: FRAME_UNIFORM_BUF_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let material_palette_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("material-palette-u"),
            size: std::mem::size_of::<MaterialPalette>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let albedo_layers = [-1i32; MATERIAL_SLOT_COUNT];
        queue.write_buffer(
            &material_palette_buf,
            0,
            bytemuck::bytes_of(&MaterialPalette::from_params(None, &albedo_layers)),
        );
        let (albedo_array, albedo_array_view, albedo_sampler) =
            create_albedo_array(&device, &queue);
        let bind_group = Self::make_bind_group(
            &device,
            &bind_group_layout,
            &uniform_buf,
            &material_palette_buf,
            &heights,
            &albedo_array_view,
            &albedo_sampler,
        );

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

        let ocean_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("ocean-pipe"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_ocean"),
                buffers: &[TerrainGrid::vertex_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_ocean"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
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

        let wireframe_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("wireframe-pipe"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[TerrainGrid::vertex_layout()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_wireframe"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: wgpu::DepthBiasState {
                    constant: -4,
                    slope_scale: -2.0,
                    clamp: 0.0,
                },
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        let depth = create_depth(&device, config.width, config.height);
        let clipmap = ClipmapConfig::for_world(4096.0, 385);
        let world_grid = clipmap.fallback.clone();
        let grid = TerrainGrid::new(&device, world_grid.grid_size);
        let ring_grids: Vec<TerrainGrid> = clipmap
            .rings
            .iter()
            .map(|ring| TerrainGrid::new(&device, ring.grid_size))
            .collect();
        let camera = OrbitCamera::default();
        let brush = BrushOverlay::new(&device, format);
        let guides = GuideOverlay::new(&device, format);
        let overhang = OverhangOverlay::new(&device, format);
        let vegetation = VegetationOverlay::new(&device, format);
        let progressive =
            progressive::ProgressiveRenderer::new(&device, config.width, config.height, format);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            wireframe_pipeline,
            uniform_buf,
            bind_group_layout,
            bind_group,
            material_palette_buf,
            albedo_array,
            albedo_array_view,
            albedo_sampler,
            albedo_layers,
            depth,
            grid,
            ring_grids,
            clipmap,
            world_grid,
            heights,
            camera,
            size,
            ocean_pipeline,
            last_upload_us: 0,
            last_grid_resolution: 0,
            last_tile_plan_exact: 0,
            last_tile_plan_fallback: 0,
            last_tile_plan_missing: 0,
            camera_framed: false,
            brush,
            guides,
            overhang,
            vegetation,
            lighting: EnvironmentLighting::default(),
            ocean_level: None,
            biome_tint_strength: 0.0,
            display_aids: ViewportDisplayAids::default(),
            progressive,
        })
    }

    // (pipeline compile complete — logged via terrain shader message above)

    fn make_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform_buf: &wgpu::Buffer,
        material_palette_buf: &wgpu::Buffer,
        heights: &HeightGpu,
        albedo_array_view: &wgpu::TextureView,
        albedo_sampler: &wgpu::Sampler,
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
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(heights.flow_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: material_palette_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 9,
                    resource: wgpu::BindingResource::TextureView(heights.snow_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 10,
                    resource: wgpu::BindingResource::TextureView(heights.temperature_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 11,
                    resource: wgpu::BindingResource::TextureView(heights.rainfall_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 12,
                    resource: wgpu::BindingResource::TextureView(albedo_array_view),
                },
                wgpu::BindGroupEntry {
                    binding: 13,
                    resource: wgpu::BindingResource::Sampler(albedo_sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 14,
                    resource: wgpu::BindingResource::TextureView(heights.placement_tint_view()),
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
        self.progressive
            .resize(&self.device, self.config.width, self.config.height);
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

    /// Upload authored tint, roughness, metalness, and optional albedo PNGs.
    pub fn upload_material_palette(&mut self, params: Option<&MaterialsParams>) {
        self.albedo_layers = [-1i32; MATERIAL_SLOT_COUNT];
        if let Some(params) = params {
            for rule in &params.rules {
                let id = (rule.id as usize).min(MATERIAL_SLOT_COUNT - 1);
                if let Some(path) = rule.albedo_path.as_ref().filter(|p| !p.is_empty()) {
                    match load_albedo_png(path) {
                        Ok(rgba) => {
                            write_albedo_layer(&self.queue, &self.albedo_array, id as u32, &rgba);
                            self.albedo_layers[id] = id as i32;
                        }
                        Err(err) => {
                            log::warn!("albedo_path load failed ({path}): {err}");
                        }
                    }
                }
            }
        }
        let palette = MaterialPalette::from_params(params, &self.albedo_layers);
        self.queue
            .write_buffer(&self.material_palette_buf, 0, bytemuck::bytes_of(&palette));
        self.progressive.invalidate();
    }

    /// Upload material-ID and wetness fields produced by the surface layers.
    /// Rebuilds terrain bind groups because the sampled views may have resized.
    pub fn upload_aux_maps(
        &mut self,
        materials: Option<&MaskField>,
        wetness: Option<&MaskField>,
        vegetation: Option<&MaskField>,
    ) {
        self.upload_aux_maps_ex(
            materials, wetness, vegetation, None, None, None, None, None, None,
        );
    }

    /// Upload materials/wetness/vegetation plus optional climate R32Float aux maps and flow.
    pub fn upload_aux_maps_ex(
        &mut self,
        materials: Option<&MaskField>,
        wetness: Option<&MaskField>,
        vegetation: Option<&MaskField>,
        temperature: Option<&MaskField>,
        rainfall: Option<&MaskField>,
        snow: Option<&MaskField>,
        soil_moisture: Option<&MaskField>,
        biomes: Option<&MaskField>,
        flow: Option<&MaskField>,
    ) {
        self.heights.upload_aux_maps_ex(
            &self.device,
            &self.queue,
            materials,
            wetness,
            vegetation,
            temperature,
            rainfall,
            snow,
            soil_moisture,
            biomes,
            flow,
        );
        self.recreate_bind_group();
        self.progressive.invalidate();
    }

    /// Upload artist biome placement colour overlay (RGBA8). Rebuilds bind groups.
    pub fn upload_placement_tint(&mut self, width: u32, height: u32, rgba: &[u8]) {
        self.heights
            .upload_placement_tint(&self.device, &self.queue, width, height, rgba);
        self.recreate_bind_group();
        self.progressive.invalidate();
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

    /// Bind a GPU engine height texture directly when formats match (full field).
    /// Partial [`SampleRect`] updates still copy through the double-buffer path.
    pub fn present_gpu_height_shared(
        &mut self,
        src: &wgpu::Texture,
        src_view: &wgpu::TextureView,
        width: u32,
        height: u32,
        world_size: (f32, f32),
        height_range: (f32, f32),
        dx: f32,
        dz: f32,
        region: Option<SampleRect>,
    ) {
        if region.is_some() {
            self.present_gpu_height_region(
                src, width, height, world_size, height_range, dx, dz, region,
            );
            return;
        }
        profiling::scope!("present_gpu_height_shared");
        let t0 = std::time::Instant::now();
        self.heights
            .present_shared_height(&self.device, &self.queue, src_view, width, height, world_size, height_range, dx, dz);
        self.finish_height_present(t0);
    }

    fn finish_height_present(&mut self, t0: std::time::Instant) {
        self.recreate_bind_group();
        let extent = self.heights.world_size.0.max(self.heights.world_size.1);
        // Only rebuild clipmap meshes when world extent / ring sizes actually change.
        // Recreating every present was freezing the UI on each filter/layer edit.
        let next = ClipmapConfig::for_world(extent, self.clipmap.fallback.grid_size);
        let rings_changed = self.clipmap.rings.len() != next.rings.len()
            || self
                .clipmap
                .rings
                .iter()
                .zip(next.rings.iter())
                .any(|(a, b)| a.grid_size != b.grid_size)
            || self.clipmap.fallback.grid_size != next.fallback.grid_size;
        if rings_changed {
            self.clipmap = next;
            self.world_grid = self.clipmap.fallback.clone();
            if self.grid.resolution != self.world_grid.grid_size {
                self.grid = TerrainGrid::new(&self.device, self.world_grid.grid_size);
            }
            self.ring_grids = self
                .clipmap
                .rings
                .iter()
                .map(|ring| TerrainGrid::new(&self.device, ring.grid_size))
                .collect();
        } else {
            // Keep ring spacings in sync with extent without reallocating meshes.
            self.clipmap = next;
            self.world_grid = self.clipmap.fallback.clone();
        }
        // Frame once (or after explicit reset). Continuous retargeting fights orbit/pan.
        if !self.camera_framed {
            self.frame_camera_to_terrain();
        } else if self.camera.distance < 10.0 || self.camera.distance > extent * 4.0 {
            self.camera.distance = extent * 1.1;
        }
        self.last_upload_us = t0.elapsed().as_micros() as u64;
        self.progressive.invalidate();
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

    fn recreate_bind_group(&mut self) {
        self.bind_group = Self::make_bind_group(
            &self.device,
            &self.bind_group_layout,
            &self.uniform_buf,
            &self.material_palette_buf,
            &self.heights,
            &self.albedo_array_view,
            &self.albedo_sampler,
        );
    }

    pub fn set_brush_gizmo(&mut self, gizmo: Option<BrushGizmo>) {
        self.brush.set_gizmo(gizmo);
    }

    /// Update brush ring mesh to match current gizmo + optional ring height samples.
    pub fn sync_brush_geometry(&mut self, ring_heights: Option<&[f32]>) {
        self.brush
            .sync_geometry(&self.queue, self.heights.world_size, ring_heights);
    }

    /// Upload or clear the Phase J overhang / cave roof proxy mesh.
    pub fn sync_overhang_mesh(&mut self, mesh: Option<&terra_core::volumetric::OverhangMesh>) {
        match mesh {
            Some(m) if !m.is_empty() => self.overhang.upload_mesh(&self.device, m),
            _ => self.overhang.clear(),
        }
        self.progressive.invalidate();
    }

    /// Rebuild sparse viewport instances from the evaluated vegetation field.
    pub fn sync_vegetation_instances(
        &mut self,
        height: &Heightfield,
        density: Option<&MaskField>,
        scale_min: f32,
        scale_max: f32,
        yaw_variation_deg: f32,
    ) {
        self.vegetation.sync(
            &self.device,
            height,
            density,
            scale_min,
            scale_max,
            yaw_variation_deg,
        );
        self.progressive.invalidate();
    }

    pub fn set_ocean_level(&mut self, level: Option<f32>) {
        let level = level.filter(|value| value.is_finite());
        if self.ocean_level != level {
            self.ocean_level = level;
            self.progressive.invalidate();
        }
    }

    /// Clear viewport GPU state that belongs to the previous document.
    pub fn reset_project_state(&mut self, world_size: (f32, f32), ocean_level: Option<f32>) {
        self.heights
            .reset_project_state(&self.device, &self.queue, world_size);
        // Empty vegetation overlay (no density → clear instances).
        let blank = terra_core::heightfield::Heightfield::zeros(
            terra_core::heightfield::HeightfieldMetrics {
                width: 8,
                height: 8,
                world_size_x: world_size.0.max(1.0),
                world_size_z: world_size.1.max(1.0),
                tile_size: 8,
                halo: 0,
            },
        );
        self.vegetation.sync(&self.device, &blank, None, 1.0, 1.0, 0.0);
        self.overhang.clear();
        self.ocean_level = ocean_level.filter(|v| v.is_finite());
        self.progressive.invalidate();
        self.request_camera_reframe();
        let extent = world_size.0.max(world_size.1);
        let next = ClipmapConfig::for_world(extent, self.clipmap.fallback.grid_size);
        self.clipmap = next;
        self.world_grid = self.clipmap.fallback.clone();
        if self.grid.resolution != self.world_grid.grid_size {
            self.grid = TerrainGrid::new(&self.device, self.world_grid.grid_size);
        }
        self.recreate_bind_group();
    }

    /// Push viewport chrome display aids (wireframe / grid / bounds / contours / shading).
    pub fn set_display_aids(&mut self, aids: ViewportDisplayAids) {
        if self.display_aids.wireframe != aids.wireframe
            || self.display_aids.grid != aids.grid
            || self.display_aids.world_bounds != aids.world_bounds
            || self.display_aids.contours != aids.contours
            || self.display_aids.shading != aids.shading
        {
            self.progressive.invalidate();
        }
        self.display_aids = aids;
        self.guides.set_state(GuideState {
            grid: aids.grid,
            bounds: aids.world_bounds,
        });
    }

    /// 0 = hide placement tint; ~0.55–0.7 is a readable artist overlay.
    pub fn set_biome_tint_strength(&mut self, strength: f32) {
        let strength = strength.clamp(0.0, 1.0);
        if (self.biome_tint_strength - strength).abs() > 1e-4 {
            self.biome_tint_strength = strength;
            self.progressive.invalidate();
        }
    }

    /// Enable World Creator-style progressive presentation. Fast Lit remains available.
    pub fn set_progressive_enabled(&mut self, enabled: bool) {
        self.progressive.set_enabled(enabled);
    }

    pub fn progressive_samples(&self) -> u32 {
        self.progressive.samples()
    }

    /// Reconcile the full-world footprint with the streamed terrain pyramid.
    /// The whole-field height texture remains the presentation fallback until shader page-table
    /// sampling is enabled; these counts describe streamed coverage over the world.
    pub fn update_visible_tile_plan(&mut self, pyramid: &TerrainPyramid) {
        let visible = NormalizedRect::new(0.0, 0.0, 1.0, 1.0).expect("unit world rect");
        let plan = plan_resident_tiles(
            pyramid,
            None,
            &FieldId::Height,
            pyramid.max_level(),
            visible,
        );
        self.last_tile_plan_exact = plan.exact_tiles;
        self.last_tile_plan_fallback = plan.fallback_tiles;
        self.last_tile_plan_missing = plan.missing_tiles;
    }

    pub fn render_terrain(&mut self) -> Result<wgpu::SurfaceTexture, RenderError> {
        let frame = self
            .surface
            .get_current_texture()
            .map_err(|e| RenderError::Msg(e.to_string()))?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let progressive_enabled = self.progressive.enabled();

        let aspect = self.config.width as f32 / self.config.height.max(1) as f32;
        let view_proj = self.camera.view_proj(aspect);
        let (min_h, max_h) = self.heights.height_range;
        let (tw, th) = self.heights.tex_size;
        self.camera.clamp_to_world(self.heights.world_size);
        self.last_grid_resolution = self.world_grid.grid_size;

        let eye = self.camera.eye();
        self.progressive.prepare_signature([
            self.lighting.light_dir[0],
            self.lighting.light_dir[1],
            self.lighting.light_dir[2],
            self.lighting.light_dir[3],
            self.lighting.exposure,
            self.lighting.clear[0],
            self.lighting.clear[1],
            self.lighting.clear[2],
        ]);
        let progressive_seed = self.progressive.frame_seed();
        let contour_interval = {
            let span = (max_h - min_h).max(1.0);
            (span / 20.0).clamp(5.0, 100.0)
        };
        let slab_base = {
            let span = (max_h - min_h).max(1.0);
            let extent = self.heights.world_size.0.max(self.heights.world_size.1);
            let thickness = span.max(extent * 0.03).max(40.0);
            min_h - thickness
        };
        let base_uniforms = FrameUniforms {
            view_proj: view_proj.to_cols_array_2d(),
            light_dir: self.lighting.light_dir,
            world: [
                self.heights.world_size.0,
                self.heights.world_size.1,
                min_h,
                max_h.max(min_h + 1e-3),
            ],
            grid: [
                tw as f32,
                th as f32,
                self.ocean_level.unwrap_or(min_h - 1.0),
                slab_base,
            ],
            clipmap: [0.0; 4],
            eye: [eye.x, eye.y, eye.z, self.lighting.exposure],
            render: [
                progressive_seed as f32,
                if progressive_enabled { 1.0 } else { 0.0 },
                self.progressive.samples() as f32,
                self.biome_tint_strength,
            ],
            viz: [
                self.display_aids.shading as u32 as f32,
                if self.display_aids.contours { 1.0 } else { 0.0 },
                contour_interval,
                0.0,
            ],
        };
        let world_x = self.heights.world_size.0;
        let world_z = self.heights.world_size.1;
        let fallback_spacing = self.clipmap.fallback.spacing_for_extent(world_x.max(world_z));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("terrain-enc"),
            });
        self.brush.upload_view_proj(&self.queue, view_proj);
        self.guides.upload_view_proj(&self.queue, view_proj);
        self.guides.sync_geometry(
            &self.queue,
            self.heights.world_size,
            self.heights.height_range,
            (self.camera.target.x, self.camera.target.z),
        );
        self.overhang
            .upload_view_proj(&self.queue, view_proj, self.lighting.light_dir);
        self.vegetation
            .upload_view_proj(&self.queue, view_proj, self.lighting.light_dir);
        {
            let color_view = if progressive_enabled {
                self.progressive.scene_view()
            } else {
                &view
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("terrain-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
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
            pass.set_bind_group(0, &self.bind_group, &[]);

            // Full-world displaced grid only.
            // Nested clipmap rings need per-draw uniform buffers (or dynamic offsets):
            // writing `uniform_buf` mid-pass is applied once before submit, so every draw
            // would share the last ring's origin/spacing and only a camera clip would appear.
            let mut uniforms = base_uniforms;
            uniforms.clipmap = [
                0.0,
                0.0,
                fallback_spacing,
                self.grid.resolution as f32,
            ];
            self.queue
                .write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&uniforms));
            pass.set_vertex_buffer(0, self.grid.vertex_buf.slice(..));
            pass.set_index_buffer(self.grid.index_buf.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..self.grid.index_count, 0, 0..1);

            if self.ocean_level.is_some() {
                pass.set_pipeline(&self.ocean_pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.grid.vertex_buf.slice(..));
                pass.set_index_buffer(self.grid.index_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.grid.surface_index_count, 0, 0..1);
            }
            if self.display_aids.wireframe {
                pass.set_pipeline(&self.wireframe_pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.grid.vertex_buf.slice(..));
                pass.set_index_buffer(self.grid.edge_index_buf.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..self.grid.edge_index_count, 0, 0..1);
            }
            self.vegetation.draw(&mut pass);
            self.overhang.draw(&mut pass);
            if !progressive_enabled {
                self.brush.draw(&mut pass);
                self.guides.draw(&mut pass);
            }
        }
        if progressive_enabled {
            self.progressive.resolve_to(
                &self.device,
                &self.queue,
                &mut encoder,
                &self.depth,
                &view,
                view_proj,
            );
            // Keep editor guides crisp and out of temporal history.
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("progressive-overlay-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.brush.draw(&mut pass);
            self.guides.draw(&mut pass);
        }
        self.queue.submit(Some(encoder.finish()));
        Ok(frame)
    }
}

const ALBEDO_TEX_SIZE: u32 = 256;

fn create_albedo_array(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
) -> (wgpu::Texture, wgpu::TextureView, wgpu::Sampler) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("material-albedo-array"),
        size: wgpu::Extent3d {
            width: ALBEDO_TEX_SIZE,
            height: ALBEDO_TEX_SIZE,
            depth_or_array_layers: MATERIAL_SLOT_COUNT as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    // Neutral mid-grey fill so unbound layers don't flash black.
    let grey = vec![180u8; (ALBEDO_TEX_SIZE * ALBEDO_TEX_SIZE * 4) as usize];
    for layer in 0..MATERIAL_SLOT_COUNT as u32 {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &grey,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ALBEDO_TEX_SIZE * 4),
                rows_per_image: Some(ALBEDO_TEX_SIZE),
            },
            wgpu::Extent3d {
                width: ALBEDO_TEX_SIZE,
                height: ALBEDO_TEX_SIZE,
                depth_or_array_layers: 1,
            },
        );
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("material-albedo-array-view"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("material-albedo-samp"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    (texture, view, sampler)
}

fn load_albedo_png(path: &str) -> Result<image::RgbaImage, String> {
    let img = image::open(path).map_err(|e| e.to_string())?;
    let rgba = img.to_rgba8();
    Ok(image::imageops::resize(
        &rgba,
        ALBEDO_TEX_SIZE,
        ALBEDO_TEX_SIZE,
        image::imageops::FilterType::Triangle,
    ))
}

fn write_albedo_layer(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    layer: u32,
    rgba: &image::RgbaImage,
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: 0,
                y: 0,
                z: layer,
            },
            aspect: wgpu::TextureAspect::All,
        },
        rgba.as_raw(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(ALBEDO_TEX_SIZE * 4),
            rows_per_image: Some(ALBEDO_TEX_SIZE),
        },
        wgpu::Extent3d {
            width: ALBEDO_TEX_SIZE,
            height: ALBEDO_TEX_SIZE,
            depth_or_array_layers: 1,
        },
    );
}

/// Backend selection for instance creation.
///
/// Explicit `backends` in `InstanceDescriptor` ignores `WGPU_BACKEND`, so we parse it
/// ourselves. On Windows, default to DX12 — Vulkan + OBS/Overwolf/Medal implicit layers
/// has been observed to STATUS_STACK_OVERFLOW inside `vkCreateDevice`.
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
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    tex.create_view(&wgpu::TextureViewDescriptor::default())
}

#[cfg(test)]
mod shader_tests {
    #[test]
    fn terrain_and_ocean_shader_parses() {
        let source = include_str!("shaders/terrain.wgsl");
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|error| panic!("terrain WGSL parse failed: {error}"));
        assert!(module
            .entry_points
            .iter()
            .any(|entry| entry.name == "vs_ocean"));
        assert!(module
            .entry_points
            .iter()
            .any(|entry| entry.name == "fs_ocean"));
    }

    #[test]
    fn vegetation_shader_parses() {
        let source = include_str!("shaders/vegetation.wgsl");
        let module = naga::front::wgsl::parse_str(source)
            .unwrap_or_else(|error| panic!("vegetation WGSL parse failed: {error}"));
        assert!(module
            .entry_points
            .iter()
            .any(|entry| entry.name == "vs_main"));
        assert!(module
            .entry_points
            .iter()
            .any(|entry| entry.name == "fs_main"));
    }

    #[test]
    fn progressive_shaders_parse() {
        for (name, source) in [
            (
                "temporal",
                include_str!("shaders/progressive_temporal.wgsl"),
            ),
            ("atrous", include_str!("shaders/progressive_atrous.wgsl")),
            (
                "composite",
                include_str!("shaders/progressive_composite.wgsl"),
            ),
        ] {
            naga::front::wgsl::parse_str(source)
                .unwrap_or_else(|error| panic!("{name} WGSL parse failed: {error}"));
        }
    }
}
