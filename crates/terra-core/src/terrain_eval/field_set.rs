//! Central multi-field terrain state with revisions and lifetimes.

use crate::fields::{AuxMaps, FieldId};
use crate::heightfield::{Heightfield, HeightfieldMetrics};
use crate::mask::MaskField;
use std::collections::HashMap;
use std::sync::Arc;

use super::scale::ScaleBand;

/// Monotonic content revision for dependency invalidation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FieldRevision(pub u64);

impl FieldRevision {
    pub fn bump(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

/// How long a field slot is expected to live.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldLifetime {
    /// Survives across frames / may be serialised (height, materials, …).
    Persistent,
    /// Valid only for the current evaluation pass (derived analysis, scratch).
    Transient,
}

/// Where sample payloads may reside. CPU is always available; GPU is optional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FieldStorage {
    /// Dense CPU samples (MaskField / Heightfield tiles).
    Cpu,
    /// GPU texture or buffer handle owned outside terra-core.
    GpuResident,
    /// Both CPU and GPU copies may be present.
    Hybrid,
}

/// One named field inside a [`TerrainFieldSet`].
#[derive(Debug, Clone)]
pub struct FieldSlot {
    pub id: FieldId,
    pub revision: FieldRevision,
    /// Revision of the primary dependency used to generate this slot (e.g. height).
    pub source_revision: FieldRevision,
    pub lifetime: FieldLifetime,
    pub storage: FieldStorage,
    pub scale_band: ScaleBand,
    pub metrics: HeightfieldMetrics,
    /// Lazy: `None` means not yet materialised.
    pub data: Option<Arc<MaskField>>,
}

impl FieldSlot {
    pub fn empty(
        id: FieldId,
        metrics: HeightfieldMetrics,
        lifetime: FieldLifetime,
        scale_band: ScaleBand,
    ) -> Self {
        Self {
            id,
            revision: FieldRevision(0),
            source_revision: FieldRevision(0),
            lifetime,
            storage: FieldStorage::Cpu,
            scale_band,
            metrics,
            data: None,
        }
    }

    pub fn is_resident(&self) -> bool {
        self.data.is_some()
    }

    pub fn byte_size(&self) -> usize {
        self.data
            .as_ref()
            .map(|d| d.data().len() * std::mem::size_of::<f32>())
            .unwrap_or(0)
    }
}

/// Shared multi-field terrain dataset.
///
/// Height remains a first-class [`Heightfield`] for mesh/export compatibility.
/// Auxiliary / derived fields live in typed [`AuxMaps`] plus optional slots for
/// fields that are not yet represented in AuxMaps.
#[derive(Debug, Clone)]
pub struct TerrainFieldSet {
    pub metrics: HeightfieldMetrics,
    pub height: Heightfield,
    pub height_revision: FieldRevision,
    pub aux: AuxMaps,
    /// Per-field metadata + optional payloads (including derived caches).
    slots: HashMap<FieldId, FieldSlot>,
    next_revision: u64,
}

/// Alias emphasising the mutable evaluation state role.
pub type TerrainState = TerrainFieldSet;

impl TerrainFieldSet {
    pub fn new(metrics: HeightfieldMetrics) -> Self {
        let mut set = Self {
            metrics,
            height: Heightfield::zeros(metrics),
            height_revision: FieldRevision(1),
            aux: AuxMaps::new(),
            slots: HashMap::new(),
            next_revision: 2,
        };
        set.ensure_slot(
            FieldId::Height,
            FieldLifetime::Persistent,
            ScaleBand::Meso,
        );
        if let Some(slot) = set.slots.get_mut(&FieldId::Height) {
            slot.revision = set.height_revision;
        }
        set
    }

    pub fn from_height(height: Heightfield) -> Self {
        let metrics = height.metrics;
        let mut set = Self::new(metrics);
        set.height = height;
        set.bump_height();
        set
    }

    pub fn from_height_and_aux(height: Heightfield, aux: AuxMaps) -> Self {
        let mut set = Self::from_height(height);
        set.aux = aux;
        set.sync_slots_from_aux();
        set
    }

    fn alloc_revision(&mut self) -> FieldRevision {
        let r = FieldRevision(self.next_revision);
        self.next_revision = self.next_revision.wrapping_add(1);
        r
    }

    pub fn bump_height(&mut self) {
        self.height_revision = self.alloc_revision();
        if let Some(slot) = self.slots.get_mut(&FieldId::Height) {
            slot.revision = self.height_revision;
            slot.source_revision = self.height_revision;
            slot.metrics = self.height.metrics;
        }
        // Height change invalidates derived analysis slots.
        self.invalidate_derived_analysis();
    }

    pub fn ensure_slot(
        &mut self,
        id: FieldId,
        lifetime: FieldLifetime,
        scale_band: ScaleBand,
    ) -> &mut FieldSlot {
        let metrics = self.metrics;
        self.slots
            .entry(id.clone())
            .or_insert_with(|| FieldSlot::empty(id, metrics, lifetime, scale_band))
    }

    pub fn slot(&self, id: &FieldId) -> Option<&FieldSlot> {
        self.slots.get(id)
    }

    pub fn revision_of(&self, id: &FieldId) -> FieldRevision {
        if *id == FieldId::Height {
            return self.height_revision;
        }
        self.slots
            .get(id)
            .map(|s| s.revision)
            .unwrap_or(FieldRevision(0))
    }

    pub fn has_field(&self, id: &FieldId) -> bool {
        match id {
            FieldId::Height => true,
            other => {
                self.aux.get(&other.cache_key()).is_some()
                    || self
                        .slots
                        .get(other)
                        .map(|s| s.is_resident())
                        .unwrap_or(false)
            }
        }
    }

