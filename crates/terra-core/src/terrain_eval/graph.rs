//! Compile the artist layer stack into an internal evaluation graph.

use crate::fields::FieldId;
use crate::layer::{LayerId, LayerStack, StackNode};
use std::collections::HashSet;

use super::operator::{LayerOperatorAdapter, OperatorDescriptor, OperatorId};

/// Graph node id (index into [`EvalGraph::nodes`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EvalNodeId(pub usize);

/// One node in the compiled evaluation graph.
#[derive(Debug, Clone)]
pub struct EvalGraphNode {
    pub id: EvalNodeId,
    pub operator: OperatorDescriptor,
    /// Nodes that must run before this one (stack predecessors + field producers).
    pub dependencies: Vec<EvalNodeId>,
}

/// Internal evaluation graph. Not exposed as a node editor.
#[derive(Debug, Clone, Default)]
pub struct EvalGraph {
    pub nodes: Vec<EvalGraphNode>,
    /// Derived analysis operators inserted when any layer needs them.
    pub derived: Vec<OperatorDescriptor>,
}

impl EvalGraph {
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Topological order equals stack order for Phase 1 (field edges refine later).
    pub fn execution_order(&self) -> Vec<EvalNodeId> {
        self.nodes.iter().map(|n| n.id).collect()
    }

    pub fn find_layer(&self, layer: LayerId) -> Option<&EvalGraphNode> {
        self.nodes
            .iter()
            .find(|n| matches!(&n.operator.id, OperatorId::Layer(id) if *id == layer))
    }

    /// Operators that write height, in execution order.
    pub fn height_pipeline(&self) -> Vec<&EvalGraphNode> {
        self.nodes
            .iter()
            .filter(|n| n.operator.modifies_height)
            .collect()
    }
}

/// Compile a layer stack into an evaluation graph.
///
/// Preserves bottom→top stack order. Inserts derived-field operator descriptors
/// for analysis fields referenced by any layer. Does not execute anything.
pub fn compile_eval_graph(stack: &LayerStack, world_seed: u64) -> EvalGraph {
    let mut graph = EvalGraph::default();
    let mut prior: Option<EvalNodeId> = None;
    let mut needed_derived: HashSet<FieldId> = HashSet::new();

    walk_nodes(
        &stack.nodes,
        &mut graph,
        &mut prior,
        world_seed,
        &mut needed_derived,
    );

    // Shared analysis ops available to every consumer via DerivedFieldCache.
    needed_derived.insert(FieldId::Slope);
    needed_derived.insert(FieldId::Curvature);
    needed_derived.insert(FieldId::Gradient);

    let mut derived_list: Vec<FieldId> = needed_derived.into_iter().collect();
    derived_list.sort_by_key(|f| f.cache_key());
    for field in derived_list {
        let reads = match field {
            FieldId::DistanceToChannel => vec![FieldId::Height, FieldId::ChannelMask],
            _ => vec![FieldId::Height],
        };
        graph.derived.push(OperatorDescriptor::derived(field, reads));
    }

    graph
}

fn walk_nodes(
    nodes: &[StackNode],
    graph: &mut EvalGraph,
    prior: &mut Option<EvalNodeId>,
    world_seed: u64,
    needed_derived: &mut HashSet<FieldId>,
) {
    for node in nodes {
        match node {
            StackNode::Layer(layer) => {
                if !layer.common.enabled {
                    continue;
                }
                let adapter = LayerOperatorAdapter::new(layer, world_seed);
                let desc = adapter.descriptor.clone();
                for f in desc.all_inputs() {
                    if f.is_derived_analysis() {
                        needed_derived.insert(f);
                    }
                }
                let id = EvalNodeId(graph.nodes.len());
                let mut deps = Vec::new();
                if let Some(p) = *prior {
                    deps.push(p);
                }
                graph.nodes.push(EvalGraphNode {
                    id,
                    operator: desc,
                    dependencies: deps,
                });
                *prior = Some(id);
            }
            StackNode::Group(group) => {
                if !group.enabled {
                    continue;
                }
                walk_nodes(
                    &group.children,
                    graph,
                    prior,
                    world_seed,
                    needed_derived,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::{FlatParams, HydraulicErosionParams, Layer, LayerKind, LayerStack};

    #[test]
    fn compile_preserves_stack_order() {
        let mut stack = LayerStack::default();
        let a = Layer::new("Flat", LayerKind::Flat(FlatParams::default()));
        let b = Layer::new(
            "Hydro",
            LayerKind::HydraulicErosion(HydraulicErosionParams::default()),
        );
        let id_a = a.id();
        let id_b = b.id();
        stack.nodes.push(StackNode::Layer(a));
        stack.nodes.push(StackNode::Layer(b));

        let g = compile_eval_graph(&stack, 7);
        assert_eq!(g.len(), 2);
        assert!(matches!(g.nodes[0].operator.id, OperatorId::Layer(id) if id == id_a));
        assert!(matches!(g.nodes[1].operator.id, OperatorId::Layer(id) if id == id_b));
        assert!(!g.derived.is_empty());
        assert!(g.nodes[1].operator.modifies_height);
        assert_eq!(g.nodes[1].dependencies, vec![EvalNodeId(0)]);
    }
}
