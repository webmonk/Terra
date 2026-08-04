use super::{Layer, LayerId};
use serde::{Deserialize, Serialize};

/// Nested group or single layer in the stack (bottom → top order in Vec).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StackNode {
    Layer(Layer),
    Group(LayerGroup),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerGroup {
    pub id: LayerId,
    pub name: String,
    pub enabled: bool,
    pub children: Vec<StackNode>,
}

impl LayerGroup {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: LayerId::new(),
            name: name.into(),
            enabled: true,
            children: Vec::new(),
        }
    }
}

/// Root ordered layer stack (index 0 = bottom).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LayerStack {
    pub nodes: Vec<StackNode>,
}

impl LayerStack {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, layer: Layer) {
        self.nodes.push(StackNode::Layer(layer));
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Flatten to layers in bottom→top evaluation order, skipping disabled groups.
    pub fn flatten_layers(&self) -> Vec<&Layer> {
        let mut out = Vec::new();
        fn walk<'a>(nodes: &'a [StackNode], out: &mut Vec<&'a Layer>) {
            for n in nodes {
                match n {
                    StackNode::Layer(l) => out.push(l),
                    StackNode::Group(g) if g.enabled => walk(&g.children, out),
                    StackNode::Group(_) => {}
                }
            }
        }
        walk(&self.nodes, &mut out);
        out
    }

    pub fn flatten_layers_mut(&mut self) -> Vec<&mut Layer> {
        let mut out = Vec::new();
        fn walk<'a>(nodes: &'a mut [StackNode], out: &mut Vec<&'a mut Layer>) {
            for n in nodes {
                match n {
                    StackNode::Layer(l) => out.push(l),
                    StackNode::Group(g) if g.enabled => walk(&mut g.children, out),
                    StackNode::Group(_) => {}
                }
            }
        }
        walk(&mut self.nodes, &mut out);
        out
    }

    pub fn find(&self, id: LayerId) -> Option<&Layer> {
        self.flatten_layers().into_iter().find(|l| l.id() == id)
    }

    pub fn find_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        self.flatten_layers_mut().into_iter().find(|l| l.id() == id)
    }

    pub fn index_of(&self, id: LayerId) -> Option<usize> {
        self.nodes.iter().position(|n| match n {
            StackNode::Layer(l) => l.id() == id,
            StackNode::Group(g) => g.id == id,
        })
    }

    pub fn remove(&mut self, id: LayerId) -> Option<StackNode> {
        let idx = self.index_of(id)?;
        Some(self.nodes.remove(idx))
    }

    pub fn reorder(&mut self, from: usize, to: usize) {
        if from >= self.nodes.len() || to >= self.nodes.len() || from == to {
            return;
        }
        let node = self.nodes.remove(from);
        self.nodes.insert(to, node);
    }

    pub fn duplicate(&mut self, id: LayerId) -> Option<LayerId> {
        let idx = self.index_of(id)?;
        let dup = match &self.nodes[idx] {
            StackNode::Layer(l) => StackNode::Layer(l.duplicate()),
            StackNode::Group(g) => {
                let mut ng = g.clone();
                ng.id = LayerId::new();
                ng.name = format!("{} Copy", g.name);
                StackNode::Group(ng)
            }
        };
        let new_id = match &dup {
            StackNode::Layer(l) => l.id(),
            StackNode::Group(g) => g.id,
        };
        self.nodes.insert(idx + 1, dup);
        Some(new_id)
    }

    /// Layer ids in flatten order from bottom to top.
    pub fn layer_ids(&self) -> Vec<LayerId> {
        self.flatten_layers().iter().map(|l| l.id()).collect()
    }

    /// Index in flatten order.
    pub fn flatten_index(&self, id: LayerId) -> Option<usize> {
        self.layer_ids().iter().position(|&x| x == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::{FlatParams, LayerKind};

    #[test]
    fn reorder_and_duplicate() {
        let mut stack = LayerStack::new();
        let a = Layer::new("A", LayerKind::Flat(FlatParams { height: 1.0 }));
        let b = Layer::new("B", LayerKind::Flat(FlatParams { height: 2.0 }));
        let id_a = a.id();
        let id_b = b.id();
        stack.push(a);
        stack.push(b);
        stack.reorder(0, 1);
        assert_eq!(stack.layer_ids()[0], id_b);
        assert_eq!(stack.layer_ids()[1], id_a);
        let dup = stack.duplicate(id_b).unwrap();
        assert_ne!(dup, id_b);
        assert_eq!(stack.len(), 3);
    }
}
