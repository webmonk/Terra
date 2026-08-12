//! River carving and stream-power incision oracles.
//!
//! Flow routing (D8 / D∞), accumulation, depression handling, watersheds, and
//! Strahler streams live in [`crate::geomorph`]. This module keeps the CPU
//! stream-power / river-carve oracles used by the erosion processors; their
//! drainage prep builds on the geomorph [`crate::geomorph::FlowGraph`].

use std::collections::VecDeque;

use crate::fields::erodibility_at_strata_depth;
use crate::geomorph::{
    accumulate_drainage_area, build_flow_graph, priority_flood_fill, FlowModel, Precipitation,
};
use crate::heightfield::Heightfield;
use crate::layer::{RiverCarveParams, Stratum, StreamPowerParams};
use crate::mask::MaskField;

/// Fill enclosed depressions to their lowest spill elevation using Priority-Flood.
///
/// Delegates to [`crate::geomorph::priority_flood_fill`]: boundary cells are
/// preserved as drainage outlets and every interior pit is raised only enough to
/// connect to one of them.
pub fn fill_depressions(hf: &Heightfield) -> Heightfield {
    priority_flood_fill(hf)
}

/// Log2 stream-order bucket for the published `STREAM_ORDER` aux overlay.
///
/// This is a visualisation bucketing, `1 + floor(log2(acc / threshold))`, **not**
/// Horton–Strahler order — see [`crate::geomorph::strahler_order`] for the real
/// donor-graph ordering carried by `analyze_terrain`'s `StreamNetwork`.
pub fn stream_order_log2(acc: &[f32], w: usize, h: usize, threshold: f32) -> Vec<u32> {
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
    let model = if p.use_dinfinity {
        FlowModel::DInfinity
    } else {
        FlowModel::D8
    };
    let graph = build_flow_graph(&filled, model);
    let flow_mask = graph.direction_mask.clone();
    let acc_vec = accumulate_drainage_area(&graph, &Precipitation::uniform(1.0));

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

            let model = if p.use_dinfinity {
                FlowModel::DInfinity
            } else {
                FlowModel::D8
            };
            let graph = build_flow_graph(&filled, model);
            let dirs = graph.d8_dir.clone();
            let flow_mask = graph.direction_mask.clone();
            let acc_vec = accumulate_drainage_area(&graph, &Precipitation::uniform(1.0));
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

    let order = stream_order_log2(&last_acc, w, h, p.stream_threshold.max(1.0));
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

            let model = if p.use_dinfinity {
                FlowModel::DInfinity
            } else {
                FlowModel::D8
            };
            let graph = build_flow_graph(&filled, model);
            let dirs = graph.d8_dir.clone();
            let acc_vec = accumulate_drainage_area(&graph, &Precipitation::uniform(1.0));
            last_acc = acc_vec.clone();
            last_flow_mask = graph.direction_mask.clone();
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

    let order = stream_order_log2(&last_acc, w, h, p.stream_threshold.max(1.0));
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
    fn fill_depressions_raises_enclosed_pit_to_spill_height() {
        let m = HeightfieldMetrics::new(3, 3, 3.0, 3.0);
        let mut hf = Heightfield::filled(m, 5.0);
        hf.set(1, 1, 1.0);

        let filled = fill_depressions(&hf);

        assert_eq!(filled.get(1, 1), 5.0);
        assert_eq!(filled.get(0, 0), 5.0);
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

}
