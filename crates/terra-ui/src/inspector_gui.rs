//! Inspector panel drawn with `terra-gui` (replaces egui right panel content).

use crate::panels::PanelAction;
use crate::presets::contextual_presets;
use crate::{layers_gui, EditorTool, InspectorSection, UiState};
use terra_core::document::TerrainDocument;
use terra_core::layer::*;
use terra_gui::style::{self, FONT_SCALE, PAD};
use terra_gui::{
    button_id, checkbox, collapsible_section, icon_toggle, label, section_header, slider_f32,
    slider_f32_id, slider_i32, slider_i32_id, Color, GuiContext, Icon, Id, Rect,
};

/// Persistent scroll for the inspector panel.
#[derive(Debug, Default)]
pub struct InspectorGuiState {
    pub scroll_y: f32,
    /// Expansion state indexed by `InspectorSection::index()`.
    pub expanded_sections: [bool; 9],
    /// The layer whose default expansion state was initialized.
    pub expanded_for: Option<LayerId>,
    /// More-menu open for the selected layer.
    pub more_menu_open: bool,
    /// Inline rename buffer when renaming from the More menu.
    pub rename_buffer: Option<String>,
}

pub fn draw_inspector_gui(
    ui: &mut GuiContext<'_>,
    doc: &TerrainDocument,
    ui_state: &mut UiState,
    state: &mut InspectorGuiState,
) -> Vec<PanelAction> {
    let mut actions = Vec::new();
    let panel = ui.right_inspector_rect();

    if ui_state.layout.inspector_collapsed {
        if ui.pointer_in(panel) {
            ui.state.set_hot(Id::new("__inspector_collapsed"));
        }
        ui.panel(panel, style::PANEL_BG);
        let label = "Inspector (collapsed) - click to expand";
        ui.label_at(
            panel.min_x + PAD,
            panel.min_y + 10.0,
            label,
            style::TEXT_MUTED,
            FONT_SCALE * 0.85,
        );
        if ui.pointer_in(panel) && ui.input.primary_released {
            ui_state.layout.inspector_collapsed = false;
            ui_state.layout_dirty = true;
        }
        return actions;
    }

    // Absorb camera while hovering the inspector chrome.
    if ui
        .input
        .pointer
        .map(|(x, y)| panel.contains(x, y))
        .unwrap_or(false)
    {
        ui.state.set_hot(Id::new("__inspector_bg"));
    }

    ui.panel(panel, style::PANEL_BG);
    ui.panel(
        Rect::from_pos_size(panel.min_x, panel.min_y, 1.0, panel.height()),
        style::SEPARATOR,
    );

    let header = Rect::from_pos_size(panel.min_x, panel.min_y, panel.width(), style::HEADER_H);
    ui.panel(header, style::PANEL_BG);
    let header_title = doc
        .selected
        .and_then(|id| doc.stack.find(id))
        .map(|l| {
            format!(
                "{} ({})",
                l.common.name,
                kind_display_name(&l.kind)
            )
        })
        .unwrap_or_else(|| "INSPECTOR".into());
    // Truncate long titles so they don't collide with chrome.
    let title = if header_title.len() > 28 {
        format!("{}...", &header_title[..25])
    } else {
        header_title
    };
    ui.label_at(
        header.min_x + PAD,
        header.min_y + 11.0,
        &title,
        style::TEXT_MUTED,
        FONT_SCALE * 0.9,
    );

    let body = Rect::from_min_max(panel.min_x, header.max_y, panel.max_x, panel.max_y);
    ui.begin_panel_scrolled(
        Id::new("inspector_scroll"),
        body,
        Color::rgba(0.0, 0.0, 0.0, 0.0),
        &mut state.scroll_y,
    );

    // Sculpt inspector when Base is the focus; mask paint when that tool is armed.
    let selected_is_base = doc
        .selected
        .and_then(|id| doc.stack.find(id))
        .is_some_and(|l| l.kind.is_sculpt_base());
    if ui_state.editor_tool == EditorTool::PaintMask {
        draw_tool_inspector(ui, doc, ui_state);
        ui.end_panel_scrolled(&mut state.scroll_y);
        return actions;
    }
    if ui_state.editor_tool.is_sculpt() && (selected_is_base || doc.selected.is_none()) {
        draw_tool_inspector(ui, doc, ui_state);
        ui.end_panel_scrolled(&mut state.scroll_y);
        return actions;
    }

    let Some(id) = doc.selected else {
        label(ui, "Select a layer to edit its properties.");
        label(ui, "Or begin with a terrain building block.");
        if button_id(ui, Id::new("insp_quick_add"), "Browse layer tools") {
            actions.push(PanelAction::OpenQuickAdd);
        }
        if button_id(ui, Id::new("insp_add_mountain"), "Add Mountains") {
            actions.push(PanelAction::AddLayer(Layer::new(
                "Mountains",
                LayerKind::Mountains(MountainParams::default()),
            )));
        }
        if button_id(ui, Id::new("insp_add_erosion"), "Add Hydraulic Erosion") {
            actions.push(PanelAction::AddLayer(Layer::new(
                "Hydraulic Erosion",
                LayerKind::HydraulicErosion(HydraulicErosionParams::default()),
            )));
        }
        if button_id(ui, Id::new("insp_add_material"), "Add Materials") {
            actions.push(PanelAction::AddLayer(Layer::new(
                "Materials",
                LayerKind::Materials(MaterialsParams::default()),
            )));
        }
        ui.end_panel_scrolled(&mut state.scroll_y);
        return actions;
    };
    let Some(layer) = doc.stack.find(id).cloned() else {
        ui.end_panel_scrolled(&mut state.scroll_y);
        return actions;
    };

    let kind_name = kind_display_name(&layer.kind);
    let primary = primary_section(&layer.kind);
    initialize_sections(state, id, primary);

    // Layer header: icon + name/subtitle + interactive Enabled / Solo / Lock / More.
    let card = ui.allocate(64.0);
    ui.panel_rounded(card, style::SURFACE, style::RADIUS_MD);
    ui.panel_rounded(
        Rect::from_pos_size(card.min_x + 8.0, card.min_y + 14.0, 32.0, 32.0),
        style::BUTTON_BG,
        6.0,
    );
    ui.icon_at(
        card.min_x + 16.0,
        card.min_y + 22.0,
        layers_gui::layer_type_icon(&layer.kind),
        style::ACCENT,
        16.0,
    );
    ui.label_at(
        card.min_x + 48.0,
        card.min_y + 14.0,
        &layer.common.name,
        style::TEXT,
        FONT_SCALE,
    );
    ui.label_at(
        card.min_x + 48.0,
        card.min_y + 32.0,
        kind_name,
        style::TEXT_MUTED,
        FONT_SCALE * 0.9,
    );

    // Place controls on the right of the header card.
    let mut cx = card.max_x - 10.0;
    let more_r = Rect::from_pos_size(cx - 24.0, card.min_y + 18.0, 24.0, 24.0);
    cx -= 28.0;
    let lock_r = Rect::from_pos_size(cx - 24.0, card.min_y + 18.0, 24.0, 24.0);
    cx -= 28.0;
    let solo_r = Rect::from_pos_size(cx - 24.0, card.min_y + 18.0, 24.0, 24.0);
    cx -= 28.0;
    let en_r = Rect::from_pos_size(cx - 24.0, card.min_y + 18.0, 24.0, 24.0);

    // Enabled toggle (checkbox look).
    let en_id = Id::new("insp_en_toggle");
    let en_hovered = ui.pointer_in(en_r);
    if en_hovered {
        ui.state.set_hot(en_id);
    }
    if en_hovered && ui.input.primary_pressed {
        ui.state.active = Some(en_id);
    }
    if ui.input.primary_released && ui.state.is_active(en_id) && en_hovered {
        actions.push(PanelAction::SetEnabled {
            id,
            enabled: !layer.common.enabled,
        });
    }
    ui.panel_rounded(
        en_r,
        if layer.common.enabled {
            style::CHECK_ON
        } else {
            style::CHECK_BG
        },
        style::RADIUS_SM,
    );
    if layer.common.enabled {
        ui.icon_at(en_r.min_x + 4.0, en_r.min_y + 4.0, Icon::Check, style::TEXT, 16.0);
    }

    if icon_toggle(
        ui,
        Id::new("insp_solo_toggle"),
        Icon::Maximize2,
        solo_r,
        layer.common.solo,
    ) {
        actions.push(PanelAction::SetSolo {
            id,
            solo: !layer.common.solo,
        });
    }
    if icon_toggle(
        ui,
        Id::new("insp_lock_toggle"),
        Icon::Lock,
        lock_r,
        layer.common.locked,
    ) {
        actions.push(PanelAction::SetLocked {
            id,
            locked: !layer.common.locked,
        });
    }
    if icon_toggle(
        ui,
        Id::new("insp_more_toggle"),
        Icon::Ellipsis,
        more_r,
        state.more_menu_open,
    ) {
        state.more_menu_open = !state.more_menu_open;
        if state.more_menu_open {
            state.rename_buffer = None;
        }
    }

    if state.more_menu_open {
        draw_inspector_more_menu(ui, ui_state, state, &layer, id, more_r, &mut actions);
    }

    // Inline rename field when active.
    if let Some(buf) = state.rename_buffer.as_mut() {
        let rename_id = Id::new("insp_rename_field");
        let row = ui.allocate(30.0);
        if ui.state.text_focus != Some(rename_id) {
            ui.state.text_focus = Some(rename_id);
            ui.state.text_buffer = buf.clone();
        }
        ui.panel_rounded(row, style::INPUT_BG, style::RADIUS_SM);
        let display = ui.state.text_buffer.clone();
        ui.label_at(
            row.min_x + 8.0,
            row.min_y + 7.0,
            &display,
            style::TEXT,
            FONT_SCALE,
        );
        if ui.state.text_focus == Some(rename_id) {
            if !ui.input.text.is_empty() {
                ui.state.text_buffer.push_str(&ui.input.text);
            }
            if ui.input.backspace_pressed {
                ui.state.text_buffer.pop();
            }
            *buf = ui.state.text_buffer.clone();
            if ui.input.enter_pressed || ui.state.text_enter {
                let name = ui.state.text_buffer.trim().to_string();
                if !name.is_empty() {
                    actions.push(PanelAction::Rename { id, name });
                }
                state.rename_buffer = None;
                ui.state.clear_text_focus();
            }
            if ui.input.escape_pressed {
                state.rename_buffer = None;
                ui.state.clear_text_focus();
            }
        }
    }

    if section(ui, state, InspectorSection::General) {
        if layer.kind.is_sculpt_base() {
            label(ui, "Use Raise / Lower / Smooth in the left palette.");
            if button_id(ui, Id::new("reset_sculpt"), "Reset Base heights") {
                actions.push(PanelAction::ResetSculptBase { id });
            }
        } else {
            label(ui, "Opacity and blend live in the Layers panel.");
        }
    }

    if section(ui, state, primary) {
        let mut kind = layer.kind.clone();
        if edit_kind(ui, &mut kind, false) {
            actions.push(PanelAction::SetKind { id, kind });
        }
    }

    let presets = contextual_presets(&layer.kind);
    if !presets.is_empty() && section(ui, state, InspectorSection::Presets) {
        for (index, preset) in presets.into_iter().enumerate() {
            if button_id(
                ui,
                Id::new("insp_context_preset").with(index as u64),
                preset.name,
            ) {
                actions.push(PanelAction::SetKind {
                    id,
                    kind: preset.kind,
                });
            }
        }
    }

    if section(ui, state, InspectorSection::Advanced) {
        ui_state.inspector_advanced = true;
        let mut kind = layer.kind.clone();
        if edit_kind(ui, &mut kind, true) {
            actions.push(PanelAction::SetKind { id, kind });
        }
    } else {
        ui_state.inspector_advanced = false;
    }

    if section(ui, state, InspectorSection::Performance) {
        label(
            ui,
            &format!("Preview quality: {}", quality_label(ui_state.quality)),
        );
        label(
            ui,
            &format!("Eval: {:.1} ms", ui_state.profile.eval_us as f32 / 1000.0),
        );
        let backend = if ui_state.profile.path.is_empty() {
            "CPU"
        } else {
            ui_state.profile.path
        };
        label(ui, &format!("Backend: {backend}"));
        if ui_state.refining {
            label(ui, "Status: refining...");
        } else if ui_state.draft_displayed {
            label(ui, "Status: interactive draft");
        } else {
            label(ui, "Status: cached / ready");
        }
    }

    ui.end_panel_scrolled(&mut state.scroll_y);
    actions
}

