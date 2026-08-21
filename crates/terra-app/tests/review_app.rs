//! Adversarial review repros (third pass, app-side features).
//!
//! `terra-app` has no lib target, so these tests exercise the exact algorithms
//! the app-side handlers run, transcribed verbatim from the handler bodies, and
//! the crates they call into. Each test names the file:line it mirrors.
//!
//! Tests marked `EXPECTED FAIL` assert the *correct* behaviour and currently
//! fail; they are the repros for the confirmed findings.

use terra_core::command::{CommandHistory, EditorCommand};
use terra_core::eval::{EvalContext, StackEvaluator};
use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::layer::{
    BlendMode, FbmParams, Layer, LayerId, LayerKind, LayerStack, StackNode, ThermalErosionParams,
};
use terra_core::mask::{MaskField, MaskId, MaskRef, PaintBuffer};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn flat(name: &str) -> Layer {
    Layer::new(
        name,
        LayerKind::Flat(terra_core::layer::FlatParams { height: 1.0 }),
    )
}

/// Root-level ids in stack order.
fn root_order(stack: &LayerStack) -> Vec<LayerId> {
    stack
        .nodes
        .iter()
        .map(|n| match n {
            StackNode::Layer(l) => l.id(),
            StackNode::Group(g) => g.id,
        })
        .collect()
}

/// Verbatim transcription of the `ReorderMany` loop in
/// `crates/terra-app/src/app/actions/layers.rs:1101-1165`.
fn reorder_many(stack: &mut LayerStack, moving: Vec<LayerId>, target: LayerId, place_before: bool) {
    if moving.contains(&target) {
        return;
    }
    let ordered: Vec<LayerId> = if place_before {
        moving
    } else {
        moving.into_iter().rev().collect()
    };
    for id in ordered {
        if id == target {
            continue;
        }
        if stack.find(id).is_none() && stack.find_group(id).is_none() {
            continue;
        }
        stack.reorder_relative(id, target, place_before);
    }
}

// ---------------------------------------------------------------------------
// FINDING 1 (CONFIRMED): multi-row drag reverses relative order in the
// hierarchy's reversed (Shape / Mask / Sim) folders.
//
// `crates/terra-app/src/ui/hierarchy/mod.rs:781-784` converts `place_before`
// from *visual* to *stack* semantics for `list_reversed` rows, but
// `collect_multi_drag_ids` (mod.rs:1867) still hands `ReorderMany` the ids in
// *visual* top-to-bottom order. The handler's forward/reverse iteration is
// keyed off the (already-converted) stack-space `place_before`, so the two
// conventions are mixed and the dragged block comes out reversed.
// ---------------------------------------------------------------------------

#[test]
fn reorder_many_preserves_relative_order_in_document_order_lists() {
    // Control: a non-reversed folder (Biomes). Visible order == stack order.
    let mut stack = LayerStack::new();
    let (a, b, c, t) = (flat("A"), flat("B"), flat("C"), flat("T"));
    let (aid, bid, cid, tid) = (a.id(), b.id(), c.id(), t.id());
    // Stack (and therefore visible) order: A B C T
    stack.push(a);
    stack.push(b);
    stack.push(c);
    stack.push(t);

    // Drag {A,B,C} onto the top half of T => visually "before T"; no reversal
    // flip is applied for a document-order list, so place_before stays true.
    reorder_many(&mut stack, vec![aid, bid, cid], tid, true);

    assert_eq!(
        root_order(&stack),
        vec![aid, bid, cid, tid],
        "document-order list: relative order preserved (this is the good path)"
    );
}

