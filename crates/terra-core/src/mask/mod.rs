//! Mask fields, sources, and compositing ops.

mod bake;
mod field;
mod ops;
mod paint;

pub use bake::bake_mask_assets;
pub use field::MaskField;
pub use ops::{apply_mask_ops, MaskOp};
pub use paint::{PaintBuffer, PaintStroke};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MaskId(pub Uuid);

impl MaskId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MaskId {
    fn default() -> Self {
        Self::new()
    }
}

/// Reference from a layer to a mask with strength.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskRef {
    pub id: MaskId,
    pub strength: f32,
    pub invert: bool,
}

impl MaskRef {
    pub fn new(id: MaskId) -> Self {
        Self {
            id,
            strength: 1.0,
            invert: false,
        }
    }
}

/// Where a mask's values come from.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub enum MaskSource {
    #[default]
    None,
    Constant(f32),
    Height {
        min: f32,
        max: f32,
    },
    Slope {
        min_deg: f32,
        max_deg: f32,
    },
    Aspect {
        center_deg: f32,
        width_deg: f32,
    },
    Curvature {
        min: f32,
        max: f32,
    },
    Convexity,
    Concavity,
    AmbientOcclusion {
        radius: u32,
        strength: f32,
    },
    DistanceField {
        threshold: f32,
    },
    Noise {
        seed: u64,
        frequency: f32,
    },
    Painted {
        mask_id: MaskId,
    },
    /// Filled by simulation layers (schema-ready).
    FlowDirection,
    FlowAccumulation {
        min: f32,
        max: f32,
    },
    Wetness,
    Sediment,
    Erosion,
    Deposition,
    Named(String),
}

/// A named mask asset in the project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskAsset {
    pub id: MaskId,
    pub name: String,
    pub source: MaskSource,
    pub ops: Vec<MaskOp>,
    /// Optional editable UV-space paint data. Missing data remains compatible with old projects.
    #[serde(default)]
    pub paint: Option<PaintBuffer>,
}
