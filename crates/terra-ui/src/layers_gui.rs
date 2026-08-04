//! Right-rail (top): Photoshop-style layer stack with reorder, lock, masks, context menu.

use crate::panels::PanelAction;
use crate::thumbnails::ThumbnailCache;
use crate::{EditorTool, UiState};
use terra_core::document::TerrainDocument;
use terra_core::layer::{BlendMode, Layer, LayerId, LayerKind, StackNode};
use terra_core::mask::MaskId;
use terra_gui::style::{self, FONT_SCALE, LAYER_BLEND_H, LAYER_ROW_H, PAD, ROW_H};
use terra_gui::{combo, icon_button, slider_f32, Color, DrawList, GuiContext, Icon, Id, Rect};

const BLEND_MODES: [BlendMode; 7] = [
    BlendMode::Normal,
    BlendMode::Add,
    BlendMode::Subtract,
    BlendMode::Multiply,
    BlendMode::Min,
    BlendMode::Max,
    BlendMode::Overlay,
];
const BLEND_LABELS: [&str; 7] = [
    "Normal", "Add", "Subtract", "Multiply", "Min", "Max", "Overlay",
];

#[derive(Debug, Default)]
pub struct LayersGuiState {
    pub scroll_y: f32,
    pub add_menu_open: bool,
    /// Drag-reorder source stack index.
    pub drag_from: Option<usize>,
    /// Context menu for layer id (screen-ish anchor in panel coords).
    pub context_menu: Option<(LayerId, f32, f32)>,
    pub collapsed_groups: Vec<LayerId>,
    /// Procedural thumbnail slots; async GPU thumbnails are future work.
    pub thumbnails: ThumbnailCache,
}

