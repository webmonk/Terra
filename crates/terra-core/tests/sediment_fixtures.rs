//! Phase G sedimentary landform fixtures: mass conservation, slope-break fans,
//! deterministic deposition, bank-slip talus.

use terra_core::analyze::{
    apply_sediment_softness, hydraulic_erode, hydraulic_erode_with_hardness,
};
use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::layer::HydraulicErosionParams;
use terra_core::mask::MaskField;

/// Steep mountain (left) dropping onto a flat plain (right) - classic fan fixture.
fn mountain_plain(res: u32) -> Heightfield {
    let m = HeightfieldMetrics::new(res, res, res as f32 * 2.0, res as f32 * 2.0);
    let mut hf = Heightfield::zeros(m);
    let break_x = (res * 2 / 5) as f32;
    for j in 0..res {
        for i in 0..res {
            let x = i as f32;
            let z = (j as f32 - res as f32 * 0.5) * 0.02;
            let h = if x < break_x {
                // Steep catchment sloping toward the break.
                80.0 - (x / break_x) * 55.0 + z.sin() * 2.0
            } else {
                // Flat apron with a gentle downslope away from the fan apex.
                25.0 - (x - break_x) * 0.08 + z.sin() * 0.4
            };
            hf.set(i, j, h);
        }
    }
    hf
}

#[test]
fn sediment_mass_conserved_within_eps() {
    let hf = mountain_plain(48);
    let p = HydraulicErosionParams {
        iterations: 40,
        rainfall: 0.04,
        evaporation: 0.01,
        capacity: 0.12,
        erosion: 0.4,
        deposition: 0.5,
        timestep: 0.2,
        fan_boost: 1.0,
        floodplain_bias: 0.8,
        bank_slip: 0.0, // mass raws exclude bank-slip
        sediment_softness: 0.0,
        ..HydraulicErosionParams::default()
    };
    let r = hydraulic_erode(&hf, &p);
    let h0 = hf.to_dense();
    let h1 = r.height.to_dense();
    let dh: f32 = h1.iter().zip(h0.iter()).map(|(a, b)| a - b).sum();
    let sed: f32 = r.sediment_raw.data().iter().sum();
    let eroded: f32 = r.erosion_raw.data().iter().sum();
    let deposited: f32 = r.deposition_raw.data().iter().sum();

    let residual = (dh + sed).abs();
    let scale = eroded.max(deposited).max(1.0);
    assert!(
        residual < scale * 0.02 + 1e-2,
        "mass residual {residual} (dh={dh}, sed={sed}, E={eroded}, D={deposited})"
    );
    assert!(
        (eroded - deposited - sed).abs() < scale * 0.02 + 1e-2,
        "E ≉ D+s: E={eroded} D={deposited} s={sed}"
    );
    assert!(deposited > 1e-3, "expected visible deposition mass");
}

#[test]
fn fan_forms_at_slope_break() {
    let res = 64;
    let hf = mountain_plain(res);
    let p = HydraulicErosionParams {
        iterations: 55,
        rainfall: 0.05,
        evaporation: 0.008,
        capacity: 0.1,
        erosion: 0.45,
        deposition: 0.7,
        timestep: 0.22,
        fan_boost: 2.0,
        floodplain_bias: 0.0,
        bank_slip: 0.0,
        ..HydraulicErosionParams::default()
    };
    let r = hydraulic_erode(&hf, &p);
    let dep = r.deposition_raw.data();
    let break_i = res * 2 / 5;
    let band = 3u32;
    let mut fan_sum = 0.0f32;
    let mut upslope_sum = 0.0f32;
    let mut fan_n = 0u32;
    let mut up_n = 0u32;
    for j in 0..res {
        for i in 0..res {
            let v = dep[(j * res + i) as usize];
            // Apron cells immediately past the break (fan apex).
            if i >= break_i && i <= break_i + band {
                fan_sum += v;
                fan_n += 1;
            } else if i + 8 < break_i {
                upslope_sum += v;
                up_n += 1;
            }
        }
    }
    let fan_mean = fan_sum / fan_n.max(1) as f32;
    let up_mean = upslope_sum / up_n.max(1) as f32;
    assert!(
        fan_mean > up_mean * 1.35 + 1e-5,
        "fan apex mean {fan_mean} should exceed upslope {up_mean}"
    );

    // Concentration vs no-boost control (same floodplain/other params).
    let control = HydraulicErosionParams {
        fan_boost: 0.0,
        ..p.clone()
    };
    let r0 = hydraulic_erode(&hf, &control);
    let dep0 = r0.deposition_raw.data();
    let total_b: f32 = dep.iter().sum::<f32>().max(1e-6);
    let total_c: f32 = dep0.iter().sum::<f32>().max(1e-6);
    let mut band_b = 0.0f32;
    let mut band_c = 0.0f32;
    for j in 0..res {
        for i in break_i..=(break_i + band).min(res - 1) {
            let idx = (j * res + i) as usize;
            band_b += dep[idx];
            band_c += dep0[idx];
        }
    }
    let share_b = band_b / total_b;
    let share_c = band_c / total_c;
    assert!(
        share_b + 1e-4 >= share_c,
        "fan_boost should not dilute apex share: share_b={share_b} share_c={share_c}"
    );
    // Absolute apex deposition should rise with fan_boost.
    assert!(
        band_b > band_c * 1.01 + 1e-5,
        "fan_boost should increase apex deposition: boosted={band_b} ctrl={band_c}"
    );

    let mut steep_drop = 0.0f32;
    let mut n_s = 0u32;
    for j in res / 4..3 * res / 4 {
        for i in 2..break_i / 2 {
            steep_drop += hf.get(i, j) - r.height.get(i, j);
            n_s += 1;
        }
    }
    assert!(
        steep_drop / n_s.max(1) as f32 > 0.0,
        "steep catchment should erode"
    );
    assert!(fan_mean > 1e-4, "expected depositional mass at fan apex");
}

