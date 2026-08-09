//! Flow routing, watersheds, river carving, and stream-power incision.
//!
//! Phase 2 geomorphology primitives (depression handling, FlowGraph, Strahler
//! streams, multi-scale derivatives) live in [`crate::geomorph`]. This module
//! keeps the SPE / river-carve oracles and the historical free functions used by
//! processors.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, VecDeque};

use crate::fields::erodibility_at_strata_depth;
use crate::heightfield::Heightfield;
use crate::layer::{RiverCarveParams, Stratum, StreamPowerParams};
use crate::mask::MaskField;

/// Sentinel used by D8 routing for a cell without a downhill neighbor.
pub const NO_FLOW: u8 = u8::MAX;

#[derive(Clone, Copy)]
struct FloodCell {
    height: f32,
    index: usize,
}

impl PartialEq for FloodCell {
    fn eq(&self, other: &Self) -> bool {
        self.index == other.index && self.height.to_bits() == other.height.to_bits()
    }
}

impl Eq for FloodCell {}

impl PartialOrd for FloodCell {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FloodCell {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse the comparison so BinaryHeap acts as a min-heap by elevation.
        other
            .height
            .total_cmp(&self.height)
            .then_with(|| other.index.cmp(&self.index))
    }
}

#[derive(Clone, Copy)]
pub struct FlowDir {
    di: i32,
    dj: i32,
    fraction: f32,
}

/// Fill enclosed depressions to their lowest spill elevation using Priority-Flood.
///
/// Boundary cells are preserved as drainage outlets; every interior pit is raised
/// only enough to connect to one of those outlets.
pub fn fill_depressions(hf: &Heightfield) -> Heightfield {
    let w = hf.metrics.width as usize;
    let h = hf.metrics.height as usize;
    if w == 0 || h == 0 {
        return hf.clone();
    }

    let mut out = hf.clone();
    let mut visited = vec![false; w * h];
    let mut queue = BinaryHeap::new();

    for j in 0..h {
        for i in 0..w {
            if i != 0 && j != 0 && i + 1 != w && j + 1 != h {
                continue;
            }
            let index = j * w + i;
            if visited[index] {
                continue;
            }
            visited[index] = true;
            queue.push(FloodCell {
                height: hf.get(i as u32, j as u32),
                index,
            });
        }
    }

    while let Some(cell) = queue.pop() {
        let i = cell.index % w;
        let j = cell.index / w;
        for dj in -1i32..=1 {
            for di in -1i32..=1 {
                if di == 0 && dj == 0 {
                    continue;
                }
                let ni = i as i32 + di;
                let nj = j as i32 + dj;
                if ni < 0 || nj < 0 || ni >= w as i32 || nj >= h as i32 {
                    continue;
                }
                let nindex = nj as usize * w + ni as usize;
                if visited[nindex] {
                    continue;
                }

                visited[nindex] = true;
                let filled_height = hf.get(ni as u32, nj as u32).max(cell.height);
                out.set(ni as u32, nj as u32, filled_height);
                queue.push(FloodCell {
                    height: filled_height,
                    index: nindex,
                });
            }
        }
    }

    out
}

/// D8 flow direction (steepest descent). No downhill neighbor is `NO_FLOW`.
pub fn flow_direction_d8(hf: &Heightfield) -> (Vec<u8>, MaskField) {
    let w = hf.metrics.width as usize;
    let h = hf.metrics.height as usize;
    let mut dirs = vec![0u8; w * h];
    let mut mask = MaskField::zeros(hf.metrics);
    let offsets: [(i32, i32); 8] = [
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
            let h0 = hf.get(i as u32, j as u32);
            let mut best = NO_FLOW;
            let mut best_s = 0.0f32;
            for (k, &(di, dj)) in offsets.iter().enumerate() {
                let ni = i as i32 + di;
                let nj = j as i32 + dj;
                if ni < 0 || nj < 0 || ni >= w as i32 || nj >= h as i32 {
                    continue;
                }
                let dist = if di != 0 && dj != 0 {
                    std::f32::consts::SQRT_2
                } else {
                    1.0
                };
                let slope = (h0 - hf.get(ni as u32, nj as u32)) / dist;
                if slope > best_s {
                    best_s = slope;
                    best = k as u8;
                }
            }
            dirs[j * w + i] = best;
            mask.set(
                i as u32,
                j as u32,
                if best == NO_FLOW {
                    -1.0
                } else {
                    best as f32 / 7.0
                },
            );
        }
    }
    (dirs, mask)
}

