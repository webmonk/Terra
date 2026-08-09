//! Terrain Recipe — linear execution-order view of the single terrain stack.

use crate::layer::{LayerId, LayerStack, StackNode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecipeItemKind {
    GlobalSection,
    FlowArrow,
    Layer,
    Group,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecipeItem {
    pub kind: RecipeItemKind,
    pub name: String,
    pub layer_id: Option<LayerId>,
    pub root_idx: Option<usize>,
    pub enabled: bool,
    pub cached: bool,
    pub priority: Option<i32>,
    pub solo: bool,
    pub depth: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum RecipeRebuildStatus {
    #[default]
    Idle,
    Pending,
    Building,
    Ready,
    Disabled,
}

impl RecipeItem {
    pub fn rebuild_status(&self, world_building: bool) -> RecipeRebuildStatus {
        if matches!(
            self.kind,
            RecipeItemKind::GlobalSection | RecipeItemKind::FlowArrow
        ) {
            return if world_building {
                RecipeRebuildStatus::Building
            } else {
                RecipeRebuildStatus::Idle
            };
        }
        if !self.enabled {
            return RecipeRebuildStatus::Disabled;
        }
        if world_building {
            return RecipeRebuildStatus::Building;
        }
        if self.cached {
            RecipeRebuildStatus::Ready
        } else {
            RecipeRebuildStatus::Pending
        }
    }
}

/// Build the Terrain Recipe from the document stack (execution order).
pub fn build_terrain_recipe_from_stack(stack: &LayerStack) -> Vec<RecipeItem> {
    let mut out = Vec::new();
    out.push(RecipeItem {
        kind: RecipeItemKind::GlobalSection,
        name: "TERRAIN".into(),
        layer_id: None,
        root_idx: None,
        enabled: true,
        cached: false,
        priority: None,
        solo: false,
        depth: 0,
    });
    emit_stack_recipe(stack, 1, &mut out);
    out
}

fn emit_stack_recipe(stack: &LayerStack, depth: u8, out: &mut Vec<RecipeItem>) {
    for (root_idx, node) in stack.nodes.iter().enumerate() {
        walk_node(node, depth, root_idx, out);
    }
}

fn walk_node(node: &StackNode, depth: u8, root_idx: usize, out: &mut Vec<RecipeItem>) {
    match node {
        StackNode::Layer(layer) => {
            out.push(RecipeItem {
                kind: RecipeItemKind::Layer,
                name: layer.common.name.clone(),
                layer_id: Some(layer.id()),
                root_idx: Some(root_idx),
                enabled: layer.common.enabled,
                cached: layer.common.cached,
                priority: None,
                solo: layer.common.solo,
                depth,
            });
        }
        StackNode::Group(g) => {
            // Category folders are transparent.
            if matches!(g.group_kind, crate::layer::GroupKind::CategoryFolder) {
                for (i, child) in g.children.iter().enumerate() {
                    walk_node(child, depth, i, out);
                }
                return;
            }
            out.push(RecipeItem {
                kind: RecipeItemKind::Group,
                name: g.name.clone(),
                layer_id: Some(g.id),
                root_idx: Some(root_idx),
                enabled: g.enabled,
                cached: false,
                priority: None,
                solo: false,
                depth,
            });
            for (i, child) in g.children.iter().enumerate() {
                walk_node(child, depth.saturating_add(1), i, out);
            }
        }
    }
}

/// Whether a recipe still matches the stack structure (lightweight fingerprint).
pub fn recipe_matches_stack(recipe: &[RecipeItem], stack: &LayerStack) -> bool {
    let live = build_terrain_recipe_from_stack(stack);
    if recipe.len() != live.len() {
        return false;
    }
    recipe
        .iter()
        .zip(live.iter())
        .all(|(a, b)| a.kind == b.kind && a.layer_id == b.layer_id && a.name == b.name)
}
