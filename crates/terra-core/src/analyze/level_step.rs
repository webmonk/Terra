//! World Creator–style multilevel simulation helpers.
//!
//! Expensive sims (thermal / hydraulic) run primarily at coarse resolutions, then
//! results are upsampled into the target field with optional light polish iterations.

use crate::heightfield::{FloatArena, Heightfield, HeightfieldMetrics};
use crate::layer::{HydraulicErosionParams, ThermalErosionParams};
use crate::mask::MaskField;
use std::cell::RefCell;

thread_local! {
    static DOWNSAMPLE_ARENA: RefCell<FloatArena> = RefCell::new(FloatArena::new());
}

/// One coarse-to-fine simulation level.
#[derive(Debug, Clone, Copy)]
pub struct SimLevel {
    pub resolution: u32,
    /// Fraction of the authoring iteration count to run at this level.
    pub iter_scale: f32,
    /// Strength / rainfall scale at this level (WC: weaker on fine levels).
    pub effect_scale: f32,
}

/// Default interactive level schedule (coarse → fine).
pub fn default_sim_levels(target_res: u32) -> Vec<SimLevel> {
    let mut levels = Vec::new();
    // Always include a cheap coarse pass.
    if target_res >= 128 {
        levels.push(SimLevel {
            resolution: 128,
            iter_scale: 0.55,
            effect_scale: 1.0,
        });
    }
    if target_res >= 256 {
        levels.push(SimLevel {
            resolution: 256.min(target_res),
            iter_scale: 0.35,
            effect_scale: 0.75,
        });
    }
    if target_res > 256 {
        levels.push(SimLevel {
            resolution: target_res,
            iter_scale: 0.15,
            effect_scale: 0.4,
        });
    } else if levels.is_empty() {
        levels.push(SimLevel {
            resolution: target_res.max(32),
            iter_scale: 1.0,
            effect_scale: 1.0,
        });
    }
    // Deduplicate if target equals a prior level.
    levels.dedup_by(|a, b| a.resolution == b.resolution);
    levels
}

/// Draft quality: single coarse level only.
pub fn draft_sim_levels(target_res: u32) -> Vec<SimLevel> {
    vec![SimLevel {
        resolution: target_res.min(128).max(64),
        iter_scale: 0.5,
        effect_scale: 1.0,
    }]
}

