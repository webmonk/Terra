//! Layer type registry / factory — UI discovers types via metadata.
//!
//! This registry is the **single source of truth** for creatable layer types:
//! display names, icons, workflow stage, capabilities, and default factories.
//! UI catalogs (`terra-app` tool rail / add-layer menu) should derive entries from
//! [`LayerTypeRegistry`] rather than maintaining parallel hand-synced lists.

use super::metadata::{
    AccentCategory, ChannelRequirements, LayerCapabilities, LayerTypeMeta, MaskCompatibility,
};
use super::operation::FieldContract;
use super::workflow::WorkflowStage;
use super::{
    BiomesParams, BlurParams, CanyonParams, CoastalParams, DomainWarpParams, DuneParams,
    EcosystemFeedbackParams, EffectFilterParams, FbmParams, FlatParams, FluidSimParams,
    GeomorphicDetailParams, GradientReconstructParams, HydraulicErosionParams,
    HydrologyRepairParams, ImportHeightmapParams, IslandParams, LandscapeEvolutionParams, Layer,
    LayerKind, LocalSdfParams, MaterialsParams, MesaParams, MountainParams,
    MultiScaleAmplifyParams, NoiseParams, OverhangStampParams, PathParams, PlateauParams,
    PolygonHeightParams, ProceduralShapeParams, RampParams, RiverCarveParams, RiverNetworkParams,
    SandSimParams, SculptParams, SculptStrokeParams, Stamp2dParams, Stamp3dParams,
    StreamPowerParams, TerraceParams, TerrainConstraintParams, ThermalErosionParams, UpliftParams,
    VegetationParams, VolcanoParams, VoronoiParams, WorleyParams,
};
use std::collections::HashMap;

/// Authoring registry: metadata + default factories for layer kinds.
#[derive(Debug, Clone)]
pub struct LayerTypeRegistry {
    entries: Vec<LayerTypeMeta>,
    /// Parallel default-kind factories (same order as entries).
    factories: Vec<fn() -> LayerKind>,
    by_id: HashMap<&'static str, usize>,
}

impl Default for LayerTypeRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

impl LayerTypeRegistry {
    pub fn builtin() -> Self {
        let mut reg = Self {
            entries: Vec::new(),
            factories: Vec::new(),
            by_id: HashMap::new(),
        };
        reg.register_all();
        reg
    }

    fn push(&mut self, meta: LayerTypeMeta, factory: fn() -> LayerKind) {
        let idx = self.entries.len();
        self.by_id.insert(meta.type_id, idx);
        self.entries.push(meta);
        self.factories.push(factory);
    }

    pub fn get(&self, type_id: &str) -> Option<&LayerTypeMeta> {
        self.by_id.get(type_id).map(|&i| &self.entries[i])
    }

    pub fn all(&self) -> &[LayerTypeMeta] {
        &self.entries
    }

    pub fn creatable(&self) -> impl Iterator<Item = &LayerTypeMeta> {
        self.entries
            .iter()
            .filter(|m| m.capabilities.user_creatable)
    }

    pub fn by_stage(&self, stage: WorkflowStage) -> impl Iterator<Item = &LayerTypeMeta> {
        self.entries
            .iter()
            .filter(move |m| m.workflow_stage == stage)
    }

    /// Create a new layer with default params and the type's display name.
    pub fn create(&self, type_id: &str) -> Option<Layer> {
        let idx = *self.by_id.get(type_id)?;
        let meta = &self.entries[idx];
        let kind = (self.factories[idx])();
        Some(Layer::new(meta.display_name, kind))
    }

    /// Create with an explicit display name.
    pub fn create_named(&self, type_id: &str, name: impl Into<String>) -> Option<Layer> {
        let idx = *self.by_id.get(type_id)?;
        let kind = (self.factories[idx])();
        Some(Layer::new(name, kind))
    }

    /// Metadata for an existing kind (lookup by `LayerKind::type_id`).
    pub fn meta_for_kind(&self, kind: &LayerKind) -> Option<&LayerTypeMeta> {
        self.get(kind.type_id())
    }

