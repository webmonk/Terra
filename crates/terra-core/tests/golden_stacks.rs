//! Golden snapshots of evaluator output for representative stacks.
//!
//! These lock the compositing semantics (blends, masks, groups, biomes, aux
//! channels) so evaluator refactors are verifiable: an intentional semantic
//! change must update the constants here in the same commit, with the delta
//! called out in the commit message.

use terra_core::eval::{EvalContext, StackEvaluator};
use terra_core::heightfield::HeightfieldMetrics;
use terra_core::layer::{
    BlendMode, FbmParams, FlatParams, Layer, LayerGroup, LayerKind, LayerStack, NoiseParams,
    StackNode, TerraceParams, ThermalErosionParams,
};
use terra_core::mask::{MaskAsset, MaskId, MaskRef, MaskSource};

const RES: u32 = 96;
const WORLD: f32 = 1024.0;

/// (mean, min, max, probe(24,24), probe(70,40))
type Fingerprint = [f32; 5];

fn metrics() -> HeightfieldMetrics {
    HeightfieldMetrics::new(RES, RES, WORLD, WORLD)
}

fn fingerprint_height(hf: &terra_core::Heightfield) -> Fingerprint {
    let (mut sum, mut min, mut max) = (0.0f64, f32::INFINITY, f32::NEG_INFINITY);
    for j in 0..RES {
        for i in 0..RES {
            let v = hf.get(i, j);
            sum += v as f64;
            min = min.min(v);
            max = max.max(v);
        }
    }
    [
        (sum / (RES as f64 * RES as f64)) as f32,
        min,
        max,
        hf.get(24, 24),
        hf.get(70, 40),
    ]
}

fn fingerprint_mask(m: &terra_core::mask::MaskField) -> Fingerprint {
    let (mut sum, mut min, mut max) = (0.0f64, f32::INFINITY, f32::NEG_INFINITY);
    for j in 0..RES {
        for i in 0..RES {
            let v = m.get(i, j);
            sum += v as f64;
            min = min.min(v);
            max = max.max(v);
        }
    }
    [
        (sum / (RES as f64 * RES as f64)) as f32,
        min,
        max,
        m.get(24, 24),
        m.get(70, 40),
    ]
}

fn assert_fingerprint(name: &str, got: Fingerprint, expected: Fingerprint) {
    for (k, (g, e)) in got.iter().zip(expected.iter()).enumerate() {
        assert!(
            (g - e).abs() <= 1e-3_f32.max(e.abs() * 1e-4),
            "{name}[{k}]: got {g}, expected {e} (full: {got:?})"
        );
    }
}

fn eval(stack: &LayerStack, ctx: &mut EvalContext) -> terra_core::Heightfield {
    let mut evaluator = StackEvaluator::new();
    evaluator.rebuild_all(stack, ctx).expect("eval")
}

#[test]
fn golden_generators_and_blends() {
    let mut stack = LayerStack::new();
    stack.push(Layer::new("Base", LayerKind::Flat(FlatParams { height: 50.0 })));
    let mut fbm = Layer::new("Fbm", LayerKind::Fbm(FbmParams::default()));
    fbm.common.blend = BlendMode::Add;
    stack.push(fbm);
    let mut terrace = Layer::new(
        "Terrace",
        LayerKind::Terrace(TerraceParams::default()),
    );
    terrace.common.opacity = 0.6;
    stack.push(terrace);

    let mut ctx = EvalContext::new(metrics());
    let out = eval(&stack, &mut ctx);
    assert_fingerprint(
        "generators_and_blends",
        fingerprint_height(&out),
        GOLDEN_GENERATORS,
    );
}

#[test]
fn golden_masked_layer() {
    let mask_id = MaskId::new();
    let mut stack = LayerStack::new();
    stack.push(Layer::new("Base", LayerKind::Flat(FlatParams { height: 20.0 })));
    let mut noise = Layer::new(
        "Noise",
        LayerKind::NoisePerlin(NoiseParams {
            amplitude: 80.0,
            ..NoiseParams::default()
        }),
    );
    noise.common.blend = BlendMode::Add;
    noise.common.masks.push(MaskRef::new(mask_id));
    stack.push(noise);

    let mut ctx = EvalContext::new(metrics());
    ctx.mask_assets.push(MaskAsset::new(
        mask_id,
        "LowAreas",
        MaskSource::Height { min: 0.0, max: 40.0 },
    ));
    let out = eval(&stack, &mut ctx);
    assert_fingerprint("masked_layer", fingerprint_height(&out), GOLDEN_MASKED);
}

#[test]
fn golden_isolated_group() {
    let mut stack = LayerStack::new();
    stack.push(Layer::new("Base", LayerKind::Flat(FlatParams { height: 30.0 })));
    let mut group = LayerGroup::isolated("G");
    group.opacity = 0.5;
    group.blend = BlendMode::Add;
    let mut fbm = Layer::new("Fbm", LayerKind::Fbm(FbmParams::default()));
    fbm.common.blend = BlendMode::Add;
    group.children.push(StackNode::Layer(fbm));
    stack.nodes.push(StackNode::Group(group));

    let mut ctx = EvalContext::new(metrics());
    let out = eval(&stack, &mut ctx);
    assert_fingerprint("isolated_group", fingerprint_height(&out), GOLDEN_GROUP);
}

