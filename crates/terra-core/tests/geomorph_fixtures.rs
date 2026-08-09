//! Fixed-seed geomorph fixtures for geological realism slice A.
//!
//! Scenario: steep hard ridge beside soft foothills. Asserts hardness resistance,
//! non-negative water/sediment, and hydraulic neighbour transport invariants.

use terra_core::analyze::{
    hydraulic_erode, hydraulic_erode_with_hardness, thermal_erode, thermal_erode_with_hardness,
};
use terra_core::fields::{bake_hardness_from_materials, AuxMaps};
use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::layer::{HydraulicErosionParams, MaterialsParams, ThermalErosionParams};
use terra_core::mask::{MaskField, MaskSource};
use terra_core::surface::material_weights;

/// Deterministic ridge+foothills heightfield (geometry-encoded, not RNG).
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
fn fixture_hard_ridge_resists_thermal_more_than_soft() {
    let res = 48;
    let hf = ridge_foothills(res);
    let k = hardness_map(res);
    let p = ThermalErosionParams {
        talus_angle_deg: 30.0,
        iterations: 50,
        strength: 0.7,
        hardness: 0.0,
        hardness_source: MaskSource::None,
        ..Default::default()
    };
    let (out, _, _) = thermal_erode_with_hardness(&hf, &p, &k);

    let ridge_i = res * 35 / 100;
    let soft_i = res * 75 / 100;
    let j = res / 2;
    let ridge_drop = hf.get(ridge_i, j) - out.get(ridge_i, j);
    let soft_drop = hf.get(soft_i, j) - out.get(soft_i, j);
    assert!(
        ridge_drop < soft_drop * 0.55 + 0.5,
        "hard ridge drop {ridge_drop} should be less than soft {soft_drop}"
    );
}

#[test]
fn fixture_uniform_hard_vs_soft_thermal() {
    let hf = ridge_foothills(32);
    let soft = ThermalErosionParams {
        talus_angle_deg: 28.0,
        iterations: 40,
        strength: 0.75,
        hardness: 0.0,
        ..ThermalErosionParams::default()
    };
    let hard = ThermalErosionParams {
        hardness: 1.0,
        ..soft.clone()
    };
    let (out_s, _, _) = thermal_erode(&hf, &soft);
    let (out_h, _, _) = thermal_erode(&hf, &hard);
    let mut peak_i = 0u32;
    let mut peak_j = 0u32;
    let mut peak = f32::NEG_INFINITY;
    for j in 0..32 {
        for i in 0..32 {
            let h = hf.get(i, j);
            if h > peak {
                peak = h;
                peak_i = i;
                peak_j = j;
            }
        }
    }
    let drop_s = peak - out_s.get(peak_i, peak_j);
    let drop_h = peak - out_h.get(peak_i, peak_j);
    assert!(drop_h.abs() < 1e-3, "K=1 peak must not move: drop={drop_h}");
    assert!(drop_s > 1.0, "soft peak should erode: drop={drop_s}");
}

#[test]
fn fixture_hydraulic_nonneg_and_transport() {
    let res = 32;
    let hf = ridge_foothills(res);
    let k = hardness_map(res);
    let p = HydraulicErosionParams {
        iterations: 25,
        rainfall: 0.04,
        evaporation: 0.01,
        capacity: 0.12,
        erosion: 0.35,
        deposition: 0.25,
        timestep: 0.35,
        hardness: 0.0,
        hardness_source: MaskSource::None,
        ..HydraulicErosionParams::default()
    };
    let r = hydraulic_erode_with_hardness(&hf, &p, &k);
    assert!(r.water_raw.data().iter().all(|&v| v >= -1e-4));
    assert!(r.sediment_raw.data().iter().all(|&v| v >= -1e-4));
    assert!(r.height.to_dense().iter().all(|v| v.is_finite()));

    let crest = r.wetness.get(res * 35 / 100, res / 2);
    let foot = r.wetness.get(res * 80 / 100, res / 2);
    assert!(
        foot + 1e-3 >= crest * 0.5,
        "foothills wetness {foot} should not be starved vs crest {crest}"
    );
}

#[test]
fn fixture_materials_bake_hardness() {
    let hf = ridge_foothills(24);
    let mats = MaterialsParams::default();
    let weights = material_weights(&hf, &mats, None, &std::collections::HashMap::new(), &[]);
    let hardness = bake_hardness_from_materials(&weights, &mats.rules, 0.5);
    assert!(hardness.data().iter().all(|&v| (0.0..=1.0).contains(&v)));
    let mut steep_k = 0.0f32;
    let mut flat_k = 0.0f32;
    let mut n_steep = 0u32;
    let mut n_flat = 0u32;
    let slope = terra_core::analyze::slope_degrees(&hf);
    for j in 0..24 {
        for i in 0..24 {
            let s = slope.get(i, j) * 90.0;
            let k = hardness.get(i, j);
            if s > 40.0 {
                steep_k += k;
                n_steep += 1;
            } else if s < 15.0 {
                flat_k += k;
                n_flat += 1;
            }
        }
    }
    if n_steep > 0 && n_flat > 0 {
        assert!(
            steep_k / n_steep as f32 > flat_k / n_flat as f32,
            "rock slopes should bake harder than grass flats"
        );
    }
}

#[test]
fn fixture_aux_maps_typed_access() {
    let m = HeightfieldMetrics::new(8, 8, 8.0, 8.0);
    let mut aux = AuxMaps::new();
    aux.insert("wetness", MaskField::filled(m, 0.4));
    aux.insert("hardness", MaskField::filled(m, 0.8));
    assert!((aux.wetness.as_ref().unwrap().get(0, 0) - 0.4).abs() < 1e-6);
    assert!((aux.hardness.as_ref().unwrap().get(0, 0) - 0.8).abs() < 1e-6);
    let map = aux.to_hashmap();
    assert!(map.contains_key("wetness") && map.contains_key("hardness"));
}

#[test]
fn fixture_before_after_mass_finite() {
    let hf = ridge_foothills(40);
    let before: f32 = hf.to_dense().iter().sum();
    let p = ThermalErosionParams {
        iterations: 30,
        strength: 0.6,
        talus_angle_deg: 32.0,
        ..ThermalErosionParams::default()
    };
    let (after, _, _) = thermal_erode(&hf, &p);
    let after_sum: f32 = after.to_dense().iter().sum();
    assert!(
        (after_sum - before).abs() / before.max(1.0) < 0.02,
        "thermal mass drift {after_sum} vs {before}"
    );
    let hr = hydraulic_erode(
        &hf,
        &HydraulicErosionParams {
            iterations: 15,
            ..HydraulicErosionParams::default()
        },
    );
    assert!(hr.height.to_dense().iter().all(|v| v.is_finite()));
}
