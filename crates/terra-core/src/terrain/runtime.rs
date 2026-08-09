use super::{
    DirtyEvent, EditorRefinementState, InvalidationSet, OperationDescriptor, PyramidConfig,
    RefinementController, RegionSet, TerrainPyramid, TerrainTileKey, TerrainWorkItem,
    TerrainWorkKind, TerrainWorkScheduler, TileState, WorkPriority, WorkSchedulerStats,
};
use crate::fields::FieldId;
use crate::layer::{LayerId, LayerKind, LayerStack};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerrainRuntimeStats {
    pub revision: u64,
    pub invalidated_tiles: usize,
    pub work: WorkSchedulerStats,
    pub refinement: EditorRefinementState,
}

/// App-facing migration seam for sparse invalidation and progressively scheduled terrain work.
/// Payload evaluation remains backend-owned, so the existing evaluator can stay active while
/// CPU and GPU operations move behind tile work items incrementally.
#[derive(Debug)]
pub struct TerrainRuntime {
    pub pyramid: TerrainPyramid,
    pub scheduler: TerrainWorkScheduler,
    pub refinement: RefinementController,
    revision: u64,
    last_invalidated_tiles: usize,
}

impl TerrainRuntime {
    pub fn new(config: PyramidConfig) -> Self {
        Self {
            pyramid: TerrainPyramid::new(config),
            scheduler: TerrainWorkScheduler::default(),
            refinement: RefinementController::default(),
            revision: 0,
            last_invalidated_tiles: 0,
        }
    }

    pub fn reconfigure(&mut self, config: PyramidConfig) {
        self.pyramid = TerrainPyramid::new(config);
        self.scheduler = TerrainWorkScheduler::default();
        self.revision = self.revision.wrapping_add(1);
        self.last_invalidated_tiles = 0;
    }

    pub fn invalidate_layer(&mut self, stack: &LayerStack, source: LayerId) -> usize {
        let Some(layer) = stack.find(source) else {
            self.invalidate_all();
            return 0;
        };
        let Some(finest) = self.pyramid.levels.last().map(|level| level.metrics) else {
            return 0;
        };
        let descriptor = OperationDescriptor::from_layer(layer, finest, self.pyramid.max_level());
        let fields = if descriptor.produced.is_empty() {
            vec![FieldId::Height]
        } else {
            descriptor.produced.clone()
        };
        self.invalidate_regions(
            stack,
            source,
            fields,
            descriptor.bounds,
            descriptor.frequency,
        )
    }

    /// Invalidate an explicit authored footprint, such as one brush-stroke segment.
    pub fn invalidate_layer_regions(
        &mut self,
        stack: &LayerStack,
        source: LayerId,
        regions: RegionSet,
    ) -> usize {
        if regions.is_empty() {
            return 0;
        }
        let Some(layer) = stack.find(source) else {
            self.invalidate_all();
            return 0;
        };
        let Some(finest) = self.pyramid.levels.last().map(|level| level.metrics) else {
            return 0;
        };
        let descriptor = OperationDescriptor::from_layer(layer, finest, self.pyramid.max_level());
        let fields = if descriptor.produced.is_empty() {
            vec![FieldId::Height]
        } else {
            descriptor.produced
        };
        self.invalidate_regions(stack, source, fields, regions, descriptor.frequency)
    }

