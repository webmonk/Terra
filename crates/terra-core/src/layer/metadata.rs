//! Shared layer type / instance metadata (domain-facing; UI-agnostic).

use super::cache_policy::{CachePolicy, CacheState};
use super::operation::{FieldContract, OperationCategory};
use super::workflow::WorkflowStage;
use super::{Layer, LayerGroup, LayerId, LayerKind};
use crate::fields::FieldId;
use serde::{Deserialize, Serialize};

/// Visual accent bucket for lists / badges (not a hardcoded UI colour).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum AccentCategory {
    #[default]
    Neutral,
    Foundation,
    Generator,
    Simulation,
    Mask,
    Biome,
    Material,
    Scatter,
    Object,
    Output,
}

impl AccentCategory {
    pub fn label(self) -> &'static str {
        match self {
            AccentCategory::Neutral => "Neutral",
            AccentCategory::Foundation => "Foundation",
            // Maps from WorkflowStage::Generation — artist language is Shape, not Generator.
            AccentCategory::Generator => "Shape",
            AccentCategory::Simulation => "Simulation",
            AccentCategory::Mask => "Mask",
            AccentCategory::Biome => "Biome",
            AccentCategory::Material => "Material",
            AccentCategory::Scatter => "Scatter",
            AccentCategory::Object => "Object",
            AccentCategory::Output => "Output",
        }
    }

    pub fn from_workflow(stage: WorkflowStage) -> Self {
        match stage {
            WorkflowStage::Foundation => AccentCategory::Foundation,
            WorkflowStage::Generation => AccentCategory::Generator,
            WorkflowStage::Simulation => AccentCategory::Simulation,
            WorkflowStage::Masks => AccentCategory::Mask,
            WorkflowStage::BiomePlacement => AccentCategory::Biome,
            WorkflowStage::Materials => AccentCategory::Material,
            WorkflowStage::Scatter => AccentCategory::Scatter,
            WorkflowStage::Objects => AccentCategory::Object,
            WorkflowStage::Output => AccentCategory::Output,
        }
    }
}

/// How a layer type relates to masks / distribution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum MaskCompatibility {
    /// Layer contribution is scaled by its distribution masks.
    #[default]
    SupportsMasks,
    /// Masks are ignored (rare; mostly organisational).
    IgnoresMasks,
    /// Layer is primarily a mask / weight producer.
    ProducesMask,
}

/// Static capabilities for a layer *type* (not an instance).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LayerCapabilities {
    pub supports_thumbnails: bool,
    pub supports_duplication: bool,
    pub can_reorder: bool,
    pub can_contain_children: bool,
    /// Appears in Add Layer / registry create paths.
    pub user_creatable: bool,
}

impl Default for LayerCapabilities {
    fn default() -> Self {
        Self {
            supports_thumbnails: true,
            supports_duplication: true,
            can_reorder: true,
            can_contain_children: false,
            user_creatable: true,
        }
    }
}

/// Declared input / output channel summary for UI and validation hints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChannelRequirements {
    pub required: Vec<FieldId>,
    pub optional: Vec<FieldId>,
    pub produced: Vec<FieldId>,
}

impl ChannelRequirements {
    pub fn from_contract(c: &FieldContract) -> Self {
        Self {
            required: c.required.clone(),
            optional: c.optional.clone(),
            produced: c.produced.clone(),
        }
    }
}

/// Stable description of a creatable / known layer *type* (not an instance).
///
/// Not project-serialized — lives in the in-process [`super::LayerTypeRegistry`].
#[derive(Debug, Clone)]
pub struct LayerTypeMeta {
    /// Stable machine id, e.g. `"mountain"`, `"hydraulic_erosion"`.
    pub type_id: &'static str,
    pub display_name: &'static str,
    pub workflow_stage: WorkflowStage,
    /// Icon key for UI mapping (terra-gui / asset thumbs); not a GPU handle.
    pub icon_key: &'static str,
    pub accent: AccentCategory,
    pub description: &'static str,
    pub operation_category: OperationCategory,
    pub channels: ChannelRequirements,
    pub mask_compatibility: MaskCompatibility,
    pub capabilities: LayerCapabilities,
    /// Inspector section ids the UI may open (presentation interprets these).
    pub inspector_sections: &'static [&'static str],
    /// Suggested follow-on type ids (empty if none).
    pub suggested_next: &'static [&'static str],
}

