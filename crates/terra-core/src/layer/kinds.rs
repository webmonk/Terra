//! All layer parameter kinds. Extended across phases; serde-stable via enum.

use super::BlendMode;
use crate::mask::MaskSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LayerKind {
    // Artist foundation — painted height buffer (always bottom in default docs)
    SculptBase(SculptParams),
    // Phase 1
    Flat(FlatParams),
    Ramp(RampParams),
    NoiseValue(NoiseParams),
    // Phase 2
    NoisePerlin(NoiseParams),
    NoiseOpenSimplex(NoiseParams),
    NoiseWorley(WorleyParams),
    Fbm(FbmParams),
    Ridged(FbmParams),
    DomainWarp(DomainWarpParams),
    Terrace(TerraceParams),
    Plateau(PlateauParams),
    Mountains(MountainParams),
    Dunes(DuneParams),
    Canyons(CanyonParams),
    VoronoiRegions(VoronoiParams),
    ImportHeightmap(ImportHeightmapParams),
    // Phase 5–7 simulations / filters
    ThermalErosion(ThermalErosionParams),
    HydraulicErosion(HydraulicErosionParams),
    RiverCarve(RiverCarveParams),
    Blur(BlurParams),
    Coastal(CoastalParams),
    // Phase 8
    Materials(MaterialsParams),
    Biomes(BiomesParams),
    Vegetation(VegetationParams),
}

impl LayerKind {
    pub fn is_sculpt_base(&self) -> bool {
        matches!(self, LayerKind::SculptBase(_))
    }

    /// World Creator–style default blend for new layers of this kind.
    ///
    /// Generators contribute absolute heights and should **Add**.
    /// Bases/imports and filters/sims that rewrite the stack use **Normal**.
    pub fn default_blend(&self) -> BlendMode {
        match self {
            LayerKind::SculptBase(_)
            | LayerKind::Flat(_)
            | LayerKind::Ramp(_)
            | LayerKind::ImportHeightmap(_)
            | LayerKind::Terrace(_)
            | LayerKind::Plateau(_)
            | LayerKind::ThermalErosion(_)
            | LayerKind::HydraulicErosion(_)
            | LayerKind::RiverCarve(_)
            | LayerKind::Blur(_)
            | LayerKind::Coastal(_)
            | LayerKind::Materials(_)
            | LayerKind::Biomes(_)
            | LayerKind::Vegetation(_) => BlendMode::Normal,

            LayerKind::NoiseValue(_)
            | LayerKind::NoisePerlin(_)
            | LayerKind::NoiseOpenSimplex(_)
            | LayerKind::NoiseWorley(_)
            | LayerKind::Fbm(_)
            | LayerKind::Ridged(_)
            | LayerKind::DomainWarp(_)
            | LayerKind::Mountains(_)
            | LayerKind::Dunes(_)
            | LayerKind::Canyons(_)
            | LayerKind::VoronoiRegions(_) => BlendMode::Add,
        }
    }
}

/// Paintable foundation heights in meters (normalized UV grid).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SculptParams {
    /// Resolution of the paint buffer (square).
    pub resolution: u32,
    /// Heights in world meters, row-major, length = resolution².
    pub samples: Vec<f32>,
    /// Fill / reset height when buffer is created or reset.
    pub fill_height: f32,
}

impl Default for SculptParams {
    fn default() -> Self {
        Self::filled(512, 20.0)
    }
}

impl SculptParams {
    pub fn filled(resolution: u32, fill_height: f32) -> Self {
        let n = (resolution as usize).saturating_mul(resolution as usize);
        Self {
            resolution,
            samples: vec![fill_height; n],
            fill_height,
        }
    }

    pub fn ensure_buffer(&mut self) {
        let n = (self.resolution as usize).saturating_mul(self.resolution as usize);
        if self.samples.len() != n {
            self.samples = vec![self.fill_height; n];
        }
    }

    pub fn reset(&mut self) {
        self.ensure_buffer();
        for s in &mut self.samples {
            *s = self.fill_height;
        }
    }

