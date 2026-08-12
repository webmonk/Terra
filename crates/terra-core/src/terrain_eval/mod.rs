//! Multi-field terrain evaluation architecture (Phase 1).
//!
//! Artists continue to author a **layer stack**. Internally, layers compile to
//! [`EvalGraph`] operators over a shared [`TerrainFieldSet`].
//!
//! This module deliberately sits beside [`crate::eval::StackEvaluator`]: existing
//! processors remain the execution path while new multi-field operators and
//! derived caches land incrementally.

mod derived;
mod determinism;
mod diagnostics;
mod field_set;
mod graph;
mod mode;
mod operator;
mod scale;
mod tiling;
mod world_space;

pub use derived::{DerivedFieldCache, DerivedFieldCacheStats};
pub use determinism::{layer_seed, sample_seed, tile_seed};
pub use diagnostics::{EvalDiagnostics, OperatorTiming};
pub use field_set::{FieldLifetime, FieldRevision, FieldSlot, FieldStorage, TerrainFieldSet};
pub use graph::{compile_eval_graph, EvalGraph, EvalGraphNode, EvalNodeId};
pub use mode::EvalMode;
pub use operator::{
    GpuKernelId, LayerOperatorAdapter, OperatorDescriptor, OperatorId, TerrainOperator,
};
pub use scale::ScaleBand;
pub use tiling::{TileEvalRequest, TileEvalSpec};
pub use world_space::{metres_to_texels, texels_to_metres, world_radius_texels};
