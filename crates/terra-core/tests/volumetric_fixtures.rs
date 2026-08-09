//! Phase J volumetric fixtures: dual-height overhang + local SDF cave pocket.

use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::layer::{LocalSdfParams, OverhangStampParams};
use terra_core::volumetric::{apply_local_sdf, apply_overhang_stamp, build_overhang_mesh};

fn cliff_plateau(res: u32, low: f32, high: f32) -> Heightfield {
    let m = HeightfieldMetrics::new(res, res, res as f32 * 2.0, res as f32 * 2.0);
    let mut hf = Heightfield::filled(m, low);
    for j in 0..res {
        for i in 0..res {
            if i as f32 > res as f32 * 0.48 {
                hf.set(i, j, high);
            }
        }
    }
    hf
}

#[test]
fn overhang_stamp_deterministic() {
    let hf = cliff_plateau(48, 8.0, 42.0);
    let p = OverhangStampParams::cliff_overhang();
    let a = apply_overhang_stamp(&hf, &p);
    let b = apply_overhang_stamp(&hf, &p);
    assert_eq!(a.height.to_dense(), b.height.to_dense());
    assert_eq!(a.ceiling.data(), b.ceiling.data());
    assert_eq!(a.mask.data(), b.mask.data());
}

#[test]
fn overhang_outside_bounds_unchanged() {
    let hf = cliff_plateau(40, 10.0, 40.0);
    let p = OverhangStampParams {
        u: 0.7,
        v: 0.5,
        radius_uv: 0.08,
        depth: 15.0,
        ..OverhangStampParams::default()
    };
    let r = apply_overhang_stamp(&hf, &p);
    // Far corner on low side — never inside stamp.
    assert!((r.height.get(1, 1) - hf.get(1, 1)).abs() < 1e-5);
    assert!(r.mask.get(1, 1) < 1e-5);
    assert!((r.ceiling.get(1, 1) - hf.get(1, 1)).abs() < 1e-5);
}

#[test]
fn overhang_creates_dual_height_cavity() {
    let hf = cliff_plateau(48, 10.0, 45.0);
    let p = OverhangStampParams {
        u: 0.55,
        v: 0.5,
        radius_uv: 0.1,
        depth: 16.0,
        lip_height: 2.0,
        entrance_angle_deg: 180.0,
        falloff: 0.3,
        seed: 5,
        noise_amplitude: 0.1,
    };
    let r = apply_overhang_stamp(&hf, &p);
    let (ci, cj) = hf
        .metrics
        .sample_index(p.u * hf.metrics.world_size_x, p.v * hf.metrics.world_size_z);
    assert!(r.mask.get(ci, cj) > 0.1, "stamp center should be active");
    assert!(
        r.height.get(ci, cj) < hf.get(ci, cj) - 2.0,
        "floor should drop"
    );
    assert!(
        r.ceiling.get(ci, cj) > r.height.get(ci, cj) + 1.0,
        "ceiling should sit above floor"
    );
    let mesh = build_overhang_mesh(&r.height, &r.ceiling, &r.mask, 0.15);
    assert!(!mesh.is_empty(), "proxy mesh should emit roof/wall tris");
}

#[test]
fn local_sdf_region_limited_and_deterministic() {
    let hf = cliff_plateau(36, 12.0, 40.0);
    let p = LocalSdfParams::karst_pocket();
    let a = apply_local_sdf(&hf, &p);
    let b = apply_local_sdf(&hf, &p);
    assert_eq!(a.height.to_dense(), b.height.to_dense());
    assert_eq!(a.mask.data(), b.mask.data());

    // Outside bounding box: unchanged.
    assert!((a.height.get(0, 0) - hf.get(0, 0)).abs() < 1e-5);
    assert!(a.mask.get(0, 0) < 1e-5);

    let carved = a.mask.data().iter().copied().filter(|&v| v > 0.05).count();
    assert!(carved > 4, "expected localized cave carve, got {carved}");
    let mesh = build_overhang_mesh(&a.height, &a.ceiling, &a.mask, 0.1);
    assert!(!mesh.is_empty());
}

#[test]
fn volumetric_does_not_touch_far_heightfield() {
    let hf = cliff_plateau(32, 5.0, 30.0);
    let mut before = Vec::new();
    for j in 0..32 {
        for i in 0..8 {
            before.push(hf.get(i, j));
        }
    }
    let overhang = apply_overhang_stamp(
        &hf,
        &OverhangStampParams {
            u: 0.75,
            v: 0.5,
            radius_uv: 0.06,
            depth: 12.0,
            ..OverhangStampParams::default()
        },
    );
    let mut after = Vec::new();
    for j in 0..32 {
        for i in 0..8 {
            after.push(overhang.height.get(i, j));
        }
    }
    assert_eq!(before, after);
}
