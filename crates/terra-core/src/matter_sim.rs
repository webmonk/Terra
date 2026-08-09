//! Artist-facing configuration for physical matter simulations.
//!
//! Matter types — Water/Rivers, Snow, Sand, Debris — sit on top of existing
//! [`crate::simulation_scenario::SimulationScenario`] and Simulation Layers.
//! Solvers are not rewritten; this module declares sources, inputs, outputs,
//! artist controls, Advanced solver knobs, diagnostics, and selective apply.

use crate::domain::SoftDiagnostic;
use crate::fields::FieldId;
use crate::mask::MaskId;
use crate::simulation_scenario::{
    MatterSource, MatterSourceKind, OutputApplicationSettings, OutputInfluence, ScenarioPass,
    ScenarioPassKind, ScenarioQuality, ScenarioScope, SimulationDomain, SimulationScenario,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Physical matter family for artist configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatterType {
    WaterRivers,
    Snow,
    Sand,
    Debris,
}

impl MatterType {
    pub fn all() -> &'static [MatterType] {
        &[Self::WaterRivers, Self::Snow, Self::Sand, Self::Debris]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::WaterRivers => "Water / Rivers",
            Self::Snow => "Snow",
            Self::Sand => "Sand",
            Self::Debris => "Debris",
        }
    }

    pub fn default_scenario_name(self) -> &'static str {
        match self {
            Self::WaterRivers => "Water & Rivers",
            Self::Snow => "Snow",
            Self::Sand => "Sand Transport",
            Self::Debris => "Debris Flows",
        }
    }
}

/// Artist-first source for a matter sim (paint / climate / import).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatterArtistSource {
    Rainfall { strength: f32 },
    PaintedSprings { mask: Option<MaskId> },
    SourceSplines,
    Lakes,
    Snowmelt { strength: f32 },
    ImportedFlow { mask: Option<MaskId> },
    Snowfall { strength: f32 },
    PaintedSnow { mask: Option<MaskId> },
    ClimateRule,
    PaintedSand { mask: Option<MaskId> },
    CoastalSediment,
    DesertBiome,
    ImportedMap { mask: Option<MaskId> },
    SteepSlopes { min_deg: f32 },
    ErodedCliffs,
    PaintedUnstable { mask: Option<MaskId> },
    HydraulicUndercutting,
}

impl MatterArtistSource {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Rainfall { .. } => "Rainfall",
            Self::PaintedSprings { .. } => "Painted Springs",
            Self::SourceSplines => "Source Splines",
            Self::Lakes => "Lakes",
            Self::Snowmelt { .. } => "Snowmelt",
            Self::ImportedFlow { .. } => "Imported Flow",
            Self::Snowfall { .. } => "Snowfall",
            Self::PaintedSnow { .. } => "Painted Snow Source",
            Self::ClimateRule => "Climate Rule",
            Self::PaintedSand { .. } => "Painted Sand Source",
            Self::CoastalSediment => "Coastal Sediment",
            Self::DesertBiome => "Desert Biome",
            Self::ImportedMap { .. } => "Imported Map",
            Self::SteepSlopes { .. } => "Steep Slopes",
            Self::ErodedCliffs => "Eroded Cliffs",
            Self::PaintedUnstable { .. } => "Painted Unstable Areas",
            Self::HydraulicUndercutting => "Hydraulic Undercutting",
        }
    }

    pub fn is_paintable(&self) -> bool {
        matches!(
            self,
            Self::PaintedSprings { .. }
                | Self::PaintedSnow { .. }
                | Self::PaintedSand { .. }
                | Self::PaintedUnstable { .. }
        )
    }

    pub fn to_scenario_source(&self) -> MatterSource {
        let kind = match self {
            Self::Rainfall { .. } => MatterSourceKind::Rainfall,
            Self::PaintedSprings { .. } => MatterSourceKind::PaintedSprings,
            Self::Snowmelt { .. } | Self::Snowfall { .. } => MatterSourceKind::Snowmelt,
            Self::PaintedSand { .. }
            | Self::CoastalSediment
            | Self::DesertBiome
            | Self::ImportedMap { .. } => MatterSourceKind::WindSediment,
            Self::ImportedFlow { .. } | Self::SourceSplines | Self::Lakes => {
                MatterSourceKind::RiverInflow
            }
            other => MatterSourceKind::Custom {
                label: other.label().into(),
            },
        };
        let mut src = MatterSource::new(kind);
        src.strength = match self {
            Self::Rainfall { strength }
            | Self::Snowmelt { strength }
            | Self::Snowfall { strength } => *strength,
            _ => 1.0,
        };
        src
    }
}

