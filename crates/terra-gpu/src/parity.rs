//! Small CPU oracle checks for GPU-preview parity.

/// Largest element-wise absolute difference between equally sized slices.
pub fn max_abs_diff(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "parity inputs must have equal lengths");
    a.iter()
        .zip(b)
        .map(|(left, right)| (left - right).abs())
        .fold(0.0, f32::max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use terra_core::analyze::thermal_erode;
    use terra_core::generators::terrace;
    use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
    use terra_core::layer::{TerraceParams, ThermalErosionParams};

    #[test]
    fn cpu_thermal_reference_runs_on_small_field() {
        // The complete GPU/CPU thermal comparison is integration-tested because it
        // requires texture readback. Touching the shared headless device here
        // verifies adapter setup when available while keeping headless test
        // environments portable (the harness returns None instead of failing).
        let _ = terra_test_gpu::headless();

        let metrics = HeightfieldMetrics::new(64, 64, 64.0, 64.0);
        let mut input = Heightfield::zeros(metrics);
        input.set(32, 32, 100.0);
        let (out, _, _) = thermal_erode(
            &input,
            &ThermalErosionParams {
                iterations: 2,
                ..ThermalErosionParams::default()
            },
        );
        assert!(out.get(32, 32) < input.get(32, 32));
    }

    #[test]
    fn cpu_terrace_is_deterministic() {
        let metrics = HeightfieldMetrics::new(64, 64, 64.0, 64.0);
        let values: Vec<f32> = (0..64 * 64).map(|i| i as f32 / 64.0).collect();
        let input = Heightfield::from_dense(metrics, &values);
        let params = TerraceParams::default();
        let first = terrace(&input, &params).to_dense();
        let second = terrace(&input, &params).to_dense();
        assert_eq!(max_abs_diff(&first, &second), 0.0);
        // GPU float arithmetic is checked in integration tests with a small tolerance.
    }
}
