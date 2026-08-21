//! Shared, deterministic terrain fixtures for the terra-core benchmarks.
//!
//! Every stack built here is fully specified (fixed seeds, fixed iteration
//! counts, no wall-clock or RNG input), so two runs of the same benchmark
//! evaluate exactly the same terrain and the numbers stay comparable across
//! commits. Nothing in this module touches the GPU.

#![allow(dead_code)]

use std::collections::HashMap;

use terra_core::eval::{EvalContext, StackEvaluator};
use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::layer::{
    BlurParams, FbmParams, FlatParams, FractalNoiseType, GroupEvalMode, GroupInputMode, Layer,
    LayerGroup, LayerKind, LayerStack, NoiseParams, StackNode, ThermalErosionParams,
    VegetationParams,
};
use terra_core::mask::{MaskAsset, MaskId, MaskSource};

/// World extent is held constant so only the sample count varies with `res`.
pub const WORLD_SIZE: f32 = 4096.0;

/// Simulation iteration counts are deliberately below the artist defaults.
/// The benches measure the *shape* of the evaluation cost (and regressions in
/// it), not a production-quality erosion run; the full 40-iteration default
/// would push the suite past its runtime budget without changing what a
/// regression looks like.
pub const THERMAL_ITERATIONS: u32 = 12;

pub fn metrics(res: u32) -> HeightfieldMetrics {
    HeightfieldMetrics::new(res, res, WORLD_SIZE, WORLD_SIZE)
}

pub fn fbm_layer(name: &str, seed: u64, frequency: f32, amplitude: f32, octaves: u32) -> Layer {
    Layer::new(
        name,
        LayerKind::Fbm(FbmParams {
            base: NoiseParams {
                seed,
                frequency,
                amplitude,
                octaves,
                ..NoiseParams::default()
            },
            noise: FractalNoiseType::Perlin,
        }),
    )
}

pub fn thermal_layer(name: &str) -> Layer {
    Layer::new(
        name,
        LayerKind::ThermalErosion(ThermalErosionParams {
            iterations: THERMAL_ITERATIONS,
            ..ThermalErosionParams::default()
        }),
    )
}

pub fn blur_layer(name: &str) -> Layer {
    Layer::new(
        name,
        LayerKind::Blur(BlurParams {
            radius: 2,
            iterations: 1,
        }),
    )
}

/// Vegetation with a sparse scatter. The artist default (`density` 0.35,
/// `min_distance` 3.0) runs a dense Poisson-disk scatter that costs seconds at
/// 256^2 and would swamp the very thing the field-aware bench is measuring -
/// whether the *layers above* vegetation get skipped. A sparse scatter keeps
/// the layer cheap so the skip shows up in the ratio.
pub fn vegetation_layer(name: &str) -> Layer {
    Layer::new(
        name,
        LayerKind::Vegetation(VegetationParams {
            density: 0.004,
            min_distance: 64.0,
            ..VegetationParams::default()
        }),
    )
}

/// Representative artist stack: flat base, fbm relief, a cheap filter, and a
/// thermal erosion sim on top. This is the shape of a real document's height
/// section and is what `rebuild_all` / `rebuild_incremental` are timed on.
pub fn representative_stack() -> LayerStack {
    let mut stack = LayerStack::new();
    stack.push(Layer::new(
        "Base",
        LayerKind::Flat(FlatParams { height: 120.0 }),
    ));
    stack.push(fbm_layer("Relief", 1337, 0.0006, 260.0, 6));
    stack.push(blur_layer("Smooth"));
    stack.push(thermal_layer("Thermal"));
    stack
}

/// Field-aware invalidation fixture: `[fbm, vegetation, blur, thermal]`.
///
/// Vegetation writes a vegetation-density field and does not modify height, so
/// dirtying it should leave the blur and thermal layers above it clean. The
/// bench pairs a vegetation edit against a bottom-layer edit to expose the win.
pub fn field_skip_stack() -> LayerStack {
    let mut stack = LayerStack::new();
    stack.push(fbm_layer("Relief", 4242, 0.0006, 260.0, 6));
    stack.push(vegetation_layer("Vegetation"));
    stack.push(blur_layer("Smooth"));
    stack.push(thermal_layer("Thermal"));
    stack
}

/// Ids of the layers in a flat stack, bottom to top.
pub fn layer_ids(stack: &LayerStack) -> Vec<terra_core::layer::LayerId> {
    stack.flatten_layers().iter().map(|l| l.id()).collect()
}

/// A base layer plus one isolated group holding three layers. Isolated groups
/// are the unit the group cache keys on (`height_fingerprint` of the group's
/// input), so this is the fixture for measuring cached vs uncached group
/// evaluation.
pub fn grouped_stack() -> LayerStack {
    let mut stack = LayerStack::new();
    stack.push(Layer::new(
        "Base",
        LayerKind::Flat(FlatParams { height: 100.0 }),
    ));

    let mut group = LayerGroup::new("Mountain Range");
    group.eval_mode = GroupEvalMode::IsolatedComposite;
    group.input_mode = GroupInputMode::CopyInput;
    group
        .children
        .push(StackNode::Layer(fbm_layer("Ridges", 99, 0.0008, 300.0, 6)));
    group.children.push(StackNode::Layer(blur_layer("Soften")));
    group
        .children
        .push(StackNode::Layer(thermal_layer("Weather")));
    stack.push_group(group);
    stack
}

/// A deterministic, non-trivial heightfield used by the fingerprint / hashing
/// and parallel-composite benches. Built from a cheap analytic function rather
/// than the evaluator so the fixture cost never shows up in the measurement.
pub fn sample_field(res: u32) -> Heightfield {
    let mut hf = Heightfield::zeros(metrics(res));
    hf.par_map_indexed(|i, j, _| {
        let x = i as f32 * 0.013;
        let z = j as f32 * 0.017;
        120.0 + 60.0 * (x.sin() + z.cos()) + 8.0 * (x * 3.1 + z * 2.7).sin()
    });
    hf
}

/// Procedural mask assets over height and slope, the two most common sources in
/// real documents (both are recomputed against the live terrain at bake time).
pub fn mask_assets() -> Vec<MaskAsset> {
    vec![
        MaskAsset::new(
            MaskId::new(),
            "Lowlands",
            MaskSource::Height {
                min: 0.0,
                max: 120.0,
            },
        ),
        MaskAsset::new(
            MaskId::new(),
            "Uplands",
            MaskSource::Height {
                min: 140.0,
                max: 260.0,
            },
        ),
        MaskAsset::new(
            MaskId::new(),
            "Cliffs",
            MaskSource::Slope {
                min_deg: 35.0,
                max_deg: 90.0,
            },
        ),
        MaskAsset::new(
            MaskId::new(),
            "Benches",
            MaskSource::Slope {
                min_deg: 0.0,
                max_deg: 8.0,
            },
        ),
        MaskAsset::new(
            MaskId::new(),
            "Concave",
            MaskSource::Curvature {
                min: -1.0,
                max: 0.0,
            },
        ),
    ]
}

/// Evaluate a stack once from cold, returning the primed evaluator/context so a
/// benchmark can time the *warm* path that follows.
pub fn primed(stack: &LayerStack, res: u32) -> (StackEvaluator, EvalContext) {
    let mut eval = StackEvaluator::new();
    let mut ctx = EvalContext::new(metrics(res));
    ctx.masks = HashMap::new();
    let _ = eval.rebuild_all(stack, &mut ctx).expect("cold rebuild");
    (eval, ctx)
}
