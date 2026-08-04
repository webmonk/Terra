//! Contextual viewport toolbar, brush bar, and view gizmo.

use crate::{LightingPreset, Preview2dMode, UiState};
use terra_gui::style::{self, FONT_SCALE, PAD};
use terra_gui::{
    segmented_button, Color, DrawList, GuiContext, Icon, Id, Rect,
};

pub fn draw_viewport_overlays(
    ui: &mut GuiContext<'_>,
    ui_state: &mut UiState,
    out: &mut crate::FrameUiOutput,
) {
    let vp = ui.viewport_rect();
    if vp.width() < 120.0 || vp.height() < 80.0 {
        return;
    }

    let y = vp.top() + PAD;
    draw_viewport_toolbar(ui, ui_state, vp, y, out);
    draw_view_gizmo(ui, vp);

    if ui_state.editor_tool.is_brush() {
        draw_brush_bar(ui, ui_state, vp);
    }

    // Bottom-left scale cue (minimap sits above when visible).
    let scale_y = if ui_state.show_minimap {
        vp.max_y - PAD - style::MINIMAP_SIZE - 28.0
    } else {
        vp.max_y - PAD - 16.0
    };
    ui.label_at(vp.left() + PAD, scale_y, "2 km", style::TEXT_DIM, FONT_SCALE);
    ui.panel(
        Rect::from_pos_size(vp.left() + PAD, scale_y + 14.0, 48.0, 2.0),
        style::TEXT_DIM,
    );
}

fn draw_viewport_toolbar(
    ui: &mut GuiContext<'_>,
    state: &mut UiState,
    vp: Rect,
    y: f32,
    _out: &mut crate::FrameUiOutput,
) {
    let primary_modes = [
        (Preview2dMode::Lit, "Lit"),
        (Preview2dMode::Height, "Height"),
        (Preview2dMode::Slope, "Slope"),
        (Preview2dMode::Flow, "Flow"),
        (Preview2dMode::Masks, "Masks"),
    ];

    let toolbar = Rect::from_min_max(
        vp.min_x + PAD,
        y,
        (vp.max_x - PAD).max(vp.min_x + PAD + 64.0),
        y + style::VIEWPORT_TOOLBAR_H,
    );
    ui.panel_rounded(toolbar, style::VIEWPORT_TOOLBAR_BG, style::RADIUS_MD);
    // Subtle border.
    ui.panel(
        Rect::from_pos_size(toolbar.min_x, toolbar.min_y, toolbar.width(), 1.0),
        style::BORDER,
    );

    let btn_h = toolbar.height() - 6.0;
    let btn_y = toolbar.min_y + 3.0;
    let inner_pad = 4.0;

    // Right cluster — Wireframe, Lighting, Bookmarks, Screenshot.
    let right_specs: [(&str, Icon, u64); 4] = [
        ("Grid", Icon::Grid3x3, 0),
        ("Wireframe", Icon::Box, 1),
        ("Lighting", Icon::Sun, 2),
        ("Bookmarks", Icon::Bookmark, 3),
    ];
    let mut right_edge = toolbar.max_x - inner_pad - 28.0; // leave room for camera
    let mut right_limit = right_edge;

    let cam_r = Rect::from_pos_size(toolbar.max_x - inner_pad - 26.0, btn_y, 26.0, btn_h);
    if icon_tool_button(ui, Id::new("viewport_shot"), cam_r, Icon::Camera, false) {
        // Screenshot hook — reserved for future capture path.
    }

    for (label, icon, index) in right_specs.iter().rev() {
        let width = if *index == 1 {
            // Wireframe benefits from text.
            DrawList::text_width(label, FONT_SCALE * 0.88) + 28.0
        } else {
            28.0
        };
        let rect = Rect::from_pos_size(right_edge - width, btn_y, width, btn_h);
        if rect.min_x < toolbar.min_x + inner_pad + 80.0 {
            break;
        }
        let active = match index {
            0 => state.viewport_overlays.grid,
            1 => state.viewport_overlays.wireframe,
            2 => state.lighting_menu_open,
            3 => false,
            _ => false,
        };
        let clicked = if *index == 1 {
            toolbar_text_icon(ui, Id::new("viewport_right").with(*index), rect, *icon, label, active)
        } else {
            icon_tool_button(ui, Id::new("viewport_right").with(*index), rect, *icon, active)
        };
        if clicked {
            match index {
                0 => state.viewport_overlays.grid = !state.viewport_overlays.grid,
                1 => state.viewport_overlays.wireframe = !state.viewport_overlays.wireframe,
                2 => {
                    state.lighting_menu_open = !state.lighting_menu_open;
                    state.viewport_more_open = false;
                }
                3 => {
                    state.show_bookmarks = true;
                }
                _ => {}
            }
        }
        right_edge = rect.min_x - 2.0;
        right_limit = right_edge;
    }

    // Left cluster — Lit / Height / Slope / Flow / Masks / More
    let mut x = toolbar.min_x + inner_pad;
    for (index, (mode, label)) in primary_modes.iter().enumerate() {
        let width = DrawList::text_width(label, FONT_SCALE * 0.92) + 16.0;
        if x + width > right_limit - 36.0 {
            break;
        }
        let rect = Rect::from_pos_size(x, btn_y, width, btn_h);
        if segmented_button(
            ui,
            Id::new("viewport_primary_mode").with(index as u64),
            rect,
            label,
            state.preview_mode == *mode,
        ) {
            state.preview_mode = *mode;
            state.viewport_more_open = false;
        }
        x += width + 1.0;
    }

    if x + 32.0 <= right_limit {
        let more = Rect::from_pos_size(x, btn_y, 32.0, btn_h);
        if toolbar_text_icon(
            ui,
            Id::new("viewport_more"),
            more,
            Icon::Ellipsis,
            "More",
            state.viewport_more_open,
        ) {
            state.viewport_more_open = !state.viewport_more_open;
            state.lighting_menu_open = false;
        }
        if state.viewport_more_open {
            draw_more_menu(ui, state, more);
        }
    }

    if state.lighting_menu_open {
        // Anchor near the lighting button (approx right side).
        let anchor = Rect::from_pos_size(right_limit - 20.0, btn_y, 28.0, btn_h);
        draw_lighting_menu(ui, state, anchor);
    }
}

