//! Quantitative terrain realism statistics (Phase 11 Rule 6).
//!
//! Prefer these metrics over screenshot-only judgment. Compare against public
//! DEM samples when available — distributions matter more than absolute RMSE.

use crate::geomorph::{analyze_terrain, multi_radius_roughness, GeomorphOptions, StreamNetwork};
use crate::heightfield::Heightfield;
use crate::mask::MaskField;

/// Histogram of continuous values into `bins` equal-width buckets over \[min, max\].
#[derive(Debug, Clone)]
pub struct Histogram {
    pub min: f32,
    pub max: f32,
    pub counts: Vec<u32>,
}

impl Histogram {
    pub fn from_slice(data: &[f32], bins: usize) -> Self {
        let bins = bins.max(1);
        let mut min = f32::INFINITY;
        let mut max = f32::NEG_INFINITY;
        for &v in data {
            if v.is_finite() {
                min = min.min(v);
                max = max.max(v);
            }
        }
        if !min.is_finite() || !max.is_finite() || (max - min).abs() < 1e-12 {
            return Self {
                min: 0.0,
                max: 1.0,
                counts: vec![0; bins],
            };
        }
        let mut counts = vec![0u32; bins];
        let span = (max - min).max(1e-12);
        for &v in data {
            if !v.is_finite() {
                continue;
            }
            let t = ((v - min) / span * bins as f32).floor() as usize;
            counts[t.min(bins - 1)] += 1;
        }
        Self { min, max, counts }
    }

    pub fn from_mask(field: &MaskField, bins: usize) -> Self {
        Self::from_slice(field.data(), bins)
    }

    pub fn total(&self) -> u32 {
        self.counts.iter().sum()
    }

    /// Normalised bin fractions (sum ≈ 1).
    pub fn fractions(&self) -> Vec<f32> {
        let n = self.total().max(1) as f32;
        self.counts.iter().map(|&c| c as f32 / n).collect()
    }
}

/// Hypsometric (area–elevation) curve: cumulative fraction of area below each
/// elevation quantile. Classic Strahler hypsometric integral is also reported.
#[derive(Debug, Clone)]
pub struct HypsometricCurve {
    /// Elevation samples at equal area fractions (low → high).
    pub elevations_m: Vec<f32>,
    /// Area fraction below corresponding elevation (0…1).
    pub area_fractions: Vec<f32>,
    /// Hypsometric integral ≈ area under the curve in elevation–area space.
    pub integral: f32,
    pub z_min: f32,
    pub z_max: f32,
    pub z_mean: f32,
    pub relief_m: f32,
}

impl HypsometricCurve {
    pub fn compute(hf: &Heightfield, samples: usize) -> Self {
        let samples = samples.clamp(8, 512);
        let m = hf.metrics;
        let mut vals = Vec::with_capacity((m.width * m.height) as usize);
        for j in 0..m.height {
            for i in 0..m.width {
                let v = hf.get(i, j);
                if v.is_finite() {
                    vals.push(v);
                }
            }
        }
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if vals.is_empty() {
            return Self {
                elevations_m: vec![0.0; samples],
                area_fractions: (0..samples)
                    .map(|i| i as f32 / (samples - 1).max(1) as f32)
                    .collect(),
                integral: 0.5,
                z_min: 0.0,
                z_max: 0.0,
                z_mean: 0.0,
                relief_m: 0.0,
            };
        }
        let z_min = vals[0];
        let z_max = *vals.last().unwrap();
        let z_mean = vals.iter().sum::<f32>() / vals.len() as f32;
        let relief = (z_max - z_min).max(1e-6);
        let mut elevations_m = Vec::with_capacity(samples);
        let mut area_fractions = Vec::with_capacity(samples);
        for i in 0..samples {
            let frac = i as f32 / (samples - 1).max(1) as f32;
            let idx = ((frac * (vals.len() - 1) as f32).round() as usize).min(vals.len() - 1);
            elevations_m.push(vals[idx]);
            area_fractions.push(frac);
        }
        // Integral of relative height vs area fraction (trapezoid).
        let mut integral = 0.0;
        for i in 1..samples {
            let h0 = (elevations_m[i - 1] - z_min) / relief;
            let h1 = (elevations_m[i] - z_min) / relief;
            let da = area_fractions[i] - area_fractions[i - 1];
            integral += 0.5 * (h0 + h1) * da;
        }
        Self {
            elevations_m,
            area_fractions,
            integral,
            z_min,
            z_max,
            z_mean,
            relief_m: relief,
        }
    }
}

/// Stream-order occupancy (Strahler), counts per integer order.
#[derive(Debug, Clone, Default)]
pub struct StreamOrderDistribution {
    /// Index 0 unused; counts[k] = cells of Strahler order k.
    pub counts: Vec<u32>,
    pub max_order: u32,
}

