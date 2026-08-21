use crate::ui::{MaskEditAction, PanelAction};

use super::super::TerraApp;
use super::ApplyCtx;

// Err intentionally carries the unhandled action back to the router.
#[allow(clippy::result_large_err)]
pub(crate) fn try_apply(
    app: &mut TerraApp,
    action: PanelAction,
    ctx: &mut ApplyCtx,
) -> Result<(), PanelAction> {
    match action {
        PanelAction::AddMask(mut asset) => {
            asset.prepare_for_document();
            let id = asset.id;
            let painted = asset.is_painted();
            app.session.document.masks.push(asset);
            app.ui_state.selected_mask = Some(id);
            if painted {
                app.ui_state.paint_mask = Some(id);
            } else {
                app.ui_state.paint_mask = None;
            }
            app.ui_state.focus_created_mask(painted);
            ctx.note_mask_mutation(id);
            ctx.doc_mutated = true;
            app.preview_dirty = true;
            app.mask_overlay_dirty = true;
        }
        PanelAction::PaintLayerMask { id } => {
            if let Some(mask_id) = app.session.document.ensure_layer_paint_mask(id) {
                app.session.document.selected = Some(id);
                app.ui_state.selected_mask = Some(mask_id);
                app.ui_state.paint_mask = Some(mask_id);
                app.ui_state.focus_created_mask(true);
                ctx.note_mask_mutation(mask_id);
                ctx.doc_mutated = true;
                app.preview_dirty = true;
                app.mask_overlay_dirty = true;
            }
        }
        PanelAction::SelectMask(mask_id) => {
            app.ui_state.selected_mask = Some(mask_id);
            let painted = app
                .session
                .document
                .masks
                .iter()
                .find(|a| a.id == mask_id)
                .is_some_and(|a| a.is_painted());
            if painted {
                app.ui_state.paint_mask = Some(mask_id);
                app.ui_state.focus_created_mask(true);
            } else {
                app.ui_state.paint_mask = None;
                if app.ui_state.editor_tool == crate::ui::EditorTool::PaintMask {
                    app.ui_state.set_editor_tool(crate::ui::EditorTool::Move);
                }
            }
            app.preview_dirty = true;
            app.mask_overlay_dirty = true;
        }
        PanelAction::UpdateMaskAsset(asset) => {
            if let Some(existing) = app
                .session
                .document
                .masks
                .iter_mut()
                .find(|existing| existing.id == asset.id)
            {
                let color_only = existing.display_color != asset.display_color
                    && existing.name == asset.name
                    && existing.ops.len() == asset.ops.len()
                    && existing
                        .paint
                        .as_ref()
                        .map(|p| (p.width, p.height, p.samples().len()))
                        == asset
                            .paint
                            .as_ref()
                            .map(|p| (p.width, p.height, p.samples().len()))
                    && existing
                        .paint
                        .as_ref()
                        .zip(asset.paint.as_ref())
                        .map(|(a, b)| a.samples() == b.samples())
                        .unwrap_or(true);
                let mutated_id = existing.id;
                *existing = asset;
                if color_only {
                    // Display colour is visual-only - refresh overlay, skip rebuild.
                    app.mask_overlay_dirty = true;
                } else {
                    ctx.note_mask_mutation(mutated_id);
                    app.preview_dirty = true;
                    app.mask_overlay_dirty = true;
                }
                ctx.doc_mutated = true;
            }
        }
        PanelAction::BindMaskToLayer { layer, mask } => {
            if let Some(target) = app.session.document.stack.find_mut(layer) {
                if !target
                    .common
                    .masks
                    .iter()
                    .any(|binding| binding.mask.id == mask)
                {
                    target
                        .common
                        .masks
                        .push(terra_core::mask::MaskRef::new(mask));
                    ctx.dirty_from = Some(layer);
                }
            } else if let Some(group) = app.session.document.stack.find_group_mut(layer) {
                if !group.masks.iter().any(|binding| binding.mask.id == mask) {
                    group.masks.push(terra_core::mask::MaskRef::new(mask));
                    app.mark_all_layers_dirty();
                    app.request_rebuild();
                    ctx.doc_mutated = true;
                }
            }
        }
        PanelAction::UnbindMask { layer, mask } => {
            if let Some(target) = app.session.document.stack.find_mut(layer) {
                target
                    .common
                    .masks
                    .retain(|binding| binding.mask.id != mask);
                ctx.dirty_from = Some(layer);
            } else if let Some(group) = app.session.document.stack.find_group_mut(layer) {
                group.masks.retain(|binding| binding.mask.id != mask);
                app.mark_all_layers_dirty();
                app.request_rebuild();
                ctx.doc_mutated = true;
            }
        }
        PanelAction::UpdateMaskBinding {
            layer,
            mask,
            strength,
            invert,
        } => {
            if let Some(binding) = app
                .session
                .document
                .stack
                .find_mut(layer)
                .and_then(|target| {
                    target
                        .common
                        .masks
                        .iter_mut()
                        .find(|binding| binding.mask.id == mask)
                })
            {
                binding.mask.strength = strength;
                binding.mask.invert = invert;
                ctx.dirty_from = Some(layer);
            } else if let Some(binding) = app
                .session
                .document
                .stack
                .find_group_mut(layer)
                .and_then(|group| group.masks.iter_mut().find(|entry| entry.mask.id == mask))
            {
                binding.mask.strength = strength;
                binding.mask.invert = invert;
                app.mark_all_layers_dirty();
                app.request_rebuild();
                ctx.doc_mutated = true;
            }
        }
        PanelAction::CycleMaskCombine { layer, mask } => {
            use terra_core::mask::MaskCombine;
            let cycle = |c: MaskCombine| c.cycle();
            let mut cycled = false;
            if let Some(binding) = app
                .session
                .document
                .stack
                .find_mut(layer)
                .and_then(|target| {
                    target
                        .common
                        .masks
                        .iter_mut()
                        .find(|binding| binding.mask.id == mask)
                })
            {
                binding.combine = cycle(binding.combine);
                ctx.dirty_from = Some(layer);
                cycled = true;
            } else if let Some(group) = app.session.document.stack.find_group_mut(layer) {
                if let Some(binding) = group.masks.iter_mut().find(|b| b.mask.id == mask) {
                    binding.combine = cycle(binding.combine);
                    cycled = true;
                }
            }
            if cycled && ctx.dirty_from.is_none() {
                app.mark_all_layers_dirty();
                app.request_rebuild();
                ctx.doc_mutated = true;
            }
        }
        PanelAction::PaintMaskStamp {
            mask_id,
            u,
            v,
            radius,
            strength,
            hardness,
            tool,
        } => {
            // Snapshot paint buffer once per stroke for undo.
            if app.mask_paint_stroke_before.is_none() {
                if let Some(asset) = app.session.document.masks.iter().find(|a| a.id == mask_id) {
                    if let Some(paint) = asset.paint.as_ref() {
                        app.mask_paint_stroke_before =
                            Some((mask_id, paint.samples().to_vec(), paint.width, paint.height));
                    } else {
                        app.mask_paint_stroke_before = Some((mask_id, Vec::new(), 512, 512));
                    }
                }
            }
            if let Some(asset) = app
                .session
                .document
                .masks
                .iter_mut()
                .find(|asset| asset.id == mask_id)
            {
                let paint = asset
                    .paint
                    .get_or_insert_with(|| terra_core::mask::PaintBuffer::new(512, 512));
                paint.edit_circle(u, v, radius, strength, hardness, tool);
                // Critical: update paint buffer + overlay only - no full terrain
                // rebuild per brush dab (was the main mask-paint lag cause).
                app.mask_overlay_dirty = true;
                // Defer document-dirty marking to stroke commit.
            }
        }
        PanelAction::EditMask { mask_id, action } => {
            if let Some(asset) = app
                .session
                .document
                .masks
                .iter_mut()
                .find(|asset| asset.id == mask_id)
            {
                let paint = asset
                    .paint
                    .get_or_insert_with(|| terra_core::mask::PaintBuffer::new(512, 512));
                let (before, w, h) = (paint.samples().to_vec(), paint.width, paint.height);
                match action {
                    MaskEditAction::Clear => paint.clear(),
                    MaskEditAction::Fill => paint.fill(),
                    MaskEditAction::FlipX => paint.flip_x(),
                    MaskEditAction::FlipY => paint.flip_y(),
                    MaskEditAction::RotateLeft => paint.rotate_left(),
                    MaskEditAction::RotateRight => paint.rotate_right(),
                }
                if paint.width == w && paint.height == h {
                    if let Some(patch) = terra_core::document::MaskPaintPatch::from_diff(
                        "Edited Mask",
                        mask_id,
                        w,
                        h,
                        &before,
                        paint.samples(),
                    ) {
                        app.session.push_mask_paint_patch(patch);
                    }
                }
                ctx.note_mask_mutation(mask_id);
                ctx.doc_mutated = true;
                app.preview_dirty = true;
                app.mask_overlay_dirty = true;
            }
        }
        PanelAction::PaintSculptStamp {
            layer,
            u,
            v,
            radius,
            strength,
            stroke_kind,
            target_height,
        } => {
            if let Some(target) = app.session.document.stack.find_mut(layer) {
                let continuing = app.last_paint_uv.is_some();
                let world_radius = radius
                    * 0.5
                    * (app.session.document.metrics.world_size_x
                        + app.session.document.metrics.world_size_z);
                if let terra_core::layer::LayerKind::SculptStrokes(params) = &mut target.kind {
                    terra_core::shape_history::stamp_stroke(
                        params,
                        stroke_kind,
                        u,
                        v,
                        world_radius.max(1.0),
                        strength,
                        target_height,
                        continuing,
                    );
                    ctx.dirty_from = Some(layer);
                    app.session.document.selected = Some(layer);
                    app.ui_state.shape_session_layer = Some(layer);
                } else if let terra_core::layer::LayerKind::TerrainConstraints(params) =
                    &mut target.kind
                {
                    use terra_core::layer::{
                        SculptPoint, TerrainConstraint, TerrainConstraintKind,
                    };
                    let constraint_kind = match stroke_kind {
                        terra_core::authoring::SculptStrokeKind::Ridge
                        | terra_core::authoring::SculptStrokeKind::MountainStamp => {
                            TerrainConstraintKind::Ridge
                        }
                        terra_core::authoring::SculptStrokeKind::Valley
                        | terra_core::authoring::SculptStrokeKind::ValleyStamp => {
                            TerrainConstraintKind::Valley
                        }
                        terra_core::authoring::SculptStrokeKind::RiverPath => {
                            TerrainConstraintKind::River
                        }
                        terra_core::authoring::SculptStrokeKind::Coastline => {
                            TerrainConstraintKind::Coastline
                        }
                        terra_core::authoring::SculptStrokeKind::Protect => {
                            TerrainConstraintKind::Protect
                        }
                        terra_core::authoring::SculptStrokeKind::PlateauStamp => {
                            TerrainConstraintKind::Plateau
                        }
                        _ => TerrainConstraintKind::Roughness,
                    };
                    let point = SculptPoint {
                        u,
                        v,
                        pressure: 1.0,
                    };
                    let append = continuing
                        && params.constraints.last().is_some_and(|last| {
                            last.kind == constraint_kind
                                && (last.width_m - world_radius).abs()
                                    <= world_radius.max(1.0) * 0.05
                        });
                    if append {
                        params.constraints.last_mut().unwrap().points.push(point);
                    } else {
                        params.constraints.push(TerrainConstraint {
                            kind: constraint_kind,
                            points: vec![point],
                            width_m: world_radius.max(1.0),
                            value: strength,
                            strength: if matches!(constraint_kind, TerrainConstraintKind::Protect) {
                                strength.clamp(0.0, 1.0)
                            } else {
                                1.0
                            },
                        });
                    }
                    ctx.dirty_from = Some(layer);
                    app.session.document.selected = Some(layer);
                } else if let terra_core::layer::LayerKind::SculptBase(params) = &mut target.kind {
                    // Legacy foundation path - prefer Shape history for new strokes.
                    let mode = match stroke_kind {
                        terra_core::authoring::SculptStrokeKind::Lower
                        | terra_core::authoring::SculptStrokeKind::Erode => 1u8,
                        terra_core::authoring::SculptStrokeKind::Smooth
                        | terra_core::authoring::SculptStrokeKind::Pinch => 2u8,
                        _ => 0u8,
                    };
                    params.stamp_circle(u, v, radius, strength, mode);
                    ctx.dirty_from = Some(layer);
                    app.session.document.selected = Some(layer);
                }
            }
            if ctx.dirty_from == Some(layer) {
                let resolution = app
                    .scheduler
                    .quality
                    .resolution(
                        app.session.document.preview_resolution.min(8192),
                        app.session.document.export_resolution,
                    )
                    .max(1);
                let x0 = ((u - radius).clamp(0.0, 1.0) * resolution as f32).floor() as u32;
                let y0 = ((v - radius).clamp(0.0, 1.0) * resolution as f32).floor() as u32;
                let x1 = ((u + radius).clamp(0.0, 1.0) * resolution as f32).ceil() as u32;
                let y1 = ((v + radius).clamp(0.0, 1.0) * resolution as f32).ceil() as u32;
                let next = (
                    x0,
                    y0,
                    x1.saturating_sub(x0).max(1),
                    y1.saturating_sub(y0).max(1),
                );
                ctx.sculpt_dirty_rect = Some(match ctx.sculpt_dirty_rect {
                    Some((ox, oy, ow, oh)) => {
                        let ex = (ox + ow).max(next.0 + next.2);
                        let ey = (oy + oh).max(next.1 + next.3);
                        let nx = ox.min(next.0);
                        let ny = oy.min(next.1);
                        (nx, ny, ex - nx, ey - ny)
                    }
                    None => next,
                });
            }
        }
        PanelAction::AddDistNode { target, kind } => {
            let node = terra_core::mask::DistNode::new(kind);
            if let Some(g) = app.session.document.stack.find_group_mut(target) {
                g.masks.push_node(node);
                TerraApp::mark_biome_placement_custom_for_group(&mut app.session.document, target);
                app.mark_all_layers_dirty();
                app.request_rebuild();
                ctx.doc_mutated = true;
            } else if let Some(l) = app.session.document.stack.find_mut(target) {
                l.common.masks.push_node(node);
                ctx.dirty_from = Some(target);
                ctx.doc_mutated = true;
            }
        }
        PanelAction::AddDistEffect { target, kind } => {
            let mut applied = false;
            if let Some(g) = app.session.document.stack.find_group_mut(target) {
                let effect = terra_core::mask::DistNode::new(kind.clone());
                if let Some(last) = g.masks.nodes.last_mut() {
                    last.children.push(effect);
                } else {
                    g.masks.push_node(effect);
                }
                TerraApp::mark_biome_placement_custom_for_group(&mut app.session.document, target);
                applied = true;
                app.mark_all_layers_dirty();
                app.request_rebuild();
                ctx.doc_mutated = true;
            } else if let Some(l) = app.session.document.stack.find_mut(target) {
                let effect = terra_core::mask::DistNode::new(kind);
                if let Some(last) = l.common.masks.nodes.last_mut() {
                    last.children.push(effect);
                } else {
                    l.common.masks.push_node(effect);
                }
                applied = true;
                ctx.dirty_from = Some(target);
                ctx.doc_mutated = true;
            }
            let _ = applied;
        }
        PanelAction::PaintSelectionStamp {
            u,
            v,
            radius,
            strength,
            hardness,
            tool,
        } => {
            // Session-only quick mask: create the hidden asset lazily on the
            // first stamp. Never marks the document dirty (not serialized).
            let resolution = app.selection_resolution();
            let asset = app.selection.get_or_insert_with(|| {
                let mut asset = terra_core::mask::MaskAsset::new_painted(
                    terra_core::mask::MaskId::new(),
                    "Selection",
                    resolution,
                );
                asset.display_color = TerraApp::SELECTION_TINT;
                asset
            });
            if let Some(paint) = asset.paint.as_mut() {
                paint.edit_circle(u, v, radius, strength, hardness, tool);
            }
            app.ui_state.selection_active = true;
            app.selection_overlay_dirty = true;
        }
        PanelAction::SelectionClear => {
            if app.selection.take().is_some() {
                app.ui_state.status = "Selection cleared".into();
            }
            app.ui_state.selection_active = false;
            app.selection_overlay_dirty = true;
            app.placement_tint_dirty = true;
            app.mask_overlay_dirty = true;
        }
        PanelAction::SelectionAll => {
            let resolution = app.selection_resolution();
            let asset = app.selection.get_or_insert_with(|| {
                let mut asset = terra_core::mask::MaskAsset::new_painted(
                    terra_core::mask::MaskId::new(),
                    "Selection",
                    resolution,
                );
                asset.display_color = TerraApp::SELECTION_TINT;
                asset
            });
            if let Some(paint) = asset.paint.as_mut() {
                paint.fill();
            }
            app.ui_state.selection_active = true;
            app.selection_overlay_dirty = true;
            app.ui_state.status = "Selected all".into();
        }
        PanelAction::SelectionInvert => {
            // Inverting an empty selection selects everything (Photoshop rule).
            let resolution = app.selection_resolution();
            let asset = app.selection.get_or_insert_with(|| {
                let mut asset = terra_core::mask::MaskAsset::new_painted(
                    terra_core::mask::MaskId::new(),
                    "Selection",
                    resolution,
                );
                asset.display_color = TerraApp::SELECTION_TINT;
                asset
            });
            if let Some(paint) = asset.paint.as_mut() {
                paint.invert();
            }
            app.ui_state.selection_active = true;
            app.selection_overlay_dirty = true;
            app.ui_state.status = "Selection inverted".into();
        }
        PanelAction::SelectionSaveAsMask => {
            let Some(selection) = app.selection.as_ref() else {
                app.ui_state.status = "No selection to save".into();
                return Ok(());
            };
            let n = app
                .session
                .document
                .masks
                .iter()
                .filter(|m| m.name.starts_with("Selection Mask"))
                .count()
                + 1;
            let mut asset = selection.clone();
            asset.id = terra_core::mask::MaskId::new();
            asset.name = format!("Selection Mask {n}");
            asset.owner = None;
            asset.source = terra_core::mask::MaskSource::Painted { mask_id: asset.id };
            app.ui_state.status = format!("Saved selection as \"{}\"", asset.name);
            // Route through the AddMask flow so the asset lands in the mask
            // list, gets selected, and arms painting.
            return try_apply(app, PanelAction::AddMask(asset), ctx);
        }
        PanelAction::MaskFromSelection { layer } => {
            if app.selection.as_ref().and_then(|a| a.paint.as_ref()).is_none() {
                app.ui_state.status = "No selection to convert".into();
                return Ok(());
            }
            if let Some(mask_id) = write_selection_into_layer_mask(app, layer, ctx) {
                app.session.document.selected = Some(layer);
                app.ui_state.selected_mask = Some(mask_id);
                app.ui_state.paint_mask = Some(mask_id);
                app.ui_state.focus_created_mask(true);
                app.ui_state.status = "Layer mask created from selection".into();
            }
        }
        PanelAction::SelectionGrow => {
            let mut asset = take_or_new_selection(app);
            if let Some(paint) = asset.paint.as_mut() {
                paint.dilate(SELECTION_MORPH_RADIUS);
            }
            app.selection = Some(asset);
            app.ui_state.selection_active = true;
            app.selection_overlay_dirty = true;
            app.ui_state.status = "Selection grown".into();
        }
        PanelAction::SelectionShrink => {
            let mut asset = take_or_new_selection(app);
            if let Some(paint) = asset.paint.as_mut() {
                paint.erode(SELECTION_MORPH_RADIUS);
            }
            app.selection = Some(asset);
            app.ui_state.selection_active = true;
            app.selection_overlay_dirty = true;
            app.ui_state.status = "Selection shrunk".into();
        }
        PanelAction::SelectionFeather => {
            let mut asset = take_or_new_selection(app);
            if let Some(paint) = asset.paint.as_mut() {
                paint.blur(SELECTION_FEATHER_RADIUS);
            }
            app.selection = Some(asset);
            app.ui_state.selection_active = true;
            app.selection_overlay_dirty = true;
            app.ui_state.status = "Selection feathered".into();
        }
        PanelAction::SelectionFromSlope { min_deg, max_deg } => {
            let Some(hf) = app.last_height.as_ref() else {
                app.ui_state.status = "No terrain yet - nothing to select by slope".into();
                return Ok(());
            };
            // `slope_degrees` stores slope normalized to [0,1] over 0..90 deg.
            let slope = terra_core::analyze::slope_degrees(hf);
            let (sw, sh) = (slope.metrics.width, slope.metrics.height);
            let mut asset = take_or_new_selection(app);
            if let Some(paint) = asset.paint.as_mut() {
                bake_band_max(paint, min_deg, max_deg, |u, v| {
                    let si = ((u * sw as f32) as u32).min(sw.saturating_sub(1));
                    let sj = ((v * sh as f32) as u32).min(sh.saturating_sub(1));
                    slope.get(si, sj) * 90.0
                });
            }
            app.selection = Some(asset);
            app.ui_state.selection_active = true;
            app.selection_overlay_dirty = true;
            app.ui_state.status =
                format!("Added slopes {min_deg:.0}-{max_deg:.0} deg to selection");
        }
        PanelAction::SelectionFromHeight { min_m, max_m } => {
            let Some(hf) = app.last_height.take() else {
                app.ui_state.status = "No terrain yet - nothing to select by height".into();
                return Ok(());
            };
            // Resolve the "auto" preset (non-finite min): above the terrain's
            // median height, from a strided heightfield sample.
            let (lo, hi) = if min_m.is_finite() && max_m.is_finite() {
                (min_m, max_m)
            } else {
                let m = hf.metrics;
                let stride = (m.width.max(m.height) / 128).max(1);
                let mut vals = Vec::new();
                let mut j = 0;
                while j < m.height {
                    let mut i = 0;
                    while i < m.width {
                        vals.push(hf.get(i, j));
                        i += stride;
                    }
                    j += stride;
                }
                vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let median = vals[vals.len() / 2];
                let top = *vals.last().unwrap_or(&median);
                let lo = if min_m.is_finite() { min_m } else { median };
                // Push the upper bound beyond the terrain max so the top soft
                // edge never trims real peaks.
                let hi = if max_m.is_finite() {
                    max_m
                } else {
                    top + (top - lo).max(1.0)
                };
                (lo, hi)
            };
            let (hw, hh) = (hf.metrics.width, hf.metrics.height);
            let mut asset = take_or_new_selection(app);
            if let Some(paint) = asset.paint.as_mut() {
                bake_band_max(paint, lo, hi, |u, v| {
                    let si = ((u * hw as f32) as u32).min(hw.saturating_sub(1));
                    let sj = ((v * hh as f32) as u32).min(hh.saturating_sub(1));
                    hf.get(si, sj)
                });
            }
            app.last_height = Some(hf);
            app.selection = Some(asset);
            app.ui_state.selection_active = true;
            app.selection_overlay_dirty = true;
            app.ui_state.status = format!("Added heights {lo:.0}-{hi:.0} m to selection");
        }
        other => return Err(other),
    };
    Ok(())
}

