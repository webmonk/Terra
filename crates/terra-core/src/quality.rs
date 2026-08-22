//! Preview quality ladder, a leaf with no crate-internal dependencies.
//!
//! `PreviewQuality` is a plain four-variant enum (Draft/Medium/Full/Export)
//! whose resolution and refine helpers are pure arithmetic, so it lives in its
//! own leaf module rather than in `eval`. `eval` re-exports it, keeping
//! `terra_core::eval::PreviewQuality` and every serialized document valid, while
//! `analyze` names it directly from here instead of importing `eval` from below.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PreviewQuality {
    Draft,  // 256
    Medium, // 512
    #[default]
    Full, // project preview res
    Export, // export res
}

impl PreviewQuality {
    pub fn resolution(self, full_preview: u32, export: u32) -> u32 {
        let full = full_preview.max(128);
        match self {
            // Interactive ladder aims closer to World Creator preview density.
            // Draft/Medium used to hard-cap at 256/512, which kept the viewport
            // looking coarse even after refine started.
            PreviewQuality::Draft => 512.min(full),
            PreviewQuality::Medium => 1024.min(full),
            PreviewQuality::Full => full,
            PreviewQuality::Export => export.max(full),
        }
    }

    /// True for the interactive rungs that refine into something better.
    ///
    /// Sims may take coarse-to-fine shortcuts here that `Full` and `Export`
    /// must not, so what gets exported stays exactly what the solver produces.
    pub fn is_preview(self) -> bool {
        matches!(self, PreviewQuality::Draft | PreviewQuality::Medium)
    }

    pub fn next_refine(self) -> Option<Self> {
        match self {
            PreviewQuality::Draft => Some(PreviewQuality::Medium),
            PreviewQuality::Medium => Some(PreviewQuality::Full),
            PreviewQuality::Full | PreviewQuality::Export => None,
        }
    }
}
