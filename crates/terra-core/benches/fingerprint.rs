//! **Sparse height fingerprint vs full content hash.**
//!
//! ```text
//! cargo bench -p terra-core --bench fingerprint
//! ```
//!
//! This is the number the group-cache work needs. Isolated groups are cached
//! keyed by a *sparse* fingerprint of their input height (`height_fingerprint`
//! in `eval/mod.rs`): five corners plus a fixed 8x8 sample grid, so O(1) in
//! the field size. It is cheap but it can miss a localized edit - a sculpt
//! stroke narrower than the sample stride hashes identically, and the group
//! then replays a stale composite.
//!
//! The alternative is validating the group cache with the full content hash
//! (`height_content_hash`, already used for scrub checkpoints), which reads
//! every interior sample and cannot miss an edit, but is O(n) in the field.
//!
//! The question this bench answers: at 256^2 / 512^2 / 1024^2, how much does
//! full hashing actually cost, and is that affordable against the group
//! evaluation it protects (see `group_cache_256` in the `eval_rebuild` bench)?
//!
//! Both functions below are copies of the private implementations in
//! `crates/terra-core/src/eval/mod.rs`. They are not exported, so they are
//! mirrored here rather than called. **If either changes in src, update this
//! file to match** - otherwise the comparison silently stops describing the
//! real code.

mod common;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;
use std::time::Duration;

use common::sample_field;
use terra_core::heightfield::Heightfield;

/// Mirror of `eval::height_fingerprint`: corners + 8x8 sample grid, O(1).
fn height_fingerprint(h: &Heightfield) -> u64 {
    let m = h.metrics;
    let mut state = (m.width as u64)
        .wrapping_mul(0x0100_0000_01b3)
        .wrapping_add(m.height as u64);
    if m.width == 0 || m.height == 0 {
        return state;
    }
    let corners = [
        (0, 0),
        (m.width - 1, 0),
        (0, m.height - 1),
        (m.width - 1, m.height - 1),
        (m.width / 2, m.height / 2),
    ];
    for (i, j) in corners {
        state ^= (h.get(i, j).to_bits() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        state = state.rotate_left(13);
    }
    let step_i = (m.width / 8).max(1);
    let step_j = (m.height / 8).max(1);
    let mut j = 0;
    while j < m.height {
        let mut i = 0;
        while i < m.width {
            state = state
                .wrapping_mul(0x0100_0000_01b3)
                .wrapping_add(h.get(i, j).to_bits() as u64);
            i += step_i;
        }
        j += step_j;
    }
    state
}

/// Mirror of `eval::height_content_hash`: FNV-1a over every interior sample, O(n).
fn height_content_hash(hf: &Heightfield) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for tile in hf.tiles() {
        for &v in tile.interior() {
            hash ^= v.to_bits() as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

fn bench_hashes(c: &mut Criterion) {
    let mut g = c.benchmark_group("cache_validation_hash");
    g.sample_size(30);
    g.warm_up_time(Duration::from_millis(300));
    g.measurement_time(Duration::from_secs(2));

    for res in [256u32, 512, 1024] {
        let field = sample_field(res);
        let samples = (res as u64) * (res as u64);
        g.throughput(Throughput::Elements(samples));

        g.bench_with_input(
            BenchmarkId::new("sparse_fingerprint", res),
            &field,
            |b, field| b.iter(|| black_box(height_fingerprint(black_box(field)))),
        );
        g.bench_with_input(
            BenchmarkId::new("full_content_hash", res),
            &field,
            |b, field| b.iter(|| black_box(height_content_hash(black_box(field)))),
        );
    }
    g.finish();
}

criterion_group!(benches, bench_hashes);
criterion_main!(benches);
