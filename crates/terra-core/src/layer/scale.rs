//! Scale bands for multi-resolution evaluation (Phase 11 Rule 3).
//!
//! Lives under `layer` so [`LayerKind::scale_band`] can declare ownership without
//! depending on `terrain_eval`.

use serde::{Deserialize, Serialize};

/// Physical frequency band an operator prefers to run at.
///
/// These are not alternate algorithms — the same operator family should converge
/// across Interactive / Preview / Final. Bands guide which pyramid level or
/// sample spacing is appropriate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum ScaleBand {
    /// Continental / topographic structure, uplift, large drainage basins.
    Macro,
    /// Ridges, valleys, river networks, cliffs, primary erosion patterns.
    #[default]
    Meso,
    /// Gullies, small channels, talus, rock breakup, fine erosion.
    Micro,
    /// Cascades Macro→Meso→Micro while preserving longer wavelengths.
    MultiScale,
}

impl ScaleBand {
    /// Typical preferred sample spacing in metres (order-of-magnitude hint).
    pub fn preferred_sample_spacing_m(self) -> f32 {
        match self {
            ScaleBand::Macro => 32.0,
            ScaleBand::Meso => 8.0,
            ScaleBand::Micro => 2.0,
            // Evaluate at meso spacing; internal cascade owns finer bands.
            ScaleBand::MultiScale => 8.0,
        }
    }

    /// Wavelength range this band roughly owns (metres).
    pub fn wavelength_range_m(self) -> (f32, f32) {
        match self {
            ScaleBand::Macro => (256.0, f32::INFINITY),
            ScaleBand::Meso => (16.0, 512.0),
            ScaleBand::Micro => (0.0, 32.0),
            ScaleBand::MultiScale => (0.0, f32::INFINITY),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ScaleBand::Macro => "Macro",
            ScaleBand::Meso => "Meso",
            ScaleBand::Micro => "Micro",
            ScaleBand::MultiScale => "Multi-Scale",
        }
    }

    /// True when this band must not rewrite wavelengths longer than the macro lock.
    pub fn respects_macro_silhouette(self) -> bool {
        matches!(self, ScaleBand::Micro | ScaleBand::MultiScale)
    }
}
