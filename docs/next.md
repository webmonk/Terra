# What's next

Forward-looking backlog. [roadmap.md](roadmap.md) is the historical phase table;
this is the live one. Ordered by value, with the evidence behind each call so a
priority can be argued with rather than just inherited.

Last updated after adversarial review round 6.

## Where things stand

An interactive sculpt stroke on a 512² stack went from **6.27 s to 165 ms** at
Draft over the last few waves. The remaining Draft budget is spread thin:

| Layer | Draft cost |
|---|---:|
| Thermal erosion | 72 ms |
| Sculpt strokes | 32 ms |
| Materials | 22 ms |
| Blur | 21 ms |
| Vegetation | 20 ms |
| **Total** | **165 ms** |

There is no single dominant cost left, which changes what is worth doing next —
see [Performance](#2-performance-read-the-caveat-first).

Adversarial review has found **25 real bugs across 6 rounds**, every one of them
in code with a green test suite.

## Recommended next step

**Build a Channels panel.** With review round 6 done (below), this is the
highest value-per-effort item left. The engine publishes ~40 named aux channels
and there is no UI to look at any of them, so "why is my mask empty" is a
debugger question rather than something the user can answer.

Adversarial review round 6 covered the three slices that were unreviewed --
the quality-cache rekeying, the flat-buffer resampler and the prop-rendering
slice -- and found 3 more real bugs, all fixed:

- The prop upload fingerprint hashed only X/Z, scale and class, so a
  height-only edit (an offset layer, a flatten) re-placed every prop at a new
  surface height without changing the fingerprint, and the viewport kept
  drawing them at the old one. `y`, `yaw_rad` and `normal` all reach the GPU
  and none were hashed.
- `DiskSmartCache::spill` rewrote `strata.json` when the output had strata but
  never removed it when it did not, while `load` reads the sidecar whenever the
  file exists. Since the path is keyed by `LayerId` alone and the store is a
  stable temp dir shared across documents, a Materials stack from an earlier
  bake could reattach to a layer that has none. `object_instances` already got
  this right; strata did not.
- `invalidate` removed the meta and raster blobs but left both non-raster
  sidecars behind, which is the same bug through a second door.

Two hypotheses were chased and **disproved**, each now pinned by a test:

- The prop box is wound clockwise-outward in local space, which alone would be
  culled as back-facing. It survives because the shader's `basis()` is
  left-handed (det = -1) and the mirror flips the winding back. Two wrongs that
  must stay wrong together -- "fixing" either half alone turns every prop
  inside out, and `object_overlay.rs` would still pass, because it only asserts
  that the frame changed. `box_faces_are_front_facing_after_the_instance_basis`
  now asserts the composition across several up vectors and yaws.
- `resample_mask` is bit-equivalent to the `Heightfield` round-trip it replaced
  in both directions (box-average down, bilinear up) and is deterministic under
  the row-parallel split. Pinned, since every levelled sim shares it.

Method that works, and should be used verbatim: give each agent a scoped target
and require it to either **prove a finding with a test that fails against the
current tree**, or **mark it disproved and say precisely why the code is sound**.
No "possible issues". Disproved hypotheses get a test that pins the invariant.

## 1. Finish what is half-built

- **Real prop meshes.** `ObjectOverlay` draws instanced boxes. Everything else
  about Scatter Objects is done — placement, classes, determinism, export,
  viewport. Boxes are the honest placeholder; real meshes need an asset
  pipeline (import, storage in the document, GPU residency), not a renderer
  change. Note before that lands: the shader's `basis()` is left-handed, so the
  viewport yaws props the opposite way from the exported `yaw_rad`. A box is
  four-fold symmetric so nothing shows it today; an asymmetric mesh will. Fix
  the handedness and the box winding together, or neither (see round 6 above).
- **Per-instance object authoring.** `README.md` names this as the remaining
  Objects gap: placing, moving and deleting a single prop by hand. Needs a
  selection model for instances and an undo command; the placement list itself
  is already carried through eval, cache, disk and export.
- **A Channels panel.** The engine publishes ~40 named aux channels (slope,
  wetness, sediment thickness, flow accumulation, scatter density, hardness…)
  and there is no UI to look at any of them. This plays directly to the
  architecture's strength and would make a whole class of "why is my mask
  empty" question answerable by the user instead of by a debugger. This is the
  current recommendation (see above).

## 2. Performance (read the caveat first)

Diminishing returns. With no cost above 72 ms, the next meaningful win requires
attacking several layers at once rather than finding another hotspot.

- **CPU dirty-rects — lower value than it looks.** The obvious idea is to limit
  processors to a dilated dirty rect on a local edit. For thermal the
  propagation radius is bounded by `iterations × (1 + transport_hops)` texels,
  which at the defaults is ~80. Draft runs its levels at 128 and 192, so an
  80-texel halo swamps the grid and saves nothing. It would pay at Full/Export
  (~7× on the thermal pass at 512²), so it is an *export* optimisation, not an
  interactivity one. Do not chase it expecting a responsive-editing win.
- **GPU aux textures / handoff past aux-writing layers.** Currently the GPU path
  hands back to the CPU whenever a layer writes aux. This is the real structural
  lever left for large worlds.
- **Narrow the quality-cache invalidation.** `LayerCache::ensure_quality` clears
  every entry when the rung changes. Deliberately conservative — the alternative
  is a per-kind "is this processor quality-sensitive" contract, which is exactly
  the declaration shape that has gone stale repeatedly here (13 under-declared
  field contracts found in one round). Only narrow it if profiling shows the
  blanket clear actually costs something.

### Measured dead ends — do not retry

Both of these looked obviously correct and were rejected by measurement:

- Parallelising the mass-wasting solver's elementwise fold-in loops made it
  **1.8× slower**. They run at memcpy speed; 120+ rayon dispatches per solve
  cost more than the work.
- An interior/border split to drop per-neighbour bounds checks in the stencil
  was slower at the resolutions Draft actually runs.

The pattern: in this codebase rayon pays on stencil and bilinear passes, not on
bandwidth-bound elementwise ones. Always measure, and re-measure the baseline
after reverting.

## 3. Known debt (small, specific)

- **Non-square grids.** `resample_mask` uses the target metrics directly, where
  the old `Heightfield` round-trip forced a square target. The new behaviour is
  more correct but is untested for non-square grids.
- **Props sit up to one texel off the surface.** `ObjectInstance.y` samples the
  nearest texel while `x`/`z` are continuous, so on steep ground an exported
  prop can float or sink slightly. Bilinear sampling would fix it.
- **Inspector seed clamp.** The Scatter Objects seed slider clamps display to
  99999 while `randomize_layer_seed` sets a full 64-bit value, so the UI shows a
  clamped number that does not match the seed in use.
- **`ScatterObjects::optional_fields`** declares `[Biomes, Materials, Wetness,
  Slope]` but its distributions can read Hardness/Snow/Erosion. Not currently
  exploitable — every layer producing those also produces something in the
  declared set, so `layer_contract_touches` dirties it anyway — but it is one
  refactor away from being a stale-cache bug.
- **Benchmark baselines** were recorded on a busy machine and should not be used
  as a gate until re-recorded on a quiet one.
- **AO tuning constants** (`AO_TANGENT_RELIEF`, radius in `normals.wgsl`) were
  probed for plausibility, never art-directed.

## 4. Photoshop-parity gaps worth considering

The stack already has blend modes, opacity, layer and clipping masks, groups,
selections with quick-mask, and unified cross-domain history. The gaps that
would actually earn their keep for terrain:

- **Blend-if / range gating per layer** — "contribute only where the incoming
  height is 200–600 m" or "only where slope is 20–40°" without authoring a mask
  asset for it. Masks can express this today, but as a per-layer range it is far
  faster to reach for.
- **Layer effects** — a reusable post-op chain attached to a layer rather than
  inserted as sibling layers.

Adjustment layers and smart objects have rough equivalents already (layer kinds
and groups respectively), so they are not gaps so much as different naming.

## Working notes that save time

- Commits are authored as Isaac Oyelowo with **no assistant attribution**. Use
  `git commit -F <file>` — PowerShell here-strings mangle messages containing
  quotes or parentheses.
- Restoring a file with `mv backup.rs source.rs` preserves the backup's mtime, so
  cargo skips the rebuild and the tests run a **stale binary**. Use `touch`, or
  restore by rewriting the file.
- `terra-render`'s GPU tests self-skip without an adapter, so the main CI legs
  report them green without running them. The lavapipe leg (`gpu-parity.yml`)
  runs them for real. Locally, `TERRA_TEST_GPU_FORCE_FALLBACK=1` runs them
  against a software adapter — a good proxy for what lavapipe will do.
- Golden stacks must be updated in the **same commit** as any intentional
  semantic change, never separately.
- Verification gate before any commit:
  `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace --locked --no-fail-fast -- --skip gpu_required_`.