fn draw_inspector_more_menu(
    ui: &mut GuiContext<'_>,
    ui_state: &mut UiState,
    state: &mut InspectorGuiState,
    layer: &Layer,
    id: LayerId,
    anchor: Rect,
    actions: &mut Vec<PanelAction>,
) {
    let items: &[(&str, &str)] = &[
        ("rename", "Rename"),
        ("dup", "Duplicate"),
        ("reset", "Reset Parameters"),
        ("copy", "Copy Settings"),
        ("paste", "Paste Settings"),
        ("cache", "Toggle Cache"),
        ("del", "Delete"),
        ("close", "Close"),
    ];
    let w = 170.0;
    let h = items.len() as f32 * 26.0 + 8.0;
    let menu = Rect::from_pos_size(
        (anchor.max_x - w).max(8.0),
        anchor.max_y + 4.0,
        w,
        h,
    );
    ui.begin_overlay();
    ui.panel_rounded(menu, style::POPUP_BG, style::RADIUS_SM);
    if ui.pointer_in(menu) {
        ui.state.set_hot(Id::new("insp_more_menu"));
    }
    let is_base = layer.kind.is_sculpt_base();
    let mut iy = menu.min_y + 4.0;
    for (i, (key, label_text)) in items.iter().enumerate() {
        let row = Rect::from_pos_size(menu.min_x + 4.0, iy, w - 8.0, 24.0);
        let disabled = match *key {
            "dup" | "del" => is_base,
            "paste" => ui_state.settings_clipboard.is_none() || is_base,
            "reset" => false,
            _ => false,
        };
        let hovered = ui.pointer_in(row) && !disabled;
        let id_row = Id::new("insp_more_item").with(i as u64);
        if hovered {
            ui.state.set_hot(id_row);
            ui.panel_rounded(row, style::HOVER_BG, style::RADIUS_SM);
        }
        if hovered && ui.input.primary_pressed {
            ui.state.active = Some(id_row);
        }
        ui.label_at(
            row.min_x + 8.0,
            row.min_y + 5.0,
            label_text,
            if disabled {
                style::TEXT_DISABLED
            } else {
                style::TEXT
            },
            FONT_SCALE * 0.9,
        );
        if ui.input.primary_released && ui.state.is_active(id_row) && hovered {
            match *key {
                "rename" => {
                    state.rename_buffer = Some(layer.common.name.clone());
                }
                "dup" if !is_base => actions.push(PanelAction::DuplicateSelected),
                "reset" => {
                    if is_base {
                        actions.push(PanelAction::ResetSculptBase { id });
                    } else {
                        actions.push(PanelAction::SetKind {
                            id,
                            kind: default_kind_like(&layer.kind),
                        });
                    }
                }
                "copy" => {
                    ui_state.settings_clipboard = Some(layer.kind.clone());
                }
                "paste" => {
                    if let Some(kind) = ui_state.settings_clipboard.clone() {
                        actions.push(PanelAction::SetKind { id, kind });
                    }
                }
                "cache" => actions.push(PanelAction::SetCached {
                    id,
                    cached: !layer.common.cached,
                }),
                "del" if !is_base => actions.push(PanelAction::RemoveSelected),
                _ => {}
            }
            if *key != "rename" {
                state.more_menu_open = false;
            } else {
                state.more_menu_open = false;
            }
        }
        iy += 26.0;
    }
    if ui.input.primary_pressed && !ui.pointer_in(menu) && !ui.pointer_in(anchor) {
        state.more_menu_open = false;
    }
    ui.end_overlay();
}

