//! Floating editor windows (mask / content / export / preview / profiler).

use crate::presets::builtin_presets;
use crate::{FrameUiOutput, Preview2dMode, UiState};
use terra_core::document::TerrainDocument;
use terra_core::layer::{Layer, LayerId, LayerKind};
use terra_core::mask::{MaskAsset, MaskId, MaskOp, MaskSource, PaintBuffer};
use terra_gui::style::{self, PAD};
use terra_gui::{
    button, button_id, checkbox, combo, label, selectable, slider_f32, slider_f32_id, slider_i32,
    GuiContext, Id, Rect,
};

#[derive(Debug)]
pub enum PanelAction {
    AddLayer(Layer),
    RemoveSelected,
    DuplicateSelected,
    Reorder {
        from: usize,
        to: usize,
    },
    Select(LayerId),
    SetEnabled {
        id: LayerId,
        enabled: bool,
    },
    SetOpacity {
        id: LayerId,
        opacity: f32,
    },
    SetBlend {
        id: LayerId,
        blend: terra_core::layer::BlendMode,
    },
    SetKind {
        id: LayerId,
        kind: LayerKind,
    },
    Rename {
        id: LayerId,
        name: String,
    },
    ApplyPreset(String),
    AddMask(MaskAsset),
    UpdateMaskAsset(MaskAsset),
    BindMaskToLayer {
        layer: LayerId,
        mask: MaskId,
    },
    UnbindMask {
        layer: LayerId,
        mask: MaskId,
    },
    UpdateMaskBinding {
        layer: LayerId,
        mask: MaskId,
        strength: f32,
        invert: bool,
    },
    PaintMaskStamp {
        mask_id: MaskId,
        u: f32,
        v: f32,
        radius: f32,
        strength: f32,
        erase: bool,
    },
    /// Stamp onto the sculptable Base height buffer.
    PaintSculptStamp {
        layer: LayerId,
        u: f32,
        v: f32,
        radius: f32,
        strength: f32,
        /// 0 = raise, 1 = lower, 2 = smooth
        mode: u8,
    },
    ResetSculptBase {
        id: LayerId,
    },
    MarkDirty(Option<LayerId>),
    SetLocked {
        id: LayerId,
        locked: bool,
    },
    SetSolo {
        id: LayerId,
        solo: bool,
    },
    SetColorTag {
        id: LayerId,
        tag: u8,
    },
    SetCached {
        id: LayerId,
        cached: bool,
    },
    AddGroup {
        name: String,
    },
    /// Request opening Quick Add popup.
    OpenQuickAdd,
    /// Randomize seed on selected procedural layer.
    RandomizeSeed {
        id: LayerId,
    },
}

#[derive(Debug, Default)]
pub struct WindowsGuiState {
    pub mask_scroll: f32,
    pub content_scroll: f32,
    pub export_scroll: f32,
    pub preview_scroll: f32,
    pub profiler_scroll: f32,
    pub pipeline_scroll: f32,
    pub history_scroll: f32,
    pub bookmarks_scroll: f32,
}

