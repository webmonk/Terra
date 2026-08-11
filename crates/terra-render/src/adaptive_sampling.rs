//! CPU-side adaptive sampling tile state (Phase 11 stub).

use crate::render_quality::RenderQualityConfig;

/// Workgroup / tile size matching the path-tracer dispatch grid.
pub const TILE_SIZE: u32 = 8;

/// Per-tile sampling priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum TileState {
    #[default]
    Active = 0,
    Reduced = 1,
    Converged = 2,
}

impl TileState {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Reduced,
            2 => Self::Converged,
            _ => Self::Active,
        }
    }

    pub fn ray_weight(self) -> f32 {
        match self {
            Self::Active => 1.0,
            Self::Reduced => 0.5,
            Self::Converged => 0.0,
        }
    }
}

/// Summary produced by future GPU variance reduction passes.
#[derive(Debug, Clone, Copy, Default)]
pub struct VarianceTileSummary {
    pub tile_x: u32,
    pub tile_y: u32,
    pub mean_luminance: f32,
    pub variance: f32,
    pub sample_count: f32,
}

/// Tracks which screen tiles still need path-tracing samples.
#[derive(Debug, Clone)]
pub struct AdaptiveSamplingState {
    pub tile_size: u32,
    pub tiles_x: u32,
    pub tiles_y: u32,
    pub tile_states: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl AdaptiveSamplingState {
    pub fn new(width: u32, height: u32) -> Self {
        let tile_size = TILE_SIZE;
        let tiles_x = width.div_ceil(tile_size);
        let tiles_y = height.div_ceil(tile_size);
        let count = (tiles_x * tiles_y) as usize;
        Self {
            tile_size,
            tiles_x,
            tiles_y,
            tile_states: vec![TileState::Active as u8; count],
            width,
            height,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        *self = Self::new(width, height);
    }

    pub fn tile_count(&self) -> u32 {
        self.tiles_x * self.tiles_y
    }

    pub fn state_at(&self, tile_x: u32, tile_y: u32) -> TileState {
        if tile_x >= self.tiles_x || tile_y >= self.tiles_y {
            return TileState::Converged;
        }
        let idx = (tile_y * self.tiles_x + tile_x) as usize;
        TileState::from_u8(self.tile_states[idx])
    }

    pub fn set_state(&mut self, tile_x: u32, tile_y: u32, state: TileState) {
        if tile_x >= self.tiles_x || tile_y >= self.tiles_y {
            return;
        }
        let idx = (tile_y * self.tiles_x + tile_x) as usize;
        self.tile_states[idx] = state as u8;
    }

    pub fn reactivate_all(&mut self) {
        self.tile_states.fill(TileState::Active as u8);
    }

    /// Mark the lowest-variance fraction converged once tiles exceed `min_samples`.
    pub fn mark_converged_fraction(
        &mut self,
        summaries: &[VarianceTileSummary],
        config: &RenderQualityConfig,
    ) {
        let min_samples = config.min_samples_before_converge.max(1) as f32;
        let target_fraction = config.converge_fraction.clamp(0.0, 1.0);
        let mut candidates: Vec<(f32, u32, u32)> = summaries
            .iter()
            .filter(|s| s.sample_count >= min_samples && s.variance.is_finite())
            .map(|s| (s.variance, s.tile_x, s.tile_y))
            .collect();
        if candidates.is_empty() {
            return;
        }
        candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let converged_count = ((candidates.len() as f32) * target_fraction).floor() as usize;
        for (_, tx, ty) in candidates.into_iter().take(converged_count) {
            self.set_state(tx, ty, TileState::Converged);
        }
    }

    /// Full-screen R8 mask — 255 = active, 128 = reduced, 0 = converged (Phase 11).
    pub fn prepare_sampling_mask(&self) -> Vec<u8> {
        let mut mask = vec![255u8; (self.width * self.height) as usize];
        for ty in 0..self.tiles_y {
            for tx in 0..self.tiles_x {
                let state = self.state_at(tx, ty);
                let value = match state {
                    TileState::Active => 255u8,
                    TileState::Reduced => 128u8,
                    TileState::Converged => 0u8,
                };
                let x0 = tx * self.tile_size;
                let y0 = ty * self.tile_size;
                let x1 = (x0 + self.tile_size).min(self.width);
                let y1 = (y0 + self.tile_size).min(self.height);
                for y in y0..y1 {
                    for x in x0..x1 {
                        mask[(y * self.width + x) as usize] = value;
                    }
                }
            }
        }
        mask
    }

    /// Phase 11 bootstrap — every pixel active until variance feedback arrives.
    pub fn prepare_all_active_mask(&self) -> Vec<u8> {
        vec![255u8; (self.width * self.height) as usize]
    }

    pub fn count_by_state(&self) -> (u32, u32, u32) {
        let mut active = 0u32;
        let mut reduced = 0u32;
        let mut converged = 0u32;
        for &raw in &self.tile_states {
            match TileState::from_u8(raw) {
                TileState::Active => active += 1,
                TileState::Reduced => reduced += 1,
                TileState::Converged => converged += 1,
            }
        }
        (active, reduced, converged)
    }

    /// Split a per-frame ray budget across non-converged tiles.
    pub fn distribute_rays_budget(&self, total_rays: u64, base_spp: u32) -> Vec<u32> {
        let mut weights = Vec::with_capacity(self.tile_states.len());
        let mut weight_sum = 0.0f64;
        for &raw in &self.tile_states {
            let w = TileState::from_u8(raw).ray_weight() as f64;
            weights.push(w);
            weight_sum += w;
        }
        if weight_sum <= 0.0 {
            return vec![0; self.tile_states.len()];
        }
        let pixels_per_tile = (self.tile_size * self.tile_size) as f64;
        weights
            .into_iter()
            .map(|w| {
                if w <= 0.0 {
                    return 0;
                }
                let share = (total_rays as f64) * (w / weight_sum);
                let spp = (share / pixels_per_tile).round() as u32;
                spp.max(base_spp.min(1)).min(base_spp.max(1))
            })
            .collect()
    }

    pub fn update_from_variance_summaries(
        &mut self,
        summaries: &[VarianceTileSummary],
        config: &RenderQualityConfig,
    ) {
        for summary in summaries {
            if summary.sample_count < config.min_samples_before_converge as f32 {
                self.set_state(summary.tile_x, summary.tile_y, TileState::Active);
                continue;
            }
            if summary.variance > config.direct_luminance_clamp * 0.01 {
                self.set_state(summary.tile_x, summary.tile_y, TileState::Active);
            } else if summary.variance > config.indirect_luminance_clamp * 0.005 {
                self.set_state(summary.tile_x, summary.tile_y, TileState::Reduced);
            } else {
                self.set_state(summary.tile_x, summary.tile_y, TileState::Converged);
            }
        }
        self.mark_converged_fraction(summaries, config);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_active_mask_is_full() {
        let state = AdaptiveSamplingState::new(64, 64);
        let mask = state.prepare_all_active_mask();
        assert_eq!(mask.len(), 64 * 64);
        assert!(mask.iter().all(|&v| v == 255));
    }

    #[test]
    fn reactivate_all_resets_tiles() {
        let mut state = AdaptiveSamplingState::new(16, 16);
        state.set_state(0, 0, TileState::Converged);
        state.reactivate_all();
        assert_eq!(state.state_at(0, 0), TileState::Active);
    }
}