pub fn draw_layers_gui(
    ui: &mut GuiContext<'_>,
    doc: &TerrainDocument,
    ui_state: &mut UiState,
    state: &mut LayersGuiState,
) -> Vec<PanelAction> {
    let mut actions = Vec::new();
    let panel = ui.right_layers_rect();

    if ui
        .input
        .pointer
        .map(|(x, y)| panel.contains(x, y))
        .unwrap_or(false)
    {
        ui.state.set_hot(Id::new("__layers_bg"));
    }

    ui.panel(panel, style::PANEL_BG);
    ui.panel(
        Rect::from_pos_size(panel.min_x, panel.min_y, 1.0, panel.height()),
        style::SEPARATOR,
    );
    ui.panel(
        Rect::from_pos_size(panel.min_x, panel.max_y - 1.0, panel.width(), 1.0),
        style::SEPARATOR,
    );

    let header = Rect::from_pos_size(panel.min_x, panel.min_y, panel.width(), style::HEADER_H);
    ui.panel(header, style::PANEL_BG);
    ui.icon_at(
        header.min_x + PAD,
        header.min_y + 8.0,
        Icon::Layers,
        style::TEXT_MUTED,
        14.0,
    );
    ui.label_at(
        header.min_x + PAD + 20.0,
        header.min_y + 11.0,
        "LAYERS",
        style::TEXT_MUTED,
        FONT_SCALE,
    );
    // Processing order — lower layers evaluate first.
    let hint = "Bottom -> Top";
    let hw = DrawList::text_width(hint, FONT_SCALE * 0.75);
    ui.label_at(
        header.max_x - 34.0 - hw - 8.0,
        header.min_y + 12.0,
        hint,
        style::TEXT_MUTED,
        FONT_SCALE * 0.75,
    );
    let add_hdr = Rect::from_pos_size(header.max_x - 34.0, header.min_y + 6.0, 26.0, 24.0);
    if icon_button(ui, Id::new("layers_hdr_add"), Icon::Plus, add_hdr) {
        actions.push(PanelAction::OpenQuickAdd);
        state.add_menu_open = false;
    }

    // Collapse / expand inspector (bottom of right rail).
    let insp_btn = Rect::from_pos_size(header.max_x - 62.0, header.min_y + 6.0, 24.0, 24.0);
    let insp_id = Id::new("layers_toggle_insp");
    let insp_hovered = ui.pointer_in(insp_btn);
    if insp_hovered {
        ui.state.set_hot(insp_id);
    }
    if insp_hovered && ui.input.primary_pressed {
        ui.state.active = Some(insp_id);
    }
    if ui.input.primary_released && ui.state.is_active(insp_id) && insp_hovered {
        ui_state.layout.inspector_collapsed = !ui_state.layout.inspector_collapsed;
        ui_state.layout_dirty = true;
    }
    ui.panel_rounded(
        insp_btn,
        if ui_state.layout.inspector_collapsed {
            style::ACCENT_SOFT
        } else {
            style::BUTTON_BG
        },
        style::RADIUS_SM,
    );
    ui.icon_at(
        insp_btn.min_x + 4.0,
        insp_btn.min_y + 4.0,
        if ui_state.layout.inspector_collapsed {
            Icon::ChevronUp
        } else {
            Icon::ChevronDown
        },
        style::TEXT_DIM,
        14.0,
    );

    let non_base_count = doc
        .stack
        .flatten_layers()
        .iter()
        .filter(|l| !l.kind.is_sculpt_base())
        .count();
    let selected_is_base = doc
        .selected
        .and_then(|id| doc.stack.find(id))
        .is_some_and(|l| l.kind.is_sculpt_base());
    let show_blend = non_base_count >= 1 && !selected_is_base;

    let blend_h = if show_blend { LAYER_BLEND_H } else { 0.0 };
    let footer_h = 36.0;
    let footer = Rect::from_pos_size(
        panel.min_x,
        panel.max_y - footer_h - blend_h,
        panel.width(),
        footer_h,
    );
    let blend_strip = Rect::from_pos_size(
        panel.min_x,
        panel.max_y - blend_h,
        panel.width(),
        blend_h.max(0.001),
    );

    if show_blend {
        draw_blend_opacity(ui, doc, blend_strip, &mut actions);
    }

    ui.panel(footer, style::SURFACE);
    ui.panel(
        Rect::from_pos_size(footer.min_x, footer.min_y, footer.width(), 1.0),
        style::SEPARATOR,
    );
    let btn_w = 28.0;
    let fy = footer.min_y + 4.0;
    let mut fx = footer.min_x + PAD;
    if icon_button(
        ui,
        Id::new("layers_add"),
        Icon::Plus,
        Rect::from_pos_size(fx, fy, btn_w, 28.0),
    ) {
        actions.push(PanelAction::OpenQuickAdd);
    }
    fx += btn_w + 6.0;
    if icon_button(
        ui,
        Id::new("layers_mask"),
        Icon::CircleDot,
        Rect::from_pos_size(fx, fy, btn_w, 28.0),
    ) {
        ui_state.show_mask_editor = true;
    }
    fx += btn_w + 6.0;
    if icon_button(
        ui,
        Id::new("layers_dup"),
        Icon::Copy,
        Rect::from_pos_size(fx, fy, btn_w, 28.0),
    ) {
        if !selected_is_base {
            actions.push(PanelAction::DuplicateSelected);
        }
    }
    fx += btn_w + 6.0;
    if icon_button(
        ui,
        Id::new("layers_del"),
        Icon::Trash2,
        Rect::from_pos_size(fx, fy, btn_w, 28.0),
    ) {
        if !selected_is_base {
            actions.push(PanelAction::RemoveSelected);
        }
    }
    fx += btn_w + 6.0;
    if icon_button(
        ui,
        Id::new("layers_group"),
        Icon::Folder,
        Rect::from_pos_size(fx, fy, btn_w, 28.0),
    ) {
        actions.push(PanelAction::AddGroup {
            name: "Group".into(),
        });
    }

    let list = Rect::from_min_max(panel.min_x, header.max_y, panel.max_x, footer.min_y);
    ui.begin_panel_scrolled(
        Id::new("layers_scroll"),
        list,
        Color::rgba(0.0, 0.0, 0.0, 0.0),
        &mut state.scroll_y,
    );

    // Empty stack state.
    if doc.stack.nodes.is_empty() {
        let tip = ui.allocate(80.0);
        ui.label_at(
            tip.min_x + 8.0,
            tip.min_y + 8.0,
            "Start by generating a base",
            style::TEXT,
            FONT_SCALE,
        );
        ui.label_at(
            tip.min_x + 8.0,
            tip.min_y + 26.0,
            "terrain or importing a heightmap.",
            style::TEXT_DIM,
            FONT_SCALE * 0.9,
        );
        if mini_selectable(
            ui,
            Id::new("empty_flat"),
            Rect::from_pos_size(tip.min_x + 8.0, tip.min_y + 48.0, 100.0, 20.0),
            "+ Flat Terrain",
            false,
        ) {
            actions.push(PanelAction::AddLayer(Layer::new(
                "Flat",
                LayerKind::Flat(Default::default()),
            )));
        }
        if mini_selectable(
            ui,
            Id::new("empty_mtn"),
            Rect::from_pos_size(tip.min_x + 120.0, tip.min_y + 48.0, 120.0, 20.0),
            "+ Mountain Range",
            false,
        ) {
            actions.push(PanelAction::AddLayer(Layer::new(
                "Mountains",
                LayerKind::Mountains(Default::default()),
            )));
        }
        ui.end_panel_scrolled(&mut state.scroll_y);
        return actions;
    }

    let rows: Vec<LayerRow> = collect_rows(doc);

    // Drop target while dragging.
    let mut drop_to: Option<usize> = None;

    for row_data in rows.iter().cloned().rev() {
        let selected = doc.selected == Some(row_data.id);
        let row = ui.allocate(LAYER_ROW_H);
        let hovered = ui.pointer_in(row);

        if selected {
            ui.panel_rounded(row, style::SELECTED_BG, style::RADIUS_SM);
            ui.panel(
                Rect::from_pos_size(row.min_x, row.min_y + 4.0, 3.0, row.height() - 8.0),
                style::ACCENT,
            );
        } else if hovered {
            ui.panel_rounded(row, style::SURFACE, style::RADIUS_SM);
        }

        // Colour tag strip.
        if row_data.color_tag > 0 {
            ui.panel(
                Rect::from_pos_size(row.min_x + 4.0, row.min_y + 8.0, 3.0, row.height() - 16.0),
                tag_color(row_data.color_tag),
            );
        }

        // Drag handle / reorder.
        let drag_id = Id::new("ldrag").child(&format!("{:?}", row_data.id));
        let handle = Rect::from_pos_size(row.min_x + 6.0, row.min_y + 14.0, 14.0, 20.0);
        if ui.pointer_in(handle) {
            ui.state.set_hot(drag_id);
        }
        if ui.pointer_in(handle) && ui.input.primary_pressed && !row_data.is_base {
            ui.state.active = Some(drag_id);
            state.drag_from = Some(row_data.idx);
        }
        ui.icon_at(
            handle.min_x,
            handle.min_y + 2.0,
            Icon::GripVertical,
            style::TEXT_MUTED,
            12.0,
        );

        if hovered && state.drag_from.is_some() && state.drag_from != Some(row_data.idx) {
            drop_to = Some(row_data.idx);
            ui.panel(
                Rect::from_pos_size(row.min_x + 4.0, row.min_y, row.width() - 8.0, 2.0),
                style::ACCENT,
            );
        }

        // Thumbnail slot. The cache supplies a cheap procedural placeholder until async GPU
        // thumbnails are available; terra-gui has one global image slot, so use its tint here.
        let icon_r = Rect::from_pos_size(row.min_x + 24.0, row.min_y + 10.0, 24.0, 24.0);
        let thumb = state.thumbnails.request_or_get(row_data.id);
        let (r, g, b) = (thumb.rgba[0], thumb.rgba[1], thumb.rgba[2]);
        let tint = Color::rgba(
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
            if row_data.enabled { 1.0 } else { 0.45 },
        );
        ui.panel_rounded(icon_r, tint, 4.0);
        let _thumbnail_dimensions = (thumb.width, thumb.height);
        ui.icon_at(
            icon_r.min_x + 4.0,
            icon_r.min_y + 4.0,
            row_data.type_icon,
            if row_data.enabled {
                style::TEXT_DIM
            } else {
                style::TEXT_DISABLED
            },
            16.0,
        );

        // Right-side controls: visibility always; solo/lock/mask on hover or selection.
        let mut rx = row.max_x - 8.0;
        let show_secondary = selected || hovered;
        if row_data.is_layer && !row_data.is_base {
            rx -= 22.0;
            let eye = Rect::from_pos_size(rx, row.min_y + (LAYER_ROW_H - 20.0) * 0.5, 20.0, 20.0);
            let icon = if row_data.enabled {
                Icon::Eye
            } else {
                Icon::EyeOff
            };
            if icon_button(
                ui,
                Id::new("leye").child(&format!("{:?}", row_data.id)),
                icon,
                eye,
            ) {
                actions.push(PanelAction::SetEnabled {
                    id: row_data.id,
                    enabled: !row_data.enabled,
                });
            }

            if show_secondary {
                rx -= 22.0;
                let lock = Rect::from_pos_size(rx, row.min_y + (LAYER_ROW_H - 20.0) * 0.5, 20.0, 20.0);
                let lock_col = if row_data.locked {
                    style::WARNING
                } else {
                    style::TEXT_MUTED
                };
                ui.panel_rounded(
                    lock,
                    if row_data.locked {
                        style::ACCENT_SOFT
                    } else {
                        style::BUTTON_BG
                    },
                    style::RADIUS_SM,
                );
                ui.icon_at(
                    lock.min_x + 3.0,
                    lock.min_y + 3.0,
                    Icon::Lock,
                    lock_col,
                    14.0,
                );
                let lock_id = Id::new("llock").child(&format!("{:?}", row_data.id));
                let lock_hovered = ui.pointer_in(lock);
                if lock_hovered {
                    ui.state.set_hot(lock_id);
                    if ui.input.primary_pressed {
                        ui.state.active = Some(lock_id);
                    }
                }
                if ui.input.primary_released && ui.state.is_active(lock_id) && lock_hovered {
                    actions.push(PanelAction::SetLocked {
                        id: row_data.id,
                        locked: !row_data.locked,
                    });
                }

                rx -= 22.0;
                let solo = Rect::from_pos_size(rx, row.min_y + (LAYER_ROW_H - 20.0) * 0.5, 20.0, 20.0);
                ui.panel_rounded(
                    solo,
                    if row_data.solo {
                        style::ACCENT_SOFT
                    } else {
                        style::BUTTON_BG
                    },
                    style::RADIUS_SM,
                );
                ui.icon_at(
                    solo.min_x + 2.0,
                    solo.min_y + 2.0,
                    Icon::Maximize2,
                    if row_data.solo {
                        style::WARNING
                    } else {
                        style::TEXT_MUTED
                    },
                    14.0,
                );
                let solo_id = Id::new("lsolo").child(&format!("{:?}", row_data.id));
                let solo_hovered = ui.pointer_in(solo);
                if solo_hovered {
                    ui.state.set_hot(solo_id);
                    if ui.input.primary_pressed {
                        ui.state.active = Some(solo_id);
                    }
                }
                if ui.input.primary_released && ui.state.is_active(solo_id) && solo_hovered {
                    actions.push(PanelAction::SetSolo {
                        id: row_data.id,
                        solo: !row_data.solo,
                    });
                }

                rx -= 22.0;
                let chip = Rect::from_pos_size(rx, row.min_y + (LAYER_ROW_H - 18.0) * 0.5, 18.0, 18.0);
                let has_mask = row_data.mask_count > 0;
                ui.panel_rounded(
                    chip,
                    if has_mask {
                        style::ACCENT_DIM
                    } else {
                        style::BUTTON_BG
                    },
                    4.0,
                );
                ui.icon_at(
                    chip.min_x + 1.0,
                    chip.min_y + 1.0,
                    Icon::CircleDot,
                    if has_mask {
                        style::TEXT
                    } else {
                        style::TEXT_MUTED
                    },
                    14.0,
                );
                let chip_id = Id::new("lmask").child(&format!("{:?}", row_data.id));
                let chip_hovered = ui.pointer_in(chip);
                if chip_hovered {
                    ui.state.set_hot(chip_id);
                }
                if chip_hovered && ui.input.primary_pressed {
                    ui.state.active = Some(chip_id);
                }
                if ui.input.primary_released && ui.state.is_active(chip_id) && chip_hovered {
                    if let Some(mid) = row_data.first_mask {
                        ui_state.selected_mask = Some(mid);
                    }
                    if ui_state.shift_context {
                        ui_state.preview_mode = crate::Preview2dMode::Masks;
                    } else {
                        ui_state.show_mask_editor = true;
                    }
                    actions.push(PanelAction::Select(row_data.id));
                }
            }
        }

        // Cached badge only.
        if row_data.cached {
            ui.label_at(
                icon_r.max_x + 2.0,
                row.min_y + 2.0,
                "*",
                style::SUCCESS,
                FONT_SCALE * 0.7,
            );
        }

        let name_rect = Rect::from_min_max(row.min_x + 52.0, row.min_y, rx - 4.0, row.max_y);
        let name_id = Id::new("lsel").child(&format!("{:?}", row_data.id));
        if mini_selectable(ui, name_id, name_rect, &row_data.name, false) {
            actions.push(PanelAction::Select(row_data.id));
            state.add_menu_open = false;
            state.context_menu = None;
            if row_data.is_base {
                if !ui_state.editor_tool.is_sculpt() {
                    ui_state.editor_tool = EditorTool::Raise;
                }
                ui_state.paint_mask = None;
                ui_state.workspace_mode = crate::WorkspaceMode::Sculpt;
            } else if ui_state.editor_tool.is_sculpt() {
                ui_state.editor_tool = EditorTool::Move;
            }
        }

        // Shift+click opens context menu (secondary mouse button not in terra-gui yet).
        if hovered && ui.input.primary_pressed {
            if ui_state.shift_context {
                if let Some((px, py)) = ui.input.pointer {
                    state.context_menu = Some((row_data.id, px, py));
                }
            }
        }

        let subtitle = if row_data.is_base {
            "Sculpt foundation"
        } else {
            row_data.subtitle
        };
        let sub_color = if row_data.enabled {
            style::TEXT_MUTED
        } else {
            style::TEXT_DISABLED
        };
        let sub_clipped = DrawList::truncate_to_width(
            subtitle,
            FONT_SCALE * 0.82,
            (name_rect.width() - 4.0).max(8.0),
        );
        ui.label_at(
            name_rect.min_x + 2.0,
            name_rect.min_y + 24.0,
            &sub_clipped,
            sub_color,
            FONT_SCALE * 0.82,
        );
    }

    // Complete reorder on release.
    if ui.input.primary_released {
        if let (Some(from), Some(to)) = (state.drag_from.take(), drop_to) {
            if from != to {
                actions.push(PanelAction::Reorder { from, to });
            }
        } else {
            state.drag_from = None;
        }
    }

    ui.end_panel_scrolled(&mut state.scroll_y);

    // Context menu overlay.
    if let Some((cid, mx, my)) = state.context_menu {
        draw_layer_context_menu(ui, doc, cid, mx, my, state, &mut actions);
    }

    let _ = ROW_H;
    actions
}