#[test]
fn reorder_many_preserves_relative_order_in_reversed_lists() {
    // Regression: `collect_multi_drag_ids` returns *visible* order while
    // `place_before` is converted to *stack* space, so in a bottom-to-top
    // folder (Shape / Mask / Sims) the dragged block landed reversed. The
    // drop handler now reverses the ids for reversed folders so both are in
    // stack space; this pins the contract that ReorderMany receives.
    let mut stack = LayerStack::new();
    let (c, b, a, t) = (flat("C"), flat("B"), flat("A"), flat("T"));
    let (cid, bid, aid, tid) = (c.id(), b.id(), a.id(), t.id());
    stack.push(c);
    stack.push(b);
    stack.push(a);
    stack.push(t);

    // Rows render T / A / B / C. The user multi-selects A, B, C and drops on
    // T's upper half: visible ids [A, B, C], reversed by the fix into stack
    // order [C, B, A]; place_before is already stack-space (false).
    let visible_order_ids = vec![aid, bid, cid];
    let stack_order_ids: Vec<LayerId> = visible_order_ids.into_iter().rev().collect();
    reorder_many(&mut stack, stack_order_ids, tid, false);

    let visible_after: Vec<LayerId> = root_order(&stack).into_iter().rev().collect();
    assert_eq!(
        visible_after,
        vec![aid, bid, cid, tid],
        "a multi-row drag in a reversed folder must keep the dragged rows in          their visible order (A, B, C above T)"
    );
}

// ---------------------------------------------------------------------------
// FINDING 3(c) target: DISPROVED. BatchRemove captures sibling_location fresh
// per id, i.e. post-previous-removal, and undo is LIFO, so indices line up.
// `crates/terra-app/src/app/actions/layers.rs:562-597`.
// ---------------------------------------------------------------------------

#[test]
fn batch_remove_undo_restores_sibling_layers_to_their_original_indices() {
    let mut stack = LayerStack::new();
    let layers: Vec<Layer> = (0..5).map(|i| flat(&format!("L{i}"))).collect();
    let ids: Vec<LayerId> = layers.iter().map(|l| l.id()).collect();
    for l in layers {
        stack.push(l);
    }
    let original = root_order(&stack);

    // Remove two siblings (indices 1 and 3), mirroring the BatchRemove loop.
    let mut history = CommandHistory::default();
    for id in [ids[1], ids[3]] {
        let loc = stack.sibling_location(id);
        if let Some(node) = stack.remove(id) {
            let (parent, index) = loc.unwrap_or((None, 0));
            history.push_executed(EditorCommand::RemoveLayer {
                id,
                node,
                index,
                parent,
            });
        }
    }
    assert_eq!(root_order(&stack), vec![ids[0], ids[2], ids[4]]);

    // Undo both (LIFO, as the app's chronological undo does).
    history.undo(&mut stack);
    history.undo(&mut stack);
    assert_eq!(
        root_order(&stack),
        original,
        "per-id sibling_location capture must survive LIFO undo"
    );
}

// ---------------------------------------------------------------------------
// FINDING 2 (CONFIRMED): the sim-progress scrub checkpoint cache ignores mask
// bindings and mask content.
//
// `scrub_params_fingerprint` (crates/terra-core/src/eval/mod.rs:198-214) hashes
// only `layer.kind`, opacity, blend, `masks.len()` and `clip_to_below`. The
// stored `ScrubEntry.height` is the *post-mix* output, which also depends on
// each binding's strength/invert/combine, on *which* mask id is bound, and on
// the mask's baked content. `LayerCache::mark_dirty` never touches
// `scrub_cache` (it is only cleared by `clear_project_caches`, mod.rs:341), so
// a scrubbed simulation layer keeps replaying a stale checkpoint after any of
// those change.
// ---------------------------------------------------------------------------

fn scrub_stack_with_mask(mask_id: MaskId) -> (LayerStack, LayerId) {
    let mut stack = LayerStack::new();
    let mut fbm = Layer::new("Fbm", LayerKind::Fbm(FbmParams::default()));
    fbm.common.blend = BlendMode::Add;
    stack.push(fbm);
    let mut thermal = Layer::new(
        "Thermal",
        LayerKind::ThermalErosion(ThermalErosionParams::default()),
    );
    thermal.common.sim_progress = 0.5;
    thermal.common.masks.push(MaskRef::new(mask_id));
    let id = thermal.id();
    stack.push(thermal);
    (stack, id)
}

