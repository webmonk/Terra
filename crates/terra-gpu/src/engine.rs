//! GPU layer-stack preview engine: texture caches, ping-pong sims, no interactive readback.

use crate::{readback_f32, GpuError};
use bytemuck::{Pod, Zeroable};
use std::collections::{HashMap, HashSet};
use terra_core::eval::PreviewQuality;
use terra_core::heightfield::{Heightfield, HeightfieldMetrics, TileId};
use terra_core::layer::{
    BlendMode, FractalNoiseType, Layer, LayerId, LayerKind, LayerStack, NoiseParams, SculptParams,
};
use terra_core::mask::{bake_mask_assets, MaskAsset, MaskField, MaskSource};
use terra_core::tiling::{SampleRect, TileScheduler};
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct NoiseU {
    width: u32,
    height: u32,
    world_x: f32,
    world_z: f32,
    seed: u32,
    octaves: u32,
    frequency: f32,
    amplitude: f32,
    lacunarity: f32,
    persistence: f32,
    offset_x: f32,
    offset_z: f32,
    remap_min: f32,
    remap_max: f32,
    noise_type: u32,
    mode: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BlendU {
    width: u32,
    height: u32,
    opacity: f32,
    mode: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FillU {
    width: u32,
    height: u32,
    value: f32,
    _pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ThermalU {
    width: u32,
    height: u32,
    dx: f32,
    talus: f32,
    strength: f32,
    _p2: f32,
    _p3: f32,
    _pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct HydraulicU {
    width: u32,
    height: u32,
    timestep: f32,
    rainfall: f32,
    evaporation: f32,
    erosion: f32,
    deposition: f32,
    capacity: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BlurU {
    width: u32,
    height: u32,
    radius: u32,
    _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TerraceU {
    width: u32,
    height: u32,
    levels: u32,
    sharpness: f32,
    min_h: f32,
    max_h: f32,
    _p0: f32,
    _p1: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RampU {
    width: u32,
    height: u32,
    world_x: f32,
    world_z: f32,
    height_min: f32,
    height_max: f32,
    direction: f32,
    _pad: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CopyU {
    width: u32,
    height: u32,
    _p0: f32,
    _p1: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ShapeU {
    width: u32,
    height: u32,
    world_x: f32,
    world_z: f32,
    seed: u32,
    octaves: u32,
    frequency: f32,
    amplitude: f32,
    lacunarity: f32,
    persistence: f32,
    offset_x: f32,
    offset_z: f32,
    ridge_sharpness: f32,
    range_angle: f32,
    range_width: f32,
    wave_frequency: f32,
    asymmetry: f32,
    depth: f32,
    canyon_width: f32,
    meander: f32,
    shape_mode: u32,
    _pad: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RiverAccumU {
    width: u32,
    height: u32,
    _p0: f32,
    _p1: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct RiverCarveU {
    width: u32,
    height: u32,
    threshold: f32,
    depth: f32,
    channel_width: f32,
    bank_smooth: f32,
    max_radius: u32,
    _pad: u32,
}

#[derive(Clone, Copy)]
enum TexSlot {
    Ping,
    #[allow(dead_code)]
    Pong,
    Layer,
    MaskOnes,
    WaterA,
    WaterB,
    SedA,
    SedB,
    Cache(LayerId),
    /// Pre-blend layer contribution (noise/shape/flat), reusable when only upstream changed.
    Contrib(LayerId),
}

struct HeightTex {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

impl HeightTex {
    fn new(device: &wgpu::Device, label: &str, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            width,
            height,
        }
    }
}

struct Pipe {
    pipeline: wgpu::ComputePipeline,
    bgl: wgpu::BindGroupLayout,
}

fn make_pipe(device: &wgpu::Device, label: &str, wgsl: &str, bgl: wgpu::BindGroupLayout) -> Pipe {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: Some(&pl),
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    Pipe { pipeline, bgl }
}

fn storage_write_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::StorageTexture {
            access: wgpu::StorageTextureAccess::WriteOnly,
            format: wgpu::TextureFormat::R32Float,
            view_dimension: wgpu::TextureViewDimension::D2,
        },
        count: None,
    }
}

fn tex_read_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

/// Ring of small uniform buffers so many dispatches can share one submit.
struct UniformPool {
    buffers: Vec<wgpu::Buffer>,
    next: usize,
}

impl UniformPool {
    const SLOT_SIZE: u64 = 256;

    fn new(device: &wgpu::Device, capacity: usize) -> Self {
        let mut buffers = Vec::with_capacity(capacity);
        for i in 0..capacity {
            buffers.push(Self::make_slot(device, i));
        }
        Self { buffers, next: 0 }
    }

    fn make_slot(device: &wgpu::Device, index: usize) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(&format!("gpu-engine-u-{index}")),
            size: Self::SLOT_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn reset(&mut self) {
        self.next = 0;
    }

    fn write<T: Pod>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &T,
    ) -> wgpu::Buffer {
        debug_assert!(
            std::mem::size_of::<T>() as u64 <= Self::SLOT_SIZE,
            "uniform larger than pool slot"
        );
        if self.next >= self.buffers.len() {
            let start = self.buffers.len();
            let grow = self.buffers.len().max(8);
            for i in start..start + grow {
                self.buffers.push(Self::make_slot(device, i));
            }
        }
        let buf = self.buffers[self.next].clone();
        self.next += 1;
        queue.write_buffer(&buf, 0, bytemuck::bytes_of(data));
        buf
    }
}

/// Generators whose height field does not depend on the composed input below them.
fn layer_input_independent(kind: &LayerKind) -> bool {
    matches!(
        kind,
        LayerKind::Flat(_)
            | LayerKind::Ramp(_)
            | LayerKind::NoiseValue(_)
            | LayerKind::NoisePerlin(_)
            | LayerKind::Fbm(_)
            | LayerKind::Ridged(_)
            | LayerKind::Mountains(_)
            | LayerKind::Dunes(_)
            | LayerKind::Canyons(_)
            | LayerKind::DomainWarp(_)
    )
}

/// Whether this layer kind can run fully on the GPU preview path.
pub fn layer_gpu_supported(kind: &LayerKind) -> bool {
    matches!(
        kind,
        LayerKind::SculptBase(_)
            | LayerKind::Flat(_)
            | LayerKind::Ramp(_)
            | LayerKind::NoiseValue(_)
            | LayerKind::NoisePerlin(_)
            | LayerKind::Fbm(_)
            | LayerKind::Ridged(_)
            | LayerKind::ThermalErosion(_)
            | LayerKind::HydraulicErosion(_)
            | LayerKind::Blur(_)
            | LayerKind::Terrace(_)
            | LayerKind::Mountains(_)
            | LayerKind::Dunes(_)
            | LayerKind::Canyons(_)
            | LayerKind::DomainWarp(_)
            | LayerKind::Coastal(_)
            | LayerKind::RiverCarve(_)
            | LayerKind::Materials(_)
            | LayerKind::Biomes(_)
            | LayerKind::Vegetation(_)
    )
}

fn gpu_mask_supported(layer: &Layer, assets: &[MaskAsset]) -> bool {
    layer.common.masks.iter().all(|binding| {
        matches!(
            assets
                .iter()
                .find(|asset| asset.id == binding.id)
                .map(|asset| &asset.source),
            Some(MaskSource::Constant(_))
                | Some(MaskSource::Height { .. })
                | Some(MaskSource::Slope { .. })
                | Some(MaskSource::Curvature { .. })
                | Some(MaskSource::Noise { .. })
                | Some(MaskSource::FlowDirection)
                | Some(MaskSource::FlowAccumulation { .. })
                | Some(MaskSource::Wetness)
        )
    })
}

fn composite_gpu_mask(
    layer: &Layer,
    baked: &HashMap<terra_core::mask::MaskId, MaskField>,
    metrics: HeightfieldMetrics,
) -> MaskField {
    let mut result = MaskField::ones(metrics);
    for binding in &layer.common.masks {
        let Some(field) = baked.get(&binding.id) else {
            return MaskField::ones(metrics);
        };
        for (out, source) in result.data_mut().iter_mut().zip(field.data()) {
            let mut value = *source * binding.strength;
            if binding.invert {
                value = 1.0 - value;
            }
            *out = (*out * value).clamp(0.0, 1.0);
        }
    }
    result
}

/// Result of a GPU preview evaluation.
pub struct GpuEvalResult {
    pub width: u32,
    pub height: u32,
    pub world_size: (f32, f32),
    pub height_range: (f32, f32),
    pub fully_gpu: bool,
    pub cpu: Option<Heightfield>,
    /// First flattened layer that must resume on the CPU.
    pub resume_cpu_from: Option<usize>,
}

/// GPU stack evaluator for interactive preview.
pub struct GpuTerrainEngine {
    fill: Pipe,
    noise: Pipe,
    blend: Pipe,
    copy: Pipe,
    thermal: Pipe,
    thermal_apply: Pipe,
    hydraulic: Pipe,
    blur: Pipe,
    terrace: Pipe,
    ramp: Pipe,
    shapes: Pipe,
    river_accum: Pipe,
    river_carve: Pipe,
    uniform_pool: UniformPool,
    ping: HeightTex,
    pong: HeightTex,
    layer_tex: HeightTex,
    mask_ones: HeightTex,
    water_a: HeightTex,
    water_b: HeightTex,
    delta: HeightTex,
    sed_a: HeightTex,
    sed_b: HeightTex,
    layer_cache: HashMap<LayerId, HeightTex>,
    /// Pre-blend generator output, keyed by layer id.
    layer_contrib: HashMap<LayerId, HeightTex>,
    dirty: HashSet<LayerId>,
    metrics: HeightfieldMetrics,
    approx_range: (f32, f32),
    /// Index of texture holding current composed height: 0=ping, 1=pong.
    current: u8,
    /// Spatial dirty tiles for region present / normal recompute (Wave D).
    tile_sched: TileScheduler,
    /// Optional texel-space edit bounds supplied by interactive painting.
    last_dirty_rect: Option<(u32, u32, u32, u32)>,
    last_quality: Option<PreviewQuality>,
    /// Maximum thermal/hydraulic iterations submitted in one interactive tick.
    pub max_sim_iters_per_tick: u32,
}

impl GpuTerrainEngine {
    pub fn new(device: &wgpu::Device, initial: u32) -> Self {
        let w = initial.max(8);
        let fill_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("fill-bgl"),
            entries: &[uniform_entry(0), storage_write_entry(1)],
        });
        let noise_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("noise-bgl"),
            entries: &[uniform_entry(0), storage_write_entry(1)],
        });
        let blend_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blend-bgl"),
            entries: &[
                uniform_entry(0),
                tex_read_entry(1),
                tex_read_entry(2),
                tex_read_entry(3),
                storage_write_entry(4),
            ],
        });
        let copy_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("copy-bgl"),
            entries: &[uniform_entry(0), tex_read_entry(1), storage_write_entry(2)],
        });
        let thermal_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("thermal-bgl"),
            entries: &[uniform_entry(0), tex_read_entry(1), storage_write_entry(2)],
        });
        let thermal_apply_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("thermal-apply-bgl"),
            entries: &[
                uniform_entry(0),
                tex_read_entry(1),
                tex_read_entry(2),
                storage_write_entry(3),
            ],
        });
        let hydraulic_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hydraulic-bgl"),
            entries: &[
                uniform_entry(0),
                tex_read_entry(1),
                tex_read_entry(2),
                tex_read_entry(3),
                storage_write_entry(4),
                storage_write_entry(5),
                storage_write_entry(6),
            ],
        });
        let blur_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blur-bgl"),
            entries: &[uniform_entry(0), tex_read_entry(1), storage_write_entry(2)],
        });
        let terrace_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrace-bgl"),
            entries: &[uniform_entry(0), tex_read_entry(1), storage_write_entry(2)],
        });
        let ramp_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ramp-bgl"),
            entries: &[uniform_entry(0), storage_write_entry(1)],
        });
        let shapes_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("shapes-bgl"),
            entries: &[uniform_entry(0), storage_write_entry(1)],
        });
        let river_accum_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("river-accum-bgl"),
            entries: &[
                uniform_entry(0),
                tex_read_entry(1),
                tex_read_entry(2),
                storage_write_entry(3),
            ],
        });
        let river_carve_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("river-carve-bgl"),
            entries: &[
                uniform_entry(0),
                tex_read_entry(1),
                tex_read_entry(2),
                storage_write_entry(3),
            ],
        });

        let fill = make_pipe(device, "fill", include_str!("shaders/fill.wgsl"), fill_bgl);
        let noise = make_pipe(
            device,
            "noise",
            include_str!("shaders/noise.wgsl"),
            noise_bgl,
        );
        let blend = make_pipe(
            device,
            "blend",
            include_str!("shaders/blend.wgsl"),
            blend_bgl,
        );
        let copy = make_pipe(device, "copy", include_str!("shaders/copy.wgsl"), copy_bgl);
        let thermal = make_pipe(
            device,
            "thermal",
            include_str!("shaders/thermal_tex.wgsl"),
            thermal_bgl,
        );
        let thermal_apply = make_pipe(
            device,
            "thermal-apply",
            include_str!("shaders/thermal_apply.wgsl"),
            thermal_apply_bgl,
        );
        let hydraulic = make_pipe(
            device,
            "hydraulic",
            include_str!("shaders/hydraulic_tex.wgsl"),
            hydraulic_bgl,
        );
        let blur = make_pipe(device, "blur", include_str!("shaders/blur.wgsl"), blur_bgl);
        let terrace = make_pipe(
            device,
            "terrace",
            include_str!("shaders/terrace.wgsl"),
            terrace_bgl,
        );
        let ramp = make_pipe(device, "ramp", include_str!("shaders/ramp.wgsl"), ramp_bgl);
        let shapes = make_pipe(
            device,
            "shapes",
            include_str!("shaders/shapes.wgsl"),
            shapes_bgl,
        );
        let river_accum = make_pipe(
            device,
            "river-accum",
            include_str!("shaders/river_accum.wgsl"),
            river_accum_bgl,
        );
        let river_carve = make_pipe(
            device,
            "river-carve",
            include_str!("shaders/river_carve.wgsl"),
            river_carve_bgl,
        );
        let ping = HeightTex::new(device, "ping", w, w);
        let pong = HeightTex::new(device, "pong", w, w);
        let layer_tex = HeightTex::new(device, "layer", w, w);
        let mask_ones = HeightTex::new(device, "mask-ones", w, w);
        let water_a = HeightTex::new(device, "water-a", w, w);
        let water_b = HeightTex::new(device, "water-b", w, w);
        let delta = HeightTex::new(device, "thermal-delta", w, w);
        let sed_a = HeightTex::new(device, "sed-a", w, w);
        let sed_b = HeightTex::new(device, "sed-b", w, w);

        Self {
            fill,
            noise,
            blend,
            copy,
            thermal,
            thermal_apply,
            hydraulic,
            blur,
            terrace,
            ramp,
            shapes,
            river_accum,
            river_carve,
            uniform_pool: UniformPool::new(device, 64),
            ping,
            pong,
            layer_tex,
            mask_ones,
            water_a,
            water_b,
            delta,
            sed_a,
            sed_b,
            layer_cache: HashMap::new(),
            layer_contrib: HashMap::new(),
            dirty: HashSet::new(),
            metrics: HeightfieldMetrics {
                width: w,
                height: w,
                world_size_x: 1000.0,
                world_size_z: 1000.0,
                tile_size: w,
                halo: 0,
            },
            approx_range: (0.0, 120.0),
            current: 0,
            tile_sched: TileScheduler::new(),
            last_dirty_rect: None,
            last_quality: None,
            max_sim_iters_per_tick: 8,
        }
    }

    /// Bounding sample rect of tiles touched since last clear (padded for normals).
    pub fn dirty_region(&self, pad: u32) -> Option<SampleRect> {
        self.tile_sched.dirty_bounds(&self.metrics, pad)
    }

    pub fn take_dirty_region(&mut self, pad: u32) -> Option<SampleRect> {
        let r = self.dirty_region(pad);
        self.tile_sched.clear();
        r
    }

    fn mark_all_tiles_dirty(&mut self) {
        self.tile_sched.clear();
        for tz in 0..self.metrics.tiles_z() {
            for tx in 0..self.metrics.tiles_x() {
                self.tile_sched.mark_tile(TileId { tx, tz });
            }
        }
    }

    pub fn mark_dirty(&mut self, id: LayerId) {
        self.dirty.insert(id);
    }

    /// Set the texel-space bounds of the most recent local terrain edit.
    pub fn set_dirty_rect(&mut self, rect: Option<(u32, u32, u32, u32)>) {
        self.last_dirty_rect = rect;
    }

    fn mark_tiles_overlapping_rect(&mut self, rect: (u32, u32, u32, u32)) {
        let (x, y, w, h) = rect;
        if w == 0 || h == 0 || self.metrics.width == 0 || self.metrics.height == 0 {
            return;
        }
        let max_x = x
            .saturating_add(w)
            .saturating_sub(1)
            .min(self.metrics.width - 1);
        let max_y = y
            .saturating_add(h)
            .saturating_sub(1)
            .min(self.metrics.height - 1);
        let tx0 = x.min(self.metrics.width - 1) / self.metrics.tile_size;
        let tz0 = y.min(self.metrics.height - 1) / self.metrics.tile_size;
        let tx1 = max_x / self.metrics.tile_size;
        let tz1 = max_y / self.metrics.tile_size;
        self.tile_sched.clear();
        for tz in tz0..=tz1 {
            for tx in tx0..=tx1 {
                self.tile_sched.mark_tile(TileId { tx, tz });
            }
        }
    }

    pub fn mark_dirty_from(&mut self, stack: &LayerStack, id: LayerId) {
        let layers = stack.flatten_layers();
        let mut seen = false;
        for layer in layers {
            if layer.id() == id {
                seen = true;
            }
            if seen {
                self.dirty.insert(layer.id());
            }
        }
    }

    pub fn mark_all_dirty(&mut self, stack: &LayerStack) {
        for layer in stack.flatten_layers() {
            self.dirty.insert(layer.id());
        }
    }

    pub fn output_texture(&self) -> &wgpu::Texture {
        if self.current == 0 {
            &self.ping.texture
        } else {
            &self.pong.texture
        }
    }

    fn ensure_size(&mut self, device: &wgpu::Device, metrics: HeightfieldMetrics) {
        let w = metrics.width.max(8);
        let h = metrics.height.max(8);
        if self.ping.width == w
            && self.ping.height == h
            && self.metrics.world_size_x == metrics.world_size_x
        {
            self.metrics = metrics;
            return;
        }
        self.metrics = metrics;
        self.ping = HeightTex::new(device, "ping", w, h);
        self.pong = HeightTex::new(device, "pong", w, h);
        self.layer_tex = HeightTex::new(device, "layer", w, h);
        self.mask_ones = HeightTex::new(device, "mask-ones", w, h);
        self.water_a = HeightTex::new(device, "water-a", w, h);
        self.water_b = HeightTex::new(device, "water-b", w, h);
        self.delta = HeightTex::new(device, "thermal-delta", w, h);
        self.sed_a = HeightTex::new(device, "sed-a", w, h);
        self.sed_b = HeightTex::new(device, "sed-b", w, h);
        self.layer_cache.clear();
        self.layer_contrib.clear();
        self.dirty.clear();
    }

    fn swap_current(&mut self) {
        self.current = 1 - self.current;
    }

    /// Write uniforms into the next pool slot and return that buffer.
    /// Each dispatch must bind its own slot — wgpu applies all `queue.write_buffer`
    /// transfers before the command buffer, so a single shared buffer would make
    /// every pass see only the last write.
    fn write_uniform<T: Pod>(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        data: &T,
    ) -> wgpu::Buffer {
        self.uniform_pool.write(device, queue, data)
    }

    fn view_of(&self, slot: TexSlot) -> &wgpu::TextureView {
        match slot {
            TexSlot::Ping => &self.ping.view,
            TexSlot::Pong => &self.pong.view,
            TexSlot::Layer => &self.layer_tex.view,
            TexSlot::MaskOnes => &self.mask_ones.view,
            TexSlot::WaterA => &self.water_a.view,
            TexSlot::WaterB => &self.water_b.view,
            TexSlot::SedA => &self.sed_a.view,
            TexSlot::SedB => &self.sed_b.view,
            TexSlot::Cache(id) => &self.layer_cache.get(&id).expect("cache").view,
            TexSlot::Contrib(id) => &self.layer_contrib.get(&id).expect("contrib").view,
        }
    }

    fn fill_slot(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        slot: TexSlot,
        value: f32,
    ) {
        let u = FillU {
            width: self.metrics.width,
            height: self.metrics.height,
            value,
            _pad: 0.0,
        };
        let u_buf = self.write_uniform(device, queue, &u);
        let view = self.view_of(slot);
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("fill-bg"),
            layout: &self.fill.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: u_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(view),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("fill"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.fill.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(
                (self.metrics.width + 7) / 8,
                (self.metrics.height + 7) / 8,
                1,
            );
        }
    }

    fn copy_slots(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        src: TexSlot,
        dst: TexSlot,
    ) {
        let u = CopyU {
            width: self.metrics.width,
            height: self.metrics.height,
            _p0: 0.0,
            _p1: 0.0,
        };
        let u_buf = self.write_uniform(device, queue, &u);
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("copy-bg"),
            layout: &self.copy.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: u_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(self.view_of(src)),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(self.view_of(dst)),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("copy"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.copy.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(
                (self.metrics.width + 7) / 8,
                (self.metrics.height + 7) / 8,
                1,
            );
        }
    }

    fn blend_mode_u(mode: BlendMode) -> u32 {
        match mode {
            BlendMode::Normal => 0,
            BlendMode::Add => 1,
            BlendMode::Subtract => 2,
            BlendMode::Multiply => 3,
            BlendMode::Min => 4,
            BlendMode::Max => 5,
            BlendMode::Overlay => 6,
        }
    }

    fn noise_type_u(t: FractalNoiseType) -> Option<u32> {
        match t {
            FractalNoiseType::Value => Some(0),
            FractalNoiseType::Perlin => Some(1),
            FractalNoiseType::OpenSimplex => None,
        }
    }

    fn gen_noise(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        p: &NoiseParams,
        noise_type: u32,
        ridged: bool,
    ) {
        let u = NoiseU {
            width: self.metrics.width,
            height: self.metrics.height,
            world_x: self.metrics.world_size_x,
            world_z: self.metrics.world_size_z,
            seed: (p.seed & 0xFFFF_FFFF) as u32,
            octaves: p.octaves.max(1),
            frequency: p.frequency,
            amplitude: p.amplitude,
            lacunarity: p.lacunarity,
            persistence: p.persistence,
            offset_x: p.offset_x,
            offset_z: p.offset_z,
            remap_min: p.remap_min,
            remap_max: p.remap_max,
            noise_type,
            mode: if ridged { 1 } else { 0 },
        };
        let u_buf = self.write_uniform(device, queue, &u);
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("noise-bg"),
            layout: &self.noise.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: u_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.layer_tex.view),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("noise"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.noise.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(
                (self.metrics.width + 7) / 8,
                (self.metrics.height + 7) / 8,
                1,
            );
        }
    }

    fn gen_shape(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        u: ShapeU,
    ) {
        let u_buf = self.write_uniform(device, queue, &u);
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shapes-bg"),
            layout: &self.shapes.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: u_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.layer_tex.view),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("shapes"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.shapes.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(
                (self.metrics.width + 7) / 8,
                (self.metrics.height + 7) / 8,
                1,
            );
        }
    }

    fn run_river_carve(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        p: &terra_core::layer::RiverCarveParams,
        quality: PreviewQuality,
    ) {
        let iters = match quality {
            PreviewQuality::Draft => 12u32,
            PreviewQuality::Medium => 32,
            PreviewQuality::Full | PreviewQuality::Export => {
                (self.metrics.width.min(self.metrics.height) / 4).clamp(48, 160)
            }
        };

        // Seed accumulation with unit rainfall.
        self.fill_slot(device, queue, encoder, TexSlot::WaterA, 1.0);
        self.fill_slot(device, queue, encoder, TexSlot::WaterB, 0.0);

        let height_slot = if self.current == 0 {
            TexSlot::Ping
        } else {
            TexSlot::Pong
        };
        let accum_u = RiverAccumU {
            width: self.metrics.width,
            height: self.metrics.height,
            _p0: 0.0,
            _p1: 0.0,
        };

        let mut src_a = true;
        for _ in 0..iters {
            let u_buf = self.write_uniform(device, queue, &accum_u);
            let (acc_in, acc_out) = if src_a {
                (&self.water_a.view, &self.water_b.view)
            } else {
                (&self.water_b.view, &self.water_a.view)
            };
            let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("river-accum-bg"),
                layout: &self.river_accum.bgl,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: u_buf.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(self.view_of(height_slot)),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(acc_in),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(acc_out),
                    },
                ],
            });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("river-accum"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.river_accum.pipeline);
                pass.set_bind_group(0, &bg, &[]);
                pass.dispatch_workgroups(
                    (self.metrics.width + 7) / 8,
                    (self.metrics.height + 7) / 8,
                    1,
                );
            }
            src_a = !src_a;
        }

        let carve_u = RiverCarveU {
            width: self.metrics.width,
            height: self.metrics.height,
            threshold: p.accumulation_threshold.max(1.0),
            depth: p.depth,
            channel_width: p.width.max(1.0),
            bank_smooth: p.bank_smooth.max(0.0),
            max_radius: match quality {
                PreviewQuality::Draft => 12,
                PreviewQuality::Medium => 20,
                PreviewQuality::Full | PreviewQuality::Export => 32,
            },
            _pad: 0,
        };
        let u_buf = self.write_uniform(device, queue, &carve_u);
        let acc_view = if src_a {
            &self.water_a.view
        } else {
            &self.water_b.view
        };
        let (src, dst) = if self.current == 0 {
            (&self.ping.view, &self.pong.view)
        } else {
            (&self.pong.view, &self.ping.view)
        };
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("river-carve-bg"),
            layout: &self.river_carve.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: u_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(src),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(acc_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(dst),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("river-carve"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.river_carve.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(
                (self.metrics.width + 7) / 8,
                (self.metrics.height + 7) / 8,
                1,
            );
        }
        self.swap_current();
        self.expand_range(self.approx_range.0 - p.depth * 2.0, self.approx_range.1);
    }

    fn blend_into_current(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        opacity: f32,
        mode: BlendMode,
    ) {
        let u = BlendU {
            width: self.metrics.width,
            height: self.metrics.height,
            opacity,
            mode: Self::blend_mode_u(mode),
        };
        let u_buf = self.write_uniform(device, queue, &u);
        let src_ping = self.current == 0;
        let (base, dst) = if src_ping {
            (&self.ping.view, &self.pong.view)
        } else {
            (&self.pong.view, &self.ping.view)
        };
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blend-bg"),
            layout: &self.blend.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: u_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(base),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&self.layer_tex.view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&self.mask_ones.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(dst),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("blend"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.blend.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups(
                (self.metrics.width + 7) / 8,
                (self.metrics.height + 7) / 8,
                1,
            );
        }
        self.swap_current();
    }

    fn scale_iters(quality: PreviewQuality, iters: u32) -> u32 {
        match quality {
            PreviewQuality::Draft => iters.min(4).max(1),
            PreviewQuality::Medium => iters.min(12).max(1),
            PreviewQuality::Full | PreviewQuality::Export => iters.max(1),
        }
    }

    /// Evaluate the GPU-compatible prefix of a stack, then return a CPU resume point if needed.
    pub fn evaluate(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        stack: &LayerStack,
        mask_assets: &[MaskAsset],
        metrics: HeightfieldMetrics,
        quality: PreviewQuality,
        want_cpu: bool,
    ) -> Result<GpuEvalResult, GpuError> {
        profiling::scope!("gpu_stack_eval");
        self.ensure_size(device, metrics);
        self.uniform_pool.reset();
        let quality_changed = self.last_quality.replace(quality) != Some(quality);

        let layers = stack.flatten_layers();
        if quality_changed {
            self.dirty.extend(layers.iter().map(|layer| layer.id()));
            self.layer_contrib.clear();
        }
        if layers.is_empty() {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gpu-empty"),
            });
            self.fill_slot(device, queue, &mut encoder, TexSlot::Ping, 0.0);
            self.current = 0;
            queue.submit(Some(encoder.finish()));
            return Ok(GpuEvalResult {
                width: metrics.width,
                height: metrics.height,
                world_size: (metrics.world_size_x, metrics.world_size_z),
                height_range: (0.0, 0.0),
                fully_gpu: true,
                cpu: if want_cpu {
                    Some(Heightfield::zeros(metrics))
                } else {
                    None
                },
                resume_cpu_from: None,
            });
        }

        // Painted and analysis-driven masks resume on CPU; constant, height, and slope
        // masks are baked against the current GPU prefix and uploaded before their blend.
        let cpu_from = layers.iter().position(|layer| {
            layer.common.enabled
                && (!layer_gpu_supported(&layer.kind) || !gpu_mask_supported(layer, mask_assets))
        });
        let gpu_end = cpu_from.unwrap_or(layers.len());

        let first_dirty = layers.iter().position(|l| self.dirty.contains(&l.id()));

        // All clean: restore top cache into current output (no recompute).
        if first_dirty.is_none() && cpu_from.is_none() {
            if let Some(top) = layers.last() {
                if let Some(cached) = self.layer_cache.get(&top.id()) {
                    if cached.width == metrics.width && cached.height == metrics.height {
                        let mut encoder =
                            device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                                label: Some("gpu-cache-hit"),
                            });
                        self.copy_slots(
                            device,
                            queue,
                            &mut encoder,
                            TexSlot::Cache(top.id()),
                            TexSlot::Ping,
                        );
                        self.current = 0;
                        queue.submit(Some(encoder.finish()));
                        let cpu = if want_cpu {
                            Some(self.readback_current(device, queue)?)
                        } else {
                            None
                        };
                        return Ok(GpuEvalResult {
                            width: metrics.width,
                            height: metrics.height,
                            world_size: (metrics.world_size_x, metrics.world_size_z),
                            height_range: self.approx_range,
                            fully_gpu: true,
                            cpu,
                            resume_cpu_from: None,
                        });
                    }
                }
            }
        }

        // The CPU suffix has no GPU cache. If it is clean, restore the final GPU prefix
        // instead of trying to use a non-existent cache for the stack's top layer.
        let first_dirty = first_dirty.unwrap_or(0).min(gpu_end);

        // Seed from previous layer cache when possible.
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("gpu-stack"),
        });
        self.fill_slot(device, queue, &mut encoder, TexSlot::MaskOnes, 1.0);

        // A full re-evaluation or quality change affects every sample. Local sculpt
        // edits can instead update only tiles intersecting the supplied stamp bounds.
        if first_dirty == 0 || quality_changed {
            self.mark_all_tiles_dirty();
            self.last_dirty_rect = None;
        } else if let Some(rect) = self.last_dirty_rect.take() {
            self.mark_tiles_overlapping_rect(rect);
        }

        if first_dirty == 0 {
            self.fill_slot(device, queue, &mut encoder, TexSlot::Ping, 0.0);
            self.current = 0;
            self.approx_range = (0.0, 1.0);
        } else {
            let prev_id = layers[first_dirty - 1].id();
            let cached_ok = self
                .layer_cache
                .get(&prev_id)
                .map(|c| c.width == metrics.width && c.height == metrics.height)
                .unwrap_or(false);
            if cached_ok {
                self.copy_slots(
                    device,
                    queue,
                    &mut encoder,
                    TexSlot::Cache(prev_id),
                    TexSlot::Ping,
                );
                self.current = 0;
            } else if self.layer_cache.contains_key(&prev_id) {
                self.fill_slot(device, queue, &mut encoder, TexSlot::Ping, 0.0);
                self.current = 0;
            } else {
                // No cache: re-run from start.
                drop(encoder);
                self.dirty.extend(layers.iter().map(|l| l.id()));
                return self.evaluate(
                    device,
                    queue,
                    stack,
                    mask_assets,
                    metrics,
                    quality,
                    want_cpu,
                );
            }
        }

        for layer in &layers[first_dirty..gpu_end] {
            if !layer.common.enabled {
                self.cache_current(device, queue, &mut encoder, layer.id());
                self.dirty.remove(&layer.id());
                continue;
            }
            if layer.common.masks.is_empty() {
                // A prior masked layer leaves its uploaded field in this shared texture.
                self.fill_slot(device, queue, &mut encoder, TexSlot::MaskOnes, 1.0);
            } else if matches!(quality, PreviewQuality::Draft | PreviewQuality::Medium) {
                // Accurate height/slope masks need a sync GPU→CPU readback (Maintain::Wait).
                // Skip on interactive qualities so UI/render stay responsive; Full bakes them.
                self.fill_slot(device, queue, &mut encoder, TexSlot::MaskOnes, 1.0);
            } else {
                // Height/slope sources depend on the composed input. Submit the prefix so
                // it can be read back, bake the composite CPU mask, then resume GPU blend.
                queue.submit(Some(encoder.finish()));
                let input = self.readback_current(device, queue)?;
                let baked = bake_mask_assets(mask_assets, &input, metrics, &HashMap::new());
                let mask = composite_gpu_mask(layer, &baked, metrics);
                self.upload_mask(queue, &mask);
                encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("gpu-stack-masked"),
                });
            }
            // Sculpt uploads must land on the queue before later layer_tex fills in this
            // encoder, otherwise a prior fill would overwrite the stamp buffer.
            let id = layer.id();
            let content_dirty = self.dirty.contains(&id);
            let contrib_ok = self
                .layer_contrib
                .get(&id)
                .map(|c| c.width == metrics.width && c.height == metrics.height)
                .unwrap_or(false);
            let reuse_contrib =
                !content_dirty && layer_input_independent(&layer.kind) && contrib_ok;

            if reuse_contrib {
                self.copy_slots(
                    device,
                    queue,
                    &mut encoder,
                    TexSlot::Contrib(id),
                    TexSlot::Layer,
                );
                self.blend_into_current(
                    device,
                    queue,
                    &mut encoder,
                    layer.common.opacity,
                    layer.common.blend,
                );
            } else {
                if let LayerKind::SculptBase(params) = &layer.kind {
                    queue.submit(Some(encoder.finish()));
                    self.upload_sculpt_to_layer(queue, params);
                    encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("gpu-stack-sculpt"),
                    });
                }
                self.eval_layer(device, queue, &mut encoder, layer, quality)?;
                if layer_input_independent(&layer.kind) {
                    let needs_new = self
                        .layer_contrib
                        .get(&id)
                        .map(|t| t.width != metrics.width || t.height != metrics.height)
                        .unwrap_or(true);
                    if needs_new {
                        self.layer_contrib.insert(
                            id,
                            HeightTex::new(device, "layer-contrib", metrics.width, metrics.height),
                        );
                    }
                    self.copy_slots(
                        device,
                        queue,
                        &mut encoder,
                        TexSlot::Layer,
                        TexSlot::Contrib(id),
                    );
                }
            }
            self.cache_current(device, queue, &mut encoder, id);
            self.dirty.remove(&id);
        }

        queue.submit(Some(encoder.finish()));

        // Hybrid CPU suffix + mask readback only on Full/Export (or explicit want_cpu).
        // Draft/Medium present the GPU prefix without Maintain::Wait stalls.
        let need_cpu = want_cpu
            || (cpu_from.is_some()
                && matches!(quality, PreviewQuality::Full | PreviewQuality::Export));
        let cpu = if need_cpu {
            Some(self.readback_current(device, queue)?)
        } else {
            None
        };
        let resume = if need_cpu { cpu_from } else { None };

        Ok(GpuEvalResult {
            width: metrics.width,
            height: metrics.height,
            world_size: (metrics.world_size_x, metrics.world_size_z),
            height_range: self.approx_range,
            fully_gpu: resume.is_none(),
            cpu,
            resume_cpu_from: resume,
        })
    }

    fn upload_mask(&self, queue: &wgpu::Queue, mask: &MaskField) {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.mask_ones.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(mask.data()),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(self.metrics.width * 4),
                rows_per_image: Some(self.metrics.height),
            },
            wgpu::Extent3d {
                width: self.metrics.width,
                height: self.metrics.height,
                depth_or_array_layers: 1,
            },
        );
    }

    /// Resample the sculpt paint buffer into `layer_tex` at the current eval resolution.
    fn upload_sculpt_to_layer(&self, queue: &wgpu::Queue, params: &SculptParams) {
        let w = self.metrics.width;
        let h = self.metrics.height;
        let mut dense = vec![0f32; (w as usize).saturating_mul(h as usize)];
        for j in 0..h {
            for i in 0..w {
                let u = (i as f32 + 0.5) / w as f32;
                let v = (j as f32 + 0.5) / h as f32;
                dense[(j * w + i) as usize] = params.sample_bilinear(u, v);
            }
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.layer_tex.texture,
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

    fn cache_current(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        id: LayerId,
    ) {
        let w = self.metrics.width;
        let h = self.metrics.height;
        let needs_new = self
            .layer_cache
            .get(&id)
            .map(|t| t.width != w || t.height != h)
            .unwrap_or(true);
        if needs_new {
            self.layer_cache
                .insert(id, HeightTex::new(device, "layer-cache", w, h));
        }
        let u = CopyU {
            width: w,
            height: h,
            _p0: 0.0,
            _p1: 0.0,
        };
        let u_buf = self.write_uniform(device, queue, &u);

        // Avoid simultaneous borrows: resolve views by current index + cache entry.
        let src_is_ping = self.current == 0;
        let src_view = if src_is_ping {
            &self.ping.view
        } else {
            &self.pong.view
        };
        let cache = self.layer_cache.get(&id).expect("cache just inserted");
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cache-bg"),
            layout: &self.copy.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: u_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(src_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&cache.view),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("cache-copy"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.copy.pipeline);
            pass.set_bind_group(0, &bg, &[]);
            pass.dispatch_workgroups((w + 7) / 8, (h + 7) / 8, 1);
        }
    }

    fn eval_layer(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        layer: &Layer,
        quality: PreviewQuality,
    ) -> Result<(), GpuError> {
        match &layer.kind {
            LayerKind::SculptBase(p) => {
                // `layer_tex` was filled by `upload_sculpt_to_layer` just before this call.
                let (lo, hi) = p.sample_range();
                self.expand_range(lo, hi);
                self.blend_into_current(
                    device,
                    queue,
                    encoder,
                    layer.common.opacity,
                    layer.common.blend,
                );
            }
            LayerKind::Flat(p) => {
                self.fill_slot(device, queue, encoder, TexSlot::Layer, p.height);
                self.expand_range(p.height, p.height);
                self.blend_into_current(
                    device,
                    queue,
                    encoder,
                    layer.common.opacity,
                    layer.common.blend,
                );
            }
            LayerKind::Ramp(p) => {
                let u = RampU {
                    width: self.metrics.width,
                    height: self.metrics.height,
                    world_x: self.metrics.world_size_x,
                    world_z: self.metrics.world_size_z,
                    height_min: p.height_min,
                    height_max: p.height_max,
                    direction: p.direction,
                    _pad: 0.0,
                };
                let u_buf = self.write_uniform(device, queue, &u);
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("ramp-bg"),
                    layout: &self.ramp.bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: u_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&self.layer_tex.view),
                        },
                    ],
                });
                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("ramp"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.ramp.pipeline);
                    pass.set_bind_group(0, &bg, &[]);
                    pass.dispatch_workgroups(
                        (self.metrics.width + 7) / 8,
                        (self.metrics.height + 7) / 8,
                        1,
                    );
                }
                self.expand_range(p.height_min, p.height_max);
                self.blend_into_current(
                    device,
                    queue,
                    encoder,
                    layer.common.opacity,
                    layer.common.blend,
                );
            }
            LayerKind::NoiseValue(p) => {
                self.gen_noise(device, queue, encoder, p, 0, false);
                self.expand_range(0.0, p.amplitude);
                self.blend_into_current(
                    device,
                    queue,
                    encoder,
                    layer.common.opacity,
                    layer.common.blend,
                );
            }
            LayerKind::NoisePerlin(p) => {
                self.gen_noise(device, queue, encoder, p, 1, false);
                self.expand_range(0.0, p.amplitude);
                self.blend_into_current(
                    device,
                    queue,
                    encoder,
                    layer.common.opacity,
                    layer.common.blend,
                );
            }
            LayerKind::Fbm(p) => {
                let nt = Self::noise_type_u(p.noise)
                    .ok_or_else(|| GpuError::Wgpu("OpenSimplex fBm not on GPU preview".into()))?;
                self.gen_noise(device, queue, encoder, &p.base, nt, false);
                self.expand_range(0.0, p.base.amplitude);
                self.blend_into_current(
                    device,
                    queue,
                    encoder,
                    layer.common.opacity,
                    layer.common.blend,
                );
            }
            LayerKind::Ridged(p) => {
                let nt = Self::noise_type_u(p.noise).unwrap_or(1);
                self.gen_noise(device, queue, encoder, &p.base, nt, true);
                self.expand_range(0.0, p.base.amplitude);
                self.blend_into_current(
                    device,
                    queue,
                    encoder,
                    layer.common.opacity,
                    layer.common.blend,
                );
            }
            // Dedicated range-mask / dune asymmetry / canyon meander kernels.
            LayerKind::Mountains(p) => {
                let u = ShapeU {
                    width: self.metrics.width,
                    height: self.metrics.height,
                    world_x: self.metrics.world_size_x,
                    world_z: self.metrics.world_size_z,
                    seed: (p.base.seed & 0xFFFF_FFFF) as u32,
                    octaves: p.base.octaves.max(1),
                    frequency: p.base.frequency,
                    amplitude: p.base.amplitude,
                    lacunarity: p.base.lacunarity,
                    persistence: p.base.persistence,
                    offset_x: p.base.offset_x,
                    offset_z: p.base.offset_z,
                    ridge_sharpness: p.ridge_sharpness,
                    range_angle: p.range_angle,
                    range_width: p.range_width,
                    wave_frequency: 0.0,
                    asymmetry: 0.0,
                    depth: 0.0,
                    canyon_width: 0.0,
                    meander: 0.0,
                    shape_mode: 0,
                    _pad: 0,
                };
                self.gen_shape(device, queue, encoder, u);
                self.expand_range(0.0, p.base.amplitude);
                self.blend_into_current(
                    device,
                    queue,
                    encoder,
                    layer.common.opacity,
                    layer.common.blend,
                );
            }
            LayerKind::Dunes(p) => {
                let u = ShapeU {
                    width: self.metrics.width,
                    height: self.metrics.height,
                    world_x: self.metrics.world_size_x,
                    world_z: self.metrics.world_size_z,
                    seed: (p.base.seed & 0xFFFF_FFFF) as u32,
                    octaves: p.base.octaves.max(1),
                    frequency: p.base.frequency,
                    amplitude: p.base.amplitude,
                    lacunarity: p.base.lacunarity,
                    persistence: p.base.persistence,
                    offset_x: p.base.offset_x,
                    offset_z: p.base.offset_z,
                    ridge_sharpness: 0.0,
                    range_angle: 0.0,
                    range_width: 0.0,
                    wave_frequency: p.wave_frequency,
                    asymmetry: p.asymmetry,
                    depth: 0.0,
                    canyon_width: 0.0,
                    meander: 0.0,
                    shape_mode: 1,
                    _pad: 0,
                };
                self.gen_shape(device, queue, encoder, u);
                self.expand_range(0.0, p.base.amplitude);
                self.blend_into_current(
                    device,
                    queue,
                    encoder,
                    layer.common.opacity,
                    layer.common.blend,
                );
            }
            LayerKind::Canyons(p) => {
                let u = ShapeU {
                    width: self.metrics.width,
                    height: self.metrics.height,
                    world_x: self.metrics.world_size_x,
                    world_z: self.metrics.world_size_z,
                    seed: (p.seed & 0xFFFF_FFFF) as u32,
                    octaves: 1,
                    frequency: 1.0,
                    amplitude: 1.0,
                    lacunarity: 2.0,
                    persistence: 0.5,
                    offset_x: 0.0,
                    offset_z: 0.0,
                    ridge_sharpness: 0.0,
                    range_angle: 0.0,
                    range_width: 0.0,
                    wave_frequency: 0.0,
                    asymmetry: 0.0,
                    depth: p.depth,
                    canyon_width: p.width,
                    meander: p.meander,
                    shape_mode: 2,
                    _pad: 0,
                };
                self.gen_shape(device, queue, encoder, u);
                self.expand_range(-p.depth, 0.0);
                self.blend_into_current(
                    device,
                    queue,
                    encoder,
                    layer.common.opacity,
                    layer.common.blend,
                );
            }
            LayerKind::DomainWarp(p) => {
                self.gen_noise(device, queue, encoder, &p.base, 1, false);
                self.expand_range(0.0, p.base.amplitude);
                self.blend_into_current(
                    device,
                    queue,
                    encoder,
                    layer.common.opacity,
                    layer.common.blend,
                );
            }
            LayerKind::ThermalErosion(p) => {
                let talus = p.talus_angle_deg.to_radians().tan() * self.metrics.dx();
                let iters = Self::scale_iters(quality, p.iterations).min(match quality {
                    PreviewQuality::Draft => self.max_sim_iters_per_tick.max(1),
                    PreviewQuality::Medium => 24,
                    PreviewQuality::Full | PreviewQuality::Export => u32::MAX,
                });
                for _ in 0..iters {
                    let strength = p.strength;
                    let talus_v = talus;
                    // Inline thermal step (avoid borrowing self.thermal while mutably borrowing self)
                    let u = ThermalU {
                        width: self.metrics.width,
                        height: self.metrics.height,
                        dx: self.metrics.dx(),
                        talus: talus_v,
                        strength,
                        _p2: 0.0,
                        _p3: 0.0,
                        _pad: 0.0,
                    };
                    let u_buf = self.write_uniform(device, queue, &u);
                    let src_ping = self.current == 0;
                    let (src, dst) = if src_ping {
                        (&self.ping.view, &self.pong.view)
                    } else {
                        (&self.pong.view, &self.ping.view)
                    };
                    let delta_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("thermal-delta-bg"),
                        layout: &self.thermal.bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: u_buf.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(src),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(&self.delta.view),
                            },
                        ],
                    });
                    {
                        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some("thermal-delta"),
                            timestamp_writes: None,
                        });
                        pass.set_pipeline(&self.thermal.pipeline);
                        pass.set_bind_group(0, &delta_bg, &[]);
                        pass.dispatch_workgroups(
                            (self.metrics.width + 7) / 8,
                            (self.metrics.height + 7) / 8,
                            1,
                        );
                    }
                    let apply_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("thermal-apply-bg"),
                        layout: &self.thermal_apply.bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: u_buf.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(src),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(&self.delta.view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: wgpu::BindingResource::TextureView(dst),
                            },
                        ],
                    });
                    {
                        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some("thermal-apply"),
                            timestamp_writes: None,
                        });
                        pass.set_pipeline(&self.thermal_apply.pipeline);
                        pass.set_bind_group(0, &apply_bg, &[]);
                        pass.dispatch_workgroups(
                            (self.metrics.width + 7) / 8,
                            (self.metrics.height + 7) / 8,
                            1,
                        );
                    }
                    self.swap_current();
                }
            }
            LayerKind::HydraulicErosion(p) => {
                self.fill_slot(device, queue, encoder, TexSlot::WaterA, 0.0);
                self.fill_slot(device, queue, encoder, TexSlot::WaterB, 0.0);
                self.fill_slot(device, queue, encoder, TexSlot::SedA, 0.0);
                self.fill_slot(device, queue, encoder, TexSlot::SedB, 0.0);
                let iters = Self::scale_iters(quality, p.iterations).min(match quality {
                    PreviewQuality::Draft => self.max_sim_iters_per_tick.max(1),
                    PreviewQuality::Medium => 24,
                    PreviewQuality::Full | PreviewQuality::Export => u32::MAX,
                });
                let mut water_flip = false;
                for _ in 0..iters {
                    let u = HydraulicU {
                        width: self.metrics.width,
                        height: self.metrics.height,
                        timestep: p.timestep,
                        rainfall: p.rainfall,
                        evaporation: p.evaporation,
                        erosion: p.erosion,
                        deposition: p.deposition,
                        capacity: p.capacity,
                    };
                    let u_buf = self.write_uniform(device, queue, &u);
                    let src_ping = self.current == 0;
                    let (h_src, h_dst) = if src_ping {
                        (&self.ping.view, &self.pong.view)
                    } else {
                        (&self.pong.view, &self.ping.view)
                    };
                    let (w_src, w_dst) = if water_flip {
                        (&self.water_b.view, &self.water_a.view)
                    } else {
                        (&self.water_a.view, &self.water_b.view)
                    };
                    let (s_src, s_dst) = if water_flip {
                        (&self.sed_b.view, &self.sed_a.view)
                    } else {
                        (&self.sed_a.view, &self.sed_b.view)
                    };
                    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("hydraulic-bg"),
                        layout: &self.hydraulic.bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: u_buf.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(h_src),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(w_src),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: wgpu::BindingResource::TextureView(s_src),
                            },
                            wgpu::BindGroupEntry {
                                binding: 4,
                                resource: wgpu::BindingResource::TextureView(h_dst),
                            },
                            wgpu::BindGroupEntry {
                                binding: 5,
                                resource: wgpu::BindingResource::TextureView(w_dst),
                            },
                            wgpu::BindGroupEntry {
                                binding: 6,
                                resource: wgpu::BindingResource::TextureView(s_dst),
                            },
                        ],
                    });
                    {
                        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some("hydraulic"),
                            timestamp_writes: None,
                        });
                        pass.set_pipeline(&self.hydraulic.pipeline);
                        pass.set_bind_group(0, &bg, &[]);
                        pass.dispatch_workgroups(
                            (self.metrics.width + 7) / 8,
                            (self.metrics.height + 7) / 8,
                            1,
                        );
                    }
                    self.swap_current();
                    water_flip = !water_flip;
                }
            }
            LayerKind::RiverCarve(p) => {
                self.run_river_carve(device, queue, encoder, p, quality);
            }
            LayerKind::Blur(p) => {
                let iters = p.iterations.max(1).min(8);
                for _ in 0..iters {
                    let u = BlurU {
                        width: self.metrics.width,
                        height: self.metrics.height,
                        radius: p.radius.max(1).min(8),
                        _pad: 0,
                    };
                    let u_buf = self.write_uniform(device, queue, &u);
                    let src_ping = self.current == 0;
                    let (src, dst) = if src_ping {
                        (&self.ping.view, &self.pong.view)
                    } else {
                        (&self.pong.view, &self.ping.view)
                    };
                    let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("blur-bg"),
                        layout: &self.blur.bgl,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: u_buf.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(src),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(dst),
                            },
                        ],
                    });
                    {
                        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                            label: Some("blur"),
                            timestamp_writes: None,
                        });
                        pass.set_pipeline(&self.blur.pipeline);
                        pass.set_bind_group(0, &bg, &[]);
                        pass.dispatch_workgroups(
                            (self.metrics.width + 7) / 8,
                            (self.metrics.height + 7) / 8,
                            1,
                        );
                    }
                    self.swap_current();
                }
            }
            // Coastal filtering is approximated as a passthrough until a
            // sea-level-aware GPU kernel is available.
            LayerKind::Coastal(_) => {
                let src = if self.current == 0 {
                    TexSlot::Ping
                } else {
                    TexSlot::Pong
                };
                let dst = if self.current == 0 {
                    TexSlot::Pong
                } else {
                    TexSlot::Ping
                };
                self.copy_slots(device, queue, encoder, src, dst);
                self.swap_current();
            }
            // These layers populate CPU-side auxiliary textures; height passes through.
            LayerKind::Materials(_) | LayerKind::Biomes(_) | LayerKind::Vegetation(_) => {}
            LayerKind::Terrace(p) => {
                let u = TerraceU {
                    width: self.metrics.width,
                    height: self.metrics.height,
                    levels: p.levels,
                    sharpness: p.sharpness,
                    min_h: self.approx_range.0 - 1e-3,
                    max_h: self.approx_range.1 + 1e-3,
                    _p0: 0.0,
                    _p1: 0.0,
                };
                let u_buf = self.write_uniform(device, queue, &u);
                let src_ping = self.current == 0;
                let (src, dst) = if src_ping {
                    (&self.ping.view, &self.pong.view)
                } else {
                    (&self.pong.view, &self.ping.view)
                };
                let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("terrace-bg"),
                    layout: &self.terrace.bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: u_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(src),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(dst),
                        },
                    ],
                });
                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("terrace"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.terrace.pipeline);
                    pass.set_bind_group(0, &bg, &[]);
                    pass.dispatch_workgroups(
                        (self.metrics.width + 7) / 8,
                        (self.metrics.height + 7) / 8,
                        1,
                    );
                }
                self.swap_current();
            }
            _ => {
                return Err(GpuError::Wgpu("unsupported layer".into()));
            }
        }
        Ok(())
    }

    fn expand_range(&mut self, lo: f32, hi: f32) {
        let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        self.approx_range.0 = self.approx_range.0.min(lo);
        self.approx_range.1 = self.approx_range.1.max(hi);
        if self.approx_range.0 > self.approx_range.1 {
            self.approx_range.1 = self.approx_range.0 + 1e-3;
        }
    }

    pub fn readback_current(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Result<Heightfield, GpuError> {
        let w = self.metrics.width;
        let h = self.metrics.height;
        let unpadded = w * 4;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded = (unpadded + align - 1) / align * align;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gpu-readback-buf"),
            size: (padded * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("gpu-readback-enc"),
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: self.output_texture(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));
        let padded_f32 = readback_f32(device, queue, &buf, (padded * h / 4) as usize)?;
        let mut dense = Vec::with_capacity((w * h) as usize);
        let row_floats = (padded / 4) as usize;
        for y in 0..h as usize {
            let start = y * row_floats;
            dense.extend_from_slice(&padded_f32[start..start + w as usize]);
        }
        Ok(Heightfield::from_dense(self.metrics, &dense))
    }
}