/// D∞ (Tarboton): partition flow between two steepest downhill neighbors.
pub fn flow_direction_dinfinity(hf: &Heightfield) -> Vec<Vec<FlowDir>> {
    let w = hf.metrics.width as usize;
    let h = hf.metrics.height as usize;
    let mut out = vec![Vec::new(); w * h];
    let offsets: [(i32, i32); 8] = [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ];
    let dx = hf.metrics.dx();
    let dz = hf.metrics.dz();

    for j in 0..h {
        for i in 0..w {
            let h0 = hf.get(i as u32, j as u32);
            let mut best_slope = 0.0f32;
            let mut best_dirs = Vec::new();

            // Tarboton D-infinity evaluates the plane on each triangular facet
            // formed by adjacent D8 neighbours. The downslope vector is routed
            // only to the two cells bounding the winning facet.
            for facet in 0..8 {
                let (di1, dj1) = offsets[facet];
                let (di2, dj2) = offsets[(facet + 1) % 8];
                let ni1 = i as i32 + di1;
                let nj1 = j as i32 + dj1;
                let ni2 = i as i32 + di2;
                let nj2 = j as i32 + dj2;
                if ni1 < 0
                    || nj1 < 0
                    || ni2 < 0
                    || nj2 < 0
                    || ni1 >= w as i32
                    || nj1 >= h as i32
                    || ni2 >= w as i32
                    || nj2 >= h as i32
                {
                    continue;
                }

                let v1 = (di1 as f32 * dx, dj1 as f32 * dz);
                let v2 = (di2 as f32 * dx, dj2 as f32 * dz);
                let z1 = hf.get(ni1 as u32, nj1 as u32) - h0;
                let z2 = hf.get(ni2 as u32, nj2 as u32) - h0;
                let drop1 = -z1;
                let drop2 = -z2;
                let det = v1.0 * v2.1 - v1.1 * v2.0;
                if det.abs() <= f32::EPSILON {
                    continue;
                }

                let gx = (z1 * v2.1 - z2 * v1.1) / det;
                let gz = (v1.0 * z2 - v2.0 * z1) / det;
                let downhill = (-gx, -gz);
                let c1 = (downhill.0 * v2.1 - downhill.1 * v2.0) / det;
                let c2 = (v1.0 * downhill.1 - v1.1 * downhill.0) / det;
                let mut facet_dirs = Vec::new();
                let mut facet_slope = 0.0;

                if c1 >= -1e-6 && c2 >= -1e-6 {
                    let mut weight1 = c1.max(0.0) * v1.0.hypot(v1.1);
                    let mut weight2 = c2.max(0.0) * v2.0.hypot(v2.1);
                    if drop1 <= 0.0 {
                        weight1 = 0.0;
                    }
                    if drop2 <= 0.0 {
                        weight2 = 0.0;
                    }
                    let total = weight1 + weight2;
                    if total > f32::EPSILON {
                        facet_slope = downhill.0.hypot(downhill.1);
                        if weight1 > 0.0 {
                            facet_dirs.push(FlowDir {
                                di: di1,
                                dj: dj1,
                                fraction: weight1 / total,
                            });
                        }
                        if weight2 > 0.0 {
                            facet_dirs.push(FlowDir {
                                di: di2,
                                dj: dj2,
                                fraction: weight2 / total,
                            });
                        }
                    }
                }

                // If the facet gradient falls outside its angular wedge, clamp
                // to the steeper bounding edge, as in the Tarboton construction.
                if facet_dirs.is_empty() {
                    let slope1 = (drop1 / v1.0.hypot(v1.1)).max(0.0);
                    let slope2 = (drop2 / v2.0.hypot(v2.1)).max(0.0);
                    if slope1 > 0.0 || slope2 > 0.0 {
                        if slope1 >= slope2 {
                            facet_slope = slope1;
                            facet_dirs.push(FlowDir {
                                di: di1,
                                dj: dj1,
                                fraction: 1.0,
                            });
                        } else {
                            facet_slope = slope2;
                            facet_dirs.push(FlowDir {
                                di: di2,
                                dj: dj2,
                                fraction: 1.0,
                            });
                        }
                    }
                }

                if facet_slope > best_slope {
                    best_slope = facet_slope;
                    best_dirs = facet_dirs;
                }
            }
            out[j * w + i] = best_dirs;
        }
    }
    out
}

pub fn flow_accumulation_d8(hf: &Heightfield, dirs: &[u8]) -> Vec<f32> {
    let w = hf.metrics.width as usize;
    let h = hf.metrics.height as usize;
    let mut acc = vec![1.0f32; w * h];
    let offsets: [(i32, i32); 8] = [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ];
    // Process cells high→low
    let mut order: Vec<(u32, u32, f32)> = Vec::with_capacity(w * h);
    for j in 0..h as u32 {
        for i in 0..w as u32 {
            order.push((i, j, hf.get(i, j)));
        }
    }
    order.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
    for &(i, j, _) in &order {
        let idx = j as usize * w + i as usize;
        let d = dirs[idx] as usize;
        if d >= offsets.len() {
            continue;
        }
        let (di, dj) = offsets[d];
        let ni = i as i32 + di;
        let nj = j as i32 + dj;
        if ni >= 0 && nj >= 0 && ni < w as i32 && nj < h as i32 {
            let nidx = nj as usize * w + ni as usize;
            acc[nidx] += acc[idx];
        }
    }
    acc
}

