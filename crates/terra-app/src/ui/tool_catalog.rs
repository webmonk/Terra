//! Tool catalog — maps workflow modes to sculpt tools and layer kinds.
//!
//! Layer-type metadata (names, icons, creatable flags) should prefer
//! [`terra_core::layer::LayerTypeRegistry`] as the source of truth. This catalog
//! still owns sculpt/mask/brush actions that are not layer types; when adding a
//! new `LayerKind`, register it in the core registry first, then expose it here.

use crate::ui::workspace::WorkspaceMode;
use crate::ui::EditorTool;
use terra_core::layer::{
    BedGeometry, BiomesParams, CoastalParams, DebrisFlowParams, DuneParams, EffectFilterParams,
    FluidSimParams, GeomorphicDetailParams, HydraulicErosionParams, HydrologyRepairParams,
    ImportHeightmapParams, LandscapeEvolutionParams, LayerKind, MaterialRule, MaterialsParams,
    MultiScaleAmplifyParams, RiverCarveParams, SandSimParams, ThermalErosionParams,
    VegetationParams,
};
use terra_core::mask::{DistNode, DistNodeKind, Distribution, MaskId, MaskSource};
use terra_gui::Icon;

use std::sync::OnceLock;
/// What activating a catalog entry does.
#[derive(Debug, Clone)]
pub enum ToolAction {
    /// Select a sculpt / navigation / paint editor tool.
    Sculpt(EditorTool),
    /// Select a biome paint brush variant (implies PaintBiome editor tool).
    BiomeBrush(terra_core::biome_paint::BiomePaintTool),
    /// Drag or click to add a layer of this kind.
    AddLayer { name: &'static str, kind: LayerKind },
    /// Create a WC biome container (`GroupKind::Biome`), not climate LUT.
    CreateBiome { name: &'static str },
    /// Click to add a project mask asset with the given source.
    AddMask {
        name: &'static str,
        source: MaskSource,
    },
    /// Mark the selected layer as baked / cached.
    BakeSelected,
}

#[derive(Debug, Clone)]
pub struct ToolDef {
    pub id: &'static str,
    pub label: &'static str,
    pub icon: Icon,
    pub mode: WorkspaceMode,
    pub action: ToolAction,
    pub description: &'static str,
    pub shortcut: Option<&'static str>,
}

impl ToolDef {
    pub fn is_sculpt(&self) -> bool {
        matches!(self.action, ToolAction::Sculpt(_))
    }
}

fn add(
    id: &'static str,
    label: &'static str,
    icon: Icon,
    mode: WorkspaceMode,
    name: &'static str,
    kind: LayerKind,
    description: &'static str,
) -> ToolDef {
    ToolDef {
        id,
        label,
        icon,
        mode,
        action: ToolAction::AddLayer { name, kind },
        description,
        shortcut: None,
    }
}

fn sculpt(
    id: &'static str,
    label: &'static str,
    icon: Icon,
    mode: WorkspaceMode,
    tool: EditorTool,
    description: &'static str,
    shortcut: Option<&'static str>,
) -> ToolDef {
    ToolDef {
        id,
        label,
        icon,
        mode,
        action: ToolAction::Sculpt(tool),
        description,
        shortcut,
    }
}

fn biome_brush(
    id: &'static str,
    label: &'static str,
    icon: Icon,
    brush: terra_core::biome_paint::BiomePaintTool,
    description: &'static str,
    shortcut: Option<&'static str>,
) -> ToolDef {
    ToolDef {
        id,
        label,
        icon,
        mode: WorkspaceMode::Biomes,
        action: ToolAction::BiomeBrush(brush),
        description,
        shortcut,
    }
}

fn add_mask(
    id: &'static str,
    label: &'static str,
    icon: Icon,
    name: &'static str,
    source: MaskSource,
    description: &'static str,
) -> ToolDef {
    ToolDef {
        id,
        label,
        icon,
        mode: WorkspaceMode::Masks,
        action: ToolAction::AddMask { name, source },
        description,
        shortcut: None,
    }
}

fn material_rule(
    name: &str,
    id: u32,
    min_slope: f32,
    max_slope: f32,
    min_height: f32,
    max_height: f32,
    mask: MaskSource,
    hardness: f32,
    tint: [f32; 3],
    roughness: f32,
) -> MaterialRule {
    MaterialRule {
        name: name.into(),
        id,
        min_slope_deg: min_slope,
        max_slope_deg: max_slope,
        min_height,
        max_height,
        mask,
        hardness,
        tint,
        roughness,
        metalness: 0.0,
        albedo_path: None,
    }
}

fn materials_single(rule: MaterialRule) -> LayerKind {
    LayerKind::Materials(MaterialsParams {
        rules: vec![rule],
        strata: Vec::new(),
        default_hardness: 0.5,
        coverage: Distribution::new(),
        bed_geometry: BedGeometry::Horizontal,
    })
}

fn veg_with_coverage(mut params: VegetationParams, nodes: Vec<DistNode>) -> VegetationParams {
    params.coverage = Distribution::from_nodes(nodes);
    params
}

/// Full catalog for all modes. One canonical home per wired capability.
pub fn all_tools() -> Vec<ToolDef> {
    let mut tools = Vec::new();

    // —— Terrain: Shape Layer types (WC-style) ——————————————————————
    tools.push(add(
        "shape.sculpt",
        "Sculpt",
        Icon::Pencil,
        WorkspaceMode::Terrain,
        "Sculpt Layer",
        LayerKind::SculptStrokes(terra_core::layer::SculptStrokeParams::default()),
        "Editable sculpt strokes — raise, lower, smooth, stamps.",
    ));
    tools.push(add(
        "shape.stamp_2d",
        "2D Stamp",
        Icon::Image,
        WorkspaceMode::Terrain,
        "2D Stamp",
        LayerKind::Stamp2d(terra_core::layer::Stamp2dParams::default()),
        "Stamp a heightmap into the terrain with blend & transform.",
    ));
    tools.push(add(
        "shape.stamp_3d",
        "3D Stamp",
        Icon::Box,
        WorkspaceMode::Terrain,
        "3D Stamp",
        LayerKind::Stamp3d(terra_core::layer::Stamp3dParams::default()),
        "Stamp a mesh (OBJ) or height image — procedural rock when empty.",
    ));
    tools.push(add(
        "shape.procedural",
        "Procedural",
        Icon::Mountain,
        WorkspaceMode::Terrain,
        "Procedural Shape",
        LayerKind::ProceduralShape(terra_core::layer::ProceduralShapeParams::default()),
        "Procedural landform — pick Mountain, Hills, Mesa, Noise, …",
    ));
    tools.push(add(
        "shape.polygon",
        "Polygon",
        Icon::Layers,
        WorkspaceMode::Terrain,
        "Polygon",
        LayerKind::PolygonHeight(terra_core::layer::PolygonHeightParams::default()),
        "Raise or carve a closed polygon region.",
    ));
    tools.push(add(
        "shape.path",
        "Path",
        Icon::Activity,
        WorkspaceMode::Terrain,
        "Path",
        LayerKind::Path(terra_core::layer::PathParams::default()),
        "Spline path for ridges, valleys, or roads.",
    ));
    tools.push(add(
        "shape.heightmap",
        "Heightmap",
        Icon::Image,
        WorkspaceMode::Terrain,
        "Heightmap",
        LayerKind::ImportHeightmap(ImportHeightmapParams::default()),
        "Import a heightmap as a shape layer (blend & reorder).",
    ));

    // —— Terrain: height brushes ————————————————————————————————————
    tools.push(sculpt(
        "sculpt.raise",
        "Raise",
        Icon::ArrowUp,
        WorkspaceMode::Terrain,
        EditorTool::Raise,
        "Raises terrain beneath the brush. Hold Ctrl to lower. Hold Shift to smooth.",
        Some("R"),
    ));
    tools.push(sculpt(
        "sculpt.lower",
        "Lower",
        Icon::ArrowDown,
        WorkspaceMode::Terrain,
        EditorTool::Lower,
        "Lowers terrain beneath the brush. Hold Ctrl to raise. Hold Shift to smooth.",
        Some("L"),
    ));
    tools.push(sculpt(
        "sculpt.smooth",
        "Smooth",
        Icon::Blend,
        WorkspaceMode::Terrain,
        EditorTool::Smooth,
        "Smooths terrain beneath the brush — writes a Shape Layer stroke.",
        Some("S"),
    ));
    tools.push(sculpt(
        "sculpt.flatten",
        "Flatten",
        Icon::Blend,
        WorkspaceMode::Terrain,
        EditorTool::Flatten,
        "Flatten toward a target height on a Shape Layer.",
        None,
    ));
    tools.push(sculpt(
        "sculpt.terrace",
        "Terrace",
        Icon::Layers,
        WorkspaceMode::Terrain,
        EditorTool::Terrace,
        "Quantize heights into terraces under the brush.",
        None,
    ));
    tools.push(sculpt(
        "sculpt.pinch",
        "Pinch",
        Icon::Maximize2,
        WorkspaceMode::Terrain,
        EditorTool::Pinch,
        "Pinch / contract detail toward the brush centre.",
        None,
    ));
    tools.push(sculpt(
        "sculpt.inflate",
        "Inflate",
        Icon::CircleDot,
        WorkspaceMode::Terrain,
        EditorTool::Inflate,
        "Inflate / bulge terrain under the brush.",
        None,
    ));
    tools.push(sculpt(
        "sculpt.erode_brush",
        "Erode Brush",
        Icon::Activity,
        WorkspaceMode::Terrain,
        EditorTool::ErodeBrush,
        "Soft erode under the brush (Shape history).",
        None,
    ));
    tools.push(sculpt(
        "sculpt.mountain_stamp",
        "Mountain Stamp",
        Icon::Mountain,
        WorkspaceMode::Terrain,
        EditorTool::MountainStamp,
        "Stamp a mountain mass into a Shape Layer.",
        None,
    ));
    tools.push(sculpt(
        "sculpt.valley_stamp",
        "Valley Stamp",
        Icon::Droplets,
        WorkspaceMode::Terrain,
        EditorTool::ValleyStamp,
        "Stamp a valley into a Shape Layer.",
        None,
    ));
    tools.push(sculpt(
        "sculpt.plateau_stamp",
        "Plateau Stamp",
        Icon::Box,
        WorkspaceMode::Terrain,
        EditorTool::PlateauStamp,
        "Stamp a plateau disk into a Shape Layer.",
        None,
    ));
    tools.push(sculpt(
        "sculpt.crater_stamp",
        "Crater Stamp",
        Icon::CircleDot,
        WorkspaceMode::Terrain,
        EditorTool::CraterStamp,
        "Stamp a crater bowl + rim.",
        None,
    ));
    tools.push(sculpt(
        "sculpt.coastline",
        "Coastline Tool",
        Icon::Waves,
        WorkspaceMode::Terrain,
        EditorTool::CoastlineTool,
        "Coastal smoothing / lower along the brush path.",
        None,
    ));
    tools.push(sculpt(
        "sculpt.river_path",
        "River Path",
        Icon::Droplets,
        WorkspaceMode::Terrain,
        EditorTool::RiverPathTool,
        "Carve a river valley path into a Shape Layer.",
        None,
    ));
    tools.push(sculpt(
        "sculpt.height_stamp",
        "Height Stamp",
        Icon::ArrowUp,
        WorkspaceMode::Terrain,
        EditorTool::HeightStamp,
        "Stamp toward an absolute height on a Shape Layer.",
        None,
    ));
    tools.push(sculpt(
        "sculpt.noise_brush",
        "Noise Brush",
        Icon::Sparkles,
        WorkspaceMode::Terrain,
        EditorTool::NoiseBrush,
        "Paint procedural noise detail onto a Shape Layer.",
        None,
    ));

    // —— Biomes ————————————————————————————————————————————————————
    tools.push(biome_brush(
        "biome.paint",
        "Paint Biome",
        Icon::Paintbrush,
        terra_core::biome_paint::BiomePaintTool::Paint,
        "Paint biome ownership weights.",
        Some("B"),
    ));
    tools.push(biome_brush(
        "biome.erase",
        "Erase Biome",
        Icon::X,
        terra_core::biome_paint::BiomePaintTool::Erase,
        "Erase biome ownership under the brush.",
        None,
    ));
    tools.push(biome_brush(
        "biome.smooth",
        "Smooth Weights",
        Icon::Blend,
        terra_core::biome_paint::BiomePaintTool::Smooth,
        "Smooth biome weight boundaries.",
        None,
    ));
    tools.push(biome_brush(
        "biome.replace",
        "Replace Biome",
        Icon::Copy,
        terra_core::biome_paint::BiomePaintTool::Replace,
        "Replace one biome with another under the brush.",
        None,
    ));
    tools.push(biome_brush(
        "biome.normalize",
        "Normalize Weights",
        Icon::SlidersHorizontal,
        terra_core::biome_paint::BiomePaintTool::Normalize,
        "Normalize biome weights under the brush.",
        None,
    ));
    tools.push(biome_brush(
        "biome.flood",
        "Flood Fill",
        Icon::Droplets,
        terra_core::biome_paint::BiomePaintTool::FloodFill,
        "Flood-fill biome ownership from a seed.",
        None,
    ));
    tools.push(biome_brush(
        "biome.polygon",
        "Polygon Fill",
        Icon::Layers,
        terra_core::biome_paint::BiomePaintTool::PolygonFill,
        "Fill a polygon with biome ownership.",
        None,
    ));
    tools.push(ToolDef {
        id: "biome.create",
        label: "Create Biome",
        icon: Icon::Plus,
        mode: WorkspaceMode::Biomes,
        action: ToolAction::CreateBiome { name: "Biome" },
        description: "Add a biome container under Biomes (Filters / Materials / Objects).",
        shortcut: None,
    });
    tools.push(add(
        "biome.climate",
        "Climate Classification",
        Icon::Sparkles,
        WorkspaceMode::Biomes,
        "Climate Classification",
        LayerKind::Biomes(BiomesParams::default()),
        "Climate-driven classification LUT (not a biome container).",
    ));

    // —— Simulation: processes —————————————————————————————————————
    tools.push(add(
        "sim.landscape_evolution",
        "Landscape Evolution",
        Icon::Mountain,
        WorkspaceMode::Simulation,
        "Landscape Evolution",
        LayerKind::LandscapeEvolution(LandscapeEvolutionParams::default()),
        "Coupled uplift, stream-power incision, hillslope diffusion, and sediment transport.",
    ));
    tools.push(add(
        "sim.multi_scale_amplify",
        "Multi-Scale Amplify",
        Icon::Sparkles,
        WorkspaceMode::Simulation,
        "Multi-Scale Amplify",
        LayerKind::MultiScaleAmplify(MultiScaleAmplifyParams::default()),
        "Add hydrologically aligned detail across scales while preserving ridges.",
    ));
    tools.push(add(
        "sim.hydrology_repair",
        "Hydrology Repair",
        Icon::Droplets,
        WorkspaceMode::Simulation,
        "Hydrology Repair",
        LayerKind::HydrologyRepair(HydrologyRepairParams::default()),
        "Reconnect drainage and remove local pits with protected-feature retargeting.",
    ));

    tools.push(add(
        "sim.hydraulic",
        "Hydraulic",
        Icon::Droplets,
        WorkspaceMode::Simulation,
        "Hydraulic Erosion",
        LayerKind::HydraulicErosion(HydraulicErosionParams::default()),
        "Simulate rainfall and sediment transport.",
    ));
    tools.push(add(
        "sim.thermal",
        "Thermal",
        Icon::Mountain,
        WorkspaceMode::Simulation,
        "Thermal Erosion",
        LayerKind::ThermalErosion(ThermalErosionParams::default()),
        "Talus weathering with layered bedrock / debris.",
    ));
    tools.push(add(
        "sim.debris_flow",
        "Debris Flow",
        Icon::Activity,
        WorkspaceMode::Simulation,
        "Debris Flow",
        LayerKind::DebrisFlow(DebrisFlowParams::default()),
        "Steep-slope debris-flow scars, cones, and fluvial coupling.",
    ));
    tools.push(add(
        "sim.wind",
        "Wind",
        Icon::Waves,
        WorkspaceMode::Simulation,
        "Sand Simulation",
        LayerKind::SandSimulation(SandSimParams::default()),
        "Aeolian sand transport — saltation, sheltering, avalanching (progressive).",
    ));
    tools.push(add(
        "sim.sediment",
        "Sediment",
        Icon::Layers,
        WorkspaceMode::Simulation,
        "Sediment Fill",
        LayerKind::EffectFilter(EffectFilterParams::sediment_fill_soft()),
        "Soft valley sediment fill (slope / curvature / flow).",
    ));
    tools.push(add(
        "sim.talus",
        "Talus",
        Icon::Activity,
        WorkspaceMode::Simulation,
        "Talus Fill",
        LayerKind::EffectFilter(EffectFilterParams::talus_fill()),
        "Scree / talus aprons below cliffs.",
    ));
    tools.push(add(
        "sim.mud",
        "Mud",
        Icon::Layers,
        WorkspaceMode::Simulation,
        "Mud Settle",
        LayerKind::EffectFilter(EffectFilterParams::mud_settle()),
        "Cohesive mud fill in low-slope drainage.",
    ));
    tools.push(add(
        "sim.weathering",
        "Weathering",
        Icon::Blend,
        WorkspaceMode::Simulation,
        "Weathering",
        LayerKind::ThermalErosion(ThermalErosionParams {
            weathering_rate: 1.2,
            material_amount: 6.0,
            transport_distance: 2.0,
            strength: 0.4,
            iterations: 24,
            ..ThermalErosionParams::default()
        }),
        "Bedrock weathering into loose debris (thermal).",
    ));

    // —— Simulation: hydrology —————————————————————————————————————
    tools.push(add(
        "sim.river",
        "River",
        Icon::Waves,
        WorkspaceMode::Simulation,
        "River Carve",
        LayerKind::RiverCarve(RiverCarveParams::default()),
        "Carve a river channel.",
    ));
    tools.push(add(
        "sim.coastal",
        "Coastal",
        Icon::Waves,
        WorkspaceMode::Simulation,
        "Coast",
        LayerKind::Coastal(CoastalParams::default()),
        "Beach shelf and coastal flatten.",
    ));
    tools.push(add(
        "sim.lake",
        "Lake",
        Icon::Droplets,
        WorkspaceMode::Simulation,
        "Lake",
        LayerKind::FluidSimulation(FluidSimParams::default()),
        "Fluid / lake local simulation.",
    ));
    tools.push(add(
        "sim.waterfall",
        "Waterfall",
        Icon::Droplets,
        WorkspaceMode::Simulation,
        "Waterfall",
        LayerKind::EffectFilter(EffectFilterParams::thin_flows()),
        "Narrow channel / waterfall gully carve.",
    ));
    tools.push(add(
        "sim.rocky_plateaus",
        "Rocky Plateaus",
        Icon::Mountain,
        WorkspaceMode::Simulation,
        "Rocky Plateaus",
        LayerKind::EffectFilter(EffectFilterParams::rocky_plateaus()),
        "Arid plateau steps with cliff walls.",
    ));
    tools.push(add(
        "sim.rocky_cliffs",
        "Rocky Cliffs",
        Icon::Mountain,
        WorkspaceMode::Simulation,
        "Rocky Cliffs",
        LayerKind::EffectFilter(EffectFilterParams::rocky_cliffs()),
        "Steep rocky cliff morphology.",
    ));
    tools.push(add(
        "sim.canyon_filter",
        "Canyon",
        Icon::Activity,
        WorkspaceMode::Simulation,
        "Canyon Filter",
        LayerKind::EffectFilter(EffectFilterParams::canyon()),
        "Deep flow-following canyon carve.",
    ));
    tools.push(add(
        "sim.sediment_flows",
        "Sediment Flows",
        Icon::Droplets,
        WorkspaceMode::Simulation,
        "Sediment Flows",
        LayerKind::EffectFilter(EffectFilterParams::sediment_flows()),
        "Flow-follow sediment deposition.",
    ));
    tools.push(add(
        "sim.wind_carve",
        "Wind Carve",
        Icon::Waves,
        WorkspaceMode::Simulation,
        "Wind Carve",
        LayerKind::EffectFilter(EffectFilterParams::wind_carve()),
        "Directional wind smear erosion.",
    ));
    tools.push(add(
        "sim.inflate",
        "Inflate",
        Icon::Maximize2,
        WorkspaceMode::Simulation,
        "Inflate",
        LayerKind::EffectFilter(EffectFilterParams::inflate()),
        "Local-max inflate morphology.",
    ));
    tools.push(add(
        "sim.deflate",
        "Deflate",
        Icon::Blend,
        WorkspaceMode::Simulation,
        "Deflate",
        LayerKind::EffectFilter(EffectFilterParams::deflate()),
        "Local-min deflate morphology.",
    ));
    tools.push(add(
        "sim.blocks",
        "Blocks",
        Icon::Box,
        WorkspaceMode::Simulation,
        "Blocks",
        LayerKind::EffectFilter(EffectFilterParams::blocks()),
        "Quantize heights into blocks.",
    ));
    tools.push(add(
        "sim.swirl",
        "Swirl",
        Icon::RotateCcw,
        WorkspaceMode::Simulation,
        "Swirl",
        LayerKind::EffectFilter(EffectFilterParams::swirl()),
        "Swirl domain warp filter.",
    ));
    tools.push(add(
        "sim.hexagons",
        "Hexagons",
        Icon::Grid3x3,
        WorkspaceMode::Simulation,
        "Hexagons",
        LayerKind::EffectFilter(EffectFilterParams::hexagons()),
        "Hexagonal cell height snap.",
    ));
    tools.push(add(
        "sim.flatten_filter",
        "Flatten Filter",
        Icon::Box,
        WorkspaceMode::Simulation,
        "Flatten Filter",
        LayerKind::EffectFilter(EffectFilterParams::flatten_filter()),
        "Pull heights toward local mean/median.",
    ));
    tools.push(add(
        "sim.curve",
        "Curve",
        Icon::Activity,
        WorkspaceMode::Simulation,
        "Curve",
        LayerKind::EffectFilter(EffectFilterParams::curve()),
        "Contrast remapping of height midtones.",
    ));
    tools.push(add(
        "sim.cutoff",
        "Cutoff",
        Icon::Maximize2,
        WorkspaceMode::Simulation,
        "Cutoff",
        LayerKind::EffectFilter(EffectFilterParams::cutoff()),
        "Hard shelf / clamp below a height level.",
    ));
    tools.push(add(
        "sim.kuwahara",
        "Kuwahara",
        Icon::Sparkles,
        WorkspaceMode::Simulation,
        "Kuwahara",
        LayerKind::EffectFilter(EffectFilterParams::kuwahara()),
        "Edge-preserving Kuwahara smoothing.",
    ));
    tools.push(add(
        "sim.terrace_simple",
        "Terrace Simple",
        Icon::Layers,
        WorkspaceMode::Simulation,
        "Terrace Simple",
        LayerKind::EffectFilter(EffectFilterParams::terrace_simple()),
        "Uniform height terraces.",
    ));
    tools.push(add(
        "sim.terrace_irregular",
        "Terrace Irregular",
        Icon::Layers,
        WorkspaceMode::Simulation,
        "Terrace Irregular",
        LayerKind::EffectFilter(EffectFilterParams::terrace_irregular()),
        "Jittered terrace steps.",
    ));
    tools.push(add(
        "sim.terrace_steep",
        "Terrace Steep",
        Icon::Layers,
        WorkspaceMode::Simulation,
        "Terrace Steep",
        LayerKind::EffectFilter(EffectFilterParams::terrace_steep()),
        "Sharp-riser terraces.",
    ));

    // —— Materials —————————————————————————————————————————————————
    tools.push(add(
        "mat.material",
        "Material Paint",
        Icon::Paintbrush,
        WorkspaceMode::Materials,
        "Materials",
        LayerKind::Materials(MaterialsParams::default()),
        "Add a materials / surface layer.",
    ));
    tools.push(add(
        "mat.colour",
        "Colour Paint",
        Icon::Paintbrush,
        WorkspaceMode::Materials,
        "Colour",
        materials_single(material_rule(
            "Colour",
            5,
            0.0,
            90.0,
            f32::NEG_INFINITY,
            f32::INFINITY,
            MaskSource::None,
            0.4,
            [0.55, 0.52, 0.48],
            0.7,
        )),
        "Surface colour / tint material layer.",
    ));
    tools.push(add(
        "mat.wetness",
        "Wetness",
        Icon::Droplets,
        WorkspaceMode::Materials,
        "Wetness",
        materials_single(material_rule(
            "Wet",
            6,
            0.0,
            25.0,
            f32::NEG_INFINITY,
            f32::INFINITY,
            MaskSource::Wetness,
            0.15,
            [0.22, 0.32, 0.28],
            0.95,
        )),
        "Wetness-driven material coverage.",
    ));
    tools.push(add(
        "mat.snow",
        "Snow",
        Icon::Sparkles,
        WorkspaceMode::Materials,
        "Snow",
        materials_single(material_rule(
            "Snow",
            4,
            0.0,
            48.0,
            180.0,
            f32::INFINITY,
            MaskSource::Snow,
            0.12,
            [0.86, 0.89, 0.93],
            0.36,
        )),
        "High-elevation snow cover material.",
    ));
    tools.push(add(
        "mat.rock",
        "Rock",
        Icon::Mountain,
        WorkspaceMode::Materials,
        "Rock",
        materials_single(material_rule(
            "Rock",
            1,
            35.0,
            90.0,
            f32::NEG_INFINITY,
            f32::INFINITY,
            MaskSource::None,
            0.85,
            [0.45, 0.42, 0.38],
            0.85,
        )),
        "Steep rock / cliff material.",
    ));
    tools.push(add(
        "mat.grass",
        "Grass",
        Icon::Sparkles,
        WorkspaceMode::Materials,
        "Grass",
        materials_single(material_rule(
            "Grass",
            2,
            0.0,
            35.0,
            5.0,
            f32::INFINITY,
            MaskSource::None,
            0.2,
            [0.28, 0.48, 0.22],
            0.9,
        )),
        "Grass / soft ground material.",
    ));

    // —— Objects (scatter) —————————————————————————————————————————
    tools.push(add(
        "obj.trees",
        "Trees",
        Icon::Package,
        WorkspaceMode::Objects,
        "Trees",
        LayerKind::Vegetation(VegetationParams::default()),
        "Scatter trees / vegetation.",
    ));
    tools.push(add(
        "obj.rocks",
        "Rocks",
        Icon::Mountain,
        WorkspaceMode::Objects,
        "Rocks",
        LayerKind::Vegetation(veg_with_coverage(
            VegetationParams {
                density: 0.22,
                min_distance: 5.0,
                min_slope_deg: 22.0,
                max_slope_deg: 90.0,
                ..VegetationParams::default()
            },
            vec![DistNode::new(DistNodeKind::Rocks {
                density: 0.45,
                threshold: 0.35,
            })],
        )),
        "Rock scatter on steep slopes.",
    ));
    tools.push(add(
        "obj.grass",
        "Grass",
        Icon::Sparkles,
        WorkspaceMode::Objects,
        "Grass",
        LayerKind::Vegetation(VegetationParams {
            density: 0.6,
            ..VegetationParams::default()
        }),
        "Dense grass / ground cover scatter.",
    ));
    tools.push(add(
        "obj.debris",
        "Debris",
        Icon::Sparkles,
        WorkspaceMode::Objects,
        "Debris",
        LayerKind::Vegetation(VegetationParams {
            density: 0.55,
            min_distance: 1.5,
            min_slope_deg: 5.0,
            max_slope_deg: 55.0,
            seed: 77,
            ..VegetationParams::default()
        }),
        "Dense debris / litter scatter.",
    ));
    tools.push(add(
        "obj.custom",
        "Custom Meshes",
        Icon::Box,
        WorkspaceMode::Objects,
        "Custom Scatter",
        LayerKind::Vegetation(VegetationParams {
            density: 0.2,
            min_distance: 8.0,
            max_slope_deg: 40.0,
            seed: 99,
            ..VegetationParams::default()
        }),
        "Generic mesh scatter (vegetation backend).",
    ));
    tools.push(add(
        "obj.density",
        "Density",
        Icon::SlidersHorizontal,
        WorkspaceMode::Objects,
        "Density Scatter",
        LayerKind::Vegetation(VegetationParams {
            density: 0.75,
            min_distance: 1.0,
            ..VegetationParams::default()
        }),
        "High-density scatter preset.",
    ));
    tools.push(add(
        "obj.scale",
        "Scale Variation",
        Icon::Maximize2,
        WorkspaceMode::Objects,
        "Scale Variation",
        LayerKind::Vegetation(VegetationParams {
            scale_min: 0.4,
            scale_max: 2.0,
            ..VegetationParams::default()
        }),
        "Scatter with wide per-instance scale variation.",
    ));
    tools.push(add(
        "obj.rotation",
        "Rotation Variation",
        Icon::RotateCcw,
        WorkspaceMode::Objects,
        "Rotation Variation",
        LayerKind::Vegetation(VegetationParams {
            yaw_variation_deg: 180.0,
            ..VegetationParams::default()
        }),
        "Scatter with full per-instance yaw variation.",
    ));
    tools.push(add(
        "obj.slope",
        "Slope Filtering",
        Icon::Activity,
        WorkspaceMode::Objects,
        "Slope Filter",
        LayerKind::Vegetation(VegetationParams {
            density: 0.4,
            min_slope_deg: 0.0,
            max_slope_deg: 18.0,
            ..VegetationParams::default()
        }),
        "Scatter gated to gentle slopes.",
    ));
    tools.push(add(
        "obj.height",
        "Height Filtering",
        Icon::Mountain,
        WorkspaceMode::Objects,
        "Height Filter",
        LayerKind::Vegetation(veg_with_coverage(
            VegetationParams {
                density: 0.4,
                max_slope_deg: 45.0,
                ..VegetationParams::default()
            },
            vec![DistNode::height(20.0, 400.0)],
        )),
        "Scatter gated by height band.",
    ));

    // —— Masks —————————————————————————————————————————————————————
    tools.push(add_mask(
        "mask.height",
        "Height",
        Icon::Mountain,
        "Height Mask",
        MaskSource::Height {
            min: 0.0,
            max: 200.0,
        },
        "Mask from a height range.",
    ));
    tools.push(add_mask(
        "mask.slope",
        "Slope",
        Icon::Activity,
        "Slope Mask",
        MaskSource::Slope {
            min_deg: 20.0,
            max_deg: 60.0,
        },
        "Mask from a slope range.",
    ));
    tools.push(add_mask(
        "mask.curvature",
        "Curvature",
        Icon::Blend,
        "Curvature Mask",
        MaskSource::Curvature {
            min: -0.5,
            max: 0.5,
        },
        "Mask from terrain curvature.",
    ));
    tools.push(add_mask(
        "mask.flow",
        "Flow",
        Icon::Waves,
        "Flow Mask",
        MaskSource::FlowAccumulation { min: 0.1, max: 1.0 },
        "Mask from flow accumulation.",
    ));
    tools.push(add_mask(
        "mask.convexity",
        "Convexity",
        Icon::CircleDot,
        "Convexity Mask",
        MaskSource::Convexity,
        "Mask convex (ridgelike) regions.",
    ));
    tools.push(add_mask(
        "mask.concavity",
        "Concavity",
        Icon::CircleDot,
        "Concavity Mask",
        MaskSource::Concavity,
        "Mask concave (valley) regions.",
    ));
    tools.push(add_mask(
        "mask.noise",
        "Noise",
        Icon::Sparkles,
        "Noise Mask",
        MaskSource::Noise {
            seed: 1,
            frequency: 0.02,
        },
        "Procedural noise mask.",
    ));
    tools.push(add_mask(
        "mask.distance",
        "Distance",
        Icon::Move,
        "Distance Mask",
        MaskSource::DistanceField { threshold: 0.5 },
        "Distance-field mask from height threshold.",
    ));
    tools.push(add_mask(
        "mask.painted",
        "Painted",
        Icon::Paintbrush,
        "Painted Mask",
        // mask_id is rewritten to the new asset id on insert.
        MaskSource::Painted {
            mask_id: MaskId::nil(),
        },
        "Hand-painted mask — creates the asset and arms paint mode.",
    ));
    tools.push(add_mask(
        "mask.combined",
        "Combined Mask",
        Icon::Layers,
        "Combined Mask",
        MaskSource::Noise {
            seed: 3,
            frequency: 0.02,
        },
        "Noise starter mask — combine with height/slope DistNodes in the stack.",
    ));

    // Utilities (Move / Measure) live on the viewport hotbar — no left-rail catalog entries.

    tools.extend(wc_filter_catalog_tools());
    tools
}

pub(crate) fn all_tools_cached() -> &'static [ToolDef] {
    static TOOLS: OnceLock<Vec<ToolDef>> = OnceLock::new();
    TOOLS.get_or_init(all_tools).as_slice()
}

/// World Creator–style Filters workspace catalog (grouped by WC category).
pub fn wc_filter_catalog_tools() -> Vec<ToolDef> {
    let m = WorkspaceMode::Filters;
    let f = |id, label, icon, name, kind, desc| add(id, label, icon, m, name, kind, desc);
    let ef = |id, label, icon, name, params: EffectFilterParams, desc| {
        f(id, label, icon, name, LayerKind::EffectFilter(params), desc)
    };

    vec![
        // —— General ————————————————————————————————————————————————
        f(
            "filter.general.geomorphic_detail",
            "Geomorphic Detail",
            Icon::Sparkles,
            "Geomorphic Detail",
            LayerKind::GeomorphicDetail(GeomorphicDetailParams::default()),
            "Slope- and drainage-aligned multi-octave relief detail.",
        ),
        ef(
            "filter.general.add_set",
            "Add/Set",
            Icon::Plus,
            "Add/Set",
            EffectFilterParams::add_set(),
            "Add a height offset, or set absolute height.",
        ),
        ef(
            "filter.general.border_blend",
            "Border Blend",
            Icon::Blend,
            "Border Blend",
            EffectFilterParams::border_blend(),
            "Blend rim toward border average.",
        ),
        ef(
            "filter.general.flatten",
            "Flatten",
            Icon::Box,
            "Flatten",
            EffectFilterParams::flatten_filter(),
            "Pull toward local mean/median.",
        ),
        ef(
            "filter.general.zero_edge",
            "Zero-Edge",
            Icon::Maximize2,
            "Zero-Edge",
            EffectFilterParams::zero_edge(),
            "Fade edges to min height.",
        ),
        // —— Design —————————————————————————————————————————————————
        ef(
            "filter.design.curve",
            "Curve",
            Icon::Activity,
            "Curve",
            EffectFilterParams::curve(),
            "Contrast remapping of height midtones.",
        ),
        ef(
            "filter.design.cutoff",
            "Cutoff",
            Icon::Maximize2,
            "Cutoff",
            EffectFilterParams::cutoff(),
            "Hard shelf / clamp below a height level.",
        ),
        f(
            "filter.design.height_map",
            "Height Map",
            Icon::Image,
            "Height Map",
            LayerKind::ImportHeightmap(ImportHeightmapParams::default()),
            "Apply an imported heightmap as a biome Design filter.",
        ),
        ef(
            "filter.design.voronoi",
            "Voronoi",
            Icon::Grid3x3,
            "Voronoi",
            EffectFilterParams::design_voronoi(),
            "Voronoi cell imprint on height.",
        ),
        // —— Effect —————————————————————————————————————————————————
        ef(
            "filter.effect.angle_blur",
            "Angle Blur",
            Icon::Blend,
            "Angle Blur",
            EffectFilterParams::angle_blur(),
            "Anisotropic blur along slope.",
        ),
        ef(
            "filter.effect.directional_blur",
            "Directional Blur",
            Icon::Move,
            "Directional Blur",
            EffectFilterParams::directional_blur(),
            "Fixed-direction anisotropic convolution.",
        ),
        ef(
            "filter.effect.balloon",
            "Balloon",
            Icon::CircleDot,
            "Balloon",
            EffectFilterParams::balloon(),
            "Slope-gated inflate.",
        ),
        ef(
            "filter.effect.blocks",
            "Blocks",
            Icon::Box,
            "Blocks",
            EffectFilterParams::blocks(),
            "Quantize height into blocks.",
        ),
        ef(
            "filter.effect.crater",
            "Crater",
            Icon::CircleDot,
            "Crater",
            EffectFilterParams::crater(),
            "Radial bowl carve.",
        ),
        ef(
            "filter.effect.deflate",
            "Deflate",
            Icon::ArrowDown,
            "Deflate",
            EffectFilterParams::deflate(),
            "Local-min shrink.",
        ),
        ef(
            "filter.effect.denoise",
            "Denoise",
            Icon::Sparkles,
            "Denoise",
            EffectFilterParams::denoise(),
            "Edge-aware denoise.",
        ),
        ef(
            "filter.effect.smooth",
            "Smooth",
            Icon::Blend,
            "Smooth",
            EffectFilterParams::smooth(),
            "Gaussian-style height smooth.",
        ),
        ef(
            "filter.effect.distortion",
            "Distortion",
            Icon::Waves,
            "Distortion",
            EffectFilterParams::distortion(),
            "Domain-warp resample.",
        ),
        ef(
            "filter.effect.hexagons",
            "Hexagons",
            Icon::Grid3x3,
            "Hexagons",
            EffectFilterParams::hexagons(),
            "Hexagonal cell height snap.",
        ),
        ef(
            "filter.effect.inflate",
            "Inflate",
            Icon::ArrowUp,
            "Inflate",
            EffectFilterParams::inflate(),
            "Local-max grow.",
        ),
        ef(
            "filter.effect.kuwahara",
            "Kuwahara",
            Icon::Sparkles,
            "Kuwahara",
            EffectFilterParams::kuwahara(),
            "Edge-preserving Kuwahara smoothing.",
        ),
        ef(
            "filter.effect.ridged",
            "Ridged",
            Icon::Mountain,
            "Ridged",
            EffectFilterParams::ridged(),
            "Ridged multi-fractal detail.",
        ),
        ef(
            "filter.effect.rugged",
            "Rugged",
            Icon::Mountain,
            "Rugged",
            EffectFilterParams::rugged(),
            "FBM + ridged mix.",
        ),
        ef(
            "filter.effect.scatter",
            "Scatter",
            Icon::Sparkles,
            "Scatter",
            EffectFilterParams::scatter_detail(),
            "Sparse rock spike scatter.",
        ),
        ef(
            "filter.effect.shore",
            "Shore",
            Icon::Waves,
            "Shore",
            EffectFilterParams::shore(),
            "Coastal flatten adapter.",
        ),
        ef(
            "filter.effect.smooth_ridges",
            "Smooth Ridges",
            Icon::Mountain,
            "Smooth Ridges",
            EffectFilterParams::smooth_ridges(),
            "Ridged detail with blur.",
        ),
        ef(
            "filter.effect.squeeze",
            "Squeeze",
            Icon::Maximize2,
            "Squeeze",
            EffectFilterParams::squeeze(),
            "Radial squeeze warp.",
        ),
        ef(
            "filter.effect.strata",
            "Strata",
            Icon::Layers,
            "Strata",
            EffectFilterParams::strata(),
            "Height banding / strata.",
        ),
        ef(
            "filter.effect.swirl",
            "Swirl",
            Icon::RotateCcw,
            "Swirl",
            EffectFilterParams::swirl(),
            "Angular domain warp.",
        ),
        ef(
            "filter.effect.washed_off",
            "Washed Off",
            Icon::Droplets,
            "Washed Off",
            EffectFilterParams::washed_off(),
            "Smooth and lower steep slopes.",
        ),
        // —— Noise ——————————————————————————————————————————————————
        ef(
            "filter.noise.billow",
            "Billow",
            Icon::Waves,
            "Billow",
            EffectFilterParams::noise_billow(),
            "Billow noise height modulation.",
        ),
        ef(
            "filter.noise.gabor",
            "Gabor",
            Icon::Activity,
            "Gabor",
            EffectFilterParams::noise_gabor(),
            "Directional Gabor-like noise.",
        ),
        ef(
            "filter.noise.perlin",
            "Perlin",
            Icon::Waves,
            "Perlin",
            EffectFilterParams::noise_perlin(),
            "Perlin FBM height modulation.",
        ),
        ef(
            "filter.noise.phasor",
            "Phasor",
            Icon::Activity,
            "Phasor",
            EffectFilterParams::noise_phasor(),
            "Phasor-like simplex pair noise.",
        ),
        ef(
            "filter.noise.ridged",
            "Ridged",
            Icon::Mountain,
            "Ridged Noise",
            EffectFilterParams::noise_ridged(),
            "Ridged multi-fractal noise.",
        ),
        ef(
            "filter.noise.simplex",
            "Simplex",
            Icon::Waves,
            "Simplex",
            EffectFilterParams::noise_simplex(),
            "OpenSimplex FBM height modulation.",
        ),
        ef(
            "filter.noise.value",
            "Value",
            Icon::Waves,
            "Value",
            EffectFilterParams::noise_value(),
            "Value noise height modulation.",
        ),
        ef(
            "filter.noise.voronoi",
            "Voronoi",
            Icon::Grid3x3,
            "Voronoi Noise",
            EffectFilterParams::noise_voronoi(),
            "Worley / Voronoi noise modulation.",
        ),
        ef(
            "filter.noise.wave",
            "Wave",
            Icon::Waves,
            "Wave",
            EffectFilterParams::noise_wave(),
            "Sinusoidal wave height pattern.",
        ),
        ef(
            "filter.noise.white",
            "White",
            Icon::Sparkles,
            "White",
            EffectFilterParams::noise_white(),
            "White noise height speckles.",
        ),
        // —— Arid ———————————————————————————————————————————————————
        ef(
            "filter.effect.cliff_reinforce",
            "Cliff Reinforce",
            Icon::Mountain,
            "Cliff Reinforce",
            EffectFilterParams::cliff_reinforce(),
            "Reinforce steep cliff faces.",
        ),
        ef(
            "filter.arid.canyon",
            "Canyon",
            Icon::Mountain,
            "Canyon",
            EffectFilterParams::canyon(),
            "Deep flow canyon carve.",
        ),
        ef(
            "filter.arid.chipped",
            "Chipped",
            Icon::Mountain,
            "Chipped",
            EffectFilterParams::chipped(),
            "High-frequency cliff chips.",
        ),
        f(
            "filter.arid.dunes",
            "Dunes",
            Icon::Waves,
            "Dunes",
            LayerKind::Dunes(DuneParams::default()),
            "Sandy dune pattern: phasor seed + limited sand transport (generator).",
        ),
        ef(
            "filter.arid.rocky_cliffs",
            "Rocky Cliffs",
            Icon::Mountain,
            "Rocky Cliffs",
            EffectFilterParams::rocky_cliffs(),
            "Steep rocky cliff reinforce.",
        ),
        ef(
            "filter.arid.rocky_hard",
            "Rocky Hard",
            Icon::Mountain,
            "Rocky Hard",
            EffectFilterParams::rocky_hard(),
            "Harder rocky cliffs variant.",
        ),
        ef(
            "filter.arid.rocky_layers",
            "Rocky Layers",
            Icon::Layers,
            "Rocky Layers",
            EffectFilterParams::rocky_layers(),
            "Bedded rocky ledges.",
        ),
        ef(
            "filter.arid.rocky_plateaus",
            "Rocky Plateaus",
            Icon::Box,
            "Rocky Plateaus",
            EffectFilterParams::rocky_plateaus(),
            "Large plateau steps.",
        ),
        ef(
            "filter.arid.rocky_sharp",
            "Rocky Sharp",
            Icon::Mountain,
            "Rocky Sharp",
            EffectFilterParams::rocky_sharp(),
            "Steep ridged rocky detail.",
        ),
        ef(
            "filter.arid.rocky_wide",
            "Rocky Wide",
            Icon::Mountain,
            "Rocky Wide",
            EffectFilterParams::rocky_wide(),
            "Broad rocky banding.",
        ),
        // —— Terrace ————————————————————————————————————————————————
        ef(
            "filter.terrace.irregular",
            "Irregular",
            Icon::Layers,
            "Terrace Irregular",
            EffectFilterParams::terrace_irregular(),
            "Jittered terrace steps.",
        ),
        ef(
            "filter.terrace.simple",
            "Simple",
            Icon::Layers,
            "Terrace Simple",
            EffectFilterParams::terrace_simple(),
            "Uniform height terraces.",
        ),
        ef(
            "filter.terrace.steep",
            "Steep",
            Icon::Layers,
            "Terrace Steep",
            EffectFilterParams::terrace_steep(),
            "Sharp-riser terraces.",
        ),
        // —— Drift ——————————————————————————————————————————————————
        ef(
            "filter.drift.angle_break",
            "Angle Break",
            Icon::Activity,
            "Angle Break",
            EffectFilterParams::angle_break(),
            "Slope continuity breaks.",
        ),
        ef(
            "filter.drift.wind",
            "Wind",
            Icon::Waves,
            "Wind",
            EffectFilterParams::wind_carve(),
            "Directional wind carve via aeolian transport.",
        ),
        // —— Basic erosion ——————————————————————————————————————————
        f(
            "filter.basic.hydraulic",
            "Hydraulic",
            Icon::Droplets,
            "Hydraulic Erosion",
            LayerKind::HydraulicErosion(HydraulicErosionParams::default()),
            "Rainfall and sediment transport simulation.",
        ),
        ef(
            "filter.basic.ridged_flows",
            "Ridged Flows",
            Icon::Activity,
            "Ridged Flows",
            EffectFilterParams::ridged_flows(),
            "Channels with ridge boost.",
        ),
        ef(
            "filter.basic.rocky",
            "Rocky",
            Icon::Mountain,
            "Rocky",
            EffectFilterParams::rocky(),
            "Soft flows + rocky sharp mix.",
        ),
        ef(
            "filter.basic.soft_flows",
            "Soft Flows",
            Icon::Droplets,
            "Soft Flows",
            EffectFilterParams::soft_flows(),
            "Broad drainage carve.",
        ),
        // —— Advanced erosion ———————————————————————————————————————
        ef(
            "filter.advanced.hydraulic_sediment",
            "Hydraulic Sediment",
            Icon::Droplets,
            "Hydraulic Sediment",
            EffectFilterParams::hydraulic_sediment(),
            "Slope-break sediment fans.",
        ),
        ef(
            "filter.advanced.sediment_flows",
            "Sediment Flows",
            Icon::Layers,
            "Sediment Flows",
            EffectFilterParams::sediment_flows(),
            "Flow-follow deposition.",
        ),
        ef(
            "filter.advanced.thin_flows",
            "Thin Flows",
            Icon::Activity,
            "Thin Flows",
            EffectFilterParams::thin_flows(),
            "Narrow gully carve.",
        ),
        ef(
            "filter.advanced.wide_flows",
            "Wide Flows",
            Icon::Waves,
            "Wide Flows",
            EffectFilterParams::wide_flows(),
            "Valley broadening.",
        ),
        // —— Sediment ———————————————————————————————————————————————
        ef(
            "filter.sediment.fill_soft",
            "Fill Soft",
            Icon::Layers,
            "Fill Soft",
            EffectFilterParams::sediment_fill_soft(),
            "Low-slope valley fill.",
        ),
        ef(
            "filter.sediment.mud",
            "Mud",
            Icon::Droplets,
            "Mud",
            EffectFilterParams::mud_settle(),
            "Sink flatten / mud settle.",
        ),
        ef(
            "filter.sediment.talus",
            "Talus",
            Icon::Mountain,
            "Talus",
            EffectFilterParams::talus_fill(),
            "Scree below cliffs.",
        ),
    ]
}

pub fn tools_for_mode(mode: WorkspaceMode) -> Vec<&'static ToolDef> {
    all_tools_cached()
        .iter()
        .filter(|tool| tool.mode == mode)
        .collect()
}

