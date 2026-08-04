//! Floating Bookmarks window — saved camera views (Ctrl/Alt+1–9).

use crate::{FrameUiOutput, UiState};
use terra_gui::style::{self, FONT_SCALE};
use terra_gui::{button, label, GuiContext, Id, Rect};

pub fn draw_bookmarks_gui(
    ui: &mut GuiContext<'_>,
    ui_state: &mut UiState,
    scroll_y: &mut f32,
    out: &mut FrameUiOutput,
) {
    if !ui_state.show_bookmarks {
        return;
    }

    let rect = Rect::from_pos_size(460.0, 100.0, 280.0, 400.0);
    if ui.begin_window(
        Id::new("win_bookmarks"),
        "Bookmarks",
        rect,
        &mut ui_state.show_bookmarks,
        scroll_y,
    ) {
        label(ui, "Saved camera views");
        label(ui, "Ctrl+1-9 save · Alt+1-9 recall");
        ui.separator();

        if button(ui, "Save to next empty slot") {
            out.request_save_bookmark = true;
        }
        ui.gap(6.0);

        for i in 0..9 {
            let slot = i;
            let filled = ui_state.bookmarks[slot].is_some();
            let row = ui.allocate(32.0);
            let hovered = ui.pointer_in(row);
            if hovered {
                ui.state.set_hot(Id::new("bm_row").with(slot as u64));
                ui.panel_rounded(row, style::HOVER_BG, style::RADIUS_SM);
            }
            let title = if filled {
                format!("Slot {} - saved view", slot + 1)
            } else {
                format!("Slot {} - empty", slot + 1)
            };
            ui.label_at(
                row.min_x + 8.0,
                row.min_y + 8.0,
                &title,
                if filled {
                    style::TEXT
                } else {
                    style::TEXT_MUTED
                },
                FONT_SCALE,
            );

            let recall_w = 56.0;
            let save_w = 48.0;
            let recall = Rect::from_pos_size(
                row.max_x - recall_w - save_w - 12.0,
                row.min_y + 4.0,
                recall_w,
                24.0,
            );
            let save =
                Rect::from_pos_size(row.max_x - save_w - 6.0, row.min_y + 4.0, save_w, 24.0);

            if filled {
                let rid = Id::new("bm_recall").with(slot as u64);
                let rh = ui.pointer_in(recall);
                if rh {
                    ui.state.set_hot(rid);
                }
                if rh && ui.input.primary_pressed {
                    ui.state.active = Some(rid);
                }
                if ui.input.primary_released && ui.state.is_active(rid) && rh {
                    out.request_recall_bookmark = Some(slot);
                }
                ui.panel_rounded(
                    recall,
                    if rh {
                        style::ACCENT
                    } else {
                        style::BUTTON_BG
                    },
                    style::RADIUS_SM,
                );
                ui.label_centered(
                    recall.center_x(),
                    recall.min_y + 5.0,
                    "Recall",
                    style::TEXT,
                    FONT_SCALE * 0.85,
                );
            }

            let sid = Id::new("bm_save").with(slot as u64);
            let sh = ui.pointer_in(save);
            if sh {
                ui.state.set_hot(sid);
            }
            if sh && ui.input.primary_pressed {
                ui.state.active = Some(sid);
            }
            if ui.input.primary_released && ui.state.is_active(sid) && sh {
                out.request_save_bookmark_slot = Some(slot);
            }
            ui.panel_rounded(
                save,
                if sh {
                    style::BUTTON_HOVER
                } else {
                    style::BUTTON_BG
                },
                style::RADIUS_SM,
            );
            ui.label_centered(
                save.center_x(),
                save.min_y + 5.0,
                "Save",
                style::TEXT,
                FONT_SCALE * 0.85,
            );
        }

        ui.end_window(scroll_y);
    }
}
