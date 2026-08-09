//! Landscape blueprint — world-scale artist boundary conditions.

use serde::{Deserialize, Serialize};

/// Stable archetype identity for New World wizard templates.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct ArchetypeId(pub String);

impl ArchetypeId {
    pub fn blank() -> Self {
        Self("blank".into())
    }

    pub fn tropical_island() -> Self {
        Self("tropical_island".into())
    }

    pub fn alpine() -> Self {
        Self("alpine".into())
    }

    pub fn desert() -> Self {
        Self("desert".into())
    }

    pub fn river_valley() -> Self {
        Self("river_valley".into())
    }

    pub fn badlands() -> Self {
        Self("badlands".into())
    }

    pub fn old_mountains() -> Self {
        Self("old_mountains".into())
    }

    pub fn young_mountains() -> Self {
        Self("young_mountains".into())
    }

    pub fn dune_field() -> Self {
        Self("dune_field".into())
    }

    pub fn coastal() -> Self {
        Self("coastal".into())
    }
}

/// Global landscape blueprint (geography + climate intent, not ecology).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LandscapeBlueprint {
    pub archetype: ArchetypeId,
    /// Sea level in metres (absolute height).
    pub sea_level: f32,
    /// World extent in metres (X).
    pub world_size_m: f32,
    /// Metres per height sample at export density.
    pub metres_per_sample: f32,
    /// Artist semantic: ridge sharpness [0, 1].
    pub ridge_sharpness: f32,
    /// Artist semantic: drainage density [0, 1].
    pub drainage_density: f32,
    /// Prevailing wind direction in degrees (0 = +X).
    pub wind_direction_deg: f32,
    /// Rainfall multiplier (drives hydro / evolution incision).
    pub rainfall: f32,
    /// Coastal energy [0, 1].
    pub coastal_energy: f32,
    /// Geological age [0, 1] → effective landscape-evolution time.
    ///
    /// Serde alias keeps older documents that stored `surface_maturity`.
    #[serde(alias = "surface_maturity", default = "default_geological_age")]
    pub geological_age: f32,
}

fn default_geological_age() -> f32 {
    0.45
}

impl Default for LandscapeBlueprint {
    fn default() -> Self {
        Self {
            archetype: ArchetypeId::blank(),
            sea_level: 0.0,
            world_size_m: 8192.0,
            metres_per_sample: 4.0,
            ridge_sharpness: 0.5,
            drainage_density: 0.5,
            wind_direction_deg: 225.0,
            rainfall: 1.05,
            coastal_energy: 0.4,
            geological_age: default_geological_age(),
        }
    }
}

impl LandscapeBlueprint {
    pub fn tropical_island(world_size_m: f32) -> Self {
        Self {
            archetype: ArchetypeId::tropical_island(),
            sea_level: 0.0,
            world_size_m,
            metres_per_sample: (world_size_m / 2048.0).max(1.0),
            ridge_sharpness: 0.65,
            drainage_density: 0.7,
            wind_direction_deg: 210.0,
            rainfall: 2.08,
            coastal_energy: 0.7,
            geological_age: 0.55,
        }
    }

    pub fn alpine(world_size_m: f32) -> Self {
        Self {
            archetype: ArchetypeId::alpine(),
            sea_level: 0.0,
            world_size_m,
            metres_per_sample: (world_size_m / 2048.0).max(1.0),
            ridge_sharpness: 0.85,
            drainage_density: 0.65,
            wind_direction_deg: 250.0,
            rainfall: 1.23,
            coastal_energy: 0.15,
            geological_age: 0.35,
        }
    }

    pub fn desert(world_size_m: f32) -> Self {
        Self {
            archetype: ArchetypeId::desert(),
            sea_level: 0.0,
            world_size_m,
            metres_per_sample: (world_size_m / 2048.0).max(1.0),
            ridge_sharpness: 0.55,
            drainage_density: 0.25,
            wind_direction_deg: 180.0,
            rainfall: 0.20,
            coastal_energy: 0.1,
            geological_age: 0.7,
        }
    }

    pub fn river_valley(world_size_m: f32) -> Self {
        Self {
            archetype: ArchetypeId::river_valley(),
            sea_level: 20.0,
            world_size_m,
            metres_per_sample: (world_size_m / 2048.0).max(1.0),
            ridge_sharpness: 0.45,
            drainage_density: 0.85,
            wind_direction_deg: 200.0,
            rainfall: 1.60,
            coastal_energy: 0.2,
            geological_age: 0.5,
        }
    }

    pub fn badlands(world_size_m: f32) -> Self {
        Self {
            archetype: ArchetypeId::badlands(),
            sea_level: 0.0,
            world_size_m,
            metres_per_sample: (world_size_m / 2048.0).max(1.0),
            ridge_sharpness: 0.7,
            drainage_density: 0.9,
            wind_direction_deg: 170.0,
            rainfall: 0.45,
            coastal_energy: 0.05,
            geological_age: 0.55,
        }
    }

    pub fn old_mountains(world_size_m: f32) -> Self {
        Self {
            archetype: ArchetypeId::old_mountains(),
            sea_level: 0.0,
            world_size_m,
            metres_per_sample: (world_size_m / 2048.0).max(1.0),
            ridge_sharpness: 0.35,
            drainage_density: 0.7,
            wind_direction_deg: 230.0,
            rainfall: 1.05,
            coastal_energy: 0.1,
            geological_age: 0.88,
        }
    }

