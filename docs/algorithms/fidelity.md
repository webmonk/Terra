# Fidelity notes (post M1–M5)

## CPU vs GPU

- **CPU** remains the export oracle (`StackEvaluator` / `terra-io` background export).
- **GPU** preview supports a conservative subset of generators, blur, terrace, thermal (two-pass redistribute), hydraulic (shallow-water approx), and selected masks. Generator compositing supports only the equations implemented exactly by the blend shader: Replace/Normal/Interpolate, Add, Subtract, Multiply, Min, Max, and Overlay. HeightBlend and the smooth blend modes fall back to CPU rather than being substituted with different equations.
- **In-place GPU kernels** (Blur, Terrace, EffectFilter, Thermal, Hydraulic, and RiverCarve) are supported only with their exact default outer composite: full opacity, no layer mask, and Replace/Normal/Interpolate blending. Masked, partial-opacity, or otherwise blended configurations fall back to CPU because these kernels do not yet preserve and composite their entering height.
- **GPU masks** are limited to one Multiply entry referencing an existing, operation-free Constant, Height, or Slope asset; this subset is checked against the CPU oracle with a maximum mask-weight error of `1e-3`. fBm/ridged layers support Value and Perlin noise; OpenSimplex variants fall back to CPU.
- **CPU fallback**: Coastal currently requires CPU evaluation. Materials, Biomes, and Vegetation also require CPU evaluation because their observable auxiliary fields are not published by the GPU path.
- **Hybrid**: GPU preview may continue supported work speculatively above an unsupported layer when no readback is requested. A requested CPU checkpoint instead stops before the first unsupported layer, and its height is exactly the field entering `resume_cpu_from`. Prefixes that publish auxiliary fields or named outputs cannot be represented by height alone and conservatively restart the CPU evaluator from layer zero.

## Remaining intentional gaps

- GPU hydraulic omits full neighbor water/sediment gather (atomic-free preview).
- GPU noise is a portable hash/Perlin approximation, not bit-identical to CPU.
- Multi-entry distributions, non-Multiply combines, mask asset operations, missing assets, and Noise/Curvature mask sources fall back to CPU.
- Unsupported noise families are never substituted with a different algorithm.
- Materials/biomes are ID masks + procedural viewport palette, not a PBR asset library.
- Clipmaps are nested full grids (no skirts/morphing yet).