/// Artist-facing controls (shown first). Solver knobs live in [`MatterAdvancedParams`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatterArtistControls {
    /// Overall influence \[0,1\].
    #[serde(default = "one")]
    pub strength: f32,
    /// Coverage / spawn density \[0,1\].
    #[serde(default = "half")]
    pub coverage: f32,
    /// Sea level (m) for water / coastal.
    #[serde(default)]
    pub sea_level_m: f32,
    /// Soft angle hint for sand / debris (degrees).
    #[serde(default = "default_angle")]
    pub repose_angle_deg: f32,
    /// Snowline / melt temperature bias.
    #[serde(default)]
    pub temperature_bias: f32,
    /// Wind influence \[0,1\].
    #[serde(default = "half")]
    pub wind: f32,
    /// Preview mode preference.
    #[serde(default)]
    pub live_preview: bool,
    /// Show domain overlay in viewport.
    #[serde(default = "default_true")]
    pub show_domain_overlay: bool,
    /// Enable source-painting tool arm.
    #[serde(default)]
    pub paint_sources: bool,
}

fn one() -> f32 {
    1.0
}
fn half() -> f32 {
    0.5
}
fn default_angle() -> f32 {
    32.0
}
fn default_true() -> bool {
    true
}

impl Default for MatterArtistControls {
    fn default() -> Self {
        Self {
            strength: 1.0,
            coverage: 0.5,
            sea_level_m: 0.0,
            repose_angle_deg: 32.0,
            temperature_bias: 0.0,
            wind: 0.5,
            live_preview: false,
            show_domain_overlay: true,
            paint_sources: false,
        }
    }
}

/// Numerical solver parameters (Advanced section).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatterAdvancedParams {
    #[serde(default = "default_iters")]
    pub iterations: u32,
    #[serde(default)]
    pub timestep: Option<f32>,
    #[serde(default = "half")]
    pub viscosity_or_friction: f32,
    #[serde(default)]
    pub spawn_amount: f32,
    #[serde(default)]
    pub erosion_rate: f32,
    #[serde(default)]
    pub deposition_rate: f32,
    #[serde(default)]
    pub capacity: f32,
    /// When true, allow height delta application (still non-destructive until Apply).
    #[serde(default)]
    pub allow_height_delta: bool,
}

fn default_iters() -> u32 {
    32
}

impl Default for MatterAdvancedParams {
    fn default() -> Self {
        Self {
            iterations: 32,
            timestep: None,
            viscosity_or_friction: 0.35,
            spawn_amount: 1.5,
            erosion_rate: 0.3,
            deposition_rate: 0.3,
            capacity: 1.0,
            allow_height_delta: false,
        }
    }
}

/// Full matter simulation configuration attached to a scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatterSimConfig {
    pub id: Uuid,
    pub matter: MatterType,
    #[serde(default)]
    pub artist: MatterArtistControls,
    #[serde(default)]
    pub advanced: MatterAdvancedParams,
    #[serde(default)]
    pub sources: Vec<MatterArtistSource>,
    /// Declared input fields (Height, Hardness, …).
    #[serde(default)]
    pub inputs: Vec<FieldId>,
    /// Declared output maps reusable by World Rules / Materials / Scatter / sims.
    #[serde(default)]
    pub outputs: Vec<FieldId>,
    /// Domain restriction (may differ from scenario domain).
    #[serde(default)]
    pub domain: SimulationDomain,
    /// Output influence for selective apply.
    #[serde(default)]
    pub influence: OutputInfluence,
    /// Fields selected for Apply (subset of outputs).
    #[serde(default)]
    pub apply_selected: Vec<FieldId>,
    /// Cached vs live preview preference.
    #[serde(default)]
    pub prefer_cached_preview: bool,
    /// Before/after compare snapshot ids (string form for UI).
    #[serde(default)]
    pub compare_before: Option<Uuid>,
    #[serde(default)]
    pub compare_after: Option<Uuid>,
}

impl MatterSimConfig {
    pub fn new(matter: MatterType) -> Self {
        let mut cfg = Self {
            id: Uuid::new_v4(),
            matter,
            artist: MatterArtistControls::default(),
            advanced: MatterAdvancedParams::default(),
            sources: default_sources(matter),
            inputs: default_inputs(matter),
            outputs: default_outputs(matter),
            domain: SimulationDomain::InheritScope,
            influence: OutputInfluence::EntireDomain,
            apply_selected: Vec::new(),
            prefer_cached_preview: true,
            compare_before: None,
            compare_after: None,
        };
        // Default selective apply: non-height aux maps.
        cfg.apply_selected = cfg
            .outputs
            .iter()
            .filter(|f| !matches!(f, FieldId::Height | FieldId::Named(_)))
            .cloned()
            .take(4)
            .collect();
        if cfg.apply_selected.is_empty() {
            cfg.apply_selected = cfg.outputs.clone();
        }
        cfg
    }

