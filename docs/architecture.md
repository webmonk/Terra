# Terra Architecture

## Separation of concerns

| Crate | Responsibility |
|-------|----------------|
| `terra-core` | Heightfields, layer stack, masks, CPU evaluation, commands |
| `terra-gpu` | WGSL compute (thermal/hydraulic), buffer pools, memory budget |
| `terra-render` | Viewport mesh/clipmaps, orbit camera |
| `terra-gui` | Custom wgpu UI (HUD, viewport layout); no egui |
| `terra-ui` | egui panels (legacy) — mutates document via `PanelAction` until migrated |
| `terra-io` | Project JSON, height/mask export, GeoTIFF bridge, background builds |
| `terra-app` | Window, event loop, wiring |

`terra-core` never depends on `wgpu` or `egui`. Composite order: terrain → `terra-gui` → egui.

## User model vs internal model

**User-facing:** ordered layer stack (World Creator style). Groups nest; no node editor.

**Internal:** each layer is a `LayerProcessor` producing a height contribution blended onto the accumulator. Masks and aux maps (erosion, flow, wetness) are cacheable artifacts. Dirty propagation invalidates from the earliest edited layer upward.

## Evaluation

```
H_0 = 0
for layer L in bottom→top:
  if disabled: continue
  G = processor(L, H_{i-1})
  M = composite masks(L)
  H_i = mix(H_{i-1}, blend(H_{i-1}, G), opacity * M)
```

Phase 1 used full rebuilds. Incremental rebuild uses `LayerCache` + `mark_dirty_from`. Progressive preview walks `Draft → Medium → Full` via `EvalScheduler`.

## Tiles & ghosts

Default tile 256² with halo 2. Halos are refreshed from neighbors before stencil reads. Phase 9 tile scheduler processes dirty tiles + neighbors to avoid seams.

## Undo

`EditorCommand` records stack/parameter deltas only — never full height textures.
