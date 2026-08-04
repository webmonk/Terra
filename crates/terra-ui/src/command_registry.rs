//! Central definition of editor commands and their default shortcuts.

use terra_gui::Icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandCategory {
    File,
    Edit,
    View,
    Layer,
    Mode,
    Viewport,
    Tools,
}

impl CommandCategory {
    pub const fn label(self) -> &'static str {
        match self {
            Self::File => "File",
            Self::Edit => "Edit",
            Self::View => "View",
            Self::Layer => "Layer",
            Self::Mode => "Mode",
            Self::Viewport => "Viewport",
            Self::Tools => "Tools",
        }
    }
}

/// Stable IDs shared by keyboard dispatch and the command palette.
pub struct CommandId;

impl CommandId {
    pub const ADD_MOUNTAIN: &'static str = "layer.add_mountain";
    pub const ADD_HYDRAULIC_EROSION: &'static str = "layer.add_hydraulic_erosion";
    pub const SCULPT: &'static str = "mode.sculpt";
    pub const GENERATE: &'static str = "mode.generate";
    pub const EROSION: &'static str = "mode.erosion";
    pub const MASKS: &'static str = "mode.masks";
    pub const PAINT: &'static str = "mode.paint";
    pub const BIOMES: &'static str = "mode.biomes";
    pub const SCATTER: &'static str = "mode.scatter";
    pub const FRAME_TERRAIN: &'static str = "viewport.frame_terrain";
    pub const TOP_VIEW: &'static str = "viewport.top_view";
    pub const FRAME_SELECTION: &'static str = "viewport.frame_selection";
    pub const TOGGLE_HEIGHT_VIEW: &'static str = "view.toggle_height";
    pub const TOGGLE_WIREFRAME: &'static str = "view.toggle_wireframe";
    pub const OPEN_QUICK_ADD: &'static str = "tools.quick_add";
    pub const OPEN_COMMAND_PALETTE: &'static str = "tools.command_palette";
    pub const UNDO: &'static str = "edit.undo";
    pub const REDO: &'static str = "edit.redo";
    pub const EXPORT: &'static str = "file.export";
    pub const SAVE: &'static str = "file.save";
    pub const BAKE_SELECTED: &'static str = "layer.bake_selected";
    pub const OPEN_PROFILER: &'static str = "view.profiler";
    pub const TOGGLE_MINIMAP: &'static str = "view.minimap";
    pub const TOGGLE_HISTORY: &'static str = "view.history";
    pub const TOGGLE_PIPELINE: &'static str = "view.pipeline";
}

#[derive(Debug, Clone, Copy)]
pub struct CommandDef {
    pub id: &'static str,
    pub name: &'static str,
    pub category: CommandCategory,
    pub default_shortcut: Option<&'static str>,
    pub icon: Option<Icon>,
}

