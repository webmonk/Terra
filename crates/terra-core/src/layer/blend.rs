use serde::{Deserialize, Serialize};

/// Height blend modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BlendMode {
    /// Replace contribution (legacy name for Replace).
    #[default]
    Normal,
    /// Explicit alias used by newer UI (same as Normal).
    Replace,
    Add,
    Subtract,
    Multiply,
    Min,
    Max,
    /// Interpolate toward contribution (same math as Normal with opacity).
    Interpolate,
    /// Height-aware blend favoring the higher contribution smoothly.
    HeightBlend,
    Overlay,
    SmoothMaximum,
    SmoothMinimum,
    SmoothUnion,
    SmoothSubtraction,
}

impl BlendMode {
    pub fn label(self) -> &'static str {
        match self {
            BlendMode::Normal | BlendMode::Replace => "Replace",
            BlendMode::Add => "Add",
            BlendMode::Subtract => "Subtract",
            BlendMode::Multiply => "Multiply",
            BlendMode::Min => "Minimum",
            BlendMode::Max => "Maximum",
            BlendMode::Interpolate => "Interpolate",
            BlendMode::HeightBlend => "Height Blend",
            BlendMode::Overlay => "Overlay",
            BlendMode::SmoothMaximum => "Smooth Maximum",
            BlendMode::SmoothMinimum => "Smooth Minimum",
            BlendMode::SmoothUnion => "Smooth Union",
            BlendMode::SmoothSubtraction => "Smooth Subtraction",
        }
    }

    /// Modes appropriate for scalar height compositing.
    pub fn height_modes() -> &'static [BlendMode] {
        &[
            BlendMode::Normal,
            BlendMode::Add,
            BlendMode::Subtract,
            BlendMode::Multiply,
            BlendMode::Min,
            BlendMode::Max,
            BlendMode::Interpolate,
            BlendMode::HeightBlend,
            BlendMode::SmoothMaximum,
            BlendMode::SmoothMinimum,
            BlendMode::SmoothUnion,
            BlendMode::SmoothSubtraction,
        ]
    }

    /// Modes appropriate for \[0,1\] mask / weight fields (no smooth SDF ops).
    pub fn weight_modes() -> &'static [BlendMode] {
        &[
            BlendMode::Normal,
            BlendMode::Add,
            BlendMode::Subtract,
            BlendMode::Multiply,
            BlendMode::Min,
            BlendMode::Max,
            BlendMode::Interpolate,
        ]
    }
}

fn smooth_min(a: f32, b: f32, k: f32) -> f32 {
    let k = k.max(1e-4);
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    b.lerp(a, h) - k * h * (1.0 - h)
}

fn smooth_max(a: f32, b: f32, k: f32) -> f32 {
    -smooth_min(-a, -b, k)
}

trait Lerp {
    fn lerp(self, other: Self, t: f32) -> Self;
}

impl Lerp for f32 {
    fn lerp(self, other: Self, t: f32) -> Self {
        self * (1.0 - t) + other * t
    }
}

/// Mode-specific combine of accumulated `a` and layer contribution `b`.
pub fn blend_pair(mode: BlendMode, a: f32, b: f32) -> f32 {
    const K: f32 = 8.0; // world-metre smoothness for smooth SDF-style ops
    match mode {
        BlendMode::Normal | BlendMode::Replace | BlendMode::Interpolate => b,
        BlendMode::Add => a + b,
        BlendMode::Subtract => a - b,
        BlendMode::Multiply => a * b,
        BlendMode::Min => a.min(b),
        BlendMode::Max => a.max(b),
        BlendMode::HeightBlend => {
            // Favor the higher of a and blended b based on relative height.
            let t = ((b - a) * 0.05 + 0.5).clamp(0.0, 1.0);
            a * (1.0 - t) + b.max(a) * t
        }
        BlendMode::Overlay => {
            if a < 0.0 {
                2.0 * a * b
            } else {
                a + b - a * b / (a.abs() + 1.0)
            }
        }
        BlendMode::SmoothMaximum | BlendMode::SmoothUnion => smooth_max(a, b, K),
        BlendMode::SmoothMinimum => smooth_min(a, b, K),
        BlendMode::SmoothSubtraction => smooth_max(a, -b, K),
    }
}

/// `H_out = mix(H_in, blend(H_in, H_layer), w)` with `w = opacity * mask`.
pub fn blend_heights(mode: BlendMode, h_in: f32, h_layer: f32, opacity: f32, mask: f32) -> f32 {
    let w = (opacity * mask).clamp(0.0, 1.0);
    let blended = blend_pair(mode, h_in, h_layer);
    h_in * (1.0 - w) + blended * w
}

/// Field-specific compositing for weight maps (materials / biomes).
pub fn blend_weights(mode: BlendMode, a: f32, b: f32, opacity: f32, mask: f32) -> f32 {
    let mode = match mode {
        BlendMode::SmoothMaximum
        | BlendMode::SmoothMinimum
        | BlendMode::SmoothUnion
        | BlendMode::SmoothSubtraction
        | BlendMode::HeightBlend
        | BlendMode::Overlay => BlendMode::Interpolate,
        other => other,
    };
    blend_heights(mode, a, b, opacity, mask).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opacity_zero_is_identity() {
        for mode in [
            BlendMode::Normal,
            BlendMode::Add,
            BlendMode::Subtract,
            BlendMode::Multiply,
            BlendMode::Min,
            BlendMode::Max,
            BlendMode::SmoothUnion,
        ] {
            let out = blend_heights(mode, 10.0, 99.0, 0.0, 1.0);
            assert_eq!(out, 10.0);
        }
    }

    #[test]
    fn opacity_one_normal_replaces() {
        assert_eq!(blend_heights(BlendMode::Normal, 10.0, 5.0, 1.0, 1.0), 5.0);
        assert_eq!(blend_heights(BlendMode::Replace, 10.0, 5.0, 1.0, 1.0), 5.0);
    }

    #[test]
    fn opacity_half_normal() {
        let out = blend_heights(BlendMode::Normal, 0.0, 10.0, 0.5, 1.0);
        assert!((out - 5.0).abs() < 1e-5);
    }

    #[test]
    fn add_mode() {
        let out = blend_heights(BlendMode::Add, 10.0, 3.0, 1.0, 1.0);
        assert!((out - 13.0).abs() < 1e-5);
    }

    #[test]
    fn mask_scales_opacity() {
        let out = blend_heights(BlendMode::Normal, 0.0, 10.0, 1.0, 0.25);
        assert!((out - 2.5).abs() < 1e-5);
    }

    #[test]
    fn smooth_max_between() {
        let out = blend_pair(BlendMode::SmoothMaximum, 10.0, 20.0);
        assert!(out > 10.0 && out <= 20.0 + 1.0);
    }
}
