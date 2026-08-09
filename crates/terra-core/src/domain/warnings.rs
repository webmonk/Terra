//! Soft diagnostics notes for the Shape / Biome domain model.

/// Documented behavioural differences after the Shape / Biome cutover.
pub fn behavioural_differences() -> &'static [(&'static str, &'static str)] {
    &[
        (
            "shape_outside_biome",
            "Foundation generators live in Shape Layers; biome Filters no longer own world-scale Flat/Island by default.",
        ),
        (
            "mountains_dual_role",
            "Mountains/Mesa/etc. under the Shape folder remain Shape-compat; under a Biome Filters section they are Terrain Filters.",
        ),
        (
            "climate_biomes_kind",
            "LayerKind::Biomes (climate bands) is CompatibilityLegacy — distinct from WC Biome groups / BiomeDefinition.",
        ),
        (
            "claim_residual",
            "Base Terrain residual Claim (1 − claimed) replaces implicit full-coverage Default Biome when higher Claim biomes exist.",
        ),
    ]
}
