use crate::ui::PanelAction;

use super::super::TerraApp;
use super::ApplyCtx;

pub(crate) fn try_apply(
    app: &mut TerraApp,
    action: PanelAction,
    ctx: &mut ApplyCtx,
) -> Result<(), PanelAction> {
    let result = match action {
                PanelAction::AddBiome { name } => {
                    let biome = terra_core::layer::LayerGroup::biome(name);
                    let id = biome.id;
                    app.session.document.stack.ensure_category_folders();
                    if let Some(folder) = app                        .session
                        .document
                        .stack
                        .find_category_mut(terra_core::layer::StackCategory::Surface)
                    {
                        folder
                            .children
                            .push(terra_core::layer::StackNode::Group(biome));
                    } else {
                        app.session.document.stack.push_group(biome);
                    }
                    app.session.document.active_biome = Some(id);
                    app.session.document.selected = Some(id);
                    app.ui_state.focus_created_biome();
                    // Structural biome create — full stack topology changed.
                    app.mark_all_layers_dirty();
                    app.request_rebuild();
                    ctx.doc_mutated = true;
                }
                PanelAction::SetActiveBiome(id) => {
                    if app                        .session
                        .document
                        .stack
                        .find_group(id)
                        .is_some_and(|g| g.is_biome())
                    {
                        app.session.document.active_biome = Some(id);
                        app.session.document.selected = Some(id);
                    }
                }
                PanelAction::PaintBiomeStamp {
                    biome,
                    u,
                    v,
                    radius,
                    strength,
                    erase,
                    mode,
                } => {
                    let placement_id = app.session.document.ensure_placement_layer();
                    let world_x = app.session.document.metrics.world_size_x.max(1.0);
                    let world_z = app.session.document.metrics.world_size_z.max(1.0);
                    let wx = u * world_x;
                    let wz = v * world_z;
                    let radius_m = radius * world_x.max(world_z);
                    let key = terra_core::sparse_paint::SparsePaintChannelKey {
                        placement_id,
                        biome_id: biome,
                    };
                    let tool = mode.unwrap_or(if erase {
                        terra_core::biome_paint::BiomePaintTool::Erase
                    } else {
                        terra_core::biome_paint::BiomePaintTool::Paint
                    });
                    // Sparse world-space paint (grows with painted area).
                    match tool {
                        terra_core::biome_paint::BiomePaintTool::Erase => {
                            app.session
                                .document
                                .sparse_paint
                                .stamp_circle(key, wx, wz, radius_m, strength, true);
                        }
                        terra_core::biome_paint::BiomePaintTool::Smooth => {
                            // Dense smooth still applied below; sparse gets a light erase+paint soften.
                            app.session.document.sparse_paint.stamp_circle(
                                key,
                                wx,
                                wz,
                                radius_m,
                                strength * 0.15,
                                true,
                            );
                        }
                        _ => {
                            app.session
                                .document
                                .sparse_paint
                                .stamp_circle(key, wx, wz, radius_m, strength, false);
                        }
                    }
                    let res = app.session.document.preview_resolution.min(1024).max(64);
                    if let Some(layer) = app.session.document.selected_placement_layer_mut() {
                        match tool {
                            terra_core::biome_paint::BiomePaintTool::Smooth => {
                                layer.smooth_at(u, v, radius, strength);
                            }
                            terra_core::biome_paint::BiomePaintTool::Replace => {
                                layer.stamp_replace(biome, u, v, radius, strength, res);
                            }
                            terra_core::biome_paint::BiomePaintTool::Add => {
                                layer.stamp_add(biome, u, v, radius, strength, res);
                            }
                            terra_core::biome_paint::BiomePaintTool::Normalize => {
                                layer.normalize_all();
                            }
                            terra_core::biome_paint::BiomePaintTool::FloodFill => {
                                layer.flood_fill(
                                    biome,
                                    u,
                                    v,
                                    if erase { 0.0 } else { strength },
                                    0.12,
                                    res,
                                );
                            }
                            terra_core::biome_paint::BiomePaintTool::PolygonFill => {
                                // Vertices committed via paint_at_cursor; treat stamp as no-op.
                            }
                            _ => {
                                layer.stamp(biome, u, v, radius, strength, erase, res);
                            }
                        }
                    }
                    app.placement_tint_dirty = true;
                    // Sync mask bridge lazily on stroke end only when not mid-stroke.
                    if !app.sculpt_stroke_active {
                        app.sync_biome_paint_to_mask(biome);
                    }
                    ctx.dirty_from = Some(biome);
                    ctx.doc_mutated = true;
                    app.preview_dirty = true;
                }
                PanelAction::NormalizeBiomePlacement => {
                    app.session.document.ensure_placement_layer();
                    if let Some(layer) = app.session.document.selected_placement_layer_mut() {
                        layer.normalize_all();
                        ctx.doc_mutated = true;
                    }
                }
                PanelAction::BeginBiomePaintStroke { biome } => {
                    let placement_id = app.session.document.ensure_placement_layer();
                    let key = terra_core::sparse_paint::SparsePaintChannelKey {
                        placement_id,
                        biome_id: biome,
                    };
                    let before_pages = app.session.document.sparse_paint.channel_pages(key);
                    let before_channels = app                        .session
                        .document
                        .selected_placement_layer()
                        .map(|l| l.channels.clone())
                        .unwrap_or_default();
                    app.session
                        .push_paint_undo(terra_core::document::PaintStrokeUndo {
                            label: "Biome Paint".into(),
                            placement_id,
                            biome_id: biome,
                            before_channels,
                            before_pages,
                            used_sparse: true,
                        });
                }
                PanelAction::EndBiomePaintStroke => {
                    if let Some(biome) = app.session.document.active_biome {
                        app.sync_biome_paint_to_mask(biome);
                        ctx.dirty_from = Some(biome);
                        ctx.doc_mutated = true;
                    }
                }
                PanelAction::EnsureBiomePaintLayer => {
                    if app.session.document.biome_layers.is_empty() {
                        let mut bl = terra_core::biome_paint::BiomeLayer::new("Biome Paint");
                        bl.show_biome_colors = true;
                        let id = bl.id;
                        app.session.document.biome_layers.push(bl);
                        app.session.document.selected_biome_layer = Some(id);
                    }
                }
                PanelAction::AddBiomePaintLayer { name } => {
                    let mut bl = terra_core::biome_paint::BiomeLayer::new(name);
                    bl.show_biome_colors = true;
                    let id = bl.id;
                    app.session.document.biome_layers.push(bl);
                    app.session.document.selected_biome_layer = Some(id);
                    app.ui_state.focus_created_biome();
                    ctx.doc_mutated = true;
                }
                PanelAction::SelectBiomePaintLayer(id) => {
                    if app                        .session
                        .document
                        .biome_layers
                        .iter()
                        .any(|b| b.id == id)
                    {
                        app.session.document.selected_biome_layer = Some(id);
                        app.ui_state.editor_tool = crate::ui::EditorTool::PaintBiome;
                        app.ui_state.workspace_mode = crate::ui::WorkspaceMode::Biomes;
                    }
                }
                PanelAction::AddHoleLayer { name } => {
                    let res = app.session.document.preview_resolution.min(512).max(64);
                    app.session
                        .document
                        .hole_layers
                        .push(terra_core::biome_paint::HoleLayer::new(name, res));
                    ctx.doc_mutated = true;
                }
                PanelAction::SetBiomeOverwrite {
                    target,
                    overwrite_filters,
                    overwrite_objects,
                } => {
                    if let Some(group) = app.session.document.stack.find_group_mut(target) {
                        group.overwrite_filters = overwrite_filters;
                        group.overwrite_objects = overwrite_objects;
                        ctx.dirty_from = Some(target);
                        ctx.doc_mutated = true;
                    }
                }
                PanelAction::SetBiomeFilterBlending { target, blending } => {
                    if let Some(group) = app.session.document.stack.find_group_mut(target) {
                        group.filter_blending = blending.clamp(0.0, 1.0);
                        ctx.dirty_from = Some(target);
                        ctx.doc_mutated = true;
                    }
                }
                PanelAction::CycleBiomePaintTool => {
                    app.ui_state.biome_paint_tool = app.ui_state.biome_paint_tool.cycle();
                }
                PanelAction::SetBiomePaintTool(tool) => {
                    if tool != terra_core::biome_paint::BiomePaintTool::PolygonFill {
                        app.biome_polygon_points.clear();
                    }
                    app.ui_state.biome_paint_tool = tool;
                }
                PanelAction::SelectBiomeDefinition(def_id) => {
                    app.session.document.biome_library.selected = Some(def_id);
                    app.ui_state.biome_focus = Some(def_id);
                    if let Some(def) = app.session.document.biome_library.get(def_id) {
                        if let Some(gid) = def.group_id {
                            app.session.document.active_biome = Some(gid);
                            // Stay in artist inspector â€” do not select the stack group.
                            app.session.document.selected = None;
                        } else {
                            app.ui_state.status =
                                format!("{} has no linked biome group yet.", def.name);
                        }
                    }
                    app.session.document.ensure_placement_layer();
                    if let Some(bl) = app.session.document.selected_placement_layer_mut() {
                        bl.show_biome_colors = true;
                    }
                    app.ui_state.editor_tool = crate::ui::EditorTool::PaintBiome;
                    app.ui_state.workspace_mode = crate::ui::WorkspaceMode::Biomes;
                    app.ui_state.app_workspace = crate::ui::AppWorkspace::Biomes;
                    app.ui_state.preview_mode = crate::ui::Preview2dMode::Biome;
                    app.ui_state.biome_paint_tool = terra_core::biome_paint::BiomePaintTool::Paint;
                    let combine = app                        .session
                        .document
                        .biome_library
                        .get(def_id)
                        .map(|d| d.placement.combine)
                        .unwrap_or_default();
                    let name = app                        .session
                        .document
                        .biome_library
                        .get(def_id)
                        .map(|d| d.name.clone())
                        .unwrap_or_else(|| "biome".into());
                    if matches!(
                        combine,
                        terra_core::biome_definition::PlacementCombineMode::PaintMulRules
                    ) {
                        app.ui_state.status = format!(
                            "Painting {name} (guided by rules â€” switch to Paint owns if strokes vanish)"
                        );
                    } else {
                        app.ui_state.status = format!("Painting {name} â€” paint owns this area");
                    }
                }
                PanelAction::SetBiomeColorPreview(on) => {
                    // Workspace-only overlay preference â€” do not create placement
                    // layers or write BiomeLayer fields (project must stay clean).
                    app.ui_state.set_biome_color_preview(on);
                    app.placement_tint_dirty = true;
                    app.preview_dirty = true;
                }
                PanelAction::SetBiomePlacementCombine {
                    definition,
                    combine,
                } => {
                    if let Some(def) = app.session.document.biome_library.get_mut(definition) {
                        def.placement.combine = combine;
                        let name = def.name.clone();
                        let label = combine.artist_label();
                        if let Some(gid) = def.group_id {
                            ctx.dirty_from = Some(gid);
                        }
                        ctx.doc_mutated = true;
                        app.ui_state.status = format!("{name} â€” {label}");
                    }
                }
                PanelAction::MarkBiomePlacementCustom { definition } => {
                    if let Some(def) = app.session.document.biome_library.get_mut(definition) {
                        let stack = def
                            .group_id
                            .and_then(|gid| {
                                app.session
                                    .document
                                    .stack
                                    .find_group(gid)
                                    .map(|g| g.masks.clone())
                            })
                            .unwrap_or_else(|| def.placement.compiled_distribution());
                        def.placement.mark_mask_stack_custom(stack);
                        ctx.doc_mutated = true;
                        app.ui_state.status = format!("{} â€” Mask Stack (Custom)", def.name);
                    }
                }
                PanelAction::ResetBiomePlacementToRules { definition } => {
                    if let Some(def) = app.session.document.biome_library.get_mut(definition) {
                        let dist = def.placement.reset_placement_to_rules();
                        let name = def.name.clone();
                        if let Some(gid) = def.group_id {
                            if let Some(g) = app.session.document.stack.find_group_mut(gid) {
                                g.masks = dist;
                            }
                            ctx.dirty_from = Some(gid);
                        }
                        ctx.doc_mutated = true;
                        app.ui_state.status = format!("{name} â€” reset to Placement rules");
                    }
                }
        other => return Err(other),
    };
    let _ = result;
    Ok(())
}

