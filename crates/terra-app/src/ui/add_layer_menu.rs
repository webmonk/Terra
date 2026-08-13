//! Add Layer categorised menu + region library helpers.
//!
//! Menu grouping follows WC artist folders (Shape Layers / Filters / Simulation / …),
//! not the internal [`OperationCategory`] Generator/Modifier split. Live UI prefers
//! [`crate::ui::tool_catalog`]; this module remains for create-by-id helpers and legacy menus.

use terra_core::layer::{
    biome_destination_section, BiomeSection, BiomesParams, BlurParams, CanyonParams, CoastalParams,
    DebrisFlowParams, DomainWarpParams, DuneParams, EcosystemFeedbackParams, EffectFilterParams,
    FbmParams, FlatParams, FluidSimParams, GeomorphicDetailParams, GradientReconstructParams,
    HydraulicErosionParams, HydrologyRepairParams, IslandParams, LandscapeEvolutionParams, Layer,
    LayerKind, LayerTypeRegistry, MaterialsParams, MesaParams, MountainParams, NoiseParams,
    OperationCategory, PathParams, PlateauParams, PolygonHeightParams, ProceduralShapeParams,
    RiverCarveParams, RiverNetworkParams, SandSimParams, SculptStrokeParams, Stamp2dParams,
    Stamp3dParams, StreamPowerParams, TerraceParams, TerrainConstraintParams,
    ThermalErosionParams, UpliftParams, VegetationParams, VolcanoParams,
};
use terra_gui::Icon;

