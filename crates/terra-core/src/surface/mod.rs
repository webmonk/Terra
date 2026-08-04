//! Materials, biomes, and vegetation density maps.

use crate::analyze::slope_degrees;
use crate::heightfield::Heightfield;
use crate::layer::{BiomesParams, MaterialsParams, VegetationParams};
use crate::mask::MaskField;
use crate::scatter;

pub fn material_weights(
    hf: &Heightfield,
    p: &MaterialsParams,
    _wetness: Option<&MaskField>,
) -> MaskField {
    let slope = slope_degrees(hf);
    let mut out = MaskField::zeros(hf.metrics);
    for j in 0..hf.metrics.height {
        for i in 0..hf.metrics.width {
            let h = hf.get(i, j);
            let s = slope.get(i, j) * 90.0;
            let mut best_id = 0.0;
            for rule in &p.rules {
                if s >= rule.min_slope_deg
                    && s <= rule.max_slope_deg
                    && h >= rule.min_height
                    && h <= rule.max_height
                {
                    best_id = rule.id as f32 / 16.0;
                }
            }
            out.set(i, j, best_id.clamp(0.0, 1.0));
        }
    }
    out
}

pub fn biome_mask(hf: &Heightfield, p: &BiomesParams, wetness: Option<&MaskField>) -> MaskField {
    let mut out = MaskField::zeros(hf.metrics);
    for j in 0..hf.metrics.height {
        for i in 0..hf.metrics.width {
            let h = hf.get(i, j);
            let w = wetness.map(|m| m.get(i, j)).unwrap_or(0.5);
            let mut id = 0.0;
            for band in &p.bands {
                if h >= band.min_height
                    && h <= band.max_height
                    && w >= band.min_wetness
                    && w <= band.max_wetness
                {
                    id = band.id as f32 / 16.0;
                }
            }
            out.set(i, j, id);
        }
    }
    out
}

pub fn vegetation_density(
    hf: &Heightfield,
    p: &VegetationParams,
    biomes: Option<&MaskField>,
) -> MaskField {
    let slope = slope_degrees(hf);
    let mut suitability = MaskField::zeros(hf.metrics);
    for j in 0..hf.metrics.height {
        for i in 0..hf.metrics.width {
            let s = slope.get(i, j) * 90.0;
            if s < p.min_slope_deg || s > p.max_slope_deg {
                continue;
            }
            if let (Some(b), Some(want)) = (biomes, p.biome_id) {
                let id = (b.get(i, j) * 16.0).round() as u32;
                if id != want {
                    continue;
                }
            }
            suitability.set(i, j, p.density);
        }
    }
    // Encode scatter count proxy via poisson acceptance
    let points = scatter::poisson_disk(hf, p);
    let mut dens = MaskField::zeros(hf.metrics);
    for &(x, z) in &points {
        let (i, j) = hf.metrics.sample_index(x, z);
        dens.set(i, j, 1.0);
    }
    // Soften
    crate::mask::apply_mask_ops(&mut dens, &[crate::mask::MaskOp::Blur { radius: 1 }]);
    for j in 0..hf.metrics.height {
        for i in 0..hf.metrics.width {
            dens.set(i, j, dens.get(i, j) * suitability.get(i, j));
        }
    }
    dens
}
