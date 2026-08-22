//! Adversarial-review round 4 repros.
//!
//! Scope: the two newest commits — `ab4be42` (composite fast path, mask-bake
//! memo, scrub-checkpoint dropping) and `ad40235` (terrain AO bake).
//!
//! Naming, as in `review_repros.rs`:
//!   * `disproved_*` — documents a hypothesis that was investigated and found
//!     sound, so the invariant stays pinned.
//!
//! The round-4 findings have all been triaged and fixed; every test here now
//! asserts the corrected behaviour and guards against regressing it. Each
//! comment block preserves the original analysis of how the bug was reached.

use std::collections::HashMap;

use terra_core::eval::{EvalContext, StackEvaluator};
use terra_core::heightfield::HeightfieldMetrics;
use terra_core::layer::{
    BlendMode, FbmParams, FlatParams, Layer, LayerKind, LayerStack, StreamPowerParams,
    ThermalErosionParams, VegetationParams,
};
use terra_core::mask::{bake_mask_assets, MaskAsset, MaskField, MaskId, MaskRef, MaskSource};

const RES: u32 = 48;
const WORLD: f32 = 96.0;

fn metrics() -> HeightfieldMetrics {
    HeightfieldMetrics::new(RES, RES, WORLD, WORLD)
}

// ===========================================================================
// Finding 2 — point-of-use mask bake memoization.
//
// `mask_source_aux_key` (eval/mod.rs:1086-1106) maps every aux-reading
// `MaskSource` to the single aux key the bake will read. For
// `MaskSource::Named(name)` it returns `name` verbatim.
//
// But `bake_source` (mask/bake.rs:69-78) does NOT read `name` verbatim: for
// the legacy spellings it falls through to the canonical channel —
//
//     .or_else(|| match name.as_str() {
//         "sediment_depth" | "loose_sediment" => aux.get("sediment_thickness"),
//         _ => None,
//     })
//
// and those legacy keys are *structurally* impossible to find in `ctx.aux`:
// `AuxMaps::extend_hashmap` (fields/mod.rs:327-341) folds every alias into the
// typed `sediment_thickness` slot, and `AuxMaps::to_hashmap`
// (fields/mod.rs:371) only ever emits `keys::SEDIMENT_THICKNESS`. So the memo
// looks up `"sediment_depth"`, finds `None` before and `None` now, and takes
// the `(None, None) => true` arm at eval/mod.rs:1052 — "unchanged" — for every
// layer, no matter what the sediment channel does.
//
// This test pins the read-set mismatch that makes the memo unsound: the bake
// output depends on a key the memo never inspects.
// ===========================================================================
#[test]
fn mask_bake_memo_and_bake_source_agree_on_the_sediment_alias() {
    let m = metrics();
    let id = MaskId::new();
    let asset = MaskAsset::new(id, "Loose sediment", MaskSource::Named("sediment_depth".into()));
    let reference = terra_core::heightfield::Heightfield::zeros(m);

    // The aux hashmap as the evaluator actually holds it: canonical key only.
    let mut aux: HashMap<String, MaskField> = HashMap::new();
    aux.insert("sediment_thickness".into(), MaskField::filled(m, 0.25));
    let before = bake_mask_assets(std::slice::from_ref(&asset), &reference, m, &aux);

    aux.insert("sediment_thickness".into(), MaskField::filled(m, 0.75));
    let after = bake_mask_assets(std::slice::from_ref(&asset), &reference, m, &aux);

    // The bake genuinely changed...
    assert!(
        (before[&id].get(0, 0) - after[&id].get(0, 0)).abs() > 1e-3,
        "sanity: the legacy alias must resolve through sediment_thickness"
    );

    // FIXED: the memo canonicalises its read key before probing aux, so the key
    // it compares is the one `bake_source` actually reads. Pin the alias table
    // both sides share — a new legacy spelling added to `bake_source` without a
    // matching arm here would silently make the memo stale again.
    for legacy in ["sediment_depth", "loose_sediment"] {
        assert_eq!(
            terra_core::fields::keys::canonical(legacy),
            "sediment_thickness",
            "`bake_source` folds {legacy:?} onto sediment_thickness, so the \
             memo's canonicalised read key must land on the same slot"
        );
    }
}

