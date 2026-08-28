//! Operation categories and field contracts for terrain layers.

use super::LayerKind;
use crate::fields::FieldId;
use crate::tiling::DirtyClass;
use serde::{Deserialize, Serialize};

use super::ScaleBand;

/// Internal height-op classification (create vs transform vs sim / surface).
///
/// **Not** the World Creator artist taxonomy. Artist-facing folders are
/// Shape Layers / Biome Filters / Simulation - see [`StackCategory`](super::group_mode::StackCategory)
/// and [`biome_destination_section`](super::stack::biome_destination_section). Do not surface
/// "Generator" / "Modifier" as Shape workflow labels in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OperationCategory {
    /// Creates height contribution (internal; often a Shape Layer or Filter by kind).
    Generator,
    /// Transforms existing height (internal; Path/Polygon stay Shape; Terrace/filters -> Biome Filters).
    Modifier,
    Simulation,
    Analysis,
    Surface,
    Group,
    ImportedData,
    Organisation,
}

impl OperationCategory {
    /// Internal / debug label - prefer WC folder names in artist UI.
    pub fn label(self) -> &'static str {
        match self {
            OperationCategory::Generator => "Height source",
            OperationCategory::Modifier => "Height transform",
            OperationCategory::Simulation => "Simulation",
            OperationCategory::Analysis => "Analysis",
            OperationCategory::Surface => "Surface",
            OperationCategory::Group => "Group",
            OperationCategory::ImportedData => "Imported",
            OperationCategory::Organisation => "Organisation",
        }
    }

    /// Internal / debug badge - not used for WC Shape Layers UI.
    pub fn short_badge(self) -> &'static str {
        match self {
            OperationCategory::Generator => "HSRC",
            OperationCategory::Modifier => "HXFM",
            OperationCategory::Simulation => "SIM",
            OperationCategory::Analysis => "ANL",
            OperationCategory::Surface => "SRF",
            OperationCategory::Group => "GRP",
            OperationCategory::ImportedData => "IMP",
            OperationCategory::Organisation => "ORG",
        }
    }
}

/// Aux field a distribution node reads, if any.
///
/// Terrain-feature nodes sample a published channel: `Flow` reads flow
/// accumulation, `Curvature` and `Cavity` read curvature, and `Slope`,
/// `Steepness` and `Angle` read slope. Fills, noise and effects are
/// self-contained, and `MaskAsset` resolves through the document's asset list,
/// which is not visible from a `LayerKind`.
///
/// This lives here rather than on `DistNodeKind` because naming a `FieldId`
/// from `mask` would close a module cycle - `fields` already depends on `mask`.
fn dist_node_field(kind: &crate::mask::DistNodeKind) -> Option<FieldId> {
    use crate::mask::DistNodeKind as K;
    match kind {
        K::Slope { .. } | K::Steepness { .. } | K::Angle { .. } => Some(FieldId::Slope),
        K::Curvature { .. } | K::Cavity { .. } => Some(FieldId::Curvature),
        K::Flow { .. } => Some(FieldId::FlowAccumulation),
        _ => None,
    }
}

impl LayerKind {
    pub fn category(&self) -> OperationCategory {
        match self {
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
            | LayerKind::Mesa(_)
            | LayerKind::Island(_)
            | LayerKind::Mountains(_)
            | LayerKind::Volcano(_)
            | LayerKind::Uplift(_)
            | LayerKind::Dunes(_)
            | LayerKind::Canyons(_)
            | LayerKind::VoronoiRegions(_)
            | LayerKind::ProceduralShape(_)
            | LayerKind::Stamp2d(_)
            | LayerKind::Stamp3d(_) => OperationCategory::Generator,

            LayerKind::ImportHeightmap(_) => OperationCategory::ImportedData,

            LayerKind::Terrace(_)
            | LayerKind::GradientReconstruct(_)
            | LayerKind::GeomorphicDetail(_)
            | LayerKind::Plateau(_)
            | LayerKind::Blur(_)
            | LayerKind::Coastal(_)
            | LayerKind::EffectFilter(_)
            | LayerKind::Path(_)
            | LayerKind::PolygonHeight(_)
            | LayerKind::OverhangStamp(_)
            | LayerKind::LocalSdf(_) => OperationCategory::Modifier,

            LayerKind::ThermalErosion(_)
            | LayerKind::LandscapeEvolution(_)
            | LayerKind::HydrologyRepair(_)
            | LayerKind::EcosystemFeedback(_)
            | LayerKind::HydraulicErosion(_)
            | LayerKind::DebrisFlow(_)
            | LayerKind::StreamPowerErosion(_)
            | LayerKind::MultiScaleAmplify(_)
            | LayerKind::RiverCarve(_)
            | LayerKind::RiverNetwork(_)
            | LayerKind::SandSimulation(_)
            | LayerKind::FluidSimulation(_) => OperationCategory::Simulation,

            LayerKind::Materials(_)
            | LayerKind::Biomes(_)
            | LayerKind::Vegetation(_)
            | LayerKind::ScatterObjects(_) => OperationCategory::Surface,
        }
    }

