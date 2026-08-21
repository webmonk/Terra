//! Mask baking and the parallel composite primitive.
//!
//! ```text
//! cargo bench -p terra-core --bench mask_and_composite
//! ```

mod common;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::HashMap;
use std::hint::black_box;
use std::time::Duration;

use common::{mask_assets, metrics, sample_field};
use terra_core::mask::bake_mask_assets;

/// `bake_mask_assets` over five procedural assets (two height ranges, two
/// slope ranges, one curvature range) at 512^2. Masks are re-baked against the
/// live terrain at point of use, so this runs once per consuming layer per
/// pass - it sits directly on the interactive path.
fn bench_mask_bake(c: &mut Criterion) {
    let res = 512u32;
    let mut g = c.benchmark_group("bake_mask_assets");
    g.sample_size(20);
    g.warm_up_time(Duration::from_millis(300));
    g.measurement_time(Duration::from_secs(3));
    g.throughput(Throughput::Elements((res as u64) * (res as u64)));

    let assets = mask_assets();
    let reference = sample_field(res);
    let target = metrics(res);
    let aux: HashMap<String, terra_core::mask::MaskField> = HashMap::new();

    g.bench_function("5_procedural_assets_512", |b| {
        b.iter(|| {
            black_box(bake_mask_assets(
                black_box(&assets),
                black_box(&reference),
                target,
                &aux,
            ))
        });
    });
    g.finish();
}

/// `Heightfield::par_map_indexed` - the rayon-backed per-sample composite
/// primitive nearly every processor and blend goes through. Regressions here
/// (lost parallelism, a tile-iteration change, an accidental clone) show up
/// across the whole evaluator, so it is measured on its own with a trivial
/// kernel that keeps the traversal, not the math, in view.
fn bench_par_map_indexed(c: &mut Criterion) {
    let mut g = c.benchmark_group("par_map_indexed");
    g.sample_size(30);
    g.warm_up_time(Duration::from_millis(300));
    g.measurement_time(Duration::from_secs(3));

    for res in [512u32, 1024] {
        g.throughput(Throughput::Elements((res as u64) * (res as u64)));
        g.bench_with_input(BenchmarkId::from_parameter(res), &res, |b, &res| {
            let mut field = sample_field(res);
            b.iter(|| {
                field.par_map_indexed(|i, j, v| v * 0.5 + (i ^ j) as f32 * 1e-4);
                black_box(field.get(0, 0))
            });
        });
    }
    g.finish();
}

criterion_group!(benches, bench_mask_bake, bench_par_map_indexed);
criterion_main!(benches);
