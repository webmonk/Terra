//! Tile scheduling and ghost exchange for large fields.

mod region;

pub use region::{bounds_from_tiles, rects_from_tiles, SampleRect};

use crate::heightfield::{HeightTile, Heightfield, TileId};
use std::collections::HashSet;

/// Process tiles with neighbor halo refresh between passes.
pub struct TileScheduler {
    pub dirty: Vec<TileId>,
}

impl TileScheduler {
    pub fn new() -> Self {
        Self { dirty: Vec::new() }
    }

    pub fn clear(&mut self) {
        self.dirty.clear();
    }

    pub fn mark_all(&mut self, hf: &Heightfield) {
        self.dirty = hf.tiles().iter().map(|t| t.id).collect();
    }

    pub fn mark_tile(&mut self, id: TileId) {
        if !self.dirty.contains(&id) {
            self.dirty.push(id);
        }
    }

    pub fn mark_tile_and_neighbors(&mut self, hf: &Heightfield, id: TileId) {
        let tx_max = hf.metrics.tiles_x();
        let tz_max = hf.metrics.tiles_z();
        for dj in -1i32..=1 {
            for di in -1i32..=1 {
                let tx = id.tx as i32 + di;
                let tz = id.tz as i32 + dj;
                if tx >= 0 && tz >= 0 && tx < tx_max as i32 && tz < tz_max as i32 {
                    self.mark_tile(TileId {
                        tx: tx as u32,
                        tz: tz as u32,
                    });
                }
            }
        }
    }

    /// Grow the dirty set by Chebyshev radius (for multi-iteration stencils).
    pub fn expand(&mut self, hf: &Heightfield, radius: u32) {
        if radius == 0 || self.dirty.is_empty() {
            return;
        }
        let tx_max = hf.metrics.tiles_x() as i32;
        let tz_max = hf.metrics.tiles_z() as i32;
        let r = radius as i32;
        let mut set: HashSet<TileId> = self.dirty.iter().copied().collect();
        let seed: Vec<TileId> = self.dirty.clone();
        for id in seed {
            for dj in -r..=r {
                for di in -r..=r {
                    let tx = id.tx as i32 + di;
                    let tz = id.tz as i32 + dj;
                    if tx >= 0 && tz >= 0 && tx < tx_max && tz < tz_max {
                        set.insert(TileId {
                            tx: tx as u32,
                            tz: tz as u32,
                        });
                    }
                }
            }
        }
        self.dirty = set.into_iter().collect();
    }

    /// Incremental ghost exchange for dirty tiles; returns max seam error on dirty edges.
    pub fn sync_dirty(&self, hf: &mut Heightfield) -> f32 {
        hf.refresh_halos_for(&self.dirty);
        measure_seams_among(hf, &self.dirty)
    }

    /// Ensure halos match neighbors; returns max absolute discontinuity across shared edges.
    pub fn sync_and_measure_seams(&self, hf: &mut Heightfield) -> f32 {
        hf.refresh_halos();
        measure_seams(hf)
    }

    pub fn dirty_bounds(
        &self,
        metrics: &crate::heightfield::HeightfieldMetrics,
        pad: u32,
    ) -> Option<SampleRect> {
        bounds_from_tiles(metrics, &self.dirty, pad)
    }
}

impl Default for TileScheduler {
    fn default() -> Self {
        Self::new()
    }
}

/// Max |halo − neighbor interior| across shared tile edges (0 after `refresh_halos`).
pub fn measure_seams(hf: &Heightfield) -> f32 {
    let mut max_err = 0.0f32;
    for tz in 0..hf.metrics.tiles_z() {
        for tx in 0..hf.metrics.tiles_x().saturating_sub(1) {
            max_err = max_err.max(edge_seam_x(hf, tx, tz));
        }
    }
    for tz in 0..hf.metrics.tiles_z().saturating_sub(1) {
        for tx in 0..hf.metrics.tiles_x() {
            max_err = max_err.max(edge_seam_z(hf, tx, tz));
        }
    }
    max_err
}

/// Seam metric restricted to edges touching any tile in `tiles`.
pub fn measure_seams_among(hf: &Heightfield, tiles: &[TileId]) -> f32 {
    let set: HashSet<TileId> = tiles.iter().copied().collect();
    let mut max_err = 0.0f32;
    for &id in tiles {
        if id.tx + 1 < hf.metrics.tiles_x() {
            let right = TileId {
                tx: id.tx + 1,
                tz: id.tz,
            };
            if set.contains(&right) || set.contains(&id) {
                max_err = max_err.max(edge_seam_x(hf, id.tx, id.tz));
            }
        }
        if id.tz + 1 < hf.metrics.tiles_z() {
            let below = TileId {
                tx: id.tx,
                tz: id.tz + 1,
            };
            if set.contains(&below) || set.contains(&id) {
                max_err = max_err.max(edge_seam_z(hf, id.tx, id.tz));
            }
        }
    }
    max_err
}

