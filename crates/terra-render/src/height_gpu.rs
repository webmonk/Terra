//! GPU-resident height + normal textures with double buffering.

use bytemuck::{Pod, Zeroable};
use terra_core::heightfield::Heightfield;
use terra_core::mask::MaskField;
use terra_core::tiling::SampleRect;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct NormalUniforms {
    width: u32,
    height: u32,
    dx: f32,
    dz: f32,
    region_x: u32,
    region_y: u32,
    region_w: u32,
    region_h: u32,
}

struct HeightSlot {
    height: wgpu::Texture,
    height_view: wgpu::TextureView,
    /// Kept alive; sampled via `normal_view`.
    _normal: wgpu::Texture,
    normal_view: wgpu::TextureView,
    width: u32,
    height_px: u32,
}

impl HeightSlot {
    fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let height_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("height-r32"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let normal_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("normal-rgba16"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING,
            view_formats: &[],
        });
        Self {
            height_view: height_tex.create_view(&wgpu::TextureViewDescriptor::default()),
            normal_view: normal_tex.create_view(&wgpu::TextureViewDescriptor::default()),
            height: height_tex,
            _normal: normal_tex,
            width,
            height_px: height,
        }
    }
}

/// Single-channel auxiliary surface map sampled by the terrain material shader.
struct AuxMap {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

impl AuxMap {
    fn new(device: &wgpu::Device, width: u32, height: u32, label: &'static str) -> Self {
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
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            _texture: texture,
            view,
            width,
            height,
        }
    }
}

/// Double-buffered GPU height/normal pair. Display index is what the viewport samples.
pub struct HeightGpu {
    slots: [HeightSlot; 2],
    pub display: usize,
    write: usize,
    sampler: wgpu::Sampler,
    normal_pipeline: wgpu::ComputePipeline,
    normal_bgl: wgpu::BindGroupLayout,
    normal_uniform: wgpu::Buffer,
    pub height_range: (f32, f32),
    pub world_size: (f32, f32),
    pub tex_size: (u32, u32),
    materials: AuxMap,
    wetness: AuxMap,
    vegetation: AuxMap,
}

