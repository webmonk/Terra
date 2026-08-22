//! General prop / object scatter parameters (`LayerKind::ScatterObjects`).

use serde::{Deserialize, Serialize};

/// One placed prop in world space (Y-up, metres).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ObjectInstance {
    /// Index into [`ScatterObjectsParams::classes`].
    pub class_index: u32,
    /// Class name copied for export consumers that have no params handy.
    pub class: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Uniform scale multiplier from the class scale range.
    pub scale: f32,
    /// Yaw in radians around +Y.
    pub yaw_rad: f32,
    /// Up vector: the surface normal when the class aligns to it, else +Y.
    pub normal: [f32; 3],
}

/// One prop family placed by a scatter layer.
///
/// Filters are per class so a single layer can mix, say, boulders on steep
/// ground with crates on flats without stacking several scatter layers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectClass {
    pub name: String,
    /// Relative selection weight among the classes eligible at a site.
    pub weight: f32,
    /// Probability a site that selected this class actually keeps it \[0,1\].
    pub density: f32,
    /// Minimum distance between two instances of this class (metres).
    pub min_spacing_m: f32,
    /// Per-instance uniform scale range \[min, max\].
    pub scale_range: [f32; 2],
    /// Max yaw variation in degrees (+/-); 180 ~ full random yaw.
    pub yaw_jitter_deg: f32,
    /// Tilt instances onto the surface normal instead of standing upright.
    pub align_to_normal: bool,
    /// Reject sites steeper than this (degrees).
    pub max_slope_deg: f32,
    /// Allowed terrain elevation band in metres \[min, max\].
    pub height_range: [f32; 2],
    /// Muted classes stay authored but place nothing.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Wide-open elevation band (matches the materials open-range convention).
pub const OBJECT_HEIGHT_MIN: f32 = -1_000_000.0;
/// See [`OBJECT_HEIGHT_MIN`].
pub const OBJECT_HEIGHT_MAX: f32 = 1_000_000.0;

impl Default for ObjectClass {
    fn default() -> Self {
        Self {
            name: "Object".into(),
            weight: 1.0,
            density: 0.5,
            min_spacing_m: 6.0,
            scale_range: [0.8, 1.25],
            yaw_jitter_deg: 180.0,
            align_to_normal: false,
            max_slope_deg: 35.0,
            height_range: [OBJECT_HEIGHT_MIN, OBJECT_HEIGHT_MAX],
            enabled: true,
        }
    }
}

impl ObjectClass {
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }
}

/// Multi-class prop placement over the terrain surface.
///
/// Height passthrough: the layer publishes a scatter density channel and an
/// instance list, and never moves terrain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScatterObjectsParams {
    /// Master seed. Every placement and per-instance attribute derives from it,
    /// so identical params over identical terrain reproduce exactly.
    pub seed: u64,
    pub classes: Vec<ObjectClass>,
    /// Where props may go (WC-style coverage stack, same as vegetation).
    #[serde(default)]
    pub coverage: crate::mask::Distribution,
    /// Where props must **not** go; subtracted from coverage.
    #[serde(default)]
    pub exclusion: crate::mask::Distribution,
}

impl Default for ScatterObjectsParams {
    fn default() -> Self {
        Self {
            seed: 1337,
            classes: vec![ObjectClass::named("Rocks")],
            coverage: crate::mask::Distribution::new(),
            exclusion: crate::mask::Distribution::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_class_without_enabled_defaults_to_enabled() {
        let mut json = serde_json::to_value(ObjectClass::default()).unwrap();
        json.as_object_mut().unwrap().remove("enabled");
        let class: ObjectClass = serde_json::from_value(json).unwrap();
        assert!(class.enabled);
    }

    #[test]
    fn params_round_trip() {
        let p = ScatterObjectsParams::default();
        let json = serde_json::to_string(&p).unwrap();
        let back: ScatterObjectsParams = serde_json::from_str(&json).unwrap();
        assert_eq!(back.seed, p.seed);
        assert_eq!(back.classes.len(), p.classes.len());
    }
}