/// Tools visible for a task workspace (metadata-driven filter).
pub fn tools_for_workspace(id: crate::ui::workspace::WorkspaceId) -> Vec<&'static ToolDef> {
    let def = crate::ui::workspace::workspace_definition(id);
    match def.tools {
        crate::ui::workspace::WorkspaceToolFilter::WcFilters => all_tools_cached()
            .iter()
            .filter(|tool| tool.mode == WorkspaceMode::Filters)
            .collect(),
        _ => all_tools_cached()
            .iter()
            .filter(|tool| def.includes_catalog_mode(tool.mode))
            .collect(),
    }
}

/// Palette section for categorized tool lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolGroup {
    Landforms,
    Hydrology,
    Noise,
    Simulation,
    Masks,
    Brushes,
    Other,
    // WC Filters workspace categories
    FilterGeneral,
    FilterDesign,
    FilterEffect,
    FilterNoise,
    FilterArid,
    FilterTerrace,
    FilterDrift,
    FilterBasicErosion,
    FilterAdvancedErosion,
    FilterSediment,
}

impl ToolGroup {
    pub fn label(self) -> &'static str {
        match self {
            Self::Landforms => "SHAPE LAYERS",
            Self::Hydrology => "HYDROLOGY",
            Self::Noise => "NOISE",
            Self::Simulation => "SIMULATION",
            Self::Masks => "MASKS",
            Self::Brushes => "BRUSHES",
            Self::Other => "TOOLS",
            Self::FilterGeneral => "GENERAL",
            Self::FilterDesign => "DESIGN",
            Self::FilterEffect => "EFFECT",
            Self::FilterNoise => "NOISE",
            Self::FilterArid => "ARID",
            Self::FilterTerrace => "TERRACE",
            Self::FilterDrift => "DRIFT",
            Self::FilterBasicErosion => "BASIC EROSION",
            Self::FilterAdvancedErosion => "ADVANCED EROSION",
            Self::FilterSediment => "SEDIMENT",
        }
    }
}