pub fn draw_windows(
    ui: &mut GuiContext<'_>,
    doc: &mut TerrainDocument,
    ui_state: &mut UiState,
    win: &mut WindowsGuiState,
    out: &mut FrameUiOutput,
) {
    if ui_state.show_mask_editor {
        let rect = Rect::from_pos_size(300.0, 56.0, 360.0, 420.0);
        if ui.begin_window(
            Id::new("win_mask"),
            "Mask Editor",
            rect,
            &mut ui_state.show_mask_editor,
            &mut win.mask_scroll,
        ) {
            mask_editor(ui, doc, ui_state, &mut out.actions);
            ui.end_window(&mut win.mask_scroll);
        }
    }

    if ui_state.show_content_browser {
        let rect = Rect::from_pos_size(300.0, 72.0, 360.0, 320.0);
        if ui.begin_window(
            Id::new("win_content"),
            "Content Browser",
            rect,
            &mut ui_state.show_content_browser,
            &mut win.content_scroll,
        ) {
            content_browser(ui, &mut out.actions);
            ui.end_window(&mut win.content_scroll);
        }
    }

    if ui_state.show_export {
        let rect = Rect::from_pos_size(380.0, 72.0, 380.0, 420.0);
        if ui.begin_window(
            Id::new("win_export"),
            "Export",
            rect,
            &mut ui_state.show_export,
            &mut win.export_scroll,
        ) {
            export_panel(ui, doc, ui_state, out);
            ui.end_window(&mut win.export_scroll);
        }
    }

    if ui_state.show_2d_preview {
        let rect = Rect::from_pos_size(320.0, 120.0, 420.0, 400.0);
        if ui.begin_window(
            Id::new("win_preview"),
            "2D Preview",
            rect,
            &mut ui_state.show_2d_preview,
            &mut win.preview_scroll,
        ) {
            preview_panel(ui, ui_state);
            ui.end_window(&mut win.preview_scroll);
        }
    }

    if ui_state.show_profiler {
        let x = (ui.viewport_rect().max_x - 296.0).max(ui.viewport_rect().min_x + 16.0);
        let rect = Rect::from_pos_size(x, 56.0, 280.0, 280.0);
        if ui.begin_window(
            Id::new("win_profiler"),
            "Profiler",
            rect,
            &mut ui_state.show_profiler,
            &mut win.profiler_scroll,
        ) {
            profiler_panel(ui, ui_state);
            ui.end_window(&mut win.profiler_scroll);
        }
    }
}

fn mask_editor(
    ui: &mut GuiContext<'_>,
    doc: &TerrainDocument,
    ui_state: &mut UiState,
    actions: &mut Vec<PanelAction>,
) {
    label(ui, "Per-layer masks and project mask assets.");
    if button(ui, "Add Mask") {
        let id = MaskId::new();
        actions.push(PanelAction::AddMask(MaskAsset {
            id,
            name: "Mask".into(),
            source: MaskSource::Constant(1.0),
            ops: Vec::new(),
            paint: None,
        }));
        ui_state.selected_mask = Some(id);
    }
    ui.separator();

    for (i, asset) in doc.masks.iter().enumerate() {
        let selected = ui_state.selected_mask == Some(asset.id);
        if selectable(ui, &asset.name, selected) {
            ui_state.selected_mask = Some(asset.id);
        }
        let _ = i;
    }

    let selected = ui_state
        .selected_mask
        .or_else(|| doc.masks.first().map(|asset| asset.id));
    if let Some(mask_id) = selected {
        ui_state.selected_mask = Some(mask_id);
        if let Some(asset) = doc.masks.iter().find(|asset| asset.id == mask_id) {
            let mut updated = asset.clone();
            let mut changed = false;
            ui.separator();
            label(ui, &format!("Name: {}", updated.name));
            label(ui, "(rename later)");

            let kind: usize = match updated.source {
                MaskSource::Constant(_) => 0,
                MaskSource::Height { .. } => 1,
                MaskSource::Slope { .. } => 2,
                MaskSource::Painted { .. } => 3,
                _ => 0,
            };
            let mut new_kind = kind;
            let kinds = ["Constant", "Height", "Slope", "Painted"];
            let _ = combo(ui, "Source", &mut new_kind, &kinds);
            if new_kind != kind {
                updated.source = match new_kind {
                    1 => MaskSource::Height {
                        min: 0.0,
                        max: 200.0,
                    },
                    2 => MaskSource::Slope {
                        min_deg: 20.0,
                        max_deg: 60.0,
                    },
                    3 => {
                        updated
                            .paint
                            .get_or_insert_with(|| PaintBuffer::new(512, 512));
                        MaskSource::Painted { mask_id }
                    }
                    _ => MaskSource::Constant(1.0),
                };
                changed = true;
            }

            match &mut updated.source {
                MaskSource::Constant(value) => {
                    changed |= slider_f32(ui, "Value", value, 0.0, 1.0);
                }
                MaskSource::Height { min, max } => {
                    changed |= slider_f32(ui, "Min", min, -500.0, 2000.0);
                    changed |= slider_f32(ui, "Max", max, -500.0, 2000.0);
                }
                MaskSource::Slope { min_deg, max_deg } => {
                    changed |= slider_f32(ui, "Min deg", min_deg, 0.0, 90.0);
                    changed |= slider_f32(ui, "Max deg", max_deg, 0.0, 90.0);
                }
                MaskSource::Painted { .. } => {
                    let painting = ui_state.paint_mask == Some(mask_id);
                    if button(
                        ui,
                        if painting {
                            "Stop Painting"
                        } else {
                            "Paint in Viewport"
                        },
                    ) {
                        ui_state.paint_mask = if painting { None } else { Some(mask_id) };
                    }
                    label(ui, "Left-drag inside the viewport to paint.");
                }
                _ => {}
            }

            if button_id(ui, Id::new("mask_add_inv"), "Add Invert") {
                updated.ops.push(MaskOp::Invert);
                changed = true;
            }
            if button_id(ui, Id::new("mask_add_clamp"), "Add Clamp") {
                updated.ops.push(MaskOp::Clamp { min: 0.0, max: 1.0 });
                changed = true;
            }
            if changed {
                actions.push(PanelAction::UpdateMaskAsset(updated));
            }
        }
    }

    if let Some(layer_id) = doc.selected {
        if let Some(layer) = doc.stack.find(layer_id) {
            ui.separator();
            label(ui, &format!("Masks on '{}'", layer.common.name));
            if let Some(mask_id) = ui_state.selected_mask {
                if layer
                    .common
                    .masks
                    .iter()
                    .any(|binding| binding.id == mask_id)
                {
                    if button(ui, "Unbind selected mask") {
                        actions.push(PanelAction::UnbindMask {
                            layer: layer_id,
                            mask: mask_id,
                        });
                    }
                } else if button(ui, "Bind selected mask") {
                    actions.push(PanelAction::BindMaskToLayer {
                        layer: layer_id,
                        mask: mask_id,
                    });
                }
            }
            for (i, binding) in layer.common.masks.iter().enumerate() {
                let mut strength = binding.strength;
                let mut invert = binding.invert;
                label(ui, &format!("Binding {}", i + 1));
                let strength_changed = slider_f32_id(
                    ui,
                    Id::new("mask_str").with(i as u64),
                    "Strength",
                    &mut strength,
                    0.0,
                    1.0,
                );
                let invert_changed = checkbox(ui, "Invert", &mut invert);
                if strength_changed || invert_changed {
                    actions.push(PanelAction::UpdateMaskBinding {
                        layer: layer_id,
                        mask: binding.id,
                        strength,
                        invert,
                    });
                }
            }
        }
    }
}

