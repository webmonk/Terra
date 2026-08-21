//! Fast/analytical landscape-evolution solver must not blow up low-drainage
//! cells (ridges / peaks) where the steady-state stream-power relief term
//! `U / (K A^m)` diverges as drainage area `A -> 0`.
//!
//! Regression for the Tropical Island preset where a ~628 m peak grew to
//! ~5.9 km in a single operator application.

use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::landscape_evolution::{
    EvolutionSolverMode, LandscapeEvolutionInput, LandscapeEvolutionOperator,
    LandscapeEvolutionParams,
};

const RES: u32 = 128;
const WORLD: f32 = 4000.0;
const PEAK: f32 = 628.0;

/// Smooth cone island: peak in the centre, shore (0 m) at ~80% of the half-width.
fn cone_island() -> Heightfield {
    let m = HeightfieldMetrics::new(RES, RES, WORLD, WORLD);
    let mut data = vec![0.0f32; (RES * RES) as usize];
    let c = (RES as f32 - 1.0) * 0.5;
    let shore_r = c * 0.8;
    for j in 0..RES {
        for i in 0..RES {
            let dx = i as f32 - c;
            let dy = j as f32 - c;
            let r = (dx * dx + dy * dy).sqrt();
            let h = PEAK * (1.0 - r / shore_r);
            data[(j * RES + i) as usize] = h.max(0.0);
        }
    }
    Heightfield::from_dense(m, &data)
}

/// Deterministic value-noise fbm island: rough ridges with tiny drainage
/// areas superimposed on the cone envelope. No wall-clock or RNG state -
/// the hash is a pure function of the lattice coordinates.
fn fbm_island() -> Heightfield {
    fn hash(ix: i32, iy: i32) -> f32 {
        let mut h = (ix as u32).wrapping_mul(374761393) ^ (iy as u32).wrapping_mul(668265263);
        h = (h ^ (h >> 13)).wrapping_mul(1274126177);
        (h ^ (h >> 16)) as f32 / u32::MAX as f32
    }
    fn value_noise(x: f32, y: f32) -> f32 {
        let ix = x.floor() as i32;
        let iy = y.floor() as i32;
        let fx = x - ix as f32;
        let fy = y - iy as f32;
        let sx = fx * fx * (3.0 - 2.0 * fx);
        let sy = fy * fy * (3.0 - 2.0 * fy);
        let v00 = hash(ix, iy);
        let v10 = hash(ix + 1, iy);
        let v01 = hash(ix, iy + 1);
        let v11 = hash(ix + 1, iy + 1);
        let a = v00 + (v10 - v00) * sx;
        let b = v01 + (v11 - v01) * sx;
        a + (b - a) * sy
    }
    let m = HeightfieldMetrics::new(RES, RES, WORLD, WORLD);
    let mut data = vec![0.0f32; (RES * RES) as usize];
    let c = (RES as f32 - 1.0) * 0.5;
    let shore_r = c * 0.8;
    for j in 0..RES {
        for i in 0..RES {
            let dx = i as f32 - c;
            let dy = j as f32 - c;
            let r = (dx * dx + dy * dy).sqrt();
            let envelope = (1.0 - r / shore_r).max(0.0);
            let mut fbm = 0.0f32;
            let mut amp = 0.5f32;
            let mut freq = 4.0f32 / RES as f32;
            for _ in 0..5 {
                fbm += amp * value_noise(i as f32 * freq, j as f32 * freq);
                amp *= 0.5;
                freq *= 2.0;
            }
            let h = PEAK * envelope * (0.55 + 0.45 * fbm);
            data[(j * RES + i) as usize] = if envelope > 0.0 { h.max(0.0) } else { 0.0 };
        }
    }
    Heightfield::from_dense(m, &data)
}

fn moderate_params() -> LandscapeEvolutionParams {
    LandscapeEvolutionParams {
        solver: EvolutionSolverMode::Fast,
        // Moderate artist uplift; everything else at defaults (which include
        // the hillslope/thermal companion that previously masked the worst
        // of the ridge singularity without removing it).
        uplift: 0.6,
        ..LandscapeEvolutionParams::default()
    }
}

