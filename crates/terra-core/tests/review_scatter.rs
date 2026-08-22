//! Adversarial review of `LayerKind::ScatterObjects` (commit dbd5edf).
//!
//! Four defects were confirmed here and are now fixed; these tests are the
//! regression guards. Names prefixed `disproved_` were hypotheses that turned
//! out to be sound, kept because they pin the invariant that makes them sound.
//!
//! The four:
//!   1. props placed inside a group were discarded by the isolated-group aux
//!      restore - and a biome section is where scatter layers land by default,
//!      so the export lost `object_instances.json` in the normal arrangement;
//!   2. a second scatter layer replaced the first layer's props instead of
//!      appending to them;
//!   3. the layer hand-rolled its `DistBakeContext` without slope/curvature/
//!      flow, so a Curvature or Flow node evaluated to ones - and in an
//!      exclusion, `coverage * (1 - exclusion)` then zeroed the whole world;
//!   4. the prop density was published onto `vegetation`, the Vegetation
//!      layer's channel, which erased the forest and instanced trees at the
//!      prop positions. It has its own `scatter_density` channel now.

use terra_core::eval::{EvalContext, StackEvaluator};
use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::layer::{
    FlatParams, Layer, LayerGroup, LayerKind, LayerStack, ObjectClass, ScatterObjectsParams,
    StackNode, VegetationParams,
};

const RES: u32 = 64;
const WORLD: f32 = 256.0;

fn metrics() -> HeightfieldMetrics {
    HeightfieldMetrics::new(RES, RES, WORLD, WORLD)
}

fn scatter_params() -> ScatterObjectsParams {
    ScatterObjectsParams {
        seed: 99,
        classes: vec![ObjectClass {
            name: "Rocks".into(),
            density: 1.0,
            min_spacing_m: 8.0,
            max_slope_deg: 90.0,
            ..ObjectClass::default()
        }],
        ..ScatterObjectsParams::default()
    }
}

fn base_layer() -> Layer {
    Layer::new("Base", LayerKind::Flat(FlatParams { height: 20.0 }))
}

// ===========================================================================
// Finding 1 - a Scatter Objects layer inside a scoped group publishes no
// instances at all.
//
// `scatter_objects` parks its placements on `ctx.aux_maps.object_instances`
// (eval/processors.rs:778), a non-raster side channel exactly like
// `aux_maps.strata`. Every place that round-trips aux carries `strata`
// explicitly:
//
//   * `CachedOutput { .., strata }`                     eval/cache.rs:15
//   * `store_cached` / `store_group_cached`             eval/mod.rs:699, 723
//   * cache-hit restore                                 eval/mod.rs:779, 825
//   * `AuxMaps::from_hashmap_preserving_strata`         fields/mod.rs:346
//   * `merge_aux_masked`                                eval/mod.rs:1400
//
// `object_instances` is carried by *none* of them. The reachable consequence
// is the isolated-group path in `evaluate_nodes` (eval/mod.rs:575-632):
//
//     let aux_snapshot = ctx.aux_maps.clone();     // pre-group: no instances
//     ... children evaluate, writing ctx.aux_maps.object_instances ...
//     ctx.aux_maps = aux_snapshot;                 // eval/mod.rs:601 - wiped
//     merge_aux_masked(ctx, &child_aux, ..);       // rasters + strata only
//
// so the instance list is discarded. This is not an exotic configuration: it
// is the *authoring default* for props. `LayerGroup::push_into_section`
// (layer/stack.rs:236) routes `ScatterObjects` into `BiomeSection::Objects`,
// which lives inside a `GroupKind::Biome` group, and `is_scoped`
// (layer/stack.rs:255-261) returns true for every biome group - so the biome
// takes the isolated-composite branch.
//
// The user-visible result: `object_instances.json` is silently omitted from
// the export (`export.rs:208-209` skips the file when the list is empty),
// even though the layer's `vegetation` / `scatter_candidates` rasters merged
// through fine and the layer reports as computed.
// ===========================================================================
#[test]
fn scatter_objects_in_a_group_still_publishes_its_instances() {
    let m = metrics();

    // Reference: the same layer at the top level places props.
    let mut flat_stack = LayerStack::new();
    flat_stack.push(base_layer());
    flat_stack.push(Layer::new(
        "Props",
        LayerKind::ScatterObjects(scatter_params()),
    ));
    let mut ctx_flat = EvalContext::new(m);
    StackEvaluator::new()
        .rebuild_all(&flat_stack, &mut ctx_flat)
        .unwrap();
    let expected = ctx_flat.aux_maps.object_instances.len();
    assert!(
        expected > 0,
        "precondition: a top-level Scatter Objects layer must place props"
    );

    // Same layer, now inside a biome group - the authoring default.
    let mut grouped = LayerStack::new();
    grouped.push(base_layer());
    let mut biome = LayerGroup::biome("Biome");
    biome.children.push(StackNode::Layer(Layer::new(
        "Props",
        LayerKind::ScatterObjects(scatter_params()),
    )));
    grouped.nodes.push(StackNode::Group(biome));

    let mut ctx = EvalContext::new(m);
    StackEvaluator::new().rebuild_all(&grouped, &mut ctx).unwrap();

    // The raster channels survive the group composite...
    let density = ctx
        .aux_maps
        .get("scatter_density")
        .expect("scatter density channel");
    assert!(
        density.data().iter().any(|v| *v > 0.0),
        "precondition: the group composite merged the scatter density raster"
    );
    // ...but the instance list, which is what the export actually writes, does not.
    assert_eq!(
        ctx.aux_maps.object_instances.len(),
        expected,
        "props placed inside a group are dropped by the isolated-group aux \
         snapshot/restore (eval/mod.rs:601), so object_instances.json is \
         silently omitted from the export"
    );
}

