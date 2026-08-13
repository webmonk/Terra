# Fidelity notes (post M1–M5)

## CPU vs GPU

- **CPU** remains the export oracle (`StackEvaluator` / `terra-io` background export).
- **GPU** preview supports a conservative subset of generators, blur, terrace, thermal (two-pass redistribute), hydraulic (shallow-water approx), and selected masks. fBm/ridged layers support Value and Perlin noise; OpenSimplex variants fall back to CPU.
- **CPU fallback**: Coastal currently requires CPU evaluation. Materials, Biomes, and Vegetation also require CPU evaluation because their observable auxiliary fields are not published by the GPU path.
- **Hybrid**: GPU may present supported work speculatively, but the first unsupported layer is recorded in `resume_cpu_from` and the CPU evaluator remains authoritative for the completed result.

## Remaining intentional gaps

- GPU hydraulic omits full neighbor water/sediment gather (atomic-free preview).
- GPU noise is a portable hash/Perlin approximation, not bit-identical to CPU.
- Unsupported noise families are never substituted with a different algorithm.
- Materials/biomes are ID masks + procedural viewport palette, not a PBR asset library.
- Clipmaps are nested full grids (no skirts/morphing yet).
