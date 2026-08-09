use crate::fields::FieldId;
use crate::heightfield::TileId;
use crate::layer::LayerId;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TerrainTileKey {
    pub layer: Option<LayerId>,
    pub field: FieldId,
    pub level: u8,
    pub tile: TileId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerrainWorkKind {
    RasterizeVector,
    Upsample,
    EvaluateLayer,
    ExchangeHalo,
    SimulateBatch,
    ResolveBasinBoundary,
    GenerateNormals,
    Publish,
    Export,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkPriority {
    pub urgency: u8,
    pub coarse_first: u8,
    pub proximity: u8,
    pub age: u32,
}

impl WorkPriority {
    pub fn interactive(level: u8, proximity: u8) -> Self {
        Self {
            urgency: 255,
            coarse_first: u8::MAX - level,
            proximity,
            age: 0,
        }
    }

    pub fn background(level: u8) -> Self {
        Self {
            urgency: 32,
            coarse_first: u8::MAX - level,
            proximity: 0,
            age: 0,
        }
    }
}

impl Ord for WorkPriority {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.urgency, self.coarse_first, self.proximity, self.age).cmp(&(
            other.urgency,
            other.coarse_first,
            other.proximity,
            other.age,
        ))
    }
}

impl PartialOrd for WorkPriority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone)]
pub struct TerrainWorkItem {
    pub id: u64,
    pub kind: TerrainWorkKind,
    pub key: TerrainTileKey,
    pub layer: Option<LayerId>,
    pub revision: u64,
    pub estimated_us: u64,
    pub priority: WorkPriority,
}

#[derive(Debug, Clone)]
struct QueuedWork(TerrainWorkItem);

impl PartialEq for QueuedWork {
    fn eq(&self, other: &Self) -> bool {
        self.0.id == other.0.id
    }
}

impl Eq for QueuedWork {}

impl Ord for QueuedWork {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0
            .priority
            .cmp(&other.0.priority)
            .then_with(|| other.0.id.cmp(&self.0.id))
    }
}

impl PartialOrd for QueuedWork {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkSchedulerStats {
    pub queued: usize,
    pub popped: u64,
    pub cancelled: u64,
    pub stale_discarded: u64,
    pub estimated_us_dispatched: u64,
}

/// Priority queue with revision-based cancellation and budget-aware dequeue.
#[derive(Debug, Default)]
pub struct TerrainWorkScheduler {
    queue: BinaryHeap<QueuedWork>,
    cancelled: HashSet<u64>,
    live_layer_revision: HashMap<LayerId, u64>,
    next_id: u64,
    stats: WorkSchedulerStats,
}

impl TerrainWorkScheduler {
    pub fn enqueue(&mut self, mut item: TerrainWorkItem) -> u64 {
        if item.id == 0 {
            self.next_id = self.next_id.wrapping_add(1).max(1);
            item.id = self.next_id;
        }
        if let Some(layer) = item.layer {
            self.live_layer_revision
                .entry(layer)
                .and_modify(|revision| *revision = (*revision).max(item.revision))
                .or_insert(item.revision);
        }
        self.purge_obsolete();
        let id = item.id;
        self.queue.push(QueuedWork(item));
        self.stats.queued = self.queue.len();
        id
    }

    pub fn set_live_revision(&mut self, layer: LayerId, revision: u64) {
        self.live_layer_revision.insert(layer, revision);
        self.purge_obsolete();
    }

    pub fn cancel(&mut self, id: u64) {
        if self.cancelled.insert(id) {
            self.stats.cancelled = self.stats.cancelled.saturating_add(1);
        }
        self.purge_obsolete();
    }
    fn purge_obsolete(&mut self) {
        let queued = std::mem::take(&mut self.queue);
        for QueuedWork(item) in queued.into_vec() {
            let cancelled = self.cancelled.contains(&item.id);
            let superseded = item.layer.is_some_and(|layer| {
                self.live_layer_revision
                    .get(&layer)
                    .is_some_and(|revision| item.revision < *revision)
            });
            if superseded {
                self.stats.stale_discarded = self.stats.stale_discarded.saturating_add(1);
            }
            if !cancelled && !superseded {
                self.queue.push(QueuedWork(item));
            }
        }
        self.stats.queued = self.queue.len();
    }

    pub fn is_stale(&self, item: &TerrainWorkItem) -> bool {
        self.cancelled.contains(&item.id)
            || item.layer.is_some_and(|layer| {
                self.live_layer_revision
                    .get(&layer)
                    .is_some_and(|revision| item.revision < *revision)
            })
    }

    /// Select work that fits the current frame budget. A single oversized item may run only
    /// when nothing else was selected, preventing permanent starvation and exposing the overrun.
    pub fn pop_budgeted(&mut self, budget_us: u64) -> Vec<TerrainWorkItem> {
        self.pop_budgeted_matching(budget_us, |_| true)
    }

