use super::MaskField;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MaskOp {
    Add {
        amount: f32,
    },
    Subtract {
        amount: f32,
    },
    Multiply {
        amount: f32,
    },
    Min {
        value: f32,
    },
    Max {
        value: f32,
    },
    Invert,
    Clamp {
        min: f32,
        max: f32,
    },
    Levels {
        in_black: f32,
        in_white: f32,
        gamma: f32,
    },
    Smoothstep {
        edge0: f32,
        edge1: f32,
    },
    Blur {
        radius: u32,
    },
    Remap {
        out_min: f32,
        out_max: f32,
    },
}

impl MaskOp {
    /// Binary-ish apply when combining two fields; unary ops ignore `b`.
    pub fn apply(self, a: f32, b: f32) -> f32 {
        match self {
            MaskOp::Add { amount } => (a + b * amount).clamp(0.0, 1.0),
            MaskOp::Subtract { amount } => (a - b * amount).clamp(0.0, 1.0),
            MaskOp::Multiply { amount } => (a * (b * amount + (1.0 - amount))).clamp(0.0, 1.0),
            MaskOp::Min { value: _ } => a.min(b),
            MaskOp::Max { value: _ } => a.max(b),
            MaskOp::Invert => 1.0 - a,
            MaskOp::Clamp { min, max } => a.clamp(min, max),
            MaskOp::Levels {
                in_black,
                in_white,
                gamma,
            } => {
                let t = ((a - in_black) / (in_white - in_black).max(1e-6)).clamp(0.0, 1.0);
                t.powf(1.0 / gamma.max(1e-6))
            }
            MaskOp::Smoothstep { edge0, edge1 } => smoothstep(edge0, edge1, a),
            MaskOp::Blur { .. } => a,
            MaskOp::Remap { out_min, out_max } => out_min + a * (out_max - out_min),
        }
    }

    /// Unary apply against a single sample. `Min`/`Max` clamp against their
    /// stored `value` here, unlike the binary `apply` where the second field
    /// takes that role (and `value` is ignored).
    pub fn apply_unary(self, a: f32) -> f32 {
        match self {
            MaskOp::Min { value } => a.min(value),
            MaskOp::Max { value } => a.max(value),
            other => other.apply(a, a),
        }
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn apply_mask_ops(field: &mut MaskField, ops: &[MaskOp]) {
    for op in ops {
        match *op {
            MaskOp::Blur { radius } => blur_inplace(field, radius),
            MaskOp::Invert => {
                for v in field.data_mut() {
                    *v = 1.0 - *v;
                }
            }
            MaskOp::Clamp { min, max } => {
                crate::simd_ops::clamp_slice_in_place(field.data_mut(), min, max);
            }
            other => {
                for v in field.data_mut() {
                    *v = other.apply_unary(*v).clamp(0.0, 1.0);
                }
            }
        }
    }
}

/// Separable box blur, equivalent to averaging the full (2r+1)^2 window
/// clipped to the field bounds: the clipped window is a rectangle, so its
/// sum and count both factor into a horizontal and a vertical pass.
fn blur_inplace(field: &mut MaskField, radius: u32) {
    use rayon::prelude::*;

    if radius == 0 {
        return;
    }
    let w = field.metrics.width as usize;
    let h = field.metrics.height as usize;
    let r = radius as usize;

    let src = field.data().to_vec();
    let mut tmp = vec![0.0f32; w * h];

    // Horizontal pass: sliding-window row averages.
    tmp.par_chunks_mut(w).enumerate().for_each(|(j, row)| {
        let src_row = &src[j * w..(j + 1) * w];
        let mut sum: f32 = src_row[..(r + 1).min(w)].iter().sum();
        let mut count = (r + 1).min(w);
        for i in 0..w {
            row[i] = sum / count as f32;
            if i + r + 1 < w {
                sum += src_row[i + r + 1];
                count += 1;
            }
            if i >= r {
                sum -= src_row[i - r];
                count -= 1;
            }
        }
    });

    // Vertical pass: per-row output, each row averaging tmp rows in its window.
    field
        .data_mut()
        .par_chunks_mut(w)
        .enumerate()
        .for_each(|(j, out_row)| {
            let lo = j.saturating_sub(r);
            let hi = (j + r).min(h - 1);
            let inv = 1.0 / (hi - lo + 1) as f32;
            for i in 0..w {
                let mut sum = 0.0f32;
                for jj in lo..=hi {
                    sum += tmp[jj * w + i];
                }
                out_row[i] = sum * inv;
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::heightfield::HeightfieldMetrics;

    #[test]
    fn invert_and_clamp() {
        let m = HeightfieldMetrics::new(4, 4, 4.0, 4.0);
        let mut f = MaskField::filled(m, 0.25);
        apply_mask_ops(
            &mut f,
            &[MaskOp::Invert, MaskOp::Clamp { min: 0.0, max: 0.5 }],
        );
        assert!((f.get(0, 0) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn unary_min_max_clamp_against_value() {
        let m = HeightfieldMetrics::new(4, 4, 4.0, 4.0);
        let mut f = MaskField::filled(m, 0.8);
        apply_mask_ops(&mut f, &[MaskOp::Min { value: 0.3 }]);
        assert!((f.get(0, 0) - 0.3).abs() < 1e-5);
        apply_mask_ops(&mut f, &[MaskOp::Max { value: 0.6 }]);
        assert!((f.get(0, 0) - 0.6).abs() < 1e-5);
    }

    #[test]
    fn binary_min_max_take_second_field() {
        assert!((MaskOp::Min { value: 0.0 }.apply(0.8, 0.2) - 0.2).abs() < 1e-5);
        assert!((MaskOp::Max { value: 1.0 }.apply(0.1, 0.5) - 0.5).abs() < 1e-5);
    }

    #[test]
    fn blur_matches_naive_reference() {
        let m = HeightfieldMetrics::new(9, 7, 9.0, 7.0);
        let mut f = MaskField::filled(m, 0.0);
        // Deterministic non-uniform pattern.
        for j in 0..7u32 {
            for i in 0..9u32 {
                f.set(i, j, ((i * 31 + j * 17) % 11) as f32 / 10.0);
            }
        }
        for radius in [1u32, 2, 3] {
            let mut fast = f.clone();
            blur_inplace(&mut fast, radius);

            // Naive O(r^2) reference with the same edge-count normalization.
            let src = f.data().to_vec();
            let (w, h) = (9i32, 7i32);
            let r = radius as i32;
            for j in 0..h {
                for i in 0..w {
                    let mut sum = 0.0f32;
                    let mut count = 0.0f32;
                    for dj in -r..=r {
                        for di in -r..=r {
                            let (ii, jj) = (i + di, j + dj);
                            if ii >= 0 && jj >= 0 && ii < w && jj < h {
                                sum += src[(jj * w + ii) as usize];
                                count += 1.0;
                            }
                        }
                    }
                    let expect = sum / count;
                    let got = fast.get(i as u32, j as u32);
                    assert!(
                        (got - expect).abs() < 1e-4,
                        "r={radius} at ({i},{j}): {got} vs {expect}"
                    );
                }
            }
        }
    }

    #[test]
    fn levels_bounds() {
        let op = MaskOp::Levels {
            in_black: 0.0,
            in_white: 1.0,
            gamma: 1.0,
        };
        assert!((op.apply_unary(0.0) - 0.0).abs() < 1e-5);
        assert!((op.apply_unary(1.0) - 1.0).abs() < 1e-5);
    }
}
