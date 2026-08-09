//! Synthetic terrains for geomorph validation.

use crate::heightfield::{Heightfield, HeightfieldMetrics};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntheticKind {
    Plane,
    Cone,
    SingleValley,
    SingleBasin,
    TwoBasins,
    NoisyMountain,
    ClosedDepression,
}

impl SyntheticKind {
    pub fn build(self, metrics: HeightfieldMetrics) -> Heightfield {
        match self {
            Self::Plane => plane(metrics),
            Self::Cone => cone(metrics),
            Self::SingleValley => single_valley(metrics),
            Self::SingleBasin => single_basin(metrics),
            Self::TwoBasins => two_basins(metrics),
            Self::NoisyMountain => noisy_mountain(metrics),
            Self::ClosedDepression => closed_depression(metrics),
        }
    }
}

/// Constant-slope plane draining +X.
pub fn plane(metrics: HeightfieldMetrics) -> Heightfield {
    let mut hf = Heightfield::zeros(metrics);
    for j in 0..metrics.height {
        for i in 0..metrics.width {
            hf.set(i, j, (metrics.width - 1 - i) as f32);
        }
    }
    hf
}

/// Cone peak at centre.
pub fn cone(metrics: HeightfieldMetrics) -> Heightfield {
    let mut hf = Heightfield::zeros(metrics);
    let cx = (metrics.width as f32 - 1.0) * 0.5;
    let cz = (metrics.height as f32 - 1.0) * 0.5;
    let max_r = cx.hypot(cz).max(1.0);
    for j in 0..metrics.height {
        for i in 0..metrics.width {
            let r = (i as f32 - cx).hypot(j as f32 - cz);
            hf.set(i, j, (1.0 - r / max_r).max(0.0) * 50.0);
        }
    }
    hf
}

/// Single V-shaped valley draining south.
pub fn single_valley(metrics: HeightfieldMetrics) -> Heightfield {
    let mut hf = Heightfield::zeros(metrics);
    let cx = (metrics.width as f32 - 1.0) * 0.5;
    for j in 0..metrics.height {
        for i in 0..metrics.width {
            let dx = (i as f32 - cx).abs();
            hf.set(i, j, dx + (metrics.height - 1 - j) as f32 * 0.15);
        }
    }
    hf
}

/// Bowl / basin with rim, draining if filled to border.
pub fn single_basin(metrics: HeightfieldMetrics) -> Heightfield {
    let mut hf = Heightfield::zeros(metrics);
    let cx = (metrics.width as f32 - 1.0) * 0.5;
    let cz = (metrics.height as f32 - 1.0) * 0.5;
    for j in 0..metrics.height {
        for i in 0..metrics.width {
            let r = (i as f32 - cx).hypot(j as f32 - cz);
            hf.set(i, j, r * 2.0);
        }
    }
    hf
}

/// Two basins separated by a central N–S ridge.
pub fn two_basins(metrics: HeightfieldMetrics) -> Heightfield {
    let mut hf = Heightfield::zeros(metrics);
    let mid = metrics.width / 2;
    let cz = (metrics.height as f32 - 1.0) * 0.5;
    for j in 0..metrics.height {
        for i in 0..metrics.width {
            let ridge = 1.0 - ((i as i32 - mid as i32).abs() as f32 / mid.max(1) as f32);
            let left = ((i as f32) / mid.max(1) as f32 - 0.5).abs() * 20.0;
            let right = (((i as f32) - mid as f32) / mid.max(1) as f32 - 0.5).abs() * 20.0;
            let basin = if i < mid { left } else { right };
            let wall = if (i as i32 - mid as i32).abs() <= 1 {
                30.0
            } else {
                0.0
            };
            let edge = (j as f32 - cz).abs() * 0.05;
            hf.set(i, j, basin + wall + edge + ridge * 0.01);
        }
    }
    hf
}

/// Noisy mountain (deterministic hash noise on a dome).
pub fn noisy_mountain(metrics: HeightfieldMetrics) -> Heightfield {
    let mut hf = cone(metrics);
    for j in 0..metrics.height {
        for i in 0..metrics.width {
            let n = hash_noise(i, j, 42) * 4.0;
            hf.set(i, j, hf.get(i, j) + n);
        }
    }
    hf
}

/// Closed depression (pit) in an otherwise draining plane.
pub fn closed_depression(metrics: HeightfieldMetrics) -> Heightfield {
    let mut hf = plane(metrics);
    let cx = metrics.width / 2;
    let cz = metrics.height / 2;
    for j in cz.saturating_sub(2)..=(cz + 2).min(metrics.height - 1) {
        for i in cx.saturating_sub(2)..=(cx + 2).min(metrics.width - 1) {
            hf.set(i, j, hf.get(i, j) - 8.0);
        }
    }
    hf.set(cx, cz, hf.get(cx, cz) - 4.0);
    hf
}

fn hash_noise(i: u32, j: u32, seed: u32) -> f32 {
    let mut x = i.wrapping_mul(374761393) ^ j.wrapping_mul(668265263) ^ seed;
    x = (x ^ (x >> 13)).wrapping_mul(1274126177);
    (x & 0xffff) as f32 / 65535.0 - 0.5
}
