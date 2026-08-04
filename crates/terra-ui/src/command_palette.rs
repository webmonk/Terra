//! Global command palette and its editor-facing action bridge.

use crate::command_registry::{commands, fuzzy_match, CommandId};
use crate::panels::PanelAction;
use crate::tool_catalog::{all_tools, ToolAction};
use crate::{Preview2dMode, UiState, WorkspaceMode};
use terra_core::layer::Layer;
use terra_gui::style::{self, FONT_SCALE, PAD, ROW_H};
use terra_gui::{button_id, Color, GuiContext, Id, Rect};

#[derive(Debug, Default)]
pub struct CommandPaletteState {
    pub query: String,
    pub scroll_y: f32,
}

#[derive(Debug)]
pub enum PaletteAction {
    Panel(PanelAction),
    Undo,
    Redo,
    Export,
    CameraReset,
    CameraTopView,
    CameraFrameSelection,
}

pub fn draw_command_palette(
    ui: &mut GuiContext<'_>,
    ui_state: &mut UiState,
    state: &mut CommandPaletteState,
) -> Vec<PaletteAction> {
    if !ui_state.show_command_palette {
        return Vec::new();
    }
    if ui.input.backspace_pressed {
        state.query.pop();
    }
    state.query.push_str(&ui.input.text);
    if ui.input.escape_pressed {
        ui_state.show_command_palette = false;
        state.query.clear();
        return Vec::new();
    }

    let popup = Rect::from_pos_size(
        (ui.screen_w - 540.0).max(16.0) * 0.5,
        72.0,
        540.0_f32.min(ui.screen_w - 32.0),
        520.0_f32.min(ui.screen_h - 90.0),
    );
    let mut actions = Vec::new();
    ui.begin_overlay();
    ui.panel_rounded(popup, style::POPUP_BG, style::RADIUS_SM);
    ui.label_at(
        popup.min_x + PAD,
        popup.min_y + PAD,
        "Command Palette",
        style::TEXT,
        FONT_SCALE * 1.15,
    );
    let query_label = if state.query.is_empty() {
        "Search commands..."
    } else {
        &state.query
    };
    let search = Rect::from_pos_size(
        popup.min_x + PAD,
        popup.min_y + 38.0,
        popup.width() - PAD * 2.0,
        ROW_H,
    );
    ui.panel_rounded(search, style::SURFACE, style::RADIUS_SM);
    ui.label_at(
        search.min_x + 10.0,
        search.min_y + 5.0,
        query_label,
        style::TEXT_DIM,
        FONT_SCALE,
    );

    let list = Rect::from_min_max(
        popup.min_x + PAD,
        search.max_y + 8.0,
        popup.max_x - PAD,
        popup.max_y - PAD,
    );
    ui.begin_panel_scrolled(
        Id::new("command_palette_list"),
        list,
        Color::rgba(0.0, 0.0, 0.0, 0.0),
        &mut state.scroll_y,
    );
    for command in commands()
        .into_iter()
        .filter(|command| fuzzy_match(&state.query, command.name))
    {
        let shortcut = command.default_shortcut.unwrap_or("");
        let label = format!(
            "{}  {}    {}",
            command.category.label(),
            command.name,
            shortcut
        );
        if button_id(ui, Id::new("command_palette").child(command.id), &label) {
            execute_command(command.id, ui_state, &mut actions);
            ui_state.show_command_palette = false;
            state.query.clear();
            break;
        }
    }
    ui.end_panel_scrolled(&mut state.scroll_y);
    ui.end_overlay();

    if ui.input.primary_pressed && !ui.pointer_in(popup) {
        ui_state.show_command_palette = false;
        state.query.clear();
    }
    actions
}

fn execute_command(id: &str, ui_state: &mut UiState, actions: &mut Vec<PaletteAction>) {
    match id {
        CommandId::ADD_MOUNTAIN => add_catalog_layer("gen.mountain", actions),
        CommandId::ADD_HYDRAULIC_EROSION => add_catalog_layer("erosion.hydraulic", actions),
        CommandId::SCULPT => set_mode(ui_state, WorkspaceMode::Sculpt),
        CommandId::GENERATE => set_mode(ui_state, WorkspaceMode::Generate),
        CommandId::EROSION => set_mode(ui_state, WorkspaceMode::Erosion),
        CommandId::MASKS => set_mode(ui_state, WorkspaceMode::Masks),
        CommandId::PAINT => set_mode(ui_state, WorkspaceMode::Paint),
        CommandId::BIOMES => set_mode(ui_state, WorkspaceMode::Biomes),
        CommandId::SCATTER => set_mode(ui_state, WorkspaceMode::Scatter),
        CommandId::FRAME_TERRAIN => actions.push(PaletteAction::CameraReset),
        CommandId::TOP_VIEW => actions.push(PaletteAction::CameraTopView),
        CommandId::FRAME_SELECTION => actions.push(PaletteAction::CameraFrameSelection),
        CommandId::TOGGLE_HEIGHT_VIEW => {
            ui_state.preview_mode = if ui_state.preview_mode == Preview2dMode::Height {
                Preview2dMode::Lit
            } else {
                Preview2dMode::Height
            };
        }
        CommandId::TOGGLE_WIREFRAME => {
            ui_state.viewport_overlays.wireframe = !ui_state.viewport_overlays.wireframe
        }
        CommandId::OPEN_QUICK_ADD => ui_state.show_quick_add = true,
        CommandId::OPEN_COMMAND_PALETTE => ui_state.show_command_palette = true,
        CommandId::UNDO => actions.push(PaletteAction::Undo),
        CommandId::REDO => actions.push(PaletteAction::Redo),
        CommandId::EXPORT => actions.push(PaletteAction::Export),
        CommandId::BAKE_SELECTED => {
            ui_state.status = "Bake Selected is not implemented yet.".into()
        }
        CommandId::OPEN_PROFILER => ui_state.show_profiler = true,
        CommandId::TOGGLE_MINIMAP => ui_state.show_minimap = !ui_state.show_minimap,
        CommandId::TOGGLE_HISTORY => ui_state.show_history = !ui_state.show_history,
        CommandId::TOGGLE_PIPELINE => ui_state.show_pipeline = !ui_state.show_pipeline,
        _ => {}
    }
}

fn set_mode(ui_state: &mut UiState, mode: WorkspaceMode) {
    ui_state.workspace_mode = mode;
    if mode == WorkspaceMode::Sculpt && !ui_state.editor_tool.is_brush() {
        ui_state.editor_tool = crate::EditorTool::Move;
    }
}

fn add_catalog_layer(id: &str, actions: &mut Vec<PaletteAction>) {
    if let Some(tool) = all_tools().into_iter().find(|tool| tool.id == id) {
        if let ToolAction::AddLayer { name, kind } = tool.action {
            actions.push(PaletteAction::Panel(PanelAction::AddLayer(Layer::new(
                name, kind,
            ))));
        }
    }
}
