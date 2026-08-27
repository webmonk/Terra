//! Non-square resampling behaviour for the levelled-sim mask helpers.
//!
//! `resample_mask` replaced a `Heightfield` round trip that forced square
//! targets, so the new path is the first one that can express a non-square
//! resample at all. `docs/next.md` lists it as untested; this closes that.
//!
//! `resample_mask` is private, so this comes at it two ways. `downsample_height`
//! shares `area_sample` and the bilinear kernel with it and covers non-square
//! *sources* - but it always builds a square target, so it cannot reach the case
//! the backlog actually flags. `thermal_erode_leveled_with_hardness` can: it
//! takes the target metrics straight from its input, so a non-square input
//! drives `upsample_mask` with a non-square target, which is the path the old
//! `Heightfield` round trip could not express at all.

use terra_core::analyze::{
    default_sim_levels, downsample_height, thermal_erode_leveled_with_hardness,
};
use terra_core::layer::ThermalErosionParams;
use terra_core::mask::MaskField;
use terra_core::heightfield::{Heightfield, HeightfieldMetrics};

/// A ramp whose height is the world X coordinate, so the correct resampled
/// value at any position is analytically known along X and constant along Z.
fn ramp(w: u32, h: u32, world_x: f32, world_z: f32) -> Heightfield {
    let m = HeightfieldMetrics::new(w, h, world_x, world_z);
    let mut hf = Heightfield::zeros(m);
    for j in 0..h {
        for i in 0..w {
            hf.set(i, j, i as f32);
        }
    }
    hf
}

/// A non-square source must survive being resampled without panicking or
/// producing non-finite samples, in both directions.
#[test]
fn non_square_source_resamples_cleanly() {
    for (w, h) in [(64u32, 16u32), (16, 64), (96, 32), (33, 17)] {
        let src = ramp(w, h, w as f32 * 10.0, h as f32 * 10.0);
        for target_res in [8u32, 32, 64, 128] {
            let out = downsample_height(&src, target_res);
            assert_eq!(out.metrics.width, target_res.max(1));
            let dense = out.to_dense();
            assert_eq!(dense.len(), (out.metrics.width * out.metrics.height) as usize);
            assert!(
                dense.iter().all(|v| v.is_finite()),
                "{w}x{h} -> {target_res} produced non-finite samples"
            );
            let (lo, hi) = (0.0f32, (w - 1) as f32);
            assert!(
                dense.iter().all(|v| *v >= lo - 1e-3 && *v <= hi + 1e-3),
                "{w}x{h} -> {target_res} escaped the source range [{lo}, {hi}]"
            );
        }
    }
}

/// Resampling a constant field must return that constant at any target size and
/// aspect. This is the sharpest check on the kernels: any weight that fails to
/// normalise, or any sample taken outside the field, shows up immediately.
#[test]
fn constant_field_survives_any_non_square_resample() {
    for (w, h) in [(64u32, 16u32), (16, 64), (48, 96), (33, 17)] {
        let m = HeightfieldMetrics::new(w, h, w as f32 * 10.0, h as f32 * 10.0);
        let src = Heightfield::filled(m, 7.5);
        for target_res in [4u32, 16, 64, 200] {
            let dense = downsample_height(&src, target_res).to_dense();
            for (k, v) in dense.iter().enumerate() {
                assert!(
                    (v - 7.5).abs() < 1e-3,
                    "{w}x{h} -> {target_res}: sample {k} drifted to {v} from a \
                     constant 7.5 field"
                );
            }
        }
    }
}

/// The documented rule is box-average when decimating and bilinear when
/// magnifying. A *mixed* resample - one axis shrinking while the other grows -
/// only arises for non-square targets, and the decision is a single `||`, so
/// both axes take the decimating path. This pins what that actually does rather
/// than asserting an intent the code does not implement: values must still be
/// sane and in range.
#[test]
fn mixed_shrink_and_grow_stays_in_range() {
    // 64 wide, 16 tall: going to 32 shrinks X and grows Y.
    let src = ramp(64, 16, 640.0, 160.0);
    let dense = downsample_height(&src, 32).to_dense();
    assert!(dense.iter().all(|v| v.is_finite()));
    assert!(
        dense.iter().all(|v| *v >= -1e-3 && *v <= 63.0 + 1e-3),
        "mixed resample escaped the source range"
    );
    // Monotonic along X is the property the ramp guarantees and any sane kernel
    // preserves, box or bilinear.
    let w = 32usize;
    for i in 1..w {
        assert!(
            dense[i] >= dense[i - 1] - 1e-3,
            "row 0 is not monotonic along X at {i}: {} then {}",
            dense[i - 1],
            dense[i]
        );
    }
}

/// Square targets are unchanged by any of the above - the common path must not
/// have moved.
#[test]
fn square_resample_is_unchanged() {
    let src = ramp(64, 64, 640.0, 640.0);
    let down = downsample_height(&src, 16);
    assert_eq!((down.metrics.width, down.metrics.height), (16, 16));
    let up = downsample_height(&src, 128);
    assert_eq!((up.metrics.width, up.metrics.height), (128, 128));
    // Ends of the ramp survive a round trip within a texel of the source range.
    let d = down.to_dense();
    assert!(d[0] < d[15], "downsampled ramp lost its gradient");
}

/// The case the backlog actually flags: a non-square *target*.
///
/// `upsample_mask` takes `target` from the levelled sim's input metrics, so a
/// non-square input exercises `resample_mask` with `tw != th`. The old
/// `Heightfield` round trip forced a square target and would have panicked on
/// the `copy_from_slice` length mismatch, so this path only became expressible
/// when `resample_mask` started working on the flat buffers directly.
#[test]
fn levelled_sim_handles_a_non_square_world() {
    for (w, h) in [(128u32, 64u32), (64, 128), (96, 48)] {
        let src = ramp(w, h, w as f32 * 8.0, h as f32 * 8.0);
        let hardness = MaskField::filled(src.metrics, 0.5);
        let levels = default_sim_levels(w.min(h));
        let params = ThermalErosionParams::default();

        let (out, erosion, deposition) =
            thermal_erode_leveled_with_hardness(&src, &params, &hardness, &levels);

        assert_eq!(
            (out.metrics.width, out.metrics.height),
            (w, h),
            "the levelled sim must return the world it was given, not a square one"
        );
        for (name, field) in [("erosion", &erosion), ("deposition", &deposition)] {
            assert_eq!(
                (field.metrics.width, field.metrics.height),
                (w, h),
                "{name} channel came back at the wrong aspect for a {w}x{h} world"
            );
            assert_eq!(field.data().len(), (w * h) as usize);
            assert!(
                field.data().iter().all(|v| v.is_finite()),
                "{name} channel has non-finite samples on a {w}x{h} world"
            );
        }
        let dense = out.to_dense();
        assert_eq!(dense.len(), (w * h) as usize);
        assert!(
            dense.iter().all(|v| v.is_finite()),
            "{w}x{h} world produced non-finite heights"
        );
    }
}