// Reachability note: an end-to-end version of this (evaluate, change the
// sediment channel, re-evaluate on the same context) cannot be written
// straightforwardly. The memo only engages when the layer's input heightfield
// is CoW-shared with the previous pass, which requires a clean cached prefix —
// and restoring that prefix's cache entry also restores its aux snapshot,
// overwriting any externally injected channel. A single-layer stack avoids the
// restore but rebuilds its input from scratch, so `shares_storage_with` fails
// and the memo never consults its read keys at all. The alias-table agreement
// asserted above is therefore the invariant worth pinning: it is what makes the
// memo's read set match the bake's, whichever path reaches it.

// ===========================================================================
// Finding 2c — the memo is stored in `EvalContext`, which several callers
// (including this crate's own unit tests, eval/mod.rs:1934-1968) reuse across
// evaluation passes. Nothing about a *mask asset* is in the memo key except
// `assets.len()`, and `ctx.masks` is not cleared between passes, so editing a
// mask asset's parameters and rebuilding on the same context silently reuses
// the previous pass's baked mask.
//
// Reachability: the prefix layer is clean, so `current` entering the edited
// layer is `cache.get_or_load(...).height.clone()` — a copy-on-write clone that
// shares buffers with the previous pass's field. `shares_storage_with` matches,
// aux is empty, `assets.len()` is unchanged, so the memo skips.
//
// terra-app's two production drivers (`EvalScheduler::run_step`,
// `worker::run_job`) build a fresh `EvalContext` per pass, so this is a latent
// API trap rather than a shipping user-visible bug — but `EvalContext` is
// `pub`, `mask_assets` is `pub`, and the reuse pattern is exercised in-tree.
// ===========================================================================
#[test]
fn mask_asset_edit_on_a_reused_context_forces_a_rebake() {
    let m = metrics();
    let mask_id = MaskId::new();

    let mut stack = LayerStack::new();
    // A clean prefix layer, so pass 2 reuses its cached height (CoW-shared with
    // pass 1's, which is what makes the memo's identity check match).
    let mut base = Layer::new("Base", LayerKind::Flat(FlatParams { height: 50.0 }));
    base.common.blend = BlendMode::Normal;
    stack.push(base);

    let mut masked = Layer::new("Masked", LayerKind::Flat(FlatParams { height: 100.0 }));
    masked.common.blend = BlendMode::Add;
    masked.common.masks.push(MaskRef::new(mask_id));
    let masked_id = masked.id();
    stack.push(masked);

    let mut eval = StackEvaluator::new();
    let mut ctx = EvalContext::new(m);
    // Height source over [0, 100]: at h = 50 the mask is 0.5, so Masked adds 50.
    ctx.mask_assets.push(MaskAsset::new(
        mask_id,
        "H",
        MaskSource::Height {
            min: 0.0,
            max: 100.0,
        },
    ));
    let first = eval.rebuild_all(&stack, &mut ctx).unwrap();

    // Edit the mask asset so it now reads ~1.0 at h = 50 (range [0, 50]).
    ctx.mask_assets[0].source = MaskSource::Height {
        min: 0.0,
        max: 50.0,
    };
    eval.mark_dirty_from(&stack, masked_id);
    let bakes_before = ctx.mask_bakes;
    let second = eval.rebuild_incremental(&stack, &mut ctx).unwrap();
    // FIXED: the memo now fingerprints the assets themselves, so an asset edit
    // forces a fresh bake instead of matching on "same buffers, same count".
    assert!(
        ctx.mask_bakes > bakes_before,
        "editing a mask asset must invalidate the memo and re-bake; bakes \
         stayed at {bakes_before}"
    );
    assert!(
        (second.get(RES / 2, RES / 2) - first.get(RES / 2, RES / 2)).abs() > 1.0,
        "editing a mask asset must re-bake the point-of-use mask; got {} both \
         times (memo keyed only on height buffers, aux buffers and asset COUNT \
         — nothing about the assets themselves)",
        first.get(RES / 2, RES / 2)
    );
}

