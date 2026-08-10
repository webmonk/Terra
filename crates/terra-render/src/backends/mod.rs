//! Presentation backends — RasterLit and ProgressivePt.
//!
//! Mode maps to exactly one backend via [`PresentationBackendId`].

pub mod progressive_post;
pub mod progressive_pt;
pub mod raster_lit;

pub use crate::frame_graph::PresentationBackendId;
pub use progressive_post::{GBufferViews, HdrFrame, ProgressivePostPipeline};
pub use progressive_pt::ProgressivePtOutput;
pub use raster_lit::{plan_raster_present, RasterLitDrawParams};
