//! High-level workspace modes for the artist-focused tool workflow.

use terra_gui::Icon;

/// Primary editor mode — drives which tools appear in the contextual palette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum WorkspaceMode {
    #[default]
    Generate,
    Sculpt,
    Erosion,
    Masks,
    Paint,
    Biomes,
    Scatter,
}

impl WorkspaceMode {
    pub const ALL: [WorkspaceMode; 7] = [
        WorkspaceMode::Generate,
        WorkspaceMode::Sculpt,
        WorkspaceMode::Erosion,
        WorkspaceMode::Masks,
        WorkspaceMode::Paint,
        WorkspaceMode::Biomes,
        WorkspaceMode::Scatter,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WorkspaceMode::Generate => "Generate",
            WorkspaceMode::Sculpt => "Sculpt",
            WorkspaceMode::Erosion => "Erosion",
            WorkspaceMode::Masks => "Masks",
            WorkspaceMode::Paint => "Paint",
            WorkspaceMode::Biomes => "Biomes",
            WorkspaceMode::Scatter => "Scatter",
        }
    }

    /// Rail label — full readable names (never abbreviate Generate).
    pub fn short_label(self) -> &'static str {
        self.label()
    }

    pub fn tools_heading(self) -> &'static str {
        match self {
            WorkspaceMode::Generate => "GENERATE TOOLS",
            WorkspaceMode::Sculpt => "SCULPT TOOLS",
            WorkspaceMode::Erosion => "EROSION TOOLS",
            WorkspaceMode::Masks => "MASK TOOLS",
            WorkspaceMode::Paint => "PAINT TOOLS",
            WorkspaceMode::Biomes => "BIOME TOOLS",
            WorkspaceMode::Scatter => "SCATTER TOOLS",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            WorkspaceMode::Generate => Icon::Mountain,
            WorkspaceMode::Sculpt => Icon::Pencil,
            WorkspaceMode::Erosion => Icon::Droplets,
            WorkspaceMode::Masks => Icon::CircleDot,
            WorkspaceMode::Paint => Icon::Paintbrush,
            WorkspaceMode::Biomes => Icon::Layers,
            WorkspaceMode::Scatter => Icon::Sparkles,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            WorkspaceMode::Generate => "Procedural terrain generators and base shapes.",
            WorkspaceMode::Sculpt => "Paint and reshape height with brushes.",
            WorkspaceMode::Erosion => "Hydraulic, thermal, and weathering simulations.",
            WorkspaceMode::Masks => "Create and combine height, slope, and painted masks.",
            WorkspaceMode::Paint => "Paint height, materials, biomes, and surface attributes.",
            WorkspaceMode::Biomes => "Define climate and biome assignment rules.",
            WorkspaceMode::Scatter => "Place trees, rocks, and other vegetation.",
        }
    }

    /// Keyboard digit 1–7 → mode.
    pub fn from_digit(digit: u8) -> Option<Self> {
        match digit {
            1 => Some(WorkspaceMode::Generate),
            2 => Some(WorkspaceMode::Sculpt),
            3 => Some(WorkspaceMode::Erosion),
            4 => Some(WorkspaceMode::Masks),
            5 => Some(WorkspaceMode::Paint),
            6 => Some(WorkspaceMode::Biomes),
            7 => Some(WorkspaceMode::Scatter),
            _ => None,
        }
    }

    pub fn digit_shortcut(self) -> u8 {
        match self {
            WorkspaceMode::Generate => 1,
            WorkspaceMode::Sculpt => 2,
            WorkspaceMode::Erosion => 3,
            WorkspaceMode::Masks => 4,
            WorkspaceMode::Paint => 5,
            WorkspaceMode::Biomes => 6,
            WorkspaceMode::Scatter => 7,
        }
    }

    pub fn shortcut_label(self) -> &'static str {
        match self {
            WorkspaceMode::Generate => "1",
            WorkspaceMode::Sculpt => "2",
            WorkspaceMode::Erosion => "3",
            WorkspaceMode::Masks => "4",
            WorkspaceMode::Paint => "5",
            WorkspaceMode::Biomes => "6",
            WorkspaceMode::Scatter => "7",
        }
    }
}

/// High-level application workspace tabs (reconfigure panels; not separate apps).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum AppWorkspace {
    #[default]
    Terrain,
    Materials,
    Water,
    Biomes,
    Vegetation,
}

impl AppWorkspace {
    pub const ALL: [AppWorkspace; 5] = [
        AppWorkspace::Terrain,
        AppWorkspace::Materials,
        AppWorkspace::Water,
        AppWorkspace::Biomes,
        AppWorkspace::Vegetation,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AppWorkspace::Terrain => "Terrain",
            AppWorkspace::Materials => "Materials",
            AppWorkspace::Water => "Water",
            AppWorkspace::Biomes => "Biomes",
            AppWorkspace::Vegetation => "Vegetation",
        }
    }
}