    /// Fields this operation requires to be present (beyond height for modifiers/sims).
    pub fn required_fields(&self) -> Vec<FieldId> {
        match self {
            LayerKind::ThermalErosion(_)
            | LayerKind::HydraulicErosion(_)
            | LayerKind::DebrisFlow(_)
            | LayerKind::StreamPowerErosion(_)
            | LayerKind::SculptStrokes(_)
            | LayerKind::TerrainConstraints(_)
            | LayerKind::GradientReconstruct(_)
            | LayerKind::LandscapeEvolution(_)
            | LayerKind::HydrologyRepair(_)
            | LayerKind::GeomorphicDetail(_)
            | LayerKind::EcosystemFeedback(_)
            | LayerKind::MultiScaleAmplify(_)
            | LayerKind::RiverCarve(_)
            | LayerKind::RiverNetwork(_)
            | LayerKind::SandSimulation(_)
            | LayerKind::FluidSimulation(_)
            | LayerKind::Terrace(_)
            | LayerKind::Plateau(_)
            | LayerKind::Blur(_)
            | LayerKind::Coastal(_)
            | LayerKind::EffectFilter(_)
            | LayerKind::Path(_)
            | LayerKind::PolygonHeight(_) => vec![FieldId::Height],
            LayerKind::Biomes(_) => vec![FieldId::Height],
            LayerKind::Vegetation(_) | LayerKind::ScatterObjects(_) => vec![FieldId::Height],
            LayerKind::Materials(_) => vec![FieldId::Height],
            _ => Vec::new(),
        }
    }

    pub fn optional_fields(&self) -> Vec<FieldId> {
        match self {
            LayerKind::SandSimulation(_) => vec![
                FieldId::WindExposure,
                FieldId::Vegetation,
                FieldId::SoilMoisture,
            ],
            LayerKind::HydraulicErosion(_)
            | LayerKind::ThermalErosion(_)
            | LayerKind::DebrisFlow(_) => {
                vec![
                    FieldId::Hardness,
                    FieldId::Rainfall,
                    FieldId::SoilDepth,
                    FieldId::Vegetation,
                    FieldId::BedrockHeight,
                    FieldId::SedimentThickness,
                    FieldId::DebrisDepth,
                ]
            }
            LayerKind::StreamPowerErosion(_) => {
                vec![FieldId::Hardness, FieldId::FlowAccumulation]
            }
            LayerKind::Biomes(_) => vec![
                FieldId::Temperature,
                FieldId::Rainfall,
                FieldId::Wetness,
                FieldId::Slope,
            ],
            LayerKind::Vegetation(_) => {
                vec![
                    FieldId::Biomes,
                    FieldId::Wetness,
                    FieldId::Slope,
                    FieldId::Snow,
                ]
            }
            LayerKind::ScatterObjects(p) => {
                // The static half is what the placer itself consults. The rest
                // depends on how the artist configured coverage and exclusion,
                // so derive it rather than guess: a Flow or Curvature node reads
                // a channel that is not in the static list, and an undeclared
                // read means the layer is not dirtied when that channel changes.
                let mut fields = vec![
                    FieldId::Biomes,
                    FieldId::Materials,
                    FieldId::Wetness,
                    FieldId::Slope,
                ];
                let mut note = |kind: &crate::mask::DistNodeKind| {
                    if let Some(f) = dist_node_field(kind) {
                        if !fields.contains(&f) {
                            fields.push(f);
                        }
                    }
                };
                p.coverage.visit_node_kinds(&mut note);
                p.exclusion.visit_node_kinds(&mut note);
                fields
            }
            LayerKind::Materials(_) => {
                vec![
                    FieldId::Slope,
                    FieldId::Curvature,
                    FieldId::Wetness,
                    FieldId::Erosion,
                ]
            }
            _ => Vec::new(),
        }
    }