#[test]
fn hydraulic_depositional_deterministic() {
    let hf = mountain_plain(32);
    let p = HydraulicErosionParams::depositional();
    let a = hydraulic_erode(&hf, &p);
    let b = hydraulic_erode(&hf, &p);
    assert_eq!(a.height.to_dense(), b.height.to_dense());
    assert_eq!(a.deposition_raw.data(), b.deposition_raw.data());
}

#[test]
fn bank_slip_moves_mass_to_toes() {
    let m = HeightfieldMetrics::new(32, 32, 64.0, 64.0);
    let mut hf = Heightfield::filled(m, 10.0);
    // Undercut bank: tall cliff next to a low floor.
    for j in 0..32 {
        for i in 0..14 {
            hf.set(i, j, 40.0);
        }
        for i in 14..32 {
            hf.set(i, j, 8.0);
        }
    }
    let p = HydraulicErosionParams {
        iterations: 8,
        rainfall: 0.02,
        erosion: 0.15,
        deposition: 0.2,
        bank_slip: 0.8,
        fan_boost: 0.0,
        floodplain_bias: 0.0,
        ..HydraulicErosionParams::default()
    };
    let r = hydraulic_erode(&hf, &p);
    // Toe cells just past the cliff should gain relative to far cliff top drop.
    let mut toe = 0.0f32;
    let mut far = 0.0f32;
    for j in 8..24 {
        toe += r.height.get(15, j) - hf.get(15, j);
        far += r.height.get(28, j) - hf.get(28, j);
    }
    assert!(
        toe > far - 0.5,
        "bank slip should preferentially load the toe: toeΔ={toe} farΔ={far}"
    );
}

#[test]
fn sediment_softness_lowers_hardness() {
    let m = HeightfieldMetrics::new(16, 16, 16.0, 16.0);
    let hard = MaskField::filled(m, 0.9);
    let mut dep = MaskField::zeros(m);
    for j in 0..16 {
        for i in 8..16 {
            dep.set(i, j, 1.0);
        }
    }
    let out = apply_sediment_softness(&hard, &dep, 0.7);
    assert!(out.get(12, 8) < out.get(2, 8));
    assert!(out.get(12, 8) < 0.5);
}

#[test]
fn hardness_still_resists_with_fans() {
    let hf = mountain_plain(40);
    let mut k = MaskField::filled(hf.metrics, 0.05);
    for j in 0..40 {
        for i in 0..16 {
            k.set(i, j, 0.95);
        }
    }
    let p = HydraulicErosionParams {
        iterations: 30,
        fan_boost: 1.0,
        floodplain_bias: 0.5,
        bank_slip: 0.0,
        ..HydraulicErosionParams::default()
    };
    let soft = MaskField::filled(hf.metrics, 0.0);
    let r_soft = hydraulic_erode_with_hardness(&hf, &p, &soft);
    let r_hard = hydraulic_erode_with_hardness(&hf, &p, &k);
    let e_soft: f32 = r_soft.erosion_raw.data().iter().sum();
    let e_hard: f32 = r_hard.erosion_raw.data().iter().sum();
    assert!(
        e_hard < e_soft * 0.85,
        "hard map should erode less: hard={e_hard} soft={e_soft}"
    );
}
