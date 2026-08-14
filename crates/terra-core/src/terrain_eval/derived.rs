//! Revision-keyed derived field cache.

use crate::analyze::{aspect_mask, concavity, convexity, curvature, slope_degrees};
use crate::fields::FieldId;
use crate::geomorph::{
    aspect_radians, cavity_openness, convexity_concavity, gaussian_curvature, gradient_components,
    laplacian, mean_curvature, multi_radius_roughness, plan_curvature, profile_curvature,
    ridge_valley_likelihood, slope_magnitude,
};
use crate::heightfield::Heightfield;
use crate::mask::MaskField;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use super::diagnostics::{EvalDiagnostics, OperatorId, OperatorTiming};
use super::field_set::{FieldRevision, TerrainFieldSet};

#[derive(Debug, Clone, Default)]
pub struct DerivedFieldCacheStats {
    pub hits: u64,
    pub misses: u64,
}

/// Shared derived-field cache keyed by (field, height revision).
///
/// `get_field(SLOPE)` checks height revision, reuses a valid entry, otherwise
/// dispatches calculation and stores the result against the input revision.
#[derive(Debug, Default)]
pub struct DerivedFieldCache {
    entries: HashMap<FieldId, CachedDerived>,
    pub stats: DerivedFieldCacheStats,
}

#[derive(Debug, Clone)]
struct CachedDerived {
    source_revision: FieldRevision,
    data: Arc<MaskField>,
}

impl DerivedFieldCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn invalidate_for_height(&mut self, height_revision: FieldRevision) {
        self.entries
            .retain(|_, e| e.source_revision == height_revision);
    }

    pub fn invalidate_all(&mut self) {
        self.entries.clear();
    }

    /// Fetch or compute a derived analysis field for the current height revision.
    pub fn get_or_compute(
        &mut self,
        field: FieldId,
        state: &mut TerrainFieldSet,
        mut diagnostics: Option<&mut EvalDiagnostics>,
    ) -> Result<Arc<MaskField>, String> {
        let height_rev = state.height_revision;

        if let Some(cached) = self.entries.get(&field) {
            if cached.source_revision == height_rev {
                self.stats.hits = self.stats.hits.saturating_add(1);
                if let Some(d) = diagnostics.as_deref_mut() {
                    d.note_cache_hit();
                }
                // Keep field set slot in sync.
                if !state.derived_valid(&field) {
                    state.set_derived(field.clone(), (*cached.data).clone());
                }
                return Ok(Arc::clone(&cached.data));
            }
        }

        // Also honour a still-valid slot on the field set.
        if state.derived_valid(&field) {
            if let Some(mask) = state.get_mask(&field) {
                let arc = Arc::new(mask);
                self.entries.insert(
                    field.clone(),
                    CachedDerived {
                        source_revision: height_rev,
                        data: Arc::clone(&arc),
                    },
                );
                self.stats.hits = self.stats.hits.saturating_add(1);
                if let Some(d) = diagnostics.as_deref_mut() {
                    d.note_cache_hit();
                }
                return Ok(arc);
            }
        }

        self.stats.misses = self.stats.misses.saturating_add(1);
        if let Some(d) = diagnostics.as_deref_mut() {
            d.note_cache_miss();
        }

        let started = Instant::now();
        let computed = compute_derived(field.clone(), &state.height)?;
        let elapsed = started.elapsed();

        state.set_derived(field.clone(), (*computed).clone());
        self.entries.insert(
            field.clone(),
            CachedDerived {
                source_revision: height_rev,
                data: Arc::clone(&computed),
            },
        );

        if let Some(d) = diagnostics {
            d.set_field_bytes(field.clone(), computed.data().len() * 4);
            d.record_operator(OperatorTiming {
                operator: OperatorId::Derived(field),
                layer: None,
                cpu: elapsed,
                gpu: Default::default(),
            });
        }

        Ok(computed)
    }

    /// Convenience: slope for current height revision.
    pub fn slope(
        &mut self,
        state: &mut TerrainFieldSet,
        diagnostics: Option<&mut EvalDiagnostics>,
    ) -> Result<Arc<MaskField>, String> {
        self.get_or_compute(FieldId::Slope, state, diagnostics)
    }

    pub fn curvature(
        &mut self,
        state: &mut TerrainFieldSet,
        diagnostics: Option<&mut EvalDiagnostics>,
    ) -> Result<Arc<MaskField>, String> {
        self.get_or_compute(FieldId::Curvature, state, diagnostics)
    }
}

