//! Deterministic CPU noise primitives.

mod perlin;
mod simplex;
mod value;
mod worley;

pub use perlin::perlin2;
pub use simplex::open_simplex2;
pub use value::value_noise2;
pub use worley::{worley2, WorleyResult};

use crate::layer::{FractalNoiseType, NoiseParams, WorleyFeature, WorleyParams};

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846ca68b);
    x ^= x >> 16;
    x
}

pub(crate) fn hash2(ix: i32, iz: i32, seed: u32) -> u32 {
    let mut h = seed;
    h ^= ix as u32;
    h = hash_u32(h);
    h ^= iz as u32;
    h = hash_u32(h);
    h
}

pub(crate) fn fade(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

pub(crate) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn remap(v: f32, in_min: f32, in_max: f32, out_min: f32, out_max: f32) -> f32 {
    let t = ((v - in_min) / (in_max - in_min).max(1e-6)).clamp(0.0, 1.0);
    out_min + t * (out_max - out_min)
}

pub fn sample_noise(kind: FractalNoiseType, x: f32, z: f32, seed: u64) -> f32 {
    match kind {
        FractalNoiseType::Value => value_noise2(x, z, seed),
        FractalNoiseType::Perlin => perlin2(x, z, seed),
        FractalNoiseType::OpenSimplex => open_simplex2(x, z, seed),
    }
}

pub fn fbm(kind: FractalNoiseType, x: f32, z: f32, params: &NoiseParams) -> f32 {
    let mut amp = 1.0;
    let mut freq = params.frequency;
    let mut sum = 0.0;
    let mut norm = 0.0;
    for o in 0..params.octaves.max(1) {
        let n = sample_noise(
            kind,
            (x + params.offset_x) * freq,
            (z + params.offset_z) * freq,
            params.seed.wrapping_add(o as u64 * 1013),
        );
        sum += n * amp;
        norm += amp;
        amp *= params.persistence;
        freq *= params.lacunarity;
    }
    let v = if norm > 0.0 { sum / norm } else { 0.0 };
    remap(v, params.remap_min, params.remap_max, -1.0, 1.0) * params.amplitude
}

pub fn ridged_mf(kind: FractalNoiseType, x: f32, z: f32, params: &NoiseParams) -> f32 {
    let mut amp = 1.0;
    let mut freq = params.frequency;
    let mut sum = 0.0;
    let mut weight = 1.0;
    for o in 0..params.octaves.max(1) {
        let mut n = sample_noise(
            kind,
            (x + params.offset_x) * freq,
            (z + params.offset_z) * freq,
            params.seed.wrapping_add(o as u64 * 9173),
        );
        n = 1.0 - n.abs();
        n *= n;
        n *= weight;
        weight = (n * 2.0).clamp(0.0, 1.0);
        sum += n * amp;
        amp *= params.persistence;
        freq *= params.lacunarity;
    }
    sum * params.amplitude
}

pub fn domain_warp_fbm(
    x: f32,
    z: f32,
    params: &NoiseParams,
    warp_strength: f32,
    warp_frequency: f32,
) -> f32 {
    let wx = perlin2(
        (x + params.offset_x) * warp_frequency,
        (z + params.offset_z) * warp_frequency,
        params.seed,
    ) * warp_strength;
    let wz = perlin2(
        (x + params.offset_x) * warp_frequency + 19.1,
        (z + params.offset_z) * warp_frequency + 7.3,
        params.seed.wrapping_add(1),
    ) * warp_strength;
    fbm(FractalNoiseType::Perlin, x + wx, z + wz, params)
}

pub fn sample_worley(x: f32, z: f32, params: &WorleyParams) -> f32 {
    let p = &params.base;
    let r = worley2(
        (x + p.offset_x) * p.frequency,
        (z + p.offset_z) * p.frequency,
        p.seed,
        params.distance_metric,
    );
    let v = match params.feature {
        WorleyFeature::F1 => r.f1,
        WorleyFeature::F2 => r.f2,
        WorleyFeature::F2MinusF1 => r.f2 - r.f1,
    };
    // Worley distances are typically ~0..1.5 in cell space; remap roughly.
    let n = remap(v, 0.0, 1.2, p.remap_min, p.remap_max);
    n * p.amplitude
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_value_noise() {
        let a = value_noise2(1.25, 3.5, 42);
        let b = value_noise2(1.25, 3.5, 42);
        assert_eq!(a, b);
    }

    #[test]
    fn fbm_finite() {
        let p = NoiseParams {
            octaves: 4,
            frequency: 0.01,
            ..NoiseParams::default()
        };
        let v = fbm(FractalNoiseType::Perlin, 10.0, 20.0, &p);
        assert!(v.is_finite());
    }

    #[test]
    fn domain_warp_finite() {
        let p = NoiseParams::default();
        let v = domain_warp_fbm(5.0, 8.0, &p, 50.0, 0.02);
        assert!(v.is_finite());
    }
}