fn default_kind_like(kind: &LayerKind) -> LayerKind {
    use LayerKind::*;
    match kind {
        SculptBase(_) => SculptBase(Default::default()),
        Flat(_) => Flat(Default::default()),
        Ramp(_) => Ramp(Default::default()),
        NoiseValue(_) => NoiseValue(Default::default()),
        NoisePerlin(_) => NoisePerlin(Default::default()),
        NoiseOpenSimplex(_) => NoiseOpenSimplex(Default::default()),
        NoiseWorley(_) => NoiseWorley(Default::default()),
        Fbm(_) => Fbm(Default::default()),
        Ridged(_) => Ridged(Default::default()),
        DomainWarp(_) => DomainWarp(Default::default()),
        Terrace(_) => Terrace(Default::default()),
        Plateau(_) => Plateau(Default::default()),
        Mountains(_) => Mountains(Default::default()),
        Canyons(_) => Canyons(Default::default()),
        Dunes(_) => Dunes(Default::default()),
        VoronoiRegions(_) => VoronoiRegions(Default::default()),
        ImportHeightmap(_) => ImportHeightmap(Default::default()),
        Blur(_) => Blur(Default::default()),
        ThermalErosion(_) => ThermalErosion(Default::default()),
        HydraulicErosion(_) => HydraulicErosion(Default::default()),
        RiverCarve(_) => RiverCarve(Default::default()),
        Coastal(_) => Coastal(Default::default()),
        Materials(_) => Materials(Default::default()),
        Biomes(_) => Biomes(Default::default()),
        Vegetation(_) => Vegetation(Default::default()),
    }
}

