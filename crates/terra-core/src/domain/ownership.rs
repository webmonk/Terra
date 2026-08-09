//! Ownership view over the single terrain stack for the domain hierarchy.

use super::classify::classify_in_context;
use super::role::{DomainParent, DomainRole};
use crate::layer::{BiomeSection, GroupKind, LayerId, LayerStack, StackCategory, StackNode};

/// A classified executable layer in the domain view.
#[derive(Debug, Clone)]
pub struct DomainLayerRef {
    pub id: LayerId,
    pub name: String,
    pub role: DomainRole,
    pub biome_id: Option<LayerId>,
    pub parent: DomainParent,
    pub enabled: bool,
}

/// Biome container summary.
#[derive(Debug, Clone)]
pub struct DomainBiomeRef {
    pub id: LayerId,
    pub name: String,
    pub is_base_terrain: bool,
    pub enabled: bool,
    pub execution_order: u32,
}

/// Read-only projection of a [`LayerStack`] into the Shape / Biome hierarchy.
#[derive(Debug, Clone, Default)]
pub struct DomainView {
    pub biomes: Vec<DomainBiomeRef>,
    pub layers: Vec<DomainLayerRef>,
}

impl DomainView {
    /// Build a domain view from the single WC terrain stack.
    pub fn from_stack(stack: &LayerStack) -> Self {
        let mut view = DomainView::default();
        let mut biome_order = 0u32;
        for node in &stack.nodes {
            walk(node, None, None, None, &mut biome_order, &mut view);
        }
        if view.biomes.iter().all(|b| !b.is_base_terrain) {
            if let Some(last) = view.biomes.last_mut() {
                last.is_base_terrain = true;
            }
        }
        view
    }

    pub fn biome(&self, id: LayerId) -> Option<&DomainBiomeRef> {
        self.biomes.iter().find(|b| b.id == id)
    }

    pub fn layers_for_parent(
        &self,
        parent: DomainParent,
        biome: Option<LayerId>,
    ) -> Vec<&DomainLayerRef> {
        self.layers
            .iter()
            .filter(|l| l.parent == parent && l.biome_id == biome)
            .collect()
    }

    /// Validate that `role` may live under `parent`.
    pub fn validate_ownership(role: DomainRole, parent: DomainParent) -> Result<(), String> {
        if role.allowed_parents().contains(&parent) {
            Ok(())
        } else {
            Err(format!(
                "{} cannot be parented under {} (allowed: {})",
                role.label(),
                parent.label(),
                role.allowed_parents()
                    .iter()
                    .map(|p| p.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        }
    }
}

fn walk(
    node: &StackNode,
    biome_id: Option<LayerId>,
    category: Option<StackCategory>,
    section: Option<BiomeSection>,
    biome_order: &mut u32,
    view: &mut DomainView,
) {
    match node {
        StackNode::Layer(layer) => {
            let role = classify_in_context(&layer.kind, category, section);
            let parent = parent_for(role, biome_id, category, section);
            view.layers.push(DomainLayerRef {
                id: layer.id(),
                name: layer.common.name.clone(),
                role,
                biome_id,
                parent,
                enabled: layer.common.enabled,
            });
        }
        StackNode::Group(g) => {
            let cat = g.category.or(category);
            if g.is_biome() {
                let order = *biome_order;
                *biome_order += 1;
                let is_base = g.name == "Global"
                    || g.name == "Default Biome"
                    || g.name == "Base Terrain"
                    || g.name.eq_ignore_ascii_case("base terrain");
                view.biomes.push(DomainBiomeRef {
                    id: g.id,
                    name: g.name.clone(),
                    is_base_terrain: is_base,
                    enabled: g.enabled,
                    execution_order: order,
                });
                for child in &g.children {
                    walk(child, Some(g.id), cat, section, biome_order, view);
                }
            } else {
                let next_section = match g.group_kind {
                    GroupKind::BiomeSection(s) => Some(s),
                    _ => section,
                };
                for child in &g.children {
                    walk(child, biome_id, cat, next_section, biome_order, view);
                }
            }
        }
    }
}

fn parent_for(
    role: DomainRole,
    biome_id: Option<LayerId>,
    category: Option<StackCategory>,
    section: Option<BiomeSection>,
) -> DomainParent {
    if biome_id.is_some() {
        if let Some(s) = section {
            return match s {
                BiomeSection::Filters => DomainParent::BiomeFilters,
                BiomeSection::Materials => DomainParent::BiomeMaterials,
                BiomeSection::Objects => match role {
                    DomainRole::ScatterLayer => DomainParent::BiomeScatter,
                    _ => DomainParent::BiomeObjects,
                },
                BiomeSection::LocalSims => DomainParent::BiomeSimulations,
            };
        }
        return match role {
            DomainRole::MaskLayer => DomainParent::BiomeMasks,
            DomainRole::MaterialLayer => DomainParent::BiomeMaterials,
            DomainRole::ScatterLayer => DomainParent::BiomeScatter,
            DomainRole::ObjectLayer => DomainParent::BiomeObjects,
            DomainRole::SimulationLayer => DomainParent::BiomeSimulations,
            _ => DomainParent::BiomeFilters,
        };
    }
    match category {
        Some(StackCategory::Simulation) => DomainParent::TerrainSimulations,
        Some(StackCategory::Surface) if matches!(role, DomainRole::MaterialLayer) => {
            DomainParent::TerrainMaterials
        }
        Some(StackCategory::Mask) => DomainParent::BiomeMasks,
        _ => DomainParent::TerrainShape,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::{FlatParams, Layer, LayerKind};

    #[test]
    fn ownership_rejects_shape_under_materials() {
        let err =
            DomainView::validate_ownership(DomainRole::ShapeLayer, DomainParent::BiomeMaterials);
        assert!(err.is_err());
    }

    #[test]
    fn view_lists_stack_layers() {
        let mut stack = LayerStack::new();
        stack.push(Layer::new(
            "Base",
            LayerKind::Flat(FlatParams { height: 1.0 }),
        ));
        let view = DomainView::from_stack(&stack);
        assert!(view.layers.iter().any(|l| l.name == "Base"));
    }
}