fn draw_more_menu(ui: &mut GuiContext<'_>, state: &mut UiState, anchor: Rect) {
    let items = [
        (Preview2dMode::Curvature, "Curvature"),
        (Preview2dMode::Convexity, "Convexity"),
        (Preview2dMode::Concavity, "Concavity"),
        (Preview2dMode::Normals, "Normals"),
        (Preview2dMode::AmbientOcclusion, "Ambient Occlusion"),
        (Preview2dMode::Material, "Material"),
        (Preview2dMode::Biome, "Biome"),
        (Preview2dMode::VegetationDensity, "Vegetation Density"),
    ];
    ui.begin_overlay();
    let item_h = 26.0;
    let menu_w = 180.0;
    let menu = Rect::from_pos_size(
        anchor.min_x,
        anchor.max_y + 4.0,
        menu_w,
        item_h * items.len() as f32 + 8.0,
    );
    ui.panel_rounded(menu, style::POPUP_BG, style::RADIUS_SM);
    if ui.pointer_in(menu) {
        ui.state.set_hot(Id::new("viewport_more_menu"));
    }
    for (i, (mode, label)) in items.iter().enumerate() {
        let row = Rect::from_pos_size(
            menu.min_x + 4.0,
            menu.min_y + 4.0 + i as f32 * item_h,
            menu.width() - 8.0,
            item_h - 2.0,
        );
        let id = Id::new("viewport_more_item").with(i as u64);
        let hovered = ui.pointer_in(row);
        let selected = state.preview_mode == *mode;
        if hovered {
            ui.state.set_hot(id);
        }
        if hovered && ui.input.primary_pressed {
            ui.state.active = Some(id);
        }
        if ui.input.primary_released && ui.state.is_active(id) && hovered {
            state.preview_mode = *mode;
            state.viewport_more_open = false;
        }
        if selected || hovered {
            ui.panel_rounded(
                row,
                if selected {
                    style::SELECTED_BG
                } else {
                    style::HOVER_BG
                },
                style::RADIUS_SM,
            );
        }
        ui.label_at(
            row.min_x + 8.0,
            row.min_y + 5.0,
            label,
            style::TEXT,
            FONT_SCALE * 0.92,
        );
    }
    ui.end_overlay();

    if ui.input.primary_pressed {
        if let Some((px, py)) = ui.input.pointer {
            if !menu.contains(px, py) && !anchor.contains(px, py) {
                state.viewport_more_open = false;
            }
        }
    }
}

