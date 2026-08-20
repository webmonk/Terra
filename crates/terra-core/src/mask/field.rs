use crate::heightfield::{Heightfield, HeightfieldMetrics};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Single-channel mask in \[0, 1\], same tiling as heightfields.
///
/// Samples are shared copy-on-write: `clone` is a refcount bump (layer caches
/// snapshot dozens of these per layer) and the first mutation after a clone
/// copies the buffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaskField {
    pub metrics: HeightfieldMetrics,
    #[serde(with = "crate::heightfield::cow_samples")]
    data: Arc<Vec<f32>>,
}

impl MaskField {
    pub fn new(metrics: HeightfieldMetrics) -> Self {
        let n = (metrics.width * metrics.height) as usize;
        Self {
            metrics,
            data: Arc::new(vec![1.0; n]),
        }
    }

    pub fn filled(metrics: HeightfieldMetrics, value: f32) -> Self {
        let n = (metrics.width * metrics.height) as usize;
        Self {
            metrics,
            data: Arc::new(vec![value.clamp(0.0, 1.0); n]),
        }
    }

    pub fn ones(metrics: HeightfieldMetrics) -> Self {
        Self::filled(metrics, 1.0)
    }

    pub fn zeros(metrics: HeightfieldMetrics) -> Self {
        Self::filled(metrics, 0.0)
    }

    /// Build from raw samples without \[0,1\] clamping (debug / sim state).
    pub fn from_raw(metrics: HeightfieldMetrics, data: &[f32]) -> Self {
        assert_eq!(data.len(), (metrics.width * metrics.height) as usize);
        Self {
            metrics,
            data: Arc::new(data.to_vec()),
        }
    }

    #[inline]
    fn idx(&self, i: u32, j: u32) -> usize {
        (j * self.metrics.width + i) as usize
    }

    pub fn get(&self, i: u32, j: u32) -> f32 {
        self.data[self.idx(i, j)]
    }

    pub fn set(&mut self, i: u32, j: u32, v: f32) {
        let i = self.idx(i, j);
        Arc::make_mut(&mut self.data)[i] = v.clamp(0.0, 1.0);
    }

    pub fn data(&self) -> &[f32] {
        &self.data
    }

    pub fn data_mut(&mut self) -> &mut [f32] {
        Arc::make_mut(&mut self.data).as_mut_slice()
    }

    pub fn from_height_range(hf: &Heightfield, min: f32, max: f32) -> Self {
        let metrics = hf.metrics;
        let mut m = Self::zeros(metrics);
        let span = (max - min).max(1e-6);
        for j in 0..metrics.height {
            for i in 0..metrics.width {
                let h = hf.get(i, j);
                let t = ((h - min) / span).clamp(0.0, 1.0);
                m.set(i, j, t);
            }
        }
        m
    }

    pub fn combine(&self, other: &Self, op: super::MaskOp) -> Self {
        assert_eq!(self.metrics.width, other.metrics.width);
        let data = self
            .data
            .iter()
            .zip(other.data.iter())
            .map(|(&a, &b)| op.apply(a, b))
            .collect();
        Self {
            metrics: self.metrics,
            data: Arc::new(data),
        }
    }

    /// Scale all samples in place with a 4-wide friendly loop.
    pub fn scale_in_place(&mut self, s: f32) {
        crate::simd_ops::scale_slice_in_place(self.data_mut(), s);
        crate::simd_ops::clamp_slice_in_place(self.data_mut(), 0.0, 1.0);
    }
}
