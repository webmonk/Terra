# Noise generators

## Value / Perlin / OpenSimplex

- Lattice hashed with a 32-bit mix (`hash2`); seed splits octave streams.
- Perlin uses 4 corner gradients + quintic fade (Perlin 1985 lineage).
- OpenSimplex-style uses skewed triangular lattice contributions (simplified).
- Outputs normalized roughly to \[-1, 1\] before amplitude/remap.

## Worley / cellular (Worley 1996)

- One feature point per cell with jitter from hash.
- Metrics: Euclidean, Manhattan, Chebyshev.
- Features: F1, F2, F2−F1.

## Fractals (Musgrave et al.)

- **fBm:** sum of octaves with lacunarity/persistence, amplitude-normalized.
- **Ridged MF:** `n → (1-|n|)^2` with weight feedback.
- **Domain warp:** offset domain by low-frequency Perlin before fBm.

## Determinism

Same `(seed, params, world x/z)` ⇒ identical `f32` on CPU. GPU ports must match within documented tolerance.
