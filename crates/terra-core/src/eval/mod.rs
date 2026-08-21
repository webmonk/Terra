//! Layer evaluation, caching, and dirty propagation.

mod cache;
mod processors;
mod scheduler;
mod smart_cache;
mod worker;

pub use crate::quality::PreviewQuality;
pub use cache::{CachedOutput, LayerCache};
pub use processors::ProcessorRegistry;
pub use scheduler::{EvalJob, EvalScheduler};
pub use smart_cache::DiskSmartCache;
pub use worker::{EvalWorkRequest, EvalWorkResult, EvalWorker};

use crate::fields::AuxMaps;
use crate::heightfield::{Heightfield, HeightfieldMetrics};
use crate::layer::{blend_heights, Layer, LayerId, LayerStack, StackNode};
use crate::mask::{MaskAsset, MaskField, MaskId};
use std::collections::{HashMap, HashSet};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::Instant;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EvalError {
    #[error("unknown layer processor for kind")]
    UnknownProcessor,
    #[error("cancelled")]
    Cancelled,
    #[error("io: {0}")]
    Io(String),
}

/// How a layer contributed to a particular evaluation pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerEvalStatus {
    Disabled,
    CacheHit,
    Computed,
}

/// CPU timing and cache provenance for one artist-visible layer.
#[derive(Debug, Clone)]
pub struct LayerEvalTiming {
    pub layer: LayerId,
    pub layer_name: String,
    pub layer_kind: &'static str,
    pub elapsed_us: u64,
    pub status: LayerEvalStatus,
}

pub struct EvalContext {
    pub metrics: HeightfieldMetrics,
    /// Project-wide progressive evaluation controls. Keeping this on the
    /// context makes CPU, worker, and hybrid evaluation use the document's
    /// authored world scale and level schedule instead of hidden defaults.
    pub level_steps: crate::analyze::LevelStepSettings,
    pub masks: HashMap<MaskId, MaskField>,
    pub mask_assets: Vec<MaskAsset>,
    /// Typed aux maps (preferred). Processors should read/write these.
    pub aux_maps: AuxMaps,
    /// String-key adapter kept in sync with [`Self::aux_maps`] for cache / IO / masks.
    pub aux: HashMap<String, MaskField>,
    /// Stable outputs published by layers already evaluated below the current layer.
    pub published_outputs: HashMap<crate::layer::OutputId, MaskField>,
    pub cancelled: bool,
    /// Aux keys actually rewritten by layers computed in the current pass,
    /// detected by copy-on-write storage identity. Cache-hit restores skip
    /// these keys so a clean layer's snapshot can't clobber fresh values
    /// produced below it.
    pub(crate) pass_changed: HashSet<String>,
    /// Shared worker generation; a mismatch cancels at the next layer boundary.
    pub(crate) cancellation_generation: Option<(Arc<AtomicU64>, u64)>,
    pub quality: PreviewQuality,
    /// Timings for the current pass, in actual layer evaluation order.
    pub layer_timings: Vec<LayerEvalTiming>,
}

impl EvalContext {
    pub fn new(metrics: HeightfieldMetrics) -> Self {
        Self {
            metrics,
            level_steps: crate::analyze::LevelStepSettings::default(),
            masks: HashMap::new(),
            mask_assets: Vec::new(),
            aux_maps: AuxMaps::new(),
            aux: HashMap::new(),
            published_outputs: HashMap::new(),
            cancelled: false,
            pass_changed: HashSet::new(),
            quality: PreviewQuality::Full,
            cancellation_generation: None,
            layer_timings: Vec::new(),
        }
    }

    pub fn check_cancelled(&self) -> Result<(), EvalError> {
        let generation_changed = self
            .cancellation_generation
            .as_ref()
            .is_some_and(|(generation, expected)| generation.load(Ordering::Acquire) != *expected);
        if self.cancelled || generation_changed {
            Err(EvalError::Cancelled)
        } else {
            Ok(())
        }
    }

    pub fn set_cancellation_generation(&mut self, generation: Arc<AtomicU64>, expected: u64) {
        self.cancellation_generation = Some((generation, expected));
    }

    /// Insert an aux map into both typed and string stores.
    pub fn aux_insert(&mut self, key: impl Into<String>, field: MaskField) {
        let key = key.into();
        let canonical = crate::fields::keys::canonical(&key).to_string();
        self.aux_maps.insert(canonical.clone(), field.clone());
        if canonical == crate::fields::keys::SEDIMENT_THICKNESS {
            self.aux.remove(crate::fields::keys::SEDIMENT_DEPTH);
            self.aux.remove(crate::fields::keys::LOOSE_SEDIMENT);
        }
        self.aux.insert(canonical, field);
    }

    /// Replace string aux and rebuild typed maps (worker / scheduler ingest).
    /// Preserves any strata already on `aux_maps` when the HashMap has none.
    pub fn set_aux_hashmap(&mut self, aux: HashMap<String, MaskField>) {
        let keep_strata = self.aux_maps.strata.take();
        self.aux_maps = AuxMaps::from_hashmap_preserving_strata(&aux, keep_strata);
        self.sync_aux_hashmap();
    }

    /// Push typed maps into the string HashMap adapter (strata stays on `aux_maps`).
    pub fn sync_aux_hashmap(&mut self) {
        self.aux = self.aux_maps.to_hashmap();
    }

    /// Ensure slope/curvature derived caches exist for the current heightfield.
    pub fn ensure_derived_fields(&mut self, hf: &Heightfield) {
        self.aux_maps.ensure_derived(hf);
        self.sync_aux_hashmap();
    }
}

pub struct StackEvaluator {
    pub registry: ProcessorRegistry,
    pub cache: LayerCache,
    /// Checkpoints of scrubbed simulation outputs keyed by progress bucket,
    /// validated by input + param fingerprints. Revisiting a scrub position
    /// replays the stored result instead of re-running the sim.
    scrub_cache: HashMap<LayerId, Vec<ScrubEntry>>,
    /// Scrub-checkpoint reuse count (observable for tests / diagnostics).
    pub scrub_hits: u64,
}

struct ScrubEntry {
    bucket: u8,
    input_fp: u64,
    params_fp: u64,
    height: Heightfield,
    aux: HashMap<String, MaskField>,
    strata: Option<Vec<crate::layer::Stratum>>,
    /// Aux keys this sim actually rewrote (runtime CoW diff at capture time).
    wrote: HashSet<String>,
}

const SCRUB_ENTRIES_PER_LAYER: usize = 16;

