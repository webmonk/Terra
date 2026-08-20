//! Domain roles for the Shape / Biome hierarchy.

use serde::{Deserialize, Serialize};

/// Artist-facing role of a layer in the hierarchy.
///
/// Distinct from [`crate::layer::WorkflowStage`] and [`crate::landscape_blueprint::EvalStage`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DomainRole {
    /// Foundation height (Shape Layer pipeline).
    ShapeLayer,
    /// Biome mask stack contribution (DistNode / paint / procedural).
    MaskLayer,
    /// Biome-local height character (mountains, filters, ...).
    TerrainFilter,
    /// Expensive iterative sim (biome-local or terrain-level).
    SimulationLayer,
    /// Appearance only (not height), unless geometry displacement.
    MaterialLayer,
    /// Density / instance scatter.
    ScatterLayer,
    /// Deliberate object placement (landmarks, layouts, ...).
    ObjectLayer,
    /// Still loads and evaluates, but is not a first-class hierarchy citizen yet.
    CompatibilityLegacy,
}

impl DomainRole {
    pub const ALL: [DomainRole; 8] = [
        DomainRole::ShapeLayer,
        DomainRole::MaskLayer,
        DomainRole::TerrainFilter,
        DomainRole::SimulationLayer,
        DomainRole::MaterialLayer,
        DomainRole::ScatterLayer,
        DomainRole::ObjectLayer,
        DomainRole::CompatibilityLegacy,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DomainRole::ShapeLayer => "Shape Layer",
            DomainRole::MaskLayer => "Mask Layer",
            DomainRole::TerrainFilter => "Terrain Filter",
            DomainRole::SimulationLayer => "Simulation Layer",
            DomainRole::MaterialLayer => "Material Layer",
            DomainRole::ScatterLayer => "Scatter Layer",
            DomainRole::ObjectLayer => "Object Layer",
            DomainRole::CompatibilityLegacy => "Compatibility (Legacy)",
        }
    }

    /// Preferred structural parent for Add / DnD validation.
    pub fn allowed_parents(self) -> &'static [DomainParent] {
        match self {
            DomainRole::ShapeLayer => &[DomainParent::TerrainShape],
            DomainRole::MaskLayer => &[DomainParent::BiomeMasks],
            DomainRole::TerrainFilter => &[DomainParent::BiomeFilters],
            DomainRole::SimulationLayer => &[
                DomainParent::BiomeSimulations,
                DomainParent::TerrainSimulations,
            ],
            DomainRole::MaterialLayer => {
                &[DomainParent::BiomeMaterials, DomainParent::TerrainMaterials]
            }
            DomainRole::ScatterLayer => &[DomainParent::BiomeScatter],
            DomainRole::ObjectLayer => &[DomainParent::BiomeObjects],
            DomainRole::CompatibilityLegacy => &[
                DomainParent::TerrainShape,
                DomainParent::BiomeFilters,
                DomainParent::BiomeSimulations,
                DomainParent::TerrainSimulations,
            ],
        }
    }
}

/// Structural parent slot inside the terrain / biome hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DomainParent {
    TerrainShape,
    BiomeMasks,
    BiomeFilters,
    BiomeSimulations,
    BiomeMaterials,
    BiomeScatter,
    BiomeObjects,
    TerrainSimulations,
    TerrainMaterials,
}

impl DomainParent {
    pub fn label(self) -> &'static str {
        match self {
            DomainParent::TerrainShape => "Shape Layers",
            DomainParent::BiomeMasks => "Mask Layers",
            DomainParent::BiomeFilters => "Terrain Filters",
            DomainParent::BiomeSimulations => "Biome Simulations",
            DomainParent::BiomeMaterials => "Materials",
            DomainParent::BiomeScatter => "Scatter",
            DomainParent::BiomeObjects => "Objects",
            DomainParent::TerrainSimulations => "Simulations",
            DomainParent::TerrainMaterials => "Materials",
        }
    }
}
