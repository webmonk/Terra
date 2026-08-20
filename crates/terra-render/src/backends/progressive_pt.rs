//! Progressive path-tracer backend - typed HDR/GBuffer contract for post.

use crate::path_tracer::PathTracer;

/// HDR + depth produced by the heightfield path tracer for progressive post.
pub struct ProgressivePtOutput<'a> {
    pub radiance: &'a wgpu::TextureView,
    pub depth: &'a wgpu::TextureView,
    pub normal: &'a wgpu::TextureView,
    pub albedo: &'a wgpu::TextureView,
    pub sample_count_hint: u32,
}

impl<'a> ProgressivePtOutput<'a> {
    pub fn from_path_tracer(pt: &'a PathTracer, sample_count_hint: u32) -> Self {
        Self {
            radiance: pt.radiance_view(),
            depth: pt.depth_view(),
            normal: pt.normal_view(),
            albedo: pt.albedo_view(),
            sample_count_hint,
        }
    }
}