#[test]
fn golden_biome_delta_composite() {
    let mut stack = LayerStack::new();
    stack.push(Layer::new("Base", LayerKind::Fbm(FbmParams::default())));
    let mut biome = LayerGroup::biome("B");
    let mut terrace = Layer::new(
        "Terrace",
        LayerKind::Terrace(TerraceParams::default()),
    );
    terrace.common.blend = BlendMode::Normal;
    biome.push_into_section(terrace);
    stack.nodes.push(StackNode::Group(biome));

    let mut ctx = EvalContext::new(metrics());
    let out = eval(&stack, &mut ctx);
    assert_fingerprint("biome_delta", fingerprint_height(&out), GOLDEN_BIOME);
}

#[test]
fn golden_erosion_and_aux() {
    let mut stack = LayerStack::new();
    let mut fbm = Layer::new("Fbm", LayerKind::Fbm(FbmParams::default()));
    fbm.common.blend = BlendMode::Add;
    stack.push(fbm);
    stack.push(Layer::new(
        "Thermal",
        LayerKind::ThermalErosion(ThermalErosionParams::default()),
    ));

    let mut ctx = EvalContext::new(metrics());
    let out = eval(&stack, &mut ctx);
    assert_fingerprint("erosion_height", fingerprint_height(&out), GOLDEN_EROSION);
    let sediment = ctx
        .aux_maps
        .sediment
        .as_ref()
        .or(ctx.aux_maps.deposition.as_ref());
    if let Some(sed) = sediment {
        assert_fingerprint("erosion_aux", fingerprint_mask(sed), GOLDEN_EROSION_AUX);
    }
}

/// Field-set compositing semantics: a masked layer's aux-channel writes are
/// scoped by its weight exactly like its height contribution. With an
/// (effectively) all-zero mask, the sim must leave no trace in any channel.
#[test]
fn masked_sim_aux_is_scoped_by_mask() {
    let mask_id = MaskId::new();
    let mut stack = LayerStack::new();
    let mut fbm = Layer::new("Fbm", LayerKind::Fbm(FbmParams::default()));
    fbm.common.blend = BlendMode::Add;
    stack.push(fbm);
    let mut thermal = Layer::new(
        "Thermal",
        LayerKind::ThermalErosion(ThermalErosionParams::default()),
    );
    thermal.common.masks.push(MaskRef::new(mask_id));
    stack.push(thermal);

    let mut ctx = EvalContext::new(metrics());
    // Height range far above the terrain: bakes to ~0 everywhere.
    ctx.mask_assets.push(MaskAsset::new(
        mask_id,
        "Nowhere",
        MaskSource::Height {
            min: 100_000.0,
            max: 100_001.0,
        },
    ));
    let masked_out = eval(&stack, &mut ctx);

    // Height must be untouched (weight zero), matching the bare generator.
    let mut bare = LayerStack::new();
    let mut fbm2 = Layer::new("Fbm", LayerKind::Fbm(FbmParams::default()));
    fbm2.common.blend = BlendMode::Add;
    bare.push(fbm2);
    let mut bare_ctx = EvalContext::new(metrics());
    let bare_out = eval(&bare, &mut bare_ctx);
    for j in (0..RES).step_by(7) {
        for i in (0..RES).step_by(7) {
            assert!(
                (masked_out.get(i, j) - bare_out.get(i, j)).abs() < 1e-3,
                "height leaked outside mask at ({i},{j})"
            );
        }
    }

    // Sim-owned channels must be ~zero everywhere (weight-zero writes).
    for key in ["sediment", "erosion", "deposition", "wetness"] {
        if let Some(field) = ctx.aux_maps.get(key) {
            let max = field.data().iter().cloned().fold(0.0f32, f32::max);
            assert!(max < 1e-3, "{key} leaked outside mask (max {max})");
        }
    }
}

// Recorded on commit b689717 (pre field-set compositing) at 96x96 / 1024m.
const GOLDEN_GENERATORS: Fingerprint = [48.39215, -26.913963, 123.085945, 43.081482, 90.68248];
const GOLDEN_MASKED: Fingerprint = [21.942556, -8.342754, 50.38075, 18.786388, 47.80087];
const GOLDEN_GROUP: Fingerprint = [46.553127, 6.5430183, 81.54297, 43.406784, 68.87991];
const GOLDEN_BIOME: Fingerprint = [-4.7505865, -76.91396, 73.085945, -9.406573, 35.96424];
const GOLDEN_EROSION: Fingerprint = [3.1062574, -76.91396, 73.085945, -3.1864357, 47.759827];
const GOLDEN_EROSION_AUX: Fingerprint = [0.015508522, 0.0, 1.0, 0.0, 0.0];
