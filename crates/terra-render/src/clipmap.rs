//! Camera-centered clipmap LOD rings + full-world fallback grid.
//!
//! Mesh vertex count is fixed per ring; the terrain vertex shader samples the
//! full-resolution height texture regardless of grid spacing.

use terra_core::{FieldId, LayerId, NormalizedRect, TerrainPyramid, TerrainTileKey};

/// Single world-covering displacement grid (coarsest / full fallback).
#[derive(Debug, Clone)]
pub struct WorldGridConfig {
    pub grid_size: u32,
    /// Optional vertical skirt depth in meters (0 = off). Mesh emission optional.
    pub skirt_depth: f32,
}

impl Default for WorldGridConfig {
    fn default() -> Self {
        Self::for_world(385)
    }
}

impl WorldGridConfig {
    /// Normalize to `4n + 1` vertices (legacy quarter-alignment).
    pub fn for_world(grid_size: u32) -> Self {
        let cells = grid_size.max(9).saturating_sub(1);
        let grid_size = cells.div_ceil(4).saturating_mul(4).saturating_add(1);
        Self {
            grid_size,
            skirt_depth: 0.0,
        }
    }

    /// True when optional skirts should be emitted by the mesh builder.
    pub fn skirts_enabled(&self) -> bool {
        self.skirt_depth > 1e-4
    }

    /// World metres between adjacent vertices when covering `world_extent`.
    pub fn spacing_for_extent(&self, world_extent: f32) -> f32 {
        let cells = self.grid_size.saturating_sub(1).max(1) as f32;
        world_extent / cells
    }
}

/// One nested clipmap ring: fixed vertex grid, world-space vertex spacing.
#[derive(Debug, Clone, Copy)]
pub struct ClipmapRingLevel {
    pub grid_size: u32,
    /// World metres between adjacent grid vertices.
    pub spacing: f32,
}

impl ClipmapRingLevel {
    /// World extent covered by this ring (metres).
    pub fn coverage(&self) -> f32 {
        self.spacing * self.grid_size.saturating_sub(1).max(1) as f32
    }

    /// Snap ring origin so vertices stay aligned across levels (Losasso & Hoppe).
    pub fn snap_origin(&self, camera_x: f32, camera_z: f32, world_x: f32, world_z: f32) -> (f32, f32) {
        let half = self.coverage() * 0.5;
        let snap = self.spacing.max(1e-6);
        let mut ox = ((camera_x - half) / snap).floor() * snap;
        let mut oz = ((camera_z - half) / snap).floor() * snap;
        let max_x = (world_x - self.coverage()).max(0.0);
        let max_z = (world_z - self.coverage()).max(0.0);
        ox = ox.clamp(0.0, max_x);
        oz = oz.clamp(0.0, max_z);
        (ox, oz)
    }
}

/// Nested camera-centered rings plus a coarse full-world fallback grid.
#[derive(Debug, Clone)]
pub struct ClipmapConfig {
    /// Finest → coarsest rings drawn around the camera target.
    pub rings: Vec<ClipmapRingLevel>,
    /// Full-world coarse grid when rings do not reach the horizon.
    pub fallback: WorldGridConfig,
    pub skirt_depth: f32,
}

impl Default for ClipmapConfig {
    fn default() -> Self {
        Self::for_world(4096.0, 385)
    }
}

impl ClipmapConfig {
    /// Build 4 nested rings with doubling spacing; `fallback_vertices` is the coarsest grid.
    pub fn for_world(world_extent: f32, fallback_vertices: u32) -> Self {
        let extent = world_extent.max(1.0);
        let fallback = WorldGridConfig::for_world(fallback_vertices);
        let fallback_spacing = fallback.spacing_for_extent(extent);

        // Inner ring is 8× finer than fallback spacing; each level doubles.
        let ring_grids = [65u32, 97, 129, 129];
        let mut rings = Vec::with_capacity(ring_grids.len());
        for (i, &requested) in ring_grids.iter().enumerate() {
            let grid_size = WorldGridConfig::for_world(requested).grid_size;
            let spacing = fallback_spacing / 2f32.powi(3 - i as i32);
            rings.push(ClipmapRingLevel { grid_size, spacing });
        }

        Self {
            rings,
            fallback,
            skirt_depth: 0.0,
        }
    }

