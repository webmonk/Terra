# Fidelity notes (post M1–M5)

## CPU vs GPU

- **CPU** remains the export oracle (`StackEvaluator` / `terra-io` background export).
- **GPU** preview supports generators, blur, terrace, thermal (two-pass redistribute), hydraulic (shallow-water approx), and Constant/Height/Slope masks.
- **Hybrid**: GPU runs the supported prefix; first unsupported/paint-heavy mask layer triggers readback and CPU suffix (`resume_cpu_from`).

## Remaining intentional gaps

- GPU hydraulic omits full neighbor water/sediment gather (atomic-free preview).
- GPU noise is a portable hash/Perlin approximation, not bit-identical to CPU.
- Materials/biomes are ID masks + procedural viewport palette, not a PBR asset library.
- Clipmaps are nested full grids (no skirts/morphing yet).
