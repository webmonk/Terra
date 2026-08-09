//! Named published outputs from layers and groups.

use crate::fields::FieldId;
use crate::layer::LayerId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable identity for a published output (survives display-name renames).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OutputId(pub Uuid);

impl OutputId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for OutputId {
    fn default() -> Self {
        Self::new()
    }
}

/// Declaration that a layer/group publishes a named field for later reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedOutputDecl {
    pub id: OutputId,
    /// Display name (not used as a persistent reference).
    pub name: String,
    pub field: FieldId,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl NamedOutputDecl {
    pub fn new(name: impl Into<String>, field: FieldId) -> Self {
        Self {
            id: OutputId::new(),
            name: name.into(),
            field,
            enabled: true,
        }
    }
}

/// Resolved published output after evaluation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublishedOutput {
    pub id: OutputId,
    pub source_layer: LayerId,
    pub name: String,
    pub field: FieldId,
}

/// Reference to a published output (by stable id).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OutputRef {
    pub id: OutputId,
}