fn collect_rows(doc: &TerrainDocument) -> Vec<LayerRow> {
    doc.stack
        .nodes
        .iter()
        .enumerate()
        .filter_map(|(idx, node)| match node {
            StackNode::Layer(layer) => Some(LayerRow {
                idx,
                id: layer.id(),
                name: layer.common.name.clone(),
                enabled: layer.common.enabled,
                locked: layer.common.locked,
                solo: layer.common.solo,
                color_tag: layer.common.color_tag,
                cached: layer.common.cached,
                is_layer: true,
                is_base: layer.kind.is_sculpt_base(),
                type_icon: layer_type_icon(&layer.kind),
                subtitle: kind_subtitle(&layer.kind),
                mask_count: layer.common.masks.len(),
                first_mask: layer.common.masks.first().map(|m| m.id),
            }),
            StackNode::Group(g) => Some(LayerRow {
                idx,
                id: g.id,
                name: g.name.clone(),
                enabled: g.enabled,
                locked: false,
                solo: false,
                color_tag: 0,
                cached: false,
                is_layer: false,
                is_base: false,
                type_icon: Icon::Folder,
                subtitle: "Group",
                mask_count: 0,
                first_mask: None,
            }),
        })
        .collect()
}

#[derive(Clone)]
struct LayerRow {
    idx: usize,
    id: LayerId,
    name: String,
    enabled: bool,
    locked: bool,
    solo: bool,
    color_tag: u8,
    cached: bool,
    is_layer: bool,
    is_base: bool,
    type_icon: Icon,
    subtitle: &'static str,
    mask_count: usize,
    first_mask: Option<MaskId>,
}