pub fn downsample_height(src: &Heightfield, target_res: u32) -> Heightfield {
    let tw = target_res.max(1);
    let th = target_res.max(1);
    let metrics = HeightfieldMetrics {
        width: tw,
        height: th,
        world_size_x: src.metrics.world_size_x,
        world_size_z: src.metrics.world_size_z,
        tile_size: src.metrics.tile_size.min(tw),
        halo: src.metrics.halo,
    };
    if src.metrics.width == tw && src.metrics.height == th {
        return src.clone();
    }
    let dense = src.to_dense();
    let sw = src.metrics.width as usize;
    let sh = src.metrics.height as usize;
    let mut out =
        DOWNSAMPLE_ARENA.with(|arena| arena.borrow_mut().acquire(tw as usize * th as usize));
    for j in 0..th as usize {
        for i in 0..tw as usize {
            let u = (i as f32 + 0.5) / tw as f32;
            let v = (j as f32 + 0.5) / th as f32;
            let x = (u * sw as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
            let z = (v * sh as f32 - 0.5).clamp(0.0, (sh - 1) as f32);
            let x0 = x.floor() as usize;
            let z0 = z.floor() as usize;
            let x1 = (x0 + 1).min(sw - 1);
            let z1 = (z0 + 1).min(sh - 1);
            let fx = x - x0 as f32;
            let fz = z - z0 as f32;
            let h00 = dense[z0 * sw + x0];
            let h10 = dense[z0 * sw + x1];
            let h01 = dense[z1 * sw + x0];
            let h11 = dense[z1 * sw + x1];
            out[j * tw as usize + i] = h00 * (1.0 - fx) * (1.0 - fz)
                + h10 * fx * (1.0 - fz)
                + h01 * (1.0 - fx) * fz
                + h11 * fx * fz;
        }
    }
    let result = Heightfield::from_dense(metrics, &out);
    DOWNSAMPLE_ARENA.with(|arena| arena.borrow_mut().release(out));
    result
}

fn scale_thermal(p: &ThermalErosionParams, level: SimLevel) -> ThermalErosionParams {
    ThermalErosionParams {
        iterations: ((p.iterations as f32 * level.iter_scale).round() as u32).max(1),
        strength: (p.strength * level.effect_scale).clamp(0.0, 1.0),
        talus_angle_deg: p.talus_angle_deg,
    }
}

fn scale_hydraulic(p: &HydraulicErosionParams, level: SimLevel) -> HydraulicErosionParams {
    HydraulicErosionParams {
        iterations: ((p.iterations as f32 * level.iter_scale).round() as u32).max(1),
        rainfall: p.rainfall * level.effect_scale,
        evaporation: p.evaporation,
        capacity: p.capacity,
        erosion: p.erosion * level.effect_scale,
        deposition: p.deposition * level.effect_scale,
        timestep: p.timestep,
    }
}

/// Run thermal erosion across multilevel schedule, returning final height + masks at target res.
pub fn thermal_erode_leveled(
    input: &Heightfield,
    p: &ThermalErosionParams,
    levels: &[SimLevel],
) -> (Heightfield, MaskField, MaskField) {
    use super::erosion::thermal_erode;

    let target = input.metrics;
    let mut current = input.clone();
    let mut last_e = MaskField::zeros(target);
    let mut last_d = MaskField::zeros(target);

    for level in levels {
        let low = downsample_height(&current, level.resolution);
        let params = scale_thermal(p, *level);
        let (h, e, d) = thermal_erode(&low, &params);
        current = upsample_to_metrics(&h, target);
        last_e = upsample_mask(&e, target);
        last_d = upsample_mask(&d, target);
    }
    (current, last_e, last_d)
}

/// Run hydraulic erosion across multilevel schedule.
pub fn hydraulic_erode_leveled(
    input: &Heightfield,
    p: &HydraulicErosionParams,
    levels: &[SimLevel],
) -> super::erosion::HydraulicResult {
    use super::erosion::{hydraulic_erode, HydraulicResult};

    let target = input.metrics;
    let mut current = input.clone();
    let mut last = HydraulicResult {
        height: current.clone(),
        wetness: MaskField::zeros(target),
        sediment: MaskField::zeros(target),
        erosion: MaskField::zeros(target),
        deposition: MaskField::zeros(target),
    };

    for level in levels {
        let low = downsample_height(&current, level.resolution);
        let params = scale_hydraulic(p, *level);
        let r = hydraulic_erode(&low, &params);
        current = upsample_to_metrics(&r.height, target);
        last = HydraulicResult {
            height: current.clone(),
            wetness: upsample_mask(&r.wetness, target),
            sediment: upsample_mask(&r.sediment, target),
            erosion: upsample_mask(&r.erosion, target),
            deposition: upsample_mask(&r.deposition, target),
        };
    }
    last
}

fn upsample_to_metrics(src: &Heightfield, target: HeightfieldMetrics) -> Heightfield {
    if src.metrics.width == target.width && src.metrics.height == target.height {
        let mut h = src.clone();
        h.metrics = target;
        return h;
    }
    downsample_height(src, target.width)
}

fn upsample_mask(src: &MaskField, target: HeightfieldMetrics) -> MaskField {
    let hf = Heightfield::from_dense(src.metrics, src.data());
    let up = downsample_height(&hf, target.width);
    let dense = up.to_dense();
    let mut out = MaskField::zeros(target);
    out.data_mut().copy_from_slice(&dense);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heightfield::HeightfieldMetrics;

    #[test]
    fn downsample_upsample_preserves_flat() {
        let m = HeightfieldMetrics {
            width: 64,
            height: 64,
            world_size_x: 1000.0,
            world_size_z: 1000.0,
            tile_size: 64,
            halo: 2,
        };
        let src = Heightfield::from_dense(m, &vec![42.0f32; 64 * 64]);
        let low = downsample_height(&src, 16);
        assert_eq!(low.metrics.width, 16);
        let up = downsample_height(&low, 64);
        let d = up.to_dense();
        assert!((d[0] - 42.0).abs() < 0.01);
    }

    #[test]
    fn default_levels_include_coarse() {
        let levels = default_sim_levels(512);
        assert!(levels[0].resolution <= 128);
        assert_eq!(levels.last().unwrap().resolution, 512);
    }
}