    /// Recompute snapped origins for every ring from the camera-centre XZ.
    pub fn ring_origins(&self, camera_x: f32, camera_z: f32, world_x: f32, world_z: f32) -> Vec<(f32, f32)> {
        self.rings
            .iter()
            .map(|ring| ring.snap_origin(camera_x, camera_z, world_x, world_z))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResidentTileSelection {
    pub requested: TerrainTileKey,
    pub resident: Option<TerrainTileKey>,
    pub fallback_levels: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ViewportTilePlan {
    pub selections: Vec<ResidentTileSelection>,
    pub exact_tiles: usize,
    pub fallback_tiles: usize,
    pub missing_tiles: usize,
}

/// Resolve desired visible tiles to their finest resident pages, using ancestors as fallback.
pub fn plan_resident_tiles(
    pyramid: &TerrainPyramid,
    layer: Option<LayerId>,
    field: &FieldId,
    desired_level: u8,
    visible: NormalizedRect,
) -> ViewportTilePlan {
    let desired_level = desired_level.min(pyramid.max_level());
    let Some(level) = pyramid.levels.get(desired_level as usize) else {
        return ViewportTilePlan::default();
    };
    let mut plan = ViewportTilePlan::default();
    for tile in terra_core::RegionSet::from_rect(visible).tiles(&level.metrics) {
        let requested = TerrainTileKey {
            layer,
            field: field.clone(),
            level: desired_level,
            tile,
        };
        let resident = pyramid.best_resident_ancestor(layer, field, desired_level, requested.tile);
        let fallback_levels = resident
            .as_ref()
            .map_or(0, |key| desired_level.saturating_sub(key.level));
        match resident.as_ref() {
            Some(key) if key.level == desired_level => plan.exact_tiles += 1,
            Some(_) => plan.fallback_tiles += 1,
            None => plan.missing_tiles += 1,
        }
        plan.selections.push(ResidentTileSelection {
            requested,
            resident,
            fallback_levels,
        });
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_grid_uses_quarter_aligned_vertices() {
        for requested in [9, 96, 97, 98, 127, 385] {
            let cfg = WorldGridConfig::for_world(requested);
            assert_eq!((cfg.grid_size - 1) % 4, 0);
        }
    }

    #[test]
    fn default_grid_is_385() {
        assert_eq!(WorldGridConfig::default().grid_size, 385);
    }

    #[test]
    fn clipmap_builds_four_rings() {
        let cfg = ClipmapConfig::for_world(4096.0, 385);
        assert_eq!(cfg.rings.len(), 4);
        assert!(cfg.rings[0].spacing < cfg.rings[3].spacing);
        assert_eq!(cfg.fallback.grid_size, 385);
    }

    #[test]
    fn ring_origin_snaps_to_spacing_grid() {
        let ring = ClipmapRingLevel {
            grid_size: 65,
            spacing: 8.0,
        };
        let (ox, oz) = ring.snap_origin(100.3, 200.7, 4096.0, 4096.0);
        assert!((ox % 8.0).abs() < 1e-4);
        assert!((oz % 8.0).abs() < 1e-4);
    }

    #[test]
    fn resident_plan_falls_back_without_leaving_holes() {
        let mut pyramid = TerrainPyramid::new(terra_core::PyramidConfig::new(1024, 4096.0, 4096.0));
        pyramid.publish(
            TerrainTileKey {
                layer: None,
                field: FieldId::Height,
                level: 0,
                tile: terra_core::TileId { tx: 0, tz: 0 },
            },
            1,
            1,
            0,
        );
        let plan = plan_resident_tiles(
            &pyramid,
            None,
            &FieldId::Height,
            pyramid.max_level(),
            NormalizedRect::new(0.45, 0.45, 0.55, 0.55).unwrap(),
        );
        assert!(!plan.selections.is_empty());
        assert_eq!(plan.missing_tiles, 0);
        assert_eq!(plan.fallback_tiles, plan.selections.len());
        assert!(plan
            .selections
            .iter()
            .all(|selection| selection.fallback_levels > 0));
    }
}
