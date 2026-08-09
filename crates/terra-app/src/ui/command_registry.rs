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
    /// Developer / analysis overlays (not shown in default artist chrome).
    Debug,
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
            Self::Debug => "Debug",
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
    pub const SAVE_AS: &'static str = "file.save_as";
    pub const NEW_PROJECT: &'static str = "file.new";
    pub const OPEN_PROJECT: &'static str = "file.open";
    pub const CLOSE_PROJECT: &'static str = "file.close";
    pub const BAKE_SELECTED: &'static str = "layer.bake_selected";
    pub const OPEN_PROFILER: &'static str = "view.profiler";
    pub const TOGGLE_HISTORY: &'static str = "view.history";
    pub const TOGGLE_PIPELINE: &'static str = "view.pipeline";
    pub const CLEAR_GEOMORPH_DEBUG: &'static str = "debug.geomorph.clear";
    pub const WORKSPACE_WORLD: &'static str = "workspace.world";
    pub const WORKSPACE_SCULPT: &'static str = "workspace.sculpt";
    pub const WORKSPACE_BIOMES: &'static str = "workspace.biomes";
    pub const WORKSPACE_DEVELOP: &'static str = "workspace.develop";
    pub const WORKSPACE_RULES: &'static str = "workspace.rules";
    pub const WORKSPACE_SIMULATION: &'static str = "workspace.simulation";
    pub const WORKSPACE_SURFACE: &'static str = "workspace.surface";
    pub const WORKSPACE_OBJECTS: &'static str = "workspace.objects";
    pub const WORKSPACE_ALL_TOOLS: &'static str = "workspace.all_tools";
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
    let mut out = vec![
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
            name: "Switch Workspace: Sculpt",
            category: Mode,
            default_shortcut: Some("2"),
            icon: Some(Icon::Pencil),
        },
        CommandDef {
            id: CommandId::GENERATE,
            name: "Switch Workspace: Sculpt",
            category: Mode,
            default_shortcut: Some("2"),
            icon: Some(Icon::Mountain),
        },
        CommandDef {
            id: CommandId::EROSION,
            name: "Switch Workspace: Simulation",
            category: Mode,
            default_shortcut: Some("6"),
            icon: Some(Icon::Droplets),
        },
        CommandDef {
            id: CommandId::MASKS,
            name: "Switch Workspace: Rules",
            category: Mode,
            default_shortcut: Some("5"),
            icon: Some(Icon::CircleDot),
        },
        CommandDef {
            id: CommandId::PAINT,
            name: "Switch Workspace: Surface",
            category: Mode,
            default_shortcut: Some("7"),
            icon: Some(Icon::Paintbrush),
        },
        CommandDef {
            id: CommandId::BIOMES,
            name: "Switch Workspace: Biomes",
            category: Mode,
            default_shortcut: Some("3"),
            icon: Some(Icon::Layers),
        },
        CommandDef {
            id: CommandId::SCATTER,
            name: "Switch Workspace: Objects",
            category: Mode,
            default_shortcut: Some("8"),
            icon: Some(Icon::Sparkles),
        },
        CommandDef {
            id: CommandId::WORKSPACE_WORLD,
            name: "Switch Workspace: World",
            category: Mode,
            default_shortcut: Some("1"),
            icon: Some(Icon::Package),
        },
        CommandDef {
            id: CommandId::WORKSPACE_SCULPT,
            name: "Focus Sculpt Tools",
            category: Mode,
            default_shortcut: Some("2"),
            icon: Some(Icon::Pencil),
        },
        CommandDef {
            id: CommandId::WORKSPACE_BIOMES,
            name: "Focus Biome Tools",
            category: Mode,
            default_shortcut: Some("3"),
            icon: Some(Icon::Layers),
        },
        CommandDef {
            id: CommandId::WORKSPACE_DEVELOP,
            name: "Switch Workspace: Filters",
            category: Mode,
            default_shortcut: Some("4"),
            icon: Some(Icon::SlidersHorizontal),
        },
        CommandDef {
            id: CommandId::WORKSPACE_RULES,
            name: "Focus Mask Tools",
            category: Mode,
            default_shortcut: Some("5"),
            icon: Some(Icon::CircleDot),
        },
        CommandDef {
            id: CommandId::WORKSPACE_SIMULATION,
            name: "Focus Simulation Tools",
            category: Mode,
            default_shortcut: Some("6"),
            icon: Some(Icon::Droplets),
        },
        CommandDef {
            id: CommandId::WORKSPACE_SURFACE,
            name: "Focus Surface Tools",
            category: Mode,
            default_shortcut: Some("7"),
            icon: Some(Icon::Paintbrush),
        },
        CommandDef {
            id: CommandId::WORKSPACE_OBJECTS,
            name: "Focus Object Tools",
            category: Mode,
            default_shortcut: Some("8"),
            icon: Some(Icon::Sparkles),
        },
        // All Tools removed from the TOOLS rail — keep CommandId for old keybinds (no-op / remaps).
        CommandDef {
            id: CommandId::WORKSPACE_ALL_TOOLS,
            name: "Switch Workspace: Objects",
            category: Mode,
            default_shortcut: None,
            icon: Some(Icon::Package),
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
            id: CommandId::NEW_PROJECT,
            name: "New Project",
            category: File,
            default_shortcut: Some("Ctrl+N"),
            icon: Some(Icon::Plus),
        },
        CommandDef {
            id: CommandId::OPEN_PROJECT,
            name: "Open Project",
            category: File,
            default_shortcut: Some("Ctrl+O"),
            icon: Some(Icon::FolderOpen),
        },
        CommandDef {
            id: CommandId::SAVE,
            name: "Save Project",
            category: File,
            default_shortcut: Some("Ctrl+S"),
            icon: Some(Icon::Save),
        },
        CommandDef {
            id: CommandId::SAVE_AS,
            name: "Save Project As",
            category: File,
            default_shortcut: Some("Ctrl+Shift+S"),
            icon: Some(Icon::Save),
        },
        CommandDef {
            id: CommandId::CLOSE_PROJECT,
            name: "Close Project",
            category: File,
            default_shortcut: Some("Ctrl+W"),
            icon: Some(Icon::X),
        },
        CommandDef {
            id: CommandId::EXPORT,
            name: "Export",
            category: File,
            default_shortcut: None,
            icon: Some(Icon::Download),
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
            id: CommandId::TOGGLE_HISTORY,
            name: "Toggle History",
            category: View,
            default_shortcut: None,
            icon: Some(Icon::ScrollText),
        },
        CommandDef {
            id: CommandId::TOGGLE_PIPELINE,
            name: "Toggle Terrain Recipe",
            category: View,
            default_shortcut: None,
            icon: Some(Icon::Activity),
        },
        CommandDef {
            id: CommandId::CLEAR_GEOMORPH_DEBUG,
            name: "Clear Geomorph Debug Overlay",
            category: Debug,
            default_shortcut: None,
            icon: Some(Icon::EyeOff),
        },
    ];

    for field in terra_core::GeomorphDebugField::ALL {
        out.push(CommandDef {
            id: field.command_id(),
            name: field.label(),
            category: Debug,
            default_shortcut: None,
            icon: Some(Icon::Activity),
        });
    }
    out
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
