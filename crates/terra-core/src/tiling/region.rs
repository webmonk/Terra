//! Sample-space rectangles for tiled dirty regions (Wave D).

use crate::heightfield::{HeightfieldMetrics, TileId};
use serde::{Deserialize, Serialize};

/// Inclusive-exclusive sample rectangle `[x, x+w) × [y, y+h)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl SampleRect {
    pub fn from_tile(metrics: &HeightfieldMetrics, id: TileId) -> Self {
        let x = id.tx * metrics.tile_size;
        let y = id.tz * metrics.tile_size;
        let w = (metrics.width - x).min(metrics.tile_size);
        let h = (metrics.height - y).min(metrics.tile_size);
        Self { x, y, w, h }
    }

    /// Expand by `pad` samples on each side (clamped to field).
    pub fn padded(self, metrics: &HeightfieldMetrics, pad: u32) -> Self {
        let x0 = self.x.saturating_sub(pad);
        let y0 = self.y.saturating_sub(pad);
        let x1 = (self.x + self.w + pad).min(metrics.width);
        let y1 = (self.y + self.h + pad).min(metrics.height);
        Self {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        }
    }

    pub fn union(self, other: Self) -> Self {
        let x0 = self.x.min(other.x);
        let y0 = self.y.min(other.y);
        let x1 = (self.x + self.w).max(other.x + other.w);
        let y1 = (self.y + self.h).max(other.y + other.h);
        Self {
            x: x0,
            y: y0,
            w: x1 - x0,
            h: y1 - y0,
        }
    }

    pub fn is_empty(self) -> bool {
        self.w == 0 || self.h == 0
    }
}

/// Union of tile interiors (no padding).
pub fn rects_from_tiles(metrics: &HeightfieldMetrics, tiles: &[TileId]) -> Vec<SampleRect> {
    tiles
        .iter()
        .map(|id| SampleRect::from_tile(metrics, *id))
        .filter(|r| !r.is_empty())
        .collect()
}

/// Single bounding rect covering all tiles, padded for normal/seam stencils.
pub fn bounds_from_tiles(
    metrics: &HeightfieldMetrics,
    tiles: &[TileId],
    pad: u32,
) -> Option<SampleRect> {
    let mut iter = tiles.iter().map(|id| SampleRect::from_tile(metrics, *id));
    let first = iter.next()?;
    let mut acc = first;
    for r in iter {
        acc = acc.union(r);
    }
    Some(acc.padded(metrics, pad))
}
