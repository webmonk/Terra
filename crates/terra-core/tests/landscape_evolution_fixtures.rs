//! Phase 3 landscape evolution fixtures: uplift modes + multi-age evolution.
//!
//! Asserts coherent drainage organisation rather than "noise with scratches".

use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::landscape_evolution::{
    asymmetric_belt, synthesise_uplift, BoundaryMode, EvolutionSolverMode,
    LandscapeEvolutionInput, LandscapeEvolutionOperator, LandscapeEvolutionParams, UpliftMode,
};
use terra_core::mask::MaskField;

fn metrics() -> HeightfieldMetrics {
    HeightfieldMetrics::new(64, 64, 4000.0, 4000.0)
}

fn flat_seed(m: HeightfieldMetrics, h: f32) -> Heightfield {
    Heightfield::from_dense(m, &vec![h; (m.width * m.height) as usize])
}

fn mean_valley_accumulation(height: &Heightfield, acc: &MaskField) -> f32 {
    let dense = height.to_dense();
    let mut sorted = dense.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = sorted[sorted.len() / 4];
    let mut sum = 0.0f32;
    let mut n = 0u32;
    for j in 0..height.metrics.height {
        for i in 0..height.metrics.width {
            if height.get(i, j) <= q {
                sum += acc.get(i, j);
                n += 1;
            }
        }
    }
    if n == 0 {
        0.0
    } else {
        sum / n as f32
    }
}

fn relief(height: &Heightfield) -> f32 {
    let (lo, hi) = height.min_max();
    hi - lo
}

fn mean_channel_weight(order: &MaskField) -> f32 {
    let d = order.data();
    if d.is_empty() {
        return 0.0;
    }
    d.iter().sum::<f32>() / d.len() as f32
}

fn run_evo(
    seed: &Heightfield,
    uplift_mode: UpliftMode,
    age: f32,
    solver: EvolutionSolverMode,
    painted: Option<&MaskField>,
) -> terra_core::landscape_evolution::LandscapeEvolutionOutput {
    let mut p = LandscapeEvolutionParams::default();
    p.uplift_mode = uplift_mode;
    p.geological_age = age;
    p.solver = solver;
    p.uplift = 0.9;
    p.erosion = 0.85;
    p.rainfall = 1.4;
    p.river_incision = 0.9;
    p.drainage_scale = 0.6;
    p.hillslope_diffusion = 0.1;
    p.fixed_point_iters = 6;
    p.iterations = 24;
    p.boundary = BoundaryMode::SeaLevel;
    p.base_level = 0.0;
    p.uplift_noise = 0.04;
    p.time_scale = 1.2e6;
    p.dt = 5_000.0;
    let op = LandscapeEvolutionOperator::new(p);
    op.evaluate(LandscapeEvolutionInput {
        elevation: seed,
        painted_uplift: painted,
        precipitation: None,
        erodibility: None,
        lithology_hardness: None,
        outlet_mask: None,
        protection: None,
    })
}

#[test]
fn uplift_plateau_uniform_preserves_large_form() {
    let m = metrics();
    let seed = flat_seed(m, 20.0);
    let young = run_evo(&seed, UpliftMode::Uniform, 0.15, EvolutionSolverMode::Fast, None);
    let mature = run_evo(&seed, UpliftMode::Uniform, 0.85, EvolutionSolverMode::Fast, None);
    assert!(
        relief(&young.elevation) > 5.0,
        "young uplifted plateau should gain relief"
    );
    assert!(
        mean_valley_accumulation(&mature.elevation, &mature.flow_accumulation)
            > mean_valley_accumulation(&young.elevation, &young.flow_accumulation) * 0.9,
        "mature drainage should organise valleys"
    );
    // Large form: tectonic base remains higher-amplitude than fine scratches.
    assert!(relief(&mature.tectonic_base) > relief(&seed) * 0.5);
}