impl StreamOrderDistribution {
    pub fn from_network(streams: &StreamNetwork) -> Self {
        let mut counts = vec![0u32; 16];
        let mut max_order = 0u32;
        for &o in streams.order.data() {
            let oi = o.max(0.0).round() as usize;
            if oi == 0 {
                continue;
            }
            if oi >= counts.len() {
                counts.resize(oi + 1, 0);
            }
            counts[oi] += 1;
            max_order = max_order.max(oi as u32);
        }
        Self { counts, max_order }
    }
}

/// Aggregate morphometric report for realism validation.
#[derive(Debug, Clone)]
pub struct TerrainStatistics {
    pub hypsometry: HypsometricCurve,
    pub slope_histogram: Histogram,
    pub curvature_histogram: Histogram,
    pub drainage_density: f32,
    pub ridge_density: f32,
    pub valley_density: f32,
    pub stream_orders: StreamOrderDistribution,
    /// Mean multi-radius roughness at each analysis radius (metres).
    pub roughness_by_scale: Vec<(f32, f32)>,
    pub mean_slope_deg: f32,
    pub pit_fraction: f32,
}

impl TerrainStatistics {
    /// Full quantitative pass (CPU). Prefer Draft/preview resolutions for CI.
    pub fn compute(hf: &Heightfield) -> Self {
        Self::compute_with_options(hf, &GeomorphOptions::default(), 32, 48)
    }

    pub fn compute_with_options(
        hf: &Heightfield,
        opts: &GeomorphOptions,
        hist_bins: usize,
        hyps_samples: usize,
    ) -> Self {
        let analysis = analyze_terrain(hf, opts);
        let slope = &analysis.derivatives.slope;
        // Slope mask stores degrees/90 in \[0,1\].
        let slope_deg: Vec<f32> = slope
            .data()
            .iter()
            .map(|&s| s.clamp(0.0, 1.0) * 90.0)
            .collect();
        let mean_slope_deg = if slope_deg.is_empty() {
            0.0
        } else {
            slope_deg.iter().sum::<f32>() / slope_deg.len() as f32
        };

        let curv = analysis.derivatives.mean_curvature.data();
        let drainage_density = mean_mask(&analysis.drainage.drainage_density);
        let ridge_density = mean_mask(&analysis.drainage.ridge_mask);
        let valley_density = mean_mask(&analysis.drainage.valley_mask);

        let radii = &opts.derivatives.roughness_radii_m;
        let rough_field = multi_radius_roughness(hf, radii);
        let mean_r = mean_mask(&rough_field);
        let roughness_by_scale: Vec<(f32, f32)> = radii.iter().map(|&r| (r, mean_r)).collect();

        let n = (hf.metrics.width * hf.metrics.height).max(1) as f32;
        let pit_cells = analysis
            .fill_delta
            .data()
            .iter()
            .filter(|&&d| d > 1e-4)
            .count();
        let pit_fraction = pit_cells as f32 / n;

        Self {
            hypsometry: HypsometricCurve::compute(hf, hyps_samples),
            slope_histogram: Histogram::from_slice(&slope_deg, hist_bins),
            curvature_histogram: Histogram::from_slice(curv, hist_bins),
            drainage_density,
            ridge_density,
            valley_density,
            stream_orders: StreamOrderDistribution::from_network(&analysis.streams),
            roughness_by_scale,
            mean_slope_deg,
            pit_fraction,
        }
    }

    /// Compact summary line for logs / CI.
    pub fn summary_line(&self) -> String {
        format!(
            "relief={:.1}m hyps_I={:.3} mean_slope={:.1}° drain_d={:.3} ridge={:.3} valley={:.3} max_strahler={} pits={:.4}",
            self.hypsometry.relief_m,
            self.hypsometry.integral,
            self.mean_slope_deg,
            self.drainage_density,
            self.ridge_density,
            self.valley_density,
            self.stream_orders.max_order,
            self.pit_fraction,
        )
    }
}

fn mean_mask(field: &MaskField) -> f32 {
    let d = field.data();
    if d.is_empty() {
        return 0.0;
    }
    d.iter().sum::<f32>() / d.len() as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geomorph::noisy_mountain;
    use crate::heightfield::HeightfieldMetrics;

    #[test]
    fn statistics_on_synthetic_mountain() {
        let metrics = HeightfieldMetrics::new(64, 64, 1024.0, 1024.0);
        let hf = noisy_mountain(metrics);
        let stats = TerrainStatistics::compute(&hf);
        assert!(stats.hypsometry.relief_m > 1.0);
        assert!(stats.mean_slope_deg > 0.0);
        assert!(!stats.slope_histogram.counts.is_empty());
        assert!(!stats.summary_line().is_empty());
    }
}