impl ToolDef {
    pub fn group(&self) -> ToolGroup {
        let id = self.id;
        if id.starts_with("filter.general.") {
            return ToolGroup::FilterGeneral;
        }
        if id.starts_with("filter.design.") {
            return ToolGroup::FilterDesign;
        }
        if id.starts_with("filter.effect.") {
            return ToolGroup::FilterEffect;
        }
        if id.starts_with("filter.noise.") {
            return ToolGroup::FilterNoise;
        }
        if id.starts_with("filter.arid.") {
            return ToolGroup::FilterArid;
        }
        if id.starts_with("filter.terrace.") {
            return ToolGroup::FilterTerrace;
        }
        if id.starts_with("filter.drift.") {
            return ToolGroup::FilterDrift;
        }
        if id.starts_with("filter.basic.") {
            return ToolGroup::FilterBasicErosion;
        }
        if id.starts_with("filter.advanced.") {
            return ToolGroup::FilterAdvancedErosion;
        }
        if id.starts_with("filter.sediment.") {
            return ToolGroup::FilterSediment;
        }
        if id.starts_with("shape.") {
            return ToolGroup::Landforms;
        }
        if id.starts_with("sculpt.")
            || id.starts_with("biome.paint")
            || id.starts_with("biome.erase")
            || id.starts_with("biome.smooth")
            || id.starts_with("biome.replace")
            || id.starts_with("biome.normalize")
            || id.starts_with("biome.flood")
            || id.starts_with("biome.polygon")
        {
            return ToolGroup::Brushes;
        }
        // Flatten / terrace live under WC Filters (`filter.general.*` / `filter.terrace.*`).
        if id.contains("mountain")
            || id.contains("volcano")
            || id.contains("mesa")
            || id.contains("canyon")
            || id.contains("plateau")
            || id.contains("hill")
            || id.contains("dune")
            || id.contains("crater")
            || id.contains("island")
            || id == "gen.flat"
        {
            return ToolGroup::Landforms;
        }
        if id.contains("river")
            || id.contains("coast")
            || id.contains("lake")
            || id.contains("waterfall")
            || id.starts_with("sim.river")
            || id.starts_with("sim.coastal")
            || id.starts_with("sim.lake")
            || id.starts_with("sim.waterfall")
        {
            return ToolGroup::Hydrology;
        }
        if id.contains("ridge")
            || id.contains("noise")
            || id.contains("warp")
            || id.contains("fbm")
            || id.contains("cellular")
            || id.contains("voronoi")
        {
            return ToolGroup::Noise;
        }
        if id.starts_with("sim.")
            || id.contains("thermal")
            || id.contains("hydraulic")
            || id.contains("weathering")
        {
            return ToolGroup::Simulation;
        }
        if id.starts_with("mask.") {
            return ToolGroup::Masks;
        }
        ToolGroup::Other
    }
}