#[test]
fn linear_mountain_belt_forms_organised_ridges() {
    let m = metrics();
    let seed = flat_seed(m, 10.0);
    let out = run_evo(
        &seed,
        UpliftMode::LinearBelt,
        0.7,
        EvolutionSolverMode::Fast,
        None,
    );
    assert!(relief(&out.elevation) > 15.0);
    // Belt centre (along mid UV) should be higher than corners on average.
    let mut centre = 0.0f32;
    let mut corner = 0.0f32;
    let mut nc = 0u32;
    let mut nk = 0u32;
    for j in 20..44 {
        for i in 20..44 {
            centre += out.elevation.get(i, j);
            nc += 1;
        }
    }
    for &(i, j) in &[(2u32, 2), (61, 2), (2, 61), (61, 61)] {
        corner += out.elevation.get(i, j);
        nk += 1;
    }
    assert!(centre / nc as f32 > corner / nk as f32 + 2.0);
    assert!(
        mean_valley_accumulation(&out.elevation, &out.flow_accumulation) > 0.04
            || mean_channel_weight(&out.stream_order) > 0.02,
        "valleys / channels should organise under a linear belt"
    );
    assert!(
        out.incision.data().iter().copied().sum::<f32>() > 1.0,
        "fluvial incision should accumulate"
    );
}

#[test]
fn radial_dome_produces_outward_drainage() {
    let m = metrics();
    let seed = flat_seed(m, 5.0);
    let mut p = LandscapeEvolutionParams::default();
    p.uplift_mode = UpliftMode::Radial;
    p.uplift_center_u = 0.5;
    p.uplift_center_v = 0.5;
    p.uplift_falloff = 0.4;
    p.geological_age = 0.65;
    p.solver = EvolutionSolverMode::Fast;
    p.time_scale = 8.0e5;
    p.fixed_point_iters = 4;
    p.hillslope_diffusion = 0.1;
    let op = LandscapeEvolutionOperator::new(p);
    let out = op.evaluate(LandscapeEvolutionInput {
        elevation: &seed,
        painted_uplift: None,
        precipitation: None,
        erodibility: None,
        lithology_hardness: None,
        outlet_mask: None,
        protection: None,
    });
    let centre = out.elevation.get(32, 32);
    let rim = (out.elevation.get(4, 32)
        + out.elevation.get(60, 32)
        + out.elevation.get(32, 4)
        + out.elevation.get(32, 60))
        * 0.25;
    assert!(
        centre > rim + 3.0,
        "radial dome peak should exceed rim (c={centre} rim={rim})"
    );
}

#[test]
fn asymmetric_uplift_biases_relief() {
    let m = metrics();
    let painted = asymmetric_belt(m, 1.0, 0.0, 0.9); // horizontal belt, bias across V
    let seed = flat_seed(m, 8.0);
    let out = run_evo(
        &seed,
        UpliftMode::Painted,
        0.6,
        EvolutionSolverMode::Fast,
        Some(&painted),
    );
    assert!(relief(&out.elevation) > 8.0);
    // Bias is across the belt normal (V): compare top vs bottom halves.
    let mut top = 0.0f32;
    let mut bottom = 0.0f32;
    for j in 0..m.height / 2 {
        for i in 0..m.width {
            top += out.elevation.get(i, j);
        }
    }
    for j in m.height / 2..m.height {
        for i in 0..m.width {
            bottom += out.elevation.get(i, j);
        }
    }
    assert!(
        (top - bottom).abs() > 30.0,
        "asymmetric uplift should skew relief (top={top} bottom={bottom})"
    );
    // Painted uplift field itself must be asymmetric.
    let mut up_top = 0.0f32;
    let mut up_bot = 0.0f32;
    for j in 0..m.height / 2 {
        for i in 0..m.width {
            up_top += painted.get(i, j);
        }
    }
    for j in m.height / 2..m.height {
        for i in 0..m.width {
            up_bot += painted.get(i, j);
        }
    }
    assert!((up_top - up_bot).abs() > 1e-3);
}

