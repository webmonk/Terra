//! Placed props must sit on the surface, not on the nearest texel centre.
//!
//! `ObjectInstance` carries continuous world `x`/`z` from the Poisson sampler,
//! but `y` used to come from `hf.get(sample_index(x, z))`, which snaps to the
//! nearest cell centre. On a slope that is wrong by up to half a texel of
//! horizontal offset times the local gradient, so an exported prop floated
//! above the ground on one side of a cell and sank into it on the other. On a
//! 12.6 km world previewed at 512 a texel is ~25 m across, which on a 30 degree
//! slope is metres of error.

use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::layer::{ObjectClass, ScatterObjectsParams};
use terra_core::scatter::scatter_objects;

/// A 45 degree ramp in X: height is exactly the world X coordinate, so the
/// correct answer at any position is analytically known.
fn ramp() -> Heightfield {
    let m = HeightfieldMetrics::new(16, 16, 160.0, 160.0);
    let mut hf = Heightfield::zeros(m);
    for j in 0..m.height {
        for i in 0..m.width {
            hf.set(i, j, m.world_x(i));
        }
    }
    hf
}

#[test]
fn bilinear_sampling_follows_the_surface_between_texels() {
    let hf = ramp();
    let m = hf.metrics;
    let dx = m.dx();

    // Exactly on a cell centre both agree - that is the easy case.
    let centre = m.world_x(8);
    assert!(
        (hf.sample_bilinear(centre, centre) - centre).abs() < 1e-3,
        "on-centre sampling must be exact"
    );

    // Half a texel off centre is where nearest-texel is worst. On this ramp the
    // true height equals the world X coordinate.
    let between = m.world_x(8) + dx * 0.5;
    let bilinear = hf.sample_bilinear(between, centre);
    let (ni, nj) = m.sample_index(between, centre);
    let nearest = hf.get(ni, nj);

    assert!(
        (bilinear - between).abs() < 1e-2,
        "bilinear height {bilinear} should track the true surface {between}"
    );
    assert!(
        (nearest - between).abs() > dx * 0.3,
        "this position must actually expose the nearest-texel error, else the \
         test proves nothing (nearest={nearest}, true={between}, dx={dx})"
    );
}

/// The sampler must stay inside the field and stay finite at and beyond the
/// edges - placements can land on the last half-texel.
#[test]
fn bilinear_sampling_is_clamped_at_the_edges() {
    let hf = ramp();
    let m = hf.metrics;
    for (x, z) in [
        (0.0, 0.0),
        (-50.0, -50.0),
        (m.world_size_x, m.world_size_z),
        (m.world_size_x * 2.0, m.world_size_z * 2.0),
    ] {
        let h = hf.sample_bilinear(x, z);
        assert!(h.is_finite(), "sample at ({x}, {z}) must be finite, got {h}");
        assert!(
            (0.0..=m.world_size_x).contains(&h),
            "sample at ({x}, {z}) escaped the field's height range: {h}"
        );
    }
}

/// A degenerate field must not panic the sampler.
#[test]
fn bilinear_sampling_handles_a_single_texel_field() {
    let m = HeightfieldMetrics::new(1, 1, 10.0, 10.0);
    let mut hf = Heightfield::zeros(m);
    hf.set(0, 0, 42.0);
    assert!((hf.sample_bilinear(5.0, 5.0) - 42.0).abs() < 1e-6);
    assert!((hf.sample_bilinear(-99.0, 99.0) - 42.0).abs() < 1e-6);
}

/// End to end: every placement the scatter layer emits must sit on the surface
/// at its own continuous X/Z, not at the nearest texel centre.
#[test]
fn placed_props_sit_on_the_surface() {
    // A steep ramp, so any snapping shows up as metres of error.
    let m = HeightfieldMetrics::new(64, 64, 1280.0, 1280.0);
    let mut hf = Heightfield::zeros(m);
    for j in 0..m.height {
        for i in 0..m.width {
            hf.set(i, j, m.world_x(i) * 0.8);
        }
    }

    let params = ScatterObjectsParams {
        seed: 4242,
        classes: vec![ObjectClass {
            density: 1.0,
            max_slope_deg: 90.0,
            ..ObjectClass::named("Rocks")
        }],
        ..ScatterObjectsParams::default()
    };
    let out = scatter_objects(&hf, &params, None, None);
    assert!(
        !out.instances.is_empty(),
        "the scatter must actually place something for this test to mean anything"
    );

    let mut worst = 0.0f32;
    for inst in &out.instances {
        let surface = hf.sample_bilinear(inst.x, inst.z);
        worst = worst.max((inst.y - surface).abs());
    }
    assert!(
        worst < 1e-2,
        "worst prop was {worst} m off its own surface height across {}          placements; placements are continuous in X/Z so Y must be too",
        out.instances.len()
    );
}
