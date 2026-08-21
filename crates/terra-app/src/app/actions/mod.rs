//! UI PanelAction dispatch, split by domain.

mod biomes;
mod layers;
mod masks;
mod scenarios;
mod settings;
mod tools;
mod world_rules;

use crate::ui::PanelAction;
use terra_core::layer::LayerId;

use super::TerraApp;

/// Shared mutation flags for a single apply_actions batch.
pub(crate) struct ApplyCtx {
    pub dirty_from: Option<LayerId>,
    pub sculpt_dirty_rect: Option<(u32, u32, u32, u32)>,
    pub doc_mutated: bool,
    pub mask_assets_mutated: bool,
    /// Masks changed this batch when each is individually known; when
    /// `mask_assets_mutated` is set with this empty, the batch falls back to
    /// a full rebuild.
    pub mutated_masks: Vec<terra_core::mask::MaskId>,
    pub continue_loop: bool,
}

impl ApplyCtx {
    pub fn new() -> Self {
        Self {
            dirty_from: None,
            sculpt_dirty_rect: None,
            doc_mutated: false,
            mask_assets_mutated: false,
            mutated_masks: Vec::new(),
            continue_loop: false,
        }
    }

    /// Record a mutation of one specific mask asset (targeted invalidation).
    pub fn note_mask_mutation(&mut self, mask: terra_core::mask::MaskId) {
        self.mask_assets_mutated = true;
        if !self.mutated_masks.contains(&mask) {
            self.mutated_masks.push(mask);
        }
    }
}

impl TerraApp {
    pub(crate) fn apply_actions(&mut self, actions: Vec<PanelAction>) {
        let mut ctx = ApplyCtx::new();
        // Photoshop-style "new layer inherits the selection as its mask":
        // when a user-facing layer-creation action arrives while a selection
        // with coverage is active, snapshot the stack's layer ids so we can
        // diff after dispatch. Coverage is checked once per batch (cheap,
        // early-exit scan). Undo/redo and sculpt-session layer creation
        // never flow through apply_actions, so they are structurally exempt.
        let batch_creates_layer = actions.iter().any(|a| {
            matches!(
                a,
                PanelAction::AddLayer(_)
                    | PanelAction::AddLayerToCategory { .. }
                    | PanelAction::AddLayerInto { .. }
                    | PanelAction::AddDevelopOperation { .. }
                    | PanelAction::AddSuggestedLayer { .. }
                    | PanelAction::ContextualCreate { .. }
            )
        });
        let layer_ids_before: Option<std::collections::HashSet<LayerId>> = (batch_creates_layer
            && masks::selection_has_coverage(self.selection.as_ref()))
        .then(|| self.session.document.stack.layer_ids().into_iter().collect());
        for action in actions {
            ctx.continue_loop = false;
            let action = match layers::try_apply(self, action, &mut ctx) {
                Ok(()) => continue,
                Err(a) => a,
            };
            if ctx.continue_loop {
                continue;
            }
            let action = match masks::try_apply(self, action, &mut ctx) {
                Ok(()) => continue,
                Err(a) => a,
            };
            if ctx.continue_loop {
                continue;
            }
            let action = match biomes::try_apply(self, action, &mut ctx) {
                Ok(()) => continue,
                Err(a) => a,
            };
            if ctx.continue_loop {
                continue;
            }
            let action = match world_rules::try_apply(self, action, &mut ctx) {
                Ok(()) => continue,
                Err(a) => a,
            };
            if ctx.continue_loop {
                continue;
            }
            let action = match scenarios::try_apply(self, action, &mut ctx) {
                Ok(()) => continue,
                Err(a) => a,
            };
            if ctx.continue_loop {
                continue;
            }
            let action = match settings::try_apply(self, action, &mut ctx) {
                Ok(()) => continue,
                Err(a) => a,
            };
            if ctx.continue_loop {
                continue;
            }
            match tools::try_apply(self, action, &mut ctx) {
                Ok(()) => {}
                Err(_a) => {
                    log::warn!("Unhandled PanelAction in apply_actions");
                }
            }
        }
        // Post-dispatch hook: every layer id that newly appeared this batch
        // inherits the active selection as its owned mask. Runs before the
        // dirty-flag processing below so note_mask_mutation feeds targeted
        // invalidation. Skips groups (find() only matches real layers) and
        // internal/auto kinds (sculpt strokes / base, compiled shape
        // constraints). No focus stealing - the user is mid-creation.
        if let Some(before) = layer_ids_before {
            let mut applied = false;
            for id in self.session.document.stack.layer_ids() {
                if before.contains(&id) {
                    continue;
                }
                let eligible = self.session.document.stack.find(id).is_some_and(|l| {
                    !matches!(
                        l.kind,
                        terra_core::layer::LayerKind::SculptStrokes(_)
                            | terra_core::layer::LayerKind::SculptBase(_)
                            | terra_core::layer::LayerKind::TerrainConstraints(_)
                    )
                });
                if !eligible {
                    continue;
                }
                if masks::write_selection_into_layer_mask(self, id, &mut ctx).is_some() {
                    applied = true;
                }
            }
            if applied {
                self.ui_state.status = "New layer masked by selection".to_string();
            }
        }
        let dirty_from = ctx.dirty_from;
        let sculpt_dirty_rect = ctx.sculpt_dirty_rect;
        let doc_mutated = ctx.doc_mutated;
        let mask_assets_mutated = ctx.mask_assets_mutated;
        let sculpt_stamp = sculpt_dirty_rect.is_some();
        if let Some(rect) = sculpt_dirty_rect {
            if let Some(gpu) = self.gpu_engine.as_mut() {
                gpu.set_dirty_rect(Some(rect));
            }
        }
        if let Some(id) = dirty_from {
            // Suffix-only dirty - do not mark_all_dirty (preserves layer cache).
            // Sculpt stamps only change the base paint buffer; keep GPU dependents clean so
            // Draft can reuse cached noise/shape contributions and just re-blend.
            // Use preview stack so Global layer ids resolve (they are not in doc.stack).
            let preview = self.session.document.preview_eval_stack();
            let is_global = false;
            if is_global {
                self.mark_all_layers_dirty();
                self.request_rebuild();
            } else {
                let sculpt_only = sculpt_stamp
                    && matches!(
                        preview.find(id).map(|layer| &layer.kind),
                        Some(terra_core::layer::LayerKind::SculptBase(_))
                    );
                self.scheduler.evaluator.mark_dirty_from(&preview, id);
                self.track_worker_dirty_from(&preview, id);
                self.terrain_runtime.advance_output_revision();
                if let Some(gpu) = self.gpu_engine.as_mut() {
                    if sculpt_only {
                        gpu.mark_dirty(id);
                    } else {
                        gpu.mark_dirty_from(&preview, id);
                    }
                }
                if sculpt_stamp {
                    self.request_rebuild();
                } else {
                    // Add/reorder/param: present Draft on the next tick (WC realtime).
                    self.request_rebuild_immediate();
                }
            }
        }
        if mask_assets_mutated {
            if ctx.mutated_masks.is_empty() {
                self.mark_all_layers_dirty();
                self.request_rebuild();
            } else {
                for mask in std::mem::take(&mut ctx.mutated_masks) {
                    self.mark_dirty_for_mask(mask);
                }
            }
        }
        if doc_mutated || dirty_from.is_some() || sculpt_stamp {
            self.mark_document_dirty();
        }
    }
}
