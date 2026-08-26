# Screenshots

Captured from a local debug build of this fork at commit `77a752b`, on Windows
with the DX12 backend. World is a 13x13 km "Alpine Range" archetype at 1024.

## Project browser

![Project browser](screenshots/01-project-browser.png)

Launcher with recent projects, world size per entry, and a search box.

## Editor and layer stack

![Editor and layer stack](screenshots/02-editor-layer-stack.png)

The 3D viewport with the Shape Layers group expanded: Foundation, Mountain
Range, Uplift Corridors, Shape Objects (compiled), Seamless Constraint
Reconstruction and Geomorphic Detail. Each row carries its own visibility
toggle, and the selected row shows blend mode and opacity inline. Drainage
networks are visible across the range.

## Aux channel views

![Flow channel](screenshots/03-flow-channel.png)

The viewport switched to the Flow channel. The engine publishes around 40 named
aux channels (slope, wetness, sediment thickness, flow accumulation, scatter
density, hardness and others); the toolbar exposes Terrain, Height, Slope, Flow
and Mask. A panel for browsing the rest of them is the next thing on the list.

## Export package

![Export package](screenshots/04-export-package.png)

The export panel listing what a package contains: 16-bit `height.png`, 32-bit
float `height_f32.tif` and `height.r32`, `manifest.json` with channels, world
size and height range, selected 16-bit aux maps, splat maps with material IDs,
baked color and normal maps, vegetation instances, a coarse collision mesh and
a streaming tile manifest.

## Unified history

![History panel](screenshots/05-history-panel.png)

The History panel after a series of sculpt strokes, newest first, with Undo and
Redo. Every entry is tagged with the stack it belongs to: edits from different
domains land in one chronological list rather than in separate per-domain undo
stacks. The Sculpt inspector on the right shows the active brush (radius,
strength in metres, target layer).

Note: the entry labels read `draft ? full on refine` where an arrow is intended.
That is a leftover encoding issue in a UI string, not a broken edit.

## Sculpting

![Sculpt stroke](screenshots/06-sculpt-stroke.gif)

Sculpt tool driven live: brush overlay on the terrain, strokes authored into the
Base layer, each one recorded in the history above. Recorded at 8 fps.

The stroke path is the one that got most of the optimisation work in this fork
(an interactive stroke on a 512 squared stack went from 6.27 s to 165 ms at
Draft). The clip does not demonstrate that number, because progressive
refinement does not converge on this build (see below), so the settled result
never arrives on screen. Treat it as evidence the tool works, not as a benchmark.

## Not shown here, and why

- **Scatter Objects** renders instanced boxes rather than real meshes. Placement,
  classes, determinism, export and viewport are done; real prop meshes need an
  asset pipeline. There is also no per-instance authoring yet.
- **Export formats are still unstable** and the package is not production ready.
- Two issues reproduce on this fork and on its upstream merge base, so they are
  not introduced by the work here:
  - Archetype relief does not scale with world size. Relief is roughly constant
    in absolute metres, so a 1-2 km world gets a mountain taller than the world
    is wide. The archetypes are authored for roughly 10 km worlds, which is why
    the world above is 13 km.
  - Progressive refinement does not converge. It sits in the 60-70% range and
    the evaluation appears to restart rather than finish.
