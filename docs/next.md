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

**Get the open branches reviewed and merged.** The Channels panel is built
(`feat/channels-panel`), and four bug-fix branches are green and waiting:
`fix/archetype-world-scaling`, `fix/refine-never-converges`,
`fix/prop-surface-height`, `fix/non-square-levelled-sim`. Two items are parked
on a maintainer decision rather than effort: extent-aware scaling for the
process chain, and capping the interactive Full rung so a preview can converge
on a large world.

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
- ~~**A Channels panel.**~~ Built on `feat/channels-panel`. `View > Channels`
  lists what the last evaluation actually published (32 entries on a 13 km
  Alpine world) with range, mean, coverage and resolution each, plus a one-line
  diagnosis separating the faults that all present as "my mask is empty":
  absent, all zero, constant, or non-finite. The classification lives in
  `terra-core` so it is testable without a window.

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

## 2b. Progressive refinement never converges (two bugs, one symptom)

The preview sits in the 60-70% band, the number drifts backwards, and the
process uses about 2 CPU-seconds per 10 wall-seconds on 16 cores. Both causes
reproduce on `8f53eb3`, so neither is from this fork's work.

**Fixed** on `fix/refine-never-converges`: `mask_overlay_dirty` starts `true`
and is only cleared when a mask overlay is actually shown, but it feeds
`meaningful_interaction`, so on any project with no mask selected it reset the
refinement controller's settle timer every frame and the state never left
`Interactive`.

**Open, and a design call rather than a bug.** With refinement escalating
again, Full still never lands. The ladder is Draft 512, Medium 1024, Full
`preview_resolution` - and `preview_resolution` is roughly world metres capped
at 8192, so any world wider than about 2 km asks Full for 8192 squared, or 67
million samples. Measured cost (release, Alpine at 12.6 km, from
`preview_cost_scaling.rs`):

| res | samples | Full eval |
|---:|---:|---:|
| 256 | 65k | 2.4 s |
| 512 | 262k | 10.2 s |
| 1024 | 1.05M | 45.8 s |
| 2048 | 4.19M | 201 s |

That is about 4.4x per 4x samples, so near-linear. The same curve puts 8192
squared at roughly an hour, in release; the editor runs the dev profile and is
slower again. Full is not slow, it is unreachable, so the preview parks on the
rung below it forever.

Capping the interactive Full rung and leaving 8192 to Export is the obvious
lever, but it changes what Full means for every project, so it needs a
maintainer decision.

Worth fixing regardless: the progress readout is derived from the renderer's
texture size, not from evaluation progress, which is why it reads a confident
63% while nothing is converging and why it can move backwards.

## 3. Known debt (small, specific)

- ~~**Non-square grids.**~~ Tested, and it turned out to be a crash rather than
  a gap: `upsample_to_metrics` resampled via `downsample_height(src,
  target.width)`, which always builds a square result, so on a non-square world
  the displacement came back the wrong length, `zip` truncated it silently, and
  `from_dense` panicked on its size assertion. Fixed on
  `fix/non-square-levelled-sim` by extracting a single `resample_dense` kernel
  and routing the height and mask paths through it.
- ~~**Props sit up to one texel off the surface.**~~ Fixed on
  `fix/prop-surface-height`: `Heightfield::sample_bilinear` samples the surface
  at the placement's own continuous X/Z. Measured at up to 8 m of error on a
  20 m texel with a 0.8 gradient before the fix, under a centimetre after.
- ~~**Inspector seed clamp.**~~ Fixed on `fix/seed-clamp`. It was worse than
  "the UI shows a clamped number": dragging the slider then committed that
  number, replacing a 64-bit seed the user could not see. Shrinking the seed
  domain was the wrong fix - the smart cache bumped to VERSION 3 for "64-bit
  seed canonicalization" - so the row now prints the real seed underneath
  whenever it falls outside the slider's span.
- ~~**`ScatterObjects::optional_fields`**~~ Fixed on
  `fix/scatter-field-contract`. It was reachable after all: coverage and
  exclusion bake through `composite_distribution`, which wires slope, curvature
  and flow accumulation from aux, so a Flow or Curvature node read a channel the
  layer never declared. The declaration now derives the second half from the
  distributions actually carried. The mapping lives in `layer/operation.rs`
  rather than on `DistNodeKind`, because naming a `FieldId` from `mask` closes a
  module cycle - `module_graph_cycles_match_allowlist` caught the first attempt.
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
- A second stale-build trap, different cause: `cargo test --workspace` can fail
  to **link** with `unresolved external symbol anon.<hash>.llvm.<n>` across
  `terra-core`, while every targeted `--lib` / `--test` run against the same
  source passes. That is an incremental-artifact mismatch on MSVC, not a code
  error. `cargo clean -p terra-core -p terra-app` and rebuild. Do not go looking
  for a real linker problem first - check whether targeted runs pass, because if
  they do it is this.
- `terra-render`'s GPU tests self-skip without an adapter, so the main CI legs
  report them green without running them. The lavapipe leg (`gpu-parity.yml`)
  runs them for real. Locally, `TERRA_TEST_GPU_FORCE_FALLBACK=1` runs them
  against a software adapter — a good proxy for what lavapipe will do.
- Golden stacks must be updated in the **same commit** as any intentional
  semantic change, never separately.
- Verification gate before any commit:
  `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo test --workspace --locked --no-fail-fast -- --skip gpu_required_`.
