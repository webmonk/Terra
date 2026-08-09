<p align="center">
  <img src="assets/logo.png" alt="Terra" width="420" />
</p>

<p align="center">
  <img src="assets/preview.png" alt="Terra editor preview" width="800" />
</p>

# Terra

> [!CAUTION]
> ## Not ready for production
>
> **Terra is an early, unfinished work-in-progress.** Do not use it for shipping games, film pipelines, or any production workflow. Builds may crash, corrupt projects, or change behavior without warning. APIs, file formats, and UI are unstable.
>
> Large parts of the editor are incomplete or only sketched in:
>
> - **Real-time terrain visualization** — live feedback when you change the landscape still needs major refinement; expect laggy, approximate, or stale previews
> - **Export pipeline** — export is not supported for production use yet (stubs / incomplete paths only)
> - **Objects** — object placement and scattering exist largely as placeholders
> - **Materials** — material / surface authoring is placeholder-level and not a full shading pipeline
>
> Treat everything you see as experimental scaffolding. Star the repo, experiment locally, and contribute if you like — but do not depend on Terra for real work yet.

**Terra** aims to be a **free and open-source**, layer-based terrain and landscape creator — heavily inspired by [World Creator](https://www.world-creator.com/), with a familiar stack of layers, masks, and tools rather than a node-graph UI.

The goal is a real-time authoring environment where artists compose landforms, biomes, masks, and simulations on a progressive heightfield, then export results for games, film, and other pipelines.

## Vision

- **Layer-and-mask workflow** — paint, stack, and blend terrain like a digital landscape canvas
- **Free & open source** — MIT OR Apache-2.0; anyone can use, study, and contribute
- **Artist-first** — shape tools, regions, biomes, and world rules without wiring a graph
- **Real-time preview** — progressive / multi-resolution evaluation with optional GPU compute

## Current status

Early development. Core pieces exist (heightfields, layers, masks, CPU evaluation, wgpu viewport), but many features are incomplete or experimental — see the disclaimer above. Longer-term direction lives in the [roadmap](docs/roadmap.md).

## Features (in progress)

- Layer stack with masks, blend modes, and distribution nodes
- Shape tools, region influence, biome placement, and world rules
- Progressive / multi-resolution preview with refinement
- GPU compute path for erosion and related sims (`terra-gpu`)
- Project save/load and heightmap export (`terra-io`) — export is still unfinished

## Stack

| Crate | Role |
|-------|------|
| `terra-core` | Heightfields, layers, masks, regions, biomes, CPU evaluation |
| `terra-gpu` | WGSL compute (erosion, tiled sims) |
| `terra-render` | wgpu 3D viewport / clipmaps |
| `terra-gui` | Custom immediate-mode wgpu UI toolkit (not egui) |
| `terra-ui` | Editor chrome, panels, tools; emits `PanelAction` |
| `terra-io` | Import/export, GeoTIFF bridge |
| `terra-app` | Application binary (winit event loop + wiring) |

`terra-core` must stay free of `wgpu` and UI crates. `terra-gui` must stay free of domain types.

## Build & run

Requires a recent Rust toolchain (edition 2021) and a working wgpu/GPU driver.

```bash
cargo run -p terra-app
cargo test --workspace
```

Real-time viewport path: GPU height/normal textures + static grid displacement (no mesh rebuild). See [docs/perf_migration.md](docs/perf_migration.md). Enable **View → Profiler** for per-frame timings.

## Documentation

- [Architecture](docs/architecture.md) — crate boundaries and evaluation model
- [Roadmap](docs/roadmap.md) — longer-term direction
- Algorithm notes under [`docs/algorithms/`](docs/algorithms/)
- Feature design notes under [`docs/architecture/`](docs/architecture/)

Historical sprint notes live under [`docs/archive/`](docs/archive/) and are not the current design source of truth.

## Contributing

Contributions are welcome as the project takes shape. See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

World Creator is a trademark of its respective owners. Terra is an independent open-source project and is not affiliated with or endorsed by World Creator.
