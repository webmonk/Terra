//! Layer evaluation, caching, and dirty propagation.

mod cache;
mod processors;
mod scheduler;
mod smart_cache;
mod worker;

pub use cache::{CachedOutput, LayerCache};
pub use processors::ProcessorRegistry;
pub use scheduler::{EvalJob, EvalScheduler, PreviewQuality};
pub use smart_cache::DiskSmartCache;
pub use worker::{EvalWorkRequest, EvalWorkResult, EvalWorker};

use crate::heightfield::{Heightfield, HeightfieldMetrics};
use crate::layer::{blend_heights, Layer, LayerId, LayerStack};
use crate::mask::{MaskAsset, MaskField, MaskId};
use std::collections::{HashMap, HashSet};
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

pub struct EvalContext {
    pub metrics: HeightfieldMetrics,
    pub masks: HashMap<MaskId, MaskField>,
    pub mask_assets: Vec<MaskAsset>,
    /// Extra maps produced by sims (wetness, sediment, …).
    pub aux: HashMap<String, MaskField>,
    pub cancelled: bool,
    pub quality: PreviewQuality,
}

impl EvalContext {
    pub fn new(metrics: HeightfieldMetrics) -> Self {
        Self {
            metrics,
            masks: HashMap::new(),
            mask_assets: Vec::new(),
            aux: HashMap::new(),
            cancelled: false,
            quality: PreviewQuality::Full,
        }
    }

    pub fn check_cancelled(&self) -> Result<(), EvalError> {
        if self.cancelled {
            Err(EvalError::Cancelled)
        } else {
            Ok(())
        }
    }
}

pub trait LayerProcessor: Send + Sync {
    fn id(&self) -> &'static str;
    fn evaluate(
        &self,
        ctx: &EvalContext,
        input: &Heightfield,
        layer: &Layer,
    ) -> Result<Heightfield, EvalError>;
}

pub struct StackEvaluator {
    pub registry: ProcessorRegistry,
    pub cache: LayerCache,
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
        }
    }

    pub fn mark_dirty_from(&mut self, stack: &LayerStack, id: LayerId) {
        let ids = stack.layer_ids();
        if let Some(start) = ids.iter().position(|&x| x == id) {
            for &dep in &ids[start..] {
                self.cache.mark_dirty(dep);
            }
        } else {
            // Unknown id: dirty everything
            for &dep in &ids {
                self.cache.mark_dirty(dep);
            }
        }
    }

    pub fn mark_all_dirty(&mut self, stack: &LayerStack) {
        for id in stack.layer_ids() {
            self.cache.mark_dirty(id);
        }
    }

    /// Full rebuild (Phase 1 path).
    pub fn rebuild_all(
        &mut self,
        stack: &LayerStack,
        ctx: &mut EvalContext,
    ) -> Result<Heightfield, EvalError> {
        profiling::scope!("rebuild_all");
        let mut current = Heightfield::zeros(ctx.metrics);
        self.cache.clear();
        for layer in stack.flatten_layers() {
            ctx.check_cancelled()?;
            current = self.evaluate_layer(ctx, &current, layer)?;
            self.store_cached(layer.id(), &current, ctx, layer.common.cached);
        }
        Ok(current)
    }

    /// Incremental rebuild from first dirty layer (Phase 4).
    pub fn rebuild_incremental(
        &mut self,
        stack: &LayerStack,
        ctx: &mut EvalContext,
    ) -> Result<Heightfield, EvalError> {
        profiling::scope!("rebuild_incremental");
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
                    return Ok(top.height.clone());
                }
            }
        }

        let first_dirty = first_dirty.unwrap_or(0);

        let mut current = if first_dirty == 0 {
            Heightfield::zeros(ctx.metrics)
        } else {
            let prev_id = layers[first_dirty - 1].id();
            self.cache
                .get_or_load(prev_id, ctx.metrics)
                .map(|c| c.height.clone())
                .unwrap_or_else(|| Heightfield::zeros(ctx.metrics))
        };

        for layer in &layers[first_dirty..] {
            ctx.check_cancelled()?;
            current = self.evaluate_layer(ctx, &current, layer)?;
            self.store_cached(layer.id(), &current, ctx, layer.common.cached);
        }

        Ok(current)
    }

    /// Continue evaluating a flattened stack from a precomputed heightfield.
    ///
    /// Used by the hybrid GPU preview path after it readbacks its compatible prefix.
    pub fn evaluate_suffix(
        &mut self,
        stack: &LayerStack,
        ctx: &mut EvalContext,
        start_index: usize,
        mut current: Heightfield,
    ) -> Result<Heightfield, EvalError> {
        let layers = stack.flatten_layers();
        for layer in layers.into_iter().skip(start_index) {
            ctx.check_cancelled()?;
            current = self.evaluate_layer(ctx, &current, layer)?;
            self.store_cached(layer.id(), &current, ctx, layer.common.cached);
        }
        Ok(current)
    }

    fn store_cached(
        &mut self,
        id: LayerId,
        height: &Heightfield,
        ctx: &EvalContext,
        baked: bool,
    ) {
        let output = CachedOutput {
            height: height.clone(),
            generation: self.cache.generation,
            dirty: false,
            aux: ctx.aux.clone(),
        };
        if baked {
            self.cache.insert_baked(id, output);
        } else {
            self.cache.insert(id, output);
        }
    }

    fn evaluate_layer(
        &mut self,
        ctx: &mut EvalContext,
        input: &Heightfield,
        layer: &Layer,
    ) -> Result<Heightfield, EvalError> {
        if !layer.common.enabled {
            return Ok(input.clone());
        }

        // A clean cached layer is a bake checkpoint: reuse both its height and
        // analysis maps rather than invoking its processor again.
        if layer.common.cached {
            if let Some(cached) = self.cache.get_or_load(layer.id(), ctx.metrics) {
                ctx.aux.extend(cached.aux.clone());
                return Ok(cached.height.clone());
            }
        }

        let generated = self.registry.evaluate(ctx, input, layer)?;
        let mask = composite_layer_mask(ctx, layer, input);
        let mut out = input.clone();
        let w = input.metrics.width;
        let h = input.metrics.height;
        for j in 0..h {
            for i in 0..w {
                let hin = input.get(i, j);
                let hlayer = generated.get(i, j);
                let m = mask.get(i, j);
                let v = blend_heights(layer.common.blend, hin, hlayer, layer.common.opacity, m);
                out.set(i, j, v);
            }
        }
        out.refresh_halos();
        Ok(out)
    }
}

fn composite_layer_mask(ctx: &EvalContext, layer: &Layer, input: &Heightfield) -> MaskField {
    if layer.common.masks.is_empty() {
        return MaskField::ones(input.metrics);
    }
    let mut acc = MaskField::ones(input.metrics);
    for mref in &layer.common.masks {
        let field = ctx
            .masks
            .get(&mref.id)
            .cloned()
            .unwrap_or_else(|| MaskField::ones(input.metrics));
        for j in 0..acc.metrics.height {
            for i in 0..acc.metrics.width {
                let mut v = field.get(i, j) * mref.strength;
                if mref.invert {
                    v = 1.0 - v;
                }
                acc.set(i, j, acc.get(i, j) * v);
            }
        }
    }
    acc
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
}
