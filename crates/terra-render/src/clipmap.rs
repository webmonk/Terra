//! Geometry clipmaps (Losasso & Hoppe) — nested regular grids for LOD.

use glam::Vec2;

/// Clipmap ring description for a heightfield world.
#[derive(Debug, Clone)]
pub struct ClipmapConfig {
    pub levels: u32,
    pub grid_size: u32,
    pub base_spacing: f32,
}

impl Default for ClipmapConfig {
    fn default() -> Self {
        Self {
            levels: 4,
            grid_size: 96,
            base_spacing: 2.0,
        }
    }
}

impl ClipmapConfig {
    /// Choose spacing so the coarsest level spans `world_extent` meters.
    pub fn for_world(world_extent: f32, levels: u32, grid_size: u32) -> Self {
        let levels = levels.max(1);
        let grid_size = grid_size.max(8);
        let extent = world_extent.max(1.0);
        let coarsest_cells = (grid_size - 1) as f32;
        let coarsest_span = extent;
        let coarsest_spacing = coarsest_span / coarsest_cells;
        let base_spacing = coarsest_spacing / (1u32 << (levels - 1)) as f32;
        Self {
            levels,
            grid_size,
            base_spacing: base_spacing.max(0.01),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClipmapLevel {
    pub level: u32,
    pub spacing: f32,
    pub origin: Vec2,
    pub grid_size: u32,
}

impl ClipmapConfig {
    /// Build LOD rings for a look-at `center`.
    ///
    /// The coarsest level is **world-anchored** so the full heightfield always draws.
    /// Finer levels snap to the camera so detail follows the view without sliding the
    /// entire terrain mesh when panning.
    pub fn levels_for_center(&self, center: Vec2, world_size: Vec2) -> Vec<ClipmapLevel> {
        let mut out = Vec::with_capacity(self.levels as usize);
        let cells = self.grid_size.saturating_sub(1) as f32;
        let world_extent = world_size.x.max(world_size.y).max(1.0);
        for l in 0..self.levels {
            let is_coarsest = l + 1 == self.levels;
            // Coarsest ring is world-anchored and sized to the full heightfield.
            // Finer rings follow the look-at for local detail.
            let (spacing, origin) = if is_coarsest {
                (world_extent / cells.max(1.0), Vec2::ZERO)
            } else {
                let spacing = self.base_spacing * (1 << l) as f32;
                let extent = spacing * cells;
                let origin = Vec2::new(
                    (center.x / spacing).floor() * spacing - extent * 0.5,
                    (center.y / spacing).floor() * spacing - extent * 0.5,
                );
                (spacing, origin)
            };
            out.push(ClipmapLevel {
                level: l,
                spacing,
                origin,
                grid_size: self.grid_size,
            });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_increase_spacing() {
        let cfg = ClipmapConfig::default();
        let levels = cfg.levels_for_center(Vec2::new(100.0, 100.0), Vec2::new(4096.0, 4096.0));
        assert!(levels[1].spacing > levels[0].spacing);
    }

    #[test]
    fn coarsest_covers_world_and_stays_anchored() {
        let world = 4096.0;
        let cfg = ClipmapConfig::for_world(world, 4, 96);
        let a = cfg.levels_for_center(Vec2::new(world * 0.5, world * 0.5), Vec2::splat(world));
        let b = cfg.levels_for_center(Vec2::new(3500.0, 800.0), Vec2::splat(world));
        let coarse_a = a.last().unwrap();
        let coarse_b = b.last().unwrap();
        let span = coarse_a.spacing * (coarse_a.grid_size - 1) as f32;
        assert!(span + 1.0 >= world * 0.98, "span {span} world {world}");
        assert_eq!(coarse_a.origin, Vec2::ZERO);
        assert_eq!(
            coarse_b.origin,
            Vec2::ZERO,
            "coarsest must not follow the camera"
        );
        // Fine level should still track the look-at.
        assert!(a[0].origin != b[0].origin);
    }
}
