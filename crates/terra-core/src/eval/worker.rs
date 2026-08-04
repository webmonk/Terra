//! Background CPU evaluation worker (Gaea-style engine/UI split).
//!
//! Medium/Full refine jobs run off the UI thread. Draft may still run sync for snappy feedback.
//! Jobs are cancelled by bumping `current_token` — stale results are discarded by the app.

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
    pub preview_res: u32,
    pub export_res: u32,
    pub aux: HashMap<String, MaskField>,
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
    pub eval_us: u64,
}

enum WorkerMsg {
    Job(EvalWorkRequest),
    Shutdown,
}

/// Owns a dedicated thread with its own [`StackEvaluator`] and layer cache.
pub struct EvalWorker {
    tx: Sender<WorkerMsg>,
    rx: Receiver<EvalWorkResult>,
    /// Shared cancel / generation id — worker skips jobs with older tokens.
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
    token_flag: &AtomicU64,
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
    } else {
        evaluator.mark_all_dirty(&job.stack);
    }

    let mut ctx = EvalContext::new(metrics);
    ctx.quality = job.quality;
    ctx.mask_assets = job.masks.clone();
    ctx.aux = job.aux.clone();
    let reference = Heightfield::zeros(metrics);
    ctx.masks = bake_mask_assets(&job.masks, &reference, metrics, &job.aux);

    // Cooperative cancel between layers.
    let hf = {
        let layers = job.stack.flatten_layers();
        // Use incremental rebuild; check token periodically via cancelled flag.
        if job.token != token_flag.load(Ordering::Acquire) {
            return Err(EvalError::Cancelled);
        }
        let _ = layers;
        evaluator.rebuild_incremental(&job.stack, &mut ctx)?
    };

    if job.token != token_flag.load(Ordering::Acquire) {
        return Err(EvalError::Cancelled);
    }

    Ok(EvalWorkResult {
        token: job.token,
        quality: job.quality,
        height: hf,
        aux: ctx.aux,
        eval_us: t0.elapsed().as_micros() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::{FlatParams, Layer, LayerKind};

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
            preview_res: 256,
            export_res: 1024,
            aux: HashMap::new(),
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
}