    /// Soft circular stamp. `mode`: 0 = raise, 1 = lower, 2 = smooth.
    pub fn stamp_circle(&mut self, u: f32, v: f32, radius_uv: f32, strength: f32, mode: u8) {
        self.ensure_buffer();
        let res = self.resolution;
        if res == 0 {
            return;
        }
        let radius = radius_uv.max(1e-6);
        let min_i = ((u - radius) * res as f32).floor().max(0.0) as u32;
        let max_i = ((u + radius) * res as f32).ceil().min(res as f32 - 1.0) as u32;
        let min_j = ((v - radius) * res as f32).floor().max(0.0) as u32;
        let max_j = ((v + radius) * res as f32).ceil().min(res as f32 - 1.0) as u32;

        if mode == 2 {
            // Smooth: blend toward local neighborhood average.
            let mut updates: Vec<(usize, f32)> = Vec::new();
            for j in min_j..=max_j {
                for i in min_i..=max_i {
                    let x = (i as f32 + 0.5) / res as f32;
                    let y = (j as f32 + 0.5) / res as f32;
                    let d = ((x - u).powi(2) + (y - v).powi(2)).sqrt() / radius;
                    if d > 1.0 {
                        continue;
                    }
                    let falloff = (1.0 - d * d) * strength.clamp(0.0, 1.0);
                    let idx = (j * res + i) as usize;
                    let mut sum = 0.0;
                    let mut count = 0.0;
                    for dj in -1i32..=1 {
                        for di in -1i32..=1 {
                            let ii = i as i32 + di;
                            let jj = j as i32 + dj;
                            if ii < 0 || jj < 0 || ii >= res as i32 || jj >= res as i32 {
                                continue;
                            }
                            sum += self.samples[(jj as u32 * res + ii as u32) as usize];
                            count += 1.0;
                        }
                    }
                    let avg = if count > 0.0 {
                        sum / count
                    } else {
                        self.samples[idx]
                    };
                    let cur = self.samples[idx];
                    updates.push((idx, cur + (avg - cur) * falloff));
                }
            }
            for (idx, val) in updates {
                self.samples[idx] = val;
            }
            return;
        }

        let delta_sign = if mode == 1 { -1.0 } else { 1.0 };
        // strength is meters of peak displacement per stamp
        let peak = strength.max(0.0) * delta_sign;
        for j in min_j..=max_j {
            for i in min_i..=max_i {
                let x = (i as f32 + 0.5) / res as f32;
                let y = (j as f32 + 0.5) / res as f32;
                let d = ((x - u).powi(2) + (y - v).powi(2)).sqrt() / radius;
                if d <= 1.0 {
                    let amount = (1.0 - d * d) * peak;
                    let sample = &mut self.samples[(j * res + i) as usize];
                    *sample += amount;
                }
            }
        }
    }

    pub fn sample_bilinear(&self, u: f32, v: f32) -> f32 {
        let res = self.resolution.max(1);
        let n = (res as usize).saturating_mul(res as usize);
        if self.samples.len() != n {
            return self.fill_height;
        }
        let uf = u.clamp(0.0, 1.0) * (res - 1) as f32;
        let vf = v.clamp(0.0, 1.0) * (res - 1) as f32;
        let i0 = uf.floor() as u32;
        let j0 = vf.floor() as u32;
        let i1 = (i0 + 1).min(res - 1);
        let j1 = (j0 + 1).min(res - 1);
        let tx = uf - i0 as f32;
        let ty = vf - j0 as f32;
        let a = self.samples[(j0 * res + i0) as usize];
        let b = self.samples[(j0 * res + i1) as usize];
        let c = self.samples[(j1 * res + i0) as usize];
        let d = self.samples[(j1 * res + i1) as usize];
        let top = a + (b - a) * tx;
        let bot = c + (d - c) * tx;
        top + (bot - top) * ty
    }