    fn invalidate_regions(
        &mut self,
        stack: &LayerStack,
        source: LayerId,
        fields: Vec<FieldId>,
        regions: RegionSet,
        frequency: super::FrequencyBand,
    ) -> usize {
        self.revision = self.revision.wrapping_add(1);
        let event = DirtyEvent {
            source,
            fields,
            regions,
            frequency,
            revision: self.revision,
        };
        let invalidation = InvalidationSet::propagate(&event, stack, &self.pyramid.level_metrics());
        self.pyramid.mark_invalid(&invalidation);
        let keys: Vec<_> = invalidation.keys().cloned().collect();
        for invalid in &keys {
            if invalid.field == FieldId::Height {
                let record = self.pyramid.record_mut(TerrainTileKey {
                    layer: None,
                    field: FieldId::Height,
                    level: invalid.level,
                    tile: invalid.tile,
                });
                record.state = if record.state == TileState::Resident {
                    TileState::Stale
                } else {
                    TileState::Dirty
                };
            }
        }
        for invalid in keys {
            let kind = stack
                .find(invalid.layer)
                .map(|layer| work_kind(&layer.kind))
                .unwrap_or(TerrainWorkKind::EvaluateLayer);
            let key = TerrainTileKey {
                layer: Some(invalid.layer),
                field: invalid.field,
                level: invalid.level,
                tile: invalid.tile,
            };
            self.scheduler.enqueue(TerrainWorkItem {
                id: 0,
                kind,
                key: key.clone(),
                layer: Some(invalid.layer),
                revision: self.revision,
                estimated_us: estimate_work_us(kind, invalid.level),
                priority: WorkPriority::interactive(invalid.level, 128),
            });
            self.pyramid.record_mut(key).state = TileState::Queued;
        }
        self.last_invalidated_tiles = invalidation.len();
        self.last_invalidated_tiles
    }

    pub fn invalidate_all(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.scheduler = TerrainWorkScheduler::default();
        self.pyramid = TerrainPyramid::new(self.pyramid.config);
        self.last_invalidated_tiles = 0;
    }

    pub fn update_refinement(
        &mut self,
        now_ms: u64,
        interaction_active: bool,
    ) -> EditorRefinementState {
        self.refinement.update(now_ms, interaction_active)
    }

    /// Dequeue work supported by one backend within the current interaction-state budget.
    pub fn take_frame_work_matching<F>(&mut self, accepts: F) -> Vec<TerrainWorkItem>
    where
        F: FnMut(&TerrainWorkItem) -> bool,
    {
        let budget_us = self.refinement.state().terrain_budget_us();
        let selected = self.scheduler.pop_budgeted_matching(budget_us, accepts);
        for work in &selected {
            self.pyramid.record_mut(work.key.clone()).state = TileState::Computing;
        }
        selected
    }

    /// Return work that a backend could not execute without losing its priority or revision.
    pub fn requeue_work(&mut self, work: TerrainWorkItem) {
        if !self.scheduler.is_stale(&work) {
            self.pyramid.record_mut(work.key.clone()).state = TileState::Queued;
            self.scheduler.enqueue(work);
        }
    }

    /// Publish only if the work revision is still live; stale completions never reach rendering.
    pub fn publish_completed(
        &mut self,
        work: &TerrainWorkItem,
        input_revision_hash: u64,
        frame: u64,
    ) -> bool {
        if self.scheduler.is_stale(work) {
            return false;
        }
        self.pyramid
            .publish(work.key.clone(), work.revision, input_revision_hash, frame);
        true
    }
    /// Mark live sparse work complete when the compatibility whole-field evaluator produced the
    /// same revision. This keeps migration queues truthful without pretending stale jobs finished.
    pub fn publish_fallback_result(&mut self, input_revision_hash: u64, frame: u64) -> usize {
        let completed = self.scheduler.pop_budgeted(u64::MAX);
        let mut published = 0;
        for work in completed {
            published += usize::from(self.publish_completed(&work, input_revision_hash, frame));
        }
        published
    }

    pub fn stats(&self) -> TerrainRuntimeStats {
        TerrainRuntimeStats {
            revision: self.revision,
            invalidated_tiles: self.last_invalidated_tiles,
            work: self.scheduler.stats(),
            refinement: self.refinement.state(),
        }
    }
}

fn work_kind(kind: &LayerKind) -> TerrainWorkKind {
    match kind {
        LayerKind::SculptBase(_) | LayerKind::Path(_) | LayerKind::PolygonHeight(_) => {
            TerrainWorkKind::RasterizeVector
        }
        LayerKind::HydraulicErosion(_)
        | LayerKind::ThermalErosion(_)
        | LayerKind::StreamPowerErosion(_)
        | LayerKind::RiverCarve(_)
        | LayerKind::RiverNetwork(_) => TerrainWorkKind::SimulateBatch,
        _ => TerrainWorkKind::EvaluateLayer,
    }
}

