//! wgpu UI pass: solid quads + bitmap-font glyphs + RGBA preview image.

use bytemuck::{Pod, Zeroable};

use crate::draw::DrawCmd;
use crate::font;
use crate::GuiContext;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FrameUniforms {
    screen: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    mode: f32,
    radius: f32,
    size: [f32; 2],
}

pub struct GuiRenderer {
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    uniform_buf: wgpu::Buffer,
    vertex_buf: wgpu::Buffer,
    vertex_capacity: u32,
    font_tex: wgpu::Texture,
    font_view: wgpu::TextureView,
    font_samp: wgpu::Sampler,
    font_physical_px: u32,
    font_ppp: f32,
    image_tex: wgpu::Texture,
    image_view: wgpu::TextureView,
    image_samp: wgpu::Sampler,
    image_w: u32,
    image_h: u32,
    format: wgpu::TextureFormat,
}

impl GuiRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        font::prepare(1.0);
        let (atlas, atlas_w, atlas_h, font_physical_px) = font::build_atlas_r8();
        let font_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gui-font-atlas"),
            size: wgpu::Extent3d {
                width: atlas_w,
                height: atlas_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &font_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas_w),
                rows_per_image: Some(atlas_h),
            },
            wgpu::Extent3d {
                width: atlas_w,
                height: atlas_h,
                depth_or_array_layers: 1,
            },
        );
        let font_view = font_tex.create_view(&wgpu::TextureViewDescriptor::default());
        // Linear sampling preserves fontdue coverage AA (nearest + hard cut looked chunky).
        let font_samp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("gui-font-samp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let image_w = 1u32;
        let image_h = 1u32;
        let image_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gui-image"),
            size: wgpu::Extent3d {
                width: image_w,
                height: image_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &image_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[255u8, 255, 255, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let image_view = image_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let image_samp = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("gui-image-samp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniform_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gui-frame-u"),
            size: std::mem::size_of::<FrameUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gui-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = make_bind_group(
            device,
            &bgl,
            &uniform_buf,
            &font_view,
            &font_samp,
            &image_view,
            &image_samp,
        );

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gui-ui"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/ui.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gui-pl"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gui-pipe"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x2,
                        1 => Float32x2,
                        2 => Float32x4,
                        3 => Float32,
                        4 => Float32,
                        5 => Float32x2,
                    ],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let vertex_capacity = 4096;
        let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("gui-verts"),
            size: (vertex_capacity as usize * std::mem::size_of::<Vertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            bgl,
            bind_group,
            uniform_buf,
            vertex_buf,
            vertex_capacity,
            font_tex,
            font_view,
            font_samp,
            font_physical_px,
            font_ppp: 1.0,
            image_tex,
            image_view,
            image_samp,
            image_w,
            image_h,
            format,
        }
    }

    fn sync_font_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let phys = font::physical_px();
        let ppp = font::active_ppp();
        if phys == self.font_physical_px && (self.font_ppp - ppp).abs() < 0.001 {
            return;
        }
        let (atlas, atlas_w, atlas_h, font_physical_px) = font::build_atlas_r8();
        self.font_physical_px = font_physical_px;
        self.font_ppp = ppp;
        self.font_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gui-font-atlas"),
            size: wgpu::Extent3d {
                width: atlas_w,
                height: atlas_h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.font_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &atlas,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(atlas_w),
                rows_per_image: Some(atlas_h),
            },
            wgpu::Extent3d {
                width: atlas_w,
                height: atlas_h,
                depth_or_array_layers: 1,
            },
        );
        self.font_view = self
            .font_tex
            .create_view(&wgpu::TextureViewDescriptor::default());
        self.bind_group = make_bind_group(
            device,
            &self.bgl,
            &self.uniform_buf,
            &self.font_view,
            &self.font_samp,
            &self.image_view,
            &self.image_samp,
        );
    }

    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }

    fn upload_image(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) {
        let width = width.max(1);
        let height = height.max(1);
        if width != self.image_w || height != self.image_h {
            self.image_w = width;
            self.image_h = height;
            self.image_tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("gui-image"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            self.image_view = self
                .image_tex
                .create_view(&wgpu::TextureViewDescriptor::default());
            self.bind_group = make_bind_group(
                device,
                &self.bgl,
                &self.uniform_buf,
                &self.font_view,
                &self.font_samp,
                &self.image_view,
                &self.image_samp,
            );
        }
        let expected = (width as usize) * (height as usize) * 4;
        if rgba.len() < expected {
            return;
        }
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.image_tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba[..expected],
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

    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        ctx: &mut GuiContext<'_>,
        fb_w: u32,
        fb_h: u32,
    ) {
        self.sync_font_atlas(device, queue);
        if let Some((w, h, rgba)) = ctx.image_rgba.take() {
            self.upload_image(device, queue, w, h, &rgba);
        }

        let fb_w = fb_w.max(1);
        let fb_h = fb_h.max(1);
        // Logical → framebuffer. Match glyph bake DPI so text and chrome share one grid.
        let ppp = ctx.pixels_per_point.max(0.5);
        let sx = ppp;
        let sy = ppp;

        #[derive(Clone, Copy)]
        struct Batch {
            clip: Option<crate::types::Rect>,
            first: u32,
            count: u32,
        }

        let total = ctx.draw.cmds.len() + ctx.overlay.cmds.len();
        let mut verts: Vec<Vertex> = Vec::with_capacity(total * 6);
        let mut batches: Vec<Batch> = Vec::new();
        let mut clip: Option<crate::types::Rect> = None;
        let mut batch_first = 0u32;

        let flush_batch = |batches: &mut Vec<Batch>,
                           clip: Option<crate::types::Rect>,
                           batch_first: &mut u32,
                           vert_len: u32| {
            let count = vert_len - *batch_first;
            if count > 0 {
                batches.push(Batch {
                    clip,
                    first: *batch_first,
                    count,
                });
            }
            *batch_first = vert_len;
        };

        for cmds in [&ctx.draw.cmds[..], &ctx.overlay.cmds[..]] {
            for cmd in cmds {
                match cmd {
                    DrawCmd::SetClip(next) => {
                        flush_batch(&mut batches, clip, &mut batch_first, verts.len() as u32);
                        clip = *next;
                    }
                    DrawCmd::Rect { rect, color } => {
                        let x0 = (rect.min_x * sx).round();
                        let y0 = (rect.min_y * sy).round();
                        let x1 = (rect.max_x * sx).round().max(x0 + 1.0);
                        let y1 = (rect.max_y * sy).round().max(y0 + 1.0);
                        push_quad(
                            &mut verts,
                            x0,
                            y0,
                            x1,
                            y1,
                            [0.0, 0.0, 1.0, 1.0],
                            color.to_array(),
                            0.0,
                            0.0,
                            [0.0, 0.0],
                        );
                    }
                    DrawCmd::RoundedRect {
                        rect,
                        color,
                        radius,
                    } => {
                        let x0 = (rect.min_x * sx).round();
                        let y0 = (rect.min_y * sy).round();
                        let x1 = (rect.max_x * sx).round().max(x0 + 1.0);
                        let y1 = (rect.max_y * sy).round().max(y0 + 1.0);
                        let w = (x1 - x0).max(1.0);
                        let h = (y1 - y0).max(1.0);
                        let r_px = (*radius * sx).min(w * 0.5).min(h * 0.5);
                        push_quad(
                            &mut verts,
                            x0,
                            y0,
                            x1,
                            y1,
                            [0.0, 0.0, w, h],
                            color.to_array(),
                            3.0,
                            r_px,
                            [w, h],
                        );
                    }
                    DrawCmd::Glyph {
                        x_px,
                        y_px,
                        w_px,
                        h_px,
                        color,
                        uv,
                    } => {
                        // Already snapped to the atlas pixel grid during layout.
                        let x0 = x_px.round();
                        let y0 = y_px.round();
                        let x1 = x0 + w_px.round().max(1.0);
                        let y1 = y0 + h_px.round().max(1.0);
                        push_quad(
                            &mut verts,
                            x0,
                            y0,
                            x1,
                            y1,
                            *uv,
                            color.to_array(),
                            1.0,
                            0.0,
                            [0.0, 0.0],
                        );
                    }
                    DrawCmd::Image { rect } => {
                        let x0 = (rect.min_x * sx).round();
                        let y0 = (rect.min_y * sy).round();
                        let x1 = (rect.max_x * sx).round().max(x0 + 1.0);
                        let y1 = (rect.max_y * sy).round().max(y0 + 1.0);
                        push_quad(
                            &mut verts,
                            x0,
                            y0,
                            x1,
                            y1,
                            [0.0, 0.0, 1.0, 1.0],
                            [1.0, 1.0, 1.0, 1.0],
                            2.0,
                            0.0,
                            [0.0, 0.0],
                        );
                    }
                }
            }
        }
        flush_batch(&mut batches, clip, &mut batch_first, verts.len() as u32);

        if verts.is_empty() {
            return;
        }

        if verts.len() as u32 > self.vertex_capacity {
            self.vertex_capacity = (verts.len() as u32).next_power_of_two().max(1024);
            self.vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("gui-verts"),
                size: (self.vertex_capacity as usize * std::mem::size_of::<Vertex>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }

        queue.write_buffer(&self.vertex_buf, 0, bytemuck::cast_slice(&verts));
        queue.write_buffer(
            &self.uniform_buf,
            0,
            bytemuck::bytes_of(&FrameUniforms {
                // Vertex positions are already in framebuffer pixels.
                screen: [fb_w as f32, fb_h as f32, 0.0, 0.0],
            }),
        );

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("gui-enc"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            for batch in &batches {
                if let Some(r) = batch.clip {
                    let x = (r.min_x * sx).round().max(0.0) as u32;
                    let y = (r.min_y * sy).round().max(0.0) as u32;
                    let x2 = (r.max_x * sx).round().clamp(0.0, fb_w as f32) as u32;
                    let y2 = (r.max_y * sy).round().clamp(0.0, fb_h as f32) as u32;
                    if x2 <= x || y2 <= y {
                        continue;
                    }
                    let w = x2 - x;
                    let h = y2 - y;
                    pass.set_scissor_rect(x.min(fb_w), y.min(fb_h), w.min(fb_w), h.min(fb_h));
                } else {
                    pass.set_scissor_rect(0, 0, fb_w, fb_h);
                }
                pass.draw(batch.first..(batch.first + batch.count), 0..1);
            }
        }
        queue.submit(Some(encoder.finish()));
    }
}

fn make_bind_group(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    uniform_buf: &wgpu::Buffer,
    font_view: &wgpu::TextureView,
    font_samp: &wgpu::Sampler,
    image_view: &wgpu::TextureView,
    image_samp: &wgpu::Sampler,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gui-bg"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(font_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(font_samp),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(image_view),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Sampler(image_samp),
            },
        ],
    })
}

fn push_quad(
    verts: &mut Vec<Vertex>,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    uv: [f32; 4],
    color: [f32; 4],
    mode: f32,
    radius: f32,
    size: [f32; 2],
) {
    let [u0, v0, u1, v1] = uv;
    let corners = [
        (x0, y0, u0, v0),
        (x1, y0, u1, v0),
        (x1, y1, u1, v1),
        (x0, y0, u0, v0),
        (x1, y1, u1, v1),
        (x0, y1, u0, v1),
    ];
    for (x, y, u, v) in corners {
        verts.push(Vertex {
            pos: [x, y],
            uv: [u, v],
            color,
            mode,
            radius,
            size,
        });
    }
}
