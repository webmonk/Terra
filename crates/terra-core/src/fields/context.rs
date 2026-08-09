//! TerrainContext — shared named field collection for stack evaluation.

use crate::eval::{EvalContext, PreviewQuality};
use crate::fields::{AuxMaps, FieldId};
use crate::heightfield::{Heightfield, HeightfieldMetrics};
use crate::layer::{LayerId, NamedOutputDecl, OutputId, PublishedOutput};
use crate::mask::{MaskAsset, MaskField, MaskId};
use crate::terrain_eval::{
    DerivedFieldCache, EvalDiagnostics, EvalMode, FieldLifetime, TerrainFieldSet,
};
use std::collections::HashMap;

/// Shared evaluation context: height plus lazily allocated named fields.
///
/// Wraps [`EvalContext`] so existing processors keep working while new code
/// uses strong [`FieldId`]s, published outputs, and the Phase 1
/// [`TerrainFieldSet`] / derived-field cache.
pub struct TerrainContext {
    pub eval: EvalContext,
    pub height: Heightfield,
    pub published: HashMap<OutputId, PublishedOutput>,
    /// Snapshot of published field data keyed by OutputId.
    pub published_fields: HashMap<OutputId, MaskField>,
    /// Multi-field state with revisions (kept in sync with height/aux).
    pub field_set: TerrainFieldSet,
    /// Shared derived analysis cache (slope, curvature, …).
    pub derived_cache: DerivedFieldCache,
    /// Internal diagnostics for the current evaluation (not artist UI).
    pub diagnostics: EvalDiagnostics,
}

impl TerrainContext {
    pub fn new(metrics: HeightfieldMetrics) -> Self {
        Self {
            eval: EvalContext::new(metrics),
            height: Heightfield::zeros(metrics),
            published: HashMap::new(),
            published_fields: HashMap::new(),
            field_set: TerrainFieldSet::new(metrics),
            derived_cache: DerivedFieldCache::new(),
            diagnostics: EvalDiagnostics::new(),
        }
    }

    pub fn from_eval(eval: EvalContext, height: Heightfield) -> Self {
        let field_set = TerrainFieldSet::from_height_and_aux(height.clone(), eval.aux_maps.clone());
        Self {
            eval,
            height,
            published: HashMap::new(),
            published_fields: HashMap::new(),
            field_set,
            derived_cache: DerivedFieldCache::new(),
            diagnostics: EvalDiagnostics::new(),
        }
    }

    pub fn metrics(&self) -> HeightfieldMetrics {
        self.eval.metrics
    }

    pub fn quality(&self) -> PreviewQuality {
        self.eval.quality
    }

    pub fn eval_mode(&self) -> EvalMode {
        EvalMode::from_preview_quality(self.eval.quality)
    }

    pub fn set_quality(&mut self, q: PreviewQuality) {
        self.eval.quality = q;
    }

    pub fn aux_maps(&self) -> &AuxMaps {
        &self.eval.aux_maps
    }

    pub fn aux_maps_mut(&mut self) -> &mut AuxMaps {
        &mut self.eval.aux_maps
    }

    /// Sync field-set height/aux after processors mutate `eval` / `height` directly.
    pub fn sync_field_set_from_eval(&mut self) {
        self.field_set.metrics = self.height.metrics;
        if !height_fingerprint_eq(&self.field_set.height, &self.height) {
            self.field_set.height = self.height.clone();
            self.field_set.bump_height();
            self.derived_cache.invalidate_all();
        }
        self.field_set.aux = self.eval.aux_maps.clone();
    }

    /// Push field-set height/aux back into the legacy eval context.
    pub fn sync_eval_from_field_set(&mut self) {
        self.height = self.field_set.height.clone();
        self.eval.metrics = self.field_set.metrics;
        self.eval.aux_maps = self.field_set.aux.clone();
        self.eval.sync_aux_hashmap();
    }

    /// Revision-aware derived field access (slope, curvature, …).
    pub fn get_derived(&mut self, id: FieldId) -> Result<MaskField, String> {
        self.sync_field_set_from_eval();
        let arc = self.derived_cache.get_or_compute(
            id,
            &mut self.field_set,
            Some(&mut self.diagnostics),
        )?;
        if let Some(slope) = self.field_set.aux.slope.as_ref() {
            self.eval.aux_maps.slope = Some(slope.clone());
        }
        if let Some(curv) = self.field_set.aux.curvature.as_ref() {
            self.eval.aux_maps.curvature = Some(curv.clone());
        }
        self.eval.sync_aux_hashmap();
        Ok((*arc).clone())
    }

    pub fn ensure_derived_fields(&mut self) {
        let _ = self.get_derived(FieldId::Slope);
        let _ = self.get_derived(FieldId::Curvature);
    }

    pub fn has_field(&self, id: &FieldId) -> bool {
        match id {
            FieldId::Height => true,
            other => self.eval.aux_maps.get(&other.cache_key()).is_some(),
        }
    }

    pub fn require_fields(&self, required: &[FieldId]) -> Result<(), String> {
        for id in required {
            if !self.has_field(id) {
                return Err(format!("missing required field: {}", id.display_name()));
            }
        }
        Ok(())
    }