// ===========================================================================
// Finding 2 - Scatter Objects erases the Vegetation layer's density channel.
//
// The processor publishes its prop-density proxy on `keys::VEGETATION`
// (eval/processors.rs:776), the very channel a Vegetation layer writes and
// the one the viewport instances *trees* from
// (`app/redraw.rs:136-142` feeds `scheduler.last_aux.get("vegetation")` into
// `sync_vegetation_instances`). `aux_insert` replaces, it does not merge, so
// a Scatter Objects layer above a Vegetation layer wipes the forest: the
// trees vanish from the viewport and from `vegetation_instances.json`, and
// are replaced by trees standing wherever the *props* were placed.
//
// The two channels are not interchangeable - props carry their own instance
// list and their own class/scale/yaw - so the correct behaviour is for
// Scatter Objects to publish on its own channel and leave `vegetation`
// alone.
// ===========================================================================
#[test]
fn scatter_objects_does_not_clobber_the_vegetation_channel() {
    let m = metrics();

    let veg_layer = || {
        Layer::new(
            "Trees",
            LayerKind::Vegetation(VegetationParams {
                seed: 7,
                max_slope_deg: 90.0,
                ..VegetationParams::default()
            }),
        )
    };

    let mut veg_only = LayerStack::new();
    veg_only.push(base_layer());
    veg_only.push(veg_layer());
    let mut ctx_veg = EvalContext::new(m);
    StackEvaluator::new()
        .rebuild_all(&veg_only, &mut ctx_veg)
        .unwrap();
    let trees = ctx_veg.aux_maps.get("vegetation").unwrap().clone();
    let tree_mass: f32 = trees.data().iter().sum();
    assert!(tree_mass > 0.0, "precondition: the Vegetation layer wrote trees");

    let mut with_props = LayerStack::new();
    with_props.push(base_layer());
    with_props.push(veg_layer());
    with_props.push(Layer::new(
        "Props",
        LayerKind::ScatterObjects(scatter_params()),
    ));
    let mut ctx = EvalContext::new(m);
    StackEvaluator::new()
        .rebuild_all(&with_props, &mut ctx)
        .unwrap();
    let after = ctx.aux_maps.get("vegetation").unwrap();

    let mut worst = 0.0f32;
    for j in 0..RES {
        for i in 0..RES {
            worst = worst.max((trees.get(i, j) - after.get(i, j)).abs());
        }
    }
    assert!(
        worst < 1e-6,
        "adding a Scatter Objects layer rewrote the shared 'vegetation' \
         channel (max delta {worst}), so the Vegetation layer's trees are \
         replaced by the prop density proxy in the viewport and in \
         vegetation_instances.json"
    );
}