    pub fn produced_fields(&self) -> Vec<FieldId> {
        match self {
            LayerKind::SculptStrokes(_) => vec![
                FieldId::Height,
                FieldId::Hardness,
                FieldId::Named(crate::fields::keys::SCULPT_PROTECTION.into()),
                FieldId::Named(crate::fields::keys::UPLIFT_RATE.into()),
                FieldId::SedimentThickness,
                FieldId::Named(crate::fields::keys::EDIT_REGION.into()),
            ],
            LayerKind::TerrainConstraints(_) => vec![
                FieldId::Height,
                FieldId::Named(crate::fields::keys::CONSTRAINT_TARGET.into()),
                FieldId::Named(crate::fields::keys::CONSTRAINT_WEIGHT.into()),
                FieldId::Named(crate::fields::keys::SCULPT_PROTECTION.into()),
                FieldId::Named(crate::fields::keys::UPLIFT_RATE.into()),
                FieldId::Named(crate::fields::keys::EDIT_REGION.into()),
            ],
            LayerKind::GradientReconstruct(_) => vec![
                FieldId::Height,
                FieldId::Named(crate::fields::keys::CONSTRAINT_ERROR.into()),
            ],
            LayerKind::LandscapeEvolution(_) => vec![
                FieldId::Height,
                FieldId::FlowDirection,
                FieldId::FlowAccumulation,
                FieldId::StreamOrder,
                FieldId::SpeIncision,
                FieldId::Erosion,
                FieldId::Deposition,
                FieldId::WaterDischarge,
                FieldId::SedimentThickness,
                FieldId::Named(crate::fields::keys::UPLIFT_RATE.into()),
                FieldId::Named(crate::fields::keys::TECTONIC_BASE.into()),
            ],
            LayerKind::HydrologyRepair(_) => vec![
                FieldId::Height,
                FieldId::FlowDirection,
                FieldId::FlowAccumulation,
                FieldId::StreamOrder,
                FieldId::SpeIncision,
                FieldId::Named(crate::fields::keys::REPAIR_REGION.into()),
            ],
            LayerKind::GeomorphicDetail(_) => vec![
                FieldId::Height,
                FieldId::Named(crate::fields::keys::DETAIL_MASK.into()),
                FieldId::FineFlow,
                FieldId::MicroChannel,
                FieldId::RidgeBreakup,
                FieldId::FineErosion,
            ],
            LayerKind::EcosystemFeedback(_) => vec![
                FieldId::Height,
                FieldId::Hardness,
                FieldId::Deposition,
                FieldId::Named(crate::fields::keys::ROOT_COHESION.into()),
            ],
            LayerKind::Island(_) => vec![
                FieldId::Height,
                FieldId::Named(crate::fields::keys::LAND_MASK.into()),
                FieldId::Named(crate::fields::keys::SHORE_DISTANCE.into()),
                FieldId::Named(crate::fields::keys::BATHYMETRY.into()),
                FieldId::Named(crate::fields::keys::SHELF.into()),
                FieldId::Named(crate::fields::keys::BEACH.into()),
                FieldId::Named(crate::fields::keys::REEF.into()),
                FieldId::Named(crate::fields::keys::MOUNTAIN_MASK.into()),
            ],
            LayerKind::HydraulicErosion(_) => vec![
                FieldId::Height,
                FieldId::Wetness,
                FieldId::Sediment,
                FieldId::Erosion,
                FieldId::Deposition,
                FieldId::Water,
                FieldId::WaterVelocity,
                FieldId::FlowAccumulation,
                FieldId::ChannelMask,
                FieldId::BedrockHeight,
                FieldId::SedimentThickness,
                FieldId::Rainfall,
                FieldId::Hardness,
                FieldId::Named(crate::fields::keys::WATER_DEPTH.into()),
            ],
            LayerKind::SandSimulation(_) | LayerKind::Dunes(_) => vec![
                FieldId::Height,
                FieldId::SandDepth,
                FieldId::BedrockHeight,
                FieldId::WindDirection,
                FieldId::WindSpeed,
                FieldId::SandFlux,
                FieldId::Erosion,
                FieldId::Deposition,
                FieldId::Sheltering,
                FieldId::DuneCrest,
                FieldId::SandMaterialMask,
                FieldId::FlowDirection,
            ],
            LayerKind::ThermalErosion(_) => {
                vec![
                    FieldId::Height,
                    FieldId::Erosion,
                    FieldId::Deposition,
                    FieldId::Hardness,
                    FieldId::BedrockHeight,
                    FieldId::DebrisDepth,
                    FieldId::SedimentThickness,
                    FieldId::TalusStability,
                    FieldId::Instability,
                ]
            }
            LayerKind::DebrisFlow(_) => {
                vec![
                    FieldId::Height,
                    FieldId::Erosion,
                    FieldId::Deposition,
                    FieldId::BedrockHeight,
                    FieldId::DebrisDepth,
                    FieldId::SedimentThickness,
                    FieldId::SlidePath,
                    FieldId::Instability,
                    FieldId::FlowAccumulation,
                    FieldId::Hardness,
                ]
            }
            LayerKind::StreamPowerErosion(_) => vec![
                FieldId::Height,
                FieldId::FlowDirection,
                FieldId::FlowAccumulation,
                FieldId::StreamOrder,
                FieldId::SpeIncision,
                FieldId::Erosion,
                FieldId::Hardness,
            ],
            LayerKind::RiverCarve(_) => vec![
                FieldId::Height,
                FieldId::FlowDirection,
                FieldId::FlowAccumulation,
                FieldId::StreamOrder,
                FieldId::SpeIncision,
                FieldId::Wetness,
            ],
            LayerKind::Materials(_) => {
                vec![
                    FieldId::Materials,
                    FieldId::Hardness,
                    FieldId::StrataReference,
                ]
            }
            LayerKind::Biomes(_) => vec![
                FieldId::Biomes,
                FieldId::Temperature,
                FieldId::Rainfall,
                FieldId::Humidity,
                FieldId::Aridity,
                FieldId::Snow,
                FieldId::SoilMoisture,
                FieldId::WindExposure,
            ],
            // Root cohesion is the only path that writes hardness, and it is
            // off by default. Declaring hardness unconditionally would
            // invalidate every hardness consumer (erosion sims) on any
            // vegetation edit; declaring it dynamically keeps those cached.
            // Callers that change parameters must union the *previous*
            // contract too - see `StackEvaluator::mark_dirty_from_fields`.
            LayerKind::Vegetation(p) => {
                if p.root_cohesion > 1e-6 {
                    vec![FieldId::Vegetation, FieldId::Hardness]
                } else {
                    vec![FieldId::Vegetation]
                }
            }
            // Scatter Objects is a height passthrough: it publishes the
            // scatter density channel the renderer / export already consume
            // plus the candidate field the placement was drawn from.
            LayerKind::ScatterObjects(_) => {
                vec![FieldId::ScatterDensity, FieldId::ScatterCandidates]
            }
            LayerKind::OverhangStamp(_) | LayerKind::LocalSdf(_) => {
                vec![
                    FieldId::Height,
                    FieldId::OverhangCeiling,
                    FieldId::OverhangMask,
                ]
            }
            LayerKind::MultiScaleAmplify(_) => vec![
                FieldId::Height,
                FieldId::Erosion,
                FieldId::Deposition,
                FieldId::Hardness,
            ],
            LayerKind::FluidSimulation(_) => vec![
                FieldId::Height,
                FieldId::Wetness,
                FieldId::Named(crate::fields::keys::WATER_DEPTH.into()),
            ],
            LayerKind::RiverNetwork(_) | LayerKind::Path(_) => {
                vec![FieldId::Height, FieldId::Wetness]
            }
            _ if matches!(self.category(), OperationCategory::Generator) => {
                vec![FieldId::Height]
            }
            _ => vec![FieldId::Height],
        }
    }

