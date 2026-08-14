//! Shared state and support utilities for terrain evaluation.
//!
//! [`crate::eval::StackEvaluator`] is the CPU execution authority. This module
//! supplies its surrounding multi-field state, derived caches, diagnostics,
//! deterministic seeds, quality modes, scale metadata, and tiling helpers.

mod derived;
mod determinism;
mod diagnostics;
mod field_set;
mod mode;
mod scale;
mod tiling;

pub use derived::{DerivedFieldCache, DerivedFieldCacheStats};
pub use determinism::{layer_seed, sample_seed, tile_seed};
pub use diagnostics::{EvalDiagnostics, OperatorId, OperatorTiming};
pub use field_set::{FieldLifetime, FieldRevision, FieldSlot, FieldStorage, TerrainFieldSet};
pub use mode::EvalMode;
pub use scale::ScaleBand;
pub use tiling::{TileEvalRequest, TileEvalSpec};
pub use crate::heightfield::{metres_to_texels, texels_to_metres, world_radius_texels};
