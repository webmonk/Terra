//! Map legacy [`LayerKind`] values onto [`DomainRole`].

use super::role::DomainRole;
use crate::layer::{BiomeSection, LayerKind, StackCategory};

/// Classify a leaf layer kind into the Region/Biome hierarchy role.
///
/// Placement inside Shape vs Biome Filters can still override this for dual-role
/// kinds (see [`classify_in_context`]).
pub fn classify_layer_kind(kind: &LayerKind) -> DomainRole {
    match kind {
        // Foundation / shape
        LayerKind::SculptBase(_)
        | LayerKind::SculptStrokes(_)
        | LayerKind::TerrainConstraints(_)
        | LayerKind::Flat(_)
        | LayerKind::Ramp(_)
        | LayerKind::NoiseValue(_)
        | LayerKind::NoisePerlin(_)
        | LayerKind::NoiseOpenSimplex(_)
        | LayerKind::NoiseWorley(_)
        | LayerKind::Fbm(_)
        | LayerKind::Ridged(_)
        | LayerKind::DomainWarp(_)
        | LayerKind::Island(_)
        | LayerKind::VoronoiRegions(_)
        | LayerKind::ImportHeightmap(_)
        | LayerKind::GradientReconstruct(_) => DomainRole::ShapeLayer,

        // Terrain filters (biome character) - also Shape-compat when under Shape folder
        LayerKind::Mountains(_)
        | LayerKind::Mesa(_)
        | LayerKind::Volcano(_)
        | LayerKind::Dunes(_)
        | LayerKind::Canyons(_)
        | LayerKind::Terrace(_)
        | LayerKind::Plateau(_)
        | LayerKind::Blur(_)
        | LayerKind::Coastal(_)
        | LayerKind::EffectFilter(_)
        | LayerKind::GeomorphicDetail(_)
        | LayerKind::OverhangStamp(_)
        | LayerKind::LocalSdf(_) => DomainRole::TerrainFilter,

        // Simulations
        LayerKind::ThermalErosion(_)
        | LayerKind::DebrisFlow(_)
        | LayerKind::HydraulicErosion(_)
        | LayerKind::StreamPowerErosion(_)
        | LayerKind::MultiScaleAmplify(_)
        | LayerKind::LandscapeEvolution(_)
        | LayerKind::HydrologyRepair(_)
        | LayerKind::EcosystemFeedback(_)
        | LayerKind::RiverCarve(_)
        | LayerKind::RiverNetwork(_)
        | LayerKind::SandSimulation(_)
        | LayerKind::FluidSimulation(_) => DomainRole::SimulationLayer,

        LayerKind::Materials(_) => DomainRole::MaterialLayer,
        LayerKind::Vegetation(_) | LayerKind::ScatterObjects(_) => DomainRole::ScatterLayer,

        // Dual / legacy
        LayerKind::Path(_) => DomainRole::ShapeLayer, // height path; object path is future
        LayerKind::ProceduralShape(_)
        | LayerKind::Stamp2d(_)
        | LayerKind::Stamp3d(_)
        | LayerKind::PolygonHeight(_) => DomainRole::ShapeLayer,
        LayerKind::Uplift(_) => DomainRole::CompatibilityLegacy,
        LayerKind::Biomes(_) => DomainRole::CompatibilityLegacy, // climate bands, not WC biome
    }
}

/// Context-aware classification using WC folder / biome section placement.
pub fn classify_in_context(
    kind: &LayerKind,
    category: Option<StackCategory>,
    biome_section: Option<BiomeSection>,
) -> DomainRole {
    let base = classify_layer_kind(kind);
    if let Some(section) = biome_section {
        return match section {
            BiomeSection::Filters => {
                if matches!(base, DomainRole::SimulationLayer) {
                    DomainRole::SimulationLayer
                } else if matches!(base, DomainRole::MaterialLayer | DomainRole::ScatterLayer) {
                    base
                } else {
                    DomainRole::TerrainFilter
                }
            }
            BiomeSection::Materials => DomainRole::MaterialLayer,
            BiomeSection::Objects => {
                if matches!(base, DomainRole::ScatterLayer) {
                    DomainRole::ScatterLayer
                } else {
                    DomainRole::ObjectLayer
                }
            }
            BiomeSection::LocalSims => DomainRole::SimulationLayer,
        };
    }
    if let Some(cat) = category {
        return match cat {
            StackCategory::Foundation | StackCategory::Shape => {
                // Landforms under Shape folder remain Shape-compat for migrated projects.
                if matches!(
                    base,
                    DomainRole::TerrainFilter
                        | DomainRole::ShapeLayer
                        | DomainRole::CompatibilityLegacy
                ) {
                    DomainRole::ShapeLayer
                } else {
                    base
                }
            }
            StackCategory::Simulation => DomainRole::SimulationLayer,
            StackCategory::Mask => DomainRole::MaskLayer,
            StackCategory::Surface => base,
        };
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::{FlatParams, HydraulicErosionParams, MountainParams, VegetationParams};

    #[test]
    fn classifies_core_kinds() {
        assert_eq!(
            classify_layer_kind(&LayerKind::Flat(FlatParams::default())),
            DomainRole::ShapeLayer
        );
        assert_eq!(
            classify_layer_kind(&LayerKind::Mountains(MountainParams::default())),
            DomainRole::TerrainFilter
        );
        assert_eq!(
            classify_layer_kind(&LayerKind::HydraulicErosion(
                HydraulicErosionParams::default()
            )),
            DomainRole::SimulationLayer
        );
        assert_eq!(
            classify_layer_kind(&LayerKind::Vegetation(VegetationParams::default())),
            DomainRole::ScatterLayer
        );
    }

    #[test]
    fn shape_folder_promotes_mountains_to_shape_compat() {
        let role = classify_in_context(
            &LayerKind::Mountains(MountainParams::default()),
            Some(StackCategory::Shape),
            None,
        );
        assert_eq!(role, DomainRole::ShapeLayer);
    }

    #[test]
    fn biome_filters_keep_mountains_as_filter() {
        let role = classify_in_context(
            &LayerKind::Mountains(MountainParams::default()),
            None,
            Some(BiomeSection::Filters),
        );
        assert_eq!(role, DomainRole::TerrainFilter);
    }
}