/// Samples at or below this are treated as unselected when deciding whether
/// the transient selection has any coverage worth inheriting.
pub(crate) const SELECTION_COVERAGE_EPSILON: f32 = 0.003;

/// True when the transient selection exists and covers anything (any painted
/// sample above [`SELECTION_COVERAGE_EPSILON`]).
pub(crate) fn selection_has_coverage(selection: Option<&terra_core::mask::MaskAsset>) -> bool {
    selection
        .and_then(|asset| asset.paint.as_ref())
        .is_some_and(|paint| {
            paint
                .samples()
                .iter()
                .any(|&sample| sample > SELECTION_COVERAGE_EPSILON)
        })
}

/// Write the transient selection into `layer`'s owned paint mask (creating
/// and binding it on first use) and record the mutation. Shared by the
/// explicit `MaskFromSelection` action and the "new layer inherits the
/// selection" post-action hook in `apply_actions`. Deliberately does NOT
/// touch focus / paint-arming - callers decide that.
pub(crate) fn write_selection_into_layer_mask(
    app: &mut TerraApp,
    layer: terra_core::layer::LayerId,
    ctx: &mut ApplyCtx,
) -> Option<terra_core::mask::MaskId> {
    let selection_paint = app.selection.as_ref().and_then(|asset| asset.paint.clone())?;
    let mask_id = app.session.document.ensure_layer_paint_mask(layer)?;
    // This overwrites whatever the layer's mask held, so record it as a
    // paint patch - otherwise applying a selection to a layer that already
    // had a mask is unrecoverable.
    let mut patch = None;
    if let Some(paint) = app
        .session
        .document
        .masks
        .iter_mut()
        .find(|m| m.id == mask_id)
        .and_then(|m| m.paint.as_mut())
    {
        let (before, w, h) = (paint.samples().to_vec(), paint.width, paint.height);
        paint.copy_from_resampled(&selection_paint);
        if paint.width == w && paint.height == h {
            patch = terra_core::document::MaskPaintPatch::from_diff(
                "Mask from Selection",
                mask_id,
                w,
                h,
                &before,
                paint.samples(),
            );
        }
    }
    if let Some(patch) = patch {
        app.session.push_mask_paint_patch(patch);
    }
    ctx.note_mask_mutation(mask_id);
    ctx.doc_mutated = true;
    app.preview_dirty = true;
    app.mask_overlay_dirty = true;
    Some(mask_id)
}

