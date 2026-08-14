//! Tiled evaluation specifications (halos + global-pass capability).

use crate::heightfield::{HeightfieldMetrics, TileId, DEFAULT_HALO, DEFAULT_TILE_SIZE};
use serde::{Deserialize, Serialize};

/// Declared tile geometry for an evaluation pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileEvalSpec {
    /// Interior tile edge length in samples.
    pub interior: u32,
    /// Overlapping halo width in samples (neighbour-sensitive reads).
    pub halo: u32,
    /// When true, evaluation needs a basin/global connectivity pass — tiles alone
    /// must not pretend to be independent.
    pub requires_global_pass: bool,
}

impl Default for TileEvalSpec {
    fn default() -> Self {
        Self {
            interior: DEFAULT_TILE_SIZE,
            halo: DEFAULT_HALO,
            requires_global_pass: false,
        }
    }
}

impl TileEvalSpec {
    pub fn local(halo: u32) -> Self {
        Self {
            interior: DEFAULT_TILE_SIZE,
            halo: halo.max(DEFAULT_HALO),
            requires_global_pass: false,
        }
    }

    pub fn basin_global(halo: u32) -> Self {
        Self {
            interior: DEFAULT_TILE_SIZE,
            halo: halo.max(DEFAULT_HALO),
            requires_global_pass: true,
        }
    }

    /// Exterior edge including halo on both sides.
    pub fn exterior(&self) -> u32 {
        self.interior.saturating_add(self.halo.saturating_mul(2))
    }

    pub fn with_metrics(self, metrics: HeightfieldMetrics) -> Self {
        Self {
            interior: metrics.tile_size.max(1),
            halo: metrics.halo.max(self.halo),
            requires_global_pass: self.requires_global_pass,
        }
    }
}

/// One tile work request produced by the scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileEvalRequest {
    pub tile: TileId,
    pub spec: TileEvalSpec,
    pub revision: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exterior_includes_halos() {
        let spec = TileEvalSpec {
            interior: 512,
            halo: 32,
            requires_global_pass: false,
        };
        assert_eq!(spec.exterior(), 576);
    }
}
