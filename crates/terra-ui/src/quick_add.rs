//! Searchable popup for adding common terrain layers.

use crate::command_registry::fuzzy_match;
use crate::panels::PanelAction;
use crate::tool_catalog::{quick_add_entries, ToolAction};
use crate::UiState;
use terra_core::layer::Layer;
use terra_gui::style::{self, FONT_SCALE, PAD, ROW_H};
use terra_gui::{button_id, Color, GuiContext, Id, Rect};

#[derive(Debug, Default)]
pub struct QuickAddState {
    pub query: String,
    pub scroll_y: f32,
}

pub fn draw_quick_add(
    ui: &mut GuiContext<'_>,
    ui_state: &mut UiState,
    state: &mut QuickAddState,
) -> Vec<PanelAction> {
    if !ui_state.show_quick_add {
        return Vec::new();
    }
    apply_search_input(ui, &mut state.query);
    if ui.input.escape_pressed {
        ui_state.show_quick_add = false;
        state.query.clear();
        return Vec::new();
    }

    let popup = Rect::from_pos_size(
        (ui.screen_w - 420.0).max(16.0) * 0.5,
        86.0,
        420.0_f32.min(ui.screen_w - 32.0),
        450.0_f32.min(ui.screen_h - 110.0),
    );
    let mut actions = Vec::new();
    ui.begin_overlay();
    ui.panel_rounded(popup, style::POPUP_BG, style::RADIUS_SM);
    ui.label_at(
        popup.min_x + PAD,
        popup.min_y + PAD,
        "Quick Add",
        style::TEXT,
        FONT_SCALE * 1.15,
    );
    let query_label = if state.query.is_empty() {
        "Search tools..."
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
        Id::new("quick_add_list"),
        list,
        Color::rgba(0.0, 0.0, 0.0, 0.0),
        &mut state.scroll_y,
    );

    let tools = quick_add_entries();
    let mut ordered: Vec<_> = tools
        .iter()
        .filter(|tool| fuzzy_match(&state.query, tool.label))
        .collect();
    ordered.sort_by_key(|tool| {
        ui_state
            .recent_tools
            .iter()
            .position(|id| id == tool.id)
            .unwrap_or(usize::MAX)
    });
    if state.query.is_empty() && !ui_state.recent_tools.is_empty() {
        ui.label_at(
            list.min_x,
            list.min_y + 4.0,
            "Recently used",
            style::TEXT_MUTED,
            FONT_SCALE * 0.85,
        );
        ui.gap(20.0);
    }
    for tool in ordered {
        let label = format!("{}  -  {}", tool.label, tool.description);
        if button_id(ui, Id::new("quick_add").child(tool.id), &label) {
            if let ToolAction::AddLayer { name, kind } = &tool.action {
                actions.push(PanelAction::AddLayer(Layer::new(*name, kind.clone())));
                remember_tool(&mut ui_state.recent_tools, tool.id);
                ui_state.show_quick_add = false;
                state.query.clear();
                break;
            }
        }
    }
    if button_id(ui, Id::new("quick_add_group"), "Group") {
        actions.push(PanelAction::AddGroup {
            name: "Group".into(),
        });
        ui_state.show_quick_add = false;
        state.query.clear();
    }
    ui.end_panel_scrolled(&mut state.scroll_y);
    ui.end_overlay();

    if ui.input.primary_pressed && !ui.pointer_in(popup) {
        ui_state.show_quick_add = false;
        state.query.clear();
    }
    actions
}

fn apply_search_input(ui: &GuiContext<'_>, query: &mut String) {
    if ui.input.backspace_pressed {
        query.pop();
    }
    query.push_str(&ui.input.text);
}

fn remember_tool(recent: &mut Vec<String>, id: &str) {
    recent.retain(|existing| existing != id);
    recent.insert(0, id.to_owned());
    recent.truncate(8);
}
