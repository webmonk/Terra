//! Tool catalog — maps workspace modes to sculpt tools and layer kinds.

use crate::workspace::WorkspaceMode;
use crate::EditorTool;
use terra_core::layer::{
    BiomesParams, BlurParams, CanyonParams, CoastalParams, DomainWarpParams, DuneParams, FbmParams,
    FlatParams, HydraulicErosionParams, LayerKind, MaterialsParams, MountainParams, NoiseParams,
    PlateauParams, RiverCarveParams, TerraceParams, ThermalErosionParams, VegetationParams,
};
use terra_gui::Icon;

/// What activating a catalog entry does.
#[derive(Debug, Clone)]
pub enum ToolAction {
    /// Select a sculpt / navigation / paint editor tool.
    Sculpt(EditorTool),
    /// Drag or click to add a layer of this kind.
    AddLayer { name: &'static str, kind: LayerKind },
    /// Shown in the palette but not yet implemented.
    Stub,
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
    pub fn is_stub(&self) -> bool {
        matches!(self.action, ToolAction::Stub)
    }

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
    tool: EditorTool,
    description: &'static str,
    shortcut: Option<&'static str>,
) -> ToolDef {
    ToolDef {
        id,
        label,
        icon,
        mode: WorkspaceMode::Sculpt,
        action: ToolAction::Sculpt(tool),
        description,
        shortcut,
    }
}

fn stub(
    id: &'static str,
    label: &'static str,
    icon: Icon,
    mode: WorkspaceMode,
    description: &'static str,
) -> ToolDef {
    ToolDef {
        id,
        label,
        icon,
        mode,
        action: ToolAction::Stub,
        description,
        shortcut: None,
    }
}

/// Full catalog for all modes (wired + stubs).
pub fn all_tools() -> Vec<ToolDef> {
    let mut tools = Vec::new();

    // —— Generate ————————————————————————————————————————————————
    tools.push(add(
        "gen.mountain",
        "Mountain",
        Icon::Mountain,
        WorkspaceMode::Generate,
        "Mountains",
        LayerKind::Mountains(MountainParams::default()),
        "Add a mountain range generator.",
    ));
    tools.push(add(
        "gen.hills",
        "Hills",
        Icon::Activity,
        WorkspaceMode::Generate,
        "Hills",
        LayerKind::Fbm(FbmParams::default()),
        "Rolling hills via fractal noise.",
    ));
    tools.push(add(
        "gen.canyon",
        "Canyon",
        Icon::Layers,
        WorkspaceMode::Generate,
        "Canyons",
        LayerKind::Canyons(CanyonParams::default()),
        "Carve canyon networks.",
    ));
    tools.push(add(
        "gen.plateau",
        "Plateau",
        Icon::Box,
        WorkspaceMode::Generate,
        "Plateau",
        LayerKind::Plateau(PlateauParams::default()),
        "Raised flat-topped landforms.",
    ));
    tools.push(stub(
        "gen.crater",
        "Crater",
        Icon::CircleDot,
        WorkspaceMode::Generate,
        "Impact crater generator (coming soon).",
    ));
    tools.push(stub(
        "gen.island",
        "Island",
        Icon::Waves,
        WorkspaceMode::Generate,
        "Island landmass generator (coming soon).",
    ));
    tools.push(add(
        "gen.dunes",
        "Dunes",
        Icon::Waves,
        WorkspaceMode::Generate,
        "Dunes",
        LayerKind::Dunes(DuneParams::default()),
        "Wind-blown sand dunes.",
    ));
    tools.push(add(
        "gen.ridge",
        "Ridge",
        Icon::Activity,
        WorkspaceMode::Generate,
        "Ridges",
        LayerKind::Ridged(FbmParams::default()),
        "Sharp ridged noise terrain.",
    ));
    tools.push(add(
        "gen.flat",
        "Flat Base",
        Icon::Box,
        WorkspaceMode::Generate,
        "Flat",
        LayerKind::Flat(FlatParams::default()),
        "Constant-height base layer.",
    ));
    tools.push(add(
        "gen.noise",
        "Noise",
        Icon::Sparkles,
        WorkspaceMode::Generate,
        "Noise",
        LayerKind::NoiseValue(NoiseParams::default()),
        "Value noise height contribution.",
    ));
    tools.push(add(
        "gen.warp",
        "Warp",
        Icon::Move,
        WorkspaceMode::Generate,
        "Domain Warp",
        LayerKind::DomainWarp(DomainWarpParams::default()),
        "Distort underlying terrain.",
    ));

    // —— Sculpt ——————————————————————————————————————————————————
    tools.push(sculpt(
        "sculpt.move",
        "Move",
        Icon::Move,
        EditorTool::Move,
        "Orbit, pan, and zoom the camera.",
        Some("V"),
    ));
    tools.push(sculpt(
        "sculpt.raise",
        "Raise",
        Icon::ArrowUp,
        EditorTool::Raise,
        "Raises terrain beneath the brush. Hold Ctrl to lower. Hold Shift to smooth.",
        Some("R"),
    ));
    tools.push(sculpt(
        "sculpt.lower",
        "Lower",
        Icon::ArrowDown,
        EditorTool::Lower,
        "Lowers terrain beneath the brush. Hold Ctrl to raise. Hold Shift to smooth.",
        Some("L"),
    ));
    tools.push(sculpt(
        "sculpt.smooth",
        "Smooth",
        Icon::Blend,
        EditorTool::Smooth,
        "Smooths terrain beneath the brush.",
        Some("S"),
    ));
    tools.push(add(
        "sculpt.flatten",
        "Flatten",
        Icon::Box,
        WorkspaceMode::Sculpt,
        "Flatten",
        LayerKind::Plateau(PlateauParams::default()),
        "Flatten region toward a target height (layer).",
    ));
    tools.push(add(
        "sculpt.terrace",
        "Terrace",
        Icon::Layers,
        WorkspaceMode::Sculpt,
        "Terrace",
        LayerKind::Terrace(TerraceParams::default()),
        "Step the heightfield into terraces.",
    ));
    tools.push(stub(
        "sculpt.pinch",
        "Pinch",
        Icon::Maximize2,
        WorkspaceMode::Sculpt,
        "Pinch brush (coming soon).",
    ));
    tools.push(stub(
        "sculpt.inflate",
        "Inflate",
        Icon::CircleDot,
        WorkspaceMode::Sculpt,
        "Inflate brush (coming soon).",
    ));
    tools.push(add(
        "sculpt.noise_brush",
        "Noise Brush",
        Icon::Sparkles,
        WorkspaceMode::Sculpt,
        "Noise Detail",
        LayerKind::NoiseValue(NoiseParams::default()),
        "Add noise detail as a layer.",
    ));
    tools.push(add(
        "sculpt.erode_brush",
        "Erode Brush",
        Icon::Droplets,
        WorkspaceMode::Sculpt,
        "Erode",
        LayerKind::ThermalErosion(ThermalErosionParams {
            iterations: 8,
            ..Default::default()
        }),
        "Light thermal weathering pass.",
    ));

    // —— Erosion —————————————————————————————————————————————————
    tools.push(add(
        "erosion.hydraulic",
        "Hydraulic",
        Icon::Droplets,
        WorkspaceMode::Erosion,
        "Hydraulic Erosion",
        LayerKind::HydraulicErosion(HydraulicErosionParams::default()),
        "Simulate rainfall and sediment transport.",
    ));
    tools.push(add(
        "erosion.thermal",
        "Thermal",
        Icon::Sun,
        WorkspaceMode::Erosion,
        "Thermal Erosion",
        LayerKind::ThermalErosion(ThermalErosionParams::default()),
        "Talus slope weathering.",
    ));
    tools.push(add(
        "erosion.coastal",
        "Coastal",
        Icon::Waves,
        WorkspaceMode::Erosion,
        "Coastal",
        LayerKind::Coastal(CoastalParams::default()),
        "Shoreline and coastal erosion.",
    ));
    tools.push(stub(
        "erosion.wind",
        "Wind",
        Icon::Activity,
        WorkspaceMode::Erosion,
        "Wind erosion (coming soon).",
    ));
    tools.push(add(
        "erosion.river",
        "River Carve",
        Icon::Waves,
        WorkspaceMode::Erosion,
        "River Carve",
        LayerKind::RiverCarve(RiverCarveParams::default()),
        "Carve rivers from flow accumulation.",
    ));
    tools.push(stub(
        "erosion.sediment",
        "Sediment",
        Icon::Layers,
        WorkspaceMode::Erosion,
        "Sediment deposition filter (coming soon).",
    ));
    tools.push(stub(
        "erosion.talus",
        "Talus",
        Icon::Mountain,
        WorkspaceMode::Erosion,
        "Talus angle filter (coming soon).",
    ));
    tools.push(add(
        "erosion.weathering",
        "Weathering",
        Icon::Blend,
        WorkspaceMode::Erosion,
        "Weathering",
        LayerKind::Blur(BlurParams::default()),
        "Gentle blur weathering pass.",
    ));

    // —— Masks ———————————————————————————————————————————————————
    // Masks are created via mask editor / paint; expose create-adjacent tools.
    tools.push(stub(
        "mask.height",
        "Height",
        Icon::Mountain,
        WorkspaceMode::Masks,
        "Height-range mask (open Mask Editor).",
    ));
    tools.push(stub(
        "mask.slope",
        "Slope",
        Icon::Activity,
        WorkspaceMode::Masks,
        "Slope mask (open Mask Editor).",
    ));
    tools.push(stub(
        "mask.curvature",
        "Curvature",
        Icon::Activity,
        WorkspaceMode::Masks,
        "Curvature mask (coming soon).",
    ));
    tools.push(stub(
        "mask.flow",
        "Flow",
        Icon::Droplets,
        WorkspaceMode::Masks,
        "Flow accumulation mask.",
    ));
    tools.push(stub(
        "mask.convexity",
        "Convexity",
        Icon::CircleDot,
        WorkspaceMode::Masks,
        "Convexity mask (coming soon).",
    ));
    tools.push(stub(
        "mask.concavity",
        "Concavity",
        Icon::CircleDot,
        WorkspaceMode::Masks,
        "Concavity mask (coming soon).",
    ));
    tools.push(stub(
        "mask.noise",
        "Noise",
        Icon::Sparkles,
        WorkspaceMode::Masks,
        "Noise mask.",
    ));
    tools.push(stub(
        "mask.distance",
        "Distance",
        Icon::Maximize2,
        WorkspaceMode::Masks,
        "Distance field mask (coming soon).",
    ));
    tools.push(ToolDef {
        id: "mask.painted",
        label: "Painted Mask",
        icon: Icon::Paintbrush,
        mode: WorkspaceMode::Masks,
        action: ToolAction::Sculpt(EditorTool::PaintMask),
        description: "Paint into the selected layer's mask.",
        shortcut: Some("M"),
    });
    tools.push(stub(
        "mask.combined",
        "Combined Mask",
        Icon::Layers,
        WorkspaceMode::Masks,
        "Combine multiple masks (coming soon).",
    ));

    // —— Paint ———————————————————————————————————————————————————
    tools.push(ToolDef {
        id: "paint.height",
        label: "Height Paint",
        icon: Icon::ArrowUp,
        mode: WorkspaceMode::Paint,
        action: ToolAction::Sculpt(EditorTool::Raise),
        description: "Paint height on the sculpt base.",
        shortcut: None,
    });
    tools.push(add(
        "paint.material",
        "Material Paint",
        Icon::Paintbrush,
        WorkspaceMode::Paint,
        "Materials",
        LayerKind::Materials(MaterialsParams::default()),
        "Add a materials layer.",
    ));
    tools.push(add(
        "paint.biome",
        "Biome Paint",
        Icon::Layers,
        WorkspaceMode::Paint,
        "Biomes",
        LayerKind::Biomes(BiomesParams::default()),
        "Add a biomes layer.",
    ));
    tools.push(ToolDef {
        id: "paint.mask",
        label: "Mask Paint",
        icon: Icon::CircleDot,
        mode: WorkspaceMode::Paint,
        action: ToolAction::Sculpt(EditorTool::PaintMask),
        description: "Paint into the active mask.",
        shortcut: None,
    });
    tools.push(stub(
        "paint.colour",
        "Colour Paint",
        Icon::Paintbrush,
        WorkspaceMode::Paint,
        "Colour paint (coming soon).",
    ));
    tools.push(stub(
        "paint.wetness",
        "Wetness",
        Icon::Droplets,
        WorkspaceMode::Paint,
        "Wetness paint (coming soon).",
    ));
    tools.push(stub(
        "paint.snow",
        "Snow",
        Icon::Sparkles,
        WorkspaceMode::Paint,
        "Snow coverage paint (coming soon).",
    ));
    tools.push(stub(
        "paint.rock",
        "Rock",
        Icon::Mountain,
        WorkspaceMode::Paint,
        "Rock material paint (coming soon).",
    ));
    tools.push(stub(
        "paint.grass",
        "Grass",
        Icon::Sparkles,
        WorkspaceMode::Paint,
        "Grass material paint (coming soon).",
    ));

    // —— Biomes ——————————————————————————————————————————————————
    tools.push(add(
        "biome.create",
        "Create Biome",
        Icon::Layers,
        WorkspaceMode::Biomes,
        "Biomes",
        LayerKind::Biomes(BiomesParams::default()),
        "Create a biome assignment layer.",
    ));
    tools.push(stub(
        "biome.terrain_rules",
        "Terrain Rules",
        Icon::Mountain,
        WorkspaceMode::Biomes,
        "Assign height/slope terrain rules.",
    ));
    tools.push(stub(
        "biome.material_rules",
        "Material Rules",
        Icon::Paintbrush,
        WorkspaceMode::Biomes,
        "Material rules for biomes.",
    ));
    tools.push(stub(
        "biome.veg_rules",
        "Vegetation Rules",
        Icon::Sparkles,
        WorkspaceMode::Biomes,
        "Vegetation rules for biomes.",
    ));
    tools.push(stub(
        "biome.water_rules",
        "Water Rules",
        Icon::Droplets,
        WorkspaceMode::Biomes,
        "Water rules for biomes.",
    ));
    tools.push(stub(
        "biome.climate",
        "Climate Rules",
        Icon::Sun,
        WorkspaceMode::Biomes,
        "Climate rules (coming soon).",
    ));

    // —— Scatter —————————————————————————————————————————————————
    tools.push(add(
        "scatter.trees",
        "Trees",
        Icon::Sparkles,
        WorkspaceMode::Scatter,
        "Forest",
        LayerKind::Vegetation(VegetationParams::default()),
        "Scatter tree vegetation.",
    ));
    tools.push(stub(
        "scatter.rocks",
        "Rocks",
        Icon::Mountain,
        WorkspaceMode::Scatter,
        "Rock scatter (coming soon).",
    ));
    tools.push(add(
        "scatter.grass",
        "Grass",
        Icon::Sparkles,
        WorkspaceMode::Scatter,
        "Grassland",
        LayerKind::Vegetation(VegetationParams {
            density: 0.6,
            ..Default::default()
        }),
        "Scatter grass vegetation.",
    ));
    tools.push(stub(
        "scatter.debris",
        "Debris",
        Icon::Package,
        WorkspaceMode::Scatter,
        "Debris scatter (coming soon).",
    ));
    tools.push(stub(
        "scatter.custom",
        "Custom Meshes",
        Icon::Box,
        WorkspaceMode::Scatter,
        "Custom mesh scatter (coming soon).",
    ));
    tools.push(stub(
        "scatter.density",
        "Density",
        Icon::Gauge,
        WorkspaceMode::Scatter,
        "Adjust scatter density (select a scatter layer).",
    ));
    tools.push(stub(
        "scatter.scale",
        "Scale Variation",
        Icon::Maximize2,
        WorkspaceMode::Scatter,
        "Scale variation settings.",
    ));
    tools.push(stub(
        "scatter.rotation",
        "Rotation Variation",
        Icon::Move,
        WorkspaceMode::Scatter,
        "Rotation variation settings.",
    ));
    tools.push(stub(
        "scatter.slope",
        "Slope Filtering",
        Icon::Activity,
        WorkspaceMode::Scatter,
        "Filter scatter by slope.",
    ));
    tools.push(stub(
        "scatter.height",
        "Height Filtering",
        Icon::Mountain,
        WorkspaceMode::Scatter,
        "Filter scatter by height.",
    ));

    tools
}

pub fn tools_for_mode(mode: WorkspaceMode) -> Vec<ToolDef> {
    all_tools().into_iter().filter(|t| t.mode == mode).collect()
}

/// Quick-Add searchable entries (wired tools that create layers + groups).
pub fn quick_add_entries() -> Vec<ToolDef> {
    all_tools()
        .into_iter()
        .filter(|t| matches!(t.action, ToolAction::AddLayer { .. }) && !t.is_stub())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mode_has_tools() {
        for mode in WorkspaceMode::ALL {
            assert!(
                !tools_for_mode(mode).is_empty(),
                "{:?} should have tools",
                mode
            );
        }
    }

    #[test]
    fn sculpt_mode_has_move_raise() {
        let ids: Vec<_> = tools_for_mode(WorkspaceMode::Sculpt)
            .iter()
            .map(|t| t.id)
            .collect();
        assert!(ids.contains(&"sculpt.move"));
        assert!(ids.contains(&"sculpt.raise"));
    }
}
