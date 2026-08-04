use crate::heightfield::Heightfield;
use crate::layer::{HydraulicErosionParams, ThermalErosionParams};
use crate::mask::MaskField;

/// Thermal erosion via talus-angle redistribution (CPU reference).
/// Returns (height, erosion_mask, deposition_mask).
pub fn thermal_erode(
    input: &Heightfield,
    p: &ThermalErosionParams,
) -> (Heightfield, MaskField, MaskField) {
    let mut h = input.to_dense();
    let w = input.metrics.width as usize;
    let hh = input.metrics.height as usize;
    let dx = input.metrics.dx();
    let talus = p.talus_angle_deg.to_radians().tan() * dx;
    let strength = p.strength.clamp(0.0, 1.0);

    let mut erosion = vec![0.0f32; w * hh];
    let mut deposit = vec![0.0f32; w * hh];

    let neighbors = [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)];

    for _ in 0..p.iterations {
        let src = h.clone();
        for j in 0..hh as i32 {
            for i in 0..w as i32 {
                let idx = j as usize * w + i as usize;
                let h0 = src[idx];
                let mut deltas = Vec::with_capacity(4);
                let mut sum = 0.0f32;
                for &(di, dj) in &neighbors {
                    let ni = i + di;
                    let nj = j + dj;
                    if ni < 0 || nj < 0 || ni >= w as i32 || nj >= hh as i32 {
                        continue;
                    }
                    let nidx = nj as usize * w + ni as usize;
                    let diff = h0 - src[nidx] - talus;
                    if diff > 0.0 {
                        deltas.push((nidx, diff));
                        sum += diff;
                    }
                }
                if sum <= 0.0 {
                    continue;
                }
                let move_amt = sum * strength * 0.25;
                h[idx] -= move_amt;
                erosion[idx] += move_amt;
                for (nidx, diff) in deltas {
                    let share = move_amt * (diff / sum);
                    h[nidx] += share;
                    deposit[nidx] += share;
                }
            }
        }
    }

    let height = Heightfield::from_dense(input.metrics, &h);
    let (e_mask, d_mask) = normalize_pair(&erosion, &deposit, input.metrics);
    (height, e_mask, d_mask)
}

pub struct HydraulicResult {
    pub height: Heightfield,
    pub wetness: MaskField,
    pub sediment: MaskField,
    pub erosion: MaskField,
    pub deposition: MaskField,
}

/// Simplified shallow-water hydraulic erosion (Mei/Šťava lineage, CPU).
pub fn hydraulic_erode(input: &Heightfield, p: &HydraulicErosionParams) -> HydraulicResult {
    let w = input.metrics.width as usize;
    let hh = input.metrics.height as usize;
    let mut height = input.to_dense();
    let mut water = vec![0.0f32; w * hh];
    let mut sediment = vec![0.0f32; w * hh];
    let mut erosion = vec![0.0f32; w * hh];
    let mut deposition = vec![0.0f32; w * hh];
    let mut wetness = vec![0.0f32; w * hh];

    let neighbors = [(-1i32, 0), (1, 0), (0, -1), (0, 1)];

    for _ in 0..p.iterations {
        // Rain
        for v in &mut water {
            *v += p.rainfall;
        }

        let h_snap = height.clone();
        let w_snap = water.clone();
        let s_snap = sediment.clone();

        for j in 0..hh as i32 {
            for i in 0..w as i32 {
                let idx = j as usize * w + i as usize;
                let total = h_snap[idx] + w_snap[idx];
                let mut outflow = 0.0f32;
                let mut targets = Vec::new();
                for &(di, dj) in &neighbors {
                    let ni = i + di;
                    let nj = j + dj;
                    if ni < 0 || nj < 0 || ni >= w as i32 || nj >= hh as i32 {
                        continue;
                    }
                    let nidx = nj as usize * w + ni as usize;
                    let ntotal = h_snap[nidx] + w_snap[nidx];
                    let delta = total - ntotal;
                    if delta > 0.0 {
                        targets.push((nidx, delta));
                        outflow += delta;
                    }
                }
                if outflow <= 0.0 || w_snap[idx] <= 0.0 {
                    continue;
                }
                let flow = (outflow * p.timestep).min(w_snap[idx]);
                water[idx] -= flow;
                let slope = outflow / 4.0;
                let cap = p.capacity * slope * flow;
                let s = s_snap[idx];
                if s < cap {
                    let erode_amt = ((cap - s) * p.erosion).min(height[idx].max(0.0) * 0.1);
                    height[idx] -= erode_amt;
                    sediment[idx] += erode_amt;
                    erosion[idx] += erode_amt;
                } else {
                    let dep = (s - cap) * p.deposition;
                    height[idx] += dep;
                    sediment[idx] -= dep;
                    deposition[idx] += dep;
                }
                for (nidx, delta) in targets {
                    let share = flow * (delta / outflow);
                    water[nidx] += share;
                    let sshare = sediment[idx].max(0.0) * (share / flow.max(1e-6)) * 0.25;
                    sediment[nidx] += sshare;
                    sediment[idx] = (sediment[idx] - sshare).max(0.0);
                }
            }
        }

        for idx in 0..water.len() {
            water[idx] *= 1.0 - p.evaporation;
            wetness[idx] += water[idx];
        }
    }

    let metrics = input.metrics;
    HydraulicResult {
        height: Heightfield::from_dense(metrics, &height),
        wetness: normalize_mask(&wetness, metrics),
        sediment: normalize_mask(&sediment, metrics),
        erosion: normalize_mask(&erosion, metrics),
        deposition: normalize_mask(&deposition, metrics),
    }
}

