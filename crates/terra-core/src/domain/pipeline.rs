//! Explicit terrain evaluation pipeline stages.
//!
//! # Independence from UI workspace
//!
//! Stage order here is the **evaluation** contract. It does not follow
//! authoring chronology or UI workspace mode order. Artists may create
//! Materials before Shape layers; evaluation still runs Shape before Material
//! when per-stage execution is selective. Today
//! [`TerrainPipelineExecutor::evaluate_stack`] records all stages then delegates to
//! [`StackEvaluator::rebuild_all`].

use crate::eval::{EvalContext, EvalError, StackEvaluator};
use crate::heightfield::Heightfield;
use serde::{Deserialize, Serialize};

/// Ordered execution stages for the terrain stack (and its biomes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[repr(u8)]
pub enum TerrainPipelineStage {
    Shape = 0,
    BiomeMask = 1,
    BiomeTerrain = 2,
    BiomeSimulation = 3,
    Composite = 4,
    Simulation = 5,
    Material = 6,
    Scatter = 7,
    Objects = 8,
    Output = 9,
}

impl TerrainPipelineStage {
    pub const ALL: [TerrainPipelineStage; 10] = [
        TerrainPipelineStage::Shape,
        TerrainPipelineStage::BiomeMask,
        TerrainPipelineStage::BiomeTerrain,
        TerrainPipelineStage::BiomeSimulation,
        TerrainPipelineStage::Composite,
        TerrainPipelineStage::Simulation,
        TerrainPipelineStage::Material,
        TerrainPipelineStage::Scatter,
        TerrainPipelineStage::Objects,
        TerrainPipelineStage::Output,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TerrainPipelineStage::Shape => "Shape",
            TerrainPipelineStage::BiomeMask => "BiomeMask",
            TerrainPipelineStage::BiomeTerrain => "BiomeTerrain",
            TerrainPipelineStage::BiomeSimulation => "BiomeSimulation",
            TerrainPipelineStage::Composite => "Composite",
            TerrainPipelineStage::Simulation => "Simulation",
            TerrainPipelineStage::Material => "Material",
            TerrainPipelineStage::Scatter => "Scatter",
            TerrainPipelineStage::Objects => "Objects",
            TerrainPipelineStage::Output => "Output",
        }
    }

    /// Deterministic full-pipeline order.
    pub fn execution_order() -> &'static [TerrainPipelineStage] {
        &Self::ALL
    }
}

/// Why a stage is rebuilding (diagnostics / “Why did this rebuild?”).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RebuildReason {
    pub stage: TerrainPipelineStage,
    pub cause: String,
}

/// Run the terrain pipeline against a single stack.
pub struct TerrainPipelineExecutor {
    pub evaluator: StackEvaluator,
    pub last_rebuild_reasons: Vec<RebuildReason>,
}

impl Default for TerrainPipelineExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl TerrainPipelineExecutor {
    pub fn new() -> Self {
        Self {
            evaluator: StackEvaluator::new(),
            last_rebuild_reasons: Vec::new(),
        }
    }

    /// Evaluate a WC terrain stack.
    pub fn evaluate_stack(
        &mut self,
        stack: &crate::layer::LayerStack,
        ctx: &mut EvalContext,
        reason: impl Into<String>,
    ) -> Result<Heightfield, EvalError> {
        let cause = reason.into();
        self.last_rebuild_reasons = TerrainPipelineStage::ALL
            .iter()
            .map(|&stage| RebuildReason {
                stage,
                cause: cause.clone(),
            })
            .collect();
        self.evaluator.rebuild_all(stack, ctx)
    }

    pub fn rebuild_reasons(&self) -> &[RebuildReason] {
        &self.last_rebuild_reasons
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_are_ordered() {
        let order = TerrainPipelineStage::execution_order();
        assert_eq!(order.first(), Some(&TerrainPipelineStage::Shape));
        assert_eq!(order.last(), Some(&TerrainPipelineStage::Output));
        for w in order.windows(2) {
            assert!(w[0] < w[1]);
        }
    }
}
