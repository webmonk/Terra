//! Terrain generator helpers used by processors.

use crate::eval::EvalError;
use crate::heightfield::{Heightfield, HeightfieldMetrics};
use crate::layer::*;
use crate::noise::{self, domain_warp_fbm, fbm, ridged_mf, sample_worley};
use rayon::prelude::*;

pub fn flat(metrics: HeightfieldMetrics, height: f32) -> Heightfield {
    Heightfield::filled(metrics, height)
}

/// Sample a sculpt paint buffer into the evaluation grid.
pub fn sculpt_base(metrics: HeightfieldMetrics, p: &SculptParams) -> Heightfield {
    let mut hf = Heightfield::zeros(metrics);
    for j in 0..metrics.height {
        for i in 0..metrics.width {
            let u = (i as f32 + 0.5) / metrics.width as f32;
            let v = (j as f32 + 0.5) / metrics.height as f32;
            hf.set(i, j, p.sample_bilinear(u, v));
        }
    }
    hf
}

pub fn ramp(metrics: HeightfieldMetrics, p: &RampParams) -> Heightfield {
    let mut hf = Heightfield::zeros(metrics);
    let dir_x = p.direction.cos();
    let dir_z = p.direction.sin();
    for j in 0..metrics.height {
        for i in 0..metrics.width {
            let x = metrics.world_x(i) / metrics.world_size_x;
            let z = metrics.world_z(j) / metrics.world_size_z;
            let t = (x * dir_x + z * dir_z).clamp(0.0, 1.0);
            hf.set(i, j, p.height_min + (p.height_max - p.height_min) * t);
        }
    }
    hf
}

pub fn noise_field(
    metrics: HeightfieldMetrics,
    p: &NoiseParams,
    kind: FractalNoiseType,
) -> Heightfield {
    fill_world(metrics, |x, z| {
        if p.octaves <= 1 {
            noise::sample_noise(
                kind,
                (x + p.offset_x) * p.frequency,
                (z + p.offset_z) * p.frequency,
                p.seed,
            ) * p.amplitude
        } else {
            fbm(kind, x, z, p)
        }
    })
}

pub fn worley_field(metrics: HeightfieldMetrics, p: &WorleyParams) -> Heightfield {
    fill_world(metrics, |x, z| sample_worley(x, z, p))
}

pub fn fbm_field(metrics: HeightfieldMetrics, p: &FbmParams) -> Heightfield {
    fill_world(metrics, |x, z| fbm(p.noise, x, z, &p.base))
}

pub fn ridged_field(metrics: HeightfieldMetrics, p: &FbmParams) -> Heightfield {
    fill_world(metrics, |x, z| ridged_mf(p.noise, x, z, &p.base))
}

pub fn domain_warp_field(metrics: HeightfieldMetrics, p: &DomainWarpParams) -> Heightfield {
    fill_world(metrics, |x, z| {
        domain_warp_fbm(x, z, &p.base, p.warp_strength, p.warp_frequency)
    })
}

pub fn terrace(input: &Heightfield, p: &TerraceParams) -> Heightfield {
    let (min_h, max_h) = input.min_max();
    let levels = p.levels.max(2) as f32;
    let mut out = input.clone();
    let sharpness = p.sharpness.clamp(0.0, 1.0);
    out.map_mut(|h| {
        let t = ((h - min_h) / (max_h - min_h).max(1e-6)).clamp(0.0, 1.0);
        let stepped = (t * levels).floor() / (levels - 1.0).max(1.0);
        let mixed = t * (1.0 - sharpness) + stepped * sharpness;
        min_h + mixed * (max_h - min_h)
    });
    out
}

pub fn plateau(input: &Heightfield, p: &PlateauParams) -> Heightfield {
    let mut out = input.clone();
    let soft = p.soft.max(1e-3);
    out.map_mut(|h| {
        if h < p.low {
            let t = ((h - (p.low - soft)) / soft).clamp(0.0, 1.0);
            (p.low - soft) + t * soft
        } else if h > p.high {
            let t = ((h - p.high) / soft).clamp(0.0, 1.0);
            p.high + t * soft * 0.25
        } else {
            // Flatten toward mid plateau.
            let mid = (p.low + p.high) * 0.5;
            h * 0.25 + mid * 0.75
        }
    });
    out
}

pub fn mountains(metrics: HeightfieldMetrics, p: &MountainParams) -> Heightfield {
    fill_world(metrics, |x, z| {
        let nx = x / metrics.world_size_x - 0.5;
        let nz = z / metrics.world_size_z - 0.5;
        let axis = nx * p.range_angle.cos() + nz * p.range_angle.sin();
        let cross_axis = nx * (-p.range_angle.sin()) + nz * p.range_angle.cos();
        let dist = cross_axis.abs();
        let range_mask = (1.0 - (dist / p.range_width.max(1e-3)))
            .clamp(0.0, 1.0)
            .powf(1.5);
        // Sample in range-aligned space so `range_angle` controls both the
        // enclosing ridge corridor and the ridge variation within it.
        let ridge_x = axis * metrics.world_size_x;
        let ridge_z = cross_axis * metrics.world_size_z;
        let amplitude = p.base.amplitude.max(0.0);
        let ridges = (ridged_mf(FractalNoiseType::Perlin, ridge_x, ridge_z, &p.base)
            / amplitude.max(1e-6))
        .clamp(0.0, 1.0);
        let shaped = ridges.powf(p.ridge_sharpness.max(0.1)) * amplitude;
        shaped * range_mask
    })
}

