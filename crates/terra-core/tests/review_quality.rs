//! Adversarial review: quality-dependent results vs the evaluator cache.
//!
//! `EvalContext::quality` changes *what* many processors compute (iteration
//! counts, sim level schedules, and since e734f9a whether layered thermal
//! erosion takes the coarse-to-fine schedule at all), but `LayerCache` used to
//! validate an entry on `metrics.width` / `metrics.height` alone.
//!
//! When the project preview resolution is <= 512, `PreviewQuality::resolution`
//! maps Draft, Medium and Full onto the *same* grid - and Medium and Full
//! always collide at the default 1024 - so the refine ladder walked
//! Draft -> Medium -> Full against one persistent evaluator without ever
//! changing the metrics the cache validated on. It found nothing dirty and
//! served the Draft solve back while the UI reported Full; the disk store
//! carried the same confusion across restarts.
//!
//! Fixed by keying the cache on the quality rung (in memory and in `DiskMeta`),
//! and by threading the incoming `MassWastingState` through the leveled preview
//! path so it no longer publishes a bare-bedrock inventory. These tests are the
//! regression guards.

use terra_core::eval::{EvalContext, EvalWorkRequest, EvalWorker, PreviewQuality, StackEvaluator};
use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::layer::{
    BlendMode, FbmParams, FlatParams, Layer, LayerKind, LayerStack, ThermalErosionParams,
};

const RES: u32 = 256;

fn metrics() -> HeightfieldMetrics {
    HeightfieldMetrics::new(RES, RES, 2048.0, 2048.0)
}

/// Base + Fbm + layered Thermal Erosion: the default thermal layer, whose
/// solve is quality-dependent (`processors.rs:289`).
fn thermal_stack() -> LayerStack {
    let mut stack = LayerStack::new();
    stack.push(Layer::new(
        "Base",
        LayerKind::Flat(FlatParams { height: 40.0 }),
    ));
    let mut fbm = Layer::new("Fbm", LayerKind::Fbm(FbmParams::default()));
    fbm.common.blend = BlendMode::Add;
    stack.push(fbm);
    stack.push(Layer::new(
        "Thermal",
        LayerKind::ThermalErosion(ThermalErosionParams::default()),
    ));
    stack
}

fn ctx_at(quality: PreviewQuality) -> EvalContext {
    let mut ctx = EvalContext::new(metrics());
    ctx.quality = quality;
    ctx
}

fn max_abs_diff(a: &Heightfield, b: &Heightfield) -> f32 {
    let mut max = 0.0f32;
    for j in 0..a.metrics.height {
        for i in 0..a.metrics.width {
            max = max.max((a.get(i, j) - b.get(i, j)).abs());
        }
    }
    max
}

/// Draft, Medium and Full collapse onto one resolution for any project whose
/// preview resolution is <= 512 - which is exactly the situation in which the
/// cache's width/height validation cannot tell the rungs apart.
#[test]
fn preview_ladder_shares_one_resolution_at_or_below_512() {
    for preview in [128u32, 256, 512] {
        let draft = PreviewQuality::Draft.resolution(preview, 4096);
        let medium = PreviewQuality::Medium.resolution(preview, 4096);
        let full = PreviewQuality::Full.resolution(preview, 4096);
        assert_eq!(
            (draft, medium),
            (full, full),
            "preview {preview}: the ladder is expected to share a grid here"
        );
    }
    // And at the *default* preview resolution (DEFAULT_PREVIEW_RES = 1024) the
    // top two rungs still share a grid, so the defect below is not restricted
    // to small worlds - the default project reaches it on the Medium -> Full
    // step.
    for preview in [768u32, 1024] {
        assert_eq!(
            PreviewQuality::Medium.resolution(preview, 4096),
            PreviewQuality::Full.resolution(preview, 4096),
            "preview {preview}: Medium and Full share a grid"
        );
    }
}