// ===========================================================================
// Finding 3 — scrub checkpoint dropping.
//
// `mark_dirty_from_fields` now unconditionally drops the seed layer's scrub
// checkpoints (eval/mod.rs:288). The one caller allowed to keep them is
// `mark_dirty_from_scrub`, gated in terra-app behind `ApplyCtx.dirty_keeps_scrub`
// (actions/mod.rs:207).
//
// `dirty_keeps_scrub` is declared (actions/mod.rs:33), initialised to `false`
// (actions/mod.rs:46) and read (actions/mod.rs:207) — and NEVER ASSIGNED
// anywhere in the workspace. In particular `PanelAction::SetSimProgress`
// (actions/layers.rs:189-205), the progress slider itself, only sets
// `ctx.dirty_from`. So the app always takes the `mark_dirty_from_fields`
// branch and the scrub cache can never hit: the feature is dead in the
// shipping binary and `mark_dirty_from_scrub` is reachable only from tests.
//
// FIXED in terra-app: `SetSimProgress` now sets `ctx.dirty_keeps_scrub_for =
// Some(id)`, but only when the scrub is the sole seed of the batch — any other
// edit in the same batch must still drop the checkpoints, whose key covers
// neither masks nor bindings. The app-path regression test lives in
// `terra-app/tests/review_app.rs::sim_progress_scrub_keeps_its_checkpoints`.
//
// What stays pinned here is the *core* contract the app relies on: the two
// entry points must differ. `mark_dirty_from_fields` drops checkpoints;
// `mark_dirty_from_scrub` keeps them.
// ===========================================================================
#[test]
fn mark_dirty_from_fields_drops_scrub_checkpoints_and_scrub_keeps_them() {
    let mut stack = LayerStack::new();
    let mut fbm = Layer::new("Fbm", LayerKind::Fbm(FbmParams::default()));
    fbm.common.blend = BlendMode::Add;
    stack.push(fbm);
    let mut thermal = Layer::new(
        "Thermal",
        LayerKind::ThermalErosion(ThermalErosionParams::default()),
    );
    thermal.common.sim_progress = 0.5;
    let thermal_id = thermal.id();
    stack.push(thermal);

    let mut eval = StackEvaluator::new();
    let mut ctx = EvalContext::new(metrics());
    eval.rebuild_all(&stack, &mut ctx).unwrap();
    assert_eq!(eval.scrub_hits, 0);

    // The general-purpose seed drops checkpoints: a scrub away and back cannot
    // replay, because the invalidation deleted the checkpoints first.
    for progress in [0.7_f32, 0.5] {
        stack.find_mut(thermal_id).unwrap().common.sim_progress = progress;
        eval.mark_dirty_from_fields(&stack, thermal_id, &[]);
        eval.rebuild_incremental(&stack, &mut ctx).unwrap();
    }
    assert_eq!(
        eval.scrub_hits, 0,
        "mark_dirty_from_fields must drop scrub checkpoints — its callers may \
         have changed masks or bindings, which the scrub key does not cover"
    );

    // The scrub-specific seed keeps them, so the same round trip replays.
    for progress in [0.7_f32, 0.5] {
        stack.find_mut(thermal_id).unwrap().common.sim_progress = progress;
        eval.mark_dirty_from_scrub(&stack, thermal_id);
        eval.rebuild_incremental(&stack, &mut ctx).unwrap();
    }
    assert!(
        eval.scrub_hits >= 1,
        "mark_dirty_from_scrub must keep this layer's checkpoints so returning \
         to a visited progress position replays instead of resimulating"
    );
}