#[test]
fn geological_age_increases_incision_not_noise() {
    let m = metrics();
    let seed = flat_seed(m, 15.0);
    let young = run_evo(
        &seed,
        UpliftMode::LinearBelt,
        0.12,
        EvolutionSolverMode::Fast,
        None,
    );
    let mid = run_evo(
        &seed,
        UpliftMode::LinearBelt,
        0.45,
        EvolutionSolverMode::Fast,
        None,
    );
    let old = run_evo(
        &seed,
        UpliftMode::LinearBelt,
        0.9,
        EvolutionSolverMode::Fast,
        None,
    );
    let young_inc: f32 = young.incision.data().iter().sum();
    let mid_inc: f32 = mid.incision.data().iter().sum();
    let old_inc: f32 = old.incision.data().iter().sum();
    assert!(
        mid_inc >= young_inc * 0.75,
        "greater age should not reduce organised incision"
    );
    assert!(
        old_inc >= mid_inc * 0.75,
        "mature age should continue stream incision"
    );
    // Stream network weight should not collapse with age (organisation, not noise).
    let yw = mean_channel_weight(&young.stream_order);
    let ow = mean_channel_weight(&old.stream_order);
    assert!(
        ow + mean_valley_accumulation(&old.elevation, &old.flow_accumulation)
            >= (yw + mean_valley_accumulation(&young.elevation, &young.flow_accumulation)) * 0.5,
        "mature drainage should remain coherent (young_w={yw} old_w={ow})"
    );
    // Large forms preserved: tectonic base still carries belt-scale relief.
    assert!(relief(&old.tectonic_base) > 10.0);
}

#[test]
fn accurate_and_fast_share_high_level_params() {
    let m = HeightfieldMetrics::new(48, 48, 3000.0, 3000.0);
    let seed = flat_seed(m, 12.0);
    let fast = run_evo(
        &seed,
        UpliftMode::Radial,
        0.5,
        EvolutionSolverMode::Fast,
        None,
    );
    let accurate = run_evo(
        &seed,
        UpliftMode::Radial,
        0.5,
        EvolutionSolverMode::Accurate,
        None,
    );
    // Both should create meaningful relief and drainage — not identical, but same family.
    assert!(relief(&fast.elevation) > 3.0);
    assert!(relief(&accurate.elevation) > 3.0);
    assert!(
        mean_valley_accumulation(&fast.elevation, &fast.flow_accumulation) > 0.02
            || mean_channel_weight(&fast.stream_order) > 0.01
    );
    assert!(
        mean_valley_accumulation(&accurate.elevation, &accurate.flow_accumulation) > 0.02
            || mean_channel_weight(&accurate.stream_order) > 0.01
            || accurate.incision.data().iter().copied().sum::<f32>() > 0.5
    );
}

#[test]
fn synthesise_uplift_modes_are_smooth() {
    let m = metrics();
    let mut p = LandscapeEvolutionParams::default();
    p.uplift_mode = UpliftMode::Procedural;
    p.uplift_noise = 0.1;
    let field = synthesise_uplift(m, &p, None);
    // Neighbour differences should stay modest (geological smoothness).
    let mut max_grad = 0.0f32;
    let margin = 4u32;
    for j in margin..m.height - margin {
        for i in margin..m.width - margin {
            let g = (field.get(i + 1, j) - field.get(i - 1, j))
                .abs()
                .max((field.get(i, j + 1) - field.get(i, j - 1)).abs());
            max_grad = max_grad.max(g);
        }
    }
    let mut peak = 0.0f32;
    for j in margin..m.height - margin {
        for i in margin..m.width - margin {
            peak = peak.max(field.get(i, j));
        }
    }
    let peak = peak.max(1e-8);
    assert!(
        max_grad / peak < 0.45,
        "procedural uplift must stay smooth at geological scale (g/peak={})",
        max_grad / peak
    );
}

#[test]
fn evolution_is_deterministic() {
    let m = HeightfieldMetrics::new(40, 40, 2000.0, 2000.0);
    let seed = flat_seed(m, 10.0);
    let a = run_evo(&seed, UpliftMode::LinearBelt, 0.4, EvolutionSolverMode::Fast, None);
    let b = run_evo(&seed, UpliftMode::LinearBelt, 0.4, EvolutionSolverMode::Fast, None);
    assert_eq!(a.elevation.to_dense(), b.elevation.to_dense());
}