pub fn flow_accumulation_dinfinity(hf: &Heightfield, dirs: &[Vec<FlowDir>]) -> Vec<f32> {
    let w = hf.metrics.width as usize;
    let h = hf.metrics.height as usize;
    let mut acc = vec![1.0f32; w * h];
    let mut order: Vec<(u32, u32, f32)> = Vec::with_capacity(w * h);
    for j in 0..h as u32 {
        for i in 0..w as u32 {
            order.push((i, j, hf.get(i, j)));
        }
    }
    order.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
    for &(i, j, _) in &order {
        let idx = j as usize * w + i as usize;
        for fd in &dirs[idx] {
            let ni = i as i32 + fd.di;
            let nj = j as i32 + fd.dj;
            if ni >= 0 && nj >= 0 && ni < w as i32 && nj < h as i32 {
                let nidx = nj as usize * w + ni as usize;
                acc[nidx] += acc[idx] * fd.fraction;
            }
        }
    }
    acc
}

/// Simple watershed labels by following flow to sink.
pub fn watersheds(hf: &Heightfield, dirs: &[u8]) -> Vec<u32> {
    let w = hf.metrics.width as usize;
    let h = hf.metrics.height as usize;
    let offsets: [(i32, i32); 8] = [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ];
    let mut labels = vec![0u32; w * h];
    let mut next_label = 1u32;

    for j in 0..h {
        for i in 0..w {
            let mut path = Vec::new();
            let mut ci = i as i32;
            let mut cj = j as i32;
            let mut guard = 0;
            loop {
                let idx = cj as usize * w + ci as usize;
                if labels[idx] != 0 {
                    let lab = labels[idx];
                    for (pi, pj) in path {
                        labels[pj as usize * w + pi as usize] = lab;
                    }
                    break;
                }
                path.push((ci, cj));
                let direction = dirs[idx] as usize;
                if direction >= offsets.len() {
                    let lab = next_label;
                    next_label += 1;
                    for (pi, pj) in path {
                        labels[pj as usize * w + pi as usize] = lab;
                    }
                    break;
                }
                let (di, dj) = offsets[direction];
                let ni = ci + di;
                let nj = cj + dj;
                if ni < 0 || nj < 0 || ni >= w as i32 || nj >= h as i32 || guard > w * h {
                    let lab = next_label;
                    next_label += 1;
                    for (pi, pj) in path {
                        labels[pj as usize * w + pi as usize] = lab;
                    }
                    break;
                }
                // Local sink
                if hf.get(ni as u32, nj as u32) >= hf.get(ci as u32, cj as u32) {
                    let lab = next_label;
                    next_label += 1;
                    for (pi, pj) in path {
                        labels[pj as usize * w + pi as usize] = lab;
                    }
                    break;
                }
                ci = ni;
                cj = nj;
                guard += 1;
            }
        }
    }
    labels
}

/// Strahler-like stream order from accumulation threshold.
pub fn stream_order(acc: &[f32], w: usize, h: usize, threshold: f32) -> Vec<u32> {
    let mut order = vec![0u32; w * h];
    for j in 0..h {
        for i in 0..w {
            let idx = j * w + i;
            if acc[idx] >= threshold {
                order[idx] = 1 + (acc[idx] / threshold).log2().floor().max(0.0) as u32;
            }
        }
    }
    order
}