    /// Min/max of the paint buffer (for GPU height-range tracking).
    pub fn sample_range(&self) -> (f32, f32) {
        let res = self.resolution.max(1);
        let n = (res as usize).saturating_mul(res as usize);
        if self.samples.len() != n {
            return (self.fill_height, self.fill_height);
        }
        let mut lo = f32::INFINITY;
        let mut hi = f32::NEG_INFINITY;
        for &s in &self.samples {
            lo = lo.min(s);
            hi = hi.max(s);
        }
        if !lo.is_finite() {
            (self.fill_height, self.fill_height)
        } else {
            (lo, hi)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlatParams {
    pub height: f32,
}

impl Default for FlatParams {
    fn default() -> Self {
        Self { height: 0.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RampParams {
    pub height_min: f32,
    pub height_max: f32,
    /// Angle in radians; 0 = +X.
    pub direction: f32,
}

impl Default for RampParams {
    fn default() -> Self {
        Self {
            height_min: 0.0,
            height_max: 100.0,
            direction: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoiseParams {
    pub seed: u64,
    pub frequency: f32,
    pub amplitude: f32,
    pub octaves: u32,
    pub lacunarity: f32,
    pub persistence: f32,
    pub offset_x: f32,
    pub offset_z: f32,
    pub remap_min: f32,
    pub remap_max: f32,
}

impl Default for NoiseParams {
    fn default() -> Self {
        Self {
            seed: 1,
            frequency: 0.002,
            amplitude: 120.0,
            octaves: 1,
            lacunarity: 2.0,
            persistence: 0.5,
            offset_x: 0.0,
            offset_z: 0.0,
            remap_min: -1.0,
            remap_max: 1.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorleyParams {
    pub base: NoiseParams,
    pub distance_metric: WorleyMetric,
    pub feature: WorleyFeature,
}

impl Default for WorleyParams {
    fn default() -> Self {
        Self {
            base: NoiseParams {
                octaves: 1,
                frequency: 0.004,
                ..NoiseParams::default()
            },
            distance_metric: WorleyMetric::Euclidean,
            feature: WorleyFeature::F1,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum WorleyMetric {
    #[default]
    Euclidean,
    Manhattan,
    Chebyshev,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum WorleyFeature {
    #[default]
    F1,
    F2,
    F2MinusF1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FbmParams {
    pub base: NoiseParams,
    pub noise: FractalNoiseType,
}

impl Default for FbmParams {
    fn default() -> Self {
        Self {
            base: NoiseParams {
                octaves: 6,
                ..NoiseParams::default()
            },
            noise: FractalNoiseType::Perlin,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum FractalNoiseType {
    Value,
    #[default]
    Perlin,
    OpenSimplex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainWarpParams {
    pub base: NoiseParams,
    pub warp_strength: f32,
    pub warp_frequency: f32,
}

impl Default for DomainWarpParams {
    fn default() -> Self {
        Self {
            base: NoiseParams {
                octaves: 5,
                ..NoiseParams::default()
            },
            warp_strength: 80.0,
            warp_frequency: 0.0015,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerraceParams {
    pub levels: u32,
    pub sharpness: f32,
}

impl Default for TerraceParams {
    fn default() -> Self {
        Self {
            levels: 8,
            sharpness: 0.85,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlateauParams {
    pub low: f32,
    pub high: f32,
    pub soft: f32,
}

impl Default for PlateauParams {
    fn default() -> Self {
        Self {
            low: 40.0,
            high: 120.0,
            soft: 8.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountainParams {
    pub base: NoiseParams,
    pub ridge_sharpness: f32,
    pub range_angle: f32,
    pub range_width: f32,
}

impl Default for MountainParams {
    fn default() -> Self {
        Self {
            base: NoiseParams {
                octaves: 5,
                frequency: 0.0015,
                amplitude: 400.0,
                ..NoiseParams::default()
            },
            ridge_sharpness: 1.8,
            range_angle: 0.4,
            range_width: 0.35,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuneParams {
    pub base: NoiseParams,
    pub wave_frequency: f32,
    pub asymmetry: f32,
}

impl Default for DuneParams {
    fn default() -> Self {
        Self {
            base: NoiseParams {
                octaves: 3,
                frequency: 0.003,
                amplitude: 40.0,
                ..NoiseParams::default()
            },
            wave_frequency: 0.01,
            asymmetry: 0.65,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanyonParams {
    pub depth: f32,
    pub width: f32,
    pub meander: f32,
    pub seed: u64,
}

impl Default for CanyonParams {
    fn default() -> Self {
        Self {
            depth: 180.0,
            width: 120.0,
            meander: 0.4,
            seed: 7,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoronoiParams {
    pub base: NoiseParams,
    pub cell_jitter: f32,
    pub height_per_cell: f32,
}

impl Default for VoronoiParams {
    fn default() -> Self {
        Self {
            base: NoiseParams {
                frequency: 0.003,
                amplitude: 80.0,
                ..NoiseParams::default()
            },
            cell_jitter: 0.75,
            height_per_cell: 60.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportHeightmapParams {
    pub path: String,
    pub height_scale: f32,
    pub height_offset: f32,
}

impl Default for ImportHeightmapParams {
    fn default() -> Self {
        Self {
            path: String::new(),
            height_scale: 500.0,
            height_offset: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalErosionParams {
    pub talus_angle_deg: f32,
    pub iterations: u32,
    pub strength: f32,
}

impl Default for ThermalErosionParams {
    fn default() -> Self {
        Self {
            talus_angle_deg: 35.0,
            iterations: 40,
            strength: 0.5,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydraulicErosionParams {
    pub iterations: u32,
    pub rainfall: f32,
    pub evaporation: f32,
    pub capacity: f32,
    pub erosion: f32,
    pub deposition: f32,
    pub timestep: f32,
}

impl Default for HydraulicErosionParams {
    fn default() -> Self {
        Self {
            iterations: 60,
            rainfall: 0.02,
            evaporation: 0.01,
            capacity: 0.1,
            erosion: 0.3,
            deposition: 0.3,
            timestep: 0.2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiverCarveParams {
    pub accumulation_threshold: f32,
    pub depth: f32,
    pub width: f32,
    pub bank_smooth: f32,
    pub use_dinfinity: bool,
}

impl Default for RiverCarveParams {
    fn default() -> Self {
        Self {
            accumulation_threshold: 50.0,
            depth: 25.0,
            width: 4.0,
            bank_smooth: 1.5,
            use_dinfinity: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlurParams {
    pub radius: u32,
    pub iterations: u32,
}

impl Default for BlurParams {
    fn default() -> Self {
        Self {
            radius: 2,
            iterations: 1,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoastalParams {
    pub sea_level: f32,
    pub beach_width: f32,
    pub flatten_below: bool,
    /// Maximum depth of the softened underwater shelf; zero preserves flat sea.
    #[serde(default)]
    pub shelf_depth: f32,
}

impl Default for CoastalParams {
    fn default() -> Self {
        Self {
            sea_level: 0.0,
            beach_width: 20.0,
            flatten_below: true,
            shelf_depth: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialsParams {
    pub rules: Vec<MaterialRule>,
}

impl Default for MaterialsParams {
    fn default() -> Self {
        Self {
            rules: vec![
                MaterialRule {
                    name: "Rock".into(),
                    id: 1,
                    min_slope_deg: 35.0,
                    max_slope_deg: 90.0,
                    min_height: f32::NEG_INFINITY,
                    max_height: f32::INFINITY,
                    mask: MaskSource::None,
                },
                MaterialRule {
                    name: "Grass".into(),
                    id: 2,
                    min_slope_deg: 0.0,
                    max_slope_deg: 35.0,
                    min_height: 5.0,
                    max_height: f32::INFINITY,
                    mask: MaskSource::None,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterialRule {
    pub name: String,
    pub id: u32,
    pub min_slope_deg: f32,
    pub max_slope_deg: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub mask: MaskSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiomesParams {
    pub bands: Vec<BiomeBand>,
}

impl Default for BiomesParams {
    fn default() -> Self {
        Self {
            bands: vec![
                BiomeBand {
                    name: "Alpine".into(),
                    id: 1,
                    min_height: 200.0,
                    max_height: f32::INFINITY,
                    min_wetness: 0.0,
                    max_wetness: 1.0,
                },
                BiomeBand {
                    name: "Temperate".into(),
                    id: 2,
                    min_height: 20.0,
                    max_height: 200.0,
                    min_wetness: 0.0,
                    max_wetness: 1.0,
                },
                BiomeBand {
                    name: "Coast".into(),
                    id: 3,
                    min_height: f32::NEG_INFINITY,
                    max_height: 20.0,
                    min_wetness: 0.0,
                    max_wetness: 1.0,
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiomeBand {
    pub name: String,
    pub id: u32,
    pub min_height: f32,
    pub max_height: f32,
    pub min_wetness: f32,
    pub max_wetness: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VegetationParams {
    pub seed: u64,
    pub density: f32,
    pub min_distance: f32,
    pub min_slope_deg: f32,
    pub max_slope_deg: f32,
    pub biome_id: Option<u32>,
}

impl Default for VegetationParams {
    fn default() -> Self {
        Self {
            seed: 42,
            density: 0.15,
            min_distance: 4.0,
            min_slope_deg: 0.0,
            max_slope_deg: 30.0,
            biome_id: Some(2),
        }
    }
}
