//! Editor session: document + undo stacks (non-region).

use crate::command::CommandHistory;
use crate::layer::LayerId;
use crate::mask::MaskId;
use crate::rebuild_feedback::RebuildFeedbackState;
use crate::simulation_scenario::SimulationScenarioCommand;
use crate::world_rules::WorldRuleCommand;
use serde::{Deserialize, Serialize};

use super::TerrainDocument;

/// One mask-paint edit as a rect-scoped before/after patch. Stores only the
/// changed region (row-major within `x,y,w,h`), so undo and redo are both
/// cheap and symmetric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskPaintPatch {
    pub label: String,
    pub mask_id: MaskId,
    /// Full paint-buffer dimensions when the patch was taken; guards against
    /// applying onto a resized buffer.
    pub buffer_width: u32,
    pub buffer_height: u32,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub before: Vec<f32>,
    pub after: Vec<f32>,
}

impl MaskPaintPatch {
    /// Build a patch by diffing two full buffers of identical dimensions.
    /// Returns `None` when nothing changed or the buffers disagree in size.
    pub fn from_diff(
        label: impl Into<String>,
        mask_id: MaskId,
        width: u32,
        height: u32,
        before: &[f32],
        after: &[f32],
    ) -> Option<Self> {
        if before.len() != after.len() || before.len() != (width * height) as usize {
            return None;
        }
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (u32::MAX, u32::MAX, 0u32, 0u32);
        for y in 0..height {
            let row = (y * width) as usize;
            for x in 0..width {
                if before[row + x as usize] != after[row + x as usize] {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        if min_x == u32::MAX {
            return None;
        }
        let (w, h) = (max_x - min_x + 1, max_y - min_y + 1);
        let copy_rect = |src: &[f32]| {
            let mut out = Vec::with_capacity((w * h) as usize);
            for y in min_y..=max_y {
                let row = (y * width + min_x) as usize;
                out.extend_from_slice(&src[row..row + w as usize]);
            }
            out
        };
        Some(Self {
            label: label.into(),
            mask_id,
            buffer_width: width,
            buffer_height: height,
            x: min_x,
            y: min_y,
            w,
            h,
            before: copy_rect(before),
            after: copy_rect(after),
        })
    }

    fn write_into(&self, samples: &mut [f32], use_before: bool) {
        let src = if use_before { &self.before } else { &self.after };
        for y in 0..self.h {
            let dst = ((self.y + y) * self.buffer_width + self.x) as usize;
            let s = (y * self.w) as usize;
            samples[dst..dst + self.w as usize]
                .copy_from_slice(&src[s..s + self.w as usize]);
        }
    }
}

#[derive(Debug, Clone)]
pub struct PaintStrokeUndo {
    pub label: String,
    pub placement_id: crate::biome_paint::BiomeLayerId,
    pub biome_id: LayerId,
    pub before_channels: Vec<crate::biome_paint::BiomeWeightChannel>,
    pub before_pages: Vec<crate::sparse_paint::PaintPage>,
    pub used_sparse: bool,
}

pub struct EditorSession {
    pub document: TerrainDocument,
    pub history: CommandHistory,
    pub outdated_sim_layers: Vec<LayerId>,
    pub rebuild_feedback: RebuildFeedbackState,
    /// UI hint: next frame should schedule a rebuild.
    pub dirty_eval: bool,
    world_rule_undo: Vec<WorldRuleCommand>,
    world_rule_redo: Vec<WorldRuleCommand>,
    scenario_undo: Vec<SimulationScenarioCommand>,
    scenario_redo: Vec<SimulationScenarioCommand>,
    mask_paint_undo: Vec<MaskPaintPatch>,
    mask_paint_redo: Vec<MaskPaintPatch>,
    paint_undo: Vec<PaintStrokeUndo>,
}

impl Default for EditorSession {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorSession {
    pub fn new() -> Self {
        Self {
            document: TerrainDocument::new_default(),
            history: CommandHistory::new(256),
            outdated_sim_layers: Vec::new(),
            rebuild_feedback: RebuildFeedbackState::default(),
            dirty_eval: false,
            world_rule_undo: Vec::new(),
            world_rule_redo: Vec::new(),
            scenario_undo: Vec::new(),
            scenario_redo: Vec::new(),
            mask_paint_undo: Vec::new(),
            mask_paint_redo: Vec::new(),
            paint_undo: Vec::new(),
        }
    }

    pub fn with_document(document: TerrainDocument) -> Self {
        Self {
            document,
            ..Self::new()
        }
    }

    pub fn push_world_rule_command(&mut self, cmd: WorldRuleCommand) {
        cmd.apply(&mut self.document.world_rules);
        self.world_rule_undo.push(cmd);
        self.world_rule_redo.clear();
    }

    pub fn undo_world_rule(&mut self) -> bool {
        let Some(cmd) = self.world_rule_undo.pop() else {
            return false;
        };
        cmd.invert(&mut self.document.world_rules);
        self.world_rule_redo.push(cmd);
        true
    }

    pub fn redo_world_rule(&mut self) -> bool {
        let Some(cmd) = self.world_rule_redo.pop() else {
            return false;
        };
        cmd.apply(&mut self.document.world_rules);
        self.world_rule_undo.push(cmd);
        true
    }

    pub fn push_scenario_command(&mut self, cmd: SimulationScenarioCommand) {
        cmd.apply(&mut self.document.simulation_scenarios);
        self.scenario_undo.push(cmd);
        self.scenario_redo.clear();
    }

    pub fn undo_scenario(&mut self) -> bool {
        let Some(cmd) = self.scenario_undo.pop() else {
            return false;
        };
        cmd.invert(&mut self.document.simulation_scenarios);
        self.scenario_redo.push(cmd);
        true
    }

    pub fn redo_scenario(&mut self) -> bool {
        let Some(cmd) = self.scenario_redo.pop() else {
            return false;
        };
        cmd.apply(&mut self.document.simulation_scenarios);
        self.scenario_undo.push(cmd);
        true
    }

    pub fn push_mask_paint_patch(&mut self, patch: MaskPaintPatch) {
        self.mask_paint_undo.push(patch);
        self.mask_paint_redo.clear();
    }

    fn apply_mask_patch(document: &mut TerrainDocument, patch: &MaskPaintPatch, before: bool) {
        if let Some(asset) = document.masks.iter_mut().find(|m| m.id == patch.mask_id) {
            if let Some(paint) = asset.paint.as_mut() {
                if paint.width == patch.buffer_width && paint.height == patch.buffer_height {
                    patch.write_into(&mut paint.samples, before);
                }
            }
        }
    }

    pub fn undo_mask_paint(&mut self) -> bool {
        let Some(patch) = self.mask_paint_undo.pop() else {
            return false;
        };
        Self::apply_mask_patch(&mut self.document, &patch, true);
        self.mask_paint_redo.push(patch);
        true
    }

    pub fn redo_mask_paint(&mut self) -> bool {
        let Some(patch) = self.mask_paint_redo.pop() else {
            return false;
        };
        Self::apply_mask_patch(&mut self.document, &patch, false);
        self.mask_paint_undo.push(patch);
        true
    }

    #[cfg(test)]
    pub(crate) fn mask_paint_stack_depths(&self) -> (usize, usize) {
        (self.mask_paint_undo.len(), self.mask_paint_redo.len())
    }

    pub fn push_paint_undo(&mut self, stroke: PaintStrokeUndo) {
        self.paint_undo.push(stroke);
    }

    pub fn undo_paint_stroke(&mut self) -> bool {
        let Some(stroke) = self.paint_undo.pop() else {
            return false;
        };
        if let Some(layer) = self
            .document
            .biome_layers
            .iter_mut()
            .find(|l| l.id == stroke.placement_id)
        {
            layer.channels = stroke.before_channels;
        }
        if stroke.used_sparse {
            let key = crate::sparse_paint::SparsePaintChannelKey {
                placement_id: stroke.placement_id,
                biome_id: stroke.biome_id,
            };
            self.document
                .sparse_paint
                .restore_pages(key, &stroke.before_pages);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mask::MaskAsset;

    #[test]
    fn mask_paint_patch_undo_redo_round_trip() {
        let mut session = EditorSession::new();
        let mask_id = MaskId::new();
        let mut asset = MaskAsset::new_painted(mask_id, "Paint", 8);
        let before: Vec<f32> = asset.paint.as_ref().unwrap().samples.clone();
        // Stroke: set a 2×2 region.
        for (x, y) in [(2u32, 3u32), (3, 3), (2, 4), (3, 4)] {
            asset.paint.as_mut().unwrap().samples[(y * 8 + x) as usize] = 0.75;
        }
        let after = asset.paint.as_ref().unwrap().samples.clone();
        session.document.masks.push(asset);

        let patch = MaskPaintPatch::from_diff("Painted Mask", mask_id, 8, 8, &before, &after)
            .expect("stroke changed samples");
        assert_eq!((patch.x, patch.y, patch.w, patch.h), (2, 3, 2, 2));
        session.push_mask_paint_patch(patch);
        assert_eq!(session.mask_paint_stack_depths(), (1, 0));

        assert!(session.undo_mask_paint());
        assert_eq!(
            session.document.masks[0].paint.as_ref().unwrap().samples,
            before
        );
        assert_eq!(session.mask_paint_stack_depths(), (0, 1));

        assert!(session.redo_mask_paint());
        assert_eq!(
            session.document.masks[0].paint.as_ref().unwrap().samples,
            after
        );
        assert!(!session.redo_mask_paint());
    }

    #[test]
    fn identical_buffers_produce_no_patch() {
        let buf = vec![0.5f32; 16];
        assert!(MaskPaintPatch::from_diff("x", MaskId::new(), 4, 4, &buf, &buf).is_none());
    }
}
