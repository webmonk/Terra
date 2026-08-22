//! General prop / object placement (`LayerKind::ScatterObjects`).
//!
//! Generalises the vegetation scatter to an arbitrary list of object classes.
//! Every placement is a pure function of `(heightfield, params, coverage,
//! exclusion)`: the site sampler runs off a single seeded stream and every
//! per-instance attribute comes from a hash of `(seed, site index, salt)`.
//! No wall-clock, no thread-local RNG, no iteration over a hash map - the
//! same inputs always produce byte-identical instances.

use crate::heightfield::Heightfield;
use crate::layer::{ObjectClass, ObjectInstance, ScatterObjectsParams};
use crate::mask::MaskField;

/// Placement result: instances plus the two aux channels the layer publishes.
#[derive(Debug, Clone)]
pub struct ScatterObjectsOutput {
    pub instances: Vec<ObjectInstance>,
    /// Placement density proxy in \[0,1\] (the renderer / export scatter channel).
    pub density: MaskField,
    /// Where props *could* go: coverage minus exclusion, before placement.
    pub candidates: MaskField,
}

/// Upper bound on candidate sites so a large world with tight spacing cannot
/// turn an interactive edit into a multi-second Poisson flood fill. Tight
/// spacing is widened toward this budget rather than honoured literally: a
/// 4096 m world at 3 m spacing would otherwise flood-fill ~1.2M points and
/// cost seconds per edit, which is what the vegetation path used to do.
const MAX_SITES: f32 = 120_000.0;

