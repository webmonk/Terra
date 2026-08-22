use super::smart_cache::DiskSmartCache;
use crate::quality::PreviewQuality;
use crate::heightfield::{Heightfield, HeightfieldMetrics};
use crate::layer::LayerId;
use crate::mask::MaskField;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct CachedOutput {
    pub height: Heightfield,
    pub generation: u64,
    pub dirty: bool,
    pub aux: HashMap<String, MaskField>,
    /// Materials strata (not stored in the aux HashMap).
    pub strata: Option<Vec<crate::layer::Stratum>>,
    /// Placed props (not a raster, so not in the aux HashMap either).
    ///
    /// Carried for the same reason as `strata`: a group cache hit skips its
    /// children, so anything they published has to come back from here or it
    /// is silently gone.
    pub object_instances: Vec<crate::layer::ObjectInstance>,
}

#[derive(Debug)]
pub struct LayerCache {
    entries: HashMap<LayerId, CachedOutput>,
    pub generation: u64,
    /// Quality every current entry was computed at.
    ///
    /// Many processors branch on `EvalContext::quality` - iteration counts, sim
    /// level schedules, whether layered thermal takes the coarse-to-fine path -
    /// so a result is only valid for the rung that produced it. Entry validity
    /// used to be checked on grid dimensions alone, and Draft, Medium and Full
    /// collapse to the same resolution whenever the project preview is at or
    /// below their caps (Medium and Full always do at the default 1024). The
    /// refine ladder then found nothing dirty and served the Draft solve back
    /// while reporting Full.
    quality: Option<PreviewQuality>,
    /// Optional on-disk spill for baked (`cached`) layer checkpoints.
    disk: Option<DiskSmartCache>,
}

impl Default for LayerCache {
    fn default() -> Self {
        Self::new()
    }
}

impl LayerCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            generation: 0,
            quality: None,
            disk: Some(DiskSmartCache::new(DiskSmartCache::default_location())),
        }
    }

    pub fn with_disk(root: impl Into<PathBuf>) -> Self {
        Self {
            entries: HashMap::new(),
            generation: 0,
            quality: None,
            disk: Some(DiskSmartCache::new(root)),
        }
    }

    pub fn without_disk() -> Self {
        Self {
            entries: HashMap::new(),
            generation: 0,
            quality: None,
            disk: None,
        }
    }

    pub fn enable_disk(&mut self, root: impl Into<PathBuf>) {
        self.disk = Some(DiskSmartCache::new(root));
    }

    pub fn disable_disk(&mut self) {
        self.disk = None;
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.generation = self.generation.wrapping_add(1);
    }

    /// Drop everything computed at a different quality rung.
    ///
    /// Conservative on purpose: rather than maintaining a per-kind
    /// "is this processor quality-sensitive" contract - the same shape of
    /// declaration that has repeatedly gone stale in this codebase and left
    /// consumers holding invalid caches - a rung change invalidates the lot.
    /// Layers that would have survived are exactly the ones a rung change is
    /// cheap for.
    pub fn ensure_quality(&mut self, quality: PreviewQuality) {
        if self.quality == Some(quality) {
            return;
        }
        if self.quality.is_some() {
            self.entries.clear();
            self.generation = self.generation.wrapping_add(1);
        }
        self.quality = Some(quality);
    }

    /// Quality the current entries were computed at, if any.
    pub fn quality(&self) -> Option<PreviewQuality> {
        self.quality
    }

    pub fn insert(&mut self, id: LayerId, output: CachedOutput) {
        self.entries.insert(id, output);
    }

    /// Insert and write a durable disk checkpoint.
    pub fn insert_baked(&mut self, id: LayerId, output: CachedOutput) {
        if let Some(disk) = &self.disk {
            let _ = disk.spill(id, &output, self.quality);
        }
        self.entries.insert(id, output);
    }

    pub fn get(&self, id: LayerId) -> Option<&CachedOutput> {
        self.entries.get(&id)
    }

    /// Memory hit, else try disk reload for a clean bake matching `metrics`.
    pub fn get_or_load(
        &mut self,
        id: LayerId,
        metrics: HeightfieldMetrics,
    ) -> Option<&CachedOutput> {
        let mem_ok = self.entries.get(&id).is_some_and(|e| {
            !e.dirty
                && e.height.metrics.width == metrics.width
                && e.height.metrics.height == metrics.height
        });
        if !mem_ok {
            // Dirty / missing / wrong size: try disk (dirty entries invalidate disk on mark).
            if self.entries.get(&id).is_none_or(|e| e.dirty) {
                if let Some(disk) = &self.disk {
                    if let Ok(Some(loaded)) = disk.load(id, metrics, self.quality) {
                        self.entries.insert(id, loaded);
                    }
                }
            }
        }
        self.entries.get(&id).filter(|e| {
            !e.dirty
                && e.height.metrics.width == metrics.width
                && e.height.metrics.height == metrics.height
        })
    }

    pub fn mark_dirty(&mut self, id: LayerId) {
        if let Some(e) = self.entries.get_mut(&id) {
            e.dirty = true;
        }
        if let Some(disk) = &self.disk {
            disk.invalidate(id);
        }
        self.generation = self.generation.wrapping_add(1);
    }

    /// Mark an existing cached result as a clean baked checkpoint and spill to disk.
    pub fn pin_baked(&mut self, id: LayerId) {
        if let Some(e) = self.entries.get_mut(&id) {
            e.dirty = false;
        }
        if let (Some(disk), Some(e)) = (&self.disk, self.entries.get(&id)) {
            let _ = disk.spill(id, e, self.quality);
        }
    }

    pub fn is_dirty(&self, id: LayerId) -> bool {
        match self.entries.get(&id) {
            Some(e) => e.dirty,
            None => true,
        }
    }

    pub fn get_mut(&mut self, id: LayerId) -> Option<&mut CachedOutput> {
        self.entries.get_mut(&id)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn disk_root(&self) -> Option<&std::path::Path> {
        self.disk.as_ref().map(|d| d.root())
    }
}