fn edge_seam_x(hf: &Heightfield, tx: u32, tz: u32) -> f32 {
    let left = hf.tile(TileId { tx, tz }).unwrap();
    let right = hf.tile(TileId { tx: tx + 1, tz }).unwrap();
    let x_left = left.interior_width - 1;
    let mut max_err = 0.0f32;
    for lz in 0..left.interior_height.min(right.interior_height) {
        let a = left.get_interior(x_left, lz);
        let b = right.get_with_halo(-1, lz as i32);
        max_err = max_err.max((a - b).abs());
    }
    max_err
}

fn edge_seam_z(hf: &Heightfield, tx: u32, tz: u32) -> f32 {
    let top = hf.tile(TileId { tx, tz }).unwrap();
    let bot = hf.tile(TileId { tx, tz: tz + 1 }).unwrap();
    let z_top = top.interior_height - 1;
    let mut max_err = 0.0f32;
    for lx in 0..top.interior_width.min(bot.interior_width) {
        let a = top.get_interior(lx, z_top);
        let b = bot.get_with_halo(lx as i32, -1);
        max_err = max_err.max((a - b).abs());
    }
    max_err
}

/// Apply a tile-local stencil that reads halo, writing only interior.
pub fn map_tiles<F>(hf: &mut Heightfield, mut f: F)
where
    F: FnMut(&HeightTile, u32, u32, f32) -> f32,
{
    let snapshot = hf.clone();
    let mut dirty = TileScheduler::new();
    for tile in snapshot.tiles() {
        dirty.mark_tile(tile.id);
        for lz in 0..tile.interior_height {
            for lx in 0..tile.interior_width {
                let v = tile.get_interior(lx, lz);
                let nv = f(tile, lx, lz, v);
                if let Some(t) = hf.tile_mut(tile.id) {
                    t.set_interior(lx, lz, nv);
                }
            }
        }
    }
    dirty.expand(hf, 1);
    dirty.sync_dirty(hf);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heightfield::HeightfieldMetrics;

    #[test]
    fn tiled_blur_matches_dense_path() {
        let metrics = HeightfieldMetrics {
            width: 64,
            height: 64,
            world_size_x: 64.0,
            world_size_z: 64.0,
            tile_size: 32,
            halo: 2,
        };
        let mut hf = Heightfield::zeros(metrics);
        for j in 0..64 {
            for i in 0..64 {
                hf.set(i, j, ((i * 13 + j * 7) % 50) as f32);
            }
        }
        hf.refresh_halos();
        let before = measure_seams(&hf);
        assert!(before < 1e-3);

        map_tiles(&mut hf, |tile, lx, lz, _| {
            let mut sum = 0.0;
            let mut c = 0.0;
            for dj in -1..=1 {
                for di in -1..=1 {
                    sum += tile.get_with_halo(lx as i32 + di, lz as i32 + dj);
                    c += 1.0;
                }
            }
            sum / c
        });
        let err = measure_seams(&hf);
        assert!(err < 1e-2, "seam err {err}");
    }

    #[test]
    fn incremental_halo_matches_full() {
        let metrics = HeightfieldMetrics {
            width: 96,
            height: 96,
            world_size_x: 96.0,
            world_size_z: 96.0,
            tile_size: 32,
            halo: 2,
        };
        let mut a = Heightfield::zeros(metrics);
        let mut b = Heightfield::zeros(metrics);
        for j in 0..96 {
            for i in 0..96 {
                let v = (i + j) as f32 * 0.25;
                a.set(i, j, v);
                b.set(i, j, v);
            }
        }
        a.refresh_halos();
        let mut sched = TileScheduler::new();
        sched.mark_tile_and_neighbors(&b, TileId { tx: 1, tz: 1 });
        sched.sync_dirty(&mut b);
        // Compare halo of center tile
        let ta = a.tile(TileId { tx: 1, tz: 1 }).unwrap();
        let tb = b.tile(TileId { tx: 1, tz: 1 }).unwrap();
        for gz in -2i32..34 {
            for gx in -2i32..34 {
                assert!(
                    (ta.get_with_halo(gx, gz) - tb.get_with_halo(gx, gz)).abs() < 1e-5,
                    "mismatch at {gx},{gz}"
                );
            }
        }
    }

    #[test]
    fn expand_grows_chebyshev() {
        let metrics = HeightfieldMetrics {
            width: 128,
            height: 128,
            world_size_x: 128.0,
            world_size_z: 128.0,
            tile_size: 32,
            halo: 2,
        };
        let hf = Heightfield::zeros(metrics);
        let mut sched = TileScheduler::new();
        sched.mark_tile(TileId { tx: 2, tz: 2 });
        sched.expand(&hf, 1);
        assert_eq!(sched.dirty.len(), 9);
    }
}