fn draw_layer_context_menu(
    ui: &mut GuiContext<'_>,
    doc: &TerrainDocument,
    id: LayerId,
    x: f32,
    y: f32,
    state: &mut LayersGuiState,
    actions: &mut Vec<PanelAction>,
) {
    let items: &[(&str, &str)] = &[
        ("rename", "Rename"),
        ("dup", "Duplicate"),
        ("del", "Delete"),
        ("group", "New Group"),
        ("enable", "Enable / Disable"),
        ("solo", "Solo"),
        ("lock", "Lock"),
        ("mask", "Add Mask"),
        ("seed", "Randomize Seed"),
        ("cache", "Toggle Cache"),
        ("close", "Close"),
    ];
    let w = 160.0;
    let h = items.len() as f32 * 24.0 + 8.0;
    let menu = Rect::from_pos_size(
        x.min(ui.screen_w - w - 4.0),
        y.min(ui.screen_h - h - 4.0),
        w,
        h,
    );
    ui.begin_overlay();
    ui.panel_rounded(menu, style::POPUP_BG, style::RADIUS_SM);
    ui.state.set_hot(Id::new("__layer_ctx"));

    let layer = doc.stack.find(id);
    let is_base = layer.is_some_and(|l| l.kind.is_sculpt_base());

    let mut iy = menu.min_y + 4.0;
    for (key, label) in items {
        let row = Rect::from_pos_size(menu.min_x + 4.0, iy, w - 8.0, 22.0);
        let disabled = is_base && matches!(*key, "dup" | "del" | "enable");
        let hovered = ui.pointer_in(row) && !disabled;
        if hovered {
            ui.panel_rounded(row, style::HOVER_BG, 3.0);
        }
        ui.label_at(
            row.min_x + 8.0,
            row.min_y + 4.0,
            label,
            if disabled {
                style::TEXT_DISABLED
            } else {
                style::TEXT
            },
            FONT_SCALE * 0.9,
        );
        if hovered && ui.input.primary_released {
            match *key {
                "dup" if !is_base => actions.push(PanelAction::DuplicateSelected),
                "del" if !is_base => actions.push(PanelAction::RemoveSelected),
                "group" => actions.push(PanelAction::AddGroup {
                    name: "Group".into(),
                }),
                "enable" => {
                    if let Some(l) = layer {
                        actions.push(PanelAction::SetEnabled {
                            id,
                            enabled: !l.common.enabled,
                        });
                    }
                }
                "solo" => {
                    if let Some(l) = layer {
                        actions.push(PanelAction::SetSolo {
                            id,
                            solo: !l.common.solo,
                        });
                    }
                }
                "lock" => {
                    if let Some(l) = layer {
                        actions.push(PanelAction::SetLocked {
                            id,
                            locked: !l.common.locked,
                        });
                    }
                }
                "mask" => {
                    actions.push(PanelAction::Select(id));
                    // Mask editor opened by caller via ui_state — emit select only.
                }
                "seed" => actions.push(PanelAction::RandomizeSeed { id }),
                "cache" => {
                    if let Some(l) = layer {
                        actions.push(PanelAction::SetCached {
                            id,
                            cached: !l.common.cached,
                        });
                    }
                }
                _ => {}
            }
            state.context_menu = None;
        }
        iy += 24.0;
    }

    // Click outside closes.
    if ui.input.primary_pressed && !ui.pointer_in(menu) {
        state.context_menu = None;
    }
    ui.end_overlay();
}

