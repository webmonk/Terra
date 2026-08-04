//! Serializable project document.

use crate::command::CommandHistory;
use crate::heightfield::HeightfieldMetrics;
use crate::layer::{Layer, LayerId, LayerKind, LayerStack, NoiseParams, SculptParams};
use crate::mask::{MaskAsset, MaskId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const DOCUMENT_VERSION: u32 = 2;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerrainDocument {
    pub version: u32,
    pub name: String,
    pub metrics: HeightfieldMetrics,
    pub preview_resolution: u32,
    pub export_resolution: u32,
    pub stack: LayerStack,
    pub masks: Vec<MaskAsset>,
    pub selected: Option<LayerId>,
    pub presets_used: Vec<String>,
}

impl Default for TerrainDocument {
    fn default() -> Self {
        Self::new_default()
    }
}

impl TerrainDocument {
    pub fn new_default() -> Self {
        let metrics = HeightfieldMetrics::preview_default();
        let mut stack = LayerStack::new();
        // Sculptable foundation — selected by default for raise/lower.
        let base = Layer::new(
            "Base",
            LayerKind::SculptBase(SculptParams::filled(512, 20.0)),
        );
        let base_id = base.id();
        stack.push(base);
        // Light detail on top — re-evaluates when Base is sculpted.
        stack.push(Layer::new(
            "Hills",
            LayerKind::NoiseValue(NoiseParams {
                seed: 1,
                frequency: 0.0015,
                amplitude: 80.0,
                octaves: 4,
                lacunarity: 2.0,
                persistence: 0.5,
                ..NoiseParams::default()
            }),
        ));
        debug_assert_eq!(
            stack.flatten_layers()[1].common.blend,
            crate::layer::BlendMode::Add
        );
        Self {
            version: DOCUMENT_VERSION,
            name: "Untitled".into(),
            metrics,
            preview_resolution: metrics.width,
            export_resolution: 2048,
            stack,
            masks: Vec::new(),
            selected: Some(base_id),
            presets_used: Vec::new(),
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        let mut doc: Self = serde_json::from_str(s)?;
        doc.migrate();
        Ok(doc)
    }

    fn migrate(&mut self) {
        // v1 → v2: LayerCommon gained locked/solo/color_tag/cached (serde defaults).
        if self.version < DOCUMENT_VERSION {
            self.version = DOCUMENT_VERSION;
        }
    }

    pub fn mask_map(&self) -> HashMap<MaskId, &MaskAsset> {
        self.masks.iter().map(|m| (m.id, m)).collect()
    }
}

/// Editor session state (not all serialized).
pub struct EditorSession {
    pub document: TerrainDocument,
    pub history: CommandHistory,
    pub dirty_eval: bool,
}

impl EditorSession {
    pub fn new() -> Self {
        Self {
            document: TerrainDocument::new_default(),
            history: CommandHistory::default(),
            dirty_eval: true,
        }
    }
}

impl Default for EditorSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_round_trip() {
        let doc = TerrainDocument::new_default();
        let s = doc.to_json().unwrap();
        let doc2 = TerrainDocument::from_json(&s).unwrap();
        assert_eq!(doc2.stack.len(), doc.stack.len());
        assert_eq!(doc2.name, doc.name);
    }
}
