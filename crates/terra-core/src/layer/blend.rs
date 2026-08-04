use serde::{Deserialize, Serialize};

/// Height blend modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BlendMode {
    #[default]
    Normal,
    Add,
    Subtract,
    Multiply,
    Min,
    Max,
    Overlay,
}

/// Mode-specific combine of accumulated `a` and layer contribution `b`.
pub fn blend_pair(mode: BlendMode, a: f32, b: f32) -> f32 {
    match mode {
        BlendMode::Normal => b,
        BlendMode::Add => a + b,
        BlendMode::Subtract => a - b,
        BlendMode::Multiply => a * b,
        BlendMode::Min => a.min(b),
        BlendMode::Max => a.max(b),
        BlendMode::Overlay => {
            // Height-space overlay: mix around mid of a.
            if a < 0.0 {
                2.0 * a * b
            } else {
                a + b - a * b / (a.abs() + 1.0)
            }
        }
    }
}

/// `H_out = mix(H_in, blend(H_in, H_layer), w)` with `w = opacity * mask`.
pub fn blend_heights(mode: BlendMode, h_in: f32, h_layer: f32, opacity: f32, mask: f32) -> f32 {
    let w = (opacity * mask).clamp(0.0, 1.0);
    let blended = blend_pair(mode, h_in, h_layer);
    h_in * (1.0 - w) + blended * w
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
        ] {
            let out = blend_heights(mode, 10.0, 99.0, 0.0, 1.0);
            assert_eq!(out, 10.0);
        }
    }

    #[test]
    fn opacity_one_normal_replaces() {
        assert_eq!(blend_heights(BlendMode::Normal, 10.0, 5.0, 1.0, 1.0), 5.0);
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
}