impl HeightGpu {
    pub fn new(device: &wgpu::Device, initial: u32) -> Self {
        let w = initial.max(8);
        let slots = [HeightSlot::new(device, w, w), HeightSlot::new(device, w, w)];
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("height-samp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("normals"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/normals.wgsl").into()),
        });
        let normal_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("normal-bgl"),
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
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("normal-pl"),
            bind_group_layouts: &[&normal_bgl],
            push_constant_ranges: &[],
        });
        let normal_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("normal-pipe"),
            layout: Some(&pl),
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let normal_uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("normal-u"),
            size: std::mem::size_of::<NormalUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let materials = AuxMap::new(device, 1, 1, "materials-r32");
        let wetness = AuxMap::new(device, 1, 1, "wetness-r32");
        let vegetation = AuxMap::new(device, 1, 1, "vegetation-r32");

        Self {
            slots,
            display: 0,
            write: 1,
            sampler,
            normal_pipeline,
            normal_bgl,
            normal_uniform,
            height_range: (0.0, 1.0),
            world_size: (1.0, 1.0),
            tex_size: (w, w),
            materials,
            wetness,
            vegetation,
        }
    }

    fn ensure_size(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.slots[self.write].width == width && self.slots[self.write].height_px == height {
            return;
        }
        self.slots[self.write] = HeightSlot::new(device, width, height);
        if self.slots[self.display].width != width || self.slots[self.display].height_px != height {
            self.slots[self.display] = HeightSlot::new(device, width, height);
        }
        self.tex_size = (width, height);
    }

    /// Upload CPU heightfield into the write slot, compute normals, then swap for display.
    pub fn upload_and_swap(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        hf: &Heightfield,
    ) {
        self.upload_regions_and_swap(device, queue, hf, None);
    }

    /// Upload full field or only `regions` (Wave D dirty tiles), then recompute normals.
    pub fn upload_regions_and_swap(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        hf: &Heightfield,
        regions: Option<&[SampleRect]>,
    ) {
        let w = hf.metrics.width;
        let h = hf.metrics.height;
        self.ensure_size(device, w, h);
        self.world_size = (hf.metrics.world_size_x, hf.metrics.world_size_z);
        self.height_range = hf.min_max();

        let data = hf.to_dense();
        let slot = &self.slots[self.write];

        let full = SampleRect { x: 0, y: 0, w, h };
        let rects: Vec<SampleRect> = match regions {
            Some(r) if !r.is_empty() => r.to_vec(),
            _ => vec![full],
        };

        for rect in &rects {
            if rect.is_empty() {
                continue;
            }
            // Pack rows tightly for write_texture (bytes_per_row = width * 4).
            let mut packed = Vec::with_capacity((rect.w * rect.h) as usize);
            for row in rect.y..rect.y + rect.h {
                let start = (row * w + rect.x) as usize;
                packed.extend_from_slice(&data[start..start + rect.w as usize]);
            }
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &slot.height,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: rect.x,
                        y: rect.y,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&packed),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(rect.w * 4),
                    rows_per_image: Some(rect.h),
                },
                wgpu::Extent3d {
                    width: rect.w,
                    height: rect.h,
                    depth_or_array_layers: 1,
                },
            );
        }

        let normal_region = regions
            .and_then(|r| {
                let mut iter = r.iter().copied();
                let first = iter.next()?;
                Some(iter.fold(first, |a, b| a.union(b)).padded(&hf.metrics, 1))
            })
            .unwrap_or(full);

        self.compute_normals_region_and_swap(
            device,
            queue,
            w,
            h,
            hf.metrics.dx(),
            hf.metrics.dz(),
            normal_region,
        );
    }

    /// GPU→GPU path: copy a height texture into the write slot (no CPU readback).
    pub fn copy_from_texture_and_swap(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src: &wgpu::Texture,
        width: u32,
        height: u32,
        world_size: (f32, f32),
        height_range: (f32, f32),
        dx: f32,
        dz: f32,
    ) {
        self.copy_from_texture_region_and_swap(
            device,
            queue,
            src,
            width,
            height,
            world_size,
            height_range,
            dx,
            dz,
            None,
        );
    }

    pub fn copy_from_texture_region_and_swap(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        src: &wgpu::Texture,
        width: u32,
        height: u32,
        world_size: (f32, f32),
        height_range: (f32, f32),
        dx: f32,
        dz: f32,
        region: Option<SampleRect>,
    ) {
        self.ensure_size(device, width, height);
        self.world_size = world_size;
        let (lo, hi) = if height_range.0 <= height_range.1 {
            height_range
        } else {
            (height_range.1, height_range.0)
        };
        self.height_range = (lo, hi.max(lo + 1e-3));

        let full = SampleRect {
            x: 0,
            y: 0,
            w: width,
            h: height,
        };
        let rect = region.unwrap_or(full);
        let partial = rect.w != width || rect.h != height;

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("height-copy-enc"),
        });
        if partial
            && self.slots[self.display].width == width
            && self.slots[self.display].height_px == height
        {
            // Seed write slot from last display, then overlay dirty region.
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.slots[self.display].height,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &self.slots[self.write].height,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: src,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: rect.x,
                    y: rect.y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &self.slots[self.write].height,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: rect.x,
                    y: rect.y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width: rect.w,
                height: rect.h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(Some(encoder.finish()));

        let normal_rect = if partial {
            SampleRect {
                x: rect.x.saturating_sub(1),
                y: rect.y.saturating_sub(1),
                w: (rect.x + rect.w + 1).min(width) - rect.x.saturating_sub(1),
                h: (rect.y + rect.h + 1).min(height) - rect.y.saturating_sub(1),
            }
        } else {
            rect
        };

        self.compute_normals_region_and_swap(device, queue, width, height, dx, dz, normal_rect);
    }

    fn compute_normals_region_and_swap(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        w: u32,
        h: u32,
        dx: f32,
        dz: f32,
        region: SampleRect,
    ) {
        let uniforms = NormalUniforms {
            width: w,
            height: h,
            dx,
            dz,
            region_x: region.x,
            region_y: region.y,
            region_w: region.w.max(1),
            region_h: region.h.max(1),
        };
        queue.write_buffer(&self.normal_uniform, 0, bytemuck::bytes_of(&uniforms));

        let slot = &self.slots[self.write];
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("normal-bg"),
            layout: &self.normal_bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.normal_uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&slot.height_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&slot.normal_view),
                },
            ],
        });

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("normal-enc"),
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("normals"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.normal_pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups((region.w + 7) / 8, (region.h + 7) / 8, 1);
        }
        queue.submit(Some(encoder.finish()));
        std::mem::swap(&mut self.display, &mut self.write);
    }

    pub fn display_height_view(&self) -> &wgpu::TextureView {
        &self.slots[self.display].height_view
    }

    pub fn display_normal_view(&self) -> &wgpu::TextureView {
        &self.slots[self.display].normal_view
    }

    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    /// Upload surface maps emitted by CPU or hybrid evaluation. Missing maps are
    /// replaced with a 1×1 zero map so old evaluation results cannot leak through.
    pub fn upload_aux_maps(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        materials: Option<&MaskField>,
        wetness: Option<&MaskField>,
        vegetation: Option<&MaskField>,
    ) {
        Self::upload_aux_map(
            device,
            queue,
            &mut self.materials,
            materials,
            "materials-r32",
        );
        Self::upload_aux_map(device, queue, &mut self.wetness, wetness, "wetness-r32");
        Self::upload_aux_map(
            device,
            queue,
            &mut self.vegetation,
            vegetation,
            "vegetation-r32",
        );
    }

    fn upload_aux_map(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        target: &mut AuxMap,
        source: Option<&MaskField>,
        label: &'static str,
    ) {
        let (width, height, data): (u32, u32, &[f32]) = match source {
            Some(map) => (map.metrics.width, map.metrics.height, map.data()),
            None => (1, 1, &[0.0]),
        };
        if target.width != width || target.height != height {
            *target = AuxMap::new(device, width, height, label);
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &target._texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytemuck::cast_slice(data),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    pub fn materials_view(&self) -> &wgpu::TextureView {
        &self.materials.view
    }

    pub fn wetness_view(&self) -> &wgpu::TextureView {
        &self.wetness.view
    }

    pub fn vegetation_view(&self) -> &wgpu::TextureView {
        &self.vegetation.view
    }
}
