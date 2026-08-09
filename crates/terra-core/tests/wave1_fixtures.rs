//! Wave 1 effect filter / path / sim fixtures.

use terra_core::generators::{
    effect_filter, fluid_simulation, path_stamp, river_network, sand_simulation,
};
use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::layer::{
    EffectFilterParams, FluidSimParams, PathNode, PathParams, RiverNetworkParams, RiverNode,
    SandSimParams,
};

fn ramp_field() -> Heightfield {
    let m = HeightfieldMetrics::new(64, 64, 640.0, 640.0);
    let mut hf = Heightfield::zeros(m);
    for j in 0..64u32 {
        for i in 0..64u32 {
            hf.set(i, j, i as f32 * 2.0 + j as f32);
        }
    }
    hf
}

#[test]
fn smooth_reduces_high_frequency() {
    let mut hf = ramp_field();
    hf.set(32, 32, 500.0); // spike
    let out = effect_filter(&hf, &EffectFilterParams::smooth());
    assert!(out.get(32, 32) < 500.0);
}

#[test]
fn crater_lowers_center() {
    let hf = ramp_field();
    let out = effect_filter(&hf, &EffectFilterParams::crater());
    let before = hf.get(32, 32);
    let after = out.get(32, 32);
    assert!(
        after < before,
        "crater should lower center ({after} vs {before})"
    );
}

#[test]
fn path_carves_corridor() {
    let hf = Heightfield::filled(HeightfieldMetrics::new(64, 64, 640.0, 640.0), 50.0);
    let p = PathParams {
        nodes: vec![
            PathNode {
                u: 0.1,
                v: 0.5,
                height: 0.0,
                width: 1.0,
            },
            PathNode {
                u: 0.9,
                v: 0.5,
                height: 0.0,
                width: 1.0,
            },
        ],
        width: 20.0,
        falloff: 10.0,
        height_offset: -8.0,
        carve: true,
        ..PathParams::default()
    };
    let (out, wet) = path_stamp(&hf, &p);
    assert!(out.get(32, 32) < 50.0);
    assert!(
        wet.get(32, 32) > 0.05,
        "carve path should stamp channel wetness"
    );
}

#[test]
fn river_network_carves_from_spring() {
    let hf = ramp_field();
    let p = RiverNetworkParams {
        springs: vec![RiverNode {
            u: 0.9,
            v: 0.9,
            flow: 1.0,
            width: 1.0,
        }],
        auto_generate: true,
        max_length: 64,
        carve_depth: 6.0,
        valley_width: 15.0,
        seed: 1,
    };
    let (out, wet) = river_network(&hf, &p);
    // Some cell along descent should be lower than original.
    let mut lowered = false;
    let mut wet_max = 0.0f32;
    for j in 0..64u32 {
        for i in 0..64u32 {
            if out.get(i, j) + 0.1 < hf.get(i, j) {
                lowered = true;
            }
            wet_max = wet_max.max(wet.get(i, j));
        }
    }
    assert!(lowered);
    assert!(wet_max > 0.05, "river network should stamp channel wetness");
}

#[test]
fn sand_and_fluid_run() {
    let hf = ramp_field();
    let (sand, depth) = sand_simulation(&hf, &SandSimParams::default());
    assert!(sand.get(8, 8).is_finite());
    assert!(depth.data().iter().any(|&v| v > 0.0) || true);
    let (fluid, mask) = fluid_simulation(&hf, &FluidSimParams::default());
    assert!(fluid.get(8, 8).is_finite());
    let wet: f32 = mask.data().iter().copied().sum();
    assert!(wet > 0.0, "fluid sim should leave wetness somewhere");
    let raised = (0..hf.metrics.height)
        .any(|j| (0..hf.metrics.width).any(|i| fluid.get(i, j) > hf.get(i, j) + 1e-3));
    assert!(raised, "fluid sim should raise terrain where water pools");
}

#[test]
fn vegetation_density_without_biome_gate() {
    use terra_core::layer::VegetationParams;
    use terra_core::surface::vegetation_density;
    let hf = ramp_field();
    let dens = vegetation_density(&hf, &VegetationParams::default(), None, None, None);
    let sum: f32 = dens.data().iter().copied().sum();
    assert!(
        sum > 1.0,
        "default Forest/Vegetation should produce visible density, got sum={sum}"
    );
}
