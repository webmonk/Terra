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
    world_rule_undo: Vec<(WorldRuleCommand, u64)>,
    world_rule_redo: Vec<(WorldRuleCommand, u64)>,
    scenario_undo: Vec<(SimulationScenarioCommand, u64)>,
    scenario_redo: Vec<(SimulationScenarioCommand, u64)>,
    mask_paint_undo: Vec<(MaskPaintPatch, u64)>,
    mask_paint_redo: Vec<(MaskPaintPatch, u64)>,
    paint_undo: Vec<(PaintStrokeUndo, u64)>,
}

/// Which undo stack owns the chronologically newest entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UndoDomain {
    Stack,
    WorldRule,
    Scenario,
    MaskPaint,
    BiomePaint,
}

/// One row of the unified chronological History panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    /// Global edit stamp from [`crate::command::next_edit_seq`].
    pub seq: u64,
    /// Artist-facing label.
    pub label: String,
    /// Which undo stack owns the entry.
    pub domain: UndoDomain,
    /// True when the entry has been undone and awaits redo.
    pub pending_redo: bool,
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

    /// Stack owning the newest undoable edit, by global edit stamp.
    pub fn newest_undo_domain(&self) -> Option<UndoDomain> {
        let candidates = [
            (UndoDomain::Stack, self.history.top_undo_seq()),
            (
                UndoDomain::WorldRule,
                self.world_rule_undo.last().map(|(_, s)| *s),
            ),
            (
                UndoDomain::Scenario,
                self.scenario_undo.last().map(|(_, s)| *s),
            ),
            (
                UndoDomain::MaskPaint,
                self.mask_paint_undo.last().map(|(_, s)| *s),
            ),
            (
                UndoDomain::BiomePaint,
                self.paint_undo.last().map(|(_, s)| *s),
            ),
        ];
        candidates
            .into_iter()
            .filter_map(|(d, s)| s.map(|s| (d, s)))
            .max_by_key(|&(_, s)| s)
            .map(|(d, _)| d)
    }

    /// Stack owning the oldest pending redo (the first edit to re-apply).
    pub fn oldest_redo_domain(&self) -> Option<UndoDomain> {
        let candidates = [
            (UndoDomain::Stack, self.history.top_redo_seq()),
            (
                UndoDomain::WorldRule,
                self.world_rule_redo.last().map(|(_, s)| *s),
            ),
            (
                UndoDomain::Scenario,
                self.scenario_redo.last().map(|(_, s)| *s),
            ),
            (
                UndoDomain::MaskPaint,
                self.mask_paint_redo.last().map(|(_, s)| *s),
            ),
        ];
        candidates
            .into_iter()
            .filter_map(|(d, s)| s.map(|s| (d, s)))
            .min_by_key(|&(_, s)| s)
            .map(|(d, _)| d)
    }

    /// Every edit across all undo stacks, merged chronologically (ascending
    /// by global edit stamp). Undone-but-redoable edits carry
    /// `pending_redo = true`.
    pub fn unified_history(&self) -> Vec<HistoryEntry> {
        let mut entries: Vec<HistoryEntry> = Vec::new();
        let mut push = |seq: u64, label: String, domain: UndoDomain, pending_redo: bool| {
            entries.push(HistoryEntry {
                seq,
                label,
                domain,
                pending_redo,
            });
        };
        for (seq, label) in self.history.undo_entries() {
            push(seq, label, UndoDomain::Stack, false);
        }
        for (seq, label) in self.history.redo_entries() {
            push(seq, label, UndoDomain::Stack, true);
        }
        for (_, seq) in &self.world_rule_undo {
            push(*seq, "World Rule Edit".into(), UndoDomain::WorldRule, false);
        }
        for (_, seq) in &self.world_rule_redo {
            push(*seq, "World Rule Edit".into(), UndoDomain::WorldRule, true);
        }
        for (_, seq) in &self.scenario_undo {
            push(*seq, "Scenario Edit".into(), UndoDomain::Scenario, false);
        }
        for (_, seq) in &self.scenario_redo {
            push(*seq, "Scenario Edit".into(), UndoDomain::Scenario, true);
        }
        for (patch, seq) in &self.mask_paint_undo {
            push(*seq, patch.label.clone(), UndoDomain::MaskPaint, false);
        }
        for (patch, seq) in &self.mask_paint_redo {
            push(*seq, patch.label.clone(), UndoDomain::MaskPaint, true);
        }
        // Biome paint strokes have no redo stack.
        for (stroke, seq) in &self.paint_undo {
            push(*seq, stroke.label.clone(), UndoDomain::BiomePaint, false);
        }
        entries.sort_by_key(|e| e.seq);
        entries
    }

    /// Cheap change fingerprint across every undo/redo stack, for UI cache
    /// invalidation (extends [`CommandHistory::ui_fingerprint`]). Includes the
    /// stack's newest edit stamp so coalesced in-place updates are caught.
    pub fn history_ui_fingerprint(&self) -> (usize, usize) {
        let (stack_undo, stack_redo) = self.history.ui_fingerprint();
        let parts = [
            stack_undo as u64,
            stack_redo as u64,
            self.history.top_undo_seq().unwrap_or(0),
            self.world_rule_undo.len() as u64,
            self.world_rule_redo.len() as u64,
            self.scenario_undo.len() as u64,
            self.scenario_redo.len() as u64,
            self.mask_paint_undo.len() as u64,
            self.mask_paint_redo.len() as u64,
            self.paint_undo.len() as u64,
        ];
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for part in parts {
            hash = (hash ^ part).wrapping_mul(0x1000_0000_01b3);
        }
        ((hash >> 32) as usize, (hash & 0xffff_ffff) as usize)
    }

    pub fn push_world_rule_command(&mut self, cmd: WorldRuleCommand) {
        cmd.apply(&mut self.document.world_rules);
        self.world_rule_undo
            .push((cmd, crate::command::next_edit_seq()));
        self.world_rule_redo.clear();
    }

    pub fn undo_world_rule(&mut self) -> bool {
        let Some((cmd, seq)) = self.world_rule_undo.pop() else {
            return false;
        };
        cmd.invert(&mut self.document.world_rules);
        self.world_rule_redo.push((cmd, seq));
        true
    }

    pub fn redo_world_rule(&mut self) -> bool {
        let Some((cmd, seq)) = self.world_rule_redo.pop() else {
            return false;
        };
        cmd.apply(&mut self.document.world_rules);
        self.world_rule_undo.push((cmd, seq));
        true
    }

    pub fn push_scenario_command(&mut self, cmd: SimulationScenarioCommand) {
        cmd.apply(&mut self.document.simulation_scenarios);
        self.scenario_undo
            .push((cmd, crate::command::next_edit_seq()));
        self.scenario_redo.clear();
    }

    pub fn undo_scenario(&mut self) -> bool {
        let Some((cmd, seq)) = self.scenario_undo.pop() else {
            return false;
        };
        cmd.invert(&mut self.document.simulation_scenarios);
        self.scenario_redo.push((cmd, seq));
        true
    }

    pub fn redo_scenario(&mut self) -> bool {
        let Some((cmd, seq)) = self.scenario_redo.pop() else {
            return false;
        };
        cmd.apply(&mut self.document.simulation_scenarios);
        self.scenario_undo.push((cmd, seq));
        true
    }

    pub fn push_mask_paint_patch(&mut self, patch: MaskPaintPatch) {
        self.mask_paint_undo
            .push((patch, crate::command::next_edit_seq()));
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

    /// Returns the edited mask's id so callers can invalidate selectively.
    pub fn undo_mask_paint(&mut self) -> Option<MaskId> {
        let (patch, seq) = self.mask_paint_undo.pop()?;
        Self::apply_mask_patch(&mut self.document, &patch, true);
        let id = patch.mask_id;
        self.mask_paint_redo.push((patch, seq));
        Some(id)
    }

    /// Returns the edited mask's id so callers can invalidate selectively.
    pub fn redo_mask_paint(&mut self) -> Option<MaskId> {
        let (patch, seq) = self.mask_paint_redo.pop()?;
        Self::apply_mask_patch(&mut self.document, &patch, false);
        let id = patch.mask_id;
        self.mask_paint_undo.push((patch, seq));
        Some(id)
    }

    #[cfg(test)]
    pub(crate) fn mask_paint_stack_depths(&self) -> (usize, usize) {
        (self.mask_paint_undo.len(), self.mask_paint_redo.len())
    }

    pub fn push_paint_undo(&mut self, stroke: PaintStrokeUndo) {
        self.paint_undo
            .push((stroke, crate::command::next_edit_seq()));
    }

    pub fn undo_paint_stroke(&mut self) -> bool {
        let Some((stroke, _seq)) = self.paint_undo.pop() else {
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

        assert_eq!(session.undo_mask_paint(), Some(mask_id));
        assert_eq!(
            session.document.masks[0].paint.as_ref().unwrap().samples,
            before
        );
        assert_eq!(session.mask_paint_stack_depths(), (0, 1));

        assert_eq!(session.redo_mask_paint(), Some(mask_id));
        assert_eq!(
            session.document.masks[0].paint.as_ref().unwrap().samples,
            after
        );
        assert!(session.redo_mask_paint().is_none());
    }

    #[test]
    fn undo_order_is_chronological_across_stacks() {
        use crate::command::EditorCommand;
        use crate::layer::{FlatParams, Layer, LayerKind};

        let mut session = EditorSession::new();
        let layer = Layer::new("A", LayerKind::Flat(FlatParams { height: 1.0 }));
        let id = layer.id();
        session.document.stack.push(layer);

        // Edit 1: stack command. Edit 2: mask paint.
        let cmd = EditorCommand::SetOpacity {
            id,
            opacity: 0.5,
            previous: 1.0,
        };
        crate::command::apply(&cmd, &mut session.document.stack);
        session.history.push_executed(cmd);
        let mask_id = MaskId::new();
        session.push_mask_paint_patch(MaskPaintPatch {
            label: "Painted Mask".into(),
            mask_id,
            buffer_width: 2,
            buffer_height: 1,
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            before: vec![0.0],
            after: vec![1.0],
        });

        // Newest edit is the paint; it must undo first, then the command.
        assert_eq!(session.newest_undo_domain(), Some(UndoDomain::MaskPaint));
        assert!(session.undo_mask_paint().is_some());
        assert_eq!(session.newest_undo_domain(), Some(UndoDomain::Stack));

        // Redo replays oldest-undone first: the stack command was undone
        // last, so once it is undone too, redo starts with it.
        session.history.undo(&mut session.document.stack);
        assert_eq!(session.oldest_redo_domain(), Some(UndoDomain::Stack));
        session.history.redo(&mut session.document.stack);
        assert_eq!(session.oldest_redo_domain(), Some(UndoDomain::MaskPaint));
    }

    #[test]
    fn unified_history_merges_stacks_chronologically() {
        use crate::command::EditorCommand;
        use crate::layer::{FlatParams, Layer, LayerKind};

        let mut session = EditorSession::new();
        let layer = Layer::new("A", LayerKind::Flat(FlatParams { height: 1.0 }));
        let id = layer.id();
        session.document.stack.push(layer);

        // Edit 1: stack command. Edit 2: mask paint. Then undo the paint.
        let cmd = EditorCommand::SetOpacity {
            id,
            opacity: 0.5,
            previous: 1.0,
        };
        crate::command::apply(&cmd, &mut session.document.stack);
        session.history.push_executed(cmd);
        session.push_mask_paint_patch(MaskPaintPatch {
            label: "Painted Mask".into(),
            mask_id: MaskId::new(),
            buffer_width: 2,
            buffer_height: 1,
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            before: vec![0.0],
            after: vec![1.0],
        });
        assert!(session.undo_mask_paint().is_some());

        let entries = session.unified_history();
        assert_eq!(entries.len(), 2);
        // Ascending by seq: the stack command came first.
        assert!(entries[0].seq < entries[1].seq);
        assert_eq!(entries[0].domain, UndoDomain::Stack);
        assert_eq!(entries[0].label, "Changed Opacity");
        assert!(!entries[0].pending_redo);
        assert_eq!(entries[1].domain, UndoDomain::MaskPaint);
        assert_eq!(entries[1].label, "Painted Mask");
        assert!(entries[1].pending_redo);
    }

    #[test]
    fn identical_buffers_produce_no_patch() {
        let buf = vec![0.5f32; 16];
        assert!(MaskPaintPatch::from_diff("x", MaskId::new(), 4, 4, &buf, &buf).is_none());
    }
}