    /// Read a field as MaskField (height is converted on demand).
    pub fn get_mask(&self, id: &FieldId) -> Option<MaskField> {
        match id {
            FieldId::Height => Some(height_to_mask(&self.height)),
            other => {
                if let Some(slot) = self.slots.get(other) {
                    if let Some(data) = &slot.data {
                        return Some((**data).clone());
                    }
                }
                self.aux.get(&other.cache_key()).cloned()
            }
        }
    }

    /// Write / update a non-height field and bump its revision.
    pub fn set_mask(&mut self, id: FieldId, field: MaskField, lifetime: FieldLifetime) {
        if id == FieldId::Height {
            for j in 0..field.metrics.height {
                for i in 0..field.metrics.width {
                    self.height.set(i, j, field.get(i, j));
                }
            }
            self.height.refresh_halos();
            self.metrics = self.height.metrics;
            self.bump_height();
            return;
        }

        let rev = self.alloc_revision();
        let src = self.height_revision;
        self.aux.insert(id.cache_key(), field.clone());
        let slot = self.ensure_slot(id.clone(), lifetime, ScaleBand::Meso);
        slot.revision = rev;
        slot.source_revision = src;
        slot.metrics = field.metrics;
        slot.lifetime = lifetime;
        slot.data = Some(Arc::new(field));
    }

    /// Store a derived field tied to the current height revision.
    pub fn set_derived(&mut self, id: FieldId, field: MaskField) {
        let rev = self.alloc_revision();
        let src = self.height_revision;
        // Keep AuxMaps in sync for slope/curvature consumers.
        if matches!(id, FieldId::Slope | FieldId::Curvature) {
            self.aux.insert(id.cache_key(), field.clone());
        }
        let slot = self.ensure_slot(id.clone(), FieldLifetime::Transient, ScaleBand::Meso);
        slot.revision = rev;
        slot.source_revision = src;
        slot.metrics = field.metrics;
        slot.lifetime = FieldLifetime::Transient;
        slot.data = Some(Arc::new(field));
    }

    /// True when a derived slot is present and still matches height revision.
    pub fn derived_valid(&self, id: &FieldId) -> bool {
        self.slots
            .get(id)
            .map(|s| s.is_resident() && s.source_revision == self.height_revision)
            .unwrap_or(false)
    }

    pub fn invalidate_derived_analysis(&mut self) {
        let derived: Vec<FieldId> = self
            .slots
            .keys()
            .filter(|id| id.is_derived_analysis())
            .cloned()
            .collect();
        for id in derived {
            if let Some(slot) = self.slots.get_mut(&id) {
                slot.data = None;
                slot.source_revision = FieldRevision(0);
            }
            // Clear lazy AuxMaps derived caches so they recompute.
            match id {
                FieldId::Slope => self.aux.slope = None,
                FieldId::Curvature => self.aux.curvature = None,
                _ => {}
            }
        }
    }

    pub fn clear_transient(&mut self) {
        self.slots.retain(|_, slot| {
            if slot.lifetime == FieldLifetime::Transient {
                false
            } else {
                true
            }
        });
        self.aux.slope = None;
        self.aux.curvature = None;
    }

    fn sync_slots_from_aux(&mut self) {
        for (key, field) in self.aux.to_hashmap() {
            let id = FieldId::from_cache_key(&key);
            let lifetime = if id.is_derived_analysis() {
                FieldLifetime::Transient
            } else {
                FieldLifetime::Persistent
            };
            let rev = self.alloc_revision();
            let src = self.height_revision;
            let slot = self.ensure_slot(id.clone(), lifetime, ScaleBand::Meso);
            slot.revision = rev;
            slot.source_revision = src;
            slot.metrics = field.metrics;
            slot.data = Some(Arc::new(field));
        }
    }

    pub fn total_resident_bytes(&self) -> usize {
        let height_bytes = (self.height.metrics.width as usize)
            * (self.height.metrics.height as usize)
            * std::mem::size_of::<f32>();
        let slot_bytes: usize = self.slots.values().map(|s| s.byte_size()).sum();
        height_bytes + slot_bytes
    }

    pub fn into_height_and_aux(self) -> (Heightfield, AuxMaps) {
        (self.height, self.aux)
    }
}

fn height_to_mask(hf: &Heightfield) -> MaskField {
    let m = hf.metrics;
    let mut data = vec![0.0f32; (m.width * m.height) as usize];
    for j in 0..m.height {
        for i in 0..m.width {
            data[(j * m.width + i) as usize] = hf.get(i, j);
        }
    }
    MaskField::from_raw(m, &data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn height_bump_invalidates_derived() {
        let m = HeightfieldMetrics::new(8, 8, 8.0, 8.0);
        let mut set = TerrainFieldSet::new(m);
        set.set_derived(FieldId::Slope, MaskField::filled(m, 0.25));
        assert!(set.derived_valid(&FieldId::Slope));
        set.height.set(2, 2, 5.0);
        set.bump_height();
        assert!(!set.derived_valid(&FieldId::Slope));
    }

    #[test]
    fn aux_roundtrip_preserves_wetness() {
        let m = HeightfieldMetrics::new(4, 4, 4.0, 4.0);
        let mut set = TerrainFieldSet::new(m);
        set.set_mask(
            FieldId::Wetness,
            MaskField::filled(m, 0.7),
            FieldLifetime::Persistent,
        );
        assert!(set.has_field(&FieldId::Wetness));
        let w = set.get_mask(&FieldId::Wetness).unwrap();
        assert!((w.get(0, 0) - 0.7).abs() < 1e-5);
    }
}