fn draw_blend_opacity(
    ui: &mut GuiContext<'_>,
    doc: &TerrainDocument,
    strip: Rect,
    actions: &mut Vec<PanelAction>,
) {
    ui.panel(strip, style::PANEL_BG);
    ui.panel(
        Rect::from_pos_size(strip.min_x, strip.min_y, strip.width(), 1.0),
        style::SEPARATOR,
    );

    let Some(id) = doc.selected else {
        return;
    };
    let Some(layer) = doc.stack.find(id) else {
        return;
    };
    if layer.kind.is_sculpt_base() {
        return;
    }

    ui.begin_panel(
        Rect::from_min_max(
            strip.min_x + 2.0,
            strip.min_y + 2.0,
            strip.max_x - 2.0,
            strip.max_y - 2.0,
        ),
        Color::rgba(0.0, 0.0, 0.0, 0.0),
    );

    let mut blend_idx = BLEND_MODES
        .iter()
        .position(|b| *b == layer.common.blend)
        .unwrap_or(0);
    if combo(ui, "Blend Mode", &mut blend_idx, &BLEND_LABELS) {
        actions.push(PanelAction::SetBlend {
            id,
            blend: BLEND_MODES[blend_idx.min(BLEND_MODES.len() - 1)],
        });
    }
    let mut opacity = layer.common.opacity;
    if slider_f32(ui, "Opacity", &mut opacity, 0.0, 1.0) {
        actions.push(PanelAction::SetOpacity { id, opacity });
    }
    ui.end_panel();
}

