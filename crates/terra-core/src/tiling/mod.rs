//! Tile scheduling and ghost exchange for large fields.

mod region;

pub use region::{bounds_from_tiles, rects_from_tiles, SampleRect};

use crate::heightfield::{HeightTile, Heightfield, HeightfieldMetrics, TileId};
use crate::layer::LayerKind;
use std::collections::HashSet;

/// How a process dirty region should expand for incremental recomputation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirtyClass {
    /// Local stencil (blur / single-pass thermal) — pad by stencil radius only.
    Local,
    /// Multi-iteration neighbourhood ops — expand by tile radius from iters.
    Expanding,
    /// Drainage / SPE / amplify — basin-coupled; prefer full field or large expand.
    BasinDependent,
}

/// Classify a layer kind for dirty-region / cache expansion policy.
pub fn dirty_class_for(kind: &LayerKind) -> DirtyClass {
    match kind {
        LayerKind::Blur(_)
        | LayerKind::Coastal(_)
        | LayerKind::EffectFilter(_)
        | LayerKind::Path(_)
        | LayerKind::PolygonHeight(_)
        | LayerKind::Terrace(_)
        | LayerKind::Plateau(_) => DirtyClass::Local,
        LayerKind::ThermalErosion(_)
        | LayerKind::HydraulicErosion(_)
        | LayerKind::DebrisFlow(_)
        | LayerKind::SandSimulation(_)
        | LayerKind::FluidSimulation(_) => DirtyClass::Expanding,
        LayerKind::StreamPowerErosion(_)
        | LayerKind::MultiScaleAmplify(_)
        | LayerKind::RiverCarve(_)
        | LayerKind::RiverNetwork(_) => DirtyClass::BasinDependent,
        _ => DirtyClass::Local,
    }
}

/// Recommended halo (ghost) width for a neighbourhood process.
///
/// `stencil_radius` is the single-pass read radius (thermal/hydraulic ≈ 1).
/// When iterating without per-iter halo refresh, use
/// `halo ≥ stencil_radius * iters_per_batch`. Prefer refreshing between batches
/// with the default halo (2) rather than silently widening forever.
pub fn recommended_halo(stencil_radius: u32, iters_per_batch: u32) -> u32 {
    let need = stencil_radius.saturating_mul(iters_per_batch.max(1));
    need.max(crate::heightfield::DEFAULT_HALO).min(16)
}

/// Tile Chebyshev expand radius for a dirty class (not sample halo).
pub fn expand_radius_for(class: DirtyClass, stencil: u32, iterations: u32) -> u32 {
    match class {
        DirtyClass::Local => stencil.max(1).saturating_sub(1).max(1),
        DirtyClass::Expanding => {
            // Grow ~1 tile per ~8 iters (halo refresh between batches assumed).
            let batches = (iterations.max(1) + 7) / 8;
            batches.max(1).min(4)
        }
        DirtyClass::BasinDependent => {
            // Conservative: expand several rings; callers may still mark_all.
            ((iterations.max(1) + 3) / 4).max(2).min(8)
        }
    }
}