    pub fn water_rivers() -> Self {
        Self::new(MatterType::WaterRivers)
    }
    pub fn snow() -> Self {
        Self::new(MatterType::Snow)
    }
    pub fn sand() -> Self {
        Self::new(MatterType::Sand)
    }
    pub fn debris() -> Self {
        Self::new(MatterType::Debris)
    }

    /// Soft diagnostics for unstable / excessive Advanced settings.
    pub fn diagnostics(&self) -> Vec<SoftDiagnostic> {
        diagnose_matter_sim(self)
    }

    /// Build / refresh a Simulation Scenario from this matter config.
    pub fn build_scenario(&self) -> SimulationScenario {
        let mut s = SimulationScenario::new(self.matter.default_scenario_name());
        s.scope = ScenarioScope::World;
        s.domain = self.domain.clone();
        s.output_influence = self.influence.clone();
        s.sources = self
            .sources
            .iter()
            .map(|src| src.to_scenario_source())
            .collect();
        s.passes = default_passes(self.matter);
        for i in 1..s.passes.len() {
            let prev = s.passes[i - 1].id;
            s.passes[i].depends_on.push(prev);
        }
        // Tag pass outputs with matter output fields where relevant.
        if let Some(last) = s.passes.last_mut() {
            for f in &self.outputs {
                if !last.outputs.contains(f) {
                    last.outputs.push(f.clone());
                }
            }
        }
        s.quality = if self.artist.live_preview {
            ScenarioQuality::Draft
        } else {
            ScenarioQuality::Medium
        };
        s.timeline.iterations = self.advanced.iterations;
        s.output_application = OutputApplicationSettings {
            selected_fields: self.apply_selected.clone(),
            apply_height: self.advanced.allow_height_delta,
            apply_aux: true,
            respect_influence: true,
        };
        s.cache_policy = if self.prefer_cached_preview {
            crate::layer::CachePolicy::Cached
        } else {
            crate::layer::CachePolicy::Live
        };
        s
    }

    /// Apply this config onto an existing scenario (preserves id / snapshots).
    pub fn apply_to_scenario(&self, scenario: &mut SimulationScenario) {
        let built = self.build_scenario();
        scenario.name = built.name;
        scenario.sources = built.sources;
        scenario.domain = built.domain;
        scenario.output_influence = built.output_influence;
        // Merge passes by kind — keep existing layer bindings.
        let mut bound: std::collections::HashMap<ScenarioPassKind, Option<crate::layer::LayerId>> =
            scenario
                .passes
                .iter()
                .map(|p| (p.kind, p.layer_id))
                .collect();
        scenario.passes = built.passes;
        for p in &mut scenario.passes {
            if let Some(lid) = bound.remove(&p.kind).flatten() {
                p.layer_id = Some(lid);
            }
        }
        scenario.quality = built.quality;
        scenario.timeline = built.timeline;
        scenario.output_application = built.output_application;
        scenario.cache_policy = built.cache_policy;
    }

    /// Fields this matter type publishes for downstream World Rules / Materials / Scatter.
    pub fn reusable_outputs(&self) -> &[FieldId] {
        &self.outputs
    }

    /// Restrict domain for cross-biome / painted movement tests.
    pub fn with_domain(mut self, domain: SimulationDomain) -> Self {
        self.domain = domain;
        self
    }

    pub fn set_compare(&mut self, before: Option<Uuid>, after: Option<Uuid>) {
        self.compare_before = before;
        self.compare_after = after;
    }
}

