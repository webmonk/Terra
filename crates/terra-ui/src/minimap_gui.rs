//! Compact terrain navigation minimap drawn over the 3D viewport.

use crate::{FrameUiOutput, UiState};
use terra_gui::style::{self, FONT_SCALE, PAD};
use terra_gui::{Color, GuiContext, Icon, Id, Rect};

pub fn draw_minimap(ui: &mut GuiContext<'_>, state: &mut UiState, out: &mut FrameUiOutput) {
    if !state.show_minimap {
        return;
    }

    let vp = ui.viewport_rect();
    let size = style::MINIMAP_SIZE
        .min(vp.width().min(vp.height()) * 0.28)
        .max(120.0);
    // Lower-left of the viewport (above the scale bar).
    let panel = Rect::from_pos_size(vp.min_x + PAD, vp.max_y - size - PAD, size, size);
    let map = Rect::from_pos_size(
        panel.min_x + 8.0,
        panel.min_y + 27.0,
        panel.width() - 16.0,
        panel.height() - 35.0,
    );
    let close = Rect::from_pos_size(panel.max_x - 25.0, panel.min_y + 5.0, 18.0, 18.0);
    let map_id = Id::new("minimap_map");
    let close_id = Id::new("minimap_close");

    ui.begin_overlay();
    ui.panel_rounded(panel, style::OVERLAY_BG, style::RADIUS_SM);
    ui.label_at(
        panel.min_x + 8.0,
        panel.min_y + 7.0,
        "MINIMAP",
        style::TEXT_DIM,
        FONT_SCALE * 0.85,
    );

    let map_color = preview_average_color(state).unwrap_or(Color::rgb(0.13, 0.20, 0.16));
    ui.panel(map, map_color);
    // Terrain footprint / bounds.
    ui.panel(
        Rect::from_pos_size(map.min_x, map.min_y, map.width(), 1.0),
        style::TEXT_DIM,
    );
    ui.panel(
        Rect::from_pos_size(map.min_x, map.max_y - 1.0, map.width(), 1.0),
        style::TEXT_DIM,
    );
    ui.panel(
        Rect::from_pos_size(map.min_x, map.min_y, 1.0, map.height()),
        style::TEXT_DIM,
    );
    ui.panel(
        Rect::from_pos_size(map.max_x - 1.0, map.min_y, 1.0, map.height()),
        style::TEXT_DIM,
    );

    let u = state.camera_xz.0.clamp(0.0, 1.0);
    let v = state.camera_xz.1.clamp(0.0, 1.0);
    let marker_x = map.min_x + u * map.width();
    let marker_y = map.min_y + v * map.height();
    // Rounded marker plus a heading point form a compact camera direction wedge.
    ui.panel_rounded(
        Rect::from_pos_size(marker_x - 4.0, marker_y - 4.0, 8.0, 8.0),
        style::ACCENT,
        4.0,
    );
    let heading_x = marker_x + state.camera_yaw.cos() * 11.0;
    let heading_y = marker_y + state.camera_yaw.sin() * 11.0;
    ui.panel_rounded(
        Rect::from_pos_size(heading_x - 2.5, heading_y - 2.5, 5.0, 5.0),
        style::TEXT,
        2.5,
    );

    let map_hovered = ui.pointer_in(map);
    if map_hovered {
        ui.state.set_hot(map_id);
    }
    if map_hovered && ui.input.primary_pressed {
        ui.state.active = Some(map_id);
    }
    if map_hovered && ui.input.primary_released && ui.state.is_active(map_id) {
        if let Some((x, y)) = ui.input.pointer {
            out.request_camera_focus_uv = Some((
                ((x - map.min_x) / map.width()).clamp(0.0, 1.0),
                ((y - map.min_y) / map.height()).clamp(0.0, 1.0),
            ));
        }
    }

    let close_hovered = ui.pointer_in(close);
    if close_hovered {
        ui.state.set_hot(close_id);
    }
    if close_hovered && ui.input.primary_pressed {
        ui.state.active = Some(close_id);
    }
    ui.panel_rounded(
        close,
        if close_hovered {
            style::BUTTON_HOVER
        } else {
            style::BUTTON_BG
        },
        style::RADIUS_SM,
    );
    ui.icon_at(
        close.min_x + (close.width() - 14.0) * 0.5,
        close.min_y + (close.height() - 14.0) * 0.5,
        Icon::X,
        style::TEXT,
        14.0,
    );
    if close_hovered && ui.input.primary_released && ui.state.is_active(close_id) {
        state.show_minimap = false;
    }
    ui.end_overlay();
}

/// The GUI renderer has one image upload slot per frame, so use a sampled flat
/// terrain color here instead of competing with the full 2D Preview window.
fn preview_average_color(state: &UiState) -> Option<Color> {
    let (width, height, rgba) = state.preview_rgba.as_ref()?;
    let pixel_count = (*width as usize).saturating_mul(*height as usize);
    if pixel_count == 0 || rgba.len() < pixel_count.saturating_mul(4) {
        return None;
    }
    let stride = (pixel_count / 256).max(1);
    let mut total = [0u64; 3];
    let mut count = 0u64;
    for pixel in rgba.chunks_exact(4).step_by(stride) {
        total[0] += pixel[0] as u64;
        total[1] += pixel[1] as u64;
        total[2] += pixel[2] as u64;
        count += 1;
    }
    Some(Color::rgb(
        total[0] as f32 / (count * 255) as f32,
        total[1] as f32 / (count * 255) as f32,
        total[2] as f32 / (count * 255) as f32,
    ))
}
