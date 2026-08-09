//! Internal evaluation diagnostics (not artist UI).

use crate::fields::FieldId;
use crate::layer::LayerId;
use std::collections::HashMap;
use std::time::Duration;

use super::OperatorId;

#[derive(Debug, Clone)]
pub struct OperatorTiming {
    pub operator: OperatorId,
    pub layer: Option<LayerId>,
    pub cpu: Duration,
    pub gpu: Duration,
}

/// Aggregated instrumentation for one evaluation pass.
#[derive(Debug, Clone, Default)]
pub struct EvalDiagnostics {
    pub operator_timings: Vec<OperatorTiming>,
    pub field_bytes: HashMap<FieldId, usize>,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub invalidated_fields: Vec<FieldId>,
    pub tiles_evaluated: u64,
    pub global_hydrology_recomputes: u64,
}

impl EvalDiagnostics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn record_operator(&mut self, timing: OperatorTiming) {
        self.operator_timings.push(timing);
    }

    pub fn note_cache_hit(&mut self) {
        self.cache_hits = self.cache_hits.saturating_add(1);
    }

    pub fn note_cache_miss(&mut self) {
        self.cache_misses = self.cache_misses.saturating_add(1);
    }

    pub fn note_invalidated(&mut self, field: FieldId) {
        if !self.invalidated_fields.contains(&field) {
            self.invalidated_fields.push(field);
        }
    }

    pub fn note_tile(&mut self) {
        self.tiles_evaluated = self.tiles_evaluated.saturating_add(1);
    }

    pub fn note_global_hydrology(&mut self) {
        self.global_hydrology_recomputes = self.global_hydrology_recomputes.saturating_add(1);
    }

    pub fn set_field_bytes(&mut self, field: FieldId, bytes: usize) {
        self.field_bytes.insert(field, bytes);
    }

    pub fn total_cpu(&self) -> Duration {
        self.operator_timings.iter().map(|t| t.cpu).sum()
    }

    pub fn total_gpu(&self) -> Duration {
        self.operator_timings.iter().map(|t| t.gpu).sum()
    }

    pub fn total_field_bytes(&self) -> usize {
        self.field_bytes.values().sum()
    }
}
