//! Ordered layer stack (user-facing model). No node-graph UI.

mod blend;
mod kinds;
mod stack;

pub use blend::{blend_heights, BlendMode};
pub use kinds::*;
pub use stack::{LayerGroup, LayerStack, StackNode};

use crate::mask::MaskRef;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable layer identity for undo/reorder/caching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LayerId(pub Uuid);

impl LayerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for LayerId {
    fn default() -> Self {
        Self::new()
    }
}

/// Parameters shared by every layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerCommon {
    pub id: LayerId,
    pub name: String,
    pub enabled: bool,
    /// Opacity in \[0, 1\].
    pub opacity: f32,
    pub blend: BlendMode,
    pub masks: Vec<MaskRef>,
    /// When true, parameters and paint are read-only.
    #[serde(default)]
    pub locked: bool,
    /// Solo this layer for preview (others dimmed / skipped in UI).
    #[serde(default)]
    pub solo: bool,
    /// Optional colour tag index (0 = none, 1–7 = palette).
    #[serde(default)]
    pub color_tag: u8,
    /// Intermediate output is cached / baked.
    #[serde(default)]
    pub cached: bool,
}

impl LayerCommon {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: LayerId::new(),
            name: name.into(),
            enabled: true,
            opacity: 1.0,
            blend: BlendMode::Normal,
            masks: Vec::new(),
            locked: false,
            solo: false,
            color_tag: 0,
            cached: false,
        }
    }
}

/// A single terrain layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layer {
    pub common: LayerCommon,
    pub kind: LayerKind,
}

impl Layer {
    pub fn new(name: impl Into<String>, kind: LayerKind) -> Self {
        let blend = kind.default_blend();
        let mut common = LayerCommon::new(name);
        common.blend = blend;
        Self { common, kind }
    }

    pub fn id(&self) -> LayerId {
        self.common.id
    }

    pub fn duplicate(&self) -> Self {
        let mut clone = self.clone();
        clone.common.id = LayerId::new();
        clone.common.name = format!("{} Copy", self.common.name);
        clone
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_gets_new_id() {
        let a = Layer::new("Base", LayerKind::Flat(FlatParams::default()));
        let b = a.duplicate();
        assert_ne!(a.id(), b.id());
        assert!(b.common.name.contains("Copy"));
    }

    #[test]
    fn generator_defaults_to_add() {
        let fbm = Layer::new("H", LayerKind::Fbm(FbmParams::default()));
        assert_eq!(fbm.common.blend, BlendMode::Add);
        let perlin = Layer::new("P", LayerKind::NoisePerlin(NoiseParams::default()));
        assert_eq!(perlin.common.blend, BlendMode::Add);
    }

    #[test]
    fn filter_defaults_to_normal() {
        let thermal = Layer::new(
            "T",
            LayerKind::ThermalErosion(ThermalErosionParams::default()),
        );
        assert_eq!(thermal.common.blend, BlendMode::Normal);
        let flat = Layer::new("B", LayerKind::Flat(FlatParams::default()));
        assert_eq!(flat.common.blend, BlendMode::Normal);
    }
}