#[test]
fn scrub_checkpoint_is_invalidated_by_a_mask_binding_change() {
    // EXPECTED FAIL - repro for FINDING 2.
    let metrics = HeightfieldMetrics::new(48, 48, 96.0, 96.0);
    let mask_id = MaskId::new();
    let (mut stack, thermal_id) = scrub_stack_with_mask(mask_id);

    // A half-covering mask so inverting it genuinely changes the composite.
    let mut mask = MaskField::new(metrics);
    for j in 0..metrics.height {
        for i in 0..metrics.width {
            mask.set(i, j, if i < metrics.width / 2 { 1.0 } else { 0.0 });
        }
    }

    let mut eval = StackEvaluator::new();
    eval.cache.disable_disk();
    let mut ctx = EvalContext::new(metrics);
    ctx.masks.insert(mask_id, mask.clone());
    let with_mask = eval.rebuild_all(&stack, &mut ctx).unwrap();

    // Flip the binding's `invert`. masks.len() is unchanged, the layer kind is
    // unchanged, and the input height into the layer is unchanged, so both the
    // params fingerprint and the input fingerprint match the stored checkpoint.
    if let Some(l) = stack.find_mut(thermal_id) {
        l.common.masks.entries[0].mask.invert = true;
    }
    eval.mark_dirty_from(&stack, thermal_id);
    let mut ctx2 = EvalContext::new(metrics);
    ctx2.masks.insert(mask_id, mask.clone());
    let inverted = eval.rebuild_incremental(&stack, &mut ctx2).unwrap();

    // Ground truth: a cold evaluator with the inverted binding.
    let mut cold = StackEvaluator::new();
    cold.cache.disable_disk();
    let mut cold_ctx = EvalContext::new(metrics);
    cold_ctx.masks.insert(mask_id, mask);
    let expected = cold.rebuild_all(&stack, &mut cold_ctx).unwrap();

    let mut max_err_vs_expected = 0.0f32;
    let mut max_delta_vs_before = 0.0f32;
    for j in (0..metrics.height).step_by(3) {
        for i in (0..metrics.width).step_by(3) {
            max_err_vs_expected =
                max_err_vs_expected.max((inverted.get(i, j) - expected.get(i, j)).abs());
            max_delta_vs_before =
                max_delta_vs_before.max((inverted.get(i, j) - with_mask.get(i, j)).abs());
        }
    }
    assert!(
        max_delta_vs_before > 1e-4,
        "inverting the mask binding must change the result, but the scrub \
         checkpoint replayed the pre-edit output verbatim (max delta {max_delta_vs_before})"
    );
    assert!(
        max_err_vs_expected < 1e-4,
        "scrubbed layer must match a cold evaluation after a mask-binding \
         change (max error {max_err_vs_expected})"
    );
}

#[test]
fn scrub_checkpoint_is_invalidated_when_a_different_mask_is_bound() {
    // EXPECTED FAIL - second face of FINDING 2 (mask id is not fingerprinted).
    let metrics = HeightfieldMetrics::new(48, 48, 96.0, 96.0);
    let mask_a = MaskId::new();
    let mask_b = MaskId::new();
    let (mut stack, thermal_id) = scrub_stack_with_mask(mask_a);

    let mut left = MaskField::new(metrics);
    let mut right = MaskField::new(metrics);
    for j in 0..metrics.height {
        for i in 0..metrics.width {
            let l = i < metrics.width / 2;
            left.set(i, j, if l { 1.0 } else { 0.0 });
            right.set(i, j, if l { 0.0 } else { 1.0 });
        }
    }

    let mut eval = StackEvaluator::new();
    eval.cache.disable_disk();
    let mut ctx = EvalContext::new(metrics);
    ctx.masks.insert(mask_a, left.clone());
    ctx.masks.insert(mask_b, right.clone());
    let before = eval.rebuild_all(&stack, &mut ctx).unwrap();

    if let Some(l) = stack.find_mut(thermal_id) {
        l.common.masks.entries[0].mask.id = mask_b;
    }
    eval.mark_dirty_from(&stack, thermal_id);
    let mut ctx2 = EvalContext::new(metrics);
    ctx2.masks.insert(mask_a, left);
    ctx2.masks.insert(mask_b, right);
    let after = eval.rebuild_incremental(&stack, &mut ctx2).unwrap();

    let mut max_delta = 0.0f32;
    for j in (0..metrics.height).step_by(3) {
        for i in (0..metrics.width).step_by(3) {
            max_delta = max_delta.max((after.get(i, j) - before.get(i, j)).abs());
        }
    }
    assert!(
        max_delta > 1e-4,
        "binding a different mask must change the scrubbed layer's output \
         (max delta {max_delta}); the checkpoint hashes only masks.len()"
    );
}