    pub fn young_mountains(world_size_m: f32) -> Self {
        Self {
            archetype: ArchetypeId::young_mountains(),
            sea_level: 0.0,
            world_size_m,
            metres_per_sample: (world_size_m / 2048.0).max(1.0),
            ridge_sharpness: 0.9,
            drainage_density: 0.45,
            wind_direction_deg: 255.0,
            rainfall: 1.35,
            coastal_energy: 0.1,
            geological_age: 0.18,
        }
    }

    pub fn dune_field(world_size_m: f32) -> Self {
        Self {
            archetype: ArchetypeId::dune_field(),
            sea_level: 0.0,
            world_size_m,
            metres_per_sample: (world_size_m / 2048.0).max(1.0),
            ridge_sharpness: 0.3,
            drainage_density: 0.1,
            wind_direction_deg: 180.0,
            rainfall: 0.08,
            coastal_energy: 0.05,
            geological_age: 0.4,
        }
    }

    pub fn coastal(world_size_m: f32) -> Self {
        Self {
            archetype: ArchetypeId::coastal(),
            sea_level: 0.0,
            world_size_m,
            metres_per_sample: (world_size_m / 2048.0).max(1.0),
            ridge_sharpness: 0.45,
            drainage_density: 0.75,
            wind_direction_deg: 210.0,
            rainfall: 1.4,
            coastal_energy: 0.85,
            geological_age: 0.5,
        }
    }

    /// Map geological age to landscape-evolution iteration budget (Accurate mode).
    pub fn evolution_iterations(&self) -> u32 {
        (8.0 + self.geological_age.clamp(0.0, 1.0) * 40.0).round() as u32
    }

    /// Effective evolution time fraction used by Fast analytical mode.
    pub fn geological_age_norm(&self) -> f32 {
        self.geological_age.clamp(0.0, 1.0)
    }

    /// Legacy alias for [`Self::geological_age`].
    pub fn surface_maturity(&self) -> f32 {
        self.geological_age
    }

    /// Rainfall scale used by hydro / ecology.
    pub fn rainfall_scale(&self) -> f32 {
        self.rainfall.max(0.0)
    }

    /// Ridge / uplift width scale in metres from sharpness.
    pub fn ridge_width_m(&self) -> f32 {
        let t = 1.0 - self.ridge_sharpness.clamp(0.0, 1.0);
        40.0 + t * 220.0
    }
}

/// World Creator–style Resolution → interactive preview samples.
///
/// WC's Resolution is world extent in metres; at default 1 m precision, sample
/// count matches that extent. We cap samples for interactive preview performance.
pub fn preview_resolution_for_world_size(world_size_m: f32) -> u32 {
    world_size_m.round().clamp(128.0, 2048.0) as u32
}

/// Explicit evaluation stages for dependency ordering / cycle prevention.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[repr(u8)]
pub enum EvalStage {
    Blueprint = 0,
    PreBiomeFields = 1,
    Climate = 2,
    BiomePlacement = 3,
    FieldBlend = 4,
    SharedHydro = 5,
    Materials = 6,
    Vegetation = 7,
    FineDetail = 8,
    Export = 9,
}

impl EvalStage {
    pub const ALL: [EvalStage; 10] = [
        EvalStage::Blueprint,
        EvalStage::PreBiomeFields,
        EvalStage::Climate,
        EvalStage::BiomePlacement,
        EvalStage::FieldBlend,
        EvalStage::SharedHydro,
        EvalStage::Materials,
        EvalStage::Vegetation,
        EvalStage::FineDetail,
        EvalStage::Export,
    ];

    pub fn label(self) -> &'static str {
        match self {
            EvalStage::Blueprint => "Blueprint",
            EvalStage::PreBiomeFields => "Pre-biome fields",
            EvalStage::Climate => "Climate",
            EvalStage::BiomePlacement => "Biome placement",
            EvalStage::FieldBlend => "Field blend",
            EvalStage::SharedHydro => "Shared hydrology",
            EvalStage::Materials => "Materials",
            EvalStage::Vegetation => "Vegetation",
            EvalStage::FineDetail => "Fine detail",
            EvalStage::Export => "Export",
        }
    }

    /// Ordinal for stage-aware invalidation (lower = earlier in pipeline).
    pub fn order(self) -> u8 {
        match self {
            EvalStage::Blueprint => 0,
            EvalStage::PreBiomeFields => 1,
            EvalStage::Climate => 2,
            EvalStage::BiomePlacement => 3,
            EvalStage::FieldBlend => 4,
            EvalStage::SharedHydro => 5,
            EvalStage::Materials => 6,
            EvalStage::Vegetation => 7,
            EvalStage::FineDetail => 8,
            EvalStage::Export => 9,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tropical_blueprint_maps_artist_controls() {
        let bp = LandscapeBlueprint::tropical_island(10_000.0);
        assert!(bp.evolution_iterations() >= 8);
        assert!(bp.rainfall_scale() > 1.0);
        assert!(bp.ridge_width_m() < 200.0);
    }

    #[test]
    fn preview_resolution_follows_world_size_like_wc() {
        assert_eq!(preview_resolution_for_world_size(1024.0), 1024);
        assert_eq!(preview_resolution_for_world_size(4096.0), 2048);
        assert_eq!(preview_resolution_for_world_size(64.0), 128);
    }
}