fn quality_label(quality: terra_core::eval::PreviewQuality) -> &'static str {
    match quality {
        terra_core::eval::PreviewQuality::Draft => "Draft",
        terra_core::eval::PreviewQuality::Medium => "Medium",
        terra_core::eval::PreviewQuality::Full => "Full",
        terra_core::eval::PreviewQuality::Export => "Export",
    }
}

fn initialize_sections(state: &mut InspectorGuiState, id: LayerId, primary: InspectorSection) {
    if state.expanded_for == Some(id) {
        return;
    }
    state.expanded_sections = [false; 9];
    state.expanded_sections[InspectorSection::General.index()] = true;
    state.expanded_sections[primary.index()] = true;
    state.expanded_for = Some(id);
}

fn section(
    ui: &mut GuiContext<'_>,
    state: &mut InspectorGuiState,
    section: InspectorSection,
) -> bool {
    let index = section.index();
    let mut expanded = state.expanded_sections[index];
    let title = section.label().to_uppercase();
    let open = collapsible_section(
        ui,
        Id::new("insp_section").with(index as u64),
        &title,
        &mut expanded,
    );
    state.expanded_sections[index] = expanded;
    open
}

fn primary_section(kind: &LayerKind) -> InspectorSection {
    match kind {
        LayerKind::ThermalErosion(_)
        | LayerKind::HydraulicErosion(_)
        | LayerKind::RiverCarve(_) => InspectorSection::Erosion,
        LayerKind::Materials(_) | LayerKind::Biomes(_) | LayerKind::Vegetation(_) => {
            InspectorSection::Output
        }
        LayerKind::Fbm(_)
        | LayerKind::Ridged(_)
        | LayerKind::NoiseValue(_)
        | LayerKind::NoisePerlin(_)
        | LayerKind::NoiseOpenSimplex(_)
        | LayerKind::NoiseWorley(_) => InspectorSection::Details,
        _ => InspectorSection::Shape,
    }
}