    /// Select only work supported by the current executor, preserving unmatched jobs.
    pub fn pop_budgeted_matching<F>(
        &mut self,
        budget_us: u64,
        mut accepts: F,
    ) -> Vec<TerrainWorkItem>
    where
        F: FnMut(&TerrainWorkItem) -> bool,
    {
        let mut selected = Vec::new();
        let mut deferred = Vec::new();
        let mut used = 0u64;
        while let Some(QueuedWork(item)) = self.queue.pop() {
            if self.is_stale(&item) {
                self.stats.stale_discarded = self.stats.stale_discarded.saturating_add(1);
                continue;
            }
            if !accepts(&item) {
                deferred.push(QueuedWork(item));
                continue;
            }
            let estimate = item.estimated_us.max(1);
            if !selected.is_empty() && used.saturating_add(estimate) > budget_us {
                deferred.push(QueuedWork(item));
                break;
            }
            used = used.saturating_add(estimate);
            self.stats.popped = self.stats.popped.saturating_add(1);
            selected.push(item);
            if used >= budget_us {
                break;
            }
        }
        self.queue.extend(deferred);
        self.stats.estimated_us_dispatched =
            self.stats.estimated_us_dispatched.saturating_add(used);
        self.stats.queued = self.queue.len();
        selected
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn stats(&self) -> WorkSchedulerStats {
        self.stats
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EditorRefinementState {
    ActiveInteraction,
    Settling,
    IdleRefinement,
    Export,
}

impl EditorRefinementState {
    pub fn terrain_budget_us(self) -> u64 {
        match self {
            Self::ActiveInteraction => 2_000,
            Self::Settling => 5_000,
            Self::IdleRefinement => 10_000,
            Self::Export => u64::MAX,
        }
    }

    pub fn simulation_iteration_cap(self) -> Option<u32> {
        match self {
            Self::ActiveInteraction => Some(4),
            Self::Settling => Some(24),
            Self::IdleRefinement => Some(256),
            Self::Export => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RefinementController {
    state: EditorRefinementState,
    last_interaction_ms: u64,
    settle_after_ms: u64,
    idle_after_ms: u64,
}

impl Default for RefinementController {
    fn default() -> Self {
        Self {
            state: EditorRefinementState::IdleRefinement,
            last_interaction_ms: 0,
            settle_after_ms: 80,
            idle_after_ms: 450,
        }
    }
}

impl RefinementController {
    pub fn state(&self) -> EditorRefinementState {
        self.state
    }

    pub fn begin_interaction(&mut self, now_ms: u64) {
        self.last_interaction_ms = now_ms;
        self.state = EditorRefinementState::ActiveInteraction;
    }

    pub fn begin_export(&mut self) {
        self.state = EditorRefinementState::Export;
    }

    pub fn finish_export(&mut self, now_ms: u64) {
        self.last_interaction_ms = now_ms;
        self.state = EditorRefinementState::Settling;
    }

    pub fn update(&mut self, now_ms: u64, interaction_active: bool) -> EditorRefinementState {
        if self.state == EditorRefinementState::Export {
            return self.state;
        }
        if interaction_active {
            self.begin_interaction(now_ms);
        } else {
            let elapsed = now_ms.saturating_sub(self.last_interaction_ms);
            self.state = if elapsed < self.settle_after_ms {
                EditorRefinementState::ActiveInteraction
            } else if elapsed < self.idle_after_ms {
                EditorRefinementState::Settling
            } else {
                EditorRefinementState::IdleRefinement
            };
        }
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: u64, layer: LayerId, revision: u64, urgency: u8) -> TerrainWorkItem {
        TerrainWorkItem {
            id,
            kind: TerrainWorkKind::EvaluateLayer,
            key: TerrainTileKey {
                layer: Some(layer),
                field: FieldId::Height,
                level: 2,
                tile: TileId { tx: 0, tz: 0 },
            },
            layer: Some(layer),
            revision,
            estimated_us: 100,
            priority: WorkPriority {
                urgency,
                coarse_first: 253,
                proximity: 0,
                age: 0,
            },
        }
    }

    #[test]
    fn scheduler_prioritizes_and_drops_stale_work() {
        let layer = LayerId::new();
        let mut scheduler = TerrainWorkScheduler::default();
        scheduler.enqueue(item(1, layer, 1, 10));
        scheduler.enqueue(item(2, layer, 2, 200));
        scheduler.set_live_revision(layer, 2);
        let selected = scheduler.pop_budgeted(100);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, 2);
        assert_eq!(scheduler.stats().stale_discarded, 1);
    }

    #[test]
    fn filtered_dispatch_preserves_work_for_other_executors() {
        let raster_layer = LayerId::new();
        let simulation_layer = LayerId::new();
        let mut raster = item(1, raster_layer, 1, 100);
        raster.kind = TerrainWorkKind::RasterizeVector;
        let mut simulation = item(2, simulation_layer, 1, 250);
        simulation.kind = TerrainWorkKind::SimulateBatch;
        let mut scheduler = TerrainWorkScheduler::default();
        scheduler.enqueue(raster);
        scheduler.enqueue(simulation);

        let selected = scheduler
            .pop_budgeted_matching(100, |work| work.kind == TerrainWorkKind::RasterizeVector);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].kind, TerrainWorkKind::RasterizeVector);
        assert_eq!(scheduler.len(), 1);
        assert_eq!(
            scheduler.pop_budgeted(100)[0].kind,
            TerrainWorkKind::SimulateBatch
        );
    }

    #[test]
    fn refinement_state_has_hysteresis() {
        let mut controller = RefinementController::default();
        controller.begin_interaction(100);
        assert_eq!(
            controller.update(120, false),
            EditorRefinementState::ActiveInteraction
        );
        assert_eq!(
            controller.update(250, false),
            EditorRefinementState::Settling
        );
        assert_eq!(
            controller.update(700, false),
            EditorRefinementState::IdleRefinement
        );
    }
}
