# Tiled processing

- Dirty tile + 8-neighbors marked for recompute (`TileScheduler::mark_tile_and_neighbors`).
- `expand(radius)` grows the dirty set for multi-iteration stencils.
- `Heightfield::refresh_halos_for` performs incremental ghost exchange without a full dense flatten.
- `sync_dirty` refreshes halos then measures seams on affected edges.
- Seam metric: `|left.interior[edge] − right.halo[-1]|` must be ~0 after sync.
- Blur/erosion tile-local reads use halo samples only (`map_tiles`).
- `SampleRect` / `bounds_from_tiles` feed region GPU upload and normal recompute.