pub fn carve_rivers(
    input: &Heightfield,
    p: &RiverCarveParams,
    guide: Option<&MaskField>,
) -> (Heightfield, MaskField, MaskField, MaskField) {
    let w = input.metrics.width as usize;
    let h = input.metrics.height as usize;
    let filled = fill_depressions(input);
    let (flow_mask, acc_vec) = if p.use_dinfinity {
        let dirs = flow_direction_dinfinity(&filled);
        let acc = flow_accumulation_dinfinity(&filled, &dirs);
        let (_, fm) = flow_direction_d8(&filled);
        (fm, acc)
    } else {
        let (dirs, fm) = flow_direction_d8(&filled);
        let acc = flow_accumulation_d8(&filled, &dirs);
        (fm, acc)
    };

    let boost = p.guide_boost.max(0.0);
    let mut effective = acc_vec.clone();
    if let Some(g) = guide {
        if boost > 1e-6 {
            for j in 0..h {
                for i in 0..w {
                    let idx = j * w + i;
                    let gv = g.get(i as u32, j as u32).clamp(0.0, 1.0);
                    effective[idx] *= 1.0 + gv * boost;
                }
            }
        }
    }

    let max_acc = effective.iter().cloned().fold(0.0f32, f32::max).max(1.0);
    let mut acc_mask = MaskField::zeros(input.metrics);
    for j in 0..h {
        for i in 0..w {
            acc_mask.set(i as u32, j as u32, effective[j * w + i] / max_acc);
        }
    }

    let mut out = input.clone();
    let mut wetness = MaskField::zeros(input.metrics);
    for j in 0..h as i32 {
        for i in 0..w as i32 {
            let idx = j as usize * w + i as usize;
            if effective[idx] < p.accumulation_threshold {
                continue;
            }
            let accumulation_scale = (effective[idx] / p.accumulation_threshold)
                .sqrt()
                .clamp(1.0, 4.0);
            let channel_width = p.width.max(1.0) * accumulation_scale;
            let channel_depth = p.depth * accumulation_scale.sqrt();
            let bank_smooth = p.bank_smooth.max(0.0);
            let bank_radius = channel_width * (1.0 + bank_smooth * 0.75);
            let sigma = channel_width * (0.35 + bank_smooth * 0.2).max(0.1);
            let radius = bank_radius.ceil() as i32;
            for dj in -radius..=radius {
                for di in -radius..=radius {
                    let ni = i + di;
                    let nj = j + dj;
                    if ni < 0 || nj < 0 || ni >= w as i32 || nj >= h as i32 {
                        continue;
                    }
                    let dist = ((di * di + dj * dj) as f32).sqrt();
                    if dist > bank_radius {
                        continue;
                    }
                    let falloff = (-0.5 * (dist / sigma).powi(2)).exp();
                    let carve = channel_depth * falloff;
                    let cur = out.get(ni as u32, nj as u32);
                    out.set(ni as u32, nj as u32, cur - carve);
                    let wet = (falloff * accumulation_scale.sqrt() * 0.85).clamp(0.0, 1.0);
                    let prev = wetness.get(ni as u32, nj as u32);
                    if wet > prev {
                        wetness.set(ni as u32, nj as u32, wet);
                    }
                }
            }
        }
    }
    (out, flow_mask, acc_mask, wetness)
}

/// Result of a stream-power erosion pass (CPU export oracle).
pub struct StreamPowerResult {
    pub height: Heightfield,
    pub flow_direction: MaskField,
    pub flow_accumulation: MaskField,
    pub stream_order: MaskField,
    /// Cumulative incision depth in meters (raw; overlays normalize).
    pub spe_incision: MaskField,
}

const D8_OFFSETS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

/// Light Dendry-inspired valley prime: lower cells proportional to distance from ridges.
///
/// Ridges are local maxima (higher than all 8 neighbors). Distance is a multi-source
/// BFS in grid steps. This is **not** a full Dendry network authoring pass.
pub fn ridge_distance_valley_seed(hf: &Heightfield, strength: f32) -> Heightfield {
    if strength <= 1e-6 {
        return hf.clone();
    }
    let w = hf.metrics.width as usize;
    let h = hf.metrics.height as usize;
    if w == 0 || h == 0 {
        return hf.clone();
    }

    let mut dist = vec![i32::MAX; w * h];
    let mut queue = VecDeque::new();
    for j in 0..h {
        for i in 0..w {
            let h0 = hf.get(i as u32, j as u32);
            let mut is_ridge = true;
            for &(di, dj) in &D8_OFFSETS {
                let ni = i as i32 + di;
                let nj = j as i32 + dj;
                if ni < 0 || nj < 0 || ni >= w as i32 || nj >= h as i32 {
                    continue;
                }
                if hf.get(ni as u32, nj as u32) >= h0 {
                    is_ridge = false;
                    break;
                }
            }
            if is_ridge {
                let idx = j * w + i;
                dist[idx] = 0;
                queue.push_back(idx);
            }
        }
    }
    // Fallback: if no local max (flat), seed the map border so distance still varies.
    if queue.is_empty() {
        for j in 0..h {
            for i in 0..w {
                if i == 0 || j == 0 || i + 1 == w || j + 1 == h {
                    let idx = j * w + i;
                    dist[idx] = 0;
                    queue.push_back(idx);
                }
            }
        }
    }

    while let Some(idx) = queue.pop_front() {
        let i = idx % w;
        let j = idx / w;
        let d0 = dist[idx];
        for &(di, dj) in &D8_OFFSETS {
            let ni = i as i32 + di;
            let nj = j as i32 + dj;
            if ni < 0 || nj < 0 || ni >= w as i32 || nj >= h as i32 {
                continue;
            }
            let nidx = nj as usize * w + ni as usize;
            let nd = d0 + 1;
            if nd < dist[nidx] {
                dist[nidx] = nd;
                queue.push_back(nidx);
            }
        }
    }

    let max_d = dist
        .iter()
        .copied()
        .filter(|&d| d < i32::MAX)
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    let (hmin, hmax) = hf.min_max();
    let relief = (hmax - hmin).max(1.0);
    let mut out = hf.clone();
    for j in 0..h {
        for i in 0..w {
            let d = dist[j * w + i];
            let t = if d == i32::MAX { 1.0 } else { d as f32 / max_d };
            let carve = strength * t * relief * 0.05;
            let cur = out.get(i as u32, j as u32);
            out.set(i as u32, j as u32, cur - carve);
        }
    }
    out
}

