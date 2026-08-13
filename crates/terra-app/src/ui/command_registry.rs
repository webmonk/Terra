//! Central definition of editor commands and their default shortcuts.

use crate::ui::workspace::{all_workspace_definitions, WorkspaceId};
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
}

#[derive(Debug, Clone, Copy)]
pub struct CommandDef {
    pub id: &'static str,
    pub name: &'static str,
    pub category: CommandCategory,
    pub default_shortcut: Option<&'static str>,
    pub icon: Option<Icon>,
}

const LEGACY_WORKSPACE_COMMAND_ALIASES: &[(&str, WorkspaceId)] = &[
    ("mode.sculpt", WorkspaceId::Sculpt),
    ("mode.generate", WorkspaceId::Sculpt),
    ("mode.erosion", WorkspaceId::Simulation),
    ("mode.masks", WorkspaceId::Rules),
    ("mode.paint", WorkspaceId::Surface),
    ("mode.biomes", WorkspaceId::Biomes),
    ("mode.scatter", WorkspaceId::Objects),
    ("workspace.all_tools", WorkspaceId::AllTools),
];

/// Resolve canonical and legacy workspace command ids without exposing aliases
/// in the visible command list.
pub(crate) fn resolve_workspace_command(id: &str) -> Option<WorkspaceId> {
    all_workspace_definitions()
        .iter()
        .find(|definition| definition.id.command_id() == Some(id))
        .map(|definition| definition.id)
        .or_else(|| {
            LEGACY_WORKSPACE_COMMAND_ALIASES
                .iter()
                .find_map(|(alias, workspace)| (*alias == id).then_some(*workspace))
        })
}

fn digit_shortcut_label(digit: u8) -> Option<&'static str> {
    match digit {
        1 => Some("1"),
        2 => Some("2"),
        3 => Some("3"),
        4 => Some("4"),
        5 => Some("5"),
        6 => Some("6"),
        7 => Some("7"),
        8 => Some("8"),
        9 => Some("9"),
        _ => None,
    }
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
    ];

    for definition in all_workspace_definitions() {
        let (Some(id), Some(name)) = (definition.id.command_id(), definition.command_name) else {
            continue;
        };
        out.push(CommandDef {
            id,
            name,
            category: Mode,
            default_shortcut: definition
                .id
                .digit_shortcut()
                .and_then(digit_shortcut_label),
            icon: Some(definition.icon),
        });
    }

    out.extend([
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
    ]);

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
    use std::collections::HashSet;

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

    #[test]
    fn visible_command_ids_and_default_shortcuts_are_unique() {
        let commands = commands();
        let mut ids = HashSet::new();
        let mut shortcuts = HashSet::new();

        for command in &commands {
            assert!(
                ids.insert(command.id),
                "duplicate command id: {}",
                command.id
            );
            if let Some(shortcut) = command.default_shortcut {
                assert!(
                    shortcuts.insert(shortcut),
                    "duplicate default shortcut: {shortcut}"
                );
            }
        }
    }

    #[test]
    fn visible_workspace_commands_come_from_workspace_metadata() {
        let commands = commands();
        let visible_definitions: Vec<_> = all_workspace_definitions()
            .iter()
            .filter(|definition| definition.id.command_id().is_some())
            .collect();

        for definition in &visible_definitions {
            let id = definition.id.command_id().unwrap();
            let command = commands
                .iter()
                .find(|command| command.id == id)
                .unwrap_or_else(|| panic!("missing workspace command: {id}"));

            assert_eq!(command.name, definition.command_name.unwrap());
            assert_eq!(command.icon, Some(definition.icon));
            assert_eq!(
                command
                    .default_shortcut
                    .and_then(|value| value.parse().ok()),
                definition.id.digit_shortcut()
            );
            assert_eq!(command.category, CommandCategory::Mode);
        }

        let mut targets = HashSet::new();
        let visible_workspace_commands: Vec<_> = commands
            .iter()
            .filter_map(|command| {
                resolve_workspace_command(command.id).map(|workspace| (command, workspace))
            })
            .collect();
        for (command, workspace) in &visible_workspace_commands {
            assert!(
                targets.insert(*workspace),
                "duplicate visible workspace target {:?} from command {}",
                workspace,
                command.id
            );
        }

        assert_eq!(visible_workspace_commands.len(), visible_definitions.len());
        assert_eq!(targets.len(), 8);
    }

    #[test]
    fn legacy_workspace_aliases_resolve_but_are_not_visible() {
        let commands = commands();

        for &(alias, expected) in LEGACY_WORKSPACE_COMMAND_ALIASES {
            assert_eq!(resolve_workspace_command(alias), Some(expected));
            assert!(
                commands.iter().all(|command| command.id != alias),
                "legacy alias is visible: {alias}"
            );
        }

        assert_eq!(
            resolve_workspace_command("workspace.all_tools"),
            Some(WorkspaceId::AllTools)
        );
        assert!(WorkspaceId::AllTools.command_id().is_none());
    }
}
