//! Stable section identifiers and labels for the layer inspector.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorSection {
    Header,
    General,
    Shape,
    Details,
    Erosion,
    Output,
    Advanced,
    Performance,
    Presets,
}

impl InspectorSection {
    pub const ALL: [Self; 9] = [
        Self::Header,
        Self::General,
        Self::Shape,
        Self::Details,
        Self::Erosion,
        Self::Output,
        Self::Advanced,
        Self::Performance,
        Self::Presets,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::Header => 0,
            Self::General => 1,
            Self::Shape => 2,
            Self::Details => 3,
            Self::Erosion => 4,
            Self::Output => 5,
            Self::Advanced => 6,
            Self::Performance => 7,
            Self::Presets => 8,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Header => "Header",
            Self::General => "General",
            Self::Shape => "Shape",
            Self::Details => "Details",
            Self::Erosion => "Erosion",
            Self::Output => "Output",
            Self::Advanced => "Advanced",
            Self::Performance => "Performance",
            Self::Presets => "Presets",
        }
    }
}
