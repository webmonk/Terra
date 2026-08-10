//! Frame graph: ordered GPU passes + dense timestamp allocation.
//!
//! Replaces ad-hoc encoder sequencing and fixed-slot query holes.

use crate::gpu_timing::GpuTimestampTimer;

/// Logical passes scheduled each frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PassKind {
    Begin,
    Shadow,
    RasterLit,
    ProgressivePt,
    ProgressivePost,
    Overlays,
    ResolveTimestamps,
}

/// Which presentation backend owns the main color/depth this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationBackendId {
    RasterLit,
    ProgressivePt,
}

impl PresentationBackendId {
    pub fn from_mode(mode: crate::ViewportRendererMode) -> Self {
        if mode.uses_progressive_path_tracer() {
            Self::ProgressivePt
        } else {
            Self::RasterLit
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::RasterLit => "RasterLit",
            Self::ProgressivePt => "ProgressivePt",
        }
    }
}

/// Planned pass list for one frame (built before recording).
#[derive(Debug, Clone, Default)]
pub struct FrameSchedule {
    pub backend: Option<PresentationBackendId>,
    pub shadow: bool,
    pub progressive_post: bool,
    pub overlays: bool,
}

impl FrameSchedule {
    pub fn for_backend(backend: PresentationBackendId, shadows_enabled: bool) -> Self {
        match backend {
            PresentationBackendId::RasterLit => Self {
                backend: Some(backend),
                shadow: shadows_enabled,
                progressive_post: false,
                overlays: true,
            },
            PresentationBackendId::ProgressivePt => Self {
                backend: Some(backend),
                shadow: false,
                progressive_post: true,
                overlays: true,
            },
        }
    }

    pub fn passes(&self) -> Vec<PassKind> {
        let mut out = vec![PassKind::Begin];
        if self.shadow {
            out.push(PassKind::Shadow);
        }
        match self.backend {
            Some(PresentationBackendId::RasterLit) => out.push(PassKind::RasterLit),
            Some(PresentationBackendId::ProgressivePt) => out.push(PassKind::ProgressivePt),
            None => {}
        }
        if self.progressive_post {
            out.push(PassKind::ProgressivePost);
        }
        if self.overlays {
            out.push(PassKind::Overlays);
        }
        out.push(PassKind::ResolveTimestamps);
        out
    }
}

/// Thin frame-graph host: schedule + timestamp resolve.
#[derive(Debug, Default)]
pub struct FrameGraph {
    pub schedule: FrameSchedule,
}

impl FrameGraph {
    pub fn begin(&mut self, schedule: FrameSchedule) {
        self.schedule = schedule;
    }

    pub fn resolve_timestamps(
        &self,
        timer: Option<&mut GpuTimestampTimer>,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        if let Some(timer) = timer {
            timer.resolve(encoder);
        }
    }
}