/// Quick-Add searchable entries (wired tools that create layers + groups).
pub fn quick_add_entries() -> Vec<ToolDef> {
    all_tools_cached()
        .iter()
        .filter(|t| {
            matches!(
                t.action,
                ToolAction::AddLayer { .. }
                    | ToolAction::CreateBiome { .. }
                    | ToolAction::AddMask { .. }
                    | ToolAction::BiomeBrush(_)
            )
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::EditorTool;
    use std::collections::HashSet;
    use terra_core::mask::MaskSource;

    /// Modes whose tools live on the viewport hotbar, not the left-rail catalog.
    /// `Utilities` (Move / Measure / camera navigation) is intentionally
    /// catalog-empty; Bake was removed. These modes are exempt from the
    /// "every mode has catalog tools" invariant and, conversely, must stay empty.
    const HOTBAR_ONLY_MODES: &[WorkspaceMode] = &[WorkspaceMode::Utilities];

    #[test]
    fn workspace_catalog_reuses_cached_definitions() {
        let cached = all_tools_cached();
        let visible = tools_for_workspace(crate::ui::workspace::WorkspaceId::Simulation);
        assert!(!visible.is_empty());
        for tool in visible {
            assert!(cached.iter().any(|candidate| std::ptr::eq(candidate, tool)));
        }
    }

    #[test]
    fn every_mode_has_tools() {
        for mode in WorkspaceMode::ALL {
            if HOTBAR_ONLY_MODES.contains(&mode) {
                continue;
            }
            assert!(
                !tools_for_mode(mode).is_empty(),
                "{:?} should have catalog tools",
                mode
            );
        }
    }

    #[test]
    fn terrain_has_shape_layers_and_brushes() {
        let ids: Vec<_> = tools_for_mode(WorkspaceMode::Terrain)
            .iter()
            .map(|t| t.id)
            .collect();
        assert!(ids.contains(&"sculpt.raise"));
        assert!(ids.contains(&"shape.procedural"));
        assert!(ids.contains(&"shape.sculpt"));
        assert!(ids.contains(&"shape.heightmap"));
        assert!(!ids.contains(&"gen.mountain"));
        assert!(!ids.contains(&"paint.height"));
    }

    #[test]
    fn shape_layer_group_excludes_biome_filters() {
        use terra_core::layer::is_shape_kind;
        for tool in all_tools() {
            if !tool.id.starts_with("shape.") {
                continue;
            }
            let ToolAction::AddLayer { kind, .. } = &tool.action else {
                continue;
            };
            assert!(
                tool.id == "shape.heightmap" || is_shape_kind(kind),
                "{} should be a WC shape layer kind",
                tool.id
            );
        }
        // Filter catalog entries must not claim the SHAPE LAYERS group.
        for tool in all_tools().into_iter().filter(|t| t.id.starts_with("filter.")) {
            assert_ne!(tool.group(), ToolGroup::Landforms);
        }
    }

    #[test]
    fn no_duplicate_raise_or_paint_mask_across_modes() {
        let mut raise_homes = HashSet::new();
        let mut painted_homes = HashSet::new();
        for t in all_tools() {
            if matches!(t.action, ToolAction::Sculpt(EditorTool::Raise)) {
                raise_homes.insert(t.mode);
            }
            if matches!(
                t.action,
                ToolAction::AddMask {
                    source: MaskSource::Painted { .. },
                    ..
                }
            ) {
                painted_homes.insert(t.mode);
            }
        }
        assert_eq!(raise_homes.len(), 1);
        assert!(raise_homes.contains(&WorkspaceMode::Terrain));
        assert_eq!(painted_homes.len(), 1);
        assert!(painted_homes.contains(&WorkspaceMode::Masks));
    }

    #[test]
    fn hotbar_only_modes_have_no_catalog_tools() {
        for &mode in HOTBAR_ONLY_MODES {
            assert!(
                tools_for_mode(mode).is_empty(),
                "{:?} tools live on the viewport hotbar; the left-rail catalog must stay empty",
                mode
            );
        }
    }

    #[test]
    fn simulation_owns_river_once() {
        let rivers: Vec<_> = all_tools()
            .into_iter()
            .filter(|t| {
                matches!(
                    t.action,
                    ToolAction::AddLayer {
                        kind: LayerKind::RiverCarve(_),
                        ..
                    }
                )
            })
            .collect();
        assert_eq!(rivers.len(), 1);
        assert_eq!(rivers[0].mode, WorkspaceMode::Simulation);
    }

    #[test]
    fn every_catalog_tool_has_a_thumbnail() {
        let missing: Vec<_> = all_tools()
            .into_iter()
            .filter(|t| crate::ui::tool_thumbs::thumb_for_tool(t.id).is_none())
            .map(|t| t.id)
            .collect();
        assert!(missing.is_empty(), "tools missing 3D thumbs: {missing:?}");
    }

    #[test]
    fn wc_filter_catalog_covers_all_effect_kinds() {
        use terra_core::layer::EffectFilterKind;
        let catalog_kinds: std::collections::HashSet<_> = wc_filter_catalog_tools()
            .into_iter()
            .filter_map(|t| match t.action {
                ToolAction::AddLayer {
                    kind: LayerKind::EffectFilter(p),
                    ..
                } => Some(p.kind),
                _ => None,
            })
            .collect();
        let missing: Vec<_> = EffectFilterKind::ALL
            .iter()
            .copied()
            // Intentionally omitted from the WC Filters shelf (legacy / advanced-add only).
            .filter(|k| {
                !matches!(
                    k,
                    EffectFilterKind::Cliffs | EffectFilterKind::SpikeRemoval
                )
            })
            .filter(|k| !catalog_kinds.contains(k))
            .map(|k| k.label())
            .collect();
        assert!(
            missing.is_empty(),
            "EffectFilterKind missing from WC filter catalog: {missing:?}"
        );
    }

    #[test]
    fn island_and_crater_remain_in_add_menu_path() {
        // Legacy landform kinds still exist for projects / Advanced Add Layer;
        // Terrain tools use Procedural Shape + generator picker instead.
        let tools = all_tools();
        assert!(tools.iter().any(|t| t.id == "shape.procedural"));
        let crater_proc = tools.iter().find(|t| t.id == "shape.procedural").unwrap();
        assert!(matches!(
            crater_proc.action,
            ToolAction::AddLayer {
                kind: LayerKind::ProceduralShape(_),
                ..
            }
        ));
    }

    #[test]
    fn mask_tools_add_mask_assets() {
        let height = all_tools()
            .into_iter()
            .find(|t| t.id == "mask.height")
            .unwrap();
        assert!(matches!(
            height.action,
            ToolAction::AddMask {
                source: MaskSource::Height { .. },
                ..
            }
        ));
        let painted = all_tools()
            .into_iter()
            .find(|t| t.id == "mask.painted")
            .unwrap();
        assert!(matches!(
            painted.action,
            ToolAction::AddMask {
                source: MaskSource::Painted { .. },
                ..
            }
        ));
    }

    #[test]
    fn bake_tool_removed_from_catalog() {
        assert!(
            all_tools().into_iter().all(|t| t.id != "util.bake"),
            "Bake belongs in commands / layer actions, not the tool catalog"
        );
    }

    #[test]
    fn biome_create_makes_wc_biome_not_climate_layer() {
        let create = all_tools()
            .into_iter()
            .find(|t| t.id == "biome.create")
            .expect("biome.create catalog entry");
        assert!(
            matches!(create.action, ToolAction::CreateBiome { name: "Biome" }),
            "Create Biome must add a WC biome container, not LayerKind::Biomes: {:?}",
            create.action
        );
        let climate = all_tools()
            .into_iter()
            .find(|t| t.id == "biome.climate")
            .expect("biome.climate catalog entry");
        assert!(matches!(
            climate.action,
            ToolAction::AddLayer {
                kind: LayerKind::Biomes(_),
                ..
            }
        ));
        assert!(quick_add_entries()
            .iter()
            .any(|t| matches!(t.action, ToolAction::CreateBiome { .. })));
    }

    #[test]
    fn filters_workspace_has_wc_categories() {
        use crate::ui::workspace::WorkspaceId;
        let tools = tools_for_workspace(WorkspaceId::Develop);
        assert!(!tools.is_empty());
        let ids: Vec<_> = tools.iter().map(|t| t.id).collect();
        assert!(ids.contains(&"filter.general.add_set"));
        assert!(ids.contains(&"filter.effect.kuwahara"));
        assert!(ids.contains(&"filter.noise.perlin"));
        assert!(ids.contains(&"filter.arid.rocky_plateaus"));
        assert!(ids.contains(&"filter.arid.dunes"));
        assert!(ids.contains(&"filter.design.height_map"));
        assert!(ids.contains(&"filter.terrace.simple"));
        assert!(ids.contains(&"filter.basic.hydraulic"));
        assert!(tools.iter().all(|t| t.id.starts_with("filter.")));
        assert!(tools.iter().all(|t| !t.id.starts_with("filter.experimental.")));
    }

    #[test]
    fn dunes_and_height_map_filter_entries_use_via_other_kinds() {
        let dunes = wc_filter_catalog_tools()
            .into_iter()
            .find(|t| t.id == "filter.arid.dunes")
            .expect("Dunes filter catalog entry");
        assert!(matches!(
            dunes.action,
            ToolAction::AddLayer {
                kind: LayerKind::Dunes(_),
                ..
            }
        ));
        let height_map = wc_filter_catalog_tools()
            .into_iter()
            .find(|t| t.id == "filter.design.height_map")
            .expect("Height Map filter catalog entry");
        assert!(matches!(
            height_map.action,
            ToolAction::AddLayer {
                kind: LayerKind::ImportHeightmap(_),
                ..
            }
        ));
    }
}