// ===========================================================================
// Finding 3 - a second Scatter Objects layer deletes the first layer's props.
//
// `ctx.aux_maps.object_instances = placed.instances` (eval/processors.rs:778)
// assigns; it does not append. Two prop layers is ordinary authoring (rocks
// + debris, or one layer per biome section), and unlike a raster channel a
// list of placed objects has no "last writer wins" blend semantics to appeal
// to - the first layer's props simply never reach object_instances.json.
// ===========================================================================
#[test]
fn a_second_scatter_layer_does_not_delete_the_first_layers_props() {
    let m = metrics();
    let rocks = ScatterObjectsParams {
        seed: 11,
        classes: vec![ObjectClass {
            name: "Rocks".into(),
            density: 1.0,
            min_spacing_m: 10.0,
            max_slope_deg: 90.0,
            ..ObjectClass::default()
        }],
        ..ScatterObjectsParams::default()
    };
    let debris = ScatterObjectsParams {
        seed: 22,
        classes: vec![ObjectClass {
            name: "Debris".into(),
            density: 1.0,
            min_spacing_m: 14.0,
            max_slope_deg: 90.0,
            ..ObjectClass::default()
        }],
        ..ScatterObjectsParams::default()
    };

    let count_for = |p: ScatterObjectsParams| {
        let mut stack = LayerStack::new();
        stack.push(base_layer());
        stack.push(Layer::new("Props", LayerKind::ScatterObjects(p)));
        let mut ctx = EvalContext::new(m);
        StackEvaluator::new().rebuild_all(&stack, &mut ctx).unwrap();
        ctx.aux_maps.object_instances.len()
    };
    let n_rocks = count_for(rocks.clone());
    let n_debris = count_for(debris.clone());
    assert!(n_rocks > 0 && n_debris > 0);

    let mut stack = LayerStack::new();
    stack.push(base_layer());
    stack.push(Layer::new("Rocks", LayerKind::ScatterObjects(rocks)));
    stack.push(Layer::new("Debris", LayerKind::ScatterObjects(debris)));
    let mut ctx = EvalContext::new(m);
    StackEvaluator::new().rebuild_all(&stack, &mut ctx).unwrap();

    assert_eq!(
        ctx.aux_maps.object_instances.len(),
        n_rocks + n_debris,
        "the upper Scatter Objects layer overwrote the lower layer's instance \
         list instead of adding to it, so only the last layer's props export"
    );
}

// ===========================================================================
// Finding 4 - Curvature / Flow nodes in the coverage or exclusion
// distribution are evaluated against nothing, and an exclusion built from one
// removes every prop in the world.
//
// The ScatterObjects arm hand-rolls its bake context
// (eval/processors.rs:754-761):
//
//     let bake_ctx = crate::mask::DistBakeContext {
//         height: Some(input),
//         slope_deg: None,
//         curvature: None,
//         flow: None,
//         ..
//     };
//
// It is the only `DistBakeContext` constructed in processors.rs; every other
// distribution in the evaluator goes through `composite_distribution`
// (eval/mod.rs:1405-1427), which wires all three from `ctx.aux`
// ("slope", "curvature", "flow_accumulation"/"flow").
//
// `eval_node_base` treats a missing field as "no opinion" and returns
// `MaskField::ones` for `DistNodeKind::Curvature` (dist_nodes.rs:646-648),
// `Cavity` (dist_nodes.rs:672) and `Flow` (dist_nodes.rs:678-680). Ones is
// benign for coverage (the filter is silently ignored) but catastrophic for
// exclusion: suitability is `coverage * (1 - exclusion)`
// (scatter/objects.rs:52), so exclusion == 1 everywhere zeroes the whole
// candidate field and the layer places nothing at all, with no diagnostic.
// (Slope survives only because `slope_deg_field` falls back to a height
// difference, dist_nodes.rs:1243-1257.)
// ===========================================================================
#[test]
fn scatter_distributions_see_the_curvature_channel() {
    use terra_core::mask::{DistNode, DistNodeKind, Distribution, MaskField};

    let m = metrics();
    // Flat ground: curvature is 0 everywhere, so a "curvature >= 0.5"
    // exclusion must exclude nothing.
    let mut exclusion = Distribution::new();
    exclusion.push_node(DistNode::new(DistNodeKind::Curvature {
        min: 0.5,
        max: 1.0,
    }));

    let mut stack = LayerStack::new();
    stack.push(base_layer());
    stack.push(Layer::new(
        "Props",
        LayerKind::ScatterObjects(ScatterObjectsParams {
            exclusion,
            ..scatter_params()
        }),
    ));

    let mut ctx = EvalContext::new(m);
    // The curvature channel the house helper would have used is present.
    ctx.aux_insert("curvature", MaskField::zeros(m));
    StackEvaluator::new().rebuild_all(&stack, &mut ctx).unwrap();

    assert!(
        !ctx.aux_maps.object_instances.is_empty(),
        "a curvature-based exclusion that selects nothing wiped out every \
         placement: the processor bakes the distribution with curvature: None \
         (eval/processors.rs:757), so the Curvature node returns ones and \
         suitability collapses to zero"
    );
}