fn normalize_mask(data: &[f32], metrics: crate::heightfield::HeightfieldMetrics) -> MaskField {
    let max_v = data.iter().cloned().fold(0.0f32, f32::max).max(1e-6);
    let mut m = MaskField::zeros(metrics);
    for (i, &v) in data.iter().enumerate() {
        let x = (i as u32) % metrics.width;
        let y = (i as u32) / metrics.width;
        m.set(x, y, (v / max_v).clamp(0.0, 1.0));
    }
    m
}

fn normalize_pair(
    a: &[f32],
    b: &[f32],
    metrics: crate::heightfield::HeightfieldMetrics,
) -> (MaskField, MaskField) {
    (normalize_mask(a, metrics), normalize_mask(b, metrics))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heightfield::HeightfieldMetrics;

    #[test]
    fn thermal_relaxes_steep_spike() {
        let m = HeightfieldMetrics::new(16, 16, 16.0, 16.0);
        let mut hf = Heightfield::filled(m, 10.0);
        hf.set(8, 8, 50.0);
        let p = ThermalErosionParams {
            talus_angle_deg: 25.0,
            iterations: 30,
            strength: 0.8,
        };
        let (out, _, _) = thermal_erode(&hf, &p);
        assert!(out.get(8, 8) < 50.0);
        assert!(out.get(8, 8) > 10.0);
    }

    #[test]
    fn thermal_flat_unchanged() {
        let m = HeightfieldMetrics::new(8, 8, 8.0, 8.0);
        let hf = Heightfield::filled(m, 5.0);
        let p = ThermalErosionParams::default();
        let (out, _, _) = thermal_erode(&hf, &p);
        for j in 0..8 {
            for i in 0..8 {
                assert!((out.get(i, j) - 5.0).abs() < 1e-3);
            }
        }
    }

    #[test]
    fn hydraulic_finite() {
        let m = HeightfieldMetrics::new(24, 24, 48.0, 48.0);
        let mut hf = Heightfield::zeros(m);
        for j in 0..24 {
            for i in 0..24 {
                hf.set(i, j, (i + j) as f32 * 0.5);
            }
        }
        let r = hydraulic_erode(
            &hf,
            &HydraulicErosionParams {
                iterations: 10,
                ..HydraulicErosionParams::default()
            },
        );
        let d = r.height.to_dense();
        assert!(d.iter().all(|v| v.is_finite()));
    }
}
