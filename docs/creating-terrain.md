# Creating terrain

A practical path through Terra’s layer workflow. Workspaces are focus modes — use them in any order. Terra is early WIP: expect incomplete preview, export, materials, and objects.

## 1. Start a project

1. From **Project Home**, choose **New Project** (or open a recent file).
2. Pick a **World Design** template if you want a head start — for example Blank, Tropical Island, Alpine Range, Desert Mesa, River Valley, Badlands, Young/Old Mountains, Dune Field, or Coastal.
3. Set **world size (m)** and **sea level**, then open the editor.

A blank-style default typically includes a **Base** sculpt foundation, a simple shape such as **Hills**, a default biome, and a biome paint layer.

## 2. Shape the land (Sculpt)

1. Switch to the **Sculpt** workspace.
2. Select **Base** or a Shape Layer in the hierarchy.
3. Raise / lower / smooth / flatten with brushes, or **Quick Add** procedural landforms, stamps, polygons, paths, or a heightmap import.
4. In the inspector, tune **blend mode**, **opacity**, and **distribution** so layers stack the way you intend.

Use a second Shape Layer with **Add** (or another blend) when you want a separable landform you can mute or remask later.

## 3. Place biomes (Biomes)

1. Switch to **Biomes**.
2. Add biome containers under **Biomes** (each gets Filters / Materials / Objects / Local Simulations sections).
3. Define coverage with distribution rules and/or paint **Biome Layers** (paint, erase, smooth, raise+paint, …).
4. Keep the active biome in mind — Add and paint targets follow context.

Biomes are *what* packages content; paint and distribution are *where* that package appears.

## 4. Develop inside biomes (Filters)

1. Switch to **Filters**.
2. Add terrain filters or local sims into the active biome’s **Filters** (or **Local Simulations**).
3. Set **Apply Where** so the filter only hits the right part of the biome (for example a slope range or near water).
4. Adjust opacity and blend so the filter reads as a refinement, not a full overwrite — unless Replace is what you want.

## 5. Masks and rules (optional)

1. Use the **Mask** workspace to paint or edit mask assets and distribution stacks.
2. Bind coverage to layers or biomes that should respect those masks.
3. Optionally add **World Rules** for cross-biome conditions (for example snow above an elevation).

## 6. Simulation (optional)

1. Switch to **Simulation**.
2. Add a global or biome-local sim (hydraulic, thermal, stream power, sand, fluid, river network, …).
3. Watch build status in the bottom dock (Ready / Outdated / Running) and cancel or wait as needed.
4. Use a **Simulation Scenario** only if you want an explicit container for sources, domain, and passes.

## 7. Surface and objects (early)

**Surface** and **Objects** workspaces exist for materials and scatter, but authoring here is still largely placeholder-level. Prefer shaping height and biomes first.

## 8. Save and export

| Action | Notes |
|--------|--------|
| **Save / Save As** | Writes the project as JSON; dirty projects show `*` in the title |
| **Open / Recents** | Project Home and File menu |
| **Export** | Panel exists for heightmaps, masks, and related outputs — **not production-ready** yet |

Keep an eye on the bottom dock for preview resolution, quality, and build progress while you iterate.

## Suggested first session

1. New **Blank** or **Alpine Range** world.
2. Sculpt Base, then add one procedural Shape Layer.
3. Add a second biome and paint a soft boundary.
4. Drop one Filter inside a biome with **Apply Where → Slope Range**.
5. Save the project.

For how folders, biomes, masks, and workspaces relate, see [Workflow structure](workflow.md). For chrome and panels, see [Editor overview](editor.md).