#[test]
fn flat_terrain_export_normalization_does_not_divide_by_zero() {
    // Control: min == max is guarded by `.max(1e-6)` and the manifest's
    // de-normalization collapses to `height_min`, which is correct.
    let metrics = HeightfieldMetrics::new(16, 16, 64.0, 64.0);
    let mut hf = Heightfield::zeros(metrics);
    for j in 0..16 {
        for i in 0..16 {
            hf.set(i, j, 137.5);
        }
    }
    hf.refresh_halos();
    let (min_h, max_h) = hf.min_max();
    assert_eq!(min_h, max_h);

    let span = (max_h - min_h).max(1e-6);
    let t = ((hf.get(3, 4) - min_h) / span).clamp(0.0, 1.0);
    let sample = (t * 65535.0) as u16;
    let metres = min_h + sample as f32 / 65535.0 * (max_h - min_h);
    assert!(t.is_finite());
    assert_eq!(metres, 137.5);
}

// ---------------------------------------------------------------------------
// Target 1(a): DISPROVED. `copy_from_resampled` resizes the destination and
// nearest-samples the source, so a selection created at one preview resolution
// converts safely into a layer mask allocated at another.
// `crates/terra-core/src/mask/paint.rs:200-222`.
// ---------------------------------------------------------------------------

#[test]
fn copy_from_resampled_is_safe_across_mismatched_resolutions() {
    for (src_n, dst_n) in [(256u32, 1024u32), (1024, 256), (1, 512), (777, 256)] {
        let mut src = PaintBuffer::new(src_n, src_n);
        src.fill();
        let mut dst = PaintBuffer::new(dst_n, dst_n);
        dst.copy_from_resampled(&src);
        assert_eq!(dst.width, dst_n);
        assert_eq!(dst.samples().len(), (dst_n * dst_n) as usize);
        assert!(dst.samples().iter().all(|s| (*s - 1.0).abs() < 1e-6));
    }
}

// ---------------------------------------------------------------------------
// Target 1(b): DISPROVED. Inverting an absent selection allocates at the
// current preview resolution and yields full coverage.
// `crates/terra-app/src/app/actions/masks.rs:507-525`.
// ---------------------------------------------------------------------------

#[test]
fn invert_of_a_fresh_selection_selects_everything() {
    for preview_resolution in [128u32, 512, 4096] {
        let resolution = preview_resolution.clamp(256, 1024); // selection_resolution()
        let mut asset = terra_core::mask::MaskAsset::new_painted(
            MaskId::new(),
            "Selection",
            resolution,
        );
        asset.paint.as_mut().unwrap().invert();
        let paint = asset.paint.as_ref().unwrap();
        assert_eq!(paint.width, resolution);
        assert!(paint.samples().iter().all(|s| *s == 1.0));
    }
}

// ---------------------------------------------------------------------------
// FINDING 5 (CONFIRMED, app-side reasoning): `BatchGroup`
// (layers.rs:625-679) sorts the selection by `stack.layer_ids()`, which skips
// layers inside disabled groups; those ids get `usize::MAX` and are appended in
// arbitrary (HashSet-iteration) order rather than in document order.
// This test pins the ordering primitive that is being misused.
// ---------------------------------------------------------------------------

#[test]
fn layer_ids_omits_layers_inside_disabled_groups() {
    let mut stack = LayerStack::new();
    let visible = flat("Visible");
    let visible_id = visible.id();
    stack.push(visible);

    let mut group = terra_core::layer::LayerGroup::new("Folder");
    let hidden = flat("Hidden");
    let hidden_id = hidden.id();
    group.children.push(StackNode::Layer(hidden));
    group.enabled = false;
    stack.push_group(group);

    let ids = stack.layer_ids();
    assert!(ids.contains(&visible_id));
    assert!(
        !ids.contains(&hidden_id),
        "layer_ids() skips disabled groups - any diff or ordering built on it \
         (apply_actions' selection-inheritance snapshot, BatchGroup's sort key) \
         is blind to those layers"
    );
    // ...and `find` is blind to them too, so the selection-inheritance hook in
    // actions/mod.rs:151 would classify such a layer as ineligible anyway.
    assert!(stack.find(hidden_id).is_none());
}