fn draw_lighting_menu(ui: &mut GuiContext<'_>, state: &mut UiState, anchor: Rect) {
    ui.begin_overlay();
    let item_h = 26.0;
    let menu_w = 140.0;
    let menu = Rect::from_pos_size(
        (anchor.max_x - menu_w).max(8.0),
        anchor.max_y + 4.0,
        menu_w,
        item_h * LightingPreset::ALL.len() as f32 + 8.0,
    );
    ui.panel_rounded(menu, style::POPUP_BG, style::RADIUS_SM);
    if ui.pointer_in(menu) {
        ui.state.set_hot(Id::new("lighting_menu"));
    }
    for (i, preset) in LightingPreset::ALL.iter().enumerate() {
        let row = Rect::from_pos_size(
            menu.min_x + 4.0,
            menu.min_y + 4.0 + i as f32 * item_h,
            menu.width() - 8.0,
            item_h - 2.0,
        );
        let id = Id::new("lighting_item").with(i as u64);
        let hovered = ui.pointer_in(row);
        let selected = state.lighting_preset == *preset;
        if hovered {
            ui.state.set_hot(id);
        }
        if hovered && ui.input.primary_pressed {
            ui.state.active = Some(id);
        }
        if ui.input.primary_released && ui.state.is_active(id) && hovered {
            state.lighting_preset = *preset;
            state.lighting_menu_open = false;
        }
        if selected || hovered {
            ui.panel_rounded(
                row,
                if selected {
                    style::SELECTED_BG
                } else {
                    style::HOVER_BG
                },
                style::RADIUS_SM,
            );
        }
        ui.label_at(
            row.min_x + 8.0,
            row.min_y + 5.0,
            preset.label(),
            style::TEXT,
            FONT_SCALE * 0.92,
        );
    }
    ui.end_overlay();

    if ui.input.primary_pressed {
        if let Some((px, py)) = ui.input.pointer {
            if !menu.contains(px, py) && !anchor.contains(px, py) {
                state.lighting_menu_open = false;
            }
        }
    }
}

fn draw_view_gizmo(ui: &mut GuiContext<'_>, vp: Rect) {
    let size = 54.0;
    let gizmo = Rect::from_pos_size(vp.max_x - PAD - size, vp.min_y + PAD + 40.0, size, size);
    ui.panel_rounded(gizmo, style::OVERLAY_BG, style::RADIUS_MD);
    let cx = gizmo.center_x();
    let cy = gizmo.center_y();
    // Simple axis indicators (X red, Y green, Z blue).
    ui.panel(
        Rect::from_pos_size(cx, cy - 1.0, 18.0, 2.0),
        Color::rgb(0.90, 0.35, 0.35),
    );
    ui.panel(
        Rect::from_pos_size(cx - 1.0, cy - 18.0, 2.0, 18.0),
        Color::rgb(0.40, 0.85, 0.45),
    );
    ui.panel(
        Rect::from_pos_size(cx - 10.0, cy + 6.0, 14.0, 2.0),
        Color::rgb(0.35, 0.55, 0.95),
    );
    ui.label_at(cx + 16.0, cy - 8.0, "X", Color::rgb(0.90, 0.35, 0.35), FONT_SCALE * 0.7);
    ui.label_at(cx - 4.0, cy - 28.0, "Y", Color::rgb(0.40, 0.85, 0.45), FONT_SCALE * 0.7);
    ui.label_at(cx - 22.0, cy + 8.0, "Z", Color::rgb(0.35, 0.55, 0.95), FONT_SCALE * 0.7);
}

