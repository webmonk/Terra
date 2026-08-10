//! Progressive, spatial terrain-evaluation primitives.
//!
//! These types deliberately sit beside the legacy whole-field evaluator. They provide the
//! shared vocabulary for migrating Terra to layer x level x tile work without exposing a node
//! graph to artists or forcing an all-at-once engine rewrite.

mod cache;
mod executor;
mod invalidation;
mod pyramid;
mod runtime;
mod work;

pub use cache::{
    ResidentTile, TerrainCacheKey, TileCacheError, TileCacheInsert, TileCacheStats, TilePageHandle,
    TileResidencyCache,
};
pub use executor::{execute_vector_height_tile, TileExecutionError, TileExecutionOutput};
pub use invalidation::{
    DirtyEvent, FrequencyBand, InvalidationKey, InvalidationSet, NormalizedRect,
    OperationDescriptor, OperationLocality, RegionSet,
};
pub use pyramid::{PyramidConfig, TerrainLevel, TerrainPyramid, TileRecord, TileState};
pub use runtime::{TerrainRuntime, TerrainRuntimeStats};
pub use work::{
    EditorRefinementState, RefinementController, RefinementTimings, TerrainTileKey, TerrainWorkItem,
    TerrainWorkKind, TerrainWorkScheduler, WorkPriority, WorkSchedulerStats,
};
