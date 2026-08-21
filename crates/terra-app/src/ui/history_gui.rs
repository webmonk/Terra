//! Read-only artist history window: one chronological list merged across
//! every undo stack (layer commands, mask/biome paint, world rules,
//! scenarios), with Undo and Redo controls.

use crate::ui::panels::viewport_float_rect;
use crate::ui::style::{self, FONT_SCALE, TYPE_CAPTION, TYPE_LABEL};
use crate::ui::{FrameUiOutput, UiState};
use terra_core::document::UndoDomain;
use terra_gui::{button, label_dim, DrawList, GuiContext, Id, Rect};

/// Small dim tag naming which undo stack owns a row.
fn domain_tag(domain: UndoDomain) -> &'static str {
    match domain {
        UndoDomain::Stack => "stack",
        UndoDomain::MaskPaint => "mask",
        UndoDomain::BiomePaint => "biome",
        UndoDomain::WorldRule => "rules",
        UndoDomain::Scenario => "sim",
    }
}

pub fn draw_history_gui(
    ui: &mut GuiContext<'_>,
    ui_state: &mut UiState,
    scroll_y: &mut f32,
    out: &mut FrameUiOutput,
) {
    if !ui_state.show_history {
        return;
    }

    let rect = viewport_float_rect(ui, 300.0, 360.0, 0.35);
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
        if ui_state.history_entries.is_empty() {
            label_dim(ui, "No recorded edits yet.");
        } else {
            label_dim(ui, "Newest first");
            // Entries arrive sorted ascending by global edit stamp; draw
            // newest first. Undone edits (pending redo) sit ahead of the
            // current position and render dimmed, Photoshop-style, with a
            // subtle divider where the done block begins.
            let mut prev_pending = false;
            let mut current_marked = false;
            for (i, entry) in ui_state.history_entries.iter().rev().enumerate() {
                if i > 0 && prev_pending && !entry.pending_redo {
                    let line = ui.allocate(5.0);
                    ui.panel(
                        Rect::from_pos_size(
                            line.min_x + 4.0,
                            line.min_y + 2.0,
                            line.width() - 8.0,
                            1.0,
                        ),
                        style::SEPARATOR,
                    );
                }
                prev_pending = entry.pending_redo;

                let row = ui.allocate(26.0);
                // Highlight the current position: the newest done edit.
                let is_current = !entry.pending_redo && !current_marked;
                if is_current {
                    current_marked = true;
                    ui.panel_rounded(row, style::SELECTED_BG, style::RADIUS_SM);
                }
                let (label_color, type_scale) = if entry.pending_redo {
                    (style::TEXT_DISABLED, TYPE_CAPTION)
                } else if is_current {
                    (style::TEXT, TYPE_LABEL)
                } else {
                    (style::TEXT_DIM, TYPE_CAPTION)
                };
                let tag_color = if entry.pending_redo {
                    style::TEXT_DISABLED
                } else {
                    style::TEXT_MUTED
                };
                ui.label_at(
                    row.min_x + 8.0,
                    row.min_y + 6.0,
                    &entry.label,
                    label_color,
                    FONT_SCALE * type_scale,
                );
                let tag = domain_tag(entry.domain);
                let tag_scale = FONT_SCALE * TYPE_CAPTION;
                let tag_w = DrawList::text_width(tag, tag_scale);
                ui.label_at(
                    row.max_x - tag_w - 8.0,
                    row.min_y + 7.0,
                    tag,
                    tag_color,
                    tag_scale,
                );
            }
        }
        ui.end_window(scroll_y);
    }
}