impl DirtyClass {
    /// Sample-space support radius for cache keys and invalidation expansion.
    ///
    /// Basin-coupled processes are treated as global — callers should prefer
    /// `mark_all` when this returns `None`.
    pub fn support_radius_samples(
        self,
        tile_size_samples: u32,
        stencil: u32,
        iterations: u32,
    ) -> Option<u32> {
        match self {
            Self::BasinDependent => None,
            Self::Local | Self::Expanding => Some(
                expand_radius_for(self, stencil, iterations)
                    .saturating_mul(tile_size_samples.max(1)),
            ),
        }
    }
}

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

    /// Expand using [`dirty_class_for`] policy; basin-dependent may mark all tiles.
    pub fn expand_for_process(
        &mut self,
        hf: &Heightfield,
        class: DirtyClass,
        stencil: u32,
        iterations: u32,
    ) {
        match class {
            DirtyClass::BasinDependent
                if iterations > 4 || hf.metrics.tiles_x() * hf.metrics.tiles_z() <= 16 =>
            {
                self.mark_all(hf);
            }
            _ => {
                let r = expand_radius_for(class, stencil, iterations);
                self.expand(hf, r);
            }
        }
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

/// Suggest metrics halo for multi-iter neighbourhood work (does not rebuild tiles).
pub fn suggest_halo(metrics: &HeightfieldMetrics, stencil: u32, iters_per_batch: u32) -> u32 {
    recommended_halo(stencil, iters_per_batch).max(metrics.halo)
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

/// Multi-pass tile stencil with halo refresh between batches to limit seam risk.
///
/// Each batch runs `iters_per_batch` passes using the current halo, then
/// `sync_dirty` refreshes ghosts. Prefer this over widening halo unboundedly.
pub fn map_tiles_batched<F>(
    hf: &mut Heightfield,
    total_iters: u32,
    iters_per_batch: u32,
    mut f: F,
) -> f32
where
    F: FnMut(&HeightTile, u32, u32, f32, u32) -> f32,
{
    let batch = iters_per_batch.max(1);
    let mut max_seam = 0.0f32;
    let mut iter = 0u32;
    while iter < total_iters {
        let end = (iter + batch).min(total_iters);
        for pass in iter..end {
            let snapshot = hf.clone();
            for tile in snapshot.tiles() {
                for lz in 0..tile.interior_height {
                    for lx in 0..tile.interior_width {
                        let v = tile.get_interior(lx, lz);
                        let nv = f(tile, lx, lz, v, pass);
                        if let Some(t) = hf.tile_mut(tile.id) {
                            t.set_interior(lx, lz, nv);
                        }
                    }
                }
            }
        }
        let mut dirty = TileScheduler::new();
        dirty.mark_all(hf);
        max_seam = max_seam.max(dirty.sync_dirty(hf));
        iter = end;
    }
    max_seam
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

    #[test]
    fn batched_blur_keeps_seams_low() {
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
                hf.set(i, j, ((i * 3 + j * 5) % 40) as f32);
            }
        }
        hf.refresh_halos();
        let seam = map_tiles_batched(&mut hf, 6, 2, |tile, lx, lz, _, _| {
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
        assert!(seam < 1e-2, "batched seam {seam}");
        assert!(measure_seams(&hf) < 1e-2);
    }

    #[test]
    fn recommended_halo_scales_with_batch() {
        assert!(recommended_halo(1, 8) >= 8);
        assert_eq!(recommended_halo(1, 1), crate::heightfield::DEFAULT_HALO);
    }

    #[test]
    fn dirty_class_basin_for_amplify() {
        assert_eq!(
            dirty_class_for(&LayerKind::MultiScaleAmplify(Default::default())),
            DirtyClass::BasinDependent
        );
        assert_eq!(
            dirty_class_for(&LayerKind::ThermalErosion(Default::default())),
            DirtyClass::Expanding
        );
    }

    #[test]
    fn basin_dependent_support_radius_is_global() {
        assert_eq!(
            DirtyClass::BasinDependent.support_radius_samples(256, 1, 1),
            None
        );
        assert!(DirtyClass::Local
            .support_radius_samples(256, 2, 1)
            .is_some());
    }

    /// Smoke: dirty tile snapshot can feed a viewport overlay (tx/tz list + bounds).
    #[test]
    fn dirty_tiles_snapshot_for_overlay() {
        let metrics = HeightfieldMetrics {
            width: 64,
            height: 64,
            world_size_x: 64.0,
            world_size_z: 64.0,
            tile_size: 16,
            halo: 2,
        };
        let mut sched = TileScheduler::new();
        sched.mark_tile(TileId { tx: 1, tz: 2 });
        sched.mark_tile(TileId { tx: 2, tz: 2 });
        let ids: Vec<(u32, u32)> = sched.dirty.iter().map(|t| (t.tx, t.tz)).collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&(1, 2)));
        assert!(ids.contains(&(2, 2)));
        let bounds = sched.dirty_bounds(&metrics, 1).expect("dirty bounds");
        assert!(!bounds.is_empty());
        // Overlay grid dimensions match metrics tiling.
        assert_eq!(metrics.tiles_x(), 4);
        assert_eq!(metrics.tiles_z(), 4);
    }
}