/// The Medium -> Full rung has the same defect, which is what the *default*
/// 1024 project hits (Draft resolves to 512 there, Medium and Full both to
/// 1024). Same mechanism as finding 1, one rung higher.
#[test]
fn refining_from_medium_to_full_recomputes_quality_dependent_layers() {
    let stack = thermal_stack();

    let mut oracle = StackEvaluator::new();
    let mut ctx = ctx_at(PreviewQuality::Full);
    let full = oracle.rebuild_all(&stack, &mut ctx).unwrap();

    let mut eval = StackEvaluator::new();
    eval.mark_all_dirty(&stack);
    let mut ctx = ctx_at(PreviewQuality::Medium);
    let medium = eval.rebuild_incremental(&stack, &mut ctx).unwrap();
    assert!(
        max_abs_diff(&medium, &full) > 1e-4,
        "precondition: Medium and Full must differ"
    );

    let mut ctx = ctx_at(PreviewQuality::Full);
    let refined = eval.rebuild_incremental(&stack, &mut ctx).unwrap();
    assert!(
        max_abs_diff(&refined, &full) < 1e-4,
        "refining Medium -> Full returned the Medium result (deviation {:.4} m)",
        max_abs_diff(&refined, &full),
    );
}

/// FINDING 1: a layer computed at Draft is served from cache to a later Full
/// evaluation on the same persistent evaluator, so the refine ladder never
/// produces Full-quality terrain.
///
/// This mirrors the real worker loop: `EvalWorker::spawn` keeps one
/// `StackEvaluator` for the process lifetime (`eval/worker.rs:113`), and the
/// refine ladder submits Medium/Full jobs with `mark_all_dirty == false`
/// (`terra-app/src/app/eval.rs:114`, driven from
/// `terra-app/src/app/lifecycle.rs:677-714`).
#[test]
fn refining_from_draft_to_full_recomputes_quality_dependent_layers() {
    let stack = thermal_stack();

    // Oracle: what Full is supposed to look like.
    let mut oracle = StackEvaluator::new();
    let mut ctx = ctx_at(PreviewQuality::Full);
    let full = oracle.rebuild_all(&stack, &mut ctx).unwrap();

    // The persistent evaluator: Draft rung first ...
    let mut eval = StackEvaluator::new();
    eval.mark_all_dirty(&stack);
    let mut ctx = ctx_at(PreviewQuality::Draft);
    let draft = eval.rebuild_incremental(&stack, &mut ctx).unwrap();
    assert!(
        max_abs_diff(&draft, &full) > 1e-4,
        "precondition: Draft and Full must differ, else this test proves nothing"
    );

    // ... then the refine ladder asks the same evaluator for Full. Nothing was
    // edited, so nothing is dirty, and the metrics are identical.
    let mut ctx = ctx_at(PreviewQuality::Full);
    let refined = eval.rebuild_incremental(&stack, &mut ctx).unwrap();

    assert!(
        max_abs_diff(&refined, &full) < 1e-4,
        "refining to Full returned the Draft result (max deviation from Full \
         {:.4} m, vs {:.4} m for the Draft pass itself): the layer cache is \
         validated only on width/height (eval/cache.rs:92-96, 107-111), so a \
         Draft-quality entry satisfies a Full-quality request at the same \
         resolution and the ladder silently stops refining",
        max_abs_diff(&refined, &full),
        max_abs_diff(&draft, &full),
    );
}