fn draw_tool_inspector(ui: &mut GuiContext<'_>, doc: &TerrainDocument, ui_state: &mut UiState) {
    if ui_state.editor_tool.is_sculpt() {
        let tool_name = match ui_state.editor_tool {
            EditorTool::Raise => "Raise",
            EditorTool::Lower => "Lower",
            EditorTool::Smooth => "Smooth",
            _ => "Sculpt",
        };
        label(ui, &format!("Sculpt · {tool_name}"));
        label(ui, "Drag in the viewport to paint Base heights.");
        slider_f32(ui, "Radius", &mut ui_state.sculpt_radius, 0.01, 0.2);
        if ui_state.editor_tool == EditorTool::Smooth {
            let mut s = (ui_state.sculpt_strength / 10.0).clamp(0.05, 1.0);
            if slider_f32(ui, "Strength", &mut s, 0.05, 1.0) {
                ui_state.sculpt_strength = s * 10.0;
            }
        } else {
            slider_f32(ui, "Strength (m)", &mut ui_state.sculpt_strength, 0.5, 40.0);
        }
        if let Some(base) = doc
            .stack
            .flatten_layers()
            .iter()
            .find(|l| l.kind.is_sculpt_base())
        {
            label(ui, &format!("Target: {}", base.common.name));
        }
        return;
    }

    label(ui, "Brush · Paint Mask");
    label(ui, "Constrains the selected upper layer.");
    if let Some(mask_id) = ui_state.paint_mask {
        let name = doc
            .masks
            .iter()
            .find(|m| m.id == mask_id)
            .map(|m| m.name.as_str())
            .unwrap_or("Mask");
        label(ui, &format!("Target: {name}"));
    } else {
        label(ui, "No paint target - open Mask Editor.");
    }
    if button_id(ui, Id::new("insp_open_mask"), "Open Mask Editor") {
        ui_state.show_mask_editor = true;
    }
}

