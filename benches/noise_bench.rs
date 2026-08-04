//! Micro-benchmarks for noise (run with Criterion when added as a bench target).
//!
//! ```text
//! cargo bench -p terra-core
//! ```
//!
//! This file documents intended benches; wire via `[[bench]]` when Criterion is enabled.

use terra_core::layer::{FractalNoiseType, NoiseParams};
use terra_core::noise::fbm;

fn main() {
    let p = NoiseParams {
        octaves: 6,
        frequency: 0.002,
        ..NoiseParams::default()
    };
    let mut acc = 0.0f32;
    for i in 0..512 {
        for j in 0..512 {
            acc += fbm(FractalNoiseType::Perlin, i as f32, j as f32, &p);
        }
    }
    std::hint::black_box(acc);
}