/// FINDING 2: the same defect through the real background worker, which is how
/// the app reaches it.
///
/// This is the exact job sequence `TerraApp` submits: `request_rebuild` starts
/// the ladder at Draft (`terra-app/src/app/eval.rs:21-31`), the first job
/// consumes `worker_mark_all_dirty` (`eval.rs:114`, `std::mem::take`), and each
/// refine tick submits the next rung with `mark_all_dirty == false` and
/// `dirty_from == None` (`lifecycle.rs:677-714` -> `enqueue_refine_job` ->
/// `enqueue_async_eval`). All three rungs share one grid because the project's
/// preview resolution is 256.
///
/// The result the app then publishes is labelled Full - `ui_state.quality =
/// quality`, `profile.quality = "Final (viewport)"`, `refining = false`
/// (`lifecycle.rs:595-610`) - so the user is told the terrain settled at Full
/// while looking at the Draft solve.
#[test]
fn worker_refine_ladder_reaches_full_quality_terrain() {
    // One stack, reused across rungs: the app resubmits the *same* document,
    // so the layer ids - the cache keys - are identical every time.
    let stack = thermal_stack();
    let request = |quality: PreviewQuality, token: u64, mark_all_dirty: bool| -> EvalWorkRequest {
        EvalWorkRequest {
            token,
            quality,
            stack: stack.clone(),
            masks: Vec::new(),
            base_metrics: metrics(),
            level_steps: terra_core::analyze::LevelStepSettings::default(),
            preview_res: RES,
            export_res: 4096,
            aux: Default::default(),
            strata: None,
            mask_reference: None,
            mark_all_dirty,
            dirty_from: None,
        }
    };

    fn wait(worker: &mut EvalWorker, token: u64) -> terra_core::eval::EvalWorkResult {
        for _ in 0..2000 {
            if let Some(result) = worker.try_recv_matching(token) {
                return result;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("worker produced no result");
    }

    // Oracle: Full computed by an evaluator that has never seen another quality.
    let mut oracle = StackEvaluator::new();
    let mut ctx = ctx_at(PreviewQuality::Full);
    let full = oracle.rebuild_all(&stack, &mut ctx).unwrap();

    let token = 1;
    let mut worker = EvalWorker::spawn();
    worker.submit(request(PreviewQuality::Draft, token, true));
    let draft = wait(&mut worker, token);
    worker.submit(request(PreviewQuality::Medium, token, false));
    let _medium = wait(&mut worker, token);
    worker.submit(request(PreviewQuality::Full, token, false));
    let refined = wait(&mut worker, token);

    assert_eq!(refined.quality, PreviewQuality::Full);
    assert!(
        max_abs_diff(&refined.height, &full) < 1e-4,
        "the worker reported a Full-quality result that is still the Draft \
         solve (deviation from a true Full solve: {:.4} m; the Draft rung's own \
         deviation was {:.4} m). The worker keeps one StackEvaluator for the \
         process lifetime (eval/worker.rs:113) and the refine rungs carry \
         mark_all_dirty == false, so every layer is a cache hit at the same \
         resolution and the ladder never recomputes anything",
        max_abs_diff(&refined.height, &full),
        max_abs_diff(&draft.height, &full),
    );
}

// ---------------------------------------------------------------------------
// Commit e734f9a: the leveled preview path for layered thermal erosion.
// ---------------------------------------------------------------------------

/// Every aux channel the leveled preview path publishes is upsampled back to
/// the target grid (`analyze/level_step.rs:510-521`), so no consumer sees a
/// coarse field. PIN: narrowing that (publishing a level-resolution field) must
/// fail loudly.
#[test]
fn leveled_thermal_publishes_aux_at_native_resolution() {
    use terra_core::fields::keys;

    let stack = thermal_stack();
    let mut eval = StackEvaluator::new();
    let mut ctx = ctx_at(PreviewQuality::Draft);
    let height = eval.rebuild_all(&stack, &mut ctx).unwrap();
    assert_eq!(height.metrics.width, RES);
    for key in [
        keys::BEDROCK_HEIGHT,
        keys::DEBRIS_DEPTH,
        keys::SEDIMENT_THICKNESS,
        keys::TALUS_STABILITY,
        keys::INSTABILITY,
        keys::EROSION,
        keys::DEPOSITION,
    ] {
        let field = ctx
            .aux_maps
            .get(key)
            .unwrap_or_else(|| panic!("{key} must be published"));
        assert_eq!(
            (field.metrics.width, field.metrics.height),
            (RES, RES),
            "{key} must be published at the evaluation resolution"
        );
    }
}

/// FINDING 3: the leveled preview path passes `None` as the incoming
/// [`MassWastingState`] for every level (`analyze/level_step.rs:504`), so it
/// silently deletes the bedrock / debris / sediment inventory a preceding sim
/// published. The Full path passes `Some(&initial)`
/// (`eval/processors.rs:292`) and preserves it.
///
/// Before e734f9a the layered path always passed `Some(&initial)`, so this is a
/// preview/final divergence the commit introduced. `sediment_thickness` is not
/// an internal detail: `mask/bake.rs:98` resolves the `sediment_depth` and
/// `loose_sediment` mask sources onto it, and `EcosystemFeedback` reads it
/// (`eval/processors.rs:171-175`), so a mask driven by soil depth resolves to
/// zero in preview and to real values at Full.
#[test]
fn leveled_thermal_preserves_incoming_layered_state() {
    use terra_core::fields::keys;
    use terra_core::mask::MaskField;

    let stack = thermal_stack();
    let n = (RES * RES) as usize;

    // Seed the inventory a preceding mass-wasting sim would have published:
    // 3 m of fine sediment over 5 m of loose debris.
    let seed = |ctx: &mut EvalContext| {
        ctx.aux_insert(
            keys::SEDIMENT_THICKNESS,
            MaskField::from_raw(metrics(), &vec![3.0; n]),
        );
        ctx.aux_insert(
            keys::DEBRIS_DEPTH,
            MaskField::from_raw(metrics(), &vec![5.0; n]),
        );
    };

    let mut full_eval = StackEvaluator::new();
    let mut full_ctx = ctx_at(PreviewQuality::Full);
    seed(&mut full_ctx);
    full_eval.rebuild_all(&stack, &mut full_ctx).unwrap();
    let full_sediment = full_ctx
        .aux_maps
        .get(keys::SEDIMENT_THICKNESS)
        .unwrap()
        .get(RES / 2, RES / 2);
    assert!(
        full_sediment > 1.0,
        "precondition: the Full path carries the incoming sediment column \
         (got {full_sediment} m)"
    );

    let mut draft_eval = StackEvaluator::new();
    let mut draft_ctx = ctx_at(PreviewQuality::Draft);
    seed(&mut draft_ctx);
    draft_eval.rebuild_all(&stack, &mut draft_ctx).unwrap();
    let draft_sediment = draft_ctx
        .aux_maps
        .get(keys::SEDIMENT_THICKNESS)
        .unwrap()
        .get(RES / 2, RES / 2);
    let draft_debris = draft_ctx
        .aux_maps
        .get(keys::DEBRIS_DEPTH)
        .unwrap()
        .get(RES / 2, RES / 2);

    assert!(
        draft_sediment > 1.0 && draft_debris > 1.0,
        "the Draft rung published sediment_thickness = {draft_sediment} m and \
         debris_depth = {draft_debris} m where 3 m of sediment and 5 m of \
         debris entered the layer, and Full published {full_sediment} m of \
         sediment: thermal_erode_layered_leveled passes `None` for the incoming \
         MassWastingState at every level, so the preview rungs erase an \
         upstream sim's soil column instead of eroding it"
    );
}

/// FINDING 1b: the same defect persists to disk and therefore across app
/// restarts. `DiskSmartCache`'s `DiskMeta` records version, metrics,
/// generation and aux names (`eval/smart_cache.rs:26-31`) - no quality - so a
/// baked (`cached`) layer solved at Draft is reloaded as a valid Full result by
/// a brand new evaluator. (The whole stack is baked here because the disk
/// reload is only reached from `rebuild_incremental`'s clean-prefix scan -
/// `eval/mod.rs:421-427` - so an uncached layer below would force a rebuild
/// anyway. `DiskSmartCache::default_location` is a stable temp path, so in the
/// app this outlives the process.)
#[test]
fn baked_disk_checkpoint_is_not_reused_across_qualities() {
    let root = std::env::temp_dir().join(format!(
        "terra_review_quality_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut stack = thermal_stack();
    // Baked checkpoints (the Smart Cache affordance) for the whole stack, so a
    // fresh session resolves every layer from disk.
    for node in stack.nodes.iter_mut() {
        if let terra_core::layer::StackNode::Layer(layer) = node {
            layer.common.cached = true;
        }
    }

    let mut oracle = StackEvaluator::new();
    oracle.cache.disable_disk();
    let mut ctx = ctx_at(PreviewQuality::Full);
    let full = oracle.rebuild_all(&stack, &mut ctx).unwrap();

    // Session 1: Draft, spilled to disk.
    {
        let mut eval = StackEvaluator::new();
        eval.cache.enable_disk(&root);
        eval.mark_all_dirty(&stack);
        let mut ctx = ctx_at(PreviewQuality::Draft);
        eval.rebuild_incremental(&stack, &mut ctx).unwrap();
    }

    // Session 2: a fresh evaluator (empty memory cache) asked for Full.
    let mut eval = StackEvaluator::new();
    eval.cache.enable_disk(&root);
    let mut ctx = ctx_at(PreviewQuality::Full);
    let reloaded = eval.rebuild_incremental(&stack, &mut ctx).unwrap();
    let deviation = max_abs_diff(&reloaded, &full);
    let _ = std::fs::remove_dir_all(&root);

    assert!(
        deviation < 1e-4,
        "a Full evaluation was served a Draft-quality disk checkpoint \
         (deviation {deviation:.4} m): DiskMeta carries no quality, so a baked \
         layer solved at Draft satisfies a Full request forever, including \
         after a restart"
    );
}
