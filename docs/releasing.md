# Releasing Terra

## Producing a release build

```
cargo build --release -p terra-app
```

The binary is named `terra` (the `[[bin]]` name in `crates/terra-app/Cargo.toml`)
and lands at:

- Windows: `target/release/terra.exe`
- Linux/macOS: `target/release/terra`

The release profile (root `Cargo.toml`) uses `lto = "thin"` and
`codegen-units = 1`, so a clean release build takes several minutes. The
executable is self-contained apart from the platform's graphics stack
(DX12 on Windows by default, Vulkan/Metal elsewhere; overridable via the
`WGPU_BACKEND` env var). Ship the exe together with nothing else — fonts,
shaders, and the tool thumbnails from `assets/` are embedded via
`include_bytes!`, which is also why the executable is large (~150 MB, most of
it `assets/tools` thumbnails).

## Export packages

Terra's in-app export (see `crates/terra-io/src/export.rs`) writes a package
directory described by a top-level `manifest.json` (`PackageManifest`:
app version, grid width/height, world size, height min/max used to
de-normalize `height.png`, a height hash, and the list of channels). Depending
on the export options, the package contains:

- `height.png` — 16-bit normalized heightmap (+ `height_meta.json` with
  min/max/world size for de-normalization)
- `height_f32.tif` — 32-bit float TIFF with raw metre heights
- `height.r32` — raw little-endian f32 heights (+ `height.r32.meta.json`)
- `splat.png` + `splat_ids.json` — material splatmap and ID→channel mapping
- `color.png` — baked albedo map
- `normal.png` — height-derived tangent-space normal map
- `aux_*.png` — auxiliary mask channels (wetness, vegetation, ...)
- `vegetation_instances.json` — Poisson-sampled vegetation placements
- `terrain_collision.obj` — coarse collision/preview mesh
- `tile_manifest.json` + `height_tiles/` — per-tile hashes and tiles for
  incremental engine import

## What CI covers

- `.github/workflows/ci.yml` — on push to `master` and on PRs:
  `cargo build --workspace --locked` and
  `cargo test --workspace --locked --no-fail-fast -- --skip gpu_required_`
  on ubuntu-latest and windows-latest, plus a clippy job. The
  `gpu_required_*` parity tests are excluded because hosted runners have no
  guaranteed GPU adapter; all other GPU tests self-skip via
  `terra_test_gpu::headless()` when no adapter exists.
- `.github/workflows/gpu-parity.yml` — runs the full GPU parity matrix
  (`terra-gpu` `parity_matrix` tests) against Mesa lavapipe, a software
  Vulkan adapter. CPU determinism and simulation contracts are covered by
  the terra-core suites inside the main CI workflow.

- `.github/workflows/release.yml` — pushing a `v*` tag builds optimized
  Windows and Linux binaries and attaches them to a GitHub release with
  generated notes.