fn run_fast(input: &Heightfield, p: &LandscapeEvolutionParams) -> Heightfield {
    let op = LandscapeEvolutionOperator::new(p.clone());
    op.evaluate(LandscapeEvolutionInput {
        elevation: input,
        painted_uplift: None,
        precipitation: None,
        erodibility: None,
        lithology_hardness: None,
        outlet_mask: None,
        protection: None,
    })
    .elevation
}

/// Bound on how much the surface may rise above the input maximum in one
/// operator application.
///
/// The solver caps the steady-state stream-power gradient `U/(K A^m)` at the
/// talus angle, so no cell can stand higher than its outlet by more than
/// `tan(talus) * longest_flow_path`, and the finite-horizon envelope limits
/// total gain to the accumulated uplift `U_max * t`. For a moderate-uplift
/// single application the peak must stay well under 2x the input relief plus
/// the uplift budget. (The harshest divergence fixtures live in the
/// `*_do_not_diverge` tests below and use the tighter `in_max + budget` bound.)
fn allowed_max(input_max: f32, p: &LandscapeEvolutionParams) -> f32 {
    let uplift_budget = p.peak_uplift_rate() * p.evolution_time();
    // The operator seeds the tectonic base with a short uplift impulse and
    // blends toward it, so allow the budget once plus modest headroom on the
    // input relief for drainage reorganisation.
    input_max * 2.0 + uplift_budget
}

#[test]
fn fast_solver_peak_stays_bounded_on_cone_island() {
    let input = cone_island();
    let (_, in_max) = input.min_max();
    let p = moderate_params();
    let out = run_fast(&input, &p);
    let (out_min, out_max) = out.min_max();
    let bound = allowed_max(in_max, &p);
    println!(
        "cone island: in_max={in_max:.1} m -> out=[{out_min:.1}, {out_max:.1}] m (bound {bound:.1} m, uplift budget {:.1} m over t={:.0} yr)",
        p.peak_uplift_rate() * p.evolution_time(),
        p.evolution_time()
    );
    assert!(out_max.is_finite(), "non-finite output elevation");
    assert!(
        out_max <= bound,
        "Fast solver overshoot: input peak {in_max:.1} m grew to {out_max:.1} m (allowed {bound:.1} m)"
    );
}

#[test]
fn fast_solver_peak_stays_bounded_without_hillslope_masking() {
    // The thermal/hillslope companion previously hid part of the ridge
    // singularity; the raw analytical solve must be bounded on its own.
    let input = cone_island();
    let (_, in_max) = input.min_max();
    let mut p = moderate_params();
    p.hillslope_diffusion = 0.0;
    let out = run_fast(&input, &p);
    let (out_min, out_max) = out.min_max();
    let bound = allowed_max(in_max, &p);
    println!(
        "cone island (no hillslope): in_max={in_max:.1} m -> out=[{out_min:.1}, {out_max:.1}] m (bound {bound:.1} m)"
    );
    assert!(out_max.is_finite(), "non-finite output elevation");
    assert!(
        out_max <= bound,
        "Fast solver overshoot (no hillslope): input peak {in_max:.1} m grew to {out_max:.1} m (allowed {bound:.1} m)"
    );
}

#[test]
fn fast_solver_still_erodes_well_drained_terrain() {
    // The fix must only touch the divergent low-drainage limit; the solver
    // still has to carve visible relief change on a well-drained island.
    let input = cone_island();
    let p = moderate_params();
    let out = run_fast(&input, &p);
    let mut changed = 0usize;
    let mut n_land = 0usize;
    for j in 0..RES {
        for i in 0..RES {
            let a = input.get(i, j);
            if a <= 0.5 {
                continue;
            }
            n_land += 1;
            if (out.get(i, j) - a).abs() > 1.0 {
                changed += 1;
            }
        }
    }
    assert!(
        changed as f32 > n_land as f32 * 0.25,
        "solver became a no-op: only {changed}/{n_land} land cells changed by >1 m"
    );
}

