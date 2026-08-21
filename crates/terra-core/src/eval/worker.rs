//! Background CPU evaluation worker (Gaea-style engine/UI split).
//!
//! All CPU stack eval (Draft refine fallback, Medium, Full) runs off the UI thread.
//! Interactive Draft prefers GPU present; jobs are cancelled by bumping `current_token`.

use super::{EvalContext, EvalError, PreviewQuality, StackEvaluator};
use crate::heightfield::{Heightfield, HeightfieldMetrics};
use crate::layer::{LayerId, LayerStack};
use crate::mask::{bake_mask_assets, MaskAsset, MaskField};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct EvalWorkRequest {
    pub token: u64,
    pub quality: PreviewQuality,
    pub stack: LayerStack,
    pub masks: Vec<MaskAsset>,
    pub base_metrics: HeightfieldMetrics,
    pub level_steps: crate::analyze::LevelStepSettings,
    pub preview_res: u32,
    pub export_res: u32,
    pub aux: HashMap<String, MaskField>,
    /// Depth-aware Materials strata (not representable in the aux HashMap).
    pub strata: Option<Vec<crate::layer::Stratum>>,
    /// Prior composed height for baking height/slope/curvature masks.
    /// Falls back to zeros only on the first build of a generation.
    pub mask_reference: Option<std::sync::Arc<Heightfield>>,
    /// When set, only this layer and above are dirty (suffix rebuild).
    pub dirty_from: Option<LayerId>,
    pub mark_all_dirty: bool,
}

#[derive(Debug)]
pub struct EvalWorkResult {
    pub token: u64,
    pub quality: PreviewQuality,
    pub height: Heightfield,
    pub aux: HashMap<String, MaskField>,
    pub strata: Option<Vec<crate::layer::Stratum>>,
    pub eval_us: u64,
    pub layer_timings: Vec<super::LayerEvalTiming>,
    /// Compact per-layer height previews for UI thumbnails, taken from the
    /// worker's own layer cache (the UI thread never sees that cache).
    pub layer_previews: Vec<LayerPreview>,
}

/// Downsampled height grid for one layer's cached (cumulative) output.
#[derive(Debug, Clone)]
pub struct LayerPreview {
    pub layer: LayerId,
    /// Worker cache generation; changes whenever the layer re-evaluates.
    pub generation: u64,
    /// Grid edge length; `heights` is `res * res` row-major.
    pub res: u32,
    pub world_size_x: f32,
    pub heights: Vec<f32>,
}

pub const LAYER_PREVIEW_RES: u32 = 48;

fn downsample_preview(layer: LayerId, generation: u64, hf: &Heightfield) -> LayerPreview {
    let res = LAYER_PREVIEW_RES;
    let (w, h) = (hf.metrics.width.max(1), hf.metrics.height.max(1));
    let mut heights = Vec::with_capacity((res * res) as usize);
    for ty in 0..res {
        for tx in 0..res {
            let i = (tx as u64 * (w as u64 - 1) / (res as u64 - 1)) as u32;
            let j = (ty as u64 * (h as u64 - 1) / (res as u64 - 1)) as u32;
            heights.push(hf.get(i.min(w - 1), j.min(h - 1)));
        }
    }
    LayerPreview {
        layer,
        generation,
        res,
        world_size_x: hf.metrics.world_size_x,
        heights,
    }
}

#[allow(clippy::large_enum_variant)]
enum WorkerMsg {
    Job(EvalWorkRequest),
    Shutdown,
}

/// Owns a dedicated thread with its own [`StackEvaluator`] and layer cache.
pub struct EvalWorker {
    tx: Sender<WorkerMsg>,
    rx: Receiver<EvalWorkResult>,
    /// Shared cancel / generation id - worker skips jobs with older tokens.
    pub current_token: Arc<AtomicU64>,
    _handle: JoinHandle<()>,
    /// True while a job may still be running (best-effort).
    pub busy: bool,
}

impl EvalWorker {
    pub fn spawn() -> Self {
        let (job_tx, job_rx) = mpsc::channel::<WorkerMsg>();
        let (result_tx, result_rx) = mpsc::channel::<EvalWorkResult>();
        let current_token = Arc::new(AtomicU64::new(0));
        let token_flag = Arc::clone(&current_token);

        let handle = thread::Builder::new()
            .name("terra-eval-worker".into())
            .spawn(move || {
                let mut evaluator = StackEvaluator::new();
                while let Ok(msg) = job_rx.recv() {
                    match msg {
                        WorkerMsg::Shutdown => break,
                        WorkerMsg::Job(job) => {
                            let live = token_flag.load(Ordering::Acquire);
                            if job.token != live {
                                continue;
                            }
                            match run_cpu_job(&mut evaluator, &job, &token_flag) {
                                Ok(result) => {
                                    let _ = result_tx.send(result);
                                }
                                Err(EvalError::Cancelled) => {}
                                Err(_) => {}
                            }
                        }
                    }
                }
            })
            .expect("spawn terra-eval-worker");

        Self {
            tx: job_tx,
            rx: result_rx,
            current_token,
            _handle: handle,
            busy: false,
        }
    }