// ===========================================================================
// DISPROVED - the 120k site budget cannot violate a class's min spacing.
//
// `site_spacing = tightest.max((area / MAX_SITES).sqrt())` (scatter/objects.rs:79)
// only ever *widens* the sampler radius, and the sampler's own Poisson
// constraint already guarantees >= site_spacing between sites. Widening
// therefore makes placements sparser, never denser; the per-class
// `SpacingGrid` (min_sq = min_spacing^2, 3x3 bucket scan, which is exact for
// cell == min_spacing) can only remove more. The user-facing effect of the
// budget is the opposite of a violated constraint: the spacing slider stops
// biting below the budget floor, which the inspector states
// (edit_kind.rs "Widened if it would exceed 120k sites.").
// ===========================================================================
#[test]
fn disproved_site_budget_never_violates_per_class_min_spacing() {
    // 4096 m world: sqrt(4096^2 / 120_000) ~= 11.8 m, so a 3 m class is
    // budget-limited and a 40 m class is not.
    let m = HeightfieldMetrics::new(128, 128, 4096.0, 4096.0);
    let hf = Heightfield::zeros(m);
    let p = ScatterObjectsParams {
        seed: 5,
        classes: vec![
            ObjectClass {
                name: "Tight".into(),
                min_spacing_m: 3.0,
                density: 1.0,
                max_slope_deg: 90.0,
                ..ObjectClass::default()
            },
            ObjectClass {
                name: "Loose".into(),
                min_spacing_m: 40.0,
                density: 1.0,
                max_slope_deg: 90.0,
                ..ObjectClass::default()
            },
        ],
        ..ScatterObjectsParams::default()
    };
    let out = terra_core::scatter::scatter_objects(&hf, &p, None, None);
    assert!(!out.instances.is_empty());
    for a in 0..out.instances.len() {
        for b in (a + 1)..out.instances.len() {
            let (x, y) = (&out.instances[a], &out.instances[b]);
            if x.class_index != y.class_index {
                continue;
            }
            let want = p.classes[x.class_index as usize].min_spacing_m;
            let (dx, dz) = (x.x - y.x, x.z - y.z);
            assert!(
                dx * dx + dz * dz >= want * want - 1e-3,
                "{} pair closer than min_spacing_m under the site budget",
                x.class
            );
        }
    }
}

