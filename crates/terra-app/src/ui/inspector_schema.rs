//! Stable section identifiers and labels for the layer inspector.

use terra_gui::Icon;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InspectorSection {
    Header,
    #[default]
    General,
    Shape,
    /// Legacy key `"details"` - unused as a primary tab.
    Details,
    /// Filter / erosion / modifier layers (WC Filters).
    Erosion,
    /// Legacy key `"output"` - prefer [`Self::Materials`] / [`Self::Objects`] / [`Self::Biome`].
    Output,
    Distribution,
    Noise,
    Advanced,
    Performance,
    Presets,
    Materials,
    Objects,
    /// Biome / climate layer primary pane.
    Biome,
    /// Local Simulations (WC).
    Simulation,
}

impl InspectorSection {
    pub const ALL: [Self; 15] = [
        Self::Header,
        Self::General,
        Self::Shape,
        Self::Details,
        Self::Erosion,
        Self::Output,
        Self::Distribution,
        Self::Noise,
        Self::Advanced,
        Self::Performance,
        Self::Presets,
        Self::Materials,
        Self::Objects,
        Self::Biome,
        Self::Simulation,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::Header => 0,
            Self::General => 1,
            Self::Shape => 2,
            Self::Details => 3,
            Self::Erosion => 4,
            Self::Output => 5,
            Self::Distribution => 6,
            Self::Noise => 7,
            Self::Advanced => 8,
            Self::Performance => 9,
            Self::Presets => 10,
            Self::Materials => 11,
            Self::Objects => 12,
            Self::Biome => 13,
            Self::Simulation => 14,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Header => "Header",
            Self::General => "Layer",
            Self::Shape => "Shape",
            Self::Details => "Details",
            Self::Erosion => "Filters",
            Self::Output => "Materials",
            Self::Distribution => "Distributions",
            Self::Noise => "Noise",
            Self::Advanced => "Advanced",
            Self::Performance => "Performance",
            Self::Presets => "Presets",
            Self::Materials => "Materials",
            Self::Objects => "Objects",
            Self::Biome => "Biome",
            Self::Simulation => "Local Sims",
        }
    }

    /// Lucide icon for the inspector section tab strip.
    pub const fn icon(self) -> Icon {
        match self {
            Self::Header => Icon::Settings2,
            Self::General => Icon::Settings2,
            Self::Shape => Icon::Mountain,
            Self::Details => Icon::Waves,
            Self::Erosion => Icon::Sparkles,
            Self::Output | Self::Materials => Icon::Grid3x3,
            Self::Distribution => Icon::CircleDot,
            Self::Noise => Icon::Sparkles,
            Self::Advanced => Icon::SlidersHorizontal,
            Self::Performance => Icon::Gauge,
            Self::Presets => Icon::Bookmark,
            Self::Objects => Icon::Package,
            Self::Biome => Icon::Layers,
            Self::Simulation => Icon::Droplets,
        }
    }
}