    pub fn set_token(&self, token: u64) {
        self.current_token.store(token, Ordering::Release);
    }

    pub fn submit(&mut self, request: EvalWorkRequest) {
        self.set_token(request.token);
        self.busy = true;
        let _ = self.tx.send(WorkerMsg::Job(request));
    }

    /// Non-blocking poll for the newest completed result matching `token`.
    pub fn try_recv_matching(&mut self, token: u64) -> Option<EvalWorkResult> {
        let mut latest = None;
        loop {
            match self.rx.try_recv() {
                Ok(result) => {
                    if result.token == token {
                        latest = Some(result);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.busy = false;
                    break;
                }
            }
        }
        if latest.is_some() {
            self.busy = false;
        }
        latest
    }

    pub fn shutdown(&self) {
        let _ = self.tx.send(WorkerMsg::Shutdown);
    }
}

impl Drop for EvalWorker {
    fn drop(&mut self) {
        let _ = self.tx.send(WorkerMsg::Shutdown);
    }
}

fn run_cpu_job(
    evaluator: &mut StackEvaluator,
    job: &EvalWorkRequest,
    token_flag: &Arc<AtomicU64>,
) -> Result<EvalWorkResult, EvalError> {
    let t0 = Instant::now();
    if job.token != token_flag.load(Ordering::Acquire) {
        return Err(EvalError::Cancelled);
    }

    let res = job.quality.resolution(job.preview_res, job.export_res);
    let metrics = HeightfieldMetrics {
        width: res,
        height: res,
        world_size_x: job.base_metrics.world_size_x,
        world_size_z: job.base_metrics.world_size_z,
        tile_size: job.base_metrics.tile_size.min(res),
        halo: job.base_metrics.halo,
    };

    if job.mark_all_dirty {
        evaluator.mark_all_dirty(&job.stack);
    } else if let Some(id) = job.dirty_from {
        evaluator.mark_dirty_from(&job.stack, id);
    }

    let mut ctx = EvalContext::new(metrics);
    ctx.set_cancellation_generation(Arc::clone(token_flag), job.token);
    ctx.quality = job.quality;
    ctx.level_steps = job.level_steps.clone();
    ctx.mask_assets = job.masks.clone();
    ctx.set_aux_hashmap(job.aux.clone());
    if let Some(strata) = &job.strata {
        ctx.aux_maps.strata = Some(strata.clone());
    }
    // Bake masks against the prior composed DEM (same as EvalScheduler), not zeros.
    let reference;
    let reference_ref: &Heightfield = match &job.mask_reference {
        Some(prev)
            if prev.metrics.width == metrics.width && prev.metrics.height == metrics.height =>
        {
            prev.as_ref()
        }
        Some(prev) => {
            // Resolution changed between qualities - resample nearest for mask bake.
            reference = resample_height_nearest(prev.as_ref(), metrics);
            &reference
        }
        None => {
            reference = Heightfield::zeros(metrics);
            &reference
        }
    };
    ctx.masks = bake_mask_assets(&job.masks, reference_ref, metrics, &job.aux);

    // Cooperative cancel between layers.
    let hf = {
        if job.token != token_flag.load(Ordering::Acquire) {
            return Err(EvalError::Cancelled);
        }
        evaluator.rebuild_incremental(&job.stack, &mut ctx)?
    };

    if job.token != token_flag.load(Ordering::Acquire) {
        return Err(EvalError::Cancelled);
    }

    ctx.sync_aux_hashmap();
    let mut layer_previews = Vec::new();
    for id in job.stack.layer_ids() {
        if let Some(entry) = evaluator.cache.get(id) {
            if !entry.dirty && entry.height.metrics.width == metrics.width {
                layer_previews.push(downsample_preview(id, entry.generation, &entry.height));
            }
        }
    }
    Ok(EvalWorkResult {
        token: job.token,
        quality: job.quality,
        height: hf,
        aux: ctx.aux,
        strata: ctx.aux_maps.strata.clone(),
        eval_us: t0.elapsed().as_micros() as u64,
        layer_timings: ctx.layer_timings,
        layer_previews,
    })
}

fn resample_height_nearest(src: &Heightfield, dst: HeightfieldMetrics) -> Heightfield {
    let mut out = Heightfield::zeros(dst);
    if src.metrics.width == 0 || src.metrics.height == 0 {
        return out;
    }
    for j in 0..dst.height {
        for i in 0..dst.width {
            let u = (i as f32 + 0.5) / dst.width as f32;
            let v = (j as f32 + 0.5) / dst.height as f32;
            let si = ((u * src.metrics.width as f32) as u32).min(src.metrics.width - 1);
            let sj = ((v * src.metrics.height as f32) as u32).min(src.metrics.height - 1);
            out.set(i, j, src.get(si, sj));
        }
    }
    out.refresh_halos();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::{BlendMode, FlatParams, Layer, LayerKind};

    #[test]
    fn worker_produces_heightfield() {
        let mut worker = EvalWorker::spawn();
        let mut stack = LayerStack::new();
        stack.push(Layer::new(
            "Flat",
            LayerKind::Flat(FlatParams { height: 12.0 }),
        ));
        let token = 1;
        worker.submit(EvalWorkRequest {
            token,
            quality: PreviewQuality::Draft,
            stack,
            masks: Vec::new(),
            base_metrics: HeightfieldMetrics::preview_default(),
            level_steps: crate::analyze::LevelStepSettings::default(),
            preview_res: 256,
            export_res: 1024,
            aux: HashMap::new(),
            strata: None,
            mask_reference: None,
            dirty_from: None,
            mark_all_dirty: true,
        });
        let mut result = None;
        for _ in 0..200 {
            if let Some(r) = worker.try_recv_matching(token) {
                result = Some(r);
                break;
            }
            thread::sleep(std::time::Duration::from_millis(5));
        }
        let r = result.expect("worker result");
        assert_eq!(r.token, token);
        assert!(r.height.metrics.width > 0);
    }

    #[test]
    fn worker_height_mask_uses_layer_input_not_previous_frame() {
        use crate::mask::{MaskAsset, MaskId, MaskRef, MaskSource};

        let metrics = HeightfieldMetrics::new(32, 32, 320.0, 320.0);
        // Deliberately stale prior DEM. Point-of-use evaluation must follow the
        // Base layer below the mask consumer instead of this previous frame.
        let reference = Heightfield::zeros(metrics);

        let mask_id = MaskId::new();
        let asset = MaskAsset {
            id: mask_id,
            name: "High".into(),
            source: MaskSource::Height {
                min: 50.0,
                max: 100.0,
            },
            ops: Vec::new(),
            paint: None,
            display_color: crate::mask::default_mask_display_color(),
            owner: None,
        };

        let mut stack = LayerStack::new();
        stack.push(Layer::new(
            "Base",
            LayerKind::Flat(FlatParams { height: 100.0 }),
        ));
        let mut raise = Layer::new("Raise", LayerKind::Flat(FlatParams { height: 40.0 }));
        raise.common.blend = BlendMode::Add;
        raise.common.masks.push(MaskRef {
            id: mask_id,
            strength: 1.0,
            invert: false,
        });
        stack.push(raise);

        let mut worker = EvalWorker::spawn();
        let token = 2;
        worker.submit(EvalWorkRequest {
            token,
            quality: PreviewQuality::Full,
            stack,
            masks: vec![asset],
            base_metrics: metrics,
            level_steps: crate::analyze::LevelStepSettings::default(),
            preview_res: 32,
            export_res: 32,
            aux: HashMap::new(),
            strata: None,
            mask_reference: Some(std::sync::Arc::new(reference)),
            dirty_from: None,
            mark_all_dirty: true,
        });
        let mut result = None;
        for _ in 0..400 {
            if let Some(r) = worker.try_recv_matching(token) {
                result = Some(r);
                break;
            }
            thread::sleep(std::time::Duration::from_millis(5));
        }
        let r = result.expect("worker result");
        assert!((r.height.get(24, 16) - 140.0).abs() < 1.0e-4);
        assert!((r.height.get(8, 16) - 140.0).abs() < 1.0e-4);
    }

    #[test]
    fn persistent_worker_evaluator_reuses_clean_stack() {
        let metrics = HeightfieldMetrics::new(32, 32, 320.0, 320.0);
        let mut stack = LayerStack::new();
        stack.push(Layer::new(
            "Base",
            LayerKind::Flat(FlatParams { height: 25.0 }),
        ));
        let token = 7;
        let live = Arc::new(AtomicU64::new(token));
        let mut evaluator = StackEvaluator::new();
        let mut request = EvalWorkRequest {
            token,
            quality: PreviewQuality::Full,
            stack,
            masks: Vec::new(),
            base_metrics: metrics,
            level_steps: crate::analyze::LevelStepSettings::default(),
            preview_res: 32,
            export_res: 32,
            aux: HashMap::new(),
            strata: None,
            mask_reference: None,
            dirty_from: None,
            mark_all_dirty: true,
        };
        let first = run_cpu_job(&mut evaluator, &request, &live).expect("first build");
        assert_eq!(
            first
                .layer_timings
                .iter()
                .filter(|timing| timing.status == super::super::LayerEvalStatus::Computed)
                .count(),
            1
        );

        request.mark_all_dirty = false;
        let second = run_cpu_job(&mut evaluator, &request, &live).expect("cached build");
        assert_eq!(
            second
                .layer_timings
                .iter()
                .filter(|timing| timing.status == super::super::LayerEvalStatus::CacheHit)
                .count(),
            1
        );
        assert_eq!(second.height.get(16, 16), 25.0);
    }

    #[test]
    fn evaluation_context_observes_superseding_generation() {
        let generation = Arc::new(AtomicU64::new(11));
        let mut ctx = EvalContext::new(HeightfieldMetrics::new(8, 8, 80.0, 80.0));
        ctx.set_cancellation_generation(Arc::clone(&generation), 11);
        assert!(ctx.check_cancelled().is_ok());
        generation.store(12, Ordering::Release);
        assert!(matches!(ctx.check_cancelled(), Err(EvalError::Cancelled)));
    }
}