fn content_browser(ui: &mut GuiContext<'_>, actions: &mut Vec<PanelAction>) {
    label(ui, "Presets");
    ui.separator();
    for (i, preset) in builtin_presets().into_iter().enumerate() {
        if button_id(ui, Id::new("preset").with(i as u64), &preset.name) {
            actions.push(PanelAction::ApplyPreset(preset.name.clone()));
        }
        label(ui, &preset.description);
        ui.gap(style::GAP);
    }
}

fn export_panel(
    ui: &mut GuiContext<'_>,
    doc: &mut TerrainDocument,
    ui_state: &mut UiState,
    out: &mut FrameUiOutput,
) {
    label(ui, "What you'll get");
    label(ui, "- height.png - 16-bit grayscale heightmap");
    label(ui, "- height.r32 - raw float heights");
    label(ui, "- height_meta.json - size / world extents");
    label(ui, "- mask_*.png - baked aux masks (8-bit)");
    ui.separator();

    label(ui, "Resolution");
    let mut export_res = doc.export_resolution as i32;
    if slider_i32(ui, "Export res", &mut export_res, 512, 8192) {
        doc.export_resolution = export_res as u32;
    }
    let mut preview_res = doc.preview_resolution as i32;
    if slider_i32(ui, "Preview res", &mut preview_res, 256, 2048) {
        doc.preview_resolution = preview_res as u32;
    }
    label(
        ui,
        &format!(
            "Export uses {}x{} at export quality (not Draft).",
            doc.export_resolution, doc.export_resolution
        ),
    );
    ui.separator();

    label(ui, "Engine kits (labels for handoff)");
    label(ui, "- Unreal Landscape - import height.png as 16-bit");
    label(ui, "- Unity Terrain - use height.r32 or height.png");
    label(ui, "Meta JSON carries world size for both.");
    ui.separator();

    label(
        ui,
        &format!(
            "Directory: {}",
            ui_state.export_path.as_deref().unwrap_or("Not selected")
        ),
    );
    if button(ui, "Choose Export Directory...") {
        out.request_export_path = true;
    }
    if button_id(ui, Id::new("export_start"), "Start Export") {
        out.request_start_export = true;
    }

    if let Some(progress) = ui_state.export_progress {
        label(ui, &format!("Exporting... {:.0}%", progress * 100.0));
    } else if ui_state.export_path.is_some() {
        label(ui, "Ready - Start Export writes files in the background.");
    } else {
        label(ui, "Pick a directory, then Start Export.");
    }
}