// ===========================================================================
// DISPROVED - suitability / class-filter edge cases are all safe.
//
// * empty class list, every class disabled, weight <= 0, density <= 0: the
//   `filter` at scatter/objects.rs:59-62 drops them, and an empty `classes`
//   returns early - so the weighted pick's `total` is never zero and there
//   is no division by zero (there is no division at all: `pick = u * total`).
// * inverted `scale_range`: `s0 + (s1 - s0) * t` stays inside [min, max]
//   whichever way round the endpoints are, then `.max(0.001)`.
// * `height_range` that excludes everything: `eligible` is empty, the site is
//   skipped.
// * `max_slope_deg` 0 / 90: the test is `slope_deg > max`, so exact 0 on flat
//   ground and exact 90 on a vertical face both pass.
// ===========================================================================
#[test]
fn disproved_degenerate_class_configurations_are_safe() {
    let m = HeightfieldMetrics::new(32, 32, 128.0, 128.0);
    let mut hf = Heightfield::zeros(m);
    for j in 0..32u32 {
        for i in 0..32u32 {
            hf.set(i, j, i as f32 * 4.0);
        }
    }
    hf.refresh_halos();

    let run = |classes: Vec<ObjectClass>| {
        terra_core::scatter::scatter_objects(
            &hf,
            &ScatterObjectsParams {
                seed: 3,
                classes,
                ..ScatterObjectsParams::default()
            },
            None,
            None,
        )
    };

    assert!(run(Vec::new()).instances.is_empty());
    assert!(run(vec![ObjectClass {
        enabled: false,
        ..ObjectClass::default()
    }])
    .instances
    .is_empty());
    assert!(run(vec![ObjectClass {
        weight: 0.0,
        ..ObjectClass::default()
    }])
    .instances
    .is_empty());
    assert!(run(vec![ObjectClass {
        density: 0.0,
        ..ObjectClass::default()
    }])
    .instances
    .is_empty());
    assert!(run(vec![ObjectClass {
        height_range: [10_000.0, 20_000.0],
        ..ObjectClass::default()
    }])
    .instances
    .is_empty());

    // Inverted scale range still yields scales inside the authored interval.
    let inverted = run(vec![ObjectClass {
        scale_range: [2.0, 0.5],
        density: 1.0,
        max_slope_deg: 90.0,
        ..ObjectClass::default()
    }]);
    assert!(!inverted.instances.is_empty());
    for inst in &inverted.instances {
        assert!(
            (0.5..=2.0).contains(&inst.scale),
            "inverted scale_range produced {}",
            inst.scale
        );
    }

    // max_slope_deg == 0 keeps exactly-flat ground.
    let flat = Heightfield::zeros(m);
    let zero_slope = terra_core::scatter::scatter_objects(
        &flat,
        &ScatterObjectsParams {
            seed: 3,
            classes: vec![ObjectClass {
                max_slope_deg: 0.0,
                density: 1.0,
                ..ObjectClass::default()
            }],
            ..ScatterObjectsParams::default()
        },
        None,
        None,
    );
    assert!(!zero_slope.instances.is_empty());
}

// ===========================================================================
// DISPROVED - placement is deterministic and `enabled` round-trips.
// ===========================================================================
#[test]
fn disproved_placement_is_deterministic_and_serde_defaults_hold() {
    let m = metrics();
    let mut hf = Heightfield::zeros(m);
    for j in 0..RES {
        for i in 0..RES {
            hf.set(i, j, (i as f32 * 0.3).sin() * 20.0 + j as f32 * 0.4);
        }
    }
    hf.refresh_halos();
    let p = scatter_params();
    let a = terra_core::scatter::scatter_objects(&hf, &p, None, None);
    for _ in 0..4 {
        let b = terra_core::scatter::scatter_objects(&hf, &p, None, None);
        assert_eq!(a.instances, b.instances);
    }

    // `#[serde(default = "default_true")]`, not `#[serde(default)]`.
    let mut json = serde_json::to_value(ObjectClass::default()).unwrap();
    json.as_object_mut().unwrap().remove("enabled");
    let back: ObjectClass = serde_json::from_value(json).unwrap();
    assert!(back.enabled, "a pre-`enabled` document must load as enabled");

    // The whole params blob round-trips through the layer enum tag.
    let kind = LayerKind::ScatterObjects(p);
    let s = serde_json::to_string(&kind).unwrap();
    assert!(s.contains("ScatterObjects"), "frozen serde tag: {s}");
    let _: LayerKind = serde_json::from_str(&s).unwrap();
}
