//! Read-only overview of the terrain evaluation stack.

use crate::{PanelAction, UiState};
use terra_core::document::TerrainDocument;
use terra_gui::{button_id, label, GuiContext, Id, Rect};

pub fn draw_pipeline_gui(
    ui: &mut GuiContext<'_>,
    doc: &TerrainDocument,
    ui_state: &mut UiState,
    scroll_y: &mut f32,
    actions: &mut Vec<PanelAction>,
) {
    if !ui_state.show_pipeline {
        return;
    }

    let rect = Rect::from_pos_size(112.0, 86.0, 300.0, 360.0);
    if ui.begin_window(
        Id::new("win_pipeline"),
        "Pipeline Overview",
        rect,
        &mut ui_state.show_pipeline,
        scroll_y,
    ) {
        label(ui, "Evaluation order · bottom to top");
        ui.separator();
        let layers = doc.stack.flatten_layers();
        for (index, layer) in layers.iter().enumerate() {
            let mut badges = Vec::new();
            if !layer.common.masks.is_empty() {
                badges.push("mask");
            }
            if layer.common.cached {
                badges.push("cached");
            }
            if !layer.common.enabled {
                badges.push("disabled");
            }
            let prefix = if index == 0 { "Base" } else { "->" };
            let suffix = if badges.is_empty() {
                String::new()
            } else {
                format!(" [{}]", badges.join(", "))
            };
            let row = format!("{prefix} {}{suffix}", layer.common.name);
            if button_id(
                ui,
                Id::new("pipeline_layer").child(&format!("{:?}", layer.id())),
                &row,
            ) {
                actions.push(PanelAction::Select(layer.id()));
            }
        }
        if layers.is_empty() {
            label(ui, "No layers in the current document.");
        }
        ui.end_window(scroll_y);
    }
}