fn slope_along_d8(hf: &Heightfield, i: u32, j: u32, dir: u8) -> f32 {
    let w = hf.metrics.width as i32;
    let h = hf.metrics.height as i32;
    if dir as usize >= D8_OFFSETS.len() {
        return 0.0;
    }
    let (di, dj) = D8_OFFSETS[dir as usize];
    let ni = i as i32 + di;
    let nj = j as i32 + dj;
    if ni < 0 || nj < 0 || ni >= w || nj >= h {
        return 0.0;
    }
    let dist = if di != 0 && dj != 0 {
        std::f32::consts::SQRT_2
    } else {
        1.0
    };
    let drop = hf.get(i, j) - hf.get(ni as u32, nj as u32);
    (drop / dist).max(0.0)
}

fn max_local_slope(hf: &Heightfield, i: u32, j: u32) -> f32 {
    let w = hf.metrics.width as i32;
    let h = hf.metrics.height as i32;
    let h0 = hf.get(i, j);
    let mut best = 0.0f32;
    for &(di, dj) in &D8_OFFSETS {
        let ni = i as i32 + di;
        let nj = j as i32 + dj;
        if ni < 0 || nj < 0 || ni >= w || nj >= h {
            continue;
        }
        let dist = if di != 0 && dj != 0 {
            std::f32::consts::SQRT_2
        } else {
            1.0
        };
        let s = (h0 - hf.get(ni as u32, nj as u32)) / dist;
        best = best.max(s);
    }
    best.max(0.0)
}

/// Stream-power incision with Priority-Flood drainage (CPU oracle).
///
/// Each iteration: optional fill → D8/D∞ accumulation →
/// \(z \mathrel{-}= K\,A^{m}\,S^{n}\,(1-K_{\mathrm{hard}})\,\Delta t\), then optional uplift.
/// Interactive Draft may reuse drainage across iters (`drainage_reuse_stride`) and
/// the GPU path runs an approximate multi-pass D8 solver (no Priority-Flood).
pub fn stream_power_erode(
    input: &Heightfield,
    p: &StreamPowerParams,
    hardness: &MaskField,
) -> StreamPowerResult {
    let w = input.metrics.width as usize;
    let h = input.metrics.height as usize;
    let mut out = if p.dendritic_seed > 0.0 {
        ridge_distance_valley_seed(input, p.dendritic_seed)
    } else {
        input.clone()
    };

    let mut incision = vec![0.0f32; w * h];
    let iters = p.iterations.max(1);
    let k = p.k.max(0.0);
    let m_exp = p.m.max(0.0);
    let n_exp = p.n.max(0.0);
    let dt = p.dt.max(0.0);
    let base = p.base_level;
    let drainage_stride = p.drainage_reuse_stride.max(1);

    let mut last_acc = vec![1.0f32; w * h];
    let mut last_flow_mask = MaskField::zeros(input.metrics);
    let mut last_dirs = vec![0u8; w * h];

    for iter in 0..iters {
        let refresh_drainage = iter == 0 || p.refill_each_iter || iter % drainage_stride == 0;
        let (dirs, _flow_mask, acc_vec, route) = if refresh_drainage {
            let filled = if iter == 0 || p.refill_each_iter {
                fill_depressions(&out)
            } else {
                // Cheap path: only fill once unless requested; still route on current DEM.
                out.clone()
            };

            let (dirs, flow_mask, acc_vec) = if p.use_dinfinity {
                let din = flow_direction_dinfinity(&filled);
                let acc = flow_accumulation_dinfinity(&filled, &din);
                let (d8, fm) = flow_direction_d8(&filled);
                (d8, fm, acc)
            } else {
                let (d8, fm) = flow_direction_d8(&filled);
                let acc = flow_accumulation_d8(&filled, &d8);
                (d8, fm, acc)
            };
            last_acc = acc_vec.clone();
            last_flow_mask = flow_mask.clone();
            last_dirs = dirs.clone();
            (dirs, flow_mask, acc_vec, filled)
        } else {
            // Cached drainage reuse across SPE iters (Draft preview; export keeps stride=1).
            (
                last_dirs.clone(),
                last_flow_mask.clone(),
                last_acc.clone(),
                out.clone(),
            )
        };

        // Cell area in world units² so K is resolution-aware enough for previews.
        let cell_area = (input.metrics.world_size_x / input.metrics.width.max(1) as f32)
            * (input.metrics.world_size_z / input.metrics.height.max(1) as f32);
        let cell_area = cell_area.max(1e-6);

        for j in 0..h as u32 {
            for i in 0..w as u32 {
                let idx = j as usize * w + i as usize;
                let area = (acc_vec[idx] * cell_area).max(cell_area);
                let slope = {
                    let s_flow = slope_along_d8(&route, i, j, dirs[idx]);
                    if s_flow > 1e-8 {
                        s_flow
                    } else {
                        max_local_slope(&route, i, j)
                    }
                }
                .max(1e-6);
                let soft = (1.0 - hardness.get(i, j).clamp(0.0, 1.0)).max(0.0);
                let power = k * area.powf(m_exp) * slope.powf(n_exp) * soft * dt;
                // Cap per-step incision to keep Draft previews stable.
                let step = power.min(slope * 2.0).min(50.0);
                let cur = out.get(i, j);
                let next = (cur - step + p.uplift_rate).max(base);
                incision[idx] += (cur - next).max(0.0);
                out.set(i, j, next);
            }
        }
    }

    let max_acc = last_acc.iter().cloned().fold(0.0f32, f32::max).max(1.0);
    let mut acc_mask = MaskField::zeros(input.metrics);
    for j in 0..h {
        for i in 0..w {
            acc_mask.set(i as u32, j as u32, last_acc[j * w + i] / max_acc);
        }
    }

    let order = stream_order(&last_acc, w, h, p.stream_threshold.max(1.0));
    let max_order = order.iter().copied().max().unwrap_or(1).max(1) as f32;
    let mut order_mask = MaskField::zeros(input.metrics);
    for j in 0..h {
        for i in 0..w {
            order_mask.set(i as u32, j as u32, order[j * w + i] as f32 / max_order);
        }
    }

    let mut incision_mask = MaskField::zeros(input.metrics);
    for j in 0..h {
        for i in 0..w {
            incision_mask.set(i as u32, j as u32, incision[j * w + i]);
        }
    }

    StreamPowerResult {
        height: out,
        flow_direction: last_flow_mask,
        flow_accumulation: acc_mask,
        stream_order: order_mask,
        spe_incision: incision_mask,
    }
}