pub fn dunes(metrics: HeightfieldMetrics, p: &DuneParams) -> Heightfield {
    fill_world(metrics, |x, z| {
        let base = fbm(FractalNoiseType::Perlin, x, z, &p.base);
        let phase =
            (x * p.wave_frequency).rem_euclid(std::f32::consts::TAU) / std::f32::consts::TAU;
        let asymmetry = p.asymmetry.clamp(0.0, 1.0);
        // The windward half rises gradually while the leeward slip-face falls
        // sharply, producing a directional dune profile along the X wind axis.
        let wave = if phase < 0.5 {
            0.5 * (phase * 2.0).powf(1.0 / (1.0 + asymmetry))
        } else {
            0.5 + 0.5 * ((phase - 0.5) * 2.0).powf(1.0 + asymmetry)
        };
        base * (0.35 + 0.65 * wave)
    })
}

pub fn canyons(metrics: HeightfieldMetrics, p: &CanyonParams) -> Heightfield {
    fill_world(metrics, |x, z| {
        let broad = noise::perlin2(z * 0.0015, p.seed as f32 * 0.017, p.seed);
        let detail = noise::perlin2(z * 0.0045, p.seed as f32 * 0.043, p.seed ^ 0x9E37);
        let meander = broad * 0.75 + detail * 0.25;
        let center = metrics.world_size_x * 0.5 + meander * p.meander * p.width * 1.75;
        let d = (x - center).abs();
        // A linear cross-section produces a pronounced V-shaped canyon wall.
        let carve = 1.0 - (d / p.width.max(1e-3)).clamp(0.0, 1.0);
        -p.depth * carve
    })
}

pub fn voronoi_regions(metrics: HeightfieldMetrics, p: &VoronoiParams) -> Heightfield {
    fill_world(metrics, |x, z| {
        let w = sample_worley(
            x,
            z,
            &WorleyParams {
                base: p.base.clone(),
                distance_metric: WorleyMetric::Euclidean,
                feature: WorleyFeature::F1,
            },
        );
        let cell = noise::value_noise2(
            (x + p.base.offset_x) * p.base.frequency,
            (z + p.base.offset_z) * p.base.frequency,
            p.base.seed ^ 0xBEEF,
        );
        w * 0.25 + cell * p.height_per_cell * p.cell_jitter
    })
}

pub fn import_heightmap(
    metrics: HeightfieldMetrics,
    p: &ImportHeightmapParams,
) -> Result<Heightfield, EvalError> {
    if p.path.is_empty() {
        return Ok(Heightfield::zeros(metrics));
    }
    let img = image::open(&p.path).map_err(|e| EvalError::Io(e.to_string()))?;
    let g = img.to_luma16();
    let (iw, ih) = g.dimensions();
    let mut hf = Heightfield::zeros(metrics);
    for j in 0..metrics.height {
        for i in 0..metrics.width {
            let u = i as f32 / metrics.width as f32;
            let v = j as f32 / metrics.height as f32;
            let x = ((u * iw as f32) as u32).min(iw - 1);
            let y = ((v * ih as f32) as u32).min(ih - 1);
            let pix = g.get_pixel(x, y).0[0] as f32 / 65535.0;
            hf.set(i, j, pix * p.height_scale + p.height_offset);
        }
    }
    Ok(hf)
}

pub fn blur(input: &Heightfield, p: &BlurParams) -> Heightfield {
    let mut out = input.clone();
    let r = p.radius as i32;
    for _ in 0..p.iterations.max(1) {
        let src = out.to_dense();
        let w = out.metrics.width;
        let h = out.metrics.height;
        for j in 0..h {
            for i in 0..w {
                let mut sum: f32 = 0.0;
                let mut c: f32 = 0.0;
                for dj in -r..=r {
                    for di in -r..=r {
                        let ii = i as i32 + di;
                        let jj = j as i32 + dj;
                        if ii >= 0 && jj >= 0 && ii < w as i32 && jj < h as i32 {
                            sum += src[(jj as u32 * w + ii as u32) as usize];
                            c += 1.0;
                        }
                    }
                }
                out.set(i, j, sum / c.max(1.0));
            }
        }
    }
    out
}

pub fn coastal(input: &Heightfield, p: &CoastalParams) -> Heightfield {
    let mut out = input.clone();
    out.map_mut(|h| {
        if p.flatten_below && h < p.sea_level {
            if p.shelf_depth <= 0.0 {
                p.sea_level
            } else {
                let depth = p.sea_level - h;
                let slope_scale = p.beach_width.max(1e-3);
                p.sea_level - p.shelf_depth * (1.0 - (-depth / slope_scale).exp())
            }
        } else if h < p.sea_level + p.beach_width {
            let t = ((h - p.sea_level) / p.beach_width.max(1e-3)).clamp(0.0, 1.0);
            // Smoothstep keeps the beach tangent gentle at sea level and inland.
            let soft = t * t * (3.0 - 2.0 * t);
            p.sea_level + soft * p.beach_width
        } else {
            h
        }
    });
    out
}

fn fill_world<F: Fn(f32, f32) -> f32 + Sync>(metrics: HeightfieldMetrics, f: F) -> Heightfield {
    let w = metrics.width;
    let h = metrics.height;
    let mut data = vec![0.0f32; (w * h) as usize];
    data.par_iter_mut().enumerate().for_each(|(idx, v)| {
        let i = (idx as u32) % w;
        let j = (idx as u32) / w;
        let x = metrics.world_x(i);
        let z = metrics.world_z(j);
        *v = f(x, z);
    });
    Heightfield::from_dense(metrics, &data)
}
