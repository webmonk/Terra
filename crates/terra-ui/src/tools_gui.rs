//! Mode rail + contextual tool palette for the active workspace mode.

use crate::panels::PanelAction;
use crate::tool_catalog::{tools_for_mode, ToolAction, ToolDef};
use crate::workspace::WorkspaceMode;
use crate::{EditorTool, UiState};
use terra_core::document::TerrainDocument;
use terra_core::layer::Layer;
use terra_gui::style::{self, FONT_SCALE, MODE_ROW_H, PAD, TOOL_ROW_H};
use terra_gui::{Color, DrawList, GuiContext, Icon, Id, Rect};

#[derive(Debug, Default)]
pub struct ToolsGuiState {
    pub scroll_y: f32,
}

/// In-flight drag of a layer tool toward the viewport.
#[derive(Debug, Clone)]
pub struct LayerToolDrag {
    pub name: &'static str,
    pub kind: terra_core::layer::LayerKind,
}

pub fn draw_tools_gui(
    ui: &mut GuiContext<'_>,
    doc: &TerrainDocument,
    ui_state: &mut UiState,
    state: &mut ToolsGuiState,
) -> Vec<PanelAction> {
    let mut actions = Vec::new();
    draw_mode_rail(ui, ui_state);
    draw_tool_panel(ui, doc, ui_state, state, &mut actions);
    finish_tool_drag(ui, ui_state, &mut actions);
    actions
}

fn draw_mode_rail(ui: &mut GuiContext<'_>, ui_state: &mut UiState) {
    let rail = ui.mode_rail_rect();
    if ui
        .input
        .pointer
        .map(|(x, y)| rail.contains(x, y))
        .unwrap_or(false)
    {
        ui.state.set_hot(Id::new("__mode_rail_bg"));
    }

    ui.panel(rail, style::MODE_RAIL_BG);
    ui.panel(
        Rect::from_pos_size(rail.max_x - 1.0, rail.min_y, 1.0, rail.height()),
        style::SEPARATOR,
    );

    let mut y = rail.min_y + 8.0;
    for mode in WorkspaceMode::ALL {
        let row = Rect::from_pos_size(rail.min_x + 4.0, y, rail.width() - 8.0, MODE_ROW_H);
        let id = Id::new("mode").child(mode.label());
        let selected = ui_state.workspace_mode == mode;
        let hovered = ui.pointer_in(row);
        if hovered {
            ui.state.set_hot(id);
        }
        if hovered && ui.input.primary_pressed {
            ui.state.active = Some(id);
        }
        if ui.input.primary_released && ui.state.is_active(id) && hovered {
            ui_state.workspace_mode = mode;
            ui_state.tool_drag = None;
            if mode == WorkspaceMode::Sculpt && !ui_state.editor_tool.is_brush() {
                ui_state.editor_tool = EditorTool::Move;
            }
        }

        if selected {
            ui.panel_rounded(row, style::ACCENT_SOFT, style::RADIUS_MD);
            ui.panel(
                Rect::from_pos_size(row.min_x, row.min_y + 8.0, 3.0, row.height() - 16.0),
                style::ACCENT,
            );
        } else if hovered {
            ui.panel_rounded(row, style::HOVER_BG, style::RADIUS_MD);
        }

        let color = if selected {
            style::ACCENT
        } else if hovered {
            style::TEXT
        } else {
            style::TEXT_DIM
        };
        let icon = mode.icon();
        ui.icon_at(
            row.min_x + (row.width() - style::ICON_LG) * 0.5,
            row.min_y + 8.0,
            icon,
            color,
            style::ICON_LG,
        );
        let label = mode.label();
        let tw = DrawList::text_width(label, FONT_SCALE * 0.72);
        ui.label_at(
            row.min_x + (row.width() - tw) * 0.5,
            row.min_y + 32.0,
            label,
            if selected {
                style::TEXT
            } else {
                style::TEXT_MUTED
            },
            FONT_SCALE * 0.72,
        );

        if hovered {
            ui.queue_tooltip(
                row,
                mode.label(),
                mode.description(),
                Some(mode.shortcut_label()),
            );
        }

        y += MODE_ROW_H + 4.0;
    }
}