/// SPE with depth-aware strata hardness (soft cap strips before hard base resists).
pub fn stream_power_erode_with_strata(
    input: &Heightfield,
    p: &StreamPowerParams,
    reference: &MaskField,
    strata: &[Stratum],
    default_hardness: f32,
) -> StreamPowerResult {
    let w = input.metrics.width as usize;
    let h = input.metrics.height as usize;
    let mut out = if p.dendritic_seed > 0.0 {
        ridge_distance_valley_seed(input, p.dendritic_seed)
    } else {
        input.clone()
    };

    let mut incision = vec![0.0f32; w * h];
    let iters = p.iterations.max(1);
    let k = p.k.max(0.0);
    let m_exp = p.m.max(0.0);
    let n_exp = p.n.max(0.0);
    let dt = p.dt.max(0.0);
    let base = p.base_level;
    let drainage_stride = p.drainage_reuse_stride.max(1);

    let mut last_acc = vec![1.0f32; w * h];
    let mut last_flow_mask = MaskField::zeros(input.metrics);
    let mut last_dirs = vec![0u8; w * h];

    for iter in 0..iters {
        let refresh_drainage = iter == 0 || p.refill_each_iter || iter % drainage_stride == 0;
        let (dirs, acc_vec, route) = if refresh_drainage {
            let filled = if iter == 0 || p.refill_each_iter {
                fill_depressions(&out)
            } else {
                out.clone()
            };

            let (dirs, flow_mask, acc_vec) = if p.use_dinfinity {
                let din = flow_direction_dinfinity(&filled);
                let acc = flow_accumulation_dinfinity(&filled, &din);
                let (d8, fm) = flow_direction_d8(&filled);
                (d8, fm, acc)
            } else {
                let (d8, fm) = flow_direction_d8(&filled);
                let acc = flow_accumulation_d8(&filled, &d8);
                (d8, fm, acc)
            };
            last_acc = acc_vec.clone();
            last_flow_mask = flow_mask;
            last_dirs = dirs.clone();
            (dirs, acc_vec, filled)
        } else {
            (last_dirs.clone(), last_acc.clone(), out.clone())
        };

        let cell_area = (input.metrics.world_size_x / input.metrics.width.max(1) as f32)
            * (input.metrics.world_size_z / input.metrics.height.max(1) as f32);
        let cell_area = cell_area.max(1e-6);

        for j in 0..h as u32 {
            for i in 0..w as u32 {
                let idx = j as usize * w + i as usize;
                let area = (acc_vec[idx] * cell_area).max(cell_area);
                let slope = {
                    let s_flow = slope_along_d8(&route, i, j, dirs[idx]);
                    if s_flow > 1e-8 {
                        s_flow
                    } else {
                        max_local_slope(&route, i, j)
                    }
                }
                .max(1e-6);
                let cur = out.get(i, j);
                let depth = (reference.get(i, j) - cur).max(0.0);
                let soft = erodibility_at_strata_depth(strata, depth, default_hardness);
                let power = k * area.powf(m_exp) * slope.powf(n_exp) * soft * dt;
                let step = power.min(slope * 2.0).min(50.0);
                let next = (cur - step + p.uplift_rate).max(base);
                incision[idx] += (cur - next).max(0.0);
                out.set(i, j, next);
            }
        }
    }

    let max_acc = last_acc.iter().cloned().fold(0.0f32, f32::max).max(1.0);
    let mut acc_mask = MaskField::zeros(input.metrics);
    for j in 0..h {
        for i in 0..w {
            acc_mask.set(i as u32, j as u32, last_acc[j * w + i] / max_acc);
        }
    }

    let order = stream_order(&last_acc, w, h, p.stream_threshold.max(1.0));
    let max_order = order.iter().copied().max().unwrap_or(1).max(1) as f32;
    let mut order_mask = MaskField::zeros(input.metrics);
    for j in 0..h {
        for i in 0..w {
            order_mask.set(i as u32, j as u32, order[j * w + i] as f32 / max_order);
        }
    }

    let mut incision_mask = MaskField::zeros(input.metrics);
    for j in 0..h {
        for i in 0..w {
            incision_mask.set(i as u32, j as u32, incision[j * w + i]);
        }
    }

    StreamPowerResult {
        height: out,
        flow_direction: last_flow_mask,
        flow_accumulation: acc_mask,
        stream_order: order_mask,
        spe_incision: incision_mask,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heightfield::HeightfieldMetrics;

    #[test]
    fn pyramid_flows_outward_down() {
        let m = HeightfieldMetrics::new(9, 9, 9.0, 9.0);
        let mut hf = Heightfield::zeros(m);
        for j in 0..9 {
            for i in 0..9 {
                let d = (i as i32 - 4).abs() + (j as i32 - 4).abs();
                hf.set(i, j, 10.0 - d as f32);
            }
        }
        let (dirs, _) = flow_direction_d8(&hf);
        let acc = flow_accumulation_d8(&hf, &dirs);
        // Center should have low accumulation; corners higher paths end lower
        assert!(acc[0] >= 1.0);
        assert!(acc.iter().any(|&a| a > 5.0));
    }

    #[test]
    fn fill_depressions_raises_enclosed_pit_to_spill_height() {
        let m = HeightfieldMetrics::new(3, 3, 3.0, 3.0);
        let mut hf = Heightfield::filled(m, 5.0);
        hf.set(1, 1, 1.0);

        let filled = fill_depressions(&hf);

        assert_eq!(filled.get(1, 1), 5.0);
        assert_eq!(filled.get(0, 0), 5.0);
    }

    #[test]
    fn d8_marks_local_sink_as_no_flow() {
        let m = HeightfieldMetrics::new(3, 3, 3.0, 3.0);
        let mut hf = Heightfield::filled(m, 2.0);
        hf.set(1, 1, 1.0);

        let (dirs, _) = flow_direction_d8(&hf);
        let acc = flow_accumulation_d8(&hf, &dirs);

        assert_eq!(dirs[4], NO_FLOW);
        assert_eq!(acc[4], 9.0);
    }

    #[test]
    fn spe_is_deterministic() {
        let m = HeightfieldMetrics::new(24, 24, 240.0, 240.0);
        let mut hf = Heightfield::zeros(m);
        for j in 0..24 {
            for i in 0..24 {
                hf.set(i, j, 30.0 + (i as f32) * 0.4 + (j as f32) * 0.2);
            }
        }
        let hard = MaskField::filled(m, 0.0);
        let p = StreamPowerParams {
            iterations: 6,
            k: 0.05,
            ..StreamPowerParams::default()
        };
        let a = stream_power_erode(&hf, &p, &hard);
        let b = stream_power_erode(&hf, &p, &hard);
        assert_eq!(a.height.to_dense(), b.height.to_dense());
    }

    #[test]
    fn spe_prefers_soft_rock() {
        let m = HeightfieldMetrics::new(32, 32, 320.0, 320.0);
        let mut hf = Heightfield::zeros(m);
        for j in 0..32 {
            for i in 0..32 {
                // Gentle ramp draining to +X edge.
                hf.set(i, j, 40.0 - i as f32 * 0.8);
            }
        }
        let mut hard = MaskField::filled(m, 0.0);
        for j in 0..32 {
            for i in 0..16 {
                hard.set(i, j, 0.95);
            }
        }
        let p = StreamPowerParams {
            iterations: 12,
            k: 0.12,
            m: 0.5,
            n: 1.0,
            dt: 1.0,
            refill_each_iter: true,
            ..StreamPowerParams::default()
        };
        let out = stream_power_erode(&hf, &p, &hard);
        let soft_drop: f32 = (16..32)
            .map(|i| hf.get(i, 16) - out.height.get(i, 16))
            .sum();
        let hard_drop: f32 = (0..16).map(|i| hf.get(i, 16) - out.height.get(i, 16)).sum();
        assert!(
            soft_drop > hard_drop * 1.25,
            "soft_drop={soft_drop} hard_drop={hard_drop}"
        );
    }

    #[test]
    fn spe_reduces_enclosed_pits_with_fill() {
        let m = HeightfieldMetrics::new(17, 17, 170.0, 170.0);
        let mut hf = Heightfield::filled(m, 20.0);
        // Slope toward borders with a central pit.
        for j in 0..17 {
            for i in 0..17 {
                let d = ((i as i32 - 8).abs() + (j as i32 - 8).abs()) as f32;
                hf.set(i, j, 10.0 + d);
            }
        }
        hf.set(8, 8, 2.0);
        let before = {
            let filled = fill_depressions(&hf);
            let a = hf.to_dense();
            let b = filled.to_dense();
            a.iter()
                .zip(b.iter())
                .map(|(h, f)| (f - h).max(0.0))
                .sum::<f32>()
        };
        let p = StreamPowerParams {
            iterations: 10,
            k: 0.15,
            refill_each_iter: true,
            dendritic_seed: 0.5,
            ..StreamPowerParams::default()
        };
        let out = stream_power_erode(&hf, &p, &MaskField::filled(m, 0.0));
        let after = {
            let filled = fill_depressions(&out.height);
            let a = out.height.to_dense();
            let b = filled.to_dense();
            a.iter()
                .zip(b.iter())
                .map(|(h, f)| (f - h).max(0.0))
                .sum::<f32>()
        };
        assert!(
            after <= before * 0.85 + 1.0,
            "fill volume before={before} after={after}"
        );
    }

    #[test]
    fn downhill_flow_single_outlet_sanity() {
        let m = HeightfieldMetrics::new(9, 9, 90.0, 90.0);
        let mut hf = Heightfield::zeros(m);
        for j in 0..9 {
            for i in 0..9 {
                hf.set(i, j, 20.0 - j as f32);
            }
        }
        let filled = fill_depressions(&hf);
        let (dirs, _) = flow_direction_d8(&filled);
        let acc = flow_accumulation_d8(&filled, &dirs);
        // Bottom row should collect the most water.
        let top: f32 = (0..9).map(|i| acc[i]).sum();
        let bot: f32 = (0..9).map(|i| acc[8 * 9 + i]).sum();
        assert!(bot > top);
        // Interior cells should generally flow south (dir index 2 = (0,1)).
        let south = dirs.iter().filter(|&&d| d == 2).count();
        assert!(south > 20);
    }

    #[test]
    fn dinfinity_routes_within_the_winning_facet() {
        let m = HeightfieldMetrics::new(5, 5, 5.0, 5.0);
        let mut hf = Heightfield::zeros(m);
        for j in 0..5 {
            for i in 0..5 {
                hf.set(i, j, 100.0 - i as f32 - 0.5 * j as f32);
            }
        }

        let routed = flow_direction_dinfinity(&hf);
        let center = &routed[2 * 5 + 2];
        assert_eq!(center.len(), 2);
        assert!(center.iter().any(|d| (d.di, d.dj) == (1, 0)));
        assert!(center.iter().any(|d| (d.di, d.dj) == (1, 1)));
        let total: f32 = center.iter().map(|d| d.fraction).sum();
        assert!((total - 1.0).abs() < 1e-6);
        assert!(center.iter().all(|d| d.fraction > 0.0));
    }

    #[test]
    fn dinfinity_cardinal_plane_does_not_split_sideways() {
        let m = HeightfieldMetrics::new(5, 5, 5.0, 5.0);
        let mut hf = Heightfield::zeros(m);
        for j in 0..5 {
            for i in 0..5 {
                hf.set(i, j, 20.0 - i as f32);
            }
        }
        let routed = flow_direction_dinfinity(&hf);
        let center = &routed[2 * 5 + 2];
        assert_eq!(center.len(), 1);
        assert_eq!((center[0].di, center[0].dj), (1, 0));
        assert!((center[0].fraction - 1.0).abs() < 1e-6);
    }
}