fn compute_derived(field: FieldId, height: &Heightfield) -> Result<Arc<MaskField>, String> {
    // Default world radius: one texel (radius_m = 0 → 1 texel in geomorph helpers).
    let radius_m = 0.0;
    let mask = match field {
        FieldId::Slope => slope_magnitude(height, radius_m),
        FieldId::Curvature | FieldId::MeanCurvature => mean_curvature(height, radius_m),
        FieldId::ProfileCurvature => profile_curvature(height, radius_m),
        FieldId::PlanCurvature => plan_curvature(height, radius_m),
        FieldId::GaussianCurvature => gaussian_curvature(height, radius_m),
        FieldId::Laplacian => laplacian(height, radius_m),
        FieldId::Convexity => convexity_concavity(height, radius_m).0,
        FieldId::Concavity => convexity_concavity(height, radius_m).1,
        FieldId::Aspect => aspect_radians(height, radius_m),
        FieldId::Gradient => {
            let (_gx, _gz, mag): (MaskField, MaskField, MaskField) =
                gradient_components(height, radius_m);
            // Soft-normalise for mask storage (compat with prior /2 clamp).
            let m = height.metrics;
            let mut out = MaskField::zeros(m);
            for j in 0..m.height {
                for i in 0..m.width {
                    out.set(i, j, (mag.get(i, j) / 2.0).clamp(0.0, 1.0));
                }
            }
            out
        }
        FieldId::Cavity => cavity_openness(height, 16.0_f32.max(height.metrics.dx() * 4.0), 8),
        FieldId::Roughness => {
            let dx = height.metrics.dx();
            multi_radius_roughness(height, &[dx * 2.0, dx * 8.0, dx * 32.0])
        }
        FieldId::RidgeLikelihood => ridge_valley_likelihood(height, radius_m).0,
        FieldId::ValleyLikelihood => ridge_valley_likelihood(height, radius_m).1,
        // Legacy analyze paths kept as fallback for mask-gated aspect bands.
        FieldId::Named(ref key) if key == "aspect_legacy" => aspect_mask(height, 0.0, 360.0),
        FieldId::Named(ref key) if key == "slope_legacy" => slope_degrees(height),
        FieldId::Named(ref key) if key == "curvature_legacy" => curvature(height),
        FieldId::Named(ref key) if key == "convexity_legacy" => convexity(height),
        FieldId::Named(ref key) if key == "concavity_legacy" => concavity(height),
        other => {
            return Err(format!(
                "no derived compute path for {}",
                other.display_name()
            ))
        }
    };
    Ok(Arc::new(mask))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heightfield::HeightfieldMetrics;

    #[test]
    fn slope_cached_until_height_changes() {
        let m = HeightfieldMetrics::new(16, 16, 16.0, 16.0);
        let mut hf = Heightfield::zeros(m);
        hf.set(8, 8, 4.0);
        let mut state = TerrainFieldSet::from_height(hf);
        let mut cache = DerivedFieldCache::new();

        let _ = cache.slope(&mut state, None).unwrap();
        assert_eq!(cache.stats.misses, 1);
        let _ = cache.slope(&mut state, None).unwrap();
        assert_eq!(cache.stats.hits, 1);

        state.height.set(4, 4, 2.0);
        state.bump_height();
        let _ = cache.slope(&mut state, None).unwrap();
        assert_eq!(cache.stats.misses, 2);
    }

    #[test]
    fn profile_curvature_is_cached() {
        let m = HeightfieldMetrics::new(16, 16, 160.0, 160.0);
        let hf = crate::geomorph::single_valley(m);
        let mut state = TerrainFieldSet::from_height(hf);
        let mut cache = DerivedFieldCache::new();
        let a = cache
            .get_or_compute(FieldId::ProfileCurvature, &mut state, None)
            .unwrap();
        let b = cache
            .get_or_compute(FieldId::ProfileCurvature, &mut state, None)
            .unwrap();
        assert_eq!(cache.stats.hits, 1);
        assert_eq!(a.data(), b.data());
    }
}