/// Dilate / erode step per Grow / Shrink click (texels).
const SELECTION_MORPH_RADIUS: u32 = 2;
/// Box-blur radius for Feather (texels).
const SELECTION_FEATHER_RADIUS: u32 = 3;

/// Take the transient selection asset out of the app, lazily creating it if
/// absent (mirrors the stamp handler). Callers must put it back via
/// `app.selection = Some(asset)`; taking it avoids borrow conflicts while
/// baking against other `TerraApp` fields.
fn take_or_new_selection(app: &mut TerraApp) -> terra_core::mask::MaskAsset {
    let resolution = app.selection_resolution();
    app.selection.take().unwrap_or_else(|| {
        let mut asset = terra_core::mask::MaskAsset::new_painted(
            terra_core::mask::MaskId::new(),
            "Selection",
            resolution,
        );
        asset.display_color = TerraApp::SELECTION_TINT;
        asset
    })
}

/// Max-combine (`sample = sample.max(band)`) membership of `[min, max]` into
/// the selection paint buffer, sampling `value_at(u, v)` nearest at each
/// texel centre. The band has a soft edge of ~10% of the range width.
fn bake_band_max(
    paint: &mut terra_core::mask::PaintBuffer,
    min: f32,
    max: f32,
    value_at: impl Fn(f32, f32) -> f32,
) {
    if paint.width == 0
        || paint.height == 0
        || paint.samples().len() != (paint.width * paint.height) as usize
    {
        return;
    }
    let range = (max - min).max(1e-3);
    let soft = (range * 0.10).max(1e-3);
    for j in 0..paint.height {
        let v = (j as f32 + 0.5) / paint.height as f32;
        for i in 0..paint.width {
            let u = (i as f32 + 0.5) / paint.width as f32;
            let s = value_at(u, v);
            let band = ((s - min) / soft).clamp(0.0, 1.0) * ((max - s) / soft).clamp(0.0, 1.0);
            let idx = (j * paint.width + i) as usize;
            let samples = paint.samples_mut();
            samples[idx] = samples[idx].max(band);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use terra_core::mask::{MaskAsset, MaskId};

    fn painted_selection(resolution: u32) -> MaskAsset {
        MaskAsset::new_painted(MaskId::new(), "Selection", resolution)
    }

    #[test]
    fn no_selection_has_no_coverage() {
        assert!(!selection_has_coverage(None));
    }

    #[test]
    fn empty_selection_has_no_coverage() {
        let asset = painted_selection(16);
        assert!(!selection_has_coverage(Some(&asset)));
    }

    #[test]
    fn epsilon_samples_do_not_count_as_coverage() {
        let mut asset = painted_selection(16);
        if let Some(paint) = asset.paint.as_mut() {
            for s in paint.samples_mut().iter_mut() {
                *s = SELECTION_COVERAGE_EPSILON;
            }
        }
        assert!(!selection_has_coverage(Some(&asset)));
    }

    #[test]
    fn single_painted_sample_counts_as_coverage() {
        let mut asset = painted_selection(16);
        if let Some(paint) = asset.paint.as_mut() {
            paint.samples_mut()[5] = 0.5;
        }
        assert!(selection_has_coverage(Some(&asset)));
    }
}
