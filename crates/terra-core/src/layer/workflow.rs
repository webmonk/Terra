//! Artist-facing workflow stages (metadata). Does not force UI folder layout.

use super::group_mode::StackCategory;
use super::operation::OperationCategory;
use crate::landscape_blueprint::EvalStage;
use serde::{Deserialize, Serialize};

/// High-level artist workflow stage for layer type metadata.
///
/// Distinct from [`EvalStage`] (pipeline ordering) and [`StackCategory`]
/// (WC folder roles). Phase 1 uses this for discovery / inspector hints only -
/// the visible layer tree is **not** regrouped by stage yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkflowStage {
    Foundation,
    Generation,
    Simulation,
    Masks,
    BiomePlacement,
    Materials,
    Scatter,
    Objects,
    Output,
}

impl WorkflowStage {
    pub const ALL: [WorkflowStage; 9] = [
        WorkflowStage::Foundation,
        WorkflowStage::Generation,
        WorkflowStage::Simulation,
        WorkflowStage::Masks,
        WorkflowStage::BiomePlacement,
        WorkflowStage::Materials,
        WorkflowStage::Scatter,
        WorkflowStage::Objects,
        WorkflowStage::Output,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WorkflowStage::Foundation => "Foundation",
            // Artist language: Shape Layers (internal stage id remains Generation).
            WorkflowStage::Generation => "Shape",
            WorkflowStage::Simulation => "Simulation",
            WorkflowStage::Masks => "Masks",
            WorkflowStage::BiomePlacement => "Biome Placement",
            WorkflowStage::Materials => "Materials",
            WorkflowStage::Scatter => "Scatter",
            WorkflowStage::Objects => "Objects",
            WorkflowStage::Output => "Output",
        }
    }

    /// Preferred metadata stage for an evaluation pipeline stage.
    pub fn from_eval_stage(stage: EvalStage) -> Self {
        match stage {
            EvalStage::Blueprint => WorkflowStage::Foundation,
            EvalStage::PreBiomeFields => WorkflowStage::Generation,
            EvalStage::Climate | EvalStage::BiomePlacement | EvalStage::FieldBlend => {
                WorkflowStage::BiomePlacement
            }
            EvalStage::SharedHydro => WorkflowStage::Simulation,
            EvalStage::Materials => WorkflowStage::Materials,
            EvalStage::Vegetation => WorkflowStage::Scatter,
            EvalStage::FineDetail => WorkflowStage::Generation,
            EvalStage::Export => WorkflowStage::Output,
        }
    }

    /// Rough mapping from WC category folders.
    pub fn from_stack_category(cat: StackCategory) -> Self {
        match cat {
            StackCategory::Foundation => WorkflowStage::Foundation,
            StackCategory::Shape => WorkflowStage::Generation,
            StackCategory::Simulation => WorkflowStage::Simulation,
            StackCategory::Mask => WorkflowStage::Masks,
            StackCategory::Surface => WorkflowStage::BiomePlacement,
        }
    }

    /// Rough mapping from operation category (used when kind-specific override absent).
    pub fn from_operation(cat: OperationCategory) -> Self {
        match cat {
            OperationCategory::Generator | OperationCategory::Modifier => WorkflowStage::Generation,
            OperationCategory::Simulation | OperationCategory::Analysis => {
                WorkflowStage::Simulation
            }
            OperationCategory::Surface => WorkflowStage::Materials,
            OperationCategory::ImportedData => WorkflowStage::Foundation,
            OperationCategory::Group | OperationCategory::Organisation => WorkflowStage::Generation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eval_stage_maps_cover_all() {
        for stage in EvalStage::ALL {
            let _ = WorkflowStage::from_eval_stage(stage);
        }
    }

    #[test]
    fn labels_nonempty() {
        for s in WorkflowStage::ALL {
            assert!(!s.label().is_empty());
        }
    }
}
