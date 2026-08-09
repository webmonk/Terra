//! Phase I multi-scale amplify fixtures: hard ridges resist amplify more than soft,
//! and tiled batched stencils keep border continuity.

use terra_core::analyze::{amplify_sim_levels, multi_scale_amplify};
use terra_core::heightfield::{Heightfield, HeightfieldMetrics, TileId};
use terra_core::layer::MultiScaleAmplifyParams;
use terra_core::mask::MaskField;
use terra_core::tiling::{map_tiles_batched, measure_seams, DirtyClass, TileScheduler};

fn ridge_foothills(res: u32) -> Heightfield {
    let m = HeightfieldMetrics::new(res, res, res as f32, res as f32);
    let mut hf = Heightfield::zeros(m);
    let cx = (res / 2) as f32;
    for j in 0..res {
        for i in 0..res {
            let x = i as f32;
            let z = j as f32;
            let ridge = (-((x - cx * 0.35).powi(2)) / (2.0 * 2.5f32.powi(2))).exp() * 40.0;
            let soft = ((x * 0.18).sin() * 0.5 + (z * 0.11).cos() * 0.5 + 1.0) * 4.0;
            hf.set(i, j, ridge + soft + 5.0);
        }
    }
    hf
}

fn hardness_map(res: u32) -> MaskField {
    let m = HeightfieldMetrics::new(res, res, res as f32, res as f32);
    let mut k = MaskField::zeros(m);
    let split = res * 2 / 5;
    for j in 0..res {
        for i in 0..res {
            let v = if i < split { 0.92 } else { 0.05 };
            k.set(i, j, v);
        }
    }
    k
}

#[test]
fn amplify_preserves_hard_ridge_more_than_soft() {
    let res = 64;
    let hf = ridge_foothills(res);
    let k = hardness_map(res);
    let p = MultiScaleAmplifyParams {
        level_count: 2,
        thermal_strength: 0.5,
        thermal_iters: 10,
        spe_strength: 0.4,
        spe_iters: 3,
        deposition_strength: 0.2,
        detail_boost: 1.35,
        lock_strength: 0.9,
        ..MultiScaleAmplifyParams::default()
    };
    let levels = amplify_sim_levels(res, 2);
    let out = multi_scale_amplify(&hf, &p, &k, None, &levels);

    let ridge_i = res * 35 / 100;
    let soft_i = res * 75 / 100;
    let j = res / 2;
    let ridge_drop = (hf.get(ridge_i, j) - out.height.get(ridge_i, j)).abs();
    let soft_drop = (hf.get(soft_i, j) - out.height.get(soft_i, j)).abs();
    assert!(
        ridge_drop < soft_drop * 0.75 + 0.75,
        "hard ridge change {ridge_drop} should be less than soft {soft_drop}"
    );
}

#[test]
fn amplify_ridge_lock_reduces_change() {
    let res = 48;
    let hf = ridge_foothills(res);
    let k = MaskField::filled(hf.metrics, 0.1);
    let mut lock = MaskField::zeros(hf.metrics);
    for j in 0..res {
        for i in 0..(res / 2) {
            lock.set(i, j, 1.0);
        }
    }
    let p = MultiScaleAmplifyParams {
        level_count: 2,
        thermal_strength: 0.55,
        thermal_iters: 8,
        spe_strength: 0.35,
        spe_iters: 2,
        detail_boost: 1.4,
        lock_strength: 1.0,
        ..MultiScaleAmplifyParams::default()
    };
    let levels = amplify_sim_levels(res, 2);
    let unlocked = multi_scale_amplify(&hf, &p, &k, None, &levels);
    let locked = multi_scale_amplify(&hf, &p, &k, Some(&lock), &levels);
    let j = res / 2;
    let i = res / 4;
    let drop_u = (hf.get(i, j) - unlocked.height.get(i, j)).abs();
    let drop_l = (hf.get(i, j) - locked.height.get(i, j)).abs();
    assert!(
        drop_l <= drop_u + 1e-3,
        "lock should not increase change: locked {drop_l} unlocked {drop_u}"
    );
}

#[test]
fn amplify_deterministic() {
    let res = 48;
    let hf = ridge_foothills(res);
    let k = hardness_map(res);
    let p = MultiScaleAmplifyParams {
        level_count: 2,
        ..MultiScaleAmplifyParams::default()
    };
    let levels = amplify_sim_levels(res, 2);
    let a = multi_scale_amplify(&hf, &p, &k, None, &levels);
    let b = multi_scale_amplify(&hf, &p, &k, None, &levels);
    for j in 0..res {
        for i in 0..res {
            assert!((a.height.get(i, j) - b.height.get(i, j)).abs() < 1e-5);
        }
    }
}

#[test]
fn seam_metric_after_batched_neighbourhood() {
    let metrics = HeightfieldMetrics {
        width: 64,
        height: 64,
        world_size_x: 64.0,
        world_size_z: 64.0,
        tile_size: 32,
        halo: 2,
    };
    let mut hf = Heightfield::zeros(metrics);
    for j in 0..64 {
        for i in 0..64 {
            hf.set(i, j, ((i * 5 + j * 3) % 37) as f32);
        }
    }
    hf.refresh_halos();
    let seam = map_tiles_batched(&mut hf, 4, 2, |tile, lx, lz, _, _| {
        let mut sum = 0.0;
        let mut c = 0.0;
        for dj in -1..=1 {
            for di in -1..=1 {
                sum += tile.get_with_halo(lx as i32 + di, lz as i32 + dj);
                c += 1.0;
            }
        }
        sum / c
    });
    assert!(seam < 1e-2, "seam {seam}");
    assert!(measure_seams(&hf) < 1e-2);
}

#[test]
fn expand_for_amplify_marks_small_field() {
    let metrics = HeightfieldMetrics {
        width: 64,
        height: 64,
        world_size_x: 64.0,
        world_size_z: 64.0,
        tile_size: 32,
        halo: 2,
    };
    let hf = Heightfield::zeros(metrics);
    let mut sched = TileScheduler::new();
    sched.mark_tile(TileId { tx: 0, tz: 0 });
    sched.expand_for_process(&hf, DirtyClass::BasinDependent, 1, 8);
    assert_eq!(sched.dirty.len(), hf.tiles().len());
}
