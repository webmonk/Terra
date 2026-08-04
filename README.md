# Terra

Real-time terrain creation with a **layer-and-mask** workflow (World Creator / Gaea style). No node-graph UI.

## Stack

| Crate | Role |
|-------|------|
| `terra-core` | Heightfields, layers, masks, CPU evaluation |
| `terra-gpu` | WGSL compute (erosion, tiled sims) |
| `terra-render` | wgpu 3D viewport / clipmaps |
| `terra-ui` | egui editor panels |
| `terra-io` | Import/export, GeoTIFF bridge |
| `terra-app` | Application binary |

## Build & run

```bash
cargo run -p terra-app
cargo test --workspace
```

Real-time viewport path: GPU height/normal textures + static grid displacement (no mesh rebuild). See [docs/perf_migration.md](docs/perf_migration.md). Enable **View → Profiler** for per-frame timings.

## Roadmap

See [docs/roadmap.md](docs/roadmap.md) and [docs/architecture.md](docs/architecture.md).

1. Heightfield, viewport, layer stack
2. Noise generators & shapes
3. Masks & analysis
4. Caching & progressive preview
5. Thermal erosion
6. Hydraulic erosion
7. Flow, watersheds, rivers
8. Materials, biomes, vegetation
9. Tiled processing & seams
10. Hi-res builds & I/O
11. Clipmaps, perf, memory budgets
12. Undo/redo, presets, production UX