fn preview_panel(ui: &mut GuiContext<'_>, ui_state: &mut UiState) {
    let modes = [
        (Preview2dMode::Height, "Height"),
        (Preview2dMode::Slope, "Slope"),
        (Preview2dMode::Flow, "Flow"),
        (Preview2dMode::Masks, "Mask"),
    ];
    let row = ui.allocate(terra_gui::style::ROW_H);
    let cell_w = row.width() / modes.len() as f32;
    for (i, (mode, name)) in modes.iter().enumerate() {
        let cell = Rect::from_pos_size(
            row.min_x + cell_w * i as f32,
            row.min_y,
            cell_w - 2.0,
            row.height(),
        );
        let selected = ui_state.preview_mode == *mode;
        let id = Id::new("preview_mode").with(i as u64);
        let hovered = ui.pointer_in(cell);
        if hovered {
            ui.state.set_hot(id);
        }
        if hovered && ui.input.primary_pressed {
            ui.state.active = Some(id);
        }
        if ui.input.primary_released && ui.state.is_active(id) && hovered {
            ui_state.preview_mode = *mode;
        }
        ui.panel(
            cell,
            if selected {
                style::SELECTED_BG
            } else if hovered {
                style::BUTTON_HOVER
            } else {
                style::BUTTON_BG
            },
        );
        ui.label_centered(cell.center_x(), cell.min_y + 4.0, name, style::TEXT, 1.0);
    }

    ui.gap(PAD);
    if let Some((width, height, rgba)) = &ui_state.preview_rgba {
        let avail = ui.allocate(220.0);
        let aspect = *width as f32 / (*height as f32).max(1.0);
        let (w, h) = if avail.width() / avail.height() > aspect {
            (avail.height() * aspect, avail.height())
        } else {
            (avail.width(), avail.width() / aspect)
        };
        let img = Rect::from_pos_size(avail.min_x, avail.min_y, w, h);
        ui.image(img, *width, *height, rgba);
    } else {
        label(ui, "Waiting for a completed terrain evaluation.");
    }
}

fn profiler_panel(ui: &mut GuiContext<'_>, ui_state: &UiState) {
    let p = &ui_state.profile;
    label(ui, &format!("Generation ID: {}", p.gen_id));
    label(
        ui,
        &format!("Quality: {}  |  Tex {}x{}", p.quality, p.tex_w, p.tex_h),
    );
    label(
        ui,
        &format!(
            "Eval path: {}",
            if p.path.is_empty() { "-" } else { p.path }
        ),
    );
    label(
        ui,
        &format!(
            "Clipmap levels: {}  |  Tiles {}x{}",
            p.clipmap_levels, p.tiles_x, p.tiles_z
        ),
    );
    ui.separator();
    label(ui, &format!("Layer eval:  {:>6} us", p.eval_us));
    label(ui, &format!("GPU upload:  {:>6} us", p.upload_us));
    label(ui, &format!("Terrain draw:{:>6} us", p.render_us));
    label(ui, &format!("UI:          {:>6} us", p.ui_us));
    label(ui, &format!("Frame total: {:>6} us", p.frame_us));
    ui.separator();
    label(ui, "Viewport never waits on eval;");
    label(ui, "last-good textures stay on screen.");
}
