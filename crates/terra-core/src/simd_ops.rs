//! Stable SIMD-friendly bulk float ops for hot height/mask paths.
//!
//! Uses explicit 4-wide chunks (no nightly `portable_simd`) so Release builds
//! auto-vectorize cleanly while Debug stays deterministic.

/// Saturating lerp of two equal-length slices: `out = a + (b - a) * t`.
pub fn lerp_slices(a: &[f32], b: &[f32], t: f32, out: &mut [f32]) {
    assert_eq!(a.len(), b.len());
    assert_eq!(a.len(), out.len());
    let t = t.clamp(0.0, 1.0);
    let n = a.len();
    let mut i = 0;
    while i + 4 <= n {
        out[i] = a[i] + (b[i] - a[i]) * t;
        out[i + 1] = a[i + 1] + (b[i + 1] - a[i + 1]) * t;
        out[i + 2] = a[i + 2] + (b[i + 2] - a[i + 2]) * t;
        out[i + 3] = a[i + 3] + (b[i + 3] - a[i + 3]) * t;
        i += 4;
    }
    while i < n {
        out[i] = a[i] + (b[i] - a[i]) * t;
        i += 1;
    }
}

/// `out[i] = a[i] * w + b[i] * (1 - w)` with broadcast weight.
pub fn mix_slices(a: &[f32], b: &[f32], w: f32, out: &mut [f32]) {
    lerp_slices(b, a, w, out);
}

/// Scale a slice in place by `s` using 4-wide chunks.
pub fn scale_slice_in_place(data: &mut [f32], s: f32) {
    let n = data.len();
    let mut i = 0;
    while i + 4 <= n {
        data[i] *= s;
        data[i + 1] *= s;
        data[i + 2] *= s;
        data[i + 3] *= s;
        i += 4;
    }
    while i < n {
        data[i] *= s;
        i += 1;
    }
}

/// Add `src` into `dst` (equal lengths) with 4-wide chunks.
pub fn add_assign_slices(dst: &mut [f32], src: &[f32]) {
    assert_eq!(dst.len(), src.len());
    let n = dst.len();
    let mut i = 0;
    while i + 4 <= n {
        dst[i] += src[i];
        dst[i + 1] += src[i + 1];
        dst[i + 2] += src[i + 2];
        dst[i + 3] += src[i + 3];
        i += 4;
    }
    while i < n {
        dst[i] += src[i];
        i += 1;
    }
}

/// Clamp every element of `data` to `[lo, hi]`.
pub fn clamp_slice_in_place(data: &mut [f32], lo: f32, hi: f32) {
    let n = data.len();
    let mut i = 0;
    while i + 4 <= n {
        data[i] = data[i].clamp(lo, hi);
        data[i + 1] = data[i + 1].clamp(lo, hi);
        data[i + 2] = data[i + 2].clamp(lo, hi);
        data[i + 3] = data[i + 3].clamp(lo, hi);
        i += 4;
    }
    while i < n {
        data[i] = data[i].clamp(lo, hi);
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_matches_scalar() {
        let a = [0.0f32, 1.0, 2.0, 3.0, 4.0];
        let b = [10.0f32, 11.0, 12.0, 13.0, 14.0];
        let mut out = [0.0f32; 5];
        lerp_slices(&a, &b, 0.5, &mut out);
        assert!((out[0] - 5.0).abs() < 1e-5);
        assert!((out[4] - 9.0).abs() < 1e-5);
    }
}