/// Place props for one [`ScatterObjectsParams`].
///
/// `coverage` and `exclusion` are pre-baked distributions (`None` = absent,
/// i.e. coverage 1 / exclusion 0). Suitability is `coverage * (1 - exclusion)`
/// and a site is only considered where it is positive, so an exclusion mask at
/// full strength removes placements outright.
pub fn scatter_objects(
    hf: &Heightfield,
    p: &ScatterObjectsParams,
    coverage: Option<&MaskField>,
    exclusion: Option<&MaskField>,
) -> ScatterObjectsOutput {
    let metrics = hf.metrics;
    let slope = crate::analyze::slope_degrees(hf);

    let mut candidates = MaskField::zeros(metrics);
    for j in 0..metrics.height {
        for i in 0..metrics.width {
            let cov = coverage.map(|c| c.get(i, j)).unwrap_or(1.0);
            let exc = exclusion.map(|e| e.get(i, j)).unwrap_or(0.0);
            candidates.set(i, j, (cov * (1.0 - exc)).clamp(0.0, 1.0));
        }
    }

    let classes: Vec<(u32, &ObjectClass)> = p
        .classes
        .iter()
        .enumerate()
        .filter(|(_, c)| c.enabled && c.weight > 0.0 && c.density > 0.0)
        .map(|(i, c)| (i as u32, c))
        .collect();
    if classes.is_empty() {
        return ScatterObjectsOutput {
            instances: Vec::new(),
            density: MaskField::zeros(metrics),
            candidates,
        };
    }

    // Site sampler: the tightest authored spacing, clamped so the flood fill
    // stays bounded on large worlds.
    let tightest = classes
        .iter()
        .map(|(_, c)| c.min_spacing_m.max(0.25))
        .fold(f32::MAX, f32::min);
    let area = (metrics.world_size_x * metrics.world_size_z).max(1.0);
    let site_spacing = tightest.max((area / MAX_SITES).sqrt());
    let site_density = classes
        .iter()
        .map(|(_, c)| c.density.clamp(0.0, 1.0))
        .fold(0.0_f32, f32::max);

    let mut accept = |x: f32, z: f32| {
        let (i, j) = metrics.sample_index(x, z);
        candidates.get(i, j) > 0.0
    };
    let sites = super::poisson_disk_filtered(
        metrics,
        site_spacing,
        site_density,
        p.seed,
        &mut accept,
    );

    // Per-class occupancy grids enforce each class's own min spacing.
    let mut occupancy: Vec<SpacingGrid> = classes
        .iter()
        .map(|(_, c)| SpacingGrid::new(metrics, c.min_spacing_m.max(0.25)))
        .collect();

    let mut instances = Vec::new();
    for (n, (x, z)) in sites.iter().copied().enumerate() {
        let (i, j) = metrics.sample_index(x, z);
        let suit = candidates.get(i, j);
        if suit <= 0.0 {
            continue;
        }
        let y = hf.get(i, j);
        let slope_deg = slope.get(i, j) * 90.0;
        let site = n as u64;

        // Eligible classes at this site, weighted.
        let mut total = 0.0_f32;
        let mut eligible: Vec<(usize, f32)> = Vec::new();
        for (slot, (_, class)) in classes.iter().enumerate() {
            if slope_deg > class.max_slope_deg {
                continue;
            }
            if y < class.height_range[0] || y > class.height_range[1] {
                continue;
            }
            total += class.weight;
            eligible.push((slot, total));
        }
        if eligible.is_empty() {
            continue;
        }

        let pick = rand_unit(p.seed, site, SALT_CLASS) * total;
        let slot = eligible
            .iter()
            .find(|(_, cumulative)| pick < *cumulative)
            .map(|(slot, _)| *slot)
            .unwrap_or(eligible[eligible.len() - 1].0);
        let (class_index, class) = classes[slot];

        // Density thins the accepted sites; suitability modulates it.
        if rand_unit(p.seed, site, SALT_ACCEPT) >= class.density.clamp(0.0, 1.0) * suit {
            continue;
        }
        if !occupancy[slot].try_place(x, z) {
            continue;
        }

        let t = rand_unit(p.seed, site, SALT_SCALE);
        let (s0, s1) = (class.scale_range[0], class.scale_range[1]);
        let scale = (s0 + (s1 - s0) * t).max(0.001);
        let yaw = (rand_unit(p.seed, site, SALT_YAW) * 2.0 - 1.0)
            * class.yaw_jitter_deg.clamp(0.0, 180.0).to_radians();
        let normal = if class.align_to_normal {
            surface_normal(hf, i, j)
        } else {
            [0.0, 1.0, 0.0]
        };

        instances.push(ObjectInstance {
            class_index,
            class: class.name.clone(),
            x,
            y,
            z,
            scale,
            yaw_rad: yaw,
            normal,
        });
    }

    // Density proxy: splat placements, soften, and gate by suitability so the
    // published channel reads like the vegetation one.
    let mut density = MaskField::zeros(metrics);
    for inst in &instances {
        let (i, j) = metrics.sample_index(inst.x, inst.z);
        density.set(i, j, 1.0);
    }
    let blur_r = if metrics.width <= 512 { 2 } else { 1 };
    crate::mask::apply_mask_ops(&mut density, &[crate::mask::MaskOp::Blur { radius: blur_r }]);
    for j in 0..metrics.height {
        for i in 0..metrics.width {
            let v = density.get(i, j) * candidates.get(i, j);
            density.set(i, j, v.clamp(0.0, 1.0));
        }
    }

    ScatterObjectsOutput {
        instances,
        density,
        candidates,
    }
}

const SALT_CLASS: u64 = 0x9E37_79B9_7F4A_7C15;
const SALT_ACCEPT: u64 = 0xC2B2_AE3D_27D4_EB4F;
const SALT_SCALE: u64 = 0x1656_67B1_9E37_79F9;
const SALT_YAW: u64 = 0xD6E8_FEB8_6659_FD93;

/// SplitMix64 finaliser over `(seed, site, salt)` -> \[0,1).
///
/// Hash-per-site rather than a running stream keeps attributes stable no
/// matter which sites get rejected earlier in the pass.
fn rand_unit(seed: u64, site: u64, salt: u64) -> f32 {
    let mut z = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(site.wrapping_mul(0xBF58_476D_1CE4_E5B9))
        ^ salt;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 40) as f32 / (1u64 << 24) as f32
}

fn surface_normal(hf: &Heightfield, i: u32, j: u32) -> [f32; 3] {
    let m = hf.metrics;
    let (w, h) = (m.width, m.height);
    let xm = hf.get(i.saturating_sub(1), j);
    let xp = hf.get((i + 1).min(w - 1), j);
    let zm = hf.get(i, j.saturating_sub(1));
    let zp = hf.get(i, (j + 1).min(h - 1));
    let dx = (xp - xm) / (2.0 * m.dx().max(1e-6));
    let dz = (zp - zm) / (2.0 * m.dz().max(1e-6));
    let len = (dx * dx + 1.0 + dz * dz).sqrt();
    [-dx / len, 1.0 / len, -dz / len]
}

