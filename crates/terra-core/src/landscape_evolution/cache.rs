//! Drainage topology cache for landscape evolution.

use crate::heightfield::Heightfield;
use crate::hydro::{fill_depressions, flow_accumulation_d8, flow_direction_d8};
use crate::mask::MaskField;

/// Cached drainage products shared across Fast / Accurate passes.
#[derive(Debug, Clone)]
pub struct DrainageCache {
    /// Downstream neighbour index (`usize::MAX` = outlet / sink).
    pub receiver: Vec<usize>,
    /// Topological order: downstream → upstream (outlets first).
    pub topo_down_to_up: Vec<usize>,
    /// Drainage area in cell counts (precipitation-scaled later).
    pub accumulation: Vec<f32>,
    /// Normalised direction mask for aux publish.
    pub direction_mask: MaskField,
    /// Height snapshot used to build this cache.
    pub height_fingerprint: Vec<f32>,
    pub max_abs_delta: f32,
}

impl DrainageCache {
    pub fn build(height: &Heightfield, use_fill: bool) -> Self {
        let filled = if use_fill {
            fill_depressions(height)
        } else {
            height.clone()
        };
        let w = filled.metrics.width as usize;
        let h = filled.metrics.height as usize;
        let n = w * h;
        let (dirs, direction_mask) = flow_direction_d8(&filled);
        let accumulation = flow_accumulation_d8(&filled, &dirs);

        let mut receiver = vec![usize::MAX; n];
        let mut donors = vec![Vec::new(); n];
        const OFFSETS: [(i32, i32); 8] = [
            (1, 0),
            (1, 1),
            (0, 1),
            (-1, 1),
            (-1, 0),
            (-1, -1),
            (0, -1),
            (1, -1),
        ];
        for j in 0..h {
            for i in 0..w {
                let idx = j * w + i;
                let d = dirs[idx];
                if d as usize >= OFFSETS.len() {
                    continue;
                }
                let (di, dj) = OFFSETS[d as usize];
                let ni = i as i32 + di;
                let nj = j as i32 + dj;
                if ni < 0 || nj < 0 || ni >= w as i32 || nj >= h as i32 {
                    continue;
                }
                let r = nj as usize * w + ni as usize;
                receiver[idx] = r;
                donors[r].push(idx);
            }
        }

        let mut topo_up_to_down = Vec::with_capacity(n);
        let mut stack: Vec<usize> = (0..n).filter(|&i| donors[i].is_empty()).collect();
        let mut remaining_donors: Vec<usize> = donors.iter().map(|d| d.len()).collect();
        while let Some(i) = stack.pop() {
            topo_up_to_down.push(i);
            let r = receiver[i];
            if r != usize::MAX {
                remaining_donors[r] = remaining_donors[r].saturating_sub(1);
                if remaining_donors[r] == 0 {
                    stack.push(r);
                }
            }
        }
        if topo_up_to_down.len() < n {
            let mut seen = vec![false; n];
            for &i in &topo_up_to_down {
                seen[i] = true;
            }
            for i in 0..n {
                if !seen[i] {
                    topo_up_to_down.push(i);
                }
            }
        }

        let mut topo_down_to_up = topo_up_to_down;
        topo_down_to_up.reverse();

        let height_fingerprint = height.to_dense();

        Self {
            receiver,
            topo_down_to_up,
            accumulation,
            direction_mask,
            height_fingerprint,
            max_abs_delta: 0.0,
        }
    }

    /// Whether height change since cache build warrants a topology rebuild.
    pub fn needs_rebuild(&self, height: &Heightfield, threshold_m: f32) -> bool {
        let dense = height.to_dense();
        if dense.len() != self.height_fingerprint.len() {
            return true;
        }
        let mut max_d = 0.0f32;
        for (a, b) in dense.iter().zip(self.height_fingerprint.iter()) {
            max_d = max_d.max((a - b).abs());
            if max_d > threshold_m {
                return true;
            }
        }
        false
    }

    pub fn refresh_fingerprint(&mut self, height: &Heightfield) {
        self.height_fingerprint = height.to_dense();
        self.max_abs_delta = 0.0;
    }
}