fn draw_brush_bar(ui: &mut GuiContext<'_>, state: &mut UiState, vp: Rect) {
    let bar_w = (vp.width() - PAD * 2.0).min(560.0).max(280.0);
    let bar = Rect::from_pos_size(
        vp.min_x + (vp.width() - bar_w) * 0.5,
        vp.max_y - PAD - style::BRUSH_BAR_H,
        bar_w,
        style::BRUSH_BAR_H,
    );
    ui.panel_rounded(bar, style::VIEWPORT_TOOLBAR_BG, style::RADIUS_MD);

    // Brush preview circle.
    let preview = Rect::from_pos_size(bar.min_x + 10.0, bar.min_y + 8.0, 32.0, 32.0);
    ui.panel_rounded(preview, style::SURFACE, 16.0);
    let inner_r = 8.0 + state.brush_falloff * 6.0;
    ui.panel_rounded(
        Rect::from_pos_size(
            preview.center_x() - inner_r * 0.5,
            preview.center_y() - inner_r * 0.5,
            inner_r,
            inner_r,
        ),
        style::ACCENT_SOFT,
        inner_r * 0.5,
    );

    let mut x = bar.min_x + 50.0;
    let stop = bar.max_x - 160.0;
    if x + 100.0 <= stop {
        x = compact_slider(
            ui,
            Id::new("brush_radius"),
            x,
            bar,
            "Radius",
            &mut state.sculpt_radius,
            0.005,
            0.25,
        );
    }
    if x + 100.0 <= stop {
        x = compact_slider(
            ui,
            Id::new("brush_strength"),
            x,
            bar,
            "Strength",
            &mut state.sculpt_strength,
            0.1,
            40.0,
        );
    }
    if x + 100.0 <= stop {
        x = compact_slider(
            ui,
            Id::new("brush_falloff"),
            x,
            bar,
            "Falloff",
            &mut state.brush_falloff,
            0.0,
            1.0,
        );
    }
    if x + 100.0 <= stop {
        x = compact_slider(
            ui,
            Id::new("brush_flow"),
            x,
            bar,
            "Flow",
            &mut state.brush_flow,
            0.05,
            1.0,
        );
    }

    let mut rx = bar.max_x - 8.0;
    let toggle_w = 52.0;
    for (label, flag_idx) in [("Sym", 2u64), ("Inv", 1), ("Shape", 0)] {
        rx -= toggle_w + 4.0;
        let rect = Rect::from_pos_size(rx, bar.min_y + 8.0, toggle_w, bar.height() - 16.0);
        let active = match flag_idx {
            0 => false,
            1 => state.invert_brush,
            2 => state.brush_symmetry,
            _ => false,
        };
        if toolbar_button(ui, Id::new("brush_toggle").with(flag_idx), rect, label, active) {
            match flag_idx {
                1 => state.invert_brush = !state.invert_brush,
                2 => state.brush_symmetry = !state.brush_symmetry,
                _ => {}
            }
        }
    }
    let _ = x;
}