    pub fn modified_fields(&self) -> Vec<FieldId> {
        self.produced_fields()
            .into_iter()
            .filter(|f| {
                *f == FieldId::Height
                    || matches!(
                        self.category(),
                        OperationCategory::Simulation | OperationCategory::Modifier
                    )
            })
            .collect()
    }

    pub fn spatial_dependency(&self) -> DirtyClass {
        crate::tiling::dirty_class_for(self)
    }

    /// Phase 11 Rule 3 - scale ownership for this operator family.
    ///
    /// Micro / MultiScale operators must not replace the macro silhouette.
    pub fn scale_band(&self) -> ScaleBand {
        match self {
            // --- MACRO: landmass / tectonic silhouette ---
            LayerKind::Flat(_)
            | LayerKind::Ramp(_)
            | LayerKind::SculptBase(_)
            | LayerKind::ImportHeightmap(_)
            | LayerKind::Island(_)
            | LayerKind::Mountains(_)
            | LayerKind::Mesa(_)
            | LayerKind::Volcano(_)
            | LayerKind::Uplift(_)
            | LayerKind::VoronoiRegions(_)
            | LayerKind::ProceduralShape(_)
            | LayerKind::LandscapeEvolution(_) => ScaleBand::Macro,

            // --- MULTI-SCALE: cascade while locking longer wavelengths ---
            LayerKind::MultiScaleAmplify(_)
            | LayerKind::GeomorphicDetail(_)
            | LayerKind::HydrologyRepair(_) => ScaleBand::MultiScale,

            // --- MICRO: fine surface / decorative detail ---
            LayerKind::Blur(_)
            | LayerKind::EffectFilter(_)
            | LayerKind::OverhangStamp(_)
            | LayerKind::LocalSdf(_)
            | LayerKind::SandSimulation(_)
            | LayerKind::Dunes(_) => ScaleBand::Micro,

            // --- MESO: ridges, valleys, drainage, primary erosion ---
            LayerKind::NoiseValue(_)
            | LayerKind::NoisePerlin(_)
            | LayerKind::NoiseOpenSimplex(_)
            | LayerKind::NoiseWorley(_)
            | LayerKind::Fbm(_)
            | LayerKind::Ridged(_)
            | LayerKind::DomainWarp(_)
            | LayerKind::Terrace(_)
            | LayerKind::Plateau(_)
            | LayerKind::Canyons(_)
            | LayerKind::Path(_)
            | LayerKind::PolygonHeight(_)
            | LayerKind::Stamp2d(_)
            | LayerKind::Stamp3d(_)
            | LayerKind::SculptStrokes(_)
            | LayerKind::TerrainConstraints(_)
            | LayerKind::GradientReconstruct(_)
            | LayerKind::ThermalErosion(_)
            | LayerKind::HydraulicErosion(_)
            | LayerKind::DebrisFlow(_)
            | LayerKind::StreamPowerErosion(_)
            | LayerKind::RiverCarve(_)
            | LayerKind::RiverNetwork(_)
            | LayerKind::Coastal(_)
            | LayerKind::FluidSimulation(_)
            | LayerKind::EcosystemFeedback(_)
            | LayerKind::Materials(_)
            | LayerKind::Biomes(_)
            | LayerKind::Vegetation(_)
            | LayerKind::ScatterObjects(_) => ScaleBand::Meso,
        }
    }
}