pub fn layer_type_icon(kind: &LayerKind) -> Icon {
    use LayerKind::*;
    match kind {
        SculptBase(_) => Icon::Pencil,
        Flat(_) | Ramp(_) | Plateau(_) => Icon::Box,
        Mountains(_) | Dunes(_) | Canyons(_) | VoronoiRegions(_) => Icon::Mountain,
        Terrace(_) => Icon::Layers,
        NoiseValue(_) | NoisePerlin(_) | NoiseOpenSimplex(_) | NoiseWorley(_) | Fbm(_)
        | Ridged(_) | DomainWarp(_) => Icon::Sparkles,
        ThermalErosion(_) | HydraulicErosion(_) | RiverCarve(_) | Blur(_) | Coastal(_) => {
            Icon::Droplets
        }
        Materials(_) => Icon::Paintbrush,
        Biomes(_) => Icon::Layers,
        Vegetation(_) => Icon::Sparkles,
        ImportHeightmap(_) => Icon::Download,
    }
}

fn kind_subtitle(kind: &LayerKind) -> &'static str {
    use LayerKind::*;
    match kind {
        SculptBase(_) => "Sculpt",
        Flat(_) => "Flat",
        Ramp(_) => "Ramp",
        NoiseValue(_) => "Value Noise",
        NoisePerlin(_) => "Perlin",
        NoiseOpenSimplex(_) => "OpenSimplex",
        NoiseWorley(_) => "Worley",
        Fbm(_) => "fBm",
        Ridged(_) => "Ridged",
        DomainWarp(_) => "Domain Warp",
        Terrace(_) => "Terrace",
        Plateau(_) => "Plateau",
        Mountains(_) => "Mountains",
        Dunes(_) => "Dunes",
        Canyons(_) => "Canyons",
        VoronoiRegions(_) => "Voronoi",
        ImportHeightmap(_) => "Import",
        ThermalErosion(_) => "Thermal",
        HydraulicErosion(_) => "Hydraulic",
        RiverCarve(_) => "Rivers",
        Blur(_) => "Blur",
        Coastal(_) => "Coastal",
        Materials(_) => "Materials",
        Biomes(_) => "Biomes",
        Vegetation(_) => "Vegetation",
    }
}

fn tag_color(tag: u8) -> Color {
    match tag {
        1 => style::TAG_RED,
        2 => style::TAG_ORANGE,
        3 => style::TAG_YELLOW,
        4 => style::TAG_GREEN,
        5 => style::TAG_BLUE,
        6 => style::TAG_PURPLE,
        _ => style::TAG_GRAY,
    }
}

fn mini_selectable(
    ui: &mut GuiContext<'_>,
    id: Id,
    rect: Rect,
    text: &str,
    selected: bool,
) -> bool {
    let hovered = ui.pointer_in(rect);
    if hovered {
        ui.state.set_hot(id);
    }
    if hovered && ui.input.primary_pressed {
        ui.state.active = Some(id);
    }
    let clicked = ui.input.primary_released && ui.state.is_active(id) && hovered;
    if selected {
        ui.panel(rect, style::SELECTED_BG);
    }
    let max_w = (rect.width() - 4.0).max(8.0);
    let clipped = DrawList::truncate_to_width(text, FONT_SCALE, max_w);
    ui.label_at(
        rect.min_x + 2.0,
        rect.min_y + 8.0,
        &clipped,
        style::TEXT,
        FONT_SCALE,
    );
    clicked
}
