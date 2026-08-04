//! Flow routing, watersheds, and river carving.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::heightfield::Heightfield;
use crate::layer::RiverCarveParams;
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
    for j in 0..h {
        for i in 0..w {
            let h0 = hf.get(i as u32, j as u32);
            let mut slopes = [(0i32, 0i32, 0.0f32); 8];
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
                let s = (h0 - hf.get(ni as u32, nj as u32)) / dist;
                slopes[k] = (di, dj, s.max(0.0));
            }
            // Pick two highest positive slopes
            let mut order: Vec<usize> = (0..8).collect();
            order.sort_by(|&a, &b| slopes[b].2.partial_cmp(&slopes[a].2).unwrap());
            let s0 = slopes[order[0]].2;
            let s1 = slopes[order[1]].2;
            let mut dirs = Vec::new();
            if s0 > 0.0 {
                let frac = if s1 > 0.0 { s0 / (s0 + s1) } else { 1.0 };
                dirs.push(FlowDir {
                    di: slopes[order[0]].0,
                    dj: slopes[order[0]].1,
                    fraction: frac,
                });
                if s1 > 0.0 {
                    dirs.push(FlowDir {
                        di: slopes[order[1]].0,
                        dj: slopes[order[1]].1,
                        fraction: 1.0 - frac,
                    });
                }
            }
            out[j * w + i] = dirs;
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
) -> (Heightfield, MaskField, MaskField) {
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

    let max_acc = acc_vec.iter().cloned().fold(0.0f32, f32::max).max(1.0);
    let mut acc_mask = MaskField::zeros(input.metrics);
    for j in 0..h {
        for i in 0..w {
            acc_mask.set(i as u32, j as u32, acc_vec[j * w + i] / max_acc);
        }
    }

    let mut out = input.clone();
    for j in 0..h as i32 {
        for i in 0..w as i32 {
            let idx = j as usize * w + i as usize;
            if acc_vec[idx] < p.accumulation_threshold {
                continue;
            }
            let accumulation_scale = (acc_vec[idx] / p.accumulation_threshold)
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
                }
            }
        }
    }
    (out, flow_mask, acc_mask)
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
}