/// Runtime build / evaluation badge for an instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum BuildStatus {
    #[default]
    Idle,
    Pending,
    Computing,
    Ready,
    /// Upstream Shape / geometry changed; not rebuilt yet (sims stay frozen).
    Outdated,
    Error,
}

/// Derived metadata for a live layer or group instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerInstanceMeta {
    pub id: LayerId,
    pub display_name: String,
    /// Registry type id when this is a leaf layer; `None` for groups.
    pub type_id: Option<String>,
    pub workflow_stage: Option<WorkflowStage>,
    pub icon_key: Option<String>,
    pub accent: AccentCategory,
    pub description: Option<String>,
    pub enabled: bool,
    /// Opacity / influence in \[0, 1\].
    pub opacity: f32,
    pub build_status: BuildStatus,
    pub cache_status: CacheState,
    pub cache_policy: CachePolicy,
    pub channels: ChannelRequirements,
    pub mask_compatibility: MaskCompatibility,
    pub capabilities: LayerCapabilities,
    pub inspector_sections: Vec<String>,
    pub suggested_next: Vec<String>,
}

impl LayerKind {
    /// Stable type id for registry / metadata (serde variant name style).
    pub fn type_id(&self) -> &'static str {
        match self {
            LayerKind::SculptBase(_) => "sculpt_base",
            LayerKind::SculptStrokes(_) => "sculpt_strokes",
            LayerKind::TerrainConstraints(_) => "terrain_constraints",
            LayerKind::GradientReconstruct(_) => "gradient_reconstruct",
            LayerKind::LandscapeEvolution(_) => "landscape_evolution",
            LayerKind::HydrologyRepair(_) => "hydrology_repair",
            LayerKind::GeomorphicDetail(_) => "geomorphic_detail",
            LayerKind::EcosystemFeedback(_) => "ecosystem_feedback",
            LayerKind::Flat(_) => "flat",
            LayerKind::Ramp(_) => "ramp",
            LayerKind::NoiseValue(_) => "noise_value",
            LayerKind::NoisePerlin(_) => "noise_perlin",
            LayerKind::NoiseOpenSimplex(_) => "noise_open_simplex",
            LayerKind::NoiseWorley(_) => "noise_worley",
            LayerKind::Fbm(_) => "fbm",
            LayerKind::Ridged(_) => "ridged",
            LayerKind::DomainWarp(_) => "domain_warp",
            LayerKind::Terrace(_) => "terrace",
            LayerKind::Plateau(_) => "plateau",
            LayerKind::Mesa(_) => "mesa",
            LayerKind::Island(_) => "island",
            LayerKind::Mountains(_) => "mountain",
            LayerKind::Volcano(_) => "volcano",
            LayerKind::Uplift(_) => "uplift",
            LayerKind::Dunes(_) => "dunes",
            LayerKind::Canyons(_) => "canyon",
            LayerKind::VoronoiRegions(_) => "voronoi",
            LayerKind::ImportHeightmap(_) => "import_heightmap",
            LayerKind::ThermalErosion(_) => "thermal_erosion",
            LayerKind::HydraulicErosion(_) => "hydraulic_erosion",
            LayerKind::DebrisFlow(_) => "debris_flow",
            LayerKind::StreamPowerErosion(_) => "stream_power",
            LayerKind::MultiScaleAmplify(_) => "multi_scale_amplify",
            LayerKind::RiverCarve(_) => "river_carve",
            LayerKind::Blur(_) => "blur",
            LayerKind::Coastal(_) => "coastal",
            LayerKind::EffectFilter(_) => "effect_filter",
            LayerKind::Materials(_) => "materials",
            LayerKind::Biomes(_) => "climate_biomes",
            LayerKind::Vegetation(_) => "vegetation",
            LayerKind::OverhangStamp(_) => "overhang_stamp",
            LayerKind::LocalSdf(_) => "local_sdf",
            LayerKind::Path(_) => "path",
            LayerKind::RiverNetwork(_) => "river_network",
            LayerKind::SandSimulation(_) => "sand_simulation",
            LayerKind::FluidSimulation(_) => "fluid_simulation",
            LayerKind::ProceduralShape(_) => "procedural_shape",
            LayerKind::Stamp2d(_) => "stamp_2d",
            LayerKind::Stamp3d(_) => "stamp_3d",
            LayerKind::PolygonHeight(_) => "polygon_height",
        }
    }

    /// Default human label for this kind (before instance rename).
    pub fn type_display_name(&self) -> &'static str {
        match self {
            LayerKind::SculptBase(_) => "Base",
            LayerKind::SculptStrokes(_) => "Semantic Sculpt",
            LayerKind::TerrainConstraints(_) => "Terrain Constraints",
            LayerKind::GradientReconstruct(_) => "Gradient Reconstruct",
            LayerKind::LandscapeEvolution(_) => "Landscape Evolution",
            LayerKind::HydrologyRepair(_) => "Hydrology Repair",
            LayerKind::GeomorphicDetail(_) => "Geomorphic Detail",
            LayerKind::EcosystemFeedback(_) => "Ecosystem Feedback",
            LayerKind::Flat(_) => "Flat",
            LayerKind::Ramp(_) => "Ramp",
            LayerKind::NoiseValue(_) => "Value Noise",
            LayerKind::NoisePerlin(_) => "Perlin Noise",
            LayerKind::NoiseOpenSimplex(_) => "OpenSimplex Noise",
            LayerKind::NoiseWorley(_) => "Worley Noise",
            LayerKind::Fbm(_) => "FBM",
            LayerKind::Ridged(_) => "Ridged",
            LayerKind::DomainWarp(_) => "Domain Warp",
            LayerKind::Terrace(_) => "Terrace",
            LayerKind::Plateau(_) => "Plateau",
            LayerKind::Mesa(_) => "Mesa",
            LayerKind::Island(_) => "Island",
            LayerKind::Mountains(_) => "Mountains",
            LayerKind::Volcano(_) => "Volcano",
            LayerKind::Uplift(_) => "Uplift",
            LayerKind::Dunes(_) => "Dunes",
            LayerKind::Canyons(_) => "Canyons",
            LayerKind::VoronoiRegions(_) => "Voronoi Regions",
            LayerKind::ImportHeightmap(_) => "Import Heightmap",
            LayerKind::ThermalErosion(_) => "Thermal Erosion",
            LayerKind::HydraulicErosion(_) => "Hydraulic Erosion",
            LayerKind::DebrisFlow(_) => "Debris Flow",
            LayerKind::StreamPowerErosion(_) => "Stream Power",
            LayerKind::MultiScaleAmplify(_) => "Multi-Scale Amplify",
            LayerKind::RiverCarve(_) => "River Carve",
            LayerKind::Blur(_) => "Blur",
            LayerKind::Coastal(_) => "Coastal",
            LayerKind::EffectFilter(_) => "Effect Filter",
            LayerKind::Materials(_) => "Materials",
            LayerKind::Biomes(_) => "Climate Classification",
            LayerKind::Vegetation(_) => "Vegetation",
            LayerKind::OverhangStamp(_) => "Overhang Stamp",
            LayerKind::LocalSdf(_) => "Local SDF",
            LayerKind::Path(_) => "Path",
            LayerKind::RiverNetwork(_) => "River Network",
            LayerKind::SandSimulation(_) => "Sand Simulation",
            LayerKind::FluidSimulation(_) => "Fluid Simulation",
            LayerKind::ProceduralShape(_) => "Procedural Shape",
            LayerKind::Stamp2d(_) => "2D Stamp",
            LayerKind::Stamp3d(_) => "3D Stamp",
            LayerKind::PolygonHeight(_) => "Polygon",
        }
    }

    /// Artist workflow stage for this kind (metadata).
    pub fn workflow_stage(&self) -> WorkflowStage {
        match self {
            LayerKind::SculptBase(_)
            | LayerKind::ImportHeightmap(_)
            | LayerKind::SculptStrokes(_)
            | LayerKind::TerrainConstraints(_) => WorkflowStage::Foundation,
            LayerKind::Materials(_) => WorkflowStage::Materials,
            LayerKind::Biomes(_) => WorkflowStage::BiomePlacement,
            LayerKind::Vegetation(_) => WorkflowStage::Scatter,
            LayerKind::SandSimulation(_) | LayerKind::FluidSimulation(_) => WorkflowStage::Objects,
            other => match other.category() {
                OperationCategory::Simulation | OperationCategory::Analysis => {
                    WorkflowStage::Simulation
                }
                OperationCategory::ImportedData => WorkflowStage::Foundation,
                OperationCategory::Surface => WorkflowStage::Materials,
                _ => WorkflowStage::Generation,
            },
        }
    }
}