fn draw_tool_panel(
    ui: &mut GuiContext<'_>,
    doc: &TerrainDocument,
    ui_state: &mut UiState,
    state: &mut ToolsGuiState,
    actions: &mut Vec<PanelAction>,
) {
    if ui_state.layout.tool_panel_collapsed {
        // Expand affordance on the mode-rail edge.
        let rail = ui.mode_rail_rect();
        let btn = Rect::from_pos_size(rail.max_x - 2.0, rail.min_y + 8.0, 18.0, 24.0);
        let id = Id::new("tools_expand");
        let hovered = ui.pointer_in(btn);
        if hovered {
            ui.state.set_hot(id);
        }
        if hovered && ui.input.primary_pressed {
            ui.state.active = Some(id);
        }
        if ui.input.primary_released && ui.state.is_active(id) && hovered {
            ui_state.layout.tool_panel_collapsed = false;
            ui_state.layout_dirty = true;
        }
        ui.panel_rounded(btn, style::BUTTON_BG, style::RADIUS_SM);
        ui.icon_at(btn.min_x + 2.0, btn.min_y + 4.0, Icon::ChevronRight, style::TEXT_DIM, 14.0);
        return;
    }

    let panel = ui.tool_panel_rect();
    if panel.width() < 40.0 {
        return;
    }

    if ui
        .input
        .pointer
        .map(|(x, y)| panel.contains(x, y))
        .unwrap_or(false)
    {
        ui.state.set_hot(Id::new("__tools_bg"));
    }

    ui.panel(panel, style::PANEL_BG);
    ui.panel(
        Rect::from_pos_size(panel.max_x - 1.0, panel.min_y, 1.0, panel.height()),
        style::SEPARATOR,
    );

    let mode = ui_state.workspace_mode;
    let search_h = 32.0;
    let recent_h = if panel.height() > 320.0 { 120.0 } else { 0.0 };
    let footer_h = search_h + recent_h + 12.0;

    let header = Rect::from_pos_size(panel.min_x, panel.min_y, panel.width(), style::HEADER_H);
    ui.label_at(
        header.min_x + PAD,
        header.min_y + 11.0,
        mode.tools_heading(),
        style::TEXT_MUTED,
        FONT_SCALE * 0.78,
    );
    // Collapse tool panel.
    let collapse = Rect::from_pos_size(header.max_x - 28.0, header.min_y + 6.0, 22.0, 22.0);
    let cid = Id::new("tools_collapse");
    if ui.pointer_in(collapse) {
        ui.state.set_hot(cid);
        if ui.input.primary_pressed {
            ui.state.active = Some(cid);
        }
        if ui.input.primary_released && ui.state.is_active(cid) {
            ui_state.layout.tool_panel_collapsed = true;
            ui_state.layout_dirty = true;
        }
    }
    ui.icon_at(
        collapse.min_x + 4.0,
        collapse.min_y + 4.0,
        Icon::ChevronDown,
        style::TEXT_MUTED,
        14.0,
    );

    let list = Rect::from_min_max(
        panel.min_x,
        header.max_y,
        panel.max_x,
        panel.max_y - footer_h,
    );
    ui.begin_panel_scrolled(
        Id::new("tools_scroll"),
        list,
        Color::rgba(0.0, 0.0, 0.0, 0.0),
        &mut state.scroll_y,
    );

    let tools = tools_for_mode(mode);
    let query = ui_state.tool_search.to_lowercase();
    for (i, tool) in tools.iter().enumerate() {
        if matches!(tool.action, ToolAction::Sculpt(EditorTool::PaintMask)) {
            let upper = doc
                .selected
                .and_then(|id| doc.stack.find(id))
                .is_some_and(|l| !l.kind.is_sculpt_base());
            if !upper && mode != WorkspaceMode::Masks && mode != WorkspaceMode::Paint {
                continue;
            }
        }
        if !query.is_empty()
            && !tool.label.to_lowercase().contains(&query)
            && !tool.description.to_lowercase().contains(&query)
        {
            continue;
        }
        tool_button(
            ui,
            ui_state,
            doc,
            actions,
            Id::new("tool").with(i as u64).child(tool.id),
            tool,
        );
    }

    ui.end_panel_scrolled(&mut state.scroll_y);

    // Search + Recent footer.
    let footer = Rect::from_pos_size(panel.min_x, panel.max_y - footer_h, panel.width(), footer_h);
    ui.panel(
        Rect::from_pos_size(footer.min_x, footer.min_y, footer.width(), 1.0),
        style::SEPARATOR,
    );

    let search = Rect::from_pos_size(
        footer.min_x + 8.0,
        footer.min_y + 8.0,
        footer.width() - 16.0,
        26.0,
    );
    draw_search_field(ui, ui_state, search);

    if recent_h > 0.0 {
        let recent_y = search.max_y + 8.0;
        ui.label_at(
            footer.min_x + PAD,
            recent_y,
            "RECENT",
            style::TEXT_MUTED,
            FONT_SCALE * 0.72,
        );
        let mut ry = recent_y + 16.0;
        let recent_ids: Vec<String> = ui_state.recent_tools.iter().take(4).cloned().collect();
        for (i, id) in recent_ids.iter().enumerate() {
            let label = resolve_tool_label(id);
            let icon = resolve_tool_icon(id);
            let row = Rect::from_pos_size(footer.min_x + 6.0, ry, footer.width() - 12.0, 22.0);
            let rid = Id::new("recent_tool").with(i as u64);
            let hovered = ui.pointer_in(row);
            if hovered {
                ui.state.set_hot(rid);
                ui.panel_rounded(row, style::HOVER_BG, style::RADIUS_SM);
            }
            ui.icon_at(row.min_x + 4.0, row.min_y + 3.0, icon, style::TEXT_DIM, 14.0);
            ui.label_at(
                row.min_x + 24.0,
                row.min_y + 4.0,
                label,
                style::TEXT_DIM,
                FONT_SCALE * 0.85,
            );
            if hovered && ui.input.primary_pressed {
                if let Some(tool) = find_tool(id) {
                    activate_tool(ui_state, doc, actions, &tool);
                }
            }
            ry += 24.0;
            if ry + 22.0 > footer.max_y - 4.0 {
                break;
            }
        }
    }
}

