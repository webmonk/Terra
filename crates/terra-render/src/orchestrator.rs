//! Viewport orchestrator — public façade for terra-app.
//!
//! During the strangler, this is a newtype over [`TerrainRenderer`] so call sites
//! can migrate method-by-method. Frame scheduling goes through [`FrameGraph`].

use crate::frame_graph::{FrameGraph, FrameSchedule, PresentationBackendId};
use crate::render_quality::ViewportRendererMode;
use crate::TerrainRenderer;

/// App-facing viewport owner. Prefer this type over `TerrainRenderer` in new code.
pub type ViewportOrchestrator = TerrainRenderer;

/// Mode → backend (single map; no dual progressive enable flags).
pub fn backend_for_mode(mode: ViewportRendererMode) -> PresentationBackendId {
    PresentationBackendId::from_mode(mode)
}

/// Build the frame schedule for the active mode.
pub fn schedule_for_mode(mode: ViewportRendererMode, shadows: bool) -> FrameSchedule {
    FrameSchedule::for_backend(backend_for_mode(mode), shadows)
}

/// Helper used by the frame path to record the planned graph.
pub fn begin_frame_graph(graph: &mut FrameGraph, mode: ViewportRendererMode, shadows: bool) {
    graph.begin(schedule_for_mode(mode, shadows));
}
