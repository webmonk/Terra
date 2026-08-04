//! Read-only artist history window with Undo and Redo controls.

use crate::{FrameUiOutput, UiState};
use terra_gui::{button, label, GuiContext, Id, Rect};

pub fn draw_history_gui(
    ui: &mut GuiContext<'_>,
    ui_state: &mut UiState,
    scroll_y: &mut f32,
    out: &mut FrameUiOutput,
) {
    if !ui_state.show_history {
        return;
    }

    let rect = Rect::from_pos_size(430.0, 86.0, 300.0, 360.0);
    if ui.begin_window(
        Id::new("win_history"),
        "History",
        rect,
        &mut ui_state.show_history,
        scroll_y,
    ) {
        if button(ui, "Undo") {
            out.request_undo = true;
        }
        if button(ui, "Redo") {
            out.request_redo = true;
        }
        ui.separator();
        if ui_state.history_descriptions.is_empty() {
            label(ui, "No recorded edits yet.");
        } else {
            label(ui, "Newest");
            for description in ui_state.history_descriptions.iter().rev() {
                label(ui, &format!("- {description}"));
            }
        }
        ui.end_window(scroll_y);
    }
}
