//! Aux channel inspector.
//!
//! The evaluator publishes around forty named channels - slope, wetness,
//! sediment thickness, flow accumulation, scatter density, hardness and the
//! rest - and before this there was no way to look at any of them. A handful
//! had a hand-written `Preview2dMode` variant each; the others were reachable
//! only from a debugger.
//!
//! This lists whatever the last evaluation actually published, so the list is
//! the engine's answer rather than a catalogue that can drift from it, and
//! shows enough per channel to tell apart the several different faults that all
//! present as "my mask is empty": the channel is absent, or present but all
//! zero, or present but constant, or fine and the mask is reading another one.

use crate::ui::panels::viewport_float_rect;
use crate::ui::style::{self, FONT_SCALE, TYPE_CAPTION, TYPE_LABEL};
use crate::ui::{Preview2dMode, UiState};
use terra_gui::{label_dim, GuiContext, Id};

/// Compact number for a channel range: enough significant figures to tell two
/// channels apart without turning the list into a wall of decimals.
fn compact(v: f32) -> String {
    let a = v.abs();
    if a == 0.0 {
        "0".into()
    } else if !(0.001..1000.0).contains(&a) {
        format!("{v:.2e}")
    } else if a >= 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.3}")
    }
}

pub fn draw_channels_gui(ui: &mut GuiContext<'_>, ui_state: &mut UiState, scroll_y: &mut f32) {
    if !ui_state.show_channels {
        return;
    }

    let rect = viewport_float_rect(ui, 360.0, 420.0, 0.55);
    if ui.begin_window(
        Id::new("win_channels"),
        "Channels",
        rect,
        &mut ui_state.show_channels,
        scroll_y,
    ) {
        if ui_state.channel_stats.is_empty() {
            label_dim(ui, "No channels published yet.");
            label_dim(ui, "Evaluate the stack to populate this list.");
            ui.end_window(scroll_y);
            return;
        }

        label_dim(
            ui,
            &format!("{} published this pass", ui_state.channel_stats.len()),
        );
        ui.separator();

        // Collected up front: the rows borrow `channel_stats` immutably while
        // the click below needs `ui_state` mutably.
        let rows: Vec<(String, String, bool)> = ui_state
            .channel_stats
            .iter()
            .map(|stat| {
                // Either what is wrong with it, or its range. A healthy channel
                // does not need prose, and a broken one does not need a range
                // the reader then has to interpret.
                let detail = match stat.diagnosis() {
                    Some(why) => why.to_string(),
                    None => format!(
                        "{} .. {}   mean {}   {:.0}% cover   {}x{}",
                        compact(stat.min),
                        compact(stat.max),
                        compact(stat.mean),
                        stat.coverage * 100.0,
                        stat.width,
                        stat.height,
                    ),
                };
                (stat.name.clone(), detail, stat.diagnosis().is_some())
            })
            .collect();

        let selected = ui_state.selected_channel.clone();
        let mut clicked: Option<String> = None;

        for (name, detail, flagged) in &rows {
            let is_selected = selected.as_deref() == Some(name.as_str());
            let row = ui.allocate(36.0);
            let hovered = ui.pointer_in(row);
            if is_selected {
                ui.panel_rounded(row, style::SELECTED_BG, style::RADIUS_SM);
            } else if hovered {
                ui.panel_rounded(row, style::ACCENT_SOFT, style::RADIUS_SM);
            }
            if hovered && ui.input.primary_pressed {
                clicked = Some(name.clone());
            }

            ui.label_at(
                row.min_x + 8.0,
                row.min_y + 4.0,
                name,
                if is_selected {
                    style::TEXT
                } else {
                    style::TEXT_DIM
                },
                FONT_SCALE * TYPE_LABEL,
            );
            ui.label_at(
                row.min_x + 8.0,
                row.min_y + 20.0,
                detail,
                if *flagged {
                    style::WARNING
                } else {
                    style::TEXT_MUTED
                },
                FONT_SCALE * TYPE_CAPTION,
            );
        }

        if let Some(name) = clicked {
            // Clicking the selected channel again clears it and restores the
            // ordinary lit view, so the panel is not a one-way door.
            if ui_state.selected_channel.as_deref() == Some(name.as_str()) {
                ui_state.selected_channel = None;
                ui_state.preview_mode = Preview2dMode::Lit;
            } else {
                ui_state.selected_channel = Some(name);
                ui_state.preview_mode = Preview2dMode::Channel;
            }
        }

        ui.separator();
        label_dim(ui, "Click a channel to view it. Click again to clear.");
        ui.end_window(scroll_y);
    }
}
