//! Adversarial-review repro tests.
//!
//! Tests named `repro_*` are EXPECTED TO FAIL against the current tree: each
//! one demonstrates a suspected correctness bug. Tests named `disproved_*`
//! pass, and document a hypothesis that was investigated and found sound.
//!
//! Nothing here should be treated as a spec until the corresponding bug is
//! triaged; the assertions encode what the reviewer believes correct
//! behaviour to be.

use terra_core::eval::{EvalContext, StackEvaluator};
use terra_core::heightfield::HeightfieldMetrics;
use terra_core::layer::{BlendMode, FlatParams, Layer, LayerKind, LayerStack};
use terra_core::mask::{MaskAsset, MaskId, MaskRef, MaskSource};

const RES: u32 = 64;
const WORLD: f32 = 1024.0;

fn metrics() -> HeightfieldMetrics {
    HeightfieldMetrics::new(RES, RES, WORLD, WORLD)
}

// ---------------------------------------------------------------------------
// Finding 1a: solo skip resets the clip base even when the skipped layer is
// itself a clipped layer.
//
// `evaluate_nodes` only lets a NON-clipped layer become the clip base
// (`if !layer.common.clip_to_below { clip_base = mask; }`), so in normal
// evaluation the base survives past intervening clipped siblings. The solo
// skip path, however, resets it unconditionally:
//
//     if soloing && !node_contains_solo(node) { clip_base = None; continue; }
//
// A skipped clipped layer never contributed a base in the first place, so
// clearing the base there is wrong: the soloed clipped layer above loses its
// clipping entirely and applies at full strength everywhere.
//
// Stack (bottom -> top):
//   Base   Flat 20                       solo
//   A      Noise/Flat +80, mask = 0.5    solo   (the clip base)
//   C1     Flat +100, clip_to_below      NOT solo  (skipped)
//   C2     Flat +50,  clip_to_below      solo
//
// C2's nearest non-clipped sibling below is A, so C2 must add 50 * 0.5 = 25.
// Actual: C1's skip nulls the base, so C2 adds the full 50.
// ---------------------------------------------------------------------------
#[test]
fn repro_solo_skip_of_clipped_layer_drops_clip_base() {
    let mask_id = MaskId::new();

    // `with_c1` toggles the intervening non-soloed clipped layer. Its presence
    // must not change C2's result: it is skipped either way.
    let build = |with_c1: bool| {
        let mut stack = LayerStack::new();

        let mut base = Layer::new("Base", LayerKind::Flat(FlatParams { height: 20.0 }));
        base.common.solo = true;
        stack.push(base);

        let mut a = Layer::new("A", LayerKind::Flat(FlatParams { height: 80.0 }));
        a.common.blend = BlendMode::Add;
        a.common.solo = true;
        // Height{0..40} against the flat-20 input bakes to 0.5 everywhere.
        a.common.masks.push(MaskRef::new(mask_id));
        stack.push(a);

        if with_c1 {
            let mut c1 = Layer::new("C1", LayerKind::Flat(FlatParams { height: 100.0 }));
            c1.common.blend = BlendMode::Add;
            c1.common.clip_to_below = true;
            c1.common.solo = false; // skipped by solo
            stack.push(c1);
        }

        let mut c2 = Layer::new("C2", LayerKind::Flat(FlatParams { height: 50.0 }));
        c2.common.blend = BlendMode::Add;
        c2.common.clip_to_below = true;
        c2.common.solo = true;
        stack.push(c2);

        stack
    };

    let run = |stack: &LayerStack| {
        let mut ctx = EvalContext::new(metrics());
        ctx.mask_assets.push(MaskAsset::new(
            mask_id,
            "LowAreas",
            MaskSource::Height { min: 0.0, max: 40.0 },
        ));
        let mut eval = StackEvaluator::new();
        eval.rebuild_all(stack, &mut ctx).expect("eval")
    };

    let without_c1 = run(&build(false));
    let with_c1 = run(&build(true));

    // Sanity: the reference run really does clip C2 to A's 0.5 mask.
    let reference = without_c1.get(RES / 2, RES / 2);
    assert!(
        (reference - (20.0 + 80.0 * 0.5 + 50.0 * 0.5)).abs() < 1e-2,
        "reference stack must clip C2 to A's mask, got {reference}"
    );

    for j in 0..RES {
        for i in 0..RES {
            let a = without_c1.get(i, j);
            let b = with_c1.get(i, j);
            assert!(
                (a - b).abs() < 1e-3,
                "a solo-skipped CLIPPED layer must not clear the clip base \
                 at ({i},{j}): without C1 {a}, with C1 {b}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Finding 1b (DISPROVED): `rebuild_incremental`'s walk-back past clipped
// layers always lands on a layer that will re-emit a mask.
//
// The walk starts at `first_dirty` (the FIRST dirty index), so every index it
// walks onto is by construction clean -> `evaluate_layer` takes the cache-hit
// path -> and `want_mask` is true there because `track_bases` is computed over
// `layers[first_dirty..]` which still contains the dirty clipped layer, and
// the landing layer is non-clipped. The loop is also bounded by
// `first_dirty > 0`, so index 0 is the floor.
// ---------------------------------------------------------------------------
#[test]
fn disproved_incremental_walkback_reclips_against_base_mask() {
    let mask_id = MaskId::new();
    let mut stack = LayerStack::new();
    stack.push(Layer::new("Base", LayerKind::Flat(FlatParams { height: 20.0 })));

    let mut a = Layer::new("A", LayerKind::Flat(FlatParams { height: 80.0 }));
    a.common.blend = BlendMode::Add;
    a.common.masks.push(MaskRef::new(mask_id));
    stack.push(a);

    let mut c1 = Layer::new("C1", LayerKind::Flat(FlatParams { height: 10.0 }));
    c1.common.blend = BlendMode::Add;
    c1.common.clip_to_below = true;
    stack.push(c1);

    let mut c2 = Layer::new("C2", LayerKind::Flat(FlatParams { height: 50.0 }));
    c2.common.blend = BlendMode::Add;
    c2.common.clip_to_below = true;
    let c2_id = c2.id();
    stack.push(c2);

    let make_ctx = || {
        let mut ctx = EvalContext::new(metrics());
        ctx.mask_assets.push(MaskAsset::new(
            mask_id,
            "LowAreas",
            MaskSource::Height { min: 0.0, max: 40.0 },
        ));
        ctx
    };

    let mut eval = StackEvaluator::new();
    let mut ctx = make_ctx();
    let cold = eval.rebuild_all(&stack, &mut ctx).expect("cold");

    // Dirty only the topmost clipped layer: the walk-back must recover A.
    eval.mark_dirty_from(&stack, c2_id);
    let mut ctx = make_ctx();
    let warm = eval.rebuild_incremental(&stack, &mut ctx).expect("warm");

    for j in 0..RES {
        for i in 0..RES {
            assert!(
                (cold.get(i, j) - warm.get(i, j)).abs() < 1e-3,
                "incremental rebuild of a clipped layer must match a cold \
                 rebuild at ({i},{j}): cold {}, warm {}",
                cold.get(i, j),
                warm.get(i, j)
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Finding 1c / 4: `rebuild_incremental` jumps into the middle of a flat stack
// without restoring the skipped prefix's published outputs (or its aux).
//
// `record_reused_layer` only appends a timing row - it restores nothing. The
// only state carried into the suffix is `current` (the cached height of
// `layers[first_dirty - 1]`). Meanwhile `EvalContext::published_outputs` is
// empty on every pass: the scheduler and the eval worker both build a fresh
// `EvalContext` and seed only `quality`, `level_steps`, `mask_assets`, `aux`,
// `strata` and `masks`.
//
// So any mask referencing `MaskSource::LayerOutput { .. }` published by a
// layer BELOW `first_dirty` resolves to nothing on an incremental rebuild,
// while a cold rebuild resolves it correctly. The layer's composite silently
// changes when the artist edits something above it.
//
// `evaluate_suffix` documents exactly this precondition ("ctx must contain the
// equivalent auxiliary and published-output state produced by the skipped
// prefix"); `rebuild_incremental` skips a prefix the same way but neither
// documents nor satisfies it.
// ---------------------------------------------------------------------------
#[test]
fn repro_incremental_prefix_skip_loses_published_outputs() {
    use terra_core::fields::FieldId;
    use terra_core::layer::NamedOutputDecl;

    let mask_id = MaskId::new();
    let output = NamedOutputDecl::new("BaseHeight", FieldId::Height);
    let output_id = output.id;

    let mut stack = LayerStack::new();
    // Base publishes its height as a named output.
    let mut base = Layer::new("Base", LayerKind::Flat(FlatParams { height: 20.0 }));
    base.common.outputs.push(output);
    stack.push(base);

    // A is masked by that published output.
    let mut a = Layer::new("A", LayerKind::Flat(FlatParams { height: 100.0 }));
    a.common.blend = BlendMode::Add;
    a.common.masks.push(MaskRef::new(mask_id));
    let a_id = a.id();
    stack.push(a);

    let make_ctx = || {
        let mut ctx = EvalContext::new(metrics());
        ctx.mask_assets.push(MaskAsset::new(
            mask_id,
            "FromBaseOutput",
            MaskSource::LayerOutput { output_id },
        ));
        ctx
    };

    let mut eval = StackEvaluator::new();
    let mut ctx = make_ctx();
    let cold = eval.rebuild_all(&stack, &mut ctx).expect("cold");

    // Edit only A. Base stays clean, so the incremental pass skips it - and
    // with it, the publication of BaseHeight.
    eval.mark_dirty_from(&stack, a_id);
    let mut ctx = make_ctx();
    let warm = eval.rebuild_incremental(&stack, &mut ctx).expect("warm");

    for j in 0..RES {
        for i in 0..RES {
            assert!(
                (cold.get(i, j) - warm.get(i, j)).abs() < 1e-3,
                "an incremental rebuild must resolve masks bound to outputs \
                 published by the skipped prefix at ({i},{j}): cold {}, warm {}",
                cold.get(i, j),
                warm.get(i, j)
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Finding 2: mask chip fingerprints sample far too sparsely.
//
// `terra_app::ui::thumbnails::paint_fingerprint` hashes
// `samples.iter().step_by(len / 512)`. Paint mask resolution is
// `preview_resolution.clamp(256, 1024)` (see
// `TerrainDocument::ensure_layer_paint_mask`), so the stride is:
//
//   256x256   len 65_536    stride 128   -> columns 0 and 128 only
//   512x512   len 262_144   stride 512   -> column 0 only (stride == width!)
//   1024x1024 len 1_048_576 stride 2048  -> column 0 of every OTHER row
//
// At 512 (a very common preview resolution) the fingerprint reads a single
// column of the mask. Any stroke that does not cross the left edge is
// invisible to it, so the mask chip in the layer panel never re-renders. The
// function below is a verbatim copy of the app-side one (the app crate is not
// a dependency of terra-core), driven by a real `PaintBuffer` and a brush
// stamp of the size the mask paint tool actually uses.
// ---------------------------------------------------------------------------
#[test]
fn disproved_interleaved_redo_replays_in_chronological_order() {
    use terra_core::command::EditorCommand;
    use terra_core::document::{EditorSession, MaskPaintPatch, UndoDomain};
    use terra_core::mask::MaskAsset;

    let mut session = EditorSession::new();
    let layer = Layer::new("L", LayerKind::Flat(FlatParams { height: 1.0 }));
    let layer_id = layer.id();
    session.document.stack.push(layer);

    let mask_id = MaskId::new();
    let mut asset = MaskAsset::new_painted(mask_id, "M", 8);
    if let Some(paint) = asset.paint.as_mut() {
        paint.fill();
    }
    session.document.masks.push(asset);

    // A, B: stack edits.
    let apply = |session: &mut EditorSession, opacity: f32| {
        let cmd = EditorCommand::SetOpacity {
            id: layer_id,
            opacity,
            previous: session
                .document
                .stack
                .find(layer_id)
                .map(|l| l.common.opacity)
                .unwrap_or(1.0),
        };
        terra_core::command::apply(&cmd, &mut session.document.stack);
        session.push_command(cmd);
    };
    apply(&mut session, 0.8);
    apply(&mut session, 0.6);

    // C: a mask paint stroke.
    session.push_mask_paint_patch(MaskPaintPatch {
        mask_id,
        label: "Paint".into(),
        buffer_width: 8,
        buffer_height: 8,
        x: 0,
        y: 0,
        w: 1,
        h: 1,
        before: vec![0.0],
        after: vec![1.0],
    });

    // Undo all three, newest first.
    for _ in 0..3 {
        match session.newest_undo_domain() {
            Some(UndoDomain::MaskPaint) => {
                session.undo_mask_paint();
            }
            _ => {
                session.undo_stack_command();
            }
        }
    }

    // Redo three times: expected order is A, B, C.
    let mut order = Vec::new();
    for _ in 0..3 {
        match session.oldest_redo_domain() {
            Some(UndoDomain::MaskPaint) => {
                session.redo_mask_paint();
                order.push(UndoDomain::MaskPaint);
            }
            _ => {
                session.redo_stack_command();
                order.push(UndoDomain::Stack);
            }
        }
    }
    assert_eq!(
        order,
        vec![UndoDomain::Stack, UndoDomain::Stack, UndoDomain::MaskPaint],
        "redo must replay chronologically: A, B, then the mask stroke"
    );
    let opacity = session
        .document
        .stack
        .find(layer_id)
        .map(|l| l.common.opacity)
        .unwrap();
    assert!((opacity - 0.6).abs() < 1e-6, "redo must land back on B");
}

// ---------------------------------------------------------------------------
// Finding 3b: a new edit does not invalidate the OTHER domains' redo stacks.
//
// Within one domain the invariant holds (`push_executed` / `push_coalesced` /
// `push_mask_paint_patch` each clear their own redo vec). Across domains
// nothing does, so:
//
//   1. paint a mask stroke      -> mask_paint_undo = [C]
//   2. undo it                  -> mask_paint_redo = [C]
//   3. make a NEW stack edit D  -> only history.redo_stack is cleared
//   4. press redo               -> oldest_redo_domain == MaskPaint, and the
//                                  abandoned stroke C is re-applied
//
// That resurrects an edit the artist undid before branching, and it lands
// *underneath* D in the unified history timeline (C's seq < D's seq), so the
// History panel shows a pending-redo row older than the newest edit.
// ---------------------------------------------------------------------------
#[test]
fn repro_new_edit_does_not_clear_other_domains_redo() {
    use terra_core::command::EditorCommand;
    use terra_core::document::{EditorSession, MaskPaintPatch};
    use terra_core::mask::MaskAsset;

    let mut session = EditorSession::new();
    let layer = Layer::new("L", LayerKind::Flat(FlatParams { height: 1.0 }));
    let layer_id = layer.id();
    session.document.stack.push(layer);

    let mask_id = MaskId::new();
    let mut asset = MaskAsset::new_painted(mask_id, "M", 8);
    if let Some(paint) = asset.paint.as_mut() {
        paint.fill();
    }
    session.document.masks.push(asset);

    // 1 + 2: a mask stroke, then undo it.
    session.push_mask_paint_patch(MaskPaintPatch {
        mask_id,
        label: "Paint".into(),
        buffer_width: 8,
        buffer_height: 8,
        x: 0,
        y: 0,
        w: 1,
        h: 1,
        before: vec![0.0],
        after: vec![1.0],
    });
    assert!(session.undo_mask_paint().is_some());

    // 3: a brand-new edit in a different domain. This starts a new branch of
    // history, so every pending redo should be discarded.
    let cmd = EditorCommand::SetOpacity {
        id: layer_id,
        opacity: 0.5,
        previous: 1.0,
    };
    terra_core::command::apply(&cmd, &mut session.document.stack);
    session.push_command(cmd);

    // 4: nothing should be redoable anywhere.
    assert_eq!(
        session.oldest_redo_domain(),
        None,
        "a new edit must discard pending redos in EVERY domain, not just its own"
    );
}
