//! Import/export and background build jobs.

mod export;
mod geotiff;
mod import;

pub use export::{export_package, ExportRequest, ExportResult};
pub use geotiff::{read_geotiff_heights, GeoTiffInfo};
pub use import::{import_heightmap_png, import_heightmap_raw};

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use terra_core::document::TerrainDocument;
use terra_core::eval::{EvalContext, StackEvaluator};
use terra_core::heightfield::HeightfieldMetrics;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum IoError {
    #[error("{0}")]
    Msg(String),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub struct BuildJob {
    pub progress: f32,
    pub done: bool,
    pub result: Option<Result<ExportResult, String>>,
}

enum JobMsg {
    Progress(f32),
    Done(Result<ExportResult, String>),
}

/// Non-blocking export worker.
pub struct BackgroundExporter {
    tx: Option<Sender<JobMsg>>,
    rx: Receiver<JobMsg>,
    pub job: BuildJob,
}

impl BackgroundExporter {
    pub fn new() -> Self {
        let (_tx, rx) = mpsc::channel();
        Self {
            tx: None,
            rx,
            job: BuildJob {
                progress: 0.0,
                done: true,
                result: None,
            },
        }
    }

    pub fn start(&mut self, doc: TerrainDocument, out_dir: PathBuf) {
        let (tx, rx) = mpsc::channel();
        self.rx = rx;
        self.tx = Some(tx.clone());
        self.job = BuildJob {
            progress: 0.0,
            done: false,
            result: None,
        };
        thread::spawn(move || {
            let _ = tx.send(JobMsg::Progress(0.1));
            let metrics = HeightfieldMetrics {
                width: doc.export_resolution,
                height: doc.export_resolution,
                world_size_x: doc.metrics.world_size_x,
                world_size_z: doc.metrics.world_size_z,
                tile_size: doc.metrics.tile_size.min(doc.export_resolution),
                halo: doc.metrics.halo,
            };
            let mut eval = StackEvaluator::new();
            let mut ctx = EvalContext::new(metrics);
            let reference = terra_core::heightfield::Heightfield::zeros(metrics);
            ctx.masks = terra_core::mask::bake_mask_assets(
                &doc.masks,
                &reference,
                metrics,
                &std::collections::HashMap::new(),
            );
            ctx.mask_assets = doc.masks.clone();
            let _ = tx.send(JobMsg::Progress(0.3));
            match eval.rebuild_all(&doc.stack, &mut ctx) {
                Ok(hf) => {
                    let _ = tx.send(JobMsg::Progress(0.7));
                    let req = ExportRequest {
                        out_dir,
                        include_masks: true,
                    };
                    let result = export_package(&hf, &ctx, &req).map_err(|e| e.to_string());
                    let _ = tx.send(JobMsg::Done(result));
                }
                Err(e) => {
                    let _ = tx.send(JobMsg::Done(Err(e.to_string())));
                }
            }
        });
    }

    pub fn poll(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                JobMsg::Progress(p) => self.job.progress = p,
                JobMsg::Done(r) => {
                    self.job.progress = 1.0;
                    self.job.done = true;
                    self.job.result = Some(r);
                }
            }
        }
    }
}

impl Default for BackgroundExporter {
    fn default() -> Self {
        Self::new()
    }
}

pub fn save_project(doc: &TerrainDocument, path: &std::path::Path) -> Result<(), IoError> {
    let json = doc.to_json()?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn load_project(path: &std::path::Path) -> Result<TerrainDocument, IoError> {
    let s = std::fs::read_to_string(path)?;
    Ok(TerrainDocument::from_json(&s)?)
}
