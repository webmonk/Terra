# Flow routing

## D8

Steepest strictly downhill of 8 neighbors; accumulation is processed high→low.
Cells without a lower neighbor are explicitly marked no-flow and retain their
accumulation rather than routing arbitrarily.

## Depression fill

Before river routing, Priority-Flood raises enclosed pits to their lowest spill
elevation while preserving boundary outlets. This avoids artificial sink basins
and produces continuous drainage where a path to the terrain boundary exists.

## D∞ (Tarboton)

Partition flow between the two steepest downhill neighbors proportional to slope.

## Rivers

Threshold accumulation → carve width and depth scaled by upstream accumulation.
Banks use a Gaussian-like falloff; increasing bank smoothing broadens the soft
shoulders around the channel. Stream order ≈ `1 + log2(acc/threshold)`.

## Pitfalls

- Flats remain no-flow cells after routing; depression fill removes only
  enclosed pits before river carving.
- Tile seams: recompute flow on full field or with sufficient halo after height edits.
