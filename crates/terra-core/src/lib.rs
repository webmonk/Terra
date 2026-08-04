//! Terra core: heightfields, layer stack, masks, and CPU evaluation.
//!
//! This crate must remain free of `wgpu` and UI crates.

pub mod analyze;
pub mod command;
pub mod document;
pub mod eval;
pub mod generators;
pub mod heightfield;
pub mod hydro;
pub mod layer;
pub mod mask;
pub mod noise;
pub mod scatter;
pub mod surface;
pub mod tiling;

pub use document::TerrainDocument;
pub use heightfield::{HeightTile, Heightfield, HeightfieldMetrics, TileId};
pub use layer::{BlendMode, Layer, LayerId, LayerKind, LayerStack};