#[cfg(test)]
mod smoke_tests {
    use super::*;
    use crate::GpuContext;
    use terra_core::eval::PreviewQuality;
    use terra_core::heightfield::HeightfieldMetrics;
    use terra_core::layer::{
        FlatParams, Layer, LayerKind, LayerStack, NoiseParams, SculptParams,
    };

    /// Regression for uniform isolation via pool slots: Draft must composite layers
    /// even when blend and cache share one submit (each pass gets its own uniform buffer).
    #[test]
    fn draft_eval_composites_sculpt_and_noise() {
        let Ok(ctx) = GpuContext::new() else {
            // Headless CI without a GPU adapter.
            return;
        };
        let metrics = HeightfieldMetrics::new(64, 64, 64.0, 64.0);
        let mut stack = LayerStack::new();
        let base = Layer::new(
            "Base",
            LayerKind::SculptBase(SculptParams::filled(64, 20.0)),
        );
        let base_id = base.id();
        stack.push(base);
        stack.push(Layer::new(
            "Hills",
            LayerKind::NoiseValue(NoiseParams {
                seed: 1,
                frequency: 0.05,
                amplitude: 10.0,
                octaves: 1,
                lacunarity: 2.0,
                persistence: 0.5,
                ..NoiseParams::default()
            }),
        ));

        let mut engine = GpuTerrainEngine::new(&ctx.device, metrics.width);
        engine.mark_dirty(base_id);
        let before = engine
            .evaluate(
                &ctx.device,
                &ctx.queue,
                &stack,
                &[],
                metrics,
                PreviewQuality::Draft,
                true,
            )
            .expect("draft eval");
        let hf0 = before.cpu.expect("cpu readback");
        let center0 = hf0.get(32, 32);
        // Without per-pass submit, blends see opacity 0 and the field stays ~0.
        assert!(
            center0 > 15.0,
            "expected sculpt base (~20) through Draft blend, got {center0}"
        );

        // Live raise: stamp then re-eval Draft without waiting for CPU refine.
        {
            let mut layers = stack.flatten_layers_mut();
            if let LayerKind::SculptBase(params) = &mut layers[0].kind {
                params.stamp_circle(0.5, 0.5, 0.15, 25.0, 0);
            }
        }
        engine.mark_dirty_from(&stack, base_id);
        let after = engine
            .evaluate(
                &ctx.device,
                &ctx.queue,
                &stack,
                &[],
                metrics,
                PreviewQuality::Draft,
                true,
            )
            .expect("draft eval after stamp");
        let hf1 = after.cpu.expect("cpu readback");
        let center1 = hf1.get(32, 32);
        assert!(
            center1 > center0 + 5.0,
            "live Raise should lift Draft heights while held; before={center0} after={center1}"
        );
    }

    #[test]
    fn draft_eval_flat_survives_cache_copy_uniform() {
        let Ok(ctx) = GpuContext::new() else {
            return;
        };
        let metrics = HeightfieldMetrics::new(32, 32, 32.0, 32.0);
        let mut stack = LayerStack::new();
        let layer = Layer::new("Flat", LayerKind::Flat(FlatParams { height: 50.0 }));
        let id = layer.id();
        stack.push(layer);

        let mut engine = GpuTerrainEngine::new(&ctx.device, metrics.width);
        engine.mark_dirty(id);
        let result = engine
            .evaluate(
                &ctx.device,
                &ctx.queue,
                &stack,
                &[],
                metrics,
                PreviewQuality::Draft,
                true,
            )
            .expect("flat draft");
        let hf = result.cpu.expect("cpu");
        let mid = hf.get(16, 16);
        assert!(
            (mid - 50.0).abs() < 0.01,
            "Flat blend must not be clobbered by cache CopyU; got {mid}"
        );
    }
}