/// One entry in the categorised Add Layer menu.
#[derive(Debug, Clone)]
pub struct AddLayerEntry {
    pub id: &'static str,
    pub label: &'static str,
    pub category: OperationCategory,
    pub icon: Icon,
    pub name: &'static str,
    pub kind: Option<LayerKind>,
    /// Organisation: pass-through or isolated group (kind is None).
    pub organisation: Option<OrganisationKind>,
    pub description: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganisationKind {
    PassThroughGroup,
    IsolatedGroup,
    Biome,
}

impl AddLayerEntry {
    /// Create a layer from this entry's explicit preset.
    ///
    /// Registry factories are reserved for create-by-type-id flows; replacing this
    /// embedded kind with a registry default would discard curated parameters.
    pub fn create_layer(&self) -> Option<Layer> {
        let kind = self.kind.as_ref()?;
        Some(create_layer_for_kind(self.name, kind))
    }
}

/// Create a layer by stable registry type id (e.g. `"mountain"`).
pub fn create_layer_by_type_id(type_id: &str) -> Option<Layer> {
    LayerTypeRegistry::builtin().create(type_id)
}

/// Create a fresh layer while preserving all parameters in an explicit kind preset.
pub fn create_layer_for_kind(name: &str, kind: &LayerKind) -> Layer {
    Layer::new(name, kind.clone())
}

/// WC-facing Add Layer groups (Shape / Filters / Sims / …).
///
/// Entries still carry internal [`OperationCategory`] for blend/routing defaults;
/// group titles use artist language, not Generator/Modifier.
pub fn add_layer_menu() -> Vec<(&'static str, Vec<AddLayerEntry>)> {
    let mut shape = Vec::new();
    let mut filters = Vec::new();
    let mut sims = Vec::new();
    let mut surface = Vec::new();
    let mut organisation = Vec::new();

    for e in all_add_layer_entries() {
        if e.organisation.is_some() || e.category == OperationCategory::Organisation {
            organisation.push(e);
            continue;
        }
        if matches!(
            e.category,
            OperationCategory::Simulation | OperationCategory::Analysis
        ) {
            sims.push(e);
            continue;
        }
        if e.category == OperationCategory::Surface {
            surface.push(e);
            continue;
        }
        match e.kind.as_ref() {
            Some(kind) => match biome_destination_section(kind) {
                None => shape.push(e),
                Some(BiomeSection::Filters) => filters.push(e),
                Some(BiomeSection::LocalSims) => sims.push(e),
                Some(BiomeSection::Materials) | Some(BiomeSection::Objects) => surface.push(e),
            },
            None => organisation.push(e),
        }
    }

    let mut out = Vec::new();
    if !shape.is_empty() {
        out.push(("Shape Layers", shape));
    }
    if !filters.is_empty() {
        out.push(("Filters", filters));
    }
    if !sims.is_empty() {
        out.push(("Simulation", sims));
    }
    if !surface.is_empty() {
        out.push(("Surface", surface));
    }
    if !organisation.is_empty() {
        out.push(("Organisation", organisation));
    }
    out
}

fn entry(
    id: &'static str,
    label: &'static str,
    category: OperationCategory,
    icon: Icon,
    name: &'static str,
    kind: LayerKind,
    description: &'static str,
) -> AddLayerEntry {
    AddLayerEntry {
        id,
        label,
        category,
        icon,
        name,
        kind: Some(kind),
        organisation: None,
        description,
    }
}

pub fn all_add_layer_entries() -> Vec<AddLayerEntry> {
    vec![
        // Shape / height sources (internal OperationCategory::Generator — WC Shape or Filter by kind)
        entry(
            "gen.island",
            "Island",
            OperationCategory::Generator,
            Icon::Waves,
            "Island",
            LayerKind::Island(IslandParams::default()),
            "Closed island landmass with coastal zones and bathymetry.",
        ),
        entry(
            "gen.noise",
            "Noise",
            OperationCategory::Generator,
            Icon::Mountain,
            "Noise",
            LayerKind::Fbm(FbmParams::default()),
            "Fractal noise height contribution.",
        ),
        entry(
            "gen.mountain",
            "Mountain Range",
            OperationCategory::Generator,
            Icon::Mountain,
            "Mountains",
            LayerKind::Mountains(MountainParams::default()),
            "Ridged mountain range.",
        ),
        entry(
            "gen.procedural_shape",
            "Procedural Shape",
            OperationCategory::Generator,
            Icon::Mountain,
            "Procedural Shape",
            LayerKind::ProceduralShape(ProceduralShapeParams::default()),
            "Procedural landscape shape with a swappable generator.",
        ),
        entry(
            "gen.stamp_2d",
            "2D Stamp",
            OperationCategory::Generator,
            Icon::Download,
            "2D Stamp",
            LayerKind::Stamp2d(Stamp2dParams::default()),
            "2D heightmap stamp.",
        ),
        entry(
            "gen.stamp_3d",
            "3D Stamp",
            OperationCategory::Generator,
            Icon::Box,
            "3D Stamp",
            LayerKind::Stamp3d(Stamp3dParams::default()),
            "3D mesh stamp (scaffold).",
        ),
        entry(
            "gen.ridge",
            "Ridge",
            OperationCategory::Generator,
            Icon::Mountain,
            "Ridged",
            LayerKind::Ridged(Default::default()),
            "Ridged multifractal detail.",
        ),
        entry(
            "gen.plateau",
            "Plateau",
            OperationCategory::Generator,
            Icon::Mountain,
            "Plateau",
            LayerKind::Plateau(PlateauParams::default()),
            "Tableland plateau.",
        ),
        entry(
            "gen.canyon",
            "Canyon",
            OperationCategory::Generator,
            Icon::Mountain,
            "Canyons",
            LayerKind::Canyons(CanyonParams::default()),
            "Canyon network.",
        ),
        entry(
            "gen.dunes",
            "Dunes",
            OperationCategory::Generator,
            Icon::Mountain,
            "Dunes",
            LayerKind::Dunes(DuneParams::default()),
            "Wind dunes (phasor seed + sand relax).",
        ),
        entry(
            "gen.volcano",
            "Volcano",
            OperationCategory::Generator,
            Icon::Mountain,
            "Volcano",
            LayerKind::Volcano(VolcanoParams::default()),
            "Volcanic cone.",
        ),
        entry(
            "gen.mesa",
            "Mesa",
            OperationCategory::Generator,
            Icon::Mountain,
            "Mesa",
            LayerKind::Mesa(MesaParams::default()),
            "Hard-cap mesa / butte.",
        ),
        entry(
            "gen.uplift",
            "Uplift",
            OperationCategory::Generator,
            Icon::Mountain,
            "Uplift",
            LayerKind::Uplift(UpliftParams::default()),
            "Large-scale ridge uplift.",
        ),
        entry(
            "gen.flat",
            "Flat",
            OperationCategory::Generator,
            Icon::Mountain,
            "Flat",
            LayerKind::Flat(FlatParams::default()),
            "Constant height.",
        ),
        entry(
            "gen.warp",
            "Domain Warp",
            OperationCategory::Generator,
            Icon::Mountain,
            "Domain Warp",
            LayerKind::DomainWarp(DomainWarpParams::default()),
            "Warped noise domain.",
        ),
        // Simulations
        entry(
            "gen.semantic_sculpt",
            "Semantic Sculpt",
            OperationCategory::Generator,
            Icon::Pencil,
            "Semantic Sculpt",
            LayerKind::SculptStrokes(SculptStrokeParams::default()),
            "Resolution-independent editable sculpt strokes.",
        ),
        entry(
            "gen.constraints",
            "Terrain Constraints",
            OperationCategory::Generator,
            Icon::Activity,
            "Terrain Constraints",
            LayerKind::TerrainConstraints(TerrainConstraintParams::default()),
            "Author elevations, ridges, valleys, rivers, coasts, divides and protected shapes.",
        ),
        entry(
            "sim.hydraulic",
            "Hydraulic Erosion",
            OperationCategory::Simulation,
            Icon::Droplets,
            "Hydraulic Erosion",
            LayerKind::HydraulicErosion(HydraulicErosionParams::default()),
            "Particle / outflow hydraulic erosion.",
        ),
        entry(
            "sim.thermal",
            "Thermal Erosion",
            OperationCategory::Simulation,
            Icon::Droplets,
            "Thermal Erosion",
            LayerKind::ThermalErosion(ThermalErosionParams::default()),
            "Talus / thermal weathering with layered debris.",
        ),
        entry(
            "sim.debris_flow",
            "Debris Flow",
            OperationCategory::Simulation,
            Icon::Activity,
            "Debris Flow",
            LayerKind::DebrisFlow(DebrisFlowParams::default()),
            "Jain et al. 2024 debris-flow scars and deposit cones.",
        ),
        entry(
            "sim.landscape_evolution",
            "Landscape Evolution",
            OperationCategory::Simulation,
            Icon::Droplets,
            "Landscape Evolution",
            LayerKind::LandscapeEvolution(LandscapeEvolutionParams::default()),
            "Coupled uplift, drainage, stream-power incision, hillslopes and sediment.",
        ),
        entry(
            "sim.hydrology_repair",
            "Hydrology Repair",
            OperationCategory::Simulation,
            Icon::Waves,
            "Hydrology Repair",
            LayerKind::HydrologyRepair(HydrologyRepairParams::default()),
            "Locally reconnect drainage after sculpting without rebuilding untouched terrain.",
        ),
        entry(
            "sim.ecosystem_feedback",
            "Ecosystem Feedback",
            OperationCategory::Simulation,
            Icon::Sparkles,
            "Ecosystem Feedback",
            LayerKind::EcosystemFeedback(EcosystemFeedbackParams::default()),
            "Feed vegetation roots, rainfall interception and sediment capture back into terrain.",
        ),
        entry(
            "sim.spe",
            "Stream Power",
            OperationCategory::Simulation,
            Icon::Droplets,
            "Stream Power",
            LayerKind::StreamPowerErosion(StreamPowerParams::default()),
            "Fluvial stream-power incision.",
        ),
        entry(
            "sim.river",
            "River Carve",
            OperationCategory::Simulation,
            Icon::Droplets,
            "River Carve",
            LayerKind::RiverCarve(RiverCarveParams::default()),
            "Carve channels from flow accumulation.",
        ),
        entry(
            "sim.river_network",
            "River Network",
            OperationCategory::Simulation,
            Icon::Droplets,
            "River Network",
            LayerKind::RiverNetwork(RiverNetworkParams::default()),
            "Spring / auto river network.",
        ),
        entry(
            "sim.sand",
            "Sand Simulation",
            OperationCategory::Simulation,
            Icon::Waves,
            "Sand Simulation",
            LayerKind::SandSimulation(SandSimParams::default()),
            "Granular sand Local Sim.",
        ),
        entry(
            "sim.fluid",
            "Fluid Simulation",
            OperationCategory::Simulation,
            Icon::Droplets,
            "Fluid Simulation",
            LayerKind::FluidSimulation(FluidSimParams::default()),
            "Fluid / lake Local Sim.",
        ),
        // Height transforms / WC Filters (internal OperationCategory::Modifier — Path/Polygon stay Shape)
        entry(
            "mod.terrace",
            "Terrace",
            OperationCategory::Modifier,
            Icon::Layers,
            "Terrace",
            LayerKind::Terrace(TerraceParams::default()),
            "Step / terrace heights.",
        ),
        entry(
            "mod.blur",
            "Blur",
            OperationCategory::Modifier,
            Icon::Layers,
            "Blur",
            LayerKind::Blur(BlurParams::default()),
            "Neighbourhood blur.",
        ),
        entry(
            "mod.gradient_reconstruct",
            "Gradient Reconstruct",
            OperationCategory::Modifier,
            Icon::Blend,
            "Gradient Reconstruct",
            LayerKind::GradientReconstruct(GradientReconstructParams::default()),
            "Screened-Poisson solve that reconciles constraints without brush seams.",
        ),
        entry(
            "mod.geomorphic_detail",
            "Geomorphic Detail",
            OperationCategory::Modifier,
            Icon::Activity,
            "Geomorphic Detail",
            LayerKind::GeomorphicDetail(GeomorphicDetailParams::default()),
            "Drainage-conditioned meso/micro amplification: nested gullies, ridge breakup, flow-aligned structure.",
        ),
        entry(
            "mod.smooth",
            "Smooth",
            OperationCategory::Modifier,
            Icon::Layers,
            "Smooth",
            LayerKind::EffectFilter(EffectFilterParams::smooth()),
            "WC smooth filter.",
        ),
        entry(
            "mod.distortion",
            "Distortion",
            OperationCategory::Modifier,
            Icon::Sparkles,
            "Distortion",
            LayerKind::EffectFilter(EffectFilterParams::distortion()),
            "Domain-warp distortion.",
        ),
        entry(
            "mod.spike",
            "Spike Removal",
            OperationCategory::Modifier,
            Icon::Maximize2,
            "Spike Removal",
            LayerKind::EffectFilter(EffectFilterParams::spike_removal()),
            "Remove height spikes.",
        ),
        entry(
            "mod.shore",
            "Shore",
            OperationCategory::Modifier,
            Icon::Waves,
            "Shore",
            LayerKind::EffectFilter(EffectFilterParams::shore()),
            "Shore terrace filter.",
        ),
        entry(
            "mod.strata",
            "Strata",
            OperationCategory::Modifier,
            Icon::Layers,
            "Strata",
            LayerKind::EffectFilter(EffectFilterParams::strata()),
            "Strata banding filter.",
        ),
        entry(
            "mod.crater",
            "Crater",
            OperationCategory::Modifier,
            Icon::CircleDot,
            "Crater",
            LayerKind::EffectFilter(EffectFilterParams::crater()),
            "Impact crater filter.",
        ),
        entry(
            "mod.denoise",
            "Denoise",
            OperationCategory::Modifier,
            Icon::Blend,
            "Denoise",
            LayerKind::EffectFilter(EffectFilterParams::denoise()),
            "Edge-aware denoise.",
        ),
        entry(
            "mod.rocky_sharp",
            "Rocky Sharp",
            OperationCategory::Modifier,
            Icon::Mountain,
            "Rocky Sharp",
            LayerKind::EffectFilter(EffectFilterParams::rocky_sharp()),
            "Steep rock detail.",
        ),
        entry(
            "mod.rocky_wide",
            "Rocky Wide",
            OperationCategory::Modifier,
            Icon::Mountain,
            "Rocky Wide",
            LayerKind::EffectFilter(EffectFilterParams::rocky_wide()),
            "Broad rocky banding.",
        ),
        entry(
            "mod.rocky_layers",
            "Rocky Layers",
            OperationCategory::Modifier,
            Icon::Layers,
            "Rocky Layers",
            LayerKind::EffectFilter(EffectFilterParams::rocky_layers()),
            "Bedded cliff layers.",
        ),
        entry(
            "mod.cliff_reinforce",
            "Cliff Reinforce",
            OperationCategory::Modifier,
            Icon::Maximize2,
            "Cliff Reinforce",
            LayerKind::EffectFilter(EffectFilterParams::cliff_reinforce()),
            "Steepen cliff walls.",
        ),
        entry(
            "mod.soft_flows",
            "Soft Flows",
            OperationCategory::Modifier,
            Icon::Waves,
            "Soft Flows",
            LayerKind::EffectFilter(EffectFilterParams::soft_flows()),
            "Broad drainage carve.",
        ),
        entry(
            "mod.thin_flows",
            "Thin Flows",
            OperationCategory::Modifier,
            Icon::Activity,
            "Thin Flows",
            LayerKind::EffectFilter(EffectFilterParams::thin_flows()),
            "Narrow gully carve.",
        ),
        entry(
            "mod.ridged_flows",
            "Ridged Flows",
            OperationCategory::Modifier,
            Icon::Activity,
            "Ridged Flows",
            LayerKind::EffectFilter(EffectFilterParams::ridged_flows()),
            "Channels + ridge boost.",
        ),
        entry(
            "mod.wide_flows",
            "Wide Flows",
            OperationCategory::Modifier,
            Icon::Waves,
            "Wide Flows",
            LayerKind::EffectFilter(EffectFilterParams::wide_flows()),
            "Valley broadening.",
        ),
        entry(
            "mod.talus_fill",
            "Talus Fill",
            OperationCategory::Modifier,
            Icon::Layers,
            "Talus Fill",
            LayerKind::EffectFilter(EffectFilterParams::talus_fill()),
            "Scree below cliffs.",
        ),
        entry(
            "mod.sediment_fill",
            "Sediment Fill Soft",
            OperationCategory::Modifier,
            Icon::Layers,
            "Sediment Fill Soft",
            LayerKind::EffectFilter(EffectFilterParams::sediment_fill_soft()),
            "Soft valley fill.",
        ),
        entry(
            "mod.mud_settle",
            "Mud Settle",
            OperationCategory::Modifier,
            Icon::Droplets,
            "Mud Settle",
            LayerKind::EffectFilter(EffectFilterParams::mud_settle()),
            "Flatten muddy sinks.",
        ),
        entry(
            "mod.hydraulic_sediment",
            "Hydraulic Sediment",
            OperationCategory::Modifier,
            Icon::Droplets,
            "Hydraulic Sediment",
            LayerKind::EffectFilter(EffectFilterParams::hydraulic_sediment()),
            "Slope-break fans.",
        ),
        entry(
            "mod.rocky_plateaus",
            "Rocky Plateaus",
            OperationCategory::Modifier,
            Icon::Mountain,
            "Rocky Plateaus",
            LayerKind::EffectFilter(EffectFilterParams::rocky_plateaus()),
            "Large arid plateau steps.",
        ),
        entry(
            "mod.rocky_cliffs",
            "Rocky Cliffs",
            OperationCategory::Modifier,
            Icon::Mountain,
            "Rocky Cliffs",
            LayerKind::EffectFilter(EffectFilterParams::rocky_cliffs()),
            "Steep rocky cliff faces.",
        ),
        entry(
            "mod.rocky_hard",
            "Rocky Hard",
            OperationCategory::Modifier,
            Icon::Mountain,
            "Rocky Hard",
            LayerKind::EffectFilter(EffectFilterParams::rocky_hard()),
            "Hard fractured rock detail.",
        ),
        entry(
            "mod.canyon_filter",
            "Canyon",
            OperationCategory::Modifier,
            Icon::Activity,
            "Canyon",
            LayerKind::EffectFilter(EffectFilterParams::canyon()),
            "Deep flow canyon carve.",
        ),
        entry(
            "mod.chipped",
            "Chipped",
            OperationCategory::Modifier,
            Icon::Sparkles,
            "Chipped",
            LayerKind::EffectFilter(EffectFilterParams::chipped()),
            "High-freq cliff chips.",
        ),
        entry(
            "mod.cliffs",
            "Cliffs",
            OperationCategory::Modifier,
            Icon::Maximize2,
            "Cliffs",
            LayerKind::EffectFilter(EffectFilterParams::cliffs()),
            "Cliff wall reinforce.",
        ),
        entry(
            "mod.rocky",
            "Rocky",
            OperationCategory::Modifier,
            Icon::Mountain,
            "Rocky",
            LayerKind::EffectFilter(EffectFilterParams::rocky()),
            "Generic rocky flow carve.",
        ),
        entry(
            "mod.sediment_flows",
            "Sediment Flows",
            OperationCategory::Modifier,
            Icon::Droplets,
            "Sediment Flows",
            LayerKind::EffectFilter(EffectFilterParams::sediment_flows()),
            "Flow-follow deposition.",
        ),
        entry(
            "mod.angle_break",
            "Angle Break",
            OperationCategory::Modifier,
            Icon::Activity,
            "Angle Break",
            LayerKind::EffectFilter(EffectFilterParams::angle_break()),
            "Break slope continuity.",
        ),
        entry(
            "mod.wind_carve",
            "Wind Carve",
            OperationCategory::Modifier,
            Icon::Waves,
            "Wind Carve",
            LayerKind::EffectFilter(EffectFilterParams::wind_carve()),
            "Directional wind smear erosion.",
        ),
        entry(
            "mod.inflate",
            "Inflate",
            OperationCategory::Modifier,
            Icon::Maximize2,
            "Inflate",
            LayerKind::EffectFilter(EffectFilterParams::inflate()),
            "Local max inflate.",
        ),
        entry(
            "mod.deflate",
            "Deflate",
            OperationCategory::Modifier,
            Icon::Blend,
            "Deflate",
            LayerKind::EffectFilter(EffectFilterParams::deflate()),
            "Local min deflate.",
        ),
        entry(
            "mod.balloon",
            "Balloon",
            OperationCategory::Modifier,
            Icon::CircleDot,
            "Balloon",
            LayerKind::EffectFilter(EffectFilterParams::balloon()),
            "Slope-gated balloon inflate.",
        ),
        entry(
            "mod.blocks",
            "Blocks",
            OperationCategory::Modifier,
            Icon::Box,
            "Blocks",
            LayerKind::EffectFilter(EffectFilterParams::blocks()),
            "Blocky height quantize.",
        ),
        entry(
            "mod.ridged_filter",
            "Ridged",
            OperationCategory::Modifier,
            Icon::Activity,
            "Ridged",
            LayerKind::EffectFilter(EffectFilterParams::ridged()),
            "Ridged noise detail.",
        ),
        entry(
            "mod.rugged",
            "Rugged",
            OperationCategory::Modifier,
            Icon::Mountain,
            "Rugged",
            LayerKind::EffectFilter(EffectFilterParams::rugged()),
            "Rugged multi-octave detail.",
        ),
        entry(
            "mod.smooth_ridges",
            "Smooth Ridges",
            OperationCategory::Modifier,
            Icon::Blend,
            "Smooth Ridges",
            LayerKind::EffectFilter(EffectFilterParams::smooth_ridges()),
            "Softened ridged detail.",
        ),
        entry(
            "mod.angle_blur",
            "Angle Blur",
            OperationCategory::Modifier,
            Icon::Blend,
            "Angle Blur",
            LayerKind::EffectFilter(EffectFilterParams::angle_blur()),
            "Anisotropic slope blur.",
        ),
        entry(
            "mod.directional_blur",
            "Directional Blur",
            OperationCategory::Modifier,
            Icon::Move,
            "Directional Blur",
            LayerKind::EffectFilter(EffectFilterParams::directional_blur()),
            "Fixed-direction anisotropic blur.",
        ),
        entry(
            "mod.squeeze",
            "Squeeze",
            OperationCategory::Modifier,
            Icon::Move,
            "Squeeze",
            LayerKind::EffectFilter(EffectFilterParams::squeeze()),
            "Nonlinear height-range remapping.",
        ),
        entry(
            "mod.swirl",
            "Swirl",
            OperationCategory::Modifier,
            Icon::RotateCcw,
            "Swirl",
            LayerKind::EffectFilter(EffectFilterParams::swirl()),
            "Swirl domain warp.",
        ),
        entry(
            "mod.washed_off",
            "Washed Off",
            OperationCategory::Modifier,
            Icon::Waves,
            "Washed Off",
            LayerKind::EffectFilter(EffectFilterParams::washed_off()),
            "Wash down steep slopes.",
        ),
        entry(
            "mod.hexagons",
            "Hexagons",
            OperationCategory::Modifier,
            Icon::Grid3x3,
            "Hexagons",
            LayerKind::EffectFilter(EffectFilterParams::hexagons()),
            "Hex cell height snap.",
        ),
        entry(
            "mod.scatter_detail",
            "Scatter",
            OperationCategory::Modifier,
            Icon::Sparkles,
            "Scatter",
            LayerKind::EffectFilter(EffectFilterParams::scatter_detail()),
            "Sparse rock spike scatter.",
        ),
        entry(
            "mod.flatten_filter",
            "Flatten",
            OperationCategory::Modifier,
            Icon::Box,
            "Flatten Filter",
            LayerKind::EffectFilter(EffectFilterParams::flatten_filter()),
            "Pull toward local mean/median.",
        ),
        entry(
            "mod.zero_edge",
            "Zero-Edge",
            OperationCategory::Modifier,
            Icon::Maximize2,
            "Zero-Edge",
            LayerKind::EffectFilter(EffectFilterParams::zero_edge()),
            "Fade edges to min height.",
        ),
        entry(
            "mod.border_blend",
            "Border Blend",
            OperationCategory::Modifier,
            Icon::Blend,
            "Border Blend",
            LayerKind::EffectFilter(EffectFilterParams::border_blend()),
            "Blend rim toward border average.",
        ),
        entry(
            "mod.curve",
            "Curve",
            OperationCategory::Modifier,
            Icon::Activity,
            "Curve",
            LayerKind::EffectFilter(EffectFilterParams::curve()),
            "Contrast remapping of height midtones.",
        ),
        entry(
            "mod.cutoff",
            "Cutoff",
            OperationCategory::Modifier,
            Icon::Maximize2,
            "Cutoff",
            LayerKind::EffectFilter(EffectFilterParams::cutoff()),
            "Hard shelf / clamp below a height level.",
        ),
        entry(
            "mod.kuwahara",
            "Kuwahara",
            OperationCategory::Modifier,
            Icon::Sparkles,
            "Kuwahara",
            LayerKind::EffectFilter(EffectFilterParams::kuwahara()),
            "Edge-preserving Kuwahara smoothing.",
        ),
        entry(
            "mod.terrace_simple",
            "Terrace Simple",
            OperationCategory::Modifier,
            Icon::Layers,
            "Terrace Simple",
            LayerKind::EffectFilter(EffectFilterParams::terrace_simple()),
            "Uniform height terraces.",
        ),
        entry(
            "mod.terrace_irregular",
            "Terrace Irregular",
            OperationCategory::Modifier,
            Icon::Layers,
            "Terrace Irregular",
            LayerKind::EffectFilter(EffectFilterParams::terrace_irregular()),
            "Jittered terrace steps.",
        ),
        entry(
            "mod.terrace_steep",
            "Terrace Steep",
            OperationCategory::Modifier,
            Icon::Layers,
            "Terrace Steep",
            LayerKind::EffectFilter(EffectFilterParams::terrace_steep()),
            "Sharp-riser terraces.",
        ),
        entry(
            "mod.path",
            "Path",
            OperationCategory::Modifier,
            Icon::Activity,
            "Path",
            LayerKind::Path(PathParams::default()),
            "Spline path stamp.",
        ),
        entry(
            "mod.polygon_height",
            "Polygon",
            OperationCategory::Modifier,
            Icon::Activity,
            "Polygon",
            LayerKind::PolygonHeight(PolygonHeightParams::default()),
            "Closed polygon raise / carve.",
        ),
        entry(
            "mod.coastal",
            "Coastal Flatten",
            OperationCategory::Modifier,
            Icon::Layers,
            "Coastal",
            LayerKind::Coastal(CoastalParams::default()),
            "Sea level / beach shelf.",
        ),
        entry(
            "mod.noise_detail",
            "Value Noise",
            OperationCategory::Modifier,
            Icon::Layers,
            "Detail",
            LayerKind::NoiseValue(NoiseParams::default()),
            "Additive surface noise (use as detail).",
        ),
        // Analysis (flow via river / SPE for now)
        entry(
            "anl.flow",
            "Flow Analysis",
            OperationCategory::Analysis,
            Icon::CircleDot,
            "Flow Analysis",
            LayerKind::RiverCarve(RiverCarveParams {
                // Analysis-leaning defaults: shallow carve
                ..RiverCarveParams::default()
            }),
            "Flow routing / river analysis (publishes flow fields).",
        ),
        // Surface
        entry(
            "srf.biomes",
            "Climate Classification",
            OperationCategory::Surface,
            Icon::Layers,
            "Climate Classification",
            LayerKind::Biomes(BiomesParams::default()),
            "Climate-driven classification LUT (not a biome container).",
        ),
        entry(
            "srf.materials",
            "Materials",
            OperationCategory::Surface,
            Icon::Layers,
            "Materials",
            LayerKind::Materials(MaterialsParams::default()),
            "Surface materials and hardness strata.",
        ),
        entry(
            "srf.vegetation",
            "Vegetation",
            OperationCategory::Surface,
            Icon::Sparkles,
            "Vegetation",
            LayerKind::Vegetation(VegetationParams::default()),
            "Vegetation density / scatter suitability.",
        ),
        // Organisation
        AddLayerEntry {
            id: "org.pass",
            label: "Pass-through Group",
            category: OperationCategory::Organisation,
            icon: Icon::Layers,
            name: "Group",
            kind: None,
            organisation: Some(OrganisationKind::PassThroughGroup),
            description: "Organise layers; children modify the live context.",
        },
        AddLayerEntry {
            id: "org.isolated",
            label: "Isolated Terrain Group",
            category: OperationCategory::Organisation,
            icon: Icon::Layers,
            name: "Terrain Group",
            kind: None,
            organisation: Some(OrganisationKind::IsolatedGroup),
            description: "Private recipe composited back with a group mask.",
        },
        AddLayerEntry {
            id: "org.biome",
            label: "Biome",
            category: OperationCategory::Organisation,
            icon: Icon::Layers,
            name: "Biome",
            kind: None,
            organisation: Some(OrganisationKind::Biome),
            description: "WC biome container: Filters, Materials, Objects, Local Sims.",
        },
        AddLayerEntry {
            id: "org.hole",
            label: "Hole Layer",
            category: OperationCategory::Organisation,
            icon: Icon::CircleDot,
            name: "Hole",
            kind: None,
            organisation: None,
            description: "Painted pierce mask for caves / cutouts.",
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use terra_core::layer::{is_shape_kind, EffectFilterKind};

    #[test]
    fn explicit_kind_creation_preserves_curated_parameters() {
        let preset = LayerKind::EffectFilter(EffectFilterParams::rocky_plateaus());
        let layer = create_layer_for_kind("Curated Plateaus", &preset);

        assert_eq!(layer.common.name, "Curated Plateaus");
        let LayerKind::EffectFilter(params) = layer.kind else {
            panic!("expected an effect-filter layer");
        };
        assert_eq!(params.kind, EffectFilterKind::RockyPlateaus);
        assert_ne!(params.kind, EffectFilterKind::Smooth);
    }

    #[test]
    fn add_layer_menu_uses_wc_group_titles_not_generator_modifier() {
        let menu = add_layer_menu();
        let titles: Vec<_> = menu.iter().map(|(t, _)| *t).collect();
        assert!(titles.contains(&"Shape Layers"));
        assert!(titles.contains(&"Filters"));
        assert!(!titles.iter().any(|t| t.contains("Generator") || t.contains("Modifier")));
    }

    #[test]
    fn shape_menu_excludes_terrace_filter_includes_path() {
        let menu = add_layer_menu();
        let shape = menu
            .iter()
            .find(|(t, _)| *t == "Shape Layers")
            .map(|(_, e)| e)
            .expect("Shape Layers group");
        assert!(shape.iter().any(|e| e.id == "mod.path" || e.label == "Path"));
        assert!(!shape.iter().any(|e| e.id == "mod.terrace"));
        let filters = menu
            .iter()
            .find(|(t, _)| *t == "Filters")
            .map(|(_, e)| e)
            .expect("Filters group");
        assert!(filters.iter().any(|e| e.id == "mod.terrace"));
        assert!(shape.iter().any(|e| {
            e.kind
                .as_ref()
                .is_some_and(is_shape_kind)
        }));
    }
}
