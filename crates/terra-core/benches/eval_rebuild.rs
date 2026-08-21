//! Evaluation benchmarks: full rebuild, incremental suffix rebuild,
//! field-aware invalidation, and isolated-group cache reuse.
//!
//! ```text
//! cargo bench -p terra-core --bench eval_rebuild
//! ```
//!
//! CPU only - nothing here touches wgpu or the GPU evaluator.

mod common;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;
use std::time::Duration;

use common::{field_skip_stack, grouped_stack, layer_ids, metrics, primed, representative_stack};
use terra_core::eval::{EvalContext, StackEvaluator};

/// Simulation-bearing stacks are far too slow for criterion's 100-sample
/// default. Ten samples over a short measurement window still separates a
/// real regression from noise, and keeps the whole suite inside its budget.
fn sim_group<'a>(
    c: &'a mut Criterion,
    name: &str,
) -> criterion::BenchmarkGroup<'a, criterion::measurement::WallTime> {
    let mut g = c.benchmark_group(name);
    g.sample_size(10);
    g.warm_up_time(Duration::from_millis(500));
    g.measurement_time(Duration::from_secs(5));
    g
}

/// Cold `rebuild_all` on `[flat, fbm, blur, thermal]`: the "open a document"
/// / "change something structural" path. Every layer recomputes.
fn bench_rebuild_all(c: &mut Criterion) {
    let mut g = sim_group(c, "rebuild_all");
    for res in [256u32, 512] {
        g.throughput(Throughput::Elements((res as u64) * (res as u64)));
        g.bench_with_input(BenchmarkId::from_parameter(res), &res, |b, &res| {
            let stack = representative_stack();
            b.iter(|| {
                let mut eval = StackEvaluator::new();
                let mut ctx = EvalContext::new(metrics(res));
                black_box(eval.rebuild_all(&stack, &mut ctx).unwrap())
            });
        });
    }
    g.finish();
}

/// Interactive "tweak the top layer" path: everything below the edited layer
/// stays cached, so only the thermal sim on top re-runs. Compare against
/// `rebuild_all` at the same resolution to read the incremental win.
///
/// The `mark_dirty_from` call is inside the timed closure because the
/// evaluator must be re-dirtied every iteration; it is a handful of hash-set
/// operations and is negligible against a suffix evaluation.
fn bench_rebuild_incremental_top(c: &mut Criterion) {
    let mut g = sim_group(c, "rebuild_incremental_top_layer");
    for res in [256u32, 512] {
        g.throughput(Throughput::Elements((res as u64) * (res as u64)));
        g.bench_with_input(BenchmarkId::from_parameter(res), &res, |b, &res| {
            let stack = representative_stack();
            let top = *layer_ids(&stack).last().unwrap();
            let (mut eval, mut ctx) = primed(&stack, res);
            b.iter(|| {
                eval.mark_dirty_from(&stack, top);
                black_box(eval.rebuild_incremental(&stack, &mut ctx).unwrap())
            });
        });
    }
    g.finish();
}

/// Field-aware skip. Stack is `[fbm, vegetation, blur, thermal]`.
///
/// * `dirty_vegetation` - vegetation writes a vegetation field and never
///   touches height, so `mark_dirty_from` should leave blur and thermal clean
///   and the rebuild should skip both.
/// * `dirty_base_noise` - the control: dirtying the bottom fbm layer changes
///   height, which forces every layer above it to recompute.
///
/// The ratio between the two is the invalidation win.
///
/// What it measures *today*: the blur is skipped (cache hit), but the thermal
/// layer is **not**. `LayerKind::Vegetation` statically declares that it
/// produces `FieldId::Hardness` - deliberately, because contracts are static
/// and root cohesion may be enabled - and thermal erosion reads hardness, so
/// thermal is invalidated even with `root_cohesion` at its default 0. The win
/// is therefore only the filter, not the simulation. If the contract is ever
/// narrowed to the dynamic case, this ratio is where the improvement lands.
fn bench_field_aware_skip(c: &mut Criterion) {
    let res = 256u32;
    let mut g = sim_group(c, "field_aware_invalidation_256");

    g.bench_function("dirty_vegetation", |b| {
        let stack = field_skip_stack();
        let ids = layer_ids(&stack);
        let vegetation = ids[1];
        let (mut eval, mut ctx) = primed(&stack, res);
        b.iter(|| {
            eval.mark_dirty_from(&stack, vegetation);
            black_box(eval.rebuild_incremental(&stack, &mut ctx).unwrap())
        });
    });

    g.bench_function("dirty_base_noise", |b| {
        let stack = field_skip_stack();
        let ids = layer_ids(&stack);
        let base = ids[0];
        let (mut eval, mut ctx) = primed(&stack, res);
        b.iter(|| {
            eval.mark_dirty_from(&stack, base);
            black_box(eval.rebuild_incremental(&stack, &mut ctx).unwrap())
        });
    });

    g.finish();
}

/// Isolated-group cache reuse. The stack is a flat base plus one isolated
/// group of three layers (fbm + blur + thermal).
///
/// * `warm_cache_hit` - nothing dirty: `try_reuse_group_cache` validates the
///   group's stored `height_fingerprint` against the incoming height and
///   replays the cached composite instead of re-running the children.
/// * `cold_full_group` - everything dirty: the children actually evaluate.
///
/// `cold_full_group / warm_cache_hit` is what the group cache buys, and
/// `warm_cache_hit` is the ceiling on what group-cache *validation* may cost
/// before it eats the win (see the `fingerprint` bench).
fn bench_group_cache(c: &mut Criterion) {
    let res = 256u32;
    let mut g = sim_group(c, "group_cache_256");

    g.bench_function("warm_cache_hit", |b| {
        let stack = grouped_stack();
        let (mut eval, mut ctx) = primed(&stack, res);
        // Second evaluation with nothing dirty must already be a hit before
        // we start timing, otherwise this measures a cold pass.
        let _ = eval.rebuild_incremental(&stack, &mut ctx).unwrap();
        b.iter(|| black_box(eval.rebuild_incremental(&stack, &mut ctx).unwrap()));
    });

    g.bench_function("cold_full_group", |b| {
        let stack = grouped_stack();
        let (mut eval, mut ctx) = primed(&stack, res);
        b.iter(|| {
            eval.mark_all_dirty(&stack);
            black_box(eval.rebuild_incremental(&stack, &mut ctx).unwrap())
        });
    });

    g.finish();
}

criterion_group!(
    benches,
    bench_rebuild_all,
    bench_rebuild_incremental_top,
    bench_field_aware_skip,
    bench_group_cache
);
criterion_main!(benches);
