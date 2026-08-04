//! Top application bar — brand, menus, workspace tabs, preview state, export.

use crate::{AppWorkspace, FrameUiOutput, UiState, WorkspaceMode};
use std::sync::OnceLock;
use terra_gui::style::{self, FONT_SCALE, PAD, TOOLBAR_BTN_H};
use terra_gui::{
    checkbox, chip_icon_button, icon_button, menu_button as menu_row, status_pill, Color, DrawList,
    GuiContext, Icon, Id, Rect, INSET_TOP,
};

fn toolbar_logo() -> &'static (u32, u32, Vec<u8>) {
    static LOGO: OnceLock<(u32, u32, Vec<u8>)> = OnceLock::new();
    LOGO.get_or_init(|| {
        let bytes = include_bytes!("../../../assets/logo.png");
        let img = image::load_from_memory(bytes)
            .expect("assets/logo.png")
            .to_rgba8();
        (img.width(), img.height(), img.into_raw())
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuId {
    File,
    Edit,
    View,
}

#[derive(Debug, Default)]
pub struct ChromeGuiState {
    open: Option<MenuId>,
    /// Vertical scroll for the open File/Edit/View dropdown.
    menu_scroll_y: f32,
    pub resolution_open: bool,
}

const RESOLUTION_OPTIONS: &[u32] = &[256, 512, 1024, 2048, 4096];

pub fn draw_menu_bar(
    ui: &mut GuiContext<'_>,
    ui_state: &mut UiState,
    state: &mut ChromeGuiState,
    out: &mut FrameUiOutput,
) {
    let bar = Rect::from_min_max(0.0, 0.0, ui.screen_w, INSET_TOP);
    ui.panel(bar, style::TOOLBAR_BG);
    ui.panel(
        Rect::from_pos_size(0.0, INSET_TOP - 1.0, ui.screen_w, 1.0),
        style::SEPARATOR,
    );
    if ui
        .input
        .pointer
        .map(|(x, y)| bar.contains(x, y))
        .unwrap_or(false)
    {
        ui.state.set_hot(Id::new("__menu_bar"));
    }

    let y = (INSET_TOP - TOOLBAR_BTN_H) * 0.5;
    let mut x = PAD;

    // Brand logo.
    let (lw, lh, rgba) = toolbar_logo();
    let logo_h = 28.0_f32;
    let logo_w = logo_h * (*lw as f32 / (*lh as f32).max(1.0));
    let logo_r = Rect::from_pos_size(x, y + (TOOLBAR_BTN_H - logo_h) * 0.5, logo_w, logo_h);
    ui.image(logo_r, *lw, *lh, rgba);
    x += logo_w + 16.0;

    x = menu_button(ui, state, MenuId::File, "File", x, y, TOOLBAR_BTN_H);
    x = menu_button(ui, state, MenuId::Edit, "Edit", x, y, TOOLBAR_BTN_H);
    x = menu_button(ui, state, MenuId::View, "View", x, y, TOOLBAR_BTN_H);
    x += 8.0;

    // Undo / Redo
    let undo_r = Rect::from_pos_size(x, y, 32.0, TOOLBAR_BTN_H);
    if icon_button(ui, Id::new("tb_undo"), Icon::Undo2, undo_r) {
        out.request_undo = true;
    }
    x += 36.0;
    let redo_r = Rect::from_pos_size(x, y, 32.0, TOOLBAR_BTN_H);
    if icon_button(ui, Id::new("tb_redo"), Icon::Redo2, redo_r) {
        out.request_redo = true;
    }
    x += 40.0;

    // Divider between menus and workspaces.
    ui.panel(
        Rect::from_pos_size(x, y + 6.0, 1.0, TOOLBAR_BTN_H - 12.0),
        style::SEPARATOR,
    );
    x += 14.0;
    let tabs_start_x = x;

    // Right cluster first so tabs can stop before colliding.
    let settings_r = Rect::from_pos_size(ui.screen_w - PAD - 28.0, y, 28.0, TOOLBAR_BTN_H);
    let export_w = DrawList::text_width("Export", FONT_SCALE) + 44.0;
    let export_r = Rect::from_pos_size(settings_r.min_x - 12.0 - export_w, y, export_w, TOOLBAR_BTN_H);
    let (pill_label, pill_color) = preview_status(ui_state);
    let pill_w = DrawList::text_width(pill_label, FONT_SCALE * 0.9) + 28.0;
    let pill_r = Rect::from_pos_size(
        export_r.min_x - 10.0 - pill_w,
        y + 2.0,
        pill_w,
        TOOLBAR_BTN_H - 4.0,
    );
    let res_value = ui_state
        .pending_preview_resolution
        .unwrap_or(ui_state.profile.tex_w.max(1));
    let res = format!("{res_value}x{res_value}");
    let res_w = DrawList::text_width(&res, FONT_SCALE) + 28.0;
    let res_r = Rect::from_pos_size(
        pill_r.min_x - 10.0 - res_w,
        y,
        res_w,
        TOOLBAR_BTN_H,
    );
    let tabs_max_x = res_r.min_x - 16.0;

    // Workspace tabs — underline selection, not filled capsules.
    let tab_bottom = INSET_TOP - 1.0;
    let mut x = tabs_start_x;
    for workspace in AppWorkspace::ALL {
        let label = workspace.label().to_uppercase();
        let w = DrawList::text_width(&label, FONT_SCALE * 0.92) + 24.0;
        if x + w > tabs_max_x {
            break;
        }
        let tab = Rect::from_pos_size(x, y, w, TOOLBAR_BTN_H);
        let id = Id::new("workspace_tab").child(workspace.label());
        let selected = ui_state.app_workspace == workspace;
        let hovered = ui.pointer_in(tab);
        if hovered {
            ui.state.set_hot(id);
        }
        if hovered && ui.input.primary_pressed {
            ui.state.active = Some(id);
        }
        if ui.input.primary_released && ui.state.is_active(id) && hovered {
            ui_state.app_workspace = workspace;
            ui_state.workspace_mode = workspace_default_mode(workspace);
        }
        if hovered && !selected {
            ui.panel_rounded(
                Rect::from_pos_size(tab.min_x + 2.0, tab.min_y + 2.0, tab.width() - 4.0, tab.height() - 4.0),
                style::HOVER_BG,
                style::RADIUS_SM,
            );
        }
        ui.label_centered_in_rect(
            tab,
            &label,
            if selected {
                style::TEXT
            } else if hovered {
                style::TEXT
            } else {
                style::TEXT_DIM
            },
            FONT_SCALE * 0.92,
        );
        if selected {
            ui.panel(
                Rect::from_pos_size(tab.min_x + 8.0, tab_bottom - 2.0, tab.width() - 16.0, 2.0),
                style::ACCENT,
            );
        }
        x += w + 10.0;
    }

    let _ = icon_button(ui, Id::new("tb_settings"), Icon::Settings2, settings_r);

    if chip_icon_button(
        ui,
        Id::new("tb_export"),
        Icon::Download,
        "Export",
        export_r,
        true,
    ) {
        ui_state.show_export = true;
    }

    status_pill(ui, pill_r, pill_label, pill_color);
    let res_id = Id::new("tb_resolution");
    let res_hovered = ui.pointer_in(res_r);
    if res_hovered {
        ui.state.set_hot(res_id);
    }
    if res_hovered && ui.input.primary_pressed {
        ui.state.active = Some(res_id);
        state.resolution_open = !state.resolution_open;
    }
    ui.panel_rounded(
        res_r,
        if state.resolution_open || res_hovered {
            style::BUTTON_HOVER
        } else {
            style::SURFACE
        },
        style::RADIUS_SM,
    );
    ui.label_in_rect(
        Rect::from_pos_size(res_r.min_x + 10.0, res_r.min_y, res_r.width() - 28.0, res_r.height()),
        &res,
        style::TEXT,
        FONT_SCALE,
    );
    ui.icon_centered(
        Rect::from_pos_size(res_r.max_x - 22.0, res_r.min_y, 18.0, res_r.height()),
        Icon::ChevronDown,
        style::TEXT_MUTED,
        14.0,
    );

    if state.resolution_open {
        draw_resolution_menu(ui, ui_state, state, res_r);
    }
}

fn preview_status(ui_state: &UiState) -> (&'static str, Color) {
    if ui_state.refining {
        ("Building Preview", style::ACCENT)
    } else if ui_state.status.to_lowercase().contains("fail") {
        ("Build Failed", style::ERROR)
    } else if ui_state.draft_displayed {
        ("Refining", style::WARNING)
    } else {
        ("Preview Ready", style::SUCCESS)
    }
}

fn draw_resolution_menu(
    ui: &mut GuiContext<'_>,
    ui_state: &mut UiState,
    state: &mut ChromeGuiState,
    anchor: Rect,
) {
    ui.begin_overlay();
    let item_h = 26.0;
    let menu_h = item_h * RESOLUTION_OPTIONS.len() as f32 + 8.0;
    let menu = Rect::from_pos_size(anchor.min_x, INSET_TOP, anchor.width().max(110.0), menu_h);
    ui.panel_rounded(menu, style::COMBO_MENU_BG, style::RADIUS_SM);
    if ui.pointer_in(menu) {
        ui.state.set_hot(Id::new("res_menu"));
    }
    let current = ui_state
        .pending_preview_resolution
        .unwrap_or(ui_state.profile.tex_w.max(1));
    for (i, &res) in RESOLUTION_OPTIONS.iter().enumerate() {
        let row = Rect::from_pos_size(menu.min_x + 4.0, menu.min_y + 4.0 + i as f32 * item_h, menu.width() - 8.0, item_h - 2.0);
        let id = Id::new("res_opt").with(i as u64);
        let hovered = ui.pointer_in(row);
        let selected = current == res;
        if hovered {
            ui.state.set_hot(id);
        }
        if hovered && ui.input.primary_pressed {
            ui.state.active = Some(id);
        }
        if ui.input.primary_released && ui.state.is_active(id) && hovered {
            ui_state.pending_preview_resolution = Some(res);
            state.resolution_open = false;
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
        let label = format!("{res}x{res}");
        ui.label_at(
            row.min_x + 8.0,
            row.min_y + 5.0,
            &label,
            style::TEXT,
            FONT_SCALE,
        );
    }
    ui.end_overlay();

    if ui.input.primary_pressed {
        if let Some((px, py)) = ui.input.pointer {
            if !menu.contains(px, py) && !anchor.contains(px, py) {
                state.resolution_open = false;
            }
        }
    }
}

/// Draw open File/Edit/View menus on the overlay layer (after all chrome).
pub fn draw_menu_overlays(
    ui: &mut GuiContext<'_>,
    ui_state: &mut UiState,
    state: &mut ChromeGuiState,
    out: &mut FrameUiOutput,
) {
    let Some(menu) = state.open else {
        return;
    };

    ui.begin_overlay();
    let popup = draw_dropdown(ui, ui_state, state, out, menu);
    ui.end_overlay();

    let bar = Rect::from_min_max(0.0, 0.0, ui.screen_w, INSET_TOP);
    if ui.input.primary_pressed {
        if let Some((px, py)) = ui.input.pointer {
            let in_bar = bar.contains(px, py);
            let in_popup = popup.contains(px, py);
            if !in_bar && !in_popup {
                state.open = None;
                state.menu_scroll_y = 0.0;
            }
        }
    }
}

fn menu_button(
    ui: &mut GuiContext<'_>,
    state: &mut ChromeGuiState,
    id: MenuId,
    label: &str,
    x: f32,
    y: f32,
    h: f32,
) -> f32 {
    let w = DrawList::text_width(label, FONT_SCALE) + 18.0;
    let rect = Rect::from_pos_size(x, y, w, h);
    let wid = match id {
        MenuId::File => Id::new("menu_file"),
        MenuId::Edit => Id::new("menu_edit"),
        MenuId::View => Id::new("menu_view"),
    };
    let hovered = ui.pointer_in(rect);
    if hovered {
        ui.state.set_hot(wid);
    }
    let open = state.open == Some(id);
    if hovered && ui.input.primary_pressed {
        let next = if open { None } else { Some(id) };
        if next != state.open {
            state.menu_scroll_y = 0.0;
        }
        state.open = next;
    } else if hovered && state.open.is_some() && state.open != Some(id) {
        state.menu_scroll_y = 0.0;
        state.open = Some(id);
    }
    if open || hovered {
        ui.panel_rounded(rect, style::BUTTON_HOVER, style::RADIUS_SM);
    }
    ui.label_at(
        rect.min_x + 9.0,
        rect.min_y + 7.0,
        label,
        style::TEXT_DIM,
        FONT_SCALE,
    );
    x + w + 2.0
}

fn draw_dropdown(
    ui: &mut GuiContext<'_>,
    ui_state: &mut UiState,
    state: &mut ChromeGuiState,
    out: &mut FrameUiOutput,
    menu: MenuId,
) -> Rect {
    let x = match menu {
        MenuId::File => PAD + 70.0,
        MenuId::Edit => PAD + 126.0,
        MenuId::View => PAD + 182.0,
    };
    // Natural height if the screen is tall enough; otherwise clamp and scroll.
    let natural_h = match menu {
        MenuId::File => style::ROW_H * 3.0 + PAD * 2.0,
        MenuId::Edit => style::ROW_H * 2.0 + PAD * 2.0,
        MenuId::View => {
            style::ROW_H * 16.0
                + (style::SEPARATOR_H + style::GAP) * 2.0
                + PAD * 2.0
        }
    };
    let max_h = (ui.screen_h - INSET_TOP - PAD).max(style::ROW_H * 4.0);
    let popup = Rect::from_pos_size(x, INSET_TOP, 228.0, natural_h.min(max_h));
    if ui
        .input
        .pointer
        .map(|(px, py)| popup.contains(px, py))
        .unwrap_or(false)
    {
        ui.state.set_hot(Id::new("menu_popup"));
    }

    let scroll_id = match menu {
        MenuId::File => Id::new("menu_scroll_file"),
        MenuId::Edit => Id::new("menu_scroll_edit"),
        MenuId::View => Id::new("menu_scroll_view"),
    };
    ui.begin_panel_scrolled(
        scroll_id,
        popup,
        style::COMBO_MENU_BG,
        &mut state.menu_scroll_y,
    );
    match menu {
        MenuId::File => {
            if menu_item(ui, "Save Project As...") {
                out.request_save_as = true;
                state.open = None;
                state.menu_scroll_y = 0.0;
            }
            if menu_item(ui, "Load Project...") {
                out.request_load_path = true;
                state.open = None;
                state.menu_scroll_y = 0.0;
            }
            if menu_item(ui, "Export...") {
                ui_state.show_export = true;
                state.open = None;
                state.menu_scroll_y = 0.0;
            }
        }
        MenuId::Edit => {
            if menu_item(ui, "Undo") {
                out.request_undo = true;
                state.open = None;
                state.menu_scroll_y = 0.0;
            }
            if menu_item(ui, "Redo") {
                out.request_redo = true;
                state.open = None;
                state.menu_scroll_y = 0.0;
            }
        }
        MenuId::View => {
            checkbox(ui, "Mask Editor", &mut ui_state.show_mask_editor);
            checkbox(
                ui,
                "Content Browser / Presets",
                &mut ui_state.show_content_browser,
            );
            checkbox(ui, "Export", &mut ui_state.show_export);
            checkbox(ui, "2D Preview", &mut ui_state.show_2d_preview);
            checkbox(ui, "Profiler", &mut ui_state.show_profiler);
            checkbox(ui, "Pipeline Overview", &mut ui_state.show_pipeline);
            checkbox(ui, "History", &mut ui_state.show_history);
            checkbox(ui, "Bookmarks", &mut ui_state.show_bookmarks);
            checkbox(ui, "Minimap", &mut ui_state.show_minimap);
            ui.separator();
            if checkbox(
                ui,
                "Collapse Tool Panel",
                &mut ui_state.layout.tool_panel_collapsed,
            ) {
                ui_state.layout_dirty = true;
            }
            if checkbox(
                ui,
                "Collapse Inspector",
                &mut ui_state.layout.inspector_collapsed,
            ) {
                ui_state.layout_dirty = true;
            }
            if menu_item(ui, "Reset Layout") {
                ui_state.layout.reset();
                ui_state.layout_dirty = true;
                state.open = None;
                state.menu_scroll_y = 0.0;
            }
            ui.separator();
            checkbox(ui, "Widget Lab", &mut ui_state.show_widget_lab);
            if menu_item(ui, "Reset Camera") {
                out.camera_reset = true;
                state.open = None;
                state.menu_scroll_y = 0.0;
            }
            if menu_item(ui, "Top View") {
                out.camera_top_view = true;
                state.open = None;
                state.menu_scroll_y = 0.0;
            }
            if menu_item(ui, "Frame Selection") {
                out.camera_frame_selection = true;
                state.open = None;
                state.menu_scroll_y = 0.0;
            }
        }
    }
    ui.end_panel_scrolled(&mut state.menu_scroll_y);
    popup
}

fn workspace_default_mode(workspace: AppWorkspace) -> WorkspaceMode {
    match workspace {
        AppWorkspace::Terrain => WorkspaceMode::Generate,
        AppWorkspace::Materials => WorkspaceMode::Paint,
        AppWorkspace::Water => WorkspaceMode::Erosion,
        AppWorkspace::Biomes => WorkspaceMode::Biomes,
        AppWorkspace::Vegetation => WorkspaceMode::Scatter,
    }
}

fn menu_item(ui: &mut GuiContext<'_>, text: &str) -> bool {
    menu_row(ui, Id::new("menu_item").child(text), text)
}