fn estimate_work_us(kind: TerrainWorkKind, level: u8) -> u64 {
    let level_cost = 1u64 << level.min(12);
    match kind {
        TerrainWorkKind::RasterizeVector => 40 + level_cost,
        TerrainWorkKind::SimulateBatch => 250 + level_cost * 4,
        _ => 80 + level_cost * 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heightfield::HeightfieldMetrics;
    use crate::layer::{Layer, PathNode, PathParams};

    #[test]
    fn local_path_edit_creates_sparse_prioritized_work() {
        let mut stack = LayerStack::new();
        let path = Layer::new(
            "Road",
            LayerKind::Path(PathParams {
                nodes: vec![
                    PathNode {
                        u: 0.1,
                        v: 0.1,
                        height: 0.0,
                        width: 1.0,
                    },
                    PathNode {
                        u: 0.2,
                        v: 0.2,
                        height: 20.0,
                        width: 1.0,
                    },
                ],
                width: 20.0,
                falloff: 10.0,
                ..PathParams::default()
            }),
        );
        let id = path.id();
        stack.push(path);
        let mut runtime = TerrainRuntime::new(PyramidConfig::new(1024, 4096.0, 4096.0));
        let count = runtime.invalidate_layer(&stack, id);
        let full_tiles: usize = runtime
            .pyramid
            .levels
            .iter()
            .map(|level| (level.metrics.tiles_x() * level.metrics.tiles_z()) as usize)
            .sum();
        assert!(count > 0);
        assert!(count < full_tiles);
        assert_eq!(runtime.scheduler.len(), count);
    }

    #[test]
    fn superseded_tile_completion_is_not_published() {
        let mut stack = LayerStack::new();
        let path = Layer::new(
            "Road",
            LayerKind::Path(PathParams {
                nodes: vec![
                    PathNode {
                        u: 0.1,
                        v: 0.1,
                        height: 0.0,
                        width: 1.0,
                    },
                    PathNode {
                        u: 0.2,
                        v: 0.2,
                        height: 10.0,
                        width: 1.0,
                    },
                ],
                ..PathParams::default()
            }),
        );
        let id = path.id();
        stack.push(path);
        let mut runtime = TerrainRuntime::new(PyramidConfig::new(512, 4096.0, 4096.0));
        runtime.invalidate_layer(&stack, id);
        let old = runtime
            .take_frame_work_matching(|work| work.kind == TerrainWorkKind::RasterizeVector)
            .into_iter()
            .next()
            .expect("old tile work");
        runtime.invalidate_layer(&stack, id);
        assert!(!runtime.publish_completed(&old, 1, 1));
    }

    #[test]
    fn reconfigure_rebuilds_levels_without_payload_assumptions() {
        let mut runtime = TerrainRuntime::new(PyramidConfig::new(512, 1024.0, 1024.0));
        runtime.reconfigure(PyramidConfig::new(1000, 8000.0, 4000.0));
        assert_eq!(runtime.pyramid.levels.last().unwrap().resolution, 1000);
        assert_eq!(
            runtime.pyramid.levels.last().unwrap().metrics,
            HeightfieldMetrics {
                width: 1000,
                height: 1000,
                world_size_x: 8000.0,
                world_size_z: 4000.0,
                tile_size: runtime.pyramid.config.tile_size.min(1000),
                halo: runtime.pyramid.config.halo,
            }
        );
    }
    #[test]
    fn brush_region_invalidates_only_touched_sculpt_tiles() {
        let mut stack = LayerStack::new();
        let sculpt = Layer::new(
            "Sculpt",
            LayerKind::SculptBase(crate::layer::SculptParams::filled(64, 0.0)),
        );
        let id = sculpt.id();
        stack.push(sculpt);
        let mut runtime = TerrainRuntime::new(PyramidConfig::new(1024, 4096.0, 4096.0));
        let region = RegionSet::from_rect(
            super::super::NormalizedRect::new(0.47, 0.47, 0.53, 0.53).unwrap(),
        );
        let count = runtime.invalidate_layer_regions(&stack, id, region);
        let full_tiles: usize = runtime
            .pyramid
            .levels
            .iter()
            .map(|level| (level.metrics.tiles_x() * level.metrics.tiles_z()) as usize)
            .sum();
        assert!(count > 0);
        assert!(count < full_tiles);
        assert_eq!(runtime.scheduler.len(), count);
    }
}
