//! Evaluation quality modes (algorithm family stays constant across modes).

use crate::quality::PreviewQuality;
use serde::{Deserialize, Serialize};

/// Conceptual evaluation quality. Maps onto the existing progressive ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum EvalMode {
    /// Lower resolution, fewer iterations, approximate global fields.
    Interactive,
    /// Medium/high resolution with accurate structure.
    #[default]
    Preview,
    /// Export resolution and converged iteration settings.
    Final,
}

impl EvalMode {
    pub fn from_preview_quality(q: PreviewQuality) -> Self {
        match q {
            PreviewQuality::Draft => EvalMode::Interactive,
            PreviewQuality::Medium | PreviewQuality::Full => EvalMode::Preview,
            PreviewQuality::Export => EvalMode::Final,
        }
    }

    pub fn to_preview_quality(self) -> PreviewQuality {
        match self {
            EvalMode::Interactive => PreviewQuality::Draft,
            EvalMode::Preview => PreviewQuality::Full,
            EvalMode::Final => PreviewQuality::Export,
        }
    }

    /// Soft iteration budget multiplier relative to authored Full settings.
    pub fn iteration_scale(self) -> f32 {
        match self {
            EvalMode::Interactive => 0.25,
            EvalMode::Preview => 0.65,
            EvalMode::Final => 1.0,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            EvalMode::Interactive => "Interactive",
            EvalMode::Preview => "Preview",
            EvalMode::Final => "Final",
        }
    }
}