fn default_sources(matter: MatterType) -> Vec<MatterArtistSource> {
    match matter {
        MatterType::WaterRivers => vec![
            MatterArtistSource::Rainfall { strength: 1.0 },
            MatterArtistSource::PaintedSprings { mask: None },
            MatterArtistSource::SourceSplines,
            MatterArtistSource::Lakes,
            MatterArtistSource::Snowmelt { strength: 0.6 },
            MatterArtistSource::ImportedFlow { mask: None },
        ],
        MatterType::Snow => vec![
            MatterArtistSource::Snowfall { strength: 1.0 },
            MatterArtistSource::PaintedSnow { mask: None },
            MatterArtistSource::ClimateRule,
        ],
        MatterType::Sand => vec![
            MatterArtistSource::PaintedSand { mask: None },
            MatterArtistSource::CoastalSediment,
            MatterArtistSource::DesertBiome,
            MatterArtistSource::ImportedMap { mask: None },
        ],
        MatterType::Debris => vec![
            MatterArtistSource::SteepSlopes { min_deg: 45.0 },
            MatterArtistSource::ErodedCliffs,
            MatterArtistSource::PaintedUnstable { mask: None },
            MatterArtistSource::HydraulicUndercutting,
        ],
    }
}

fn default_inputs(matter: MatterType) -> Vec<FieldId> {
    match matter {
        MatterType::WaterRivers => vec![
            FieldId::Height,
            FieldId::Hardness,
            FieldId::Named("infiltration".into()),
            FieldId::Vegetation,
            FieldId::Named("sea_level".into()),
        ],
        MatterType::Snow => vec![
            FieldId::Height,
            FieldId::Temperature,
            FieldId::WindExposure,
            FieldId::Slope,
            FieldId::Named("sun_exposure".into()),
            FieldId::Named("obstacles".into()),
        ],
        MatterType::Sand => vec![
            FieldId::Height,
            FieldId::WindExposure,
            FieldId::SoilMoisture,
            FieldId::Named("obstacles".into()),
            FieldId::Vegetation,
        ],
        MatterType::Debris => vec![
            FieldId::Height,
            FieldId::Slope,
            FieldId::Named("friction".into()),
            FieldId::Materials,
            FieldId::Named("obstacles".into()),
        ],
    }
}

fn default_outputs(matter: MatterType) -> Vec<FieldId> {
    match matter {
        MatterType::WaterRivers => vec![
            FieldId::Height, // height delta when applied
            FieldId::FlowDirection,
            FieldId::FlowAccumulation,
            FieldId::RiverChannel,
            FieldId::WaterDepth,
            FieldId::Erosion,
            FieldId::Deposition,
            FieldId::Wetness,
            FieldId::Floodplain,
        ],
        MatterType::Snow => vec![
            FieldId::SnowDepth,
            FieldId::Snow, // coverage
            FieldId::Drift,
            FieldId::SlidePath,
            FieldId::Meltwater,
            FieldId::Wetness,
            FieldId::SnowMaterialMask,
            FieldId::Height, // optional height offset
        ],
        MatterType::Sand => vec![
            FieldId::Height,
            FieldId::SandDepth,
            FieldId::DuneCrest,
            FieldId::FlowDirection, // transport direction
            FieldId::WindDirection,
            FieldId::WindSpeed,
            FieldId::SandFlux,
            FieldId::Sheltering,
            FieldId::BedrockHeight,
            FieldId::Erosion,
            FieldId::Deposition,
            FieldId::SandMaterialMask,
        ],
        MatterType::Debris => vec![
            FieldId::Height,
            FieldId::DebrisDepth,
            FieldId::SlidePath,
            FieldId::Deposition,
            FieldId::Named("rock_distribution".into()),
            FieldId::Instability,
            FieldId::ScatterCandidates,
        ],
    }
}

fn default_passes(matter: MatterType) -> Vec<ScenarioPass> {
    match matter {
        MatterType::WaterRivers => vec![
            ScenarioPass::new(ScenarioPassKind::Flow),
            ScenarioPass::new(ScenarioPassKind::RiverExtraction),
            ScenarioPass::new(ScenarioPassKind::HydraulicErosion),
            ScenarioPass::new(ScenarioPassKind::Sediment),
        ],
        MatterType::Snow => vec![
            ScenarioPass::new(ScenarioPassKind::Flow), // melt routing
            ScenarioPass::new(ScenarioPassKind::ThermalErosion),
            ScenarioPass::new(ScenarioPassKind::Fluid),
        ],
        MatterType::Sand => vec![
            ScenarioPass::new(ScenarioPassKind::Sand),
            ScenarioPass::new(ScenarioPassKind::Coastal),
        ],
        MatterType::Debris => vec![
            ScenarioPass::new(ScenarioPassKind::LandscapeEvolution),
            ScenarioPass::new(ScenarioPassKind::HydraulicErosion),
            ScenarioPass::new(ScenarioPassKind::DebrisFlow),
            ScenarioPass::new(ScenarioPassKind::ThermalErosion),
        ],
    }
}

