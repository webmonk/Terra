//! Command-based undo/redo (params & stack ops, not texture snapshots).

use crate::layer::{BlendMode, Layer, LayerGroup, LayerId, LayerKind, LayerStack, StackNode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EditorCommand {
    AddLayer {
        layer: Layer,
        index: usize,
    },
    RemoveLayer {
        id: LayerId,
        node: StackNode,
        /// Sibling index within `parent` (root when `parent` is `None`).
        index: usize,
        /// Parent group when the node lived nested in the WC tree.
        #[serde(default)]
        parent: Option<LayerId>,
    },
    Reorder {
        from: usize,
        to: usize,
    },
    SetEnabled {
        id: LayerId,
        enabled: bool,
        previous: bool,
    },
    SetOpacity {
        id: LayerId,
        opacity: f32,
        previous: f32,
    },
    SetBlend {
        id: LayerId,
        blend: BlendMode,
        previous: BlendMode,
    },
    SetKind {
        id: LayerId,
        kind: LayerKind,
        previous: LayerKind,
    },
    Rename {
        id: LayerId,
        name: String,
        previous: String,
    },
    Duplicate {
        source: LayerId,
        new_id: LayerId,
    },
    SetLocked {
        id: LayerId,
        locked: bool,
        previous: bool,
    },
    SetSolo {
        id: LayerId,
        solo: bool,
        previous: bool,
    },
    SetColorTag {
        id: LayerId,
        tag: u8,
        previous: u8,
    },
    SetCached {
        id: LayerId,
        cached: bool,
        previous: bool,
    },
    AddGroup {
        name: String,
        id: LayerId,
        index: usize,
    },
    /// Cross-parent structural move (drag reorder, group, ungroup, biome
    /// section move). Records the sibling location before and after the move
    /// so undo/redo replay it exactly.
    MoveNode {
        id: LayerId,
        from_parent: Option<LayerId>,
        from_index: usize,
        to_parent: Option<LayerId>,
        to_index: usize,
    },
    /// Insert a prebuilt node under `parent` (root when `None`) at `index`.
    /// Used for auto-created layers (e.g. sculpt Shape Layers) so their
    /// creation is undoable at the exact tree location.
    InsertNode {
        node: StackNode,
        parent: Option<LayerId>,
        index: usize,
    },
    /// One sculpt gesture on a `SculptStrokes` layer. Undo truncates the
    /// stroke list back to its pre-gesture shape; redo re-appends the
    /// recorded strokes/points.
    SculptGesture {
        id: LayerId,
        /// `strokes.len()` before the gesture.
        base_strokes: usize,
        /// Point count of the last pre-existing stroke (it may have been extended).
        base_last_points: usize,
        /// Points appended to that pre-existing last stroke.
        last_extension: Vec<crate::authoring::SculptPoint>,
        /// Whole strokes appended by the gesture.
        added_strokes: Vec<crate::authoring::SculptStroke>,
    },
    /// Records an artist action whose data cannot yet be restored by undo.
    Annotate {
        label: String,
    },
    /// Develop Apply Where / local placement (undo restores prior placement + masks).
    SetOperationPlacement {
        id: LayerId,
        placement: crate::operation_placement::OperationPlacement,
        previous: crate::operation_placement::OperationPlacement,
        previous_masks: crate::mask::Distribution,
    },
}

pub struct CommandHistory {
    undo_stack: Vec<EditorCommand>,
    redo_stack: Vec<EditorCommand>,
    snapshots: Vec<(String, usize)>,
    last_coalesce_key: Option<(u64, &'static str)>,
    pub limit: usize,
}

impl Default for CommandHistory {
    fn default() -> Self {
        Self::new(128)
    }
}

impl CommandHistory {
    pub fn new(limit: usize) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            snapshots: Vec::new(),
            last_coalesce_key: None,
            limit,
        }
    }

    pub fn push_executed(&mut self, cmd: EditorCommand) {
        self.last_coalesce_key = None;
        self.undo_stack.push(cmd);
        if self.undo_stack.len() > self.limit {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
    }

    /// Push a command that was already applied, replacing the preceding command
    /// when it belongs to the same continuous control interaction.
    pub fn push_coalesced(
        &mut self,
        cmd: EditorCommand,
        coalesce_key: Option<(u64, &'static str)>,
    ) {
        if coalesce_key.is_some() && coalesce_key == self.last_coalesce_key {
            if let Some(last) = self.undo_stack.last_mut() {
                // Preserve the first command's `previous` value so one Undo
                // restores the value from before the entire drag began.
                match (last, cmd) {
                    (
                        EditorCommand::SetOpacity { opacity: prior, .. },
                        EditorCommand::SetOpacity { opacity, .. },
                    ) => *prior = opacity,
                    (
                        EditorCommand::SetKind { kind: prior, .. },
                        EditorCommand::SetKind { kind, .. },
                    ) => *prior = kind,
                    (prior, replacement) => *prior = replacement,
                }
                self.redo_stack.clear();
                return;
            }
        }
        self.undo_stack.push(cmd);
        if self.undo_stack.len() > self.limit {
            self.undo_stack.remove(0);
        }
        self.redo_stack.clear();
        self.last_coalesce_key = coalesce_key;
    }

    pub fn mark_snapshot(&mut self, name: impl Into<String>) {
        self.snapshots.push((name.into(), self.undo_stack.len()));
    }

    pub fn snapshots(&self) -> &[(String, usize)] {
        &self.snapshots
    }

    /// Undo labels ordered from oldest to newest.
    pub fn undo_descriptions(&self) -> Vec<String> {
        self.undo_stack
            .iter()
            .map(EditorCommand::describe)
            .collect()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Cheap change fingerprint for UI cache invalidation.
    pub fn ui_fingerprint(&self) -> (usize, usize) {
        (self.undo_stack.len(), self.redo_stack.len())
    }

    pub fn undo(&mut self, stack: &mut LayerStack) -> Option<LayerId> {
        self.last_coalesce_key = None;
        let cmd = self.undo_stack.pop()?;
        let dirty = invert(&cmd, stack);
        self.redo_stack.push(cmd);
        dirty
    }

    pub fn redo(&mut self, stack: &mut LayerStack) -> Option<LayerId> {
        self.last_coalesce_key = None;
        let cmd = self.redo_stack.pop()?;
        let dirty = apply(&cmd, stack);
        self.undo_stack.push(cmd);
        dirty
    }
}

impl EditorCommand {
    /// Concise, artist-facing description suitable for the History panel.
    pub fn describe(&self) -> String {
        match self {
            Self::AddLayer { layer, .. } => format!("Added {}", layer.common.name),
            Self::RemoveLayer { .. } => "Removed Layer".into(),
            Self::Reorder { .. } => "Reordered Layers".into(),
            Self::SetEnabled { enabled, .. } => if *enabled {
                "Enabled Layer"
            } else {
                "Disabled Layer"
            }
            .into(),
            Self::SetOpacity { .. } => "Changed Opacity".into(),
            Self::SetBlend { .. } => "Changed Blend Mode".into(),
            Self::SetKind { kind, previous, .. } => {
                use crate::layer::param_reflect::{changed_paths, humanize_path};
                let paths = changed_paths(previous, kind);
                match paths.as_slice() {
                    [] => "Changed Layer Type".into(),
                    [path] => format!("Changed {}", humanize_path(path)),
                    many => format!("Changed {} Parameters", many.len()),
                }
            }
            Self::Rename { name, .. } => format!("Renamed to {}", name),
            Self::Duplicate { .. } => "Duplicated Layer".into(),
            Self::SetLocked { locked, .. } => if *locked {
                "Locked Layer"
            } else {
                "Unlocked Layer"
            }
            .into(),
            Self::SetSolo { solo, .. } => if *solo {
                "Soloed Layer"
            } else {
                "Unsoloed Layer"
            }
            .into(),
            Self::SetColorTag { .. } => "Changed Color Tag".into(),
            Self::SetCached { cached, .. } => if *cached {
                "Cached Layer"
            } else {
                "Uncached Layer"
            }
            .into(),
            Self::AddGroup { name, .. } => format!("Added Group {}", name),
            Self::MoveNode { .. } => "Moved Layer".into(),
            Self::InsertNode { node, .. } => match node {
                StackNode::Layer(l) => format!("Added {}", l.common.name),
                StackNode::Group(g) => format!("Added Group {}", g.name),
            },
            Self::SculptGesture { .. } => "Sculpt Stroke".into(),
            Self::Annotate { label } => label.clone(),
            Self::SetOperationPlacement { .. } => "Changed Apply Where".into(),
        }
    }
}

pub fn apply(cmd: &EditorCommand, stack: &mut LayerStack) -> Option<LayerId> {
    match cmd {
        EditorCommand::AddLayer { layer, index } => {
            let id = layer.id();
            let idx = (*index).min(stack.nodes.len());
            stack.nodes.insert(idx, StackNode::Layer(layer.clone()));
            Some(id)
        }
        EditorCommand::RemoveLayer { id, .. } => {
            stack.remove(*id);
            stack.layer_ids().first().copied()
        }
        EditorCommand::Reorder { from, to } => {
            stack.reorder(*from, *to);
            None
        }
        EditorCommand::SetEnabled { id, enabled, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.enabled = *enabled;
            } else if let Some(g) = stack.find_group_mut(*id) {
                g.enabled = *enabled;
            }
            Some(*id)
        }
        EditorCommand::SetOpacity { id, opacity, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.opacity = *opacity;
            } else if let Some(g) = stack.find_group_mut(*id) {
                g.opacity = *opacity;
            }
            Some(*id)
        }
        EditorCommand::SetBlend { id, blend, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.blend = *blend;
            } else if let Some(g) = stack.find_group_mut(*id) {
                g.blend = *blend;
            }
            Some(*id)
        }
        EditorCommand::SetKind { id, kind, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.kind = kind.clone();
            }
            Some(*id)
        }
        EditorCommand::Rename { id, name, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.name = name.clone();
            } else if let Some(g) = stack.find_group_mut(*id) {
                // Structural folders/sections keep fixed labels.
                if !matches!(
                    g.group_kind,
                    crate::layer::GroupKind::BiomeSection(_)
                        | crate::layer::GroupKind::CategoryFolder
                ) {
                    g.name = name.clone();
                }
            }
            Some(*id)
        }
        EditorCommand::Duplicate { source, .. } => stack.duplicate(*source),
        EditorCommand::SetLocked { id, locked, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.locked = *locked;
            }
            Some(*id)
        }
        EditorCommand::SetSolo { id, solo, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.solo = *solo;
            }
            Some(*id)
        }
        EditorCommand::SetColorTag { id, tag, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.color_tag = *tag;
            }
            Some(*id)
        }
        EditorCommand::SetCached { id, cached, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.cached = *cached;
            }
            Some(*id)
        }
        EditorCommand::AddGroup { name, id, index } => {
            let mut g = LayerGroup::new(name.clone());
            g.id = *id;
            let idx = (*index).min(stack.nodes.len());
            stack.nodes.insert(idx, StackNode::Group(g));
            Some(*id)
        }
        EditorCommand::MoveNode {
            id,
            to_parent,
            to_index,
            ..
        } => {
            stack.move_node_to(*id, *to_parent, *to_index);
            Some(*id)
        }
        EditorCommand::InsertNode {
            node,
            parent,
            index,
        } => {
            stack.insert_at_parent(*parent, *index, node.clone());
            match node {
                StackNode::Layer(l) => Some(l.id()),
                StackNode::Group(g) => Some(g.id),
            }
        }
        EditorCommand::SculptGesture {
            id,
            base_strokes,
            last_extension,
            added_strokes,
            ..
        } => {
            if let Some(l) = stack.find_mut(*id) {
                if let LayerKind::SculptStrokes(p) = &mut l.kind {
                    if *base_strokes > 0 && !last_extension.is_empty() {
                        if let Some(s) = p.strokes.get_mut(base_strokes - 1) {
                            s.points.extend(last_extension.iter().cloned());
                        }
                    }
                    p.strokes.extend(added_strokes.iter().cloned());
                }
            }
            Some(*id)
        }
        EditorCommand::Annotate { .. } => None,
        EditorCommand::SetOperationPlacement { id, placement, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.operation_placement = placement.clone();
                l.sync_operation_placement_masks();
            }
            Some(*id)
        }
    }
}