/// Old, weakly eroding island: `K A^m` is small on ridge cells, so before the
/// talus-gradient cap the `U / (K A^m)` steady-state term (and the
/// downstream-consistency floor it feeds) diverged. On this fixture the cone
/// peak grew from 619 m to 1853 m and the fbm island from 430 m to 1613 m -
/// past even the input maximum plus the full accumulated uplift `U_max * t`,
/// which is the hard physical ceiling asserted here.
#[test]
fn fast_solver_low_drainage_ridges_do_not_diverge() {
    let p = LandscapeEvolutionParams {
        solver: EvolutionSolverMode::Fast,
        geological_age: 1.0,
        erosion: 0.25,
        uplift: 0.65,
        rainfall: 0.5,
        drainage_scale: 0.2,
        hillslope_diffusion: 0.0,
        ..LandscapeEvolutionParams::default()
    };
    let uplift_budget = p.peak_uplift_rate() * p.evolution_time();
    for (name, input) in [("cone", cone_island()), ("fbm", fbm_island())] {
        let (_, in_max) = input.min_max();
        let out = run_fast(&input, &p);
        let (out_min, out_max) = out.min_max();
        let bound = in_max + uplift_budget;
        println!(
            "{name}: in_max={in_max:.1} m -> out=[{out_min:.1}, {out_max:.1}] m (bound {bound:.1} m, budget {uplift_budget:.1} m)"
        );
        assert!(out_max.is_finite(), "{name}: non-finite output");
        assert!(
            out_max <= bound,
            "{name}: ridge divergence - peak {in_max:.1} m grew to {out_max:.1} m, \
             exceeding input max + accumulated uplift ({bound:.1} m)"
        );
    }
}

/// Highly resistant rock shrinks the erodibility `K` (via softness), which
/// also diverges `U / (K A^m)`. Before the cap the cone peak reached 1947 m
/// (3.1x the 619 m input, above input max + accumulated uplift of 1771 m).
#[test]
fn fast_solver_resistant_rock_does_not_diverge() {
    let p = LandscapeEvolutionParams {
        solver: EvolutionSolverMode::Fast,
        geological_age: 0.8,
        erosion: 0.6,
        uplift: 0.8,
        terrain_resistance: 0.85,
        hillslope_diffusion: 0.0,
        ..LandscapeEvolutionParams::default()
    };
    let input = cone_island();
    let (_, in_max) = input.min_max();
    let out = run_fast(&input, &p);
    let (out_min, out_max) = out.min_max();
    let bound = in_max + p.peak_uplift_rate() * p.evolution_time();
    println!(
        "resistant rock: in_max={in_max:.1} m -> out=[{out_min:.1}, {out_max:.1}] m (bound {bound:.1} m)"
    );
    assert!(out_max.is_finite(), "non-finite output");
    assert!(
        out_max <= bound,
        "resistant-rock divergence: peak {in_max:.1} m grew to {out_max:.1} m (allowed {bound:.1} m)"
    );
}

/// Tropical Island preset parameters (post sea-level-datum fix) on a rough
/// island: the well-drained large-scale output the preset depends on must not
/// blow up either, and land must keep eroding.
#[test]
fn fast_solver_tropical_preset_params_stay_bounded() {
    let p = LandscapeEvolutionParams {
        solver: EvolutionSolverMode::Fast,
        iterations: 28,
        uplift_rate: 0.028,
        incision_k: 0.00042,
        area_exponent: 0.52,
        slope_exponent: 1.05,
        hillslope_diffusion: 0.22,
        talus_angle_deg: 33.0,
        sediment_transport: 0.48,
        constraint_preservation: 0.82,
        base_level: 0.0,
        use_dinfinity: true,
        geological_age: 0.55,
        rainfall: 1.8,
        erosion: 0.75,
        uplift: 0.65,
        river_incision: 0.7,
        drainage_scale: 0.7,
        ..LandscapeEvolutionParams::default()
    };
    let input = fbm_island();
    let (_, in_max) = input.min_max();
    let out = run_fast(&input, &p);
    let (out_min, out_max) = out.min_max();
    let bound = in_max + p.peak_uplift_rate() * p.evolution_time();
    println!(
        "tropical params: in_max={in_max:.1} m -> out=[{out_min:.1}, {out_max:.1}] m (bound {bound:.1} m)"
    );
    assert!(out_max.is_finite(), "non-finite output");
    assert!(out_max <= bound, "tropical params overshoot: {out_max:.1} m > {bound:.1} m");
    assert!(
        out_max > 50.0,
        "tropical params erased the island entirely (max {out_max:.1} m)"
    );
}