impl Layer {
    /// Build instance metadata from a live layer (optional runtime statuses).
    pub fn instance_meta(
        &self,
        type_meta: Option<&LayerTypeMeta>,
        build_status: BuildStatus,
        cache_status: CacheState,
    ) -> LayerInstanceMeta {
        let contract = FieldContract::from_kind(&self.kind);
        let caps = type_meta
            .map(|m| m.capabilities)
            .unwrap_or(LayerCapabilities {
                supports_thumbnails: true,
                supports_duplication: !self.kind.is_sculpt_base(),
                can_reorder: !self.kind.is_sculpt_base(),
                can_contain_children: false,
                user_creatable: !self.kind.is_sculpt_base(),
            });
        let stage = type_meta
            .map(|m| m.workflow_stage)
            .unwrap_or_else(|| self.kind.workflow_stage());
        LayerInstanceMeta {
            id: self.id(),
            display_name: self.common.name.clone(),
            type_id: Some(self.kind.type_id().into()),
            workflow_stage: Some(stage),
            icon_key: type_meta.map(|m| m.icon_key.to_string()),
            accent: type_meta
                .map(|m| m.accent)
                .unwrap_or_else(|| AccentCategory::from_workflow(stage)),
            description: type_meta.map(|m| m.description.to_string()),
            enabled: self.common.enabled,
            opacity: self.common.opacity,
            build_status,
            cache_status,
            cache_policy: self.common.resolved_cache_policy(),
            channels: ChannelRequirements::from_contract(&contract),
            mask_compatibility: type_meta
                .map(|m| m.mask_compatibility)
                .unwrap_or(MaskCompatibility::SupportsMasks),
            capabilities: caps,
            inspector_sections: type_meta
                .map(|m| {
                    m.inspector_sections
                        .iter()
                        .map(|s| (*s).to_string())
                        .collect()
                })
                .unwrap_or_default(),
            suggested_next: type_meta
                .map(|m| m.suggested_next.iter().map(|s| (*s).to_string()).collect())
                .unwrap_or_default(),
        }
    }
}