fn compact_slider(
    ui: &mut GuiContext<'_>,
    id: Id,
    x: f32,
    bar: Rect,
    label: &str,
    value: &mut f32,
    min: f32,
    max: f32,
) -> f32 {
    let width = 100.0;
    let rect = Rect::from_pos_size(x, bar.min_y + 4.0, width, bar.height() - 8.0);
    let track = Rect::from_pos_size(rect.min_x, rect.max_y - 8.0, width - 4.0, 4.0);
    let hovered = ui.pointer_in(rect);
    if hovered {
        ui.state.set_hot(id);
    }
    if hovered && ui.input.primary_pressed {
        ui.state.active = Some(id);
    }
    if ui.state.is_active(id) {
        if let Some((pointer_x, _)) = ui.input.pointer {
            *value =
                (min + (pointer_x - track.min_x) / track.width() * (max - min)).clamp(min, max);
        }
    }
    let fill = ((*value - min) / (max - min)).clamp(0.0, 1.0) * track.width();
    ui.label_at(
        rect.min_x,
        rect.min_y,
        &format!("{label} {:.2}", *value),
        style::TEXT_DIM,
        FONT_SCALE * 0.85,
    );
    ui.panel_rounded(track, style::TRACK_BG, 2.0);
    ui.panel_rounded(
        Rect::from_pos_size(track.min_x, track.min_y, fill.max(4.0), track.height()),
        style::ACCENT,
        2.0,
    );
    x + width + 8.0
}

fn toolbar_button(ui: &mut GuiContext<'_>, id: Id, rect: Rect, label: &str, active: bool) -> bool {
    let hovered = ui.pointer_in(rect);
    if hovered {
        ui.state.set_hot(id);
    }
    if hovered && ui.input.primary_pressed {
        ui.state.active = Some(id);
    }
    let clicked = ui.input.primary_released && ui.state.is_active(id) && hovered;
    ui.panel_rounded(
        rect,
        if active {
            style::ACCENT
        } else if hovered {
            style::BUTTON_HOVER
        } else {
            style::BUTTON_BG
        },
        style::RADIUS_SM,
    );
    ui.label_centered_in_rect(rect, label, style::TEXT, FONT_SCALE * 0.85);
    clicked
}

fn icon_tool_button(ui: &mut GuiContext<'_>, id: Id, rect: Rect, icon: Icon, active: bool) -> bool {
    let hovered = ui.pointer_in(rect);
    if hovered {
        ui.state.set_hot(id);
    }
    if hovered && ui.input.primary_pressed {
        ui.state.active = Some(id);
    }
    let clicked = ui.input.primary_released && ui.state.is_active(id) && hovered;
    ui.panel_rounded(
        rect,
        if active {
            style::ACCENT
        } else if hovered {
            style::BUTTON_HOVER
        } else {
            Color::rgba(0.0, 0.0, 0.0, 0.0)
        },
        style::RADIUS_SM,
    );
    ui.icon_centered(
        rect,
        icon,
        if active { style::TEXT } else { style::TEXT_DIM },
        14.0,
    );
    clicked
}

fn toolbar_text_icon(
    ui: &mut GuiContext<'_>,
    id: Id,
    rect: Rect,
    icon: Icon,
    label: &str,
    active: bool,
) -> bool {
    let hovered = ui.pointer_in(rect);
    if hovered {
        ui.state.set_hot(id);
    }
    if hovered && ui.input.primary_pressed {
        ui.state.active = Some(id);
    }
    let clicked = ui.input.primary_released && ui.state.is_active(id) && hovered;
    ui.panel_rounded(
        rect,
        if active {
            style::ACCENT
        } else if hovered {
            style::BUTTON_HOVER
        } else {
            Color::rgba(0.0, 0.0, 0.0, 0.0)
        },
        style::RADIUS_SM,
    );
    ui.icon_centered(
        Rect::from_pos_size(rect.min_x + 4.0, rect.min_y, 18.0, rect.height()),
        icon,
        style::TEXT_DIM,
        14.0,
    );
    ui.label_in_rect(
        Rect::from_pos_size(rect.min_x + 24.0, rect.min_y, rect.width() - 28.0, rect.height()),
        label,
        style::TEXT,
        FONT_SCALE * 0.88,
    );
    clicked
}