    /// Icon key for a type id (for UI catalogs deriving from this registry).
    pub fn icon_key(&self, type_id: &str) -> Option<&'static str> {
        self.get(type_id).map(|m| m.icon_key)
    }

    /// Display name for a type id.
    pub fn display_name(&self, type_id: &str) -> Option<&'static str> {
        self.get(type_id).map(|m| m.display_name)
    }

    fn register_all(&mut self) {
        // Helpers shrink boilerplate.
        macro_rules! entry {
            (
                $id:literal, $name:literal, $stage:expr, $icon:literal, $desc:literal,
                $factory:expr, $creatable:expr, $sections:expr, $next:expr
            ) => {{
                let kind = ($factory)();
                let contract = FieldContract::from_kind(&kind);
                let op = kind.category();
                let stage: WorkflowStage = $stage;
                self.push(
                    LayerTypeMeta {
                        type_id: $id,
                        display_name: $name,
                        workflow_stage: stage,
                        icon_key: $icon,
                        accent: AccentCategory::from_workflow(stage),
                        description: $desc,
                        operation_category: op,
                        channels: ChannelRequirements::from_contract(&contract),
                        mask_compatibility: MaskCompatibility::SupportsMasks,
                        capabilities: LayerCapabilities {
                            supports_thumbnails: true,
                            supports_duplication: $creatable,
                            can_reorder: $creatable,
                            can_contain_children: false,
                            user_creatable: $creatable,
                        },
                        inspector_sections: $sections,
                        suggested_next: $next,
                    },
                    $factory,
                );
            }};
        }

        entry!(
            "sculpt_base",
            "Base",
            WorkflowStage::Foundation,
            "pencil",
            "Paintable foundation height buffer.",
            || LayerKind::SculptBase(SculptParams::default()),
            false,
            &["shape"],
            &[] as &[&str]
        );
        entry!(
            "import_heightmap",
            "Import Heightmap",
            WorkflowStage::Foundation,
            "image",
            "Import an external heightmap.",
            || LayerKind::ImportHeightmap(ImportHeightmapParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "sculpt_strokes",
            "Semantic Sculpt",
            WorkflowStage::Foundation,
            "pencil",
            "Resolution-independent editable sculpt strokes.",
            || LayerKind::SculptStrokes(SculptStrokeParams::default()),
            true,
            &["shape"],
            &["terrain_constraints"]
        );
        entry!(
            "terrain_constraints",
            "Terrain Constraints",
            WorkflowStage::Foundation,
            "activity",
            "Author elevations, ridges, valleys, rivers, coasts and protected shapes.",
            || LayerKind::TerrainConstraints(TerrainConstraintParams::default()),
            true,
            &["shape"],
            &["gradient_reconstruct"]
        );
        entry!(
            "gradient_reconstruct",
            "Gradient Reconstruct",
            WorkflowStage::Generation,
            "activity",
            "Screened-Poisson reconstruction for seamless authored edits.",
            || LayerKind::GradientReconstruct(GradientReconstructParams::default()),
            true,
            &["shape"],
            &["landscape_evolution"]
        );

        entry!(
            "flat",
            "Flat",
            WorkflowStage::Generation,
            "mountain",
            "Constant height contribution.",
            || LayerKind::Flat(FlatParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "ramp",
            "Ramp",
            WorkflowStage::Generation,
            "mountain",
            "Linear height ramp.",
            || LayerKind::Ramp(RampParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "noise_value",
            "Value Noise",
            WorkflowStage::Generation,
            "mountain",
            "Value noise height contribution.",
            || LayerKind::NoiseValue(NoiseParams::default()),
            true,
            &["noise"],
            &[]
        );
        entry!(
            "noise_perlin",
            "Perlin Noise",
            WorkflowStage::Generation,
            "mountain",
            "Perlin noise height contribution.",
            || LayerKind::NoisePerlin(NoiseParams::default()),
            true,
            &["noise"],
            &[]
        );
        entry!(
            "noise_open_simplex",
            "OpenSimplex Noise",
            WorkflowStage::Generation,
            "mountain",
            "OpenSimplex noise height contribution.",
            || LayerKind::NoiseOpenSimplex(NoiseParams::default()),
            true,
            &["noise"],
            &[]
        );
        entry!(
            "noise_worley",
            "Worley Noise",
            WorkflowStage::Generation,
            "mountain",
            "Cellular / Worley noise.",
            || LayerKind::NoiseWorley(WorleyParams::default()),
            true,
            &["noise"],
            &[]
        );
        entry!(
            "fbm",
            "FBM",
            WorkflowStage::Generation,
            "mountain",
            "Fractal Brownian motion noise.",
            || LayerKind::Fbm(FbmParams::default()),
            true,
            &["noise"],
            &["thermal_erosion"]
        );
        entry!(
            "ridged",
            "Ridged",
            WorkflowStage::Generation,
            "mountain",
            "Ridged multifractal detail.",
            || LayerKind::Ridged(FbmParams::default()),
            true,
            &["noise"],
            &[]
        );
        entry!(
            "domain_warp",
            "Domain Warp",
            WorkflowStage::Generation,
            "mountain",
            "Warped noise domain.",
            || LayerKind::DomainWarp(DomainWarpParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "mountain",
            "Mountains",
            WorkflowStage::Generation,
            "mountain",
            "Ridged mountain range.",
            || LayerKind::Mountains(MountainParams::default()),
            true,
            &["shape"],
            &["hydraulic_erosion", "thermal_erosion"]
        );
        entry!(
            "volcano",
            "Volcano",
            WorkflowStage::Generation,
            "mountain",
            "Volcanic cone with optional crater.",
            || LayerKind::Volcano(VolcanoParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "mesa",
            "Mesa",
            WorkflowStage::Generation,
            "mountain",
            "Hard-cap mesa / butte.",
            || LayerKind::Mesa(MesaParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "island",
            "Island",
            WorkflowStage::Generation,
            "waves",
            "Closed island landmass with coastal zones and bathymetry.",
            || LayerKind::Island(IslandParams::default()),
            true,
            &["shape"],
            &["coastal", "hydraulic_erosion"]
        );
        entry!(
            "uplift",
            "Uplift",
            WorkflowStage::Generation,
            "mountain",
            "Large-scale ridge / corridor uplift.",
            || LayerKind::Uplift(UpliftParams::default()),
            true,
            &["shape"],
            &["stream_power"]
        );
        entry!(
            "dunes",
            "Dunes",
            WorkflowStage::Generation,
            "mountain",
            "Wind dune fields.",
            || LayerKind::Dunes(DuneParams::default()),
            true,
            &["shape"],
            &["sand_simulation"]
        );
        entry!(
            "canyon",
            "Canyons",
            WorkflowStage::Generation,
            "mountain",
            "Canyon network.",
            || LayerKind::Canyons(CanyonParams::default()),
            true,
            &["shape"],
            &["river_carve"]
        );
        entry!(
            "plateau",
            "Plateau",
            WorkflowStage::Generation,
            "mountain",
            "Tableland plateau.",
            || LayerKind::Plateau(PlateauParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "terrace",
            "Terrace",
            WorkflowStage::Generation,
            "mountain",
            "Terraced height quantisation.",
            || LayerKind::Terrace(TerraceParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "voronoi",
            "Voronoi Regions",
            WorkflowStage::Generation,
            "mountain",
            "Cellular region heights.",
            || LayerKind::VoronoiRegions(VoronoiParams::default()),
            true,
            &["noise"],
            &[]
        );
        entry!(
            "blur",
            "Blur",
            WorkflowStage::Generation,
            "blend",
            "Gaussian-like height blur.",
            || LayerKind::Blur(BlurParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "effect_filter",
            "Effect Filter",
            WorkflowStage::Generation,
            "sparkles",
            "WC-style effect filters (smooth, distortion, shore, strata, …).",
            || LayerKind::EffectFilter(EffectFilterParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "path",
            "Path",
            WorkflowStage::Generation,
            "activity",
            "Drawn spline path (road / ridge / valley).",
            || LayerKind::Path(PathParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "procedural_shape",
            "Procedural Shape",
            WorkflowStage::Generation,
            "mountain",
            "Procedural landscape shape with a swappable generator.",
            || LayerKind::ProceduralShape(ProceduralShapeParams::default()),
            true,
            &["shape"],
            &["hydraulic_erosion", "thermal_erosion"]
        );
        entry!(
            "stamp_2d",
            "2D Stamp",
            WorkflowStage::Foundation,
            "image",
            "2D heightmap stamp.",
            || LayerKind::Stamp2d(Stamp2dParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "stamp_3d",
            "3D Stamp",
            WorkflowStage::Foundation,
            "box",
            "3D mesh stamp — samples an OBJ or height image into the heightfield.",
            || LayerKind::Stamp3d(Stamp3dParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "polygon_height",
            "Polygon",
            WorkflowStage::Generation,
            "activity",
            "Closed polygon raise / carve.",
            || LayerKind::PolygonHeight(PolygonHeightParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "overhang_stamp",
            "Overhang Stamp",
            WorkflowStage::Generation,
            "box",
            "Cliff undercut / shelf via dual-height.",
            || LayerKind::OverhangStamp(OverhangStampParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "local_sdf",
            "Local SDF",
            WorkflowStage::Generation,
            "box",
            "Local analytic SDF cave pocket.",
            || LayerKind::LocalSdf(LocalSdfParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "geomorphic_detail",
            "Geomorphic Detail",
            WorkflowStage::Generation,
            "mountain",
            "Drainage-conditioned multi-scale amplification (meso ridges / micro gullies).",
            || LayerKind::GeomorphicDetail(GeomorphicDetailParams::default()),
            true,
            &["shape", "amplify"],
            &[]
        );

        // Simulations
        entry!(
            "thermal_erosion",
            "Thermal Erosion",
            WorkflowStage::Simulation,
            "activity",
            "Talus / thermal weathering with layered debris.",
            || LayerKind::ThermalErosion(ThermalErosionParams::default()),
            true,
            &["erosion"],
            &["debris_flow"]
        );
        entry!(
            "debris_flow",
            "Debris Flow",
            WorkflowStage::Simulation,
            "activity",
            "Jain et al. 2024 debris-flow erosion, scars and deposit cones.",
            || LayerKind::DebrisFlow(crate::layer::DebrisFlowParams::default()),
            true,
            &["erosion"],
            &["thermal_erosion"]
        );
        entry!(
            "hydraulic_erosion",
            "Hydraulic Erosion",
            WorkflowStage::Simulation,
            "droplets",
            "Particle / outflow hydraulic erosion.",
            || LayerKind::HydraulicErosion(HydraulicErosionParams::default()),
            true,
            &["erosion"],
            &["debris_flow", "thermal_erosion"]
        );
        entry!(
            "stream_power",
            "Stream Power",
            WorkflowStage::Simulation,
            "waves",
            "Stream-power incision (fluvial SPE).",
            || LayerKind::StreamPowerErosion(StreamPowerParams::default()),
            true,
            &["erosion"],
            &["hydraulic_erosion"]
        );
        entry!(
            "multi_scale_amplify",
            "Multi-Scale Amplify",
            WorkflowStage::Simulation,
            "activity",
            "Wavelength-aware multi-scale amplify.",
            || LayerKind::MultiScaleAmplify(MultiScaleAmplifyParams::default()),
            true,
            &["erosion"],
            &[]
        );
        entry!(
            "river_carve",
            "River Carve",
            WorkflowStage::Simulation,
            "waves",
            "Carve river channels into height.",
            || LayerKind::RiverCarve(RiverCarveParams::default()),
            true,
            &["erosion"],
            &[]
        );
        entry!(
            "river_network",
            "River Network",
            WorkflowStage::Simulation,
            "waves",
            "Interactive river network (springs / auto-flow).",
            || LayerKind::RiverNetwork(RiverNetworkParams::default()),
            true,
            &["erosion"],
            &["river_carve"]
        );
        entry!(
            "coastal",
            "Coastal",
            WorkflowStage::Simulation,
            "waves",
            "Coastal erosion and shelf shaping.",
            || LayerKind::Coastal(CoastalParams::default()),
            true,
            &["erosion"],
            &[]
        );
        entry!(
            "landscape_evolution",
            "Landscape Evolution",
            WorkflowStage::Simulation,
            "activity",
            "Coupled uplift, SPE incision, hillslope diffusion and sediment.",
            || LayerKind::LandscapeEvolution(LandscapeEvolutionParams::default()),
            true,
            &["erosion"],
            &["hydrology_repair"]
        );
        entry!(
            "hydrology_repair",
            "Hydrology Repair",
            WorkflowStage::Simulation,
            "droplets",
            "Re-establish connected drainage after local edits.",
            || LayerKind::HydrologyRepair(HydrologyRepairParams::default()),
            true,
            &["erosion"],
            &[]
        );
        entry!(
            "ecosystem_feedback",
            "Ecosystem Feedback",
            WorkflowStage::Simulation,
            "sparkles",
            "Bounded vegetation/root/soil feedback into terrain evolution.",
            || LayerKind::EcosystemFeedback(EcosystemFeedbackParams::default()),
            true,
            &["erosion"],
            &[]
        );

        // Surface / scatter / objects
        entry!(
            "climate_biomes",
            "Climate Classification",
            WorkflowStage::BiomePlacement,
            "layers",
            "Climate / biome classification filter (not a biome container).",
            || LayerKind::Biomes(BiomesParams::default()),
            true,
            &["surface"],
            &["materials", "vegetation"]
        );
        entry!(
            "materials",
            "Materials",
            WorkflowStage::Materials,
            "blend",
            "Surface material / hardness rules.",
            || LayerKind::Materials(MaterialsParams::default()),
            true,
            &["surface"],
            &["vegetation"]
        );
        entry!(
            "vegetation",
            "Vegetation",
            WorkflowStage::Scatter,
            "sparkles",
            "Vegetation density / scatter weights.",
            || LayerKind::Vegetation(VegetationParams::default()),
            true,
            &["surface"],
            &[]
        );
        entry!(
            "sand_simulation",
            "Sand Simulation",
            WorkflowStage::Objects,
            "mountain",
            "Granular sand local simulation.",
            || LayerKind::SandSimulation(SandSimParams::default()),
            true,
            &["shape"],
            &[]
        );
        entry!(
            "fluid_simulation",
            "Fluid Simulation",
            WorkflowStage::Objects,
            "droplets",
            "Fluid / lake local simulation.",
            || LayerKind::FluidSimulation(FluidSimParams::default()),
            true,
            &["shape"],
            &[]
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_covers_unique_ids() {
        let reg = LayerTypeRegistry::builtin();
        let mut seen = std::collections::HashSet::new();
        for m in reg.all() {
            assert!(seen.insert(m.type_id), "duplicate type_id {}", m.type_id);
        }
        assert!(reg.all().len() >= 40);
    }

    #[test]
    fn create_mountain() {
        let reg = LayerTypeRegistry::builtin();
        let layer = reg.create("mountain").expect("mountain");
        assert!(matches!(layer.kind, LayerKind::Mountains(_)));
        assert_eq!(layer.kind.type_id(), "mountain");
        assert_eq!(layer.common.name, "Mountains");
    }

    #[test]
    fn sculpt_base_not_creatable() {
        let reg = LayerTypeRegistry::builtin();
        let meta = reg.get("sculpt_base").unwrap();
        assert!(!meta.capabilities.user_creatable);
        assert!(reg.create("sculpt_base").is_some()); // factory still works
    }

    #[test]
    fn stage_filter() {
        let reg = LayerTypeRegistry::builtin();
        let sims: Vec<_> = reg.by_stage(WorkflowStage::Simulation).collect();
        assert!(sims.iter().any(|m| m.type_id == "hydraulic_erosion"));
    }

    #[test]
    fn meta_matches_kind_type_id() {
        let reg = LayerTypeRegistry::builtin();
        let kind = LayerKind::Island(IslandParams::default());
        let meta = reg.meta_for_kind(&kind).unwrap();
        assert_eq!(meta.type_id, "island");
        assert_eq!(meta.workflow_stage, WorkflowStage::Generation);
    }
}