fn edit_kind(ui: &mut GuiContext<'_>, kind: &mut LayerKind, advanced: bool) -> bool {
    let mut changed = false;
    match kind {
        LayerKind::SculptBase(p) => {
            changed |= slider_f32(ui, "Fill height", &mut p.fill_height, -100.0, 500.0);
        }
        LayerKind::Flat(p) => {
            changed |= slider_f32(ui, "Height", &mut p.height, -500.0, 2000.0);
        }
        LayerKind::Ramp(p) => {
            changed |= slider_f32(ui, "Min", &mut p.height_min, -200.0, 500.0);
            changed |= slider_f32(ui, "Max", &mut p.height_max, -200.0, 2000.0);
            if advanced {
                changed |= slider_f32(ui, "Direction", &mut p.direction, 0.0, 6.28);
            }
        }
        LayerKind::NoiseValue(p) | LayerKind::NoisePerlin(p) | LayerKind::NoiseOpenSimplex(p) => {
            changed |= edit_noise(ui, p, advanced);
        }
        LayerKind::NoiseWorley(p) => {
            changed |= edit_noise(ui, &mut p.base, advanced);
        }
        LayerKind::Fbm(p) | LayerKind::Ridged(p) => {
            changed |= edit_noise(ui, &mut p.base, advanced);
        }
        LayerKind::DomainWarp(p) => {
            changed |= edit_noise(ui, &mut p.base, advanced);
            changed |= slider_f32(ui, "Warp Strength", &mut p.warp_strength, 0.0, 300.0);
            if advanced {
                changed |= slider_f32(ui, "Warp Freq", &mut p.warp_frequency, 0.0001, 0.02);
            }
        }
        LayerKind::Terrace(p) => {
            let mut levels = p.levels as i32;
            if slider_i32(ui, "Levels", &mut levels, 2, 32) {
                p.levels = levels as u32;
                changed = true;
            }
            changed |= slider_f32(ui, "Sharpness", &mut p.sharpness, 0.0, 1.0);
        }
        LayerKind::Plateau(p) => {
            changed |= slider_f32(ui, "Low", &mut p.low, -100.0, 500.0);
            changed |= slider_f32(ui, "High", &mut p.high, 0.0, 1000.0);
            if advanced {
                changed |= slider_f32(ui, "Soft", &mut p.soft, 0.1, 50.0);
            }
        }
        LayerKind::Mountains(p) => {
            if advanced {
                changed |= edit_noise(ui, &mut p.base, true);
                changed |= slider_f32(ui, "Sharpness", &mut p.ridge_sharpness, 0.5, 4.0);
                changed |= slider_f32(ui, "Range Width", &mut p.range_width, 0.05, 0.8);
            } else {
                // Artist controls map directly to the procedural parameters.
                let mut seed = p.base.seed.min(99999) as i32;
                if slider_i32(ui, "Seed", &mut seed, 0, 99999) {
                    p.base.seed = seed as u64;
                    changed = true;
                }
                changed |= slider_f32(ui, "Peak Height", &mut p.base.amplitude, 0.0, 1000.0);
                let mut octaves = p.base.octaves as i32;
                if slider_i32(ui, "Fine Detail", &mut octaves, 1, 12) {
                    p.base.octaves = octaves as u32;
                    changed = true;
                }
                changed |= slider_f32(ui, "Ridge Sharpness", &mut p.ridge_sharpness, 0.5, 4.0);
            }
        }
        LayerKind::Dunes(p) => {
            if advanced {
                changed |= edit_noise(ui, &mut p.base, true);
                changed |= slider_f32(ui, "Dune Spacing", &mut p.wave_frequency, 0.001, 0.05);
            } else {
                changed |= edit_noise(ui, &mut p.base, false);
                changed |= slider_f32(ui, "Dune Spacing", &mut p.wave_frequency, 0.001, 0.05);
            }
        }
        LayerKind::Canyons(p) => {
            changed |= slider_f32(ui, "Depth", &mut p.depth, 10.0, 500.0);
            changed |= slider_f32(ui, "Width", &mut p.width, 10.0, 400.0);
            if advanced {
                changed |= slider_f32(ui, "Meander", &mut p.meander, 0.0, 1.0);
            }
        }
        LayerKind::VoronoiRegions(p) => {
            changed |= edit_noise(ui, &mut p.base, advanced);
            if advanced {
                changed |= slider_f32(ui, "Jitter", &mut p.cell_jitter, 0.0, 1.0);
            }
        }
        LayerKind::ImportHeightmap(p) => {
            label(ui, "Path:");
            label(
                ui,
                if p.path.is_empty() {
                    "(empty)"
                } else {
                    &p.path
                },
            );
            changed |= slider_f32(ui, "Scale", &mut p.height_scale, 1.0, 2000.0);
            if advanced {
                changed |= slider_f32(ui, "Offset", &mut p.height_offset, -500.0, 500.0);
            }
        }
        LayerKind::ThermalErosion(p) => {
            changed |= slider_f32(ui, "Strength", &mut p.strength, 0.0, 1.0);
            changed |= slider_f32(ui, "Talus deg", &mut p.talus_angle_deg, 5.0, 60.0);
            if advanced {
                let mut iters = p.iterations as i32;
                if slider_i32(ui, "Iterations", &mut iters, 1, 200) {
                    p.iterations = iters as u32;
                    changed = true;
                }
            }
        }
        LayerKind::HydraulicErosion(p) => {
            if advanced {
                let mut iters = p.iterations as i32;
                if slider_i32(ui, "Iterations", &mut iters, 1, 200) {
                    p.iterations = iters as u32;
                    changed = true;
                }
                changed |= slider_f32(ui, "Rain", &mut p.rainfall, 0.0, 0.2);
                changed |= slider_f32(ui, "Evap", &mut p.evaporation, 0.0, 0.2);
                changed |= slider_f32(ui, "Capacity", &mut p.capacity, 0.0, 1.0);
                changed |= slider_f32(ui, "Erosion", &mut p.erosion, 0.0, 1.0);
                changed |= slider_f32(ui, "Deposit", &mut p.deposition, 0.0, 1.0);
                changed |= slider_f32(ui, "Timestep", &mut p.timestep, 0.01, 2.0);
            } else {
                // Strength, Wetness, Iterations
                changed |= slider_f32(ui, "Weathering Strength", &mut p.erosion, 0.0, 1.0);
                changed |= slider_f32(ui, "Rainfall", &mut p.rainfall, 0.0, 0.2);
                let mut iters = p.iterations as i32;
                if slider_i32(ui, "Iterations", &mut iters, 1, 200) {
                    p.iterations = iters as u32;
                    changed = true;
                }
            }
        }
        LayerKind::RiverCarve(p) => {
            changed |= slider_f32(ui, "Depth", &mut p.depth, 1.0, 100.0);
            changed |= slider_f32(ui, "Width", &mut p.width, 1.0, 20.0);
            if advanced {
                changed |= slider_f32(ui, "Threshold", &mut p.accumulation_threshold, 1.0, 500.0);
                changed |= slider_f32(ui, "Bank Smooth", &mut p.bank_smooth, 0.0, 10.0);
                changed |= checkbox(ui, "D-inf routing", &mut p.use_dinfinity);
            }
        }
        LayerKind::Blur(p) => {
            let mut radius = p.radius as i32;
            if slider_i32(ui, "Radius", &mut radius, 1, 8) {
                p.radius = radius as u32;
                changed = true;
            }
            if advanced {
                let mut iters = p.iterations as i32;
                if slider_i32_id(ui, Id::new("blur_iters"), "Iterations", &mut iters, 1, 8) {
                    p.iterations = iters as u32;
                    changed = true;
                }
            }
        }
        LayerKind::Coastal(p) => {
            changed |= slider_f32(ui, "Sea Level", &mut p.sea_level, -50.0, 100.0);
            changed |= slider_f32(ui, "Beach", &mut p.beach_width, 1.0, 100.0);
            if advanced {
                changed |= checkbox(ui, "Flatten below sea", &mut p.flatten_below);
                changed |= slider_f32(ui, "Shelf Depth", &mut p.shelf_depth, 0.0, 200.0);
            }
        }
        LayerKind::Vegetation(p) => {
            let mut seed = p.seed.min(99999) as i32;
            if slider_i32(ui, "Seed", &mut seed, 0, 99999) {
                p.seed = seed as u64;
                changed = true;
            }
            changed |= slider_f32(ui, "Density", &mut p.density, 0.0, 1.0);
            if advanced {
                changed |= slider_f32(ui, "Min Dist", &mut p.min_distance, 1.0, 20.0);
                changed |= slider_f32(ui, "Min Slope", &mut p.min_slope_deg, 0.0, 90.0);
                changed |= slider_f32(ui, "Max Slope", &mut p.max_slope_deg, 0.0, 90.0);
            }
        }
        LayerKind::Materials(p) => {
            label(ui, &format!("{} material rules", p.rules.len()));
            if !advanced {
                label(ui, "Enable Advanced to edit rule slopes.");
            }
            if advanced {
                for (i, rule) in p.rules.iter_mut().enumerate() {
                    label(ui, &rule.name);
                    let mut rid = rule.id as i32;
                    if slider_i32_id(ui, Id::new("mat_id").with(i as u64), "ID", &mut rid, 0, 64) {
                        rule.id = rid as u32;
                        changed = true;
                    }
                    changed |= slider_f32_id(
                        ui,
                        Id::new("mat_min_s").with(i as u64),
                        "Min Slope",
                        &mut rule.min_slope_deg,
                        0.0,
                        90.0,
                    );
                    changed |= slider_f32_id(
                        ui,
                        Id::new("mat_max_s").with(i as u64),
                        "Max Slope",
                        &mut rule.max_slope_deg,
                        0.0,
                        90.0,
                    );
                }
                if button_id(ui, Id::new("add_mat_rule"), "Add Material Rule") {
                    let id = p.rules.iter().map(|rule| rule.id).max().unwrap_or(0) + 1;
                    p.rules.push(MaterialRule {
                        name: format!("Material {id}"),
                        id,
                        min_slope_deg: 0.0,
                        max_slope_deg: 90.0,
                        min_height: f32::NEG_INFINITY,
                        max_height: f32::INFINITY,
                        mask: terra_core::mask::MaskSource::None,
                    });
                    changed = true;
                }
            }
        }
        LayerKind::Biomes(_) => {
            label(ui, "Edit biomes via presets for now.");
        }
    }
    changed
}