    pub fn get_field(&self, id: &FieldId) -> Option<MaskField> {
        match id {
            FieldId::Height => {
                let m = self.height.metrics;
                let mut data = vec![0.0f32; (m.width * m.height) as usize];
                for j in 0..m.height {
                    for i in 0..m.width {
                        data[(j * m.width + i) as usize] = self.height.get(i, j);
                    }
                }
                Some(MaskField::from_raw(m, &data))
            }
            other => self.eval.aux_maps.get(&other.cache_key()).cloned(),
        }
    }

    pub fn set_field(&mut self, id: &FieldId, field: MaskField) {
        match id {
            FieldId::Height => {
                for j in 0..field.metrics.height {
                    for i in 0..field.metrics.width {
                        self.height.set(i, j, field.get(i, j));
                    }
                }
                self.height.refresh_halos();
                self.field_set.height = self.height.clone();
                self.field_set.bump_height();
                self.derived_cache.invalidate_all();
            }
            other => {
                self.eval.aux_insert(other.cache_key(), field.clone());
                self.field_set
                    .set_mask(other.clone(), field, FieldLifetime::Persistent);
            }
        }
    }

    pub fn publish(&mut self, source: LayerId, decl: &NamedOutputDecl, field: MaskField) {
        if !decl.enabled {
            return;
        }
        self.published.insert(
            decl.id,
            PublishedOutput {
                id: decl.id,
                source_layer: source,
                name: decl.name.clone(),
                field: decl.field.clone(),
            },
        );
        self.published_fields.insert(decl.id, field);
    }

    pub fn get_published(&self, id: OutputId) -> Option<&MaskField> {
        self.published_fields.get(&id)
    }

    pub fn set_mask_assets(&mut self, assets: Vec<MaskAsset>, baked: HashMap<MaskId, MaskField>) {
        self.eval.mask_assets = assets;
        self.eval.masks = baked;
    }

    /// Deep clone suitable for isolated group private contexts.
    pub fn isolated_clone(&self) -> Self {
        Self {
            eval: EvalContext {
                metrics: self.eval.metrics,
                level_steps: self.eval.level_steps.clone(),
                masks: self.eval.masks.clone(),
                mask_assets: self.eval.mask_assets.clone(),
                aux_maps: self.eval.aux_maps.clone(),
                aux: self.eval.aux.clone(),
                published_outputs: self.eval.published_outputs.clone(),
                cancelled: self.eval.cancelled,
                cancellation_generation: self.eval.cancellation_generation.clone(),
                quality: self.eval.quality,
                layer_timings: self.eval.layer_timings.clone(),
            },
            height: self.height.clone(),
            published: self.published.clone(),
            published_fields: self.published_fields.clone(),
            field_set: self.field_set.clone(),
            derived_cache: DerivedFieldCache::new(),
            diagnostics: EvalDiagnostics::new(),
        }
    }

    /// Start an isolated group with empty height, optionally keeping aux maps.
    pub fn with_empty_height(&self) -> Self {
        let mut c = self.isolated_clone();
        c.height = Heightfield::zeros(self.eval.metrics);
        c.field_set =
            TerrainFieldSet::from_height_and_aux(c.height.clone(), c.eval.aux_maps.clone());
        c.derived_cache.invalidate_all();
        c
    }

    /// Merge aux maps from a private group context into this parent (after composite).
    pub fn merge_aux_from(&mut self, other: &TerrainContext) {
        self.eval.aux_maps.extend(&other.eval.aux_maps);
        self.eval.sync_aux_hashmap();
        self.field_set.aux.extend(&other.eval.aux_maps);
        for (k, v) in &other.published {
            self.published.insert(*k, v.clone());
        }
        for (k, v) in &other.published_fields {
            self.published_fields.insert(*k, v.clone());
        }
    }
}

fn height_fingerprint_eq(a: &Heightfield, b: &Heightfield) -> bool {
    if a.metrics != b.metrics {
        return false;
    }
    let m = a.metrics;
    if m.width == 0 || m.height == 0 {
        return true;
    }
    let samples = [
        (0, 0),
        (m.width / 2, m.height / 2),
        (m.width - 1, m.height - 1),
    ];
    for (i, j) in samples {
        if (a.get(i, j) - b.get(i, j)).abs() > 1e-6 {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_always_present() {
        let m = HeightfieldMetrics::new(8, 8, 8.0, 8.0);
        let ctx = TerrainContext::new(m);
        assert!(ctx.has_field(&FieldId::Height));
        assert!(!ctx.has_field(&FieldId::Wetness));
    }

    #[test]
    fn derived_slope_via_cache() {
        let m = HeightfieldMetrics::new(16, 16, 16.0, 16.0);
        let mut ctx = TerrainContext::new(m);
        ctx.height.set(8, 8, 5.0);
        ctx.field_set.height = ctx.height.clone();
        ctx.field_set.bump_height();
        let s1 = ctx.get_derived(FieldId::Slope).unwrap();
        assert!(s1.get(7, 8) > 0.0 || s1.get(9, 8) > 0.0);
        let hits_before = ctx.derived_cache.stats.hits;
        let _ = ctx.get_derived(FieldId::Slope).unwrap();
        assert!(ctx.derived_cache.stats.hits > hits_before);
    }
}