/// Uniform bucket grid enforcing one class's minimum spacing.
struct SpacingGrid {
    cell: f32,
    gw: usize,
    gh: usize,
    min_sq: f32,
    buckets: Vec<Vec<(f32, f32)>>,
}

impl SpacingGrid {
    fn new(metrics: crate::heightfield::HeightfieldMetrics, min_spacing: f32) -> Self {
        let cell = min_spacing.max(0.25);
        let gw = ((metrics.world_size_x / cell).ceil() as usize).max(1);
        let gh = ((metrics.world_size_z / cell).ceil() as usize).max(1);
        Self {
            cell,
            gw,
            gh,
            min_sq: cell * cell,
            buckets: vec![Vec::new(); gw * gh],
        }
    }

    fn try_place(&mut self, x: f32, z: f32) -> bool {
        let gx = (x / self.cell).floor() as isize;
        let gz = (z / self.cell).floor() as isize;
        for dj in -1..=1 {
            for di in -1..=1 {
                let (cx, cz) = (gx + di, gz + dj);
                if cx < 0 || cz < 0 || cx >= self.gw as isize || cz >= self.gh as isize {
                    continue;
                }
                for &(px, pz) in &self.buckets[cz as usize * self.gw + cx as usize] {
                    let (dx, dz) = (px - x, pz - z);
                    if dx * dx + dz * dz < self.min_sq {
                        return false;
                    }
                }
            }
        }
        if gx >= 0 && gz >= 0 && (gx as usize) < self.gw && (gz as usize) < self.gh {
            self.buckets[gz as usize * self.gw + gx as usize].push((x, z));
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heightfield::HeightfieldMetrics;
    use crate::layer::ObjectClass;

    fn ramp(width: u32, world: f32) -> Heightfield {
        let m = HeightfieldMetrics::new(width, width, world, world);
        let mut hf = Heightfield::zeros(m);
        // Flat left half, steep right half, so the slope filter has a boundary.
        for j in 0..width {
            for i in 0..width {
                let t = i as f32 / (width - 1) as f32;
                let steep = if t > 0.5 { (t - 0.5) * 400.0 } else { 0.0 };
                hf.set(i, j, steep);
            }
        }
        hf.refresh_halos();
        hf
    }

    fn two_classes() -> ScatterObjectsParams {
        ScatterObjectsParams {
            seed: 1234,
            classes: vec![
                ObjectClass {
                    name: "Boulder".into(),
                    min_spacing_m: 6.0,
                    ..ObjectClass::default()
                },
                ObjectClass {
                    name: "Crate".into(),
                    weight: 0.5,
                    min_spacing_m: 4.0,
                    ..ObjectClass::default()
                },
            ],
            ..ScatterObjectsParams::default()
        }
    }

    #[test]
    fn same_seed_same_placements() {
        let hf = ramp(64, 256.0);
        let p = two_classes();
        let a = scatter_objects(&hf, &p, None, None);
        let b = scatter_objects(&hf, &p, None, None);
        assert!(!a.instances.is_empty());
        assert_eq!(a.instances, b.instances);
    }

    #[test]
    fn different_seed_moves_placements() {
        let hf = ramp(64, 256.0);
        let mut p = two_classes();
        let a = scatter_objects(&hf, &p, None, None);
        p.seed = 4321;
        let b = scatter_objects(&hf, &p, None, None);
        assert_ne!(a.instances, b.instances);
    }

    #[test]
    fn exclusion_removes_placements() {
        let hf = ramp(64, 256.0);
        let p = two_classes();
        let metrics = hf.metrics;
        // Exclude the left half of the world outright.
        let mut exclusion = MaskField::zeros(metrics);
        for j in 0..metrics.height {
            for i in 0..metrics.width / 2 {
                exclusion.set(i, j, 1.0);
            }
        }
        let open = scatter_objects(&hf, &p, None, None);
        let gated = scatter_objects(&hf, &p, None, Some(&exclusion));
        assert!(gated.instances.len() < open.instances.len());
        let half = metrics.world_size_x * 0.5;
        assert!(
            gated.instances.iter().all(|inst| inst.x >= half),
            "excluded region still received props"
        );
        // Full exclusion places nothing at all.
        let all = MaskField::filled(metrics, 1.0);
        assert!(scatter_objects(&hf, &p, None, Some(&all)).instances.is_empty());
    }

    #[test]
    fn per_class_slope_filter_is_respected() {
        let hf = ramp(64, 256.0);
        let slope = crate::analyze::slope_degrees(&hf);
        let p = ScatterObjectsParams {
            seed: 7,
            classes: vec![
                ObjectClass {
                    name: "Flat only".into(),
                    max_slope_deg: 5.0,
                    min_spacing_m: 5.0,
                    ..ObjectClass::default()
                },
                ObjectClass {
                    name: "Any slope".into(),
                    max_slope_deg: 90.0,
                    min_spacing_m: 5.0,
                    ..ObjectClass::default()
                },
            ],
            ..ScatterObjectsParams::default()
        };
        let out = scatter_objects(&hf, &p, None, None);
        assert!(out.instances.iter().any(|i| i.class_index == 0));
        assert!(out.instances.iter().any(|i| i.class_index == 1));
        for inst in &out.instances {
            let (i, j) = hf.metrics.sample_index(inst.x, inst.z);
            let deg = slope.get(i, j) * 90.0;
            let limit = p.classes[inst.class_index as usize].max_slope_deg;
            assert!(
                deg <= limit + 1e-3,
                "{} placed on {deg} deg (limit {limit})",
                inst.class
            );
        }
    }

    #[test]
    fn per_class_spacing_is_respected() {
        let hf = ramp(64, 256.0);
        let p = two_classes();
        let out = scatter_objects(&hf, &p, None, None);
        for a in 0..out.instances.len() {
            for b in (a + 1)..out.instances.len() {
                let (x, y) = (&out.instances[a], &out.instances[b]);
                if x.class_index != y.class_index {
                    continue;
                }
                let spacing = p.classes[x.class_index as usize].min_spacing_m;
                let (dx, dz) = (x.x - y.x, x.z - y.z);
                assert!(dx * dx + dz * dz >= spacing * spacing - 1e-3);
            }
        }
    }

    #[test]
    fn height_range_gates_classes() {
        let hf = ramp(64, 256.0);
        let p = ScatterObjectsParams {
            seed: 21,
            classes: vec![ObjectClass {
                name: "Highland".into(),
                height_range: [50.0, 1_000.0],
                max_slope_deg: 90.0,
                min_spacing_m: 5.0,
                ..ObjectClass::default()
            }],
            ..ScatterObjectsParams::default()
        };
        let out = scatter_objects(&hf, &p, None, None);
        assert!(!out.instances.is_empty());
        assert!(out.instances.iter().all(|i| i.y >= 50.0));
    }

    #[test]
    fn align_to_normal_tilts_only_when_enabled() {
        let hf = ramp(64, 256.0);
        let p = ScatterObjectsParams {
            seed: 3,
            classes: vec![
                ObjectClass {
                    name: "Aligned".into(),
                    align_to_normal: true,
                    max_slope_deg: 90.0,
                    min_spacing_m: 5.0,
                    ..ObjectClass::default()
                },
                ObjectClass {
                    name: "Upright".into(),
                    align_to_normal: false,
                    max_slope_deg: 90.0,
                    min_spacing_m: 5.0,
                    ..ObjectClass::default()
                },
            ],
            ..ScatterObjectsParams::default()
        };
        let out = scatter_objects(&hf, &p, None, None);
        assert!(out
            .instances
            .iter()
            .filter(|i| i.class_index == 1)
            .all(|i| i.normal == [0.0, 1.0, 0.0]));
        assert!(out
            .instances
            .iter()
            .filter(|i| i.class_index == 0)
            .any(|i| i.normal != [0.0, 1.0, 0.0]));
    }

    #[test]
    fn empty_class_list_places_nothing() {
        let hf = ramp(32, 128.0);
        let p = ScatterObjectsParams {
            classes: Vec::new(),
            ..ScatterObjectsParams::default()
        };
        let out = scatter_objects(&hf, &p, None, None);
        assert!(out.instances.is_empty());
        assert!(out.density.data().iter().all(|v| *v == 0.0));
    }
}