/// Built-in commands presented by the command palette.
pub fn commands() -> Vec<CommandDef> {
    use CommandCategory::*;
    vec![
        CommandDef {
            id: CommandId::ADD_MOUNTAIN,
            name: "Add Mountain",
            category: Layer,
            default_shortcut: None,
            icon: Some(Icon::Mountain),
        },
        CommandDef {
            id: CommandId::ADD_HYDRAULIC_EROSION,
            name: "Add Hydraulic Erosion",
            category: Layer,
            default_shortcut: None,
            icon: Some(Icon::Droplets),
        },
        CommandDef {
            id: CommandId::SCULPT,
            name: "Switch to Sculpt",
            category: Mode,
            default_shortcut: Some("2"),
            icon: Some(Icon::Pencil),
        },
        CommandDef {
            id: CommandId::GENERATE,
            name: "Switch to Generate",
            category: Mode,
            default_shortcut: Some("1"),
            icon: Some(Icon::Mountain),
        },
        CommandDef {
            id: CommandId::EROSION,
            name: "Switch to Erosion",
            category: Mode,
            default_shortcut: Some("3"),
            icon: Some(Icon::Droplets),
        },
        CommandDef {
            id: CommandId::MASKS,
            name: "Switch to Masks",
            category: Mode,
            default_shortcut: Some("4"),
            icon: Some(Icon::CircleDot),
        },
        CommandDef {
            id: CommandId::PAINT,
            name: "Switch to Paint",
            category: Mode,
            default_shortcut: Some("5"),
            icon: Some(Icon::Paintbrush),
        },
        CommandDef {
            id: CommandId::BIOMES,
            name: "Switch to Biomes",
            category: Mode,
            default_shortcut: Some("6"),
            icon: Some(Icon::Layers),
        },
        CommandDef {
            id: CommandId::SCATTER,
            name: "Switch to Scatter",
            category: Mode,
            default_shortcut: Some("7"),
            icon: Some(Icon::Sparkles),
        },
        CommandDef {
            id: CommandId::FRAME_TERRAIN,
            name: "Frame Terrain",
            category: Viewport,
            default_shortcut: Some("F"),
            icon: Some(Icon::Camera),
        },
        CommandDef {
            id: CommandId::TOP_VIEW,
            name: "Top View",
            category: Viewport,
            default_shortcut: None,
            icon: Some(Icon::Camera),
        },
        CommandDef {
            id: CommandId::FRAME_SELECTION,
            name: "Frame Selection",
            category: Viewport,
            default_shortcut: None,
            icon: Some(Icon::Camera),
        },
        CommandDef {
            id: CommandId::TOGGLE_HEIGHT_VIEW,
            name: "Toggle Height View",
            category: View,
            default_shortcut: None,
            icon: Some(Icon::Image),
        },
        CommandDef {
            id: CommandId::TOGGLE_WIREFRAME,
            name: "Toggle Wireframe",
            category: Viewport,
            default_shortcut: None,
            icon: Some(Icon::Grid3x3),
        },
        CommandDef {
            id: CommandId::OPEN_QUICK_ADD,
            name: "Open Quick Add",
            category: Tools,
            default_shortcut: Some("Ctrl+L / Insert"),
            icon: Some(Icon::Plus),
        },
        CommandDef {
            id: CommandId::OPEN_COMMAND_PALETTE,
            name: "Open Command Palette",
            category: Tools,
            default_shortcut: Some("Ctrl+P"),
            icon: Some(Icon::ScrollText),
        },
        CommandDef {
            id: CommandId::UNDO,
            name: "Undo",
            category: Edit,
            default_shortcut: Some("Ctrl+Z"),
            icon: Some(Icon::Undo2),
        },
        CommandDef {
            id: CommandId::REDO,
            name: "Redo",
            category: Edit,
            default_shortcut: Some("Ctrl+Y"),
            icon: Some(Icon::Redo2),
        },
        CommandDef {
            id: CommandId::EXPORT,
            name: "Export",
            category: File,
            default_shortcut: None,
            icon: Some(Icon::Download),
        },
        CommandDef {
            id: CommandId::SAVE,
            name: "Save Project",
            category: File,
            default_shortcut: Some("Ctrl+S"),
            icon: Some(Icon::Save),
        },
        CommandDef {
            id: CommandId::BAKE_SELECTED,
            name: "Bake Selected",
            category: Layer,
            default_shortcut: None,
            icon: Some(Icon::Layers),
        },
        CommandDef {
            id: CommandId::OPEN_PROFILER,
            name: "Open Profiler",
            category: View,
            default_shortcut: None,
            icon: Some(Icon::Gauge),
        },
        CommandDef {
            id: CommandId::TOGGLE_MINIMAP,
            name: "Toggle Minimap",
            category: View,
            default_shortcut: None,
            icon: Some(Icon::Image),
        },
        CommandDef {
            id: CommandId::TOGGLE_HISTORY,
            name: "Toggle History",
            category: View,
            default_shortcut: None,
            icon: Some(Icon::ScrollText),
        },
        CommandDef {
            id: CommandId::TOGGLE_PIPELINE,
            name: "Toggle Pipeline",
            category: View,
            default_shortcut: None,
            icon: Some(Icon::Activity),
        },
    ]
}

/// Case-insensitive substring or ordered-subsequence matching.
pub fn fuzzy_match(query: &str, text: &str) -> bool {
    let query = query.trim().to_lowercase();
    let text = text.to_lowercase();
    if query.is_empty() || text.contains(&query) {
        return true;
    }
    let mut chars = text.chars();
    query
        .chars()
        .all(|needle| chars.by_ref().any(|hay| hay == needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_match_handles_subsequences_and_case() {
        assert!(fuzzy_match("hyd ero", "Add Hydraulic Erosion"));
        assert!(fuzzy_match("MOUN", "Add Mountain"));
        assert!(!fuzzy_match("xyz", "Add Mountain"));
    }

    #[test]
    fn command_list_is_populated() {
        assert!(!commands().is_empty());
    }
}