/// Non-blocking diagnostics for unstable or excessive settings.
pub fn diagnose_matter_sim(cfg: &MatterSimConfig) -> Vec<SoftDiagnostic> {
    let mut out = Vec::new();
    let name = cfg.matter.label();
    if cfg.sources.is_empty() {
        out.push(SoftDiagnostic::new(
            "matter_no_sources",
            format!("‘{name}’ has no sources — paint or enable rainfall / snowfall."),
        ));
    }
    if cfg.advanced.iterations > 256 {
        out.push(SoftDiagnostic::new(
            "matter_excessive_iterations",
            format!(
                "‘{name}’ iterations ({}) are very high — may be slow or unstable.",
                cfg.advanced.iterations
            ),
        ));
    }
    if cfg.artist.strength > 1.5 {
        out.push(SoftDiagnostic::new(
            "matter_excessive_strength",
            format!("‘{name}’ strength is unusually high — results may overshoot."),
        ));
    }
    if cfg.artist.repose_angle_deg < 5.0 || cfg.artist.repose_angle_deg > 70.0 {
        out.push(SoftDiagnostic::new(
            "matter_unstable_angle",
            format!(
                "‘{name}’ repose angle {:.0}° is extreme — check Advanced / artist angle.",
                cfg.artist.repose_angle_deg
            ),
        ));
    }
    if cfg.advanced.spawn_amount > 20.0 {
        out.push(SoftDiagnostic::new(
            "matter_excessive_spawn",
            format!("‘{name}’ spawn amount is excessive — reduce under Advanced."),
        ));
    }
    if cfg.advanced.allow_height_delta
        && cfg
            .apply_selected
            .iter()
            .any(|f| matches!(f, FieldId::Height))
    {
        out.push(SoftDiagnostic::new(
            "matter_height_apply_enabled",
            format!(
                "‘{name}’ will apply height deltas when Apply is used — non-destructive until then."
            ),
        ));
    }
    if cfg.artist.paint_sources && !cfg.sources.iter().any(|s| s.is_paintable()) {
        out.push(SoftDiagnostic::new(
            "matter_paint_no_target",
            format!("Source painting is on but ‘{name}’ has no paintable source."),
        ));
    }
    out
}

/// Attach matter config metadata onto a scenario via output application fields.
pub fn sync_scenario_outputs_from_matter(scenario: &mut SimulationScenario, cfg: &MatterSimConfig) {
    scenario.output_application.selected_fields = cfg.apply_selected.clone();
    scenario.output_application.apply_height = cfg.advanced.allow_height_delta;
    scenario.output_application.apply_aux = true;
}

/// Downstream consumers that can read matter output maps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatterOutputConsumer {
    WorldRules,
    Materials,
    BiomePlacement,
    Scatter,
    OtherSimulation,
}

impl MatterOutputConsumer {
    pub fn can_consume(self, field: &FieldId) -> bool {
        match self {
            Self::WorldRules => !matches!(field, FieldId::Height),
            Self::Materials => matches!(
                field,
                FieldId::Wetness
                    | FieldId::Snow
                    | FieldId::SnowMaterialMask
                    | FieldId::SandMaterialMask
                    | FieldId::Materials
                    | FieldId::Floodplain
                    | FieldId::WaterDepth
            ),
            Self::BiomePlacement => matches!(
                field,
                FieldId::Wetness
                    | FieldId::Snow
                    | FieldId::Temperature
                    | FieldId::SandDepth
                    | FieldId::WaterDepth
            ),
            Self::Scatter => matches!(
                field,
                FieldId::ScatterCandidates
                    | FieldId::Instability
                    | FieldId::SlidePath
                    | FieldId::SandDepth
                    | FieldId::DebrisDepth
            ),
            Self::OtherSimulation => true,
        }
    }
}

/// Fields from a matter config available to a given consumer.
pub fn outputs_for_consumer(cfg: &MatterSimConfig, consumer: MatterOutputConsumer) -> Vec<FieldId> {
    cfg.outputs
        .iter()
        .filter(|f| consumer.can_consume(f))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_declares_expected_io() {
        let w = MatterSimConfig::water_rivers();
        assert!(w.outputs.contains(&FieldId::RiverChannel));
        assert!(w.outputs.contains(&FieldId::WaterDepth));
        assert!(w.outputs.contains(&FieldId::Floodplain));
        assert!(w.inputs.contains(&FieldId::Height));
    }

    #[test]
    fn sand_build_scenario_uses_sand_pass() {
        let s = MatterSimConfig::sand().build_scenario();
        assert!(s.passes.iter().any(|p| p.kind == ScenarioPassKind::Sand));
    }
}