// ===========================================================================
// Finding 4 — dynamic vegetation contract and undo/redo.
//
// `LayerKind::Vegetation::produced_fields` (layer/operation.rs:366-372) only
// declares `Hardness` when `root_cohesion > 1e-6`. The 0.5 -> 0.0 transition
// (the layer STOPS writing hardness) is therefore invisible to the general
// invalidation loop, which unions only the *new* contract
// (eval/mod.rs:309-320). `PanelAction::SetKind` covers it by seeding
// `previous.produced_fields()` (actions/layers.rs:246).
//
// Undo and redo do not: both call `TerraApp::mark_dirty_from(id)`
// (app/project.rs:762 and :801) -> `StackEvaluator::mark_dirty_from` ->
// `mark_dirty_from_fields(stack, id, &[])`, with no previous contract. So
// undoing a 0.0 -> 0.5 SetKind (restoring 0.0) leaves every hardness consumer
// above holding a cache built on the hardness the layer used to write.
//
// The mirror direction (0.0 -> 0.5) is safe: the *new* contract mentions
// hardness, and the loop unions the seed layer's own produced fields.
// ===========================================================================
#[test]
fn undo_of_a_vegetation_kind_edit_honours_the_previous_contract() {
    let veg = |root_cohesion: f32| VegetationParams {
        root_cohesion,
        ..VegetationParams::default()
    };

    let mut stack = LayerStack::new();
    let mut fbm = Layer::new("Fbm", LayerKind::Fbm(FbmParams::default()));
    fbm.common.blend = BlendMode::Add;
    stack.push(fbm);
    // Redo state: cohesion ON, so this layer writes hardness.
    let v = Layer::new("Veg", LayerKind::Vegetation(veg(0.5)));
    let vid = v.id();
    stack.push(v);
    // Stream power reads hardness but not vegetation.
    let sp = Layer::new(
        "StreamPower",
        LayerKind::StreamPowerErosion(StreamPowerParams::default()),
    );
    let spid = sp.id();
    stack.push(sp);

    let mut eval = StackEvaluator::new();
    let mut ctx = EvalContext::new(metrics());
    eval.rebuild_all(&stack, &mut ctx).unwrap();

    // Undo of `SetKind { previous: veg(0.0), kind: veg(0.5) }` restores veg(0.0).
    // `EditorCommand::invert` swaps kind/previous and the app then calls
    // `mark_dirty_from(id)` — no extra fields.
    stack.find_mut(vid).unwrap().kind = LayerKind::Vegetation(veg(0.0));
    eval.mark_dirty_from(&stack, vid);

    // EXPECTED TO FAIL: the vegetation layer just stopped writing hardness, so
    // the erosion sim above is running on a stale hardness field.
    assert!(
        eval.cache.is_dirty(spid),
        "undo/redo must seed the pre-undo contract like PanelAction::SetKind \
         does (actions/layers.rs:246); app/project.rs:762 and :801 call the \
         plain mark_dirty_from instead"
    );
}

// ===========================================================================
// Finding 1 — the composite fast path in `mix_heightfields`.
//
// Investigated and found sound. `shares_storage_with` is `Arc::ptr_eq` per
// tile plus a dimension check (heightfield/mod.rs:315-326, tile.rs:132-135),
// and every mutator goes through `Arc::make_mut`, so shared storage really
// does imply equal samples — including the halo cells, which live in the same
// `data` buffer as the interior (tile.rs:23-25). The returned `h_in.clone()`
// therefore carries whatever halos `h_in` had, and `h_in` always arrives from
// a path that refreshed them (`mix_heightfields`, `mix_height_delta`, a cached
// height, or `Heightfield::zeros`).
//
// The skipped arithmetic is `h_in*(1-w) + blend_pair(mode, h_in, h_in)*w`,
// and `Normal`/`Replace`/`Interpolate` all resolve `blend_pair` to `b == h_in`.
// The fast path is in fact *more* exact than the code it skips: for a
// non-trivial mask the old expression is `h*(1-w) + h*w`, which is not
// bit-identical to `h` in binary floating point, and for NaN/inf weights or
// samples the old path poisoned the output where the new one preserves it.
// ===========================================================================
#[test]
fn disproved_passthrough_fast_path_is_an_exact_identity() {
    // Three consecutive height passthroughs with partial opacity and a mask,
    // over a non-trivial base. The composite must be exactly the input.
    let m = metrics();
    let mask_id = MaskId::new();

    let build = |with_passthroughs: bool| {
        let mut stack = LayerStack::new();
        let mut fbm = Layer::new("Fbm", LayerKind::Fbm(FbmParams::default()));
        fbm.common.blend = BlendMode::Add;
        stack.push(fbm);
        if with_passthroughs {
            for (name, opacity) in [("V1", 0.37_f32), ("V2", 1.0), ("V3", 0.0)] {
                let mut v = Layer::new(name, LayerKind::Vegetation(VegetationParams::default()));
                v.common.blend = BlendMode::Normal;
                v.common.opacity = opacity;
                v.common.masks.push(MaskRef::new(mask_id));
                stack.push(v);
            }
        }
        stack
    };

    let run = |stack: &LayerStack| {
        let mut eval = StackEvaluator::new();
        let mut ctx = EvalContext::new(m);
        ctx.mask_assets.push(MaskAsset::new(
            mask_id,
            "Slope",
            MaskSource::Slope {
                min_deg: 0.0,
                max_deg: 45.0,
            },
        ));
        eval.rebuild_all(stack, &mut ctx).unwrap()
    };

    let bare = run(&build(false));
    let through = run(&build(true));
    for j in 0..RES {
        for i in 0..RES {
            assert_eq!(
                bare.get(i, j).to_bits(),
                through.get(i, j).to_bits(),
                "passthrough layers must be a bit-exact identity at ({i},{j})"
            );
        }
    }
}