fn invert(cmd: &EditorCommand, stack: &mut LayerStack) -> Option<LayerId> {
    match cmd {
        EditorCommand::AddLayer { layer, .. } => {
            stack.remove(layer.id());
            None
        }
        EditorCommand::RemoveLayer {
            node,
            index,
            parent,
            ..
        } => {
            let idx = *index;
            if let Some(pid) = parent {
                if let Some(group) = stack.find_group_mut(*pid) {
                    let idx = idx.min(group.children.len());
                    group.children.insert(idx, node.clone());
                } else {
                    // Parent gone — restore at root.
                    let idx = idx.min(stack.nodes.len());
                    stack.nodes.insert(idx, node.clone());
                }
            } else {
                let idx = idx.min(stack.nodes.len());
                stack.nodes.insert(idx, node.clone());
            }
            match node {
                StackNode::Layer(l) => Some(l.id()),
                StackNode::Group(g) => Some(g.id),
            }
        }
        EditorCommand::Reorder { from, to } => {
            stack.reorder(*to, *from);
            None
        }
        EditorCommand::SetEnabled { id, previous, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.enabled = *previous;
            } else if let Some(g) = stack.find_group_mut(*id) {
                g.enabled = *previous;
            }
            Some(*id)
        }
        EditorCommand::SetOpacity { id, previous, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.opacity = *previous;
            } else if let Some(g) = stack.find_group_mut(*id) {
                g.opacity = *previous;
            }
            Some(*id)
        }
        EditorCommand::SetBlend { id, previous, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.blend = *previous;
            } else if let Some(g) = stack.find_group_mut(*id) {
                g.blend = *previous;
            }
            Some(*id)
        }
        EditorCommand::SetKind { id, previous, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.kind = previous.clone();
            }
            Some(*id)
        }
        EditorCommand::Rename { id, previous, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.name = previous.clone();
            } else if let Some(g) = stack.find_group_mut(*id) {
                if !matches!(
                    g.group_kind,
                    crate::layer::GroupKind::BiomeSection(_)
                        | crate::layer::GroupKind::CategoryFolder
                ) {
                    g.name = previous.clone();
                }
            }
            Some(*id)
        }
        EditorCommand::Duplicate { new_id, .. } => {
            stack.remove(*new_id);
            None
        }
        EditorCommand::SetLocked { id, previous, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.locked = *previous;
            }
            Some(*id)
        }
        EditorCommand::SetSolo { id, previous, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.solo = *previous;
            }
            Some(*id)
        }
        EditorCommand::SetColorTag { id, previous, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.color_tag = *previous;
            }
            Some(*id)
        }
        EditorCommand::SetCached { id, previous, .. } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.cached = *previous;
            }
            Some(*id)
        }
        EditorCommand::AddGroup { id, .. } => {
            stack.remove(*id);
            None
        }
        EditorCommand::MoveNode {
            id,
            from_parent,
            from_index,
            ..
        } => {
            stack.move_node_to(*id, *from_parent, *from_index);
            Some(*id)
        }
        EditorCommand::InsertNode { node, .. } => {
            match node {
                StackNode::Layer(l) => stack.remove(l.id()),
                StackNode::Group(g) => stack.remove(g.id),
            };
            None
        }
        EditorCommand::SculptGesture {
            id,
            base_strokes,
            base_last_points,
            ..
        } => {
            if let Some(l) = stack.find_mut(*id) {
                if let LayerKind::SculptStrokes(p) = &mut l.kind {
                    p.strokes.truncate(*base_strokes);
                    if *base_strokes > 0 {
                        if let Some(s) = p.strokes.get_mut(base_strokes - 1) {
                            s.points.truncate(*base_last_points);
                        }
                    }
                }
            }
            Some(*id)
        }
        EditorCommand::Annotate { .. } => None,
        EditorCommand::SetOperationPlacement {
            id,
            previous,
            previous_masks,
            ..
        } => {
            if let Some(l) = stack.find_mut(*id) {
                l.common.operation_placement = previous.clone();
                l.common.masks = previous_masks.clone();
            }
            Some(*id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::FlatParams;

    #[test]
    fn undo_redo_opacity() {
        let mut stack = LayerStack::new();
        let layer = Layer::new("A", LayerKind::Flat(FlatParams { height: 1.0 }));
        let id = layer.id();
        stack.push(layer);
        let mut hist = CommandHistory::new(32);
        let cmd = EditorCommand::SetOpacity {
            id,
            opacity: 0.5,
            previous: 1.0,
        };
        apply(&cmd, &mut stack);
        hist.push_executed(cmd);
        assert_eq!(stack.find(id).unwrap().common.opacity, 0.5);
        hist.undo(&mut stack);
        assert_eq!(stack.find(id).unwrap().common.opacity, 1.0);
        hist.redo(&mut stack);
        assert_eq!(stack.find(id).unwrap().common.opacity, 0.5);
    }

    #[test]
    fn sculpt_gesture_undo_redo_round_trip() {
        use crate::authoring::{SculptPoint, SculptStroke, SculptStrokeKind, SculptStrokeParams};

        let pre = SculptStroke {
            kind: SculptStrokeKind::Raise,
            points: vec![SculptPoint::default()],
            ..Default::default()
        };
        let mut stack = LayerStack::new();
        let layer = Layer::new(
            "Sculpt",
            LayerKind::SculptStrokes(SculptStrokeParams {
                strokes: vec![pre.clone()],
                ..Default::default()
            }),
        );
        let id = layer.id();
        stack.push(layer);

        // Gesture: extend the pre-existing stroke and add one new stroke.
        let ext = SculptPoint {
            u: 0.5,
            v: 0.5,
            pressure: 1.0,
        };
        let added = SculptStroke {
            kind: SculptStrokeKind::Smooth,
            points: vec![SculptPoint::default(); 3],
            ..Default::default()
        };
        let cmd = EditorCommand::SculptGesture {
            id,
            base_strokes: 1,
            base_last_points: 1,
            last_extension: vec![ext],
            added_strokes: vec![added],
        };
        apply(&cmd, &mut stack);
        let strokes = |stack: &LayerStack| match &stack.find(id).unwrap().kind {
            LayerKind::SculptStrokes(p) => p.strokes.clone(),
            _ => unreachable!(),
        };
        assert_eq!(strokes(&stack).len(), 2);
        assert_eq!(strokes(&stack)[0].points.len(), 2);

        let mut hist = CommandHistory::new(32);
        hist.push_executed(cmd);
        hist.undo(&mut stack);
        assert_eq!(strokes(&stack).len(), 1);
        assert_eq!(strokes(&stack)[0].points.len(), 1);
        hist.redo(&mut stack);
        assert_eq!(strokes(&stack).len(), 2);
        assert_eq!(strokes(&stack)[0].points.len(), 2);
    }

    #[test]
    fn insert_node_undo_removes_layer() {
        let mut stack = LayerStack::new();
        let group = LayerGroup::new("G");
        let gid = group.id;
        stack.nodes.push(StackNode::Group(group));
        let layer = Layer::new("A", LayerKind::Flat(FlatParams { height: 1.0 }));
        let id = layer.id();

        let cmd = EditorCommand::InsertNode {
            node: StackNode::Layer(layer),
            parent: Some(gid),
            index: 0,
        };
        apply(&cmd, &mut stack);
        assert_eq!(stack.sibling_location(id), Some((Some(gid), 0)));
        let mut hist = CommandHistory::new(32);
        hist.push_executed(cmd);
        hist.undo(&mut stack);
        assert!(stack.sibling_location(id).is_none());
        hist.redo(&mut stack);
        assert_eq!(stack.sibling_location(id), Some((Some(gid), 0)));
    }

    #[test]
    fn move_node_undo_redo_round_trip() {
        let mut stack = LayerStack::new();
        let group = LayerGroup::new("G");
        let gid = group.id;
        stack.nodes.push(StackNode::Group(group));
        let layer = Layer::new("A", LayerKind::Flat(FlatParams { height: 1.0 }));
        let id = layer.id();
        stack.push(layer);
        assert_eq!(stack.sibling_location(id), Some((None, 1)));

        // Simulate an already-performed "move into group" and record it.
        let (from_parent, from_index) = stack.sibling_location(id).unwrap();
        assert!(stack.move_into_group(id, gid));
        let (to_parent, to_index) = stack.sibling_location(id).unwrap();
        let mut hist = CommandHistory::new(32);
        hist.push_executed(EditorCommand::MoveNode {
            id,
            from_parent,
            from_index,
            to_parent,
            to_index,
        });
        assert_eq!(stack.sibling_location(id), Some((Some(gid), 0)));

        hist.undo(&mut stack);
        assert_eq!(stack.sibling_location(id), Some((None, 1)));
        hist.redo(&mut stack);
        assert_eq!(stack.sibling_location(id), Some((Some(gid), 0)));
    }

    #[test]
    fn coalesced_opacity_undo_restores_drag_start() {
        let mut stack = LayerStack::new();
        let layer = Layer::new("A", LayerKind::Flat(FlatParams { height: 1.0 }));
        let id = layer.id();
        stack.push(layer);
        let mut hist = CommandHistory::new(32);

        let first = EditorCommand::SetOpacity {
            id,
            opacity: 0.8,
            previous: 1.0,
        };
        apply(&first, &mut stack);
        hist.push_coalesced(first, Some((1, "opacity")));
        let second = EditorCommand::SetOpacity {
            id,
            opacity: 0.4,
            previous: 0.8,
        };
        apply(&second, &mut stack);
        hist.push_coalesced(second, Some((1, "opacity")));

        hist.undo(&mut stack);
        assert_eq!(stack.find(id).unwrap().common.opacity, 1.0);
    }
}
