# Terra benchmarks

Measurable, repeatable numbers for the terrain evaluation paths the
performance work depends on, so a change can be judged instead of guessed.

Everything here is **CPU only** — no benchmark touches wgpu or the GPU
evaluator, so the suite runs anywhere `cargo` does.

## Where the benchmarks live

The real suite is a set of Criterion targets on `terra-core`:

| File | Target |
| --- | --- |
| `crates/terra-core/benches/common/mod.rs` | shared deterministic terrain fixtures (not a bench target) |
| `crates/terra-core/benches/eval_rebuild.rs` | `--bench eval_rebuild` |
| `crates/terra-core/benches/fingerprint.rs` | `--bench fingerprint` |
| `crates/terra-core/benches/mask_and_composite.rs` | `--bench mask_and_composite` |

`benches/noise_bench.rs` in this directory is the older standalone fbm probe.
It predates the Criterion suite, is not wired into any `Cargo.toml`, and is
kept only as a minimal `fn main()` reference — it is *not* run by
`cargo bench`.

## Running

```sh
# whole suite (~2 minutes on a modern desktop CPU)
cargo bench -p terra-core

# compile-only check, e.g. in CI
cargo bench -p terra-core --no-run

# one target
cargo bench -p terra-core --bench fingerprint

# one benchmark by name filter
cargo bench -p terra-core --bench eval_rebuild -- group_cache
```

Criterion stores results under `target/criterion/`, so a second run reports
`change: [-x% +y%]` against the previous one. That regression delta is the
point of the suite — run the baseline before your change, then again after.

## What each benchmark covers

### `fingerprint` — sparse fingerprint vs full content hash

**The headline number.** Isolated group caches are validated by
`height_fingerprint`, a *sparse* hash (5 corners + a fixed 8×8 sample grid,
O(1) in field size). Being sparse it can miss a localized edit — a sculpt
stroke narrower than the sample stride hashes identically and the group
replays a stale composite. The alternative is validating with
`height_content_hash`, which reads every interior sample and cannot miss an
edit, but is O(n).

`cache_validation_hash/{sparse_fingerprint,full_content_hash}/{256,512,1024}`
prices both so the trade can be decided on numbers. Read it against
`group_cache_256/warm_cache_hit` in the `eval_rebuild` bench: full hashing is
only affordable if it stays small next to the cached group evaluation it
protects.

Both hash functions are **copies** of the private implementations in
`crates/terra-core/src/eval/mod.rs`. If either changes there, update
`fingerprint.rs` to match or the comparison quietly stops describing the real
code.

### `eval_rebuild` — the evaluation paths

Fixture stacks are built by `common/mod.rs` with fixed seeds and fixed
iteration counts, so runs are comparable across commits.

- **`rebuild_all/{256,512}`** — cold `StackEvaluator::rebuild_all` on a
  representative stack (`flat base + fbm + blur + thermal erosion`). The
  "open a document" / "change something structural" cost.
- **`rebuild_incremental_top_layer/{256,512}`** — same stack, top layer marked
  dirty, `rebuild_incremental`. The interactive "tweak the top layer" path;
  everything below stays cached. Read as a ratio against `rebuild_all` at the
  same resolution.
- **`field_aware_invalidation_256/{dirty_vegetation,dirty_base_noise}`** — a
  `[fbm, vegetation, blur, thermal]` stack. Vegetation writes a vegetation
  field and never modifies height, so dirtying it should leave the layers
  above it clean; dirtying the bottom fbm layer changes height and forces
  everything above to recompute. The ratio between the two is the field-aware
  invalidation win.

  What it measures *today*: the blur is skipped, the thermal layer is **not**.
  `LayerKind::Vegetation` statically declares that it produces
  `FieldId::Hardness` — deliberately, since contracts are static and root
  cohesion may be enabled — and thermal erosion reads hardness, so thermal is
  invalidated even with `root_cohesion` at its default `0`. The current win is
  therefore the filter only, and this ratio is where any future narrowing of
  that contract would show up.
- **`group_cache_256/{warm_cache_hit,cold_full_group}`** — a base layer plus
  one isolated group of three layers. Warm: nothing dirty, so
  `try_reuse_group_cache` validates the stored fingerprint and replays the
  cached composite. Cold: everything dirty, children actually evaluate.

### `mask_and_composite` — the primitives underneath

- **`bake_mask_assets/5_procedural_assets_512`** — five procedural masks (two
  height ranges, two slope ranges, one curvature range) at 512². Masks are
  re-baked against the live terrain at point of use, once per consuming layer
  per pass, so this sits directly on the interactive path.
- **`par_map_indexed/{512,1024}`** — the rayon-backed per-sample composite
  primitive nearly every processor and blend routes through. A regression here
  (lost parallelism, a changed tile traversal, an accidental clone) shows up
  everywhere, so it is measured on its own with a trivial kernel that keeps
  the traversal, not the math, in view.

## Reference numbers

First recorded baseline, so a later run has something to be surprised by.
Windows 11 desktop, `bench` profile, **with other builds running on the same
machine** — treat these as shape, not as a calibrated baseline. Re-record on a
quiet machine before using them as a gate.

| Benchmark | Median |
| --- | --- |
| `sparse_fingerprint/256` | 361 ns |
| `sparse_fingerprint/512` | 342 ns |
| `sparse_fingerprint/1024` | 351 ns |
| `full_content_hash/256` | 98.3 µs |
| `full_content_hash/512` | 383 µs |
| `full_content_hash/1024` | 1.55 ms |
| `rebuild_all/256` | 83–102 ms |
| `rebuild_all/512` | 352–414 ms |
| `rebuild_incremental_top_layer/256` | 66–77 ms |
| `rebuild_incremental_top_layer/512` | 283–429 ms |
| `field_aware_invalidation_256/dirty_vegetation` | 94–98 ms |
| `field_aware_invalidation_256/dirty_base_noise` | 103–113 ms |
| `group_cache_256/warm_cache_hit` | 3.9 ms |
| `group_cache_256/cold_full_group` | 93–96 ms |
| `bake_mask_assets/5_procedural_assets_512` | 44.8 ms |
| `par_map_indexed/512` | 169 µs |
| `par_map_indexed/1024` | 303 µs |

The sparse fingerprint is flat in resolution (it always reads ~70 samples),
while the full content hash scales linearly at roughly 670 Melem/s. Full
hashing is **~270×** the sparse cost at 256², **~1100×** at 512², and
**~4400×** at 1024².

## Keeping the suite fast

Target is a full `cargo bench` under about three minutes, which means:

- **Small sample counts.** Simulation-bearing benchmarks use
  `sample_size(10)` with a short measurement window. That is enough to
  separate a real regression from noise at this cost scale; Criterion's
  100-sample default would take tens of minutes.
- **Reduced simulation work.** Thermal erosion runs 12 iterations here
  (`common::THERMAL_ITERATIONS`) rather than the artist default of 40, and
  vegetation scatters sparsely rather than at the default density. These
  benchmarks measure the *shape* of the evaluation cost and regressions in
  it, not production-quality output.
- **Modest resolutions.** 256²/512² for evaluation, 1024² only for the O(n)
  primitives where the per-sample rate is the whole point.

If you add a benchmark, keep those rules: prefer resolution and iteration
counts where the shape of the number matters more than its precision.