/// Declared fields for an operation (used by evaluator validation / UI).
#[derive(Debug, Clone)]
pub struct FieldContract {
    pub required: Vec<FieldId>,
    pub optional: Vec<FieldId>,
    pub produced: Vec<FieldId>,
    pub modified: Vec<FieldId>,
    pub spatial: DirtyClass,
}

impl Default for FieldContract {
    fn default() -> Self {
        Self {
            required: Vec::new(),
            optional: Vec::new(),
            produced: Vec::new(),
            modified: Vec::new(),
            spatial: DirtyClass::Local,
        }
    }
}

impl FieldContract {
    pub fn from_kind(kind: &LayerKind) -> Self {
        Self {
            required: kind.required_fields(),
            optional: kind.optional_fields(),
            produced: kind.produced_fields(),
            modified: kind.modified_fields(),
            spatial: kind.spatial_dependency(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::{
        EffectFilterParams, HydraulicErosionParams, MultiScaleAmplifyParams, ScaleBand,
        UpliftParams,
    };

    /// A Scatter Objects layer whose coverage reads Flow must declare
    /// FlowAccumulation, or `layer_contract_touches` will not dirty it when the
    /// flow field changes and it will keep placing props from a stale field.
    ///
    /// The static list is `[Biomes, Materials, Wetness, Slope]` - none of which
    /// is flow - so before this the declaration was simply wrong for any
    /// artist who reached for a Flow node.
    #[test]
    fn scatter_declares_the_fields_its_distribution_actually_reads() {
        use crate::layer::ScatterObjectsParams;
        use crate::mask::{DistNode, DistNodeKind, Distribution};

        let mut p = ScatterObjectsParams::default();
        assert!(
            !LayerKind::ScatterObjects(p.clone())
                .optional_fields()
                .contains(&FieldId::FlowAccumulation),
            "an unconfigured scatter has no reason to declare flow"
        );

        p.coverage = Distribution::from_nodes(vec![DistNode {
            kind: DistNodeKind::Flow { min: 0.0, max: 1.0 },
            ..DistNode::default()
        }]);
        let fields = LayerKind::ScatterObjects(p).optional_fields();
        assert!(
            fields.contains(&FieldId::FlowAccumulation),
            "a Flow coverage node reads flow accumulation, so the layer must              declare it; got {fields:?}"
        );
        // The static half must survive the derivation.
        for f in [
            FieldId::Biomes,
            FieldId::Materials,
            FieldId::Wetness,
            FieldId::Slope,
        ] {
            assert!(fields.contains(&f), "{f:?} dropped out of the declaration");
        }
    }

    /// Exclusion is a dependency too - it is subtracted from coverage, so a
    /// stale exclusion field changes placement just as much.
    #[test]
    fn scatter_derives_fields_from_exclusion_and_from_nested_nodes() {
        use crate::layer::ScatterObjectsParams;
        use crate::mask::{DistNode, DistNodeKind, Distribution};

        let exclusion = Distribution::from_nodes(vec![DistNode {
            kind: DistNodeKind::Fill { value: 1.0 },
            // Nested effect nodes hang off a parent, so the walk has to recurse
            // or a curvature filter one level down goes undeclared.
            children: vec![DistNode {
                kind: DistNodeKind::Curvature {
                    min: 0.0,
                    max: 1.0,
                },
                ..DistNode::default()
            }],
            ..DistNode::default()
        }]);
        let p = ScatterObjectsParams {
            exclusion,
            ..ScatterObjectsParams::default()
        };
        let fields = LayerKind::ScatterObjects(p).optional_fields();
        assert!(
            fields.contains(&FieldId::Curvature),
            "a nested Curvature node under an exclusion node must still be              declared; got {fields:?}"
        );
    }

    /// The declaration must not repeat a field the static half already lists.
    #[test]
    fn derived_fields_do_not_duplicate_the_static_ones() {
        use crate::layer::ScatterObjectsParams;
        use crate::mask::{DistNode, DistNodeKind, Distribution};

        let p = ScatterObjectsParams {
            coverage: Distribution::from_nodes(vec![DistNode {
                kind: DistNodeKind::Slope {
                    min_deg: 0.0,
                    max_deg: 90.0,
                },
                ..DistNode::default()
            }]),
            ..ScatterObjectsParams::default()
        };
        let fields = LayerKind::ScatterObjects(p).optional_fields();
        let slopes = fields.iter().filter(|f| **f == FieldId::Slope).count();
        assert_eq!(slopes, 1, "Slope listed {slopes} times: {fields:?}");
    }

    #[test]
    fn hydraulic_is_simulation() {
        let k = LayerKind::HydraulicErosion(HydraulicErosionParams::default());
        assert_eq!(k.category(), OperationCategory::Simulation);
        assert!(k.produced_fields().contains(&FieldId::Wetness));
    }

    #[test]
    fn erosion_family_declares_distinct_debris_and_sediment_fields() {
        for kind in [
            LayerKind::ThermalErosion(Default::default()),
            LayerKind::DebrisFlow(Default::default()),
        ] {
            assert!(kind.optional_fields().contains(&FieldId::DebrisDepth));
            assert!(kind.optional_fields().contains(&FieldId::SedimentThickness));
            assert!(kind.produced_fields().contains(&FieldId::DebrisDepth));
            assert!(kind.produced_fields().contains(&FieldId::SedimentThickness));
        }
    }

    #[test]
    fn scale_band_separates_macro_from_micro() {
        assert_eq!(
            LayerKind::Uplift(UpliftParams::default()).scale_band(),
            ScaleBand::Macro
        );
        assert_eq!(
            LayerKind::HydraulicErosion(HydraulicErosionParams::default()).scale_band(),
            ScaleBand::Meso
        );
        assert_eq!(
            LayerKind::EffectFilter(EffectFilterParams::default()).scale_band(),
            ScaleBand::Micro
        );
        assert_eq!(
            LayerKind::MultiScaleAmplify(MultiScaleAmplifyParams::default()).scale_band(),
            ScaleBand::MultiScale
        );
        assert!(ScaleBand::Micro.respects_macro_silhouette());
    }
}
