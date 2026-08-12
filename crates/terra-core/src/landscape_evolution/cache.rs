//! Drainage topology cache for landscape evolution.

use crate::heightfield::Heightfield;
use crate::geomorph::{
    accumulate_drainage_area, build_flow_graph, priority_flood_fill, FlowModel, Precipitation,
};
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
            priority_flood_fill(height)
        } else {
            height.clone()
        };
        let graph = build_flow_graph(&filled, FlowModel::D8);
        let n = graph.width * graph.height;
        let accumulation = accumulate_drainage_area(&graph, &Precipitation::uniform(1.0));

        // Single D8 receiver per cell (`usize::MAX` = outlet / sink).
        let mut receiver = vec![usize::MAX; n];
        for idx in 0..n {
            if let Some(r) = graph.d8_receiver_index(idx) {
                receiver[idx] = r;
            }
        }

        // The graph's topo order runs upstream → downstream; reverse it for the
        // outlets-first order the analytical / iterative solvers consume.
        let mut topo_down_to_up = graph.topo_order.clone();
        topo_down_to_up.reverse();

        let direction_mask = graph.direction_mask.clone();
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