fn draw_search_field(ui: &mut GuiContext<'_>, ui_state: &mut UiState, rect: Rect) {
    let id = Id::new("tool_search");
    let hovered = ui.pointer_in(rect);
    if hovered {
        ui.state.set_hot(id);
    }
    if hovered && ui.input.primary_pressed {
        ui.state.active = Some(id);
        ui.state.text_focus = Some(id);
        ui.state.text_buffer = ui_state.tool_search.clone();
    }
    let focused = ui.state.text_focus == Some(id);
    ui.panel_rounded(rect, style::INPUT_BG, style::RADIUS_SM);
    ui.icon_at(
        rect.min_x + 6.0,
        rect.min_y + 5.0,
        Icon::Search,
        style::TEXT_MUTED,
        14.0,
    );
    if focused {
        if !ui.input.text.is_empty() {
            ui.state.text_buffer.push_str(&ui.input.text);
            ui_state.tool_search = ui.state.text_buffer.clone();
        }
        if ui.input.backspace_pressed {
            ui.state.text_buffer.pop();
            ui_state.tool_search = ui.state.text_buffer.clone();
        }
        if ui.input.escape_pressed {
            ui_state.tool_search.clear();
            ui.state.clear_text_focus();
        }
    }
    let placeholder = ui_state.tool_search.is_empty() && !focused;
    let display = if placeholder {
        "Search tools...".to_string()
    } else if focused {
        ui.state.text_buffer.clone()
    } else {
        ui_state.tool_search.clone()
    };
    ui.label_at(
        rect.min_x + 26.0,
        rect.min_y + 5.0,
        &display,
        if placeholder {
            style::TEXT_MUTED
        } else {
            style::TEXT
        },
        FONT_SCALE * 0.9,
    );
}