impl LayerGroup {
    /// Instance metadata for a group (organisational / biome container).
    pub fn instance_meta(&self, cache_status: CacheState) -> LayerInstanceMeta {
        let (stage, accent, icon, caps) = if self.is_biome() {
            (
                Some(WorkflowStage::BiomePlacement),
                AccentCategory::Biome,
                Some("layers"),
                LayerCapabilities {
                    supports_thumbnails: true,
                    supports_duplication: true,
                    can_reorder: true,
                    can_contain_children: true,
                    user_creatable: true,
                },
            )
        } else if self.is_category_folder() {
            let stage = self
                .category
                .map(WorkflowStage::from_stack_category)
                .unwrap_or(WorkflowStage::Generation);
            (
                Some(stage),
                AccentCategory::from_workflow(stage),
                Some("folder"),
                LayerCapabilities {
                    supports_thumbnails: false,
                    supports_duplication: false,
                    can_reorder: false,
                    can_contain_children: true,
                    user_creatable: false,
                },
            )
        } else {
            (
                Some(WorkflowStage::Generation),
                AccentCategory::Neutral,
                Some("folder"),
                LayerCapabilities {
                    supports_thumbnails: false,
                    supports_duplication: true,
                    can_reorder: true,
                    can_contain_children: true,
                    user_creatable: true,
                },
            )
        };
        LayerInstanceMeta {
            id: self.id,
            display_name: self.name.clone(),
            type_id: None,
            workflow_stage: stage,
            icon_key: icon.map(|s| s.to_string()),
            accent,
            description: None,
            enabled: self.enabled,
            opacity: self.opacity,
            build_status: BuildStatus::Idle,
            cache_status,
            cache_policy: self.cache_policy,
            channels: ChannelRequirements::default(),
            mask_compatibility: MaskCompatibility::SupportsMasks,
            capabilities: caps,
            inspector_sections: vec!["group".into()],
            suggested_next: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::{FlatParams, HydraulicErosionParams, MountainParams};

    #[test]
    fn type_ids_stable() {
        assert_eq!(
            LayerKind::Mountains(MountainParams::default()).type_id(),
            "mountain"
        );
        assert_eq!(
            LayerKind::HydraulicErosion(HydraulicErosionParams::default()).type_id(),
            "hydraulic_erosion"
        );
    }

    #[test]
    fn instance_meta_from_layer() {
        let layer = Layer::new("Hills", LayerKind::Flat(FlatParams::default()));
        let meta = layer.instance_meta(None, BuildStatus::Ready, CacheState::Fresh);
        assert_eq!(meta.display_name, "Hills");
        assert_eq!(meta.type_id.as_deref(), Some("flat"));
        assert!(meta.enabled);
    }
}