fn edit_noise(ui: &mut GuiContext<'_>, p: &mut NoiseParams, advanced: bool) -> bool {
    let mut changed = false;
    if advanced {
        section_header(ui, "TRANSFORM");
        let mut seed = p.seed.min(99999) as i32;
        if slider_i32(ui, "Seed", &mut seed, 0, 99999) {
            p.seed = seed as u64;
            changed = true;
        }
        changed |= slider_f32(ui, "Offset X", &mut p.offset_x, -10000.0, 10000.0);
        changed |= slider_f32(ui, "Offset Z", &mut p.offset_z, -10000.0, 10000.0);

        section_header(ui, "SHAPE");
        changed |= slider_f32(ui, "Terrain Scale", &mut p.frequency, 0.0001, 0.05);
        changed |= slider_f32(ui, "Height Range", &mut p.amplitude, 0.0, 1000.0);

        section_header(ui, "FRACTAL");
        let mut octaves = p.octaves as i32;
        if slider_i32(ui, "Fine Detail", &mut octaves, 1, 12) {
            p.octaves = octaves as u32;
            changed = true;
        }
        changed |= slider_f32(ui, "Lacunarity", &mut p.lacunarity, 1.1, 4.0);
        changed |= slider_f32(ui, "Persistence", &mut p.persistence, 0.1, 0.95);

        section_header(ui, "OUTPUT RANGE");
        changed |= slider_f32(ui, "In Min", &mut p.remap_min, -2.0, 0.0);
        changed |= slider_f32(ui, "In Max", &mut p.remap_max, 0.0, 2.0);
    } else {
        let mut seed = p.seed.min(99999) as i32;
        if slider_i32(ui, "Seed", &mut seed, 0, 99999) {
            p.seed = seed as u64;
            changed = true;
        }
        changed |= slider_f32(ui, "Terrain Scale", &mut p.frequency, 0.0001, 0.05);
        changed |= slider_f32(ui, "Height Range", &mut p.amplitude, 0.0, 1000.0);
        let mut octaves = p.octaves as i32;
        if slider_i32(ui, "Fine Detail", &mut octaves, 1, 12) {
            p.octaves = octaves as u32;
            changed = true;
        }
    }
    changed
}

fn kind_display_name(kind: &LayerKind) -> &'static str {
    use LayerKind::*;
    match kind {
        SculptBase(_) => "Base",
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
        ThermalErosion(_) => "Thermal Erosion",
        HydraulicErosion(_) => "Hydraulic Erosion",
        RiverCarve(_) => "Rivers",
        Blur(_) => "Blur",
        Coastal(_) => "Coastal",
        Materials(_) => "Materials",
        Biomes(_) => "Biomes",
        Vegetation(_) => "Vegetation",
    }
}