fn tool_button(
    ui: &mut GuiContext<'_>,
    ui_state: &mut UiState,
    doc: &TerrainDocument,
    actions: &mut Vec<PanelAction>,
    id: Id,
    tool: &ToolDef,
) {
    let row = ui.allocate(TOOL_ROW_H);
    let inner = shrink_rect(row, 3.0);
    let hovered = ui.pointer_in(inner);
    let stub = tool.is_stub();
    let selected = match &tool.action {
        ToolAction::Sculpt(t) => ui_state.editor_tool == *t,
        ToolAction::AddLayer { name, .. } => {
            ui_state.tool_drag.as_ref().is_some_and(|d| d.name == *name)
        }
        ToolAction::Stub => false,
    };

    if hovered {
        ui.state.set_hot(id);
    }

    if hovered && !stub && ui.input.primary_pressed {
        ui.state.active = Some(id);
        match &tool.action {
            ToolAction::AddLayer { name, kind } => {
                ui_state.tool_drag = Some(LayerToolDrag {
                    name,
                    kind: kind.clone(),
                });
            }
            ToolAction::Sculpt(_) | ToolAction::Stub => {}
        }
    }

    if !stub && ui.input.primary_released && ui.state.is_active(id) && hovered {
        if let ToolAction::Sculpt(editor_tool) = tool.action {
            apply_sculpt_tool(ui_state, doc, actions, editor_tool);
            push_recent(ui_state, tool.id);
        }
    }

    let bg = if selected {
        style::SELECTED_BG
    } else if stub {
        Color::rgba(0.0, 0.0, 0.0, 0.0)
    } else if hovered {
        style::HOVER_BG
    } else {
        Color::rgba(0.0, 0.0, 0.0, 0.0)
    };
    if bg.a > 0.0 || selected {
        ui.panel_rounded(inner, bg, style::RADIUS_SM);
    }
    if selected {
        ui.panel(
            Rect::from_pos_size(inner.min_x, inner.min_y + 6.0, 3.0, inner.height() - 12.0),
            style::ACCENT,
        );
    }

    let color = if stub {
        style::TEXT_DISABLED
    } else if selected {
        style::ACCENT
    } else {
        style::TEXT_DIM
    };
    ui.icon_at(
        inner.min_x + 10.0,
        inner.min_y + (inner.height() - style::ICON_MD) * 0.5,
        tool.icon,
        color,
        style::ICON_MD,
    );
    ui.label_in_rect(
        Rect::from_pos_size(inner.min_x + 36.0, inner.min_y, inner.width() - 40.0, inner.height()),
        tool.label,
        if stub {
            style::TEXT_DISABLED
        } else if selected {
            style::TEXT
        } else {
            style::TEXT_DIM
        },
        FONT_SCALE * 0.95,
    );

    if hovered {
        ui.queue_tooltip(row, tool.label, tool.description, tool.shortcut);
    }
}

fn activate_tool(
    ui_state: &mut UiState,
    doc: &TerrainDocument,
    actions: &mut Vec<PanelAction>,
    tool: &ToolDef,
) {
    match &tool.action {
        ToolAction::Sculpt(t) => {
            apply_sculpt_tool(ui_state, doc, actions, *t);
            push_recent(ui_state, tool.id);
        }
        ToolAction::AddLayer { name, kind } => {
            actions.push(PanelAction::AddLayer(Layer::new(*name, kind.clone())));
            push_recent(ui_state, tool.id);
        }
        ToolAction::Stub => {}
    }
}

fn push_recent(ui_state: &mut UiState, id: &str) {
    ui_state.recent_tools.retain(|t| t != id);
    ui_state.recent_tools.insert(0, id.to_string());
    ui_state.recent_tools.truncate(8);
}

fn find_tool(id: &str) -> Option<ToolDef> {
    crate::tool_catalog::all_tools()
        .into_iter()
        .find(|t| t.id == id)
}