/// Full-content hash of a heightfield's interior samples.
///
/// Scrub checkpoints must not replay against a *different* input, and the
/// sparse [`height_fingerprint`] sample grid can miss a localized edit
/// entirely (a sculpt stroke narrower than the sample stride), so
/// checkpoint validation hashes every sample. The cost is negligible
/// against re-running the simulation the checkpoint exists to avoid.
fn height_content_hash(hf: &Heightfield) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for tile in hf.tiles() {
        for &v in tile.interior() {
            hash ^= v.to_bits() as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

/// Cheap parameter identity for scrub-checkpoint validation: any edit beyond
/// the progress scrub itself changes the fingerprint and misses the cache.
fn scrub_params_fingerprint(layer: &Layer) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut mix = |v: u64| {
        hash ^= v;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    };
    if let Ok(json) = serde_json::to_string(&layer.kind) {
        for b in json.as_bytes() {
            mix(*b as u64);
        }
    }
    mix(layer.common.opacity.to_bits() as u64);
    mix(layer.common.blend as u64);
    mix(layer.common.masks.len() as u64);
    mix(layer.common.clip_to_below as u64);
    hash
}

impl Default for StackEvaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl StackEvaluator {
    pub fn new() -> Self {
        Self {
            registry: ProcessorRegistry::builtin(),
            cache: LayerCache::new(),
            scrub_cache: HashMap::new(),
            scrub_hits: 0,
        }
    }

    /// Field-aware suffix invalidation: dirty the edited layer, then only
    /// the layers above it that the edit can actually reach - an affected
    /// height entering them, an overlap between their field contract
    /// (reads or writes) and the channels changed below, or a mask/binding
    /// that may read arbitrary channels. A vegetation tweak under a blur no
    /// longer re-runs the blur.
    pub fn mark_dirty_from(&mut self, stack: &LayerStack, id: LayerId) {
        let layers = stack.flatten_layers();
        let Some(start) = layers.iter().position(|l| l.id() == id) else {
            self.mark_all_dirty(stack);
            return;
        };
        let mut changed: HashSet<String> = HashSet::new();
        let mut height_changed = false;
        for (i, layer) in layers.iter().enumerate().skip(start) {
            let affected = i == start
                || height_changed
                || layer_contract_touches(layer, &changed)
                || (!changed.is_empty()
                    && (!layer.common.masks.is_empty()
                        || !layer.common.param_bindings.is_empty()));
            if !affected {
                continue;
            }
            self.cache.mark_dirty(layer.id());
            for field in layer
                .kind
                .modified_fields()
                .into_iter()
                .chain(layer.kind.produced_fields())
            {
                if field == crate::fields::FieldId::Height {
                    height_changed = true;
                } else {
                    changed.insert(field.cache_key());
                }
            }
        }
    }

    /// Stage-aware dirty: only invalidate this layer and later EvalStages.
    ///
    /// Material edits do not rebuild height (Blueprint / PreBiome / Hydro).
    /// Vegetation / scatter edits do not rebuild materials.
    pub fn mark_dirty_from_stage(&mut self, stack: &LayerStack, id: LayerId) {
        let Some(layer) = stack.find(id) else {
            self.mark_dirty_from(stack, id);
            return;
        };
        let min_order = layer.kind.eval_stage().order();
        self.cache.mark_dirty(id);
        for lid in stack.layer_ids() {
            if lid == id {
                continue;
            }
            if let Some(other) = stack.find(lid) {
                if other.kind.eval_stage().order() >= min_order {
                    self.cache.mark_dirty(lid);
                }
            }
        }
    }

    /// Dirty all layers at or after an EvalStage (World Rule selective invalidation).
    pub fn mark_dirty_from_eval_stage(
        &mut self,
        stack: &LayerStack,
        stage: crate::landscape_blueprint::EvalStage,
    ) {
        let min_order = stage.order();
        for lid in stack.layer_ids() {
            if let Some(layer) = stack.find(lid) {
                if layer.kind.eval_stage().order() >= min_order {
                    self.cache.mark_dirty(lid);
                }
            }
        }
    }

    pub fn mark_all_dirty(&mut self, stack: &LayerStack) {
        for id in stack.layer_ids() {
            self.cache.mark_dirty(id);
        }
    }

    /// Discard every layer cache entry (project switch / hard reset).
    pub fn clear_project_caches(&mut self) {
        self.cache.clear();
        self.scrub_cache.clear();
    }

    /// Full rebuild (Phase 1 path) - tree walk so scoped groups compose correctly.
    pub fn rebuild_all(
        &mut self,
        stack: &LayerStack,
        ctx: &mut EvalContext,
    ) -> Result<Heightfield, EvalError> {
        profiling::scope!("rebuild_all");
        self.cache.clear();
        ctx.pass_changed.clear();
        let seed = Heightfield::zeros(ctx.metrics);
        self.evaluate_nodes(&stack.nodes, ctx, &seed)
    }

    /// Incremental rebuild from first dirty layer (Phase 4).
    ///
    /// Flat stacks use suffix-only evaluation from the first dirty layer. Scoped
    /// groups and solo mode use the same dirty-aware tree walk as [`evaluate_nodes`].
    pub fn rebuild_incremental(
        &mut self,
        stack: &LayerStack,
        ctx: &mut EvalContext,
    ) -> Result<Heightfield, EvalError> {
        profiling::scope!("rebuild_incremental");
        ctx.pass_changed.clear();
        if stack.requires_tree_evaluation() {
            let seed = Heightfield::zeros(ctx.metrics);
            return self.evaluate_nodes(&stack.nodes, ctx, &seed);
        }

        let layers: Vec<&Layer> = stack.flatten_layers();
        if layers.is_empty() {
            return Ok(Heightfield::zeros(ctx.metrics));
        }

        let first_dirty = layers.iter().position(|l| {
            if l.common.cached {
                self.cache.get_or_load(l.id(), ctx.metrics).is_none()
            } else {
                self.cache.is_dirty(l.id())
            }
        });

        // All clean: return cached top if dimensions match.
        if first_dirty.is_none() {
            if let Some(top) = self.cache.get(layers.last().unwrap().id()) {
                if top.height.metrics.width == ctx.metrics.width
                    && top.height.metrics.height == ctx.metrics.height
                    && !top.dirty
                {
                    for layer in &layers {
                        record_reused_layer(ctx, layer);
                    }
                    return Ok(top.height.clone());
                }
            }
        }

        let mut first_dirty = first_dirty.unwrap_or(0);
        // A dirty clipped layer needs its base's mask; walk back to the
        // nearest non-clipped layer so the base re-emits it (cache hit).
        while first_dirty > 0 && layers[first_dirty].common.clip_to_below {
            first_dirty -= 1;
        }
        for layer in &layers[..first_dirty] {
            record_reused_layer(ctx, layer);
        }

        let mut current = if first_dirty == 0 {
            Heightfield::zeros(ctx.metrics)
        } else {
            let prev_id = layers[first_dirty - 1].id();
            self.cache
                .get_or_load(prev_id, ctx.metrics)
                .map(|c| c.height.clone())
                .unwrap_or_else(|| Heightfield::zeros(ctx.metrics))
        };

        let track_bases = layers[first_dirty..]
            .iter()
            .any(|l| l.common.clip_to_below);
        let mut clip_base: Option<MaskField> = None;
        for layer in &layers[first_dirty..] {
            ctx.check_cancelled()?;
            let base = if layer.common.clip_to_below {
                clip_base.as_ref()
            } else {
                None
            };
            let want_mask = track_bases && !layer.common.clip_to_below;
            let (out, mask) = self.evaluate_layer(ctx, &current, layer, base, want_mask)?;
            current = out;
            if !layer.common.clip_to_below {
                clip_base = mask;
            }
            self.store_cached(layer.id(), &current, ctx, layer.common.cached);
        }

        Ok(current)
    }

    /// Evaluate a node list bottom->top, composing scoped groups as units.
    pub fn evaluate_nodes(
        &mut self,
        nodes: &[StackNode],
        ctx: &mut EvalContext,
        input: &Heightfield,
    ) -> Result<Heightfield, EvalError> {
        let mut current = input.clone();
        let soloing = nodes.iter().any(node_contains_solo);
        let track_bases = nodes
            .iter()
            .any(|n| matches!(n, StackNode::Layer(l) if l.common.clip_to_below));
        // Baked mask of the nearest evaluated non-clipped sibling layer:
        // clipped layers restrict their effect to where it applied.
        let mut clip_base: Option<MaskField> = None;
        for node in nodes {
            ctx.check_cancelled()?;
            if soloing && !node_contains_solo(node) {
                clip_base = None;
                continue;
            }
            match node {
                StackNode::Layer(layer) => {
                    let base = if layer.common.clip_to_below {
                        clip_base.as_ref()
                    } else {
                        None
                    };
                    let want_mask = track_bases && !layer.common.clip_to_below;
                    let (out, mask) = self.evaluate_layer(ctx, &current, layer, base, want_mask)?;
                    current = out;
                    if !layer.common.clip_to_below {
                        clip_base = mask;
                    }
                    self.store_cached(layer.id(), &current, ctx, layer.common.cached);
                }
                StackNode::Group(group) if !group.enabled => {}
                StackNode::Group(group) => {
                    use crate::layer::{GroupEvalMode, GroupInputMode};

                    refresh_point_of_use_masks(ctx, &current);
                    let pass_through =
                        matches!(group.eval_mode, GroupEvalMode::PassThrough) && !group.is_scoped();

                    if pass_through {
                        // Organisational folder: children mutate the live context.
                        current = self.evaluate_nodes(&group.children, ctx, &current)?;
                    } else {
                        // Isolated composite: private working height, then mix back.
                        let private_seed = match &group.input_mode {
                            GroupInputMode::CopyInput => current.clone(),
                            GroupInputMode::EmptyHeight => Heightfield::zeros(ctx.metrics),
                            GroupInputMode::SelectedField(_) => current.clone(),
                        };
                        // Snapshot aux so child sims don't leak into the parent
                        // until after the group composite.
                        let aux_snapshot = ctx.aux_maps.clone();
                        let aux_hash_snapshot = ctx.aux.clone();
                        let descendant_ids = collect_descendant_layer_ids(&group.children);
                        let (group_out, child_aux) = if let Some((height, aux)) = self
                            .try_reuse_group_cache(group.id, ctx, &descendant_ids, &private_seed)
                        {
                            record_subtree_cache_hits(ctx, &group.children);
                            (height, aux)
                        } else {
                            let group_out =
                                self.evaluate_nodes(&group.children, ctx, &private_seed)?;
                            let child_aux = ctx.aux_maps.clone();
                            self.store_group_cached(
                                group.id,
                                &group_out,
                                &child_aux,
                                &private_seed,
                                ctx,
                                group.cache_policy.to_legacy_cached(),
                            );
                            (group_out, child_aux)
                        };
                        // Restore parent aux, then selectively merge published child aux
                        // under the group mask after height composite.
                        ctx.aux_maps = aux_snapshot;
                        ctx.aux = aux_hash_snapshot;
                        ctx.sync_aux_hashmap();

                        let mask = effective_layer_mask(ctx, &group.masks, &current);
                        // Biome Filters blend toward lower biomes at `filter_blending`
                        // (1.0 = full mix, 0.0 = no contribution) rather than a hard cut.
                        // Height-delta semantics: with CopyInput, Normal mix is equivalent to
                        //   H = shared + w * (biome_result - shared)
                        // which avoids blending unrelated absolute heights.
                        let mix_opacity =
                            if matches!(group.group_kind, crate::layer::GroupKind::Biome) {
                                group.opacity * group.filter_blending
                            } else {
                                group.opacity
                            };
                        current = if matches!(group.group_kind, crate::layer::GroupKind::Biome)
                            && matches!(group.input_mode, GroupInputMode::CopyInput)
                        {
                            mix_height_delta(
                                &current,
                                &private_seed,
                                &group_out,
                                mix_opacity,
                                &mask,
                            )
                        } else {
                            mix_heightfields(&current, &group_out, group.blend, mix_opacity, &mask)
                        };
                        // Merge child aux weighted by group mask (non-destructive leak fix).
                        // Biomes keep plain interpolation (height uses delta semantics);
                        // other isolated groups honor the group blend for weight channels.
                        let aux_blend =
                            if matches!(group.group_kind, crate::layer::GroupKind::Biome) {
                                crate::layer::BlendMode::Interpolate
                            } else {
                                group.blend
                            };
                        merge_aux_masked(ctx, &child_aux, &mask, mix_opacity, aux_blend);
                    }
                }
            }
            // Groups reset the clip base: clipped layers only chain to a
            // sibling layer, never across a group boundary.
            if matches!(node, StackNode::Group(_)) {
                clip_base = None;
            }
        }
        Ok(current)
    }

    /// Continue evaluating a flattened stack from a precomputed heightfield.
    ///
    /// `current` must be the height entering `start_index`, and `ctx` must contain the
    /// equivalent auxiliary and published-output state produced by the skipped prefix.
    /// Callers that cannot supply that complete checkpoint must restart from layer zero.
    pub fn evaluate_suffix(
        &mut self,
        stack: &LayerStack,
        ctx: &mut EvalContext,
        start_index: usize,
        mut current: Heightfield,
    ) -> Result<Heightfield, EvalError> {
        // A suffix run is a fresh pass: no layer below it was computed here,
        // so nothing is pending-restore from this context.
        ctx.pass_changed.clear();
        let layers = stack.flatten_layers();
        let track_bases = layers
            .iter()
            .skip(start_index)
            .any(|l| l.common.clip_to_below);
        // Suffix evaluation only sees bases inside the suffix: a clipped
        // layer at the very start evaluates unclipped (callers restart from
        // the base when its mask matters).
        let mut clip_base: Option<MaskField> = None;
        for layer in layers.into_iter().skip(start_index) {
            ctx.check_cancelled()?;
            let base = if layer.common.clip_to_below {
                clip_base.as_ref()
            } else {
                None
            };
            let want_mask = track_bases && !layer.common.clip_to_below;
            let (out, mask) = self.evaluate_layer(ctx, &current, layer, base, want_mask)?;
            current = out;
            if !layer.common.clip_to_below {
                clip_base = mask;
            }
            self.store_cached(layer.id(), &current, ctx, layer.common.cached);
        }
        Ok(current)
    }

    fn store_cached(&mut self, id: LayerId, height: &Heightfield, ctx: &EvalContext, baked: bool) {
        let output = CachedOutput {
            height: height.clone(),
            generation: self.cache.generation,
            dirty: false,
            aux: ctx.aux_maps.to_hashmap(),
            strata: ctx.aux_maps.strata.clone(),
        };
        if baked {
            self.cache.insert_baked(id, output);
        } else {
            self.cache.insert(id, output);
        }
    }

    /// Cache an isolated group's private composite keyed by its input fingerprint.
    fn store_group_cached(
        &mut self,
        id: LayerId,
        height: &Heightfield,
        child_aux: &crate::fields::AuxMaps,
        input: &Heightfield,
        _ctx: &EvalContext,
        baked: bool,
    ) {
        let output = CachedOutput {
            height: height.clone(),
            generation: height_fingerprint(input),
            dirty: false,
            aux: child_aux.to_hashmap(),
            strata: child_aux.strata.clone(),
        };
        if baked {
            self.cache.insert_baked(id, output);
        } else {
            self.cache.insert(id, output);
        }
    }

    /// Evaluate one layer. `clip_base` is the baked mask of the nearest
    /// non-clipped sibling below (clipping-mask semantics); the returned mask
    /// is the layer's own effective mask, produced only when `want_mask` so
    /// cache hits stay free when no sibling clips.
    fn evaluate_layer(
        &mut self,
        ctx: &mut EvalContext,
        input: &Heightfield,
        layer: &Layer,
        clip_base: Option<&MaskField>,
        want_mask: bool,
    ) -> Result<(Heightfield, Option<MaskField>), EvalError> {
        let timing_started = Instant::now();
        if !layer.common.enabled {
            record_layer_timing(ctx, layer, timing_started, LayerEvalStatus::Disabled);
            // A disabled base applied nowhere, so clipped layers above see zero.
            let mask = want_mask.then(|| MaskField::zeros(ctx.metrics));
            return Ok((input.clone(), mask));
        }

        // Terrain-aware and runtime masks are evaluated against the exact field
        // entering their owner. This makes placement deterministic in preview,
        // export, and cold evaluation instead of depending on a previous frame.
        refresh_point_of_use_masks(ctx, input);

        // Any clean cached checkpoint reuses height + aux without re-invoking
        // the processor. Keys rewritten by layers computed earlier this pass
        // are NOT restored - the snapshot predates those writes, and this
        // clean layer was judged unaffected by them.
        if !self.cache.is_dirty(layer.id()) {
            if let Some(cached) = self.cache.get_or_load(layer.id(), ctx.metrics) {
                if ctx.pass_changed.is_empty() {
                    ctx.aux_maps.extend_hashmap(&cached.aux);
                } else {
                    for (key, field) in &cached.aux {
                        if !ctx.pass_changed.contains(key) {
                            ctx.aux_maps.insert(key.clone(), field.clone());
                        }
                    }
                }
                if cached.strata.is_some() {
                    ctx.aux_maps.strata = cached.strata.clone();
                }
                ctx.sync_aux_hashmap();
                publish_layer_outputs(ctx, layer, &cached.height);
                record_layer_timing(ctx, layer, timing_started, LayerEvalStatus::CacheHit);
                // Point-of-use masks were refreshed against this exact input,
                // so rebaking here reproduces the mask the hit was built with.
                let mask =
                    want_mask.then(|| effective_layer_mask(ctx, &layer.common.masks, input));
                return Ok((cached.height.clone(), mask));
            }
        }

        // Scrub checkpoints: a revisited progress position replays the
        // stored result instead of re-running the sim, validated against the
        // exact input and parameter state it was captured from.
        let scrubbing = layer.common.sim_progress < 0.999
            && matches!(
                layer.kind.category(),
                crate::layer::OperationCategory::Simulation
            );
        let scrub_key = scrubbing.then(|| {
            (
                (layer.common.sim_progress.clamp(0.0, 1.0) * 100.0).round() as u8,
                height_content_hash(input),
                scrub_params_fingerprint(layer),
            )
        });
        if let Some((bucket, input_fp, params_fp)) = scrub_key {
            if let Some(entry) = self.scrub_cache.get(&layer.id()).and_then(|entries| {
                entries.iter().find(|e| {
                    e.bucket == bucket && e.input_fp == input_fp && e.params_fp == params_fp
                })
            }) {
                if ctx.pass_changed.is_empty() {
                    for (key, field) in &entry.aux {
                        ctx.aux_maps.insert(key.clone(), field.clone());
                    }
                } else {
                    for (key, field) in &entry.aux {
                        if !ctx.pass_changed.contains(key) {
                            ctx.aux_maps.insert(key.clone(), field.clone());
                        }
                    }
                }
                if entry.strata.is_some() {
                    ctx.aux_maps.strata = entry.strata.clone();
                }
                ctx.sync_aux_hashmap();
                ctx.pass_changed.extend(entry.wrote.iter().cloned());
                publish_layer_outputs(ctx, layer, &entry.height);
                record_layer_timing(ctx, layer, timing_started, LayerEvalStatus::CacheHit);
                self.scrub_hits += 1;
                let mask =
                    want_mask.then(|| effective_layer_mask(ctx, &layer.common.masks, input));
                return Ok((entry.height.clone(), mask));
            }
        }
        let pass_changed_before = scrub_key
            .is_some()
            .then(|| ctx.pass_changed.clone());

        let scaled_layer = layer_with_world_scale(layer, ctx.level_steps.world_scale);
        let mut bound_layer = apply_param_bindings(ctx, &scaled_layer);
        // Simulation timeline scrub: scale the iteration budget through
        // param reflection so any sim exposing an `iterations` knob can be
        // scrubbed like a timeline (the document keeps the full budget).
        if layer.common.sim_progress < 0.999
            && matches!(
                layer.kind.category(),
                crate::layer::OperationCategory::Simulation
            )
        {
            if let Some(iters) =
                crate::layer::param_reflect::get_param_f32(&bound_layer.kind, "iterations")
            {
                let scaled = (iters * layer.common.sim_progress.max(0.0)).round().max(1.0);
                let _ = crate::layer::param_reflect::set_param_f32(
                    &mut bound_layer.kind,
                    "iterations",
                    scaled,
                );
            }
        }
        // Scoped layers (mask or partial opacity): snapshot aux before the
        // processor so its channel writes composite under the same weight as
        // height. Copy-on-write storage makes the snapshot a refcount bump.
        let scoped = !layer.common.masks.is_empty()
            || layer.common.opacity < 0.999
            || layer.common.clip_to_below;
        let aux_before = scoped.then(|| ctx.aux_maps.clone());
        // CoW identity snapshot: after the processor runs, any key whose
        // buffer pointer changed was rewritten this pass (refcount bumps
        // only - no data copies).
        let pass_before = ctx.aux_maps.to_hashmap();
        let generated = self.registry.evaluate(ctx, input, &bound_layer)?;
        // Avoid unused-mut warning if future passes mutate further.
        let _ = &mut bound_layer;
        let mut mask = effective_layer_mask(ctx, &layer.common.masks, input);
        if layer.common.clip_to_below {
            if let Some(base) = clip_base {
                if base.metrics.width == mask.metrics.width
                    && base.metrics.height == mask.metrics.height
                {
                    let base_data = base.data();
                    for (v, b) in mask.data_mut().iter_mut().zip(base_data) {
                        *v *= *b;
                    }
                }
            }
        }
        if let Some(before) = aux_before {
            scope_aux_writes(ctx, &before, &mask, layer.common.opacity);
        }
        for (key, field) in ctx.aux_maps.to_hashmap() {
            if pass_before
                .get(&key)
                .is_none_or(|b| !b.shares_storage(&field))
            {
                ctx.pass_changed.insert(key);
            }
        }
        let out = mix_heightfields(
            input,
            &generated,
            layer.common.blend,
            layer.common.opacity,
            &mask,
        );
        publish_layer_outputs(ctx, layer, &out);
        record_layer_timing(ctx, layer, timing_started, LayerEvalStatus::Computed);
        if let (Some((bucket, input_fp, params_fp)), Some(before)) =
            (scrub_key, pass_changed_before)
        {
            let wrote: HashSet<String> = ctx
                .pass_changed
                .difference(&before)
                .cloned()
                .collect();
            let entries = self.scrub_cache.entry(layer.id()).or_default();
            entries.retain(|e| {
                !(e.bucket == bucket && e.input_fp == input_fp && e.params_fp == params_fp)
            });
            if entries.len() >= SCRUB_ENTRIES_PER_LAYER {
                entries.remove(0);
            }
            entries.push(ScrubEntry {
                bucket,
                input_fp,
                params_fp,
                height: out.clone(),
                aux: ctx.aux_maps.to_hashmap(),
                strata: ctx.aux_maps.strata.clone(),
                wrote,
            });
        }
        Ok((out, want_mask.then_some(mask)))
    }

    fn try_reuse_group_cache(
        &mut self,
        group_id: LayerId,
        ctx: &EvalContext,
        descendant_ids: &[LayerId],
        input: &Heightfield,
    ) -> Option<(Heightfield, crate::fields::AuxMaps)> {
        if self.cache.is_dirty(group_id) {
            return None;
        }
        if descendant_ids.iter().any(|&id| self.cache.is_dirty(id)) {
            return None;
        }
        let cached = self.cache.get_or_load(group_id, ctx.metrics)?;
        if cached.generation != height_fingerprint(input) {
            return None;
        }
        let child_aux = crate::fields::AuxMaps::from_hashmap_preserving_strata(
            &cached.aux,
            cached.strata.clone(),
        );
        Some((cached.height.clone(), child_aux))
    }
}

fn collect_descendant_layer_ids(nodes: &[StackNode]) -> Vec<LayerId> {
    let mut ids = Vec::new();
    collect_descendant_layer_ids_into(nodes, &mut ids);
    ids
}

fn collect_descendant_layer_ids_into(nodes: &[StackNode], out: &mut Vec<LayerId>) {
    for node in nodes {
        match node {
            StackNode::Layer(layer) => out.push(layer.id()),
            StackNode::Group(group) if group.enabled => {
                collect_descendant_layer_ids_into(&group.children, out);
            }
            StackNode::Group(_) => {}
        }
    }
}

fn record_subtree_cache_hits(ctx: &mut EvalContext, nodes: &[StackNode]) {
    let soloing = nodes.iter().any(node_contains_solo);
    for node in nodes {
        if soloing && !node_contains_solo(node) {
            continue;
        }
        match node {
            StackNode::Layer(layer) if layer.common.enabled => record_reused_layer(ctx, layer),
            StackNode::Group(group) if group.enabled => {
                record_subtree_cache_hits(ctx, &group.children);
            }
            _ => {}
        }
    }
}

fn record_layer_timing(
    ctx: &mut EvalContext,
    layer: &Layer,
    started: Instant,
    status: LayerEvalStatus,
) {
    ctx.layer_timings.push(LayerEvalTiming {
        layer: layer.id(),
        layer_name: layer.common.name.clone(),
        layer_kind: layer.kind.type_display_name(),
        elapsed_us: started.elapsed().as_micros() as u64,
        status,
    });
}

fn record_reused_layer(ctx: &mut EvalContext, layer: &Layer) {
    ctx.layer_timings.push(LayerEvalTiming {
        layer: layer.id(),
        layer_name: layer.common.name.clone(),
        layer_kind: layer.kind.type_display_name(),
        elapsed_us: 0,
        status: LayerEvalStatus::CacheHit,
    });
}

fn node_contains_solo(node: &StackNode) -> bool {
    match node {
        StackNode::Layer(layer) => layer.common.solo,
        StackNode::Group(group) => group.children.iter().any(node_contains_solo),
    }
}

fn refresh_point_of_use_masks(ctx: &mut EvalContext, input: &Heightfield) {
    let assets: Vec<_> = ctx
        .mask_assets
        .iter()
        .filter(|asset| mask_source_is_point_of_use(&asset.source))
        .cloned()
        .collect();
    if assets.is_empty() {
        return;
    }
    let rebaked = crate::mask::bake_mask_assets_resolved(
        &assets,
        input,
        input.metrics,
        &ctx.aux,
        &ctx.published_outputs,
    );
    ctx.masks.extend(rebaked);
}

fn mask_source_is_point_of_use(source: &crate::mask::MaskSource) -> bool {
    use crate::mask::MaskSource::*;
    matches!(
        source,
        Height { .. }
            | Slope { .. }
            | Aspect { .. }
            | Curvature { .. }
            | Convexity
            | Concavity
            | AmbientOcclusion { .. }
            | DistanceField { .. }
            | Named(_)
            | FlowDirection
            | FlowAccumulation { .. }
            | Wetness
            | Sediment
            | Erosion
            | Deposition
            | Hardness
            | Temperature
            | Rainfall
            | Humidity
            | Snow
            | SoilMoisture
            | WindExposure
            | LayerOutput { .. }
    )
}

fn apply_param_bindings(ctx: &EvalContext, layer: &Layer) -> Layer {
    if layer.common.param_bindings.is_empty() {
        return layer.clone();
    }
    let mut out = layer.clone();
    for binding in &layer.common.param_bindings {
        let sample = sample_binding_source(ctx, &binding.source);
        if binding.target.0 == "opacity" {
            out.common.opacity = binding
                .apply_scalar(layer.common.opacity, sample)
                .clamp(0.0, 1.0);
        } else {
            out.kind.apply_param_binding(binding, sample);
        }
    }
    out
}

fn sample_binding_source(ctx: &EvalContext, source: &crate::layer::BindingSource) -> f32 {
    use crate::layer::BindingSource;
    match source {
        BindingSource::Constant(v) => v.clamp(0.0, 1.0),
        BindingSource::Mask(id) => mean_mask(ctx.masks.get(id)),
        BindingSource::LayerOutput(id) | BindingSource::GroupOutput(id) => {
            mean_mask(ctx.published_outputs.get(id))
        }
        BindingSource::Field(field) => mean_mask(ctx.aux_maps.get(&field.cache_key())),
    }
}

fn mean_mask(field: Option<&MaskField>) -> f32 {
    let Some(f) = field else {
        return 0.0;
    };
    let w = f.metrics.width;
    let h = f.metrics.height;
    if w == 0 || h == 0 {
        return 0.0;
    }
    let mut sum = 0.0f32;
    let mut n = 0u32;
    // Subsample for speed - binding modulation uses mean influence, not every cell.
    let step = (w.max(h) / 64).max(1);
    let mut j = 0u32;
    while j < h {
        let mut i = 0u32;
        while i < w {
            sum += f.get(i, j);
            n += 1;
            i += step;
        }
        j += step;
    }
    if n == 0 {
        0.0
    } else {
        (sum / n as f32).clamp(0.0, 1.0)
    }
}

fn layer_with_world_scale(layer: &Layer, world_scale: f32) -> Layer {
    let scale = world_scale.clamp(0.05, 20.0);
    if (scale - 1.0).abs() < 1e-6 {
        return layer.clone();
    }
    let mut layer = layer.clone();
    let scale_noise = |noise: &mut crate::layer::NoiseParams| {
        noise.frequency /= scale;
        noise.offset_x *= scale;
        noise.offset_z *= scale;
    };
    match &mut layer.kind {
        crate::layer::LayerKind::NoiseValue(p)
        | crate::layer::LayerKind::NoisePerlin(p)
        | crate::layer::LayerKind::NoiseOpenSimplex(p) => scale_noise(p),
        crate::layer::LayerKind::NoiseWorley(p) => scale_noise(&mut p.base),
        crate::layer::LayerKind::Fbm(p) | crate::layer::LayerKind::Ridged(p) => {
            scale_noise(&mut p.base)
        }
        crate::layer::LayerKind::DomainWarp(p) => {
            scale_noise(&mut p.base);
            p.warp_frequency /= scale;
        }
        crate::layer::LayerKind::Mountains(p) => scale_noise(&mut p.base),
        crate::layer::LayerKind::Dunes(p) => {
            scale_noise(&mut p.base);
            p.wave_frequency /= scale;
        }
        crate::layer::LayerKind::Uplift(p) => {
            p.frequency /= scale;
            p.detail_frequency /= scale;
        }
        crate::layer::LayerKind::Island(p) => {
            p.coastline_frequency /= scale;
            p.ridge_frequency /= scale;
        }
        crate::layer::LayerKind::VoronoiRegions(p) => scale_noise(&mut p.base),
        _ => {}
    }
    layer
}

fn publish_layer_outputs(ctx: &mut EvalContext, layer: &Layer, height: &Heightfield) {
    for output in &layer.common.outputs {
        if !output.enabled {
            continue;
        }
        let field = if output.field == crate::fields::FieldId::Height {
            MaskField::from_raw(height.metrics, &height.to_dense())
        } else {
            let key = output.field.cache_key();
            let Some(field) = ctx.aux_maps.get(&key).cloned() else {
                continue;
            };
            field
        };
        ctx.published_outputs.insert(output.id, field);
    }
}

fn mix_heightfields(
    h_in: &Heightfield,
    h_layer: &Heightfield,
    blend: crate::layer::BlendMode,
    opacity: f32,
    mask: &MaskField,
) -> Heightfield {
    let mut out = h_in.clone();
    out.par_map_indexed(|i, j, hin| {
        blend_heights(blend, hin, h_layer.get(i, j), opacity, mask.get(i, j))
    });
    out.refresh_halos();
    out
}

/// Biome height-delta composite: `H += w * (biome_result - shared_input)`.
///
/// `h_parent` is the stack accumulator below this biome, `shared` is the biome's
/// CopyInput seed, and `biome_result` is the biome group's private output.
fn mix_height_delta(
    h_parent: &Heightfield,
    shared: &Heightfield,
    biome_result: &Heightfield,
    opacity: f32,
    mask: &MaskField,
) -> Heightfield {
    let mut out = h_parent.clone();
    out.par_map_indexed(|i, j, hp| {
        let w = (mask.get(i, j) * opacity).clamp(0.0, 1.0);
        hp + w * (biome_result.get(i, j) - shared.get(i, j))
    });
    out.refresh_halos();
    out
}

/// Merge child aux maps into the parent context, weighted by the group mask
/// and composited with the group's blend mode (weight-field semantics - see
/// `blend_weights` for which modes participate).
fn merge_aux_masked(
    ctx: &mut EvalContext,
    child: &crate::fields::AuxMaps,
    mask: &MaskField,
    opacity: f32,
    blend: crate::layer::BlendMode,
) {
    use crate::fields::{channel_class, ChannelClass};
    use crate::layer::blend_weights;
    let child_map = child.to_hashmap();
    for (key, child_field) in child_map {
        ctx.pass_changed.insert(key.clone());
        let class = channel_class(&key);
        let mut out = ctx
            .aux_maps
            .get(&key)
            .cloned()
            .unwrap_or_else(|| MaskField::zeros(ctx.metrics));
        {
            use rayon::prelude::*;
            let width = ctx.metrics.width as usize;
            out.data_mut()
                .par_chunks_mut(width)
                .enumerate()
                .for_each(|(j, row)| {
                    for (i, v) in row.iter_mut().enumerate() {
                        let (i, j) = (i as u32, j as u32);
                        let c = child_field.get(i, j);
                        let w = (mask.get(i, j) * opacity).clamp(0.0, 1.0);
                        *v = match class {
                            ChannelClass::Weight => {
                                blend_weights(blend, *v, c, opacity, mask.get(i, j))
                            }
                            ChannelClass::Metric => *v * (1.0 - w) + c * w,
                            ChannelClass::Categorical => {
                                if w >= 0.5 {
                                    c
                                } else {
                                    *v
                                }
                            }
                        };
                    }
                });
        }
        ctx.aux_insert(key, out);
    }
    if child.strata.is_some() {
        ctx.aux_maps.strata = child.strata.clone();
    }
}

fn composite_distribution(
    ctx: &EvalContext,
    dist: &crate::mask::Distribution,
    input: &Heightfield,
) -> MaskField {
    use crate::mask::DistBakeContext;
    let slope = ctx.aux.get("slope").map(|m| m.data());
    let curv = ctx.aux.get("curvature").map(|m| m.data());
    let flow = ctx
        .aux
        .get("flow_accumulation")
        .or_else(|| ctx.aux.get("flow"))
        .map(|m| m.data());
    let bake_ctx = DistBakeContext {
        height: Some(input),
        slope_deg: slope,
        curvature: curv,
        flow,
        masks: &ctx.masks,
        aux: Some(&ctx.aux),
    };
    crate::mask::bake_distribution_with_context(dist, input.metrics, &bake_ctx)
}

/// Effective contribution mask from the layer's local distribution.
fn effective_layer_mask(
    ctx: &EvalContext,
    local: &crate::mask::Distribution,
    input: &Heightfield,
) -> MaskField {
    composite_distribution(ctx, local, input)
}

/// Fingerprint of height data for group-cache keyed reuse.
fn height_fingerprint(h: &Heightfield) -> u64 {
    let m = h.metrics;
    let mut state = (m.width as u64)
        .wrapping_mul(0x0100_0000_01b3)
        .wrapping_add(m.height as u64);
    if m.width == 0 || m.height == 0 {
        return state;
    }
    let corners = [
        (0, 0),
        (m.width - 1, 0),
        (0, m.height - 1),
        (m.width - 1, m.height - 1),
        (m.width / 2, m.height / 2),
    ];
    for (i, j) in corners {
        state ^= (h.get(i, j).to_bits() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        state = state.rotate_left(13);
    }
    let step_i = (m.width / 8).max(1);
    let step_j = (m.height / 8).max(1);
    let mut j = 0;
    while j < m.height {
        let mut i = 0;
        while i < m.width {
            state = state
                .wrapping_mul(0x0100_0000_01b3)
                .wrapping_add(h.get(i, j).to_bits() as u64);
            i += step_i;
        }
        j += step_j;
    }
    state
}

/// Composite a scoped layer's aux-channel writes under its weight
/// (opacity × mask), mirroring what `blend_heights` does for height:
/// `out = prior * (1 - w) + written * w`, with zeros standing in for a
/// channel the layer introduced. Channels the processor never wrote (still
/// sharing storage with the pre-layer snapshot) are left untouched, as are
/// the slope/curvature caches - those must always describe the actual
/// current height, not a blend of two heights' derivatives.
fn scope_aux_writes(
    ctx: &mut EvalContext,
    before: &crate::fields::AuxMaps,
    mask: &MaskField,
    opacity: f32,
) {
    use crate::fields::keys;
    use rayon::prelude::*;

    let before_map = before.to_hashmap();
    let after_map = ctx.aux_maps.to_hashmap();
    let metrics = ctx.metrics;
    for (key, after) in after_map {
        if key == keys::SLOPE || key == keys::CURVATURE {
            continue;
        }
        let prior = before_map.get(&key);
        if prior.is_some_and(|p| p.shares_storage(&after)) {
            continue;
        }
        if after.metrics.width != metrics.width || after.metrics.height != metrics.height {
            continue;
        }
        let prior = prior.filter(|p| {
            p.metrics.width == metrics.width && p.metrics.height == metrics.height
        });
        let class = crate::fields::channel_class(&key);
        let mut out = after;
        let width = metrics.width as usize;
        out.data_mut()
            .par_chunks_mut(width)
            .enumerate()
            .for_each(|(j, row)| {
                for (i, v) in row.iter_mut().enumerate() {
                    let (iu, ju) = (i as u32, j as u32);
                    let w = (mask.get(iu, ju) * opacity).clamp(0.0, 1.0);
                    let base = prior.map(|p| p.get(iu, ju)).unwrap_or(0.0);
                    *v = match class {
                        // A fractional id is meaningless - the write wins
                        // only where the layer's weight dominates.
                        crate::fields::ChannelClass::Categorical => {
                            if w >= 0.5 {
                                *v
                            } else {
                                base
                            }
                        }
                        _ => base * (1.0 - w) + *v * w,
                    };
                }
            });
        ctx.aux_insert(key, out);
    }
}

/// True when the layer's static field contract (reads or writes) overlaps
/// any changed non-height channel. Height is handled separately - every
/// layer transforms the height entering it.
fn layer_contract_touches(layer: &Layer, changed: &HashSet<String>) -> bool {
    if changed.is_empty() {
        return false;
    }
    layer
        .kind
        .required_fields()
        .into_iter()
        .chain(layer.kind.optional_fields())
        .chain(layer.kind.modified_fields())
        .chain(layer.kind.produced_fields())
        .any(|f| f != crate::fields::FieldId::Height && changed.contains(&f.cache_key()))
}

/// Helper used by tests to count processor invocations.
pub fn dirty_suffix_ids(stack: &LayerStack, from: LayerId) -> HashSet<LayerId> {
    let ids = stack.layer_ids();
    let mut set = HashSet::new();
    if let Some(start) = ids.iter().position(|&x| x == from) {
        for &id in &ids[start..] {
            set.insert(id);
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::{BlendMode, FlatParams, LayerKind, NoiseParams};

    #[test]
    fn disabled_layer_noop() {
        let mut stack = LayerStack::new();
        let mut flat = Layer::new("Flat", LayerKind::Flat(FlatParams { height: 50.0 }));
        flat.common.enabled = false;
        stack.push(flat);
        let mut eval = StackEvaluator::new();
        let metrics = HeightfieldMetrics::new(16, 16, 64.0, 64.0);
        let mut ctx = EvalContext::new(metrics);
        let out = eval.rebuild_all(&stack, &mut ctx).unwrap();
        assert_eq!(out.get(0, 0), 0.0);
    }

    #[test]
    fn mark_dirty_from_suffix() {
        let mut stack = LayerStack::new();
        let a = Layer::new("A", LayerKind::Flat(FlatParams { height: 1.0 }));
        let b = Layer::new("B", LayerKind::Flat(FlatParams { height: 2.0 }));
        let c = Layer::new("C", LayerKind::NoiseValue(NoiseParams::default()));
        let id_a = a.id();
        let id_b = b.id();
        let id_c = c.id();
        stack.push(a);
        stack.push(b);
        stack.push(c);
        let mut eval = StackEvaluator::new();
        eval.mark_all_dirty(&stack);
        // clear dirty artificially
        for id in [id_a, id_b, id_c] {
            eval.cache.insert(
                id,
                CachedOutput {
                    height: Heightfield::zeros(HeightfieldMetrics::new(4, 4, 4.0, 4.0)),
                    generation: 0,
                    dirty: false,
                    aux: HashMap::new(),
                    strata: None,
                },
            );
        }
        eval.mark_dirty_from(&stack, id_b);
        assert!(!eval.cache.is_dirty(id_a));
        assert!(eval.cache.is_dirty(id_b));
        assert!(eval.cache.is_dirty(id_c));
        let suffix = dirty_suffix_ids(&stack, id_b);
        assert!(suffix.contains(&id_b) && suffix.contains(&id_c) && !suffix.contains(&id_a));
    }

    #[test]
    fn aux_only_edit_skips_unaffected_layers_above() {
        use crate::layer::{BlurParams, FbmParams, VegetationParams};

        let mut stack = LayerStack::new();
        let mut fbm = Layer::new("Fbm", LayerKind::Fbm(FbmParams::default()));
        fbm.common.blend = BlendMode::Add;
        stack.push(fbm);
        let veg = Layer::new("Veg", LayerKind::Vegetation(VegetationParams::default()));
        let veg_id = veg.id();
        stack.push(veg);
        let blur = Layer::new("Blur", LayerKind::Blur(BlurParams::default()));
        let blur_id = blur.id();
        stack.push(blur);

        let metrics = HeightfieldMetrics::new(48, 48, 96.0, 96.0);
        let mut eval = StackEvaluator::new();
        let mut ctx = EvalContext::new(metrics);
        let first = eval.rebuild_all(&stack, &mut ctx).unwrap();

        // Vegetation edit: density change. Blur neither reads nor writes
        // vegetation and sees an unchanged height, so it must stay clean.
        if let Some(l) = stack.find_mut(veg_id) {
            l.kind = LayerKind::Vegetation(VegetationParams {
                density: 0.9,
                ..VegetationParams::default()
            });
        }
        eval.mark_dirty_from(&stack, veg_id);
        assert!(eval.cache.is_dirty(veg_id));
        assert!(
            !eval.cache.is_dirty(blur_id),
            "height-passthrough edit must not dirty an unrelated layer above"
        );

        ctx.layer_timings.clear();
        let second = eval.rebuild_incremental(&stack, &mut ctx).unwrap();
        let status_of = |id: LayerId| {
            ctx.layer_timings
                .iter()
                .find(|t| t.layer == id)
                .map(|t| t.status)
        };
        assert_eq!(status_of(veg_id), Some(LayerEvalStatus::Computed));
        assert_eq!(
            status_of(blur_id),
            Some(LayerEvalStatus::CacheHit),
            "blur must reuse its cache across a vegetation-only edit"
        );
        // Vegetation doesn't touch height: output identical.
        for j in (0..48).step_by(11) {
            for i in (0..48).step_by(11) {
                assert_eq!(first.get(i, j), second.get(i, j));
            }
        }

        // Clobber protection: the blur cache hit must not restore stale
        // vegetation over the freshly computed channel. Compare against a
        // cold evaluation of the edited stack.
        let mut ref_eval = StackEvaluator::new();
        let mut ref_ctx = EvalContext::new(metrics);
        ref_eval.rebuild_all(&stack, &mut ref_ctx).unwrap();
        let (fresh, incremental) = (
            ref_ctx.aux_maps.get("vegetation").expect("veg aux"),
            ctx.aux_maps.get("vegetation").expect("veg aux"),
        );
        for j in (0..48).step_by(7) {
            for i in (0..48).step_by(7) {
                assert!(
                    (fresh.get(i, j) - incremental.get(i, j)).abs() < 1e-5,
                    "stale vegetation restored at ({i},{j})"
                );
            }
        }
    }

    /// Field-aware invalidation trusts the static field contracts: a layer
    /// that writes a channel it never declares can leave consumers above it
    /// stale, since they are only dirtied when a *declared* channel they read
    /// changes. This validates every registered kind's declaration against
    /// what its processor actually writes at runtime (copy-on-write buffer
    /// identity), so a new under-declaration fails here instead of silently
    /// corrupting incremental rebuilds.
    #[test]
    fn processor_writes_stay_within_declared_contracts() {
        use crate::fields::keys;
        use crate::layer::LayerTypeRegistry;

        let registry = LayerTypeRegistry::builtin();
        let metrics = HeightfieldMetrics::new(32, 32, 256.0, 256.0);
        // A little relief so sims and analysis have something to act on.
        let mut input = Heightfield::zeros(metrics);
        for j in 0..32u32 {
            for i in 0..32u32 {
                let (x, y) = (i as f32 / 31.0, j as f32 / 31.0);
                input.set(i, j, 40.0 * (x * 6.0).sin() * (y * 5.0).cos() + 60.0 * x);
            }
        }
        input.refresh_halos();

        let mut violations: Vec<String> = Vec::new();
        for meta in registry.all() {
            let Some(layer) = registry.create(meta.type_id) else {
                continue;
            };
            let declared: HashSet<String> = layer
                .kind
                .produced_fields()
                .into_iter()
                .chain(layer.kind.modified_fields())
                .map(|f| f.cache_key())
                .collect();

            let mut eval = StackEvaluator::new();
            let mut ctx = EvalContext::new(metrics);
            ctx.quality = PreviewQuality::Draft;
            if eval
                .evaluate_layer(&mut ctx, &input, &layer, None, false)
                .is_err()
            {
                continue;
            }
            for key in &ctx.pass_changed {
                // Derived caches are recomputed from the current height by
                // the context itself, and any height change already dirties
                // everything above, so they are not the layer's product.
                if key == keys::SLOPE || key == keys::CURVATURE {
                    continue;
                }
                if !declared.contains(key) {
                    violations.push(format!("{} writes undeclared '{}'", meta.type_id, key));
                }
            }
        }
        violations.sort();
        assert!(
            violations.is_empty(),
            "layer kinds write channels their field contracts do not declare, so \
             consumers of those channels are not invalidated:\n  {}",
            violations.join("\n  ")
        );
    }

    #[test]
    fn scrub_checkpoints_replay_visited_positions() {
        use crate::layer::{FbmParams, ThermalErosionParams};

        let mut stack = LayerStack::new();
        let mut fbm = Layer::new("Fbm", LayerKind::Fbm(FbmParams::default()));
        fbm.common.blend = BlendMode::Add;
        stack.push(fbm);
        let mut thermal = Layer::new(
            "Thermal",
            LayerKind::ThermalErosion(ThermalErosionParams::default()),
        );
        thermal.common.sim_progress = 0.5;
        let thermal_id = thermal.id();
        stack.push(thermal);

        let metrics = HeightfieldMetrics::new(48, 48, 96.0, 96.0);
        let mut eval = StackEvaluator::new();
        let mut ctx = EvalContext::new(metrics);
        let first = eval.rebuild_all(&stack, &mut ctx).unwrap();
        assert_eq!(eval.scrub_hits, 0);

        // Revisit the same position: replayed from the checkpoint.
        eval.mark_dirty_from(&stack, thermal_id);
        let replay = eval.rebuild_incremental(&stack, &mut ctx).unwrap();
        assert_eq!(eval.scrub_hits, 1);
        for j in (0..48).step_by(9) {
            for i in (0..48).step_by(9) {
                assert_eq!(first.get(i, j), replay.get(i, j));
            }
        }

        // A new position computes fresh...
        stack.find_mut(thermal_id).unwrap().common.sim_progress = 0.7;
        eval.mark_dirty_from(&stack, thermal_id);
        eval.rebuild_incremental(&stack, &mut ctx).unwrap();
        assert_eq!(eval.scrub_hits, 1);

        // ...and scrubbing back replays instantly again.
        stack.find_mut(thermal_id).unwrap().common.sim_progress = 0.5;
        eval.mark_dirty_from(&stack, thermal_id);
        eval.rebuild_incremental(&stack, &mut ctx).unwrap();
        assert_eq!(eval.scrub_hits, 2);

        // A parameter edit invalidates checkpoints for that state.
        if let Some(l) = stack.find_mut(thermal_id) {
            if let LayerKind::ThermalErosion(p) = &mut l.kind {
                p.strength *= 1.5;
            }
        }
        eval.mark_dirty_from(&stack, thermal_id);
        eval.rebuild_incremental(&stack, &mut ctx).unwrap();
        assert_eq!(eval.scrub_hits, 2, "changed params must miss the checkpoint");
    }

    /// Isolated-group cache reuse is validated by a sparse input fingerprint,
    /// which a small edit can slip between. That is safe only because the
    /// fingerprint is a *secondary* check: a height-changing edit below the
    /// group dirties every layer above it, including the group's descendants,
    /// and `try_reuse_group_cache` refuses reuse when any descendant is dirty.
    /// This pins that invariant - if dirty propagation is ever narrowed so a
    /// group's descendants can stay clean while its input height changes, the
    /// fingerprint becomes load-bearing and must be strengthened.
    #[test]
    fn group_cache_is_not_fooled_by_a_localized_edit_below() {
        use crate::authoring::{SculptPoint, SculptStroke, SculptStrokeKind};
        use crate::layer::{FbmParams, FlatParams, LayerGroup, SculptStrokeParams};

        let metrics = HeightfieldMetrics::new(96, 96, 512.0, 512.0);
        let mut stack = LayerStack::new();
        stack.push(Layer::new(
            "Base",
            LayerKind::Flat(FlatParams { height: 100.0 }),
        ));
        let sculpt = Layer::new(
            "Sculpt",
            LayerKind::SculptStrokes(SculptStrokeParams::default()),
        );
        let sculpt_id = sculpt.id();
        stack.push(sculpt);
        let mut group = LayerGroup::isolated("G");
        let mut fbm = Layer::new("Fbm", LayerKind::Fbm(FbmParams::default()));
        fbm.common.blend = BlendMode::Add;
        let inner_id = fbm.id();
        group.children.push(StackNode::Layer(fbm));
        stack.nodes.push(StackNode::Group(group));

        let mut eval = StackEvaluator::new();
        let mut ctx = EvalContext::new(metrics);
        let before = eval.rebuild_all(&stack, &mut ctx).unwrap();

        // Same between-samples stroke that defeated the scrub fingerprint.
        if let Some(l) = stack.find_mut(sculpt_id) {
            if let LayerKind::SculptStrokes(p) = &mut l.kind {
                p.strokes.push(SculptStroke {
                    kind: SculptStrokeKind::Raise,
                    points: vec![SculptPoint {
                        u: 0.4427,
                        v: 0.4427,
                        pressure: 1.0,
                    }],
                    radius_m: 18.0,
                    strength: 120.0,
                    ..Default::default()
                });
            }
        }
        eval.mark_dirty_from(&stack, sculpt_id);
        assert!(
            eval.cache.is_dirty(inner_id),
            "a height edit below an isolated group must dirty the group's \
             descendants - this is what keeps the sparse group fingerprint safe"
        );
        let after = eval.rebuild_incremental(&stack, &mut ctx).unwrap();

        let mut diff = 0.0f32;
        for j in 0..96 {
            for i in 0..96 {
                diff = diff.max((before.get(i, j) - after.get(i, j)).abs());
            }
        }
        assert!(diff > 1e-3, "localized edit below a group must reach the output");
    }

    /// A localized edit below a scrubbed sim must invalidate its checkpoint.
    /// The sparse fingerprint sample grid can miss a small edit entirely, so
    /// checkpoint keys hash full content.
    #[test]
    fn scrub_checkpoint_sees_localized_edits_below() {
        use crate::layer::{FlatParams, SculptStrokeParams, ThermalErosionParams};
        use crate::authoring::{SculptPoint, SculptStroke, SculptStrokeKind};

        let metrics = HeightfieldMetrics::new(96, 96, 512.0, 512.0);
        let mut stack = LayerStack::new();
        stack.push(Layer::new(
            "Base",
            LayerKind::Flat(FlatParams { height: 100.0 }),
        ));
        let sculpt = Layer::new("Sculpt", LayerKind::SculptStrokes(SculptStrokeParams::default()));
        let sculpt_id = sculpt.id();
        stack.push(sculpt);
        let mut thermal = Layer::new(
            "Thermal",
            LayerKind::ThermalErosion(ThermalErosionParams::default()),
        );
        thermal.common.sim_progress = 0.5;
        stack.push(thermal);

        let mut eval = StackEvaluator::new();
        let mut ctx = EvalContext::new(metrics);
        let before = eval.rebuild_all(&stack, &mut ctx).unwrap();

        // A narrow stroke between fingerprint sample points.
        if let Some(l) = stack.find_mut(sculpt_id) {
            if let LayerKind::SculptStrokes(p) = &mut l.kind {
                p.strokes.push(SculptStroke {
                    kind: SculptStrokeKind::Raise,
                    // Centred at texel ~42.5 with a ~3-texel radius: the
                    // sparse fingerprint samples every 12th texel (36, 48),
                    // so this edit falls entirely between sample points.
                    points: vec![SculptPoint {
                        u: 0.4427,
                        v: 0.4427,
                        pressure: 1.0,
                    }],
                    radius_m: 18.0,
                    strength: 120.0,
                    ..Default::default()
                });
            }
        }
        eval.mark_dirty_from(&stack, sculpt_id);
        let after = eval.rebuild_incremental(&stack, &mut ctx).unwrap();

        let mut diff = 0.0f32;
        for j in 0..96 {
            for i in 0..96 {
                diff = diff.max((before.get(i, j) - after.get(i, j)).abs());
            }
        }
        assert!(
            diff > 1e-3,
            "a localized edit below a scrubbed sim must not replay a stale checkpoint"
        );
    }

    #[test]
    fn sim_progress_scrubs_iteration_budget() {
        use crate::layer::{FbmParams, ThermalErosionParams};

        let build = |progress: f32| {
            let mut stack = LayerStack::new();
            let mut fbm = Layer::new("Fbm", LayerKind::Fbm(FbmParams::default()));
            fbm.common.blend = BlendMode::Add;
            stack.push(fbm);
            let mut thermal = Layer::new(
                "Thermal",
                LayerKind::ThermalErosion(ThermalErosionParams::default()),
            );
            thermal.common.sim_progress = progress;
            stack.push(thermal);
            let metrics = HeightfieldMetrics::new(48, 48, 96.0, 96.0);
            let mut eval = StackEvaluator::new();
            let mut ctx = EvalContext::new(metrics);
            eval.rebuild_all(&stack, &mut ctx).unwrap()
        };

        let full = build(1.0);
        let full_again = build(1.0);
        let early = build(0.1);
        // Full progress is the untouched default behavior and deterministic.
        for j in (0..48).step_by(9) {
            for i in (0..48).step_by(9) {
                assert_eq!(full.get(i, j), full_again.get(i, j));
            }
        }
        // Scrubbed-back sim must differ from the completed one.
        let mut diff = 0.0f32;
        for j in 0..48 {
            for i in 0..48 {
                diff = diff.max((full.get(i, j) - early.get(i, j)).abs());
            }
        }
        assert!(
            diff > 1e-3,
            "scrubbing a thermal sim to 10% must change its output (diff {diff})"
        );
    }

    #[test]
    fn group_aux_merge_respects_channel_classes() {
        let metrics = HeightfieldMetrics::new(8, 8, 8.0, 8.0);
        let n = 64;
        let mut child = AuxMaps::new();
        child.insert(
            "bedrock_height".to_string(),
            MaskField::from_raw(metrics, &vec![250.0; n]),
        );
        child.insert(
            "materials".to_string(),
            MaskField::from_raw(metrics, &vec![3.0; n]),
        );
        child.insert(
            "wetness".to_string(),
            MaskField::from_raw(metrics, &vec![0.8; n]),
        );

        // Full-weight merge: metric takes the child value unclamped,
        // categorical switches identity, weight lerps.
        let mut ctx = EvalContext::new(metrics);
        ctx.aux_insert("bedrock_height", MaskField::from_raw(metrics, &vec![100.0; n]));
        ctx.aux_insert("materials", MaskField::from_raw(metrics, &vec![2.0; n]));
        ctx.aux_insert("wetness", MaskField::from_raw(metrics, &vec![0.2; n]));
        merge_aux_masked(
            &mut ctx,
            &child,
            &MaskField::ones(metrics),
            1.0,
            crate::layer::BlendMode::Normal,
        );
        assert_eq!(
            ctx.aux_maps.get("bedrock_height").unwrap().get(3, 3),
            250.0,
            "metric channels must not be clamped to [0,1]"
        );
        assert_eq!(ctx.aux_maps.get("materials").unwrap().get(3, 3), 3.0);
        assert!((ctx.aux_maps.get("wetness").unwrap().get(3, 3) - 0.8).abs() < 1e-6);

        // Sub-half weight: categorical keeps the parent identity outright
        // (never a fractional id), metric lerps partway.
        let mut ctx = EvalContext::new(metrics);
        ctx.aux_insert("bedrock_height", MaskField::from_raw(metrics, &vec![100.0; n]));
        ctx.aux_insert("materials", MaskField::from_raw(metrics, &vec![2.0; n]));
        merge_aux_masked(
            &mut ctx,
            &child,
            &MaskField::filled(metrics, 0.4),
            1.0,
            crate::layer::BlendMode::Normal,
        );
        assert_eq!(ctx.aux_maps.get("materials").unwrap().get(3, 3), 2.0);
        let bedrock = ctx.aux_maps.get("bedrock_height").unwrap().get(3, 3);
        assert!((bedrock - 160.0).abs() < 1e-3, "expected lerp, got {bedrock}");
    }

    #[test]
    fn baked_lower_layer_is_reused_when_upper_layer_is_dirty() {
        let mut stack = LayerStack::new();
        let mut baked = Layer::new("Baked", LayerKind::Flat(FlatParams { height: 1.0 }));
        baked.common.cached = true;
        let baked_id = baked.id();
        let mut upper = Layer::new("Upper", LayerKind::Flat(FlatParams { height: 3.0 }));
        upper.common.blend = BlendMode::Add;
        let upper_id = upper.id();
        stack.push(baked);
        stack.push(upper);

        let metrics = HeightfieldMetrics::new(8, 8, 8.0, 8.0);
        let mut eval = StackEvaluator::new();
        eval.cache.insert(
            baked_id,
            CachedOutput {
                // Deliberately differs from the processor output. Rebuilding only
                // the upper layer must seed from this frozen checkpoint.
                height: Heightfield::filled(metrics, 7.0),
                generation: 0,
                dirty: false,
                aux: HashMap::new(),
                strata: None,
            },
        );
        eval.mark_dirty_from(&stack, upper_id);

        let mut ctx = EvalContext::new(metrics);
        let out = eval.rebuild_incremental(&stack, &mut ctx).unwrap();
        assert_eq!(out.get(0, 0), 10.0);
        assert!(!eval.cache.is_dirty(baked_id));
    }

    #[test]
    fn deterministic_noise_layer() {
        let mut stack = LayerStack::new();
        stack.push(Layer::new(
            "N",
            LayerKind::NoiseValue(NoiseParams {
                seed: 123,
                frequency: 0.05,
                amplitude: 10.0,
                octaves: 1,
                ..NoiseParams::default()
            }),
        ));
        let metrics = HeightfieldMetrics::new(32, 32, 128.0, 128.0);
        let mut eval = StackEvaluator::new();
        let mut ctx = EvalContext::new(metrics);
        let a = eval.rebuild_all(&stack, &mut ctx).unwrap().to_dense();
        let mut eval2 = StackEvaluator::new();
        let mut ctx2 = EvalContext::new(metrics);
        let b = eval2.rebuild_all(&stack, &mut ctx2).unwrap().to_dense();
        assert_eq!(a, b);
    }

    #[test]
    fn add_blend_merges_with_base() {
        use crate::layer::BlendMode;
        let metrics = HeightfieldMetrics::new(16, 16, 64.0, 64.0);
        let mut stack = LayerStack::new();
        stack.push(Layer::new(
            "Base",
            LayerKind::Flat(FlatParams { height: 10.0 }),
        ));
        let noise = Layer::new(
            "Hills",
            LayerKind::NoiseValue(NoiseParams {
                seed: 7,
                frequency: 0.08,
                amplitude: 5.0,
                octaves: 1,
                ..NoiseParams::default()
            }),
        );
        assert_eq!(noise.common.blend, BlendMode::Add);
        stack.push(noise);

        // Noise-only for comparison
        let mut noise_only = LayerStack::new();
        let mut n = Layer::new(
            "Hills",
            LayerKind::NoiseValue(NoiseParams {
                seed: 7,
                frequency: 0.08,
                amplitude: 5.0,
                octaves: 1,
                ..NoiseParams::default()
            }),
        );
        n.common.blend = BlendMode::Normal;
        noise_only.push(n);

        let mut eval = StackEvaluator::new();
        let mut ctx = EvalContext::new(metrics);
        let merged = eval.rebuild_all(&stack, &mut ctx).unwrap();

        let mut eval2 = StackEvaluator::new();
        let mut ctx2 = EvalContext::new(metrics);
        let only = eval2.rebuild_all(&noise_only, &mut ctx2).unwrap();

        let sample = merged.get(8, 8);
        let noise_sample = only.get(8, 8);
        assert!(
            (sample - (10.0 + noise_sample)).abs() < 1e-3,
            "expected base+noise {sample} vs {}",
            10.0 + noise_sample
        );
        assert!(
            (sample - noise_sample).abs() > 1.0,
            "merged should not equal noise alone"
        );
    }

    #[test]
    fn normal_blend_replaces_base() {
        use crate::layer::BlendMode;
        let metrics = HeightfieldMetrics::new(16, 16, 64.0, 64.0);
        let mut stack = LayerStack::new();
        stack.push(Layer::new(
            "Base",
            LayerKind::Flat(FlatParams { height: 10.0 }),
        ));
        let mut noise = Layer::new(
            "Hills",
            LayerKind::NoiseValue(NoiseParams {
                seed: 7,
                frequency: 0.08,
                amplitude: 5.0,
                octaves: 1,
                ..NoiseParams::default()
            }),
        );
        noise.common.blend = BlendMode::Normal;
        stack.push(noise);

        let mut noise_only = LayerStack::new();
        let mut n = Layer::new(
            "Hills",
            LayerKind::NoiseValue(NoiseParams {
                seed: 7,
                frequency: 0.08,
                amplitude: 5.0,
                octaves: 1,
                ..NoiseParams::default()
            }),
        );
        n.common.blend = BlendMode::Normal;
        noise_only.push(n);

        let mut eval = StackEvaluator::new();
        let mut ctx = EvalContext::new(metrics);
        let replaced = eval.rebuild_all(&stack, &mut ctx).unwrap();
        let mut eval2 = StackEvaluator::new();
        let mut ctx2 = EvalContext::new(metrics);
        let only = eval2.rebuild_all(&noise_only, &mut ctx2).unwrap();

        assert!((replaced.get(8, 8) - only.get(8, 8)).abs() < 1e-4);
    }

    #[test]
    fn solo_skips_non_solo_layers_in_the_same_stack() {
        let metrics = HeightfieldMetrics::new(8, 8, 80.0, 80.0);
        let mut stack = LayerStack::new();
        stack.push(Layer::new(
            "Base",
            LayerKind::Flat(FlatParams { height: 100.0 }),
        ));
        let mut solo = Layer::new("Solo", LayerKind::Flat(FlatParams { height: 20.0 }));
        solo.common.blend = BlendMode::Add;
        solo.common.solo = true;
        stack.push(solo);
        let mut after = Layer::new("After", LayerKind::Flat(FlatParams { height: 50.0 }));
        after.common.blend = BlendMode::Add;
        stack.push(after);

        let mut eval = StackEvaluator::new();
        let mut ctx = EvalContext::new(metrics);
        let out = eval.rebuild_all(&stack, &mut ctx).unwrap();
        assert!((out.get(4, 4) - 20.0).abs() < 1.0e-4);
    }

    #[test]
    fn height_mask_is_rebaked_against_the_owning_layers_input() {
        use crate::mask::{bake_mask_assets, MaskAsset, MaskId, MaskRef, MaskSource};

        let metrics = HeightfieldMetrics::new(16, 16, 160.0, 160.0);
        let mask_id = MaskId::new();
        let asset = MaskAsset::new(
            mask_id,
            "High ground",
            MaskSource::Height {
                min: 50.0,
                max: 100.0,
            },
        );

        let mut stack = LayerStack::new();
        stack.push(Layer::new(
            "Base",
            LayerKind::Flat(FlatParams { height: 100.0 }),
        ));
        let mut raise = Layer::new(
            "Raise high ground",
            LayerKind::Flat(FlatParams { height: 25.0 }),
        );
        raise.common.blend = BlendMode::Add;
        raise.common.masks.push(MaskRef::new(mask_id));
        stack.push(raise);

        let mut ctx = EvalContext::new(metrics);
        // Simulate a cold export / stale preview bake. The zero-height reference
        // produces an empty mask, but evaluation must replace it at point of use.
        ctx.masks = bake_mask_assets(
            std::slice::from_ref(&asset),
            &Heightfield::zeros(metrics),
            metrics,
            &HashMap::new(),
        );
        ctx.mask_assets.push(asset);

        let mut eval = StackEvaluator::new();
        let out = eval.rebuild_all(&stack, &mut ctx).unwrap();
        assert!((out.get(8, 8) - 125.0).abs() < 1.0e-4);
    }

    #[test]
    fn scoped_group_mask_limits_child_normal_filter() {
        use crate::layer::LayerGroup;
        use crate::mask::{bake_mask_assets, MaskAsset, MaskId, MaskRef, MaskSource};

        let metrics = HeightfieldMetrics::new(32, 32, 320.0, 320.0);
        let mask_id = MaskId::new();
        let asset = MaskAsset {
            id: mask_id,
            name: "Right".into(),
            source: MaskSource::Height {
                min: 0.0,
                max: 50.0,
            },
            ops: Vec::new(),
            paint: None,
            display_color: crate::mask::default_mask_display_color(),
            owner: None,
        };
        let mut reference = Heightfield::zeros(metrics);
        for j in 0..32 {
            for i in 0..32 {
                reference.set(i, j, if i >= 16 { 100.0 } else { 0.0 });
            }
        }

        let mut stack = LayerStack::new();
        stack.push(Layer::new(
            "Base",
            LayerKind::Flat(FlatParams { height: 10.0 }),
        ));
        let mut group = LayerGroup::new("Scoped");
        group.masks.push(MaskRef::new(mask_id));
        group.children.push(StackNode::Layer(Layer::new(
            "Raise",
            LayerKind::Flat(FlatParams { height: 80.0 }),
        )));
        stack.push_group(group);

        let mut eval = StackEvaluator::new();
        let mut ctx = EvalContext::new(metrics);
        ctx.masks = bake_mask_assets(&[asset], &reference, metrics, &HashMap::new());
        let out = eval.rebuild_all(&stack, &mut ctx).unwrap();
        assert!(out.get(24, 16) > 60.0, "inside group mask should raise");
        assert!(
            (out.get(8, 16) - 10.0).abs() < 1e-3,
            "outside group mask should keep base"
        );
    }

    fn push_flat_to_biome_filters(biome: &mut crate::layer::LayerGroup, height: f32) -> LayerId {
        use crate::layer::{BiomeSection, FlatParams, LayerKind};
        biome.ensure_biome_sections();
        let layer = Layer::new("Flat", LayerKind::Flat(FlatParams { height }));
        let id = layer.id();
        if let Some(sec) = biome.find_section_mut(BiomeSection::Filters) {
            sec.children.push(StackNode::Layer(layer));
        } else {
            biome.children.push(StackNode::Layer(layer));
        }
        id
    }

    #[test]
    fn incremental_scoped_groups_reuse_clean_sibling_biome() {
        use crate::layer::LayerGroup;

        let metrics = HeightfieldMetrics::new(16, 16, 160.0, 160.0);
        let mut stack = LayerStack::new();
        stack.push(Layer::new(
            "Base",
            LayerKind::Flat(FlatParams { height: 0.0 }),
        ));

        let mut biome_a = LayerGroup::biome("Alpine");
        let layer_a = push_flat_to_biome_filters(&mut biome_a, 10.0);
        let biome_a_id = biome_a.id;

        let mut biome_b = LayerGroup::biome("Desert");
        let layer_b = push_flat_to_biome_filters(&mut biome_b, 20.0);

        stack.push_group(biome_a);
        stack.push_group(biome_b);

        let mut eval = StackEvaluator::new();
        let mut ctx = EvalContext::new(metrics);
        let _ = eval.rebuild_all(&stack, &mut ctx).unwrap();
        assert!(
            eval.cache.get(biome_a_id).is_some(),
            "scoped group output should be cached"
        );

        eval.cache.mark_dirty(layer_a);
        let mut ctx = EvalContext::new(metrics);
        let _ = eval.rebuild_incremental(&stack, &mut ctx).unwrap();

        let b_timing = ctx
            .layer_timings
            .iter()
            .find(|t| t.layer == layer_b)
            .expect("biome B layer should appear in timings");
        assert_eq!(
            b_timing.status,
            LayerEvalStatus::CacheHit,
            "clean sibling biome layer must not recompute"
        );
        assert!(
            !eval.cache.is_dirty(layer_b),
            "sibling layer cache should stay clean"
        );
    }
}