// ---------------------------------------------------------------------------
// FINDING 7 (CONFIRMED): redo of a contextual "Create Biome" / "Create Sculpt
// Layer" degrades the created node.
//
// `contextual_create::create_biome` (crates/terra-core/src/contextual_create.rs
// :706-736) pushes a `LayerGroup::biome(..)` into the *Surface category folder*
// but records `EditorCommand::AddGroup { index: 0 }`. `apply(AddGroup)`
// (command/mod.rs:482-488) reconstructs a plain `LayerGroup::new(..)` at root
// index 0, so undo-then-redo replaces the Biome group (GroupKind::Biome,
// isolated eval, biome sections, preview colour) with an empty plain folder at
// the top of the root stack. `create_sculpt_layer` (contextual_create.rs:664-
// 704) has the same shape: `AddLayer { index: nodes.len() }` while the layer
// actually goes into the Shape category folder.
// ---------------------------------------------------------------------------

#[test]
fn redo_of_a_contextual_biome_create_restores_a_biome_group_in_place() {
    // Regression: contextual create recorded AddGroup, which replays as a
    // plain root-level folder, so redo downgraded the biome and left
    // active_biome pointing at a non-biome. It records InsertNode now.
    use terra_core::contextual_create::{
        execute_create, CreateContext, CreateKind, CreateWorkspace,
    };
    use terra_core::document::EditorSession;

    let mut session = EditorSession::new();
    let ctx = CreateContext::from_document(&session.document, CreateWorkspace::Biomes, false);
    let outcome = execute_create(&mut session, CreateKind::Biome, &ctx, None, Some("Alpine".into()))
        .expect("create biome");
    let biome_id = match outcome.entity {
        terra_core::contextual_create::CreatedEntity::BiomeGroup(id) => id,
        other => panic!("expected a biome group, got {other:?}"),
    };
    let before = session
        .document
        .stack
        .sibling_location(biome_id)
        .expect("biome placed in the tree");
    assert!(session.document.stack.find_group(biome_id).unwrap().is_biome());

    session.undo_stack_command();
    assert!(session.document.stack.find_group(biome_id).is_none());

    session.redo_stack_command();
    let restored = session
        .document
        .stack
        .find_group(biome_id)
        .expect("group back after redo");
    assert!(
        restored.is_biome(),
        "redo must restore a Biome group, not a plain folder"
    );
    assert_eq!(
        session.document.stack.sibling_location(biome_id),
        Some(before),
        "redo must restore it under the Surface folder, not at root"
    );
}

// ---------------------------------------------------------------------------
// FINDING 6 (CONFIRMED, app-side reasoning): enabling a group in the same
// action batch that creates a layer makes every pre-existing layer inside that
// group look "new" to `apply_actions`' selection-inheritance diff
// (crates/terra-app/src/app/actions/mod.rs:76-78 vs 145-165), so their existing
// owned paint masks are overwritten by the transient selection.
// ---------------------------------------------------------------------------

#[test]
fn all_layer_ids_is_stable_across_group_enable_toggles() {
    // Regression: the selection-inheritance diff used `layer_ids`, which
    // skips disabled groups, so enabling a group made its existing layers
    // look newly created and clobbered their masks. `all_layer_ids` walks
    // every group regardless of `enabled`, which is what the diff uses now.
    use terra_core::layer::{FlatParams, LayerGroup, LayerKind};

    let mut stack = LayerStack::new();
    let mut group = LayerGroup::new("G");
    let inner = Layer::new("Inner", LayerKind::Flat(FlatParams { height: 1.0 }));
    let inner_id = inner.id();
    group.children.push(StackNode::Layer(inner));
    let group_id = group.id;
    stack.nodes.push(StackNode::Group(group));

    let enabled = stack.all_layer_ids();
    stack.find_group_mut(group_id).unwrap().enabled = false;
    let disabled = stack.all_layer_ids();

    assert!(enabled.contains(&inner_id) && disabled.contains(&inner_id));
    assert_eq!(
        enabled, disabled,
        "the diff's id set must not depend on group visibility"
    );
    assert!(
        !stack.layer_ids().contains(&inner_id),
        "layer_ids still skips disabled groups by design - that is why the          diff cannot use it"
    );
}