fn resolve_tool_label(id: &str) -> &'static str {
    match id {
        "erosion.hydraulic" => "Hydraulic",
        "erosion.thermal" => "Thermal",
        "erosion.coastal" => "Coastal",
        "erosion.wind" => "Wind",
        "erosion.river" => "River Carve",
        "erosion.sediment" => "Sediment",
        "erosion.talus" => "Talus",
        "erosion.weathering" | "erosion.blur" => "Weathering",
        "sculpt.raise" => "Raise",
        "sculpt.lower" => "Lower",
        "sculpt.smooth" => "Smooth",
        "sculpt.move" => "Move",
        "gen.mountain" => "Mountain",
        _ => "Tool",
    }
}

fn resolve_tool_icon(id: &str) -> Icon {
    match id {
        "erosion.hydraulic" | "erosion.coastal" => Icon::Droplets,
        "erosion.thermal" | "erosion.talus" => Icon::Sun,
        "erosion.river" | "erosion.wind" => Icon::Waves,
        "erosion.weathering" | "erosion.blur" => Icon::Sparkles,
        "sculpt.raise" | "sculpt.lower" | "sculpt.smooth" | "sculpt.move" => Icon::Pencil,
        "gen.mountain" => Icon::Mountain,
        _ => Icon::CircleDot,
    }
}

fn apply_sculpt_tool(
    ui_state: &mut UiState,
    doc: &TerrainDocument,
    actions: &mut Vec<PanelAction>,
    tool: EditorTool,
) {
    ui_state.editor_tool = tool;
    ui_state.tool_drag = None;
    if tool.is_move() {
        return;
    }
    if tool.is_sculpt() {
        ui_state.paint_mask = None;
        if let Some(base) = doc
            .stack
            .flatten_layers()
            .iter()
            .find(|l| l.kind.is_sculpt_base())
        {
            actions.push(PanelAction::Select(base.id()));
        }
    } else if tool == EditorTool::PaintMask {
        if let Some(layer_id) = doc.selected {
            if let Some(layer) = doc.stack.find(layer_id) {
                if let Some(binding) = layer.common.masks.first() {
                    ui_state.paint_mask = Some(binding.id);
                    ui_state.selected_mask = Some(binding.id);
                } else {
                    ui_state.show_mask_editor = true;
                }
            }
        }
    }
}

fn finish_tool_drag(
    ui: &mut GuiContext<'_>,
    ui_state: &mut UiState,
    actions: &mut Vec<PanelAction>,
) {
    if ui.input.primary_released {
        if let Some(drag) = ui_state.tool_drag.take() {
            let over_vp = ui
                .input
                .pointer
                .is_some_and(|(x, y)| ui.viewport_rect().contains(x, y));
            let over_tools = ui
                .input
                .pointer
                .is_some_and(|(x, y)| ui.tool_panel_rect().contains(x, y));
            if over_vp || over_tools {
                // Record recent for AddLayer tools when possible.
                actions.push(PanelAction::AddLayer(Layer::new(drag.name, drag.kind)));
            }
        }
    }

    if let Some(drag) = &ui_state.tool_drag {
        if let Some((px, py)) = ui.input.pointer {
            ui.state.set_hot(Id::new("__tool_drag"));
            let tw = DrawList::text_width(drag.name, FONT_SCALE) + 16.0;
            let ghost = Rect::from_pos_size(px + 12.0, py + 12.0, tw, 22.0);
            ui.begin_overlay();
            ui.panel_rounded(ghost, style::ACCENT_DIM, style::RADIUS_SM);
            ui.label_at(
                ghost.min_x + 8.0,
                ghost.min_y + 4.0,
                drag.name,
                style::TEXT,
                FONT_SCALE,
            );
            ui.end_overlay();
        }
        if !ui.input.primary_down {
            ui_state.tool_drag = None;
        }
    }
}

fn shrink_rect(r: Rect, pad: f32) -> Rect {
    Rect::from_min_max(r.min_x + pad, r.min_y + pad, r.max_x - pad, r.max_y - pad)
}
