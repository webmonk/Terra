//! Simulation Scenarios — artist-facing authoring containers for physical setups.
//!
//! A [`SimulationScenario`] is **not** a second solver. It groups Domain, Sources,
//! Passes, Quality, and Result Snapshots around existing Simulation Layers executed
//! by [`crate::eval::StackEvaluator`]. Scenarios may be created, edited, run, frozen,
//! or ignored at any time — they are not mandatory workflow steps.
//!
//! Three spatial concepts are kept separate:
//! 1. **Source** — where matter originates
//! 2. **Simulation Domain** — where matter may move
//! 3. **Output Influence** — where results may affect the project

use crate::biome_definition::BiomeDefinitionId;
use crate::domain::SoftDiagnostic;
use crate::fields::FieldId;
use crate::layer::{CachePolicy, LayerId, LayerKind};
use crate::mask::PlacementDefinition;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Stable identity for a Simulation Scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SimulationScenarioId(pub Uuid);

impl SimulationScenarioId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SimulationScenarioId {
    fn default() -> Self {
        Self::new()
    }
}

/// Where a scenario is authored / may run.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioScope {
    #[default]
    World,
    SelectedBiomes(Vec<BiomeDefinitionId>),
    PaintedDomain {
        #[serde(default)]
        paint_mask: Option<crate::mask::MaskId>,
    },
    CustomConditions {
        #[serde(default)]
        placement: PlacementDefinition,
    },
}

impl ScenarioScope {
    pub fn label(&self) -> &'static str {
        match self {
            Self::World => "World",
            Self::SelectedBiomes(_) => "Selected Biomes",
            Self::PaintedDomain { .. } => "Painted Domain",
            Self::CustomConditions { .. } => "Custom Conditions",
        }
    }
}

/// Where matter originates (rainfall, springs, snowmelt, …).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatterSourceKind {
    Rainfall,
    PaintedSprings,
    Snowmelt,
    RiverInflow,
    WindSediment,
    Custom { label: String },
}

impl MatterSourceKind {
    pub fn label(&self) -> String {
        match self {
            Self::Rainfall => "Rainfall".into(),
            Self::PaintedSprings => "Painted Springs".into(),
            Self::Snowmelt => "Snowmelt".into(),
            Self::RiverInflow => "River Inflow".into(),
            Self::WindSediment => "Wind Sediment".into(),
            Self::Custom { label } => label.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MatterSource {
    pub id: Uuid,
    pub kind: MatterSourceKind,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Optional strength / rate \[0,1\] or relative scale.
    #[serde(default = "default_strength")]
    pub strength: f32,
    /// Optional painted / mask restriction for this source only.
    #[serde(default)]
    pub restriction: Option<PlacementDefinition>,
}

fn default_true() -> bool {
    true
}
fn default_strength() -> f32 {
    1.0
}

impl MatterSource {
    pub fn new(kind: MatterSourceKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            enabled: true,
            strength: 1.0,
            restriction: None,
        }
    }
}

/// Spatial extent where matter may move during the scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulationDomain {
    /// Same as scenario scope.
    InheritScope,
    EntireWorld,
    /// Named domain label for artist-facing docs (resolved at run time).
    NamedRegion {
        name: String,
    },
    Painted {
        #[serde(default)]
        paint_mask: Option<crate::mask::MaskId>,
    },
    CustomConditions {
        #[serde(default)]
        placement: PlacementDefinition,
    },
}

impl Default for SimulationDomain {
    fn default() -> Self {
        Self::InheritScope
    }
}

impl SimulationDomain {
    pub fn label(&self) -> String {
        match self {
            Self::InheritScope => "Inherit Scope".into(),
            Self::EntireWorld => "Entire World".into(),
            Self::NamedRegion { name } => name.clone(),
            Self::Painted { .. } => "Painted Domain".into(),
            Self::CustomConditions { .. } => "Custom Conditions".into(),
        }
    }
}

/// Where generated results may affect the project (may be smaller than domain).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputInfluence {
    /// Results apply everywhere the simulation ran.
    EntireDomain,
    InheritScope,
    Painted {
        #[serde(default)]
        paint_mask: Option<crate::mask::MaskId>,
    },
    CustomConditions {
        #[serde(default)]
        placement: PlacementDefinition,
    },
}

impl Default for OutputInfluence {
    fn default() -> Self {
        Self::EntireDomain
    }
}

impl OutputInfluence {
    pub fn label(&self) -> &'static str {
        match self {
            Self::EntireDomain => "Entire Domain",
            Self::InheritScope => "Inherit Scope",
            Self::Painted { .. } => "Painted",
            Self::CustomConditions { .. } => "Custom Conditions",
        }
    }
}

/// Artist-facing pass kinds that map onto existing Simulation Layer kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioPassKind {
    Flow,
    RiverExtraction,
    HydraulicErosion,
    ThermalErosion,
    DebrisFlow,
    StreamPower,
    Sediment,
    LandscapeEvolution,
    HydrologyRepair,
    Sand,
    Fluid,
    Coastal,
    RiverCarve,
    EcosystemFeedback,
    GeomorphicDetail,
    MultiScaleAmplify,
}

impl ScenarioPassKind {
    pub fn all() -> &'static [ScenarioPassKind] {
        &[
            Self::Flow,
            Self::RiverExtraction,
            Self::HydraulicErosion,
            Self::ThermalErosion,
            Self::DebrisFlow,
            Self::StreamPower,
            Self::Sediment,
            Self::LandscapeEvolution,
            Self::HydrologyRepair,
            Self::Sand,
            Self::Fluid,
            Self::Coastal,
            Self::RiverCarve,
            Self::EcosystemFeedback,
            Self::GeomorphicDetail,
            Self::MultiScaleAmplify,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Flow => "Flow",
            Self::RiverExtraction => "River Extraction",
            Self::HydraulicErosion => "Hydraulic Erosion",
            Self::ThermalErosion => "Thermal Erosion",
            Self::DebrisFlow => "Debris Flow",
            Self::StreamPower => "Stream Power",
            Self::Sediment => "Sediment",
            Self::LandscapeEvolution => "Landscape Evolution",
            Self::HydrologyRepair => "Hydrology Repair",
            Self::Sand => "Sand",
            Self::Fluid => "Fluid",
            Self::Coastal => "Coastal",
            Self::RiverCarve => "River Carve",
            Self::EcosystemFeedback => "Ecosystem Feedback",
            Self::GeomorphicDetail => "Geomorphic Detail",
            Self::MultiScaleAmplify => "Multi-Scale Amplify",
        }
    }

    /// Default stack position band (lower = earlier). Used for ordering diagnostics.
    pub fn order_band(self) -> u8 {
        match self {
            Self::Flow | Self::HydrologyRepair => 0,
            Self::RiverExtraction | Self::RiverCarve => 1,
            Self::LandscapeEvolution | Self::StreamPower | Self::HydraulicErosion => 2,
            Self::DebrisFlow => 2,
            Self::ThermalErosion | Self::Sediment | Self::MultiScaleAmplify => 3,
            Self::Sand | Self::Fluid | Self::Coastal => 4,
            Self::GeomorphicDetail | Self::EcosystemFeedback => 5,
        }
    }

    /// Fields this pass typically consumes (beyond Height).
    pub fn typical_inputs(self) -> &'static [FieldId] {
        match self {
            Self::HydraulicErosion | Self::Sediment => &[FieldId::Hardness, FieldId::Rainfall],
            Self::DebrisFlow => &[
                FieldId::Hardness,
                FieldId::BedrockHeight,
                FieldId::DebrisDepth,
                FieldId::SedimentThickness,
            ],
            Self::StreamPower | Self::LandscapeEvolution => &[FieldId::Hardness],
            Self::ThermalErosion => &[FieldId::Hardness, FieldId::BedrockHeight, FieldId::DebrisDepth],
            Self::RiverExtraction | Self::RiverCarve => &[FieldId::FlowAccumulation],
            Self::EcosystemFeedback => &[FieldId::Hardness, FieldId::Vegetation],
            _ => &[],
        }
    }

    /// Fields this pass typically produces.
    pub fn typical_outputs(self) -> &'static [FieldId] {
        match self {
            Self::Flow => &[FieldId::FlowAccumulation, FieldId::FlowDirection],
            Self::RiverExtraction | Self::RiverCarve => &[
                FieldId::FlowAccumulation,
                FieldId::StreamOrder,
                FieldId::Wetness,
            ],
            Self::HydraulicErosion => &[
                FieldId::Wetness,
                FieldId::Sediment,
                FieldId::Erosion,
                FieldId::Deposition,
            ],
            Self::ThermalErosion => &[
                FieldId::Erosion,
                FieldId::Deposition,
                FieldId::DebrisDepth,
                FieldId::TalusStability,
            ],
            Self::DebrisFlow => &[
                FieldId::Erosion,
                FieldId::Deposition,
                FieldId::DebrisDepth,
                FieldId::SlidePath,
                FieldId::Instability,
                FieldId::FlowAccumulation,
            ],
            Self::StreamPower | Self::LandscapeEvolution => &[
                FieldId::FlowAccumulation,
                FieldId::StreamOrder,
                FieldId::SpeIncision,
                FieldId::Erosion,
            ],
            Self::Sediment => &[FieldId::Sediment, FieldId::Deposition],
            Self::Fluid => &[FieldId::Wetness],
            _ => &[],
        }
    }

    /// Map to a concrete LayerKind for ensure / compatibility.
    pub fn default_layer_kind(self) -> LayerKind {
        use crate::layer::*;
        match self {
            Self::Flow | Self::RiverExtraction => {
                LayerKind::RiverNetwork(RiverNetworkParams::default())
            }
            Self::HydraulicErosion | Self::Sediment => {
                LayerKind::HydraulicErosion(HydraulicErosionParams::default())
            }
            Self::ThermalErosion => LayerKind::ThermalErosion(ThermalErosionParams::default()),
            Self::DebrisFlow => LayerKind::DebrisFlow(DebrisFlowParams::default()),
            Self::StreamPower => LayerKind::StreamPowerErosion(StreamPowerParams::default()),
            Self::LandscapeEvolution => {
                LayerKind::LandscapeEvolution(LandscapeEvolutionParams::default())
            }
            Self::HydrologyRepair => LayerKind::HydrologyRepair(HydrologyRepairParams::default()),
            Self::Sand => LayerKind::SandSimulation(SandSimParams::default()),
            Self::Fluid => LayerKind::FluidSimulation(FluidSimParams::default()),
            Self::Coastal => LayerKind::Coastal(CoastalParams::default()),
            Self::RiverCarve => LayerKind::RiverCarve(RiverCarveParams::default()),
            Self::EcosystemFeedback => {
                LayerKind::EcosystemFeedback(EcosystemFeedbackParams::default())
            }
            Self::GeomorphicDetail => {
                LayerKind::GeomorphicDetail(GeomorphicDetailParams::default())
            }
            Self::MultiScaleAmplify => {
                LayerKind::MultiScaleAmplify(MultiScaleAmplifyParams::default())
            }
        }
    }

    pub fn from_layer_kind(kind: &LayerKind) -> Option<Self> {
        match kind {
            LayerKind::RiverNetwork(_) => Some(Self::RiverExtraction),
            LayerKind::HydraulicErosion(_) => Some(Self::HydraulicErosion),
            LayerKind::ThermalErosion(_) => Some(Self::ThermalErosion),
            LayerKind::DebrisFlow(_) => Some(Self::DebrisFlow),
            LayerKind::StreamPowerErosion(_) => Some(Self::StreamPower),
            LayerKind::LandscapeEvolution(_) => Some(Self::LandscapeEvolution),
            LayerKind::HydrologyRepair(_) => Some(Self::HydrologyRepair),
            LayerKind::SandSimulation(_) => Some(Self::Sand),
            LayerKind::FluidSimulation(_) => Some(Self::Fluid),
            LayerKind::Coastal(_) => Some(Self::Coastal),
            LayerKind::RiverCarve(_) => Some(Self::RiverCarve),
            LayerKind::EcosystemFeedback(_) => Some(Self::EcosystemFeedback),
            LayerKind::GeomorphicDetail(_) => Some(Self::GeomorphicDetail),
            LayerKind::MultiScaleAmplify(_) => Some(Self::MultiScaleAmplify),
            _ => None,
        }
    }
}

/// One pass in a scenario — references an existing Simulation Layer when bound.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioPass {
    pub id: Uuid,
    pub name: String,
    pub kind: ScenarioPassKind,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Bound Simulation Layer instance (None = unbound template / ensure later).
    #[serde(default)]
    pub layer_id: Option<LayerId>,
    /// Declared inputs (defaults from kind).
    #[serde(default)]
    pub inputs: Vec<FieldId>,
    /// Declared outputs (defaults from kind).
    #[serde(default)]
    pub outputs: Vec<FieldId>,
    /// Explicit dependencies on other pass ids in this scenario.
    #[serde(default)]
    pub depends_on: Vec<Uuid>,
}

impl ScenarioPass {
    pub fn new(kind: ScenarioPassKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: kind.label().into(),
            kind,
            enabled: true,
            layer_id: None,
            inputs: kind.typical_inputs().to_vec(),
            outputs: kind.typical_outputs().to_vec(),
            depends_on: Vec::new(),
        }
    }

    pub fn from_layer(
        layer_id: LayerId,
        kind: &LayerKind,
        name: impl Into<String>,
    ) -> Option<Self> {
        let pass_kind = ScenarioPassKind::from_layer_kind(kind)?;
        let mut p = Self::new(pass_kind);
        p.layer_id = Some(layer_id);
        p.name = name.into();
        // Prefer contract fields when available.
        p.inputs = kind.required_fields();
        p.outputs = kind
            .produced_fields()
            .into_iter()
            .filter(|f| *f != FieldId::Height)
            .collect();
        Some(p)
    }
}

/// Quality preset for scenario runs (maps onto existing PreviewQuality / iteration caps).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioQuality {
    Draft,
    #[default]
    Medium,
    High,
    Final,
}

impl ScenarioQuality {
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::Final => "Final",
        }
    }

    pub fn iteration_scale(self) -> f32 {
        match self {
            Self::Draft => 0.25,
            Self::Medium => 0.5,
            Self::High => 1.0,
            Self::Final => 1.5,
        }
    }
}

/// Iteration / timeline settings for the scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioTimeline {
    #[serde(default = "default_iterations")]
    pub iterations: u32,
    /// Optional physical time step (seconds) when the solver supports it.
    #[serde(default)]
    pub timestep: Option<f32>,
    /// Pause points (iteration indices) when Pause is supported.
    #[serde(default)]
    pub pause_at: Vec<u32>,
}

fn default_iterations() -> u32 {
    32
}

impl Default for ScenarioTimeline {
    fn default() -> Self {
        Self {
            iterations: default_iterations(),
            timestep: None,
            pause_at: Vec::new(),
        }
    }
}

/// Result lifecycle state for a scenario snapshot / run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioResultState {
    #[default]
    Ready,
    Running,
    /// Actively applied / current in the project.
    Current,
    /// Upstream changed; snapshot kept.
    Outdated,
    /// Artist froze this snapshot — never auto-delete.
    Frozen,
    Failed,
    Cancelled,
}

impl ScenarioResultState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Running => "Running",
            Self::Current => "Current",
            Self::Outdated => "Outdated",
            Self::Frozen => "Frozen",
            Self::Failed => "Failed",
            Self::Cancelled => "Cancelled",
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Ready
                | Self::Current
                | Self::Outdated
                | Self::Frozen
                | Self::Failed
                | Self::Cancelled
        )
    }
}

/// Non-destructive snapshot of scenario outputs (references, not a second solver buffer).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioSnapshot {
    pub id: Uuid,
    pub label: String,
    pub state: ScenarioResultState,
    /// Generation counter when captured (for outdated detection).
    #[serde(default)]
    pub generation: u64,
    /// Which FieldIds were retained in this snapshot.
    #[serde(default)]
    pub retained_fields: Vec<FieldId>,
    /// Optional published output ids from layers (stable cross-refs).
    #[serde(default)]
    pub published_outputs: Vec<crate::layer::OutputId>,
    /// Lightweight coverage / hash fingerprint (not full heightfield).
    #[serde(default)]
    pub fingerprint: u64,
    /// When true, Apply Selected Outputs may write these fields into the project.
    #[serde(default)]
    pub apply_selected: Vec<FieldId>,
    #[serde(default)]
    pub notes: String,
}

impl ScenarioSnapshot {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            label: label.into(),
            state: ScenarioResultState::Ready,
            generation: 0,
            retained_fields: Vec::new(),
            published_outputs: Vec::new(),
            fingerprint: 0,
            apply_selected: Vec::new(),
            notes: String::new(),
        }
    }
}

/// How outputs from a snapshot are applied into the live project.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OutputApplicationSettings {
    /// Fields to apply when the artist chooses Apply Selected Outputs.
    #[serde(default)]
    pub selected_fields: Vec<FieldId>,
    /// When true, height deltas from the scenario may update the live stack.
    #[serde(default)]
    pub apply_height: bool,
    /// When true, aux maps (wetness, sediment, …) are published for downstream use.
    #[serde(default = "default_true")]
    pub apply_aux: bool,
    /// Respect Output Influence mask when applying.
    #[serde(default = "default_true")]
    pub respect_influence: bool,
}

/// Artist-facing Simulation Scenario (authoring container).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationScenario {
    pub id: SimulationScenarioId,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub scope: ScenarioScope,
    #[serde(default)]
    pub sources: Vec<MatterSource>,
    #[serde(default)]
    pub domain: SimulationDomain,
    #[serde(default)]
    pub output_influence: OutputInfluence,
    #[serde(default)]
    pub passes: Vec<ScenarioPass>,
    #[serde(default)]
    pub quality: ScenarioQuality,
    #[serde(default)]
    pub timeline: ScenarioTimeline,
    #[serde(default)]
    pub snapshots: Vec<ScenarioSnapshot>,
    /// Currently previewed snapshot (Preview Old Result).
    #[serde(default)]
    pub preview_snapshot: Option<Uuid>,
    /// Snapshot marked Current / applied.
    #[serde(default)]
    pub current_snapshot: Option<Uuid>,
    /// Snapshot held for Compare (A/B).
    #[serde(default)]
    pub compare_snapshot: Option<Uuid>,
    #[serde(default)]
    pub output_application: OutputApplicationSettings,
    /// Optional matter-sim artist configs (Water / Snow / Sand / Debris).
    #[serde(default)]
    pub matter: Vec<crate::matter_sim::MatterSimConfig>,
    #[serde(default)]
    pub cache_policy: CachePolicy,
    /// Aggregate result state for the scenario run.
    #[serde(default)]
    pub result_state: ScenarioResultState,
    /// Cancellation requested while Running.
    #[serde(default)]
    pub cancel_requested: bool,
    /// Pause requested (when supported).
    #[serde(default)]
    pub pause_requested: bool,
    #[serde(default)]
    pub last_error: Option<String>,
}

impl SimulationScenario {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: SimulationScenarioId::new(),
            name: name.into(),
            enabled: true,
            scope: ScenarioScope::World,
            sources: Vec::new(),
            domain: SimulationDomain::InheritScope,
            output_influence: OutputInfluence::EntireDomain,
            passes: Vec::new(),
            quality: ScenarioQuality::Medium,
            timeline: ScenarioTimeline::default(),
            snapshots: Vec::new(),
            preview_snapshot: None,
            current_snapshot: None,
            compare_snapshot: None,
            output_application: OutputApplicationSettings::default(),
            matter: Vec::new(),
            cache_policy: CachePolicy::Manual,
            result_state: ScenarioResultState::Ready,
            cancel_requested: false,
            pause_requested: false,
            last_error: None,
        }
    }

    pub fn continental_hydrology_preset() -> Self {
        let mut s = Self::new("Continental Hydrology");
        s.sources = vec![
            MatterSource::new(MatterSourceKind::Rainfall),
            MatterSource::new(MatterSourceKind::PaintedSprings),
            MatterSource::new(MatterSourceKind::Snowmelt),
        ];
        s.domain = SimulationDomain::NamedRegion {
            name: "Main Continent".into(),
        };
        s.output_influence = OutputInfluence::EntireDomain;
        s.passes = vec![
            ScenarioPass::new(ScenarioPassKind::Flow),
            ScenarioPass::new(ScenarioPassKind::RiverExtraction),
            ScenarioPass::new(ScenarioPassKind::HydraulicErosion),
            ScenarioPass::new(ScenarioPassKind::Sediment),
        ];
        // Wire soft dependencies by order.
        for i in 1..s.passes.len() {
            let prev = s.passes[i - 1].id;
            s.passes[i].depends_on.push(prev);
        }
        s.timeline.iterations = 48;
        s
    }

    pub fn enabled_passes(&self) -> impl Iterator<Item = &ScenarioPass> {
        self.passes.iter().filter(|p| p.enabled)
    }

    pub fn bound_layer_ids(&self) -> Vec<LayerId> {
        self.passes
            .iter()
            .filter(|p| p.enabled)
            .filter_map(|p| p.layer_id)
            .collect()
    }

    /// Clear diagnostics for invalid pass ordering / missing deps.
    pub fn diagnostics(&self) -> Vec<SoftDiagnostic> {
        diagnose_scenario(self)
    }

    // —— Lifecycle ——

    pub fn begin_run(&mut self) {
        self.cancel_requested = false;
        self.pause_requested = false;
        self.last_error = None;
        self.result_state = ScenarioResultState::Running;
    }

    pub fn request_pause(&mut self) {
        if self.result_state == ScenarioResultState::Running {
            self.pause_requested = true;
        }
    }

    pub fn request_cancel(&mut self) {
        if self.result_state == ScenarioResultState::Running {
            self.cancel_requested = true;
        }
    }

    pub fn mark_cancelled(&mut self) {
        self.result_state = ScenarioResultState::Cancelled;
        self.cancel_requested = false;
        self.pause_requested = false;
    }

    pub fn mark_failed(&mut self, message: impl Into<String>) {
        self.result_state = ScenarioResultState::Failed;
        self.last_error = Some(message.into());
        self.cancel_requested = false;
        self.pause_requested = false;
    }

    pub fn complete_run(&mut self, generation: u64) -> Uuid {
        let mut snap = ScenarioSnapshot::new(format!("{} · gen {generation}", self.name));
        snap.state = ScenarioResultState::Current;
        snap.generation = generation;
        snap.retained_fields = self
            .enabled_passes()
            .flat_map(|p| p.outputs.iter().cloned())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        snap.apply_selected = self.output_application.selected_fields.clone();
        let id = snap.id;
        // Previous current → Outdated (unless Frozen).
        if let Some(cur) = self.current_snapshot {
            if let Some(s) = self.snapshots.iter_mut().find(|s| s.id == cur) {
                if s.state != ScenarioResultState::Frozen {
                    s.state = ScenarioResultState::Outdated;
                }
            }
        }
        self.snapshots.push(snap);
        self.current_snapshot = Some(id);
        self.result_state = ScenarioResultState::Current;
        self.cancel_requested = false;
        self.pause_requested = false;
        id
    }

    /// Upstream project change — mark current result Outdated, keep snapshots.
    pub fn mark_outdated(&mut self) {
        if matches!(
            self.result_state,
            ScenarioResultState::Frozen | ScenarioResultState::Running
        ) {
            // Frozen scenario stays Frozen; running continues but will be outdated after.
            if self.result_state == ScenarioResultState::Frozen {
                return;
            }
        }
        self.result_state = ScenarioResultState::Outdated;
        if let Some(cur) = self.current_snapshot {
            if let Some(s) = self.snapshots.iter_mut().find(|s| s.id == cur) {
                if s.state != ScenarioResultState::Frozen {
                    s.state = ScenarioResultState::Outdated;
                }
            }
        }
    }

    pub fn freeze_result(&mut self, snapshot_id: Option<Uuid>) -> bool {
        let id = snapshot_id.or(self.current_snapshot);
        let Some(id) = id else {
            return false;
        };
        if let Some(s) = self.snapshots.iter_mut().find(|s| s.id == id) {
            s.state = ScenarioResultState::Frozen;
            self.result_state = ScenarioResultState::Frozen;
            true
        } else {
            false
        }
    }

    pub fn preview_old_result(&mut self, snapshot_id: Uuid) -> bool {
        if self.snapshots.iter().any(|s| s.id == snapshot_id) {
            self.preview_snapshot = Some(snapshot_id);
            true
        } else {
            false
        }
    }

    pub fn set_compare(&mut self, snapshot_id: Option<Uuid>) {
        self.compare_snapshot = snapshot_id;
    }

    pub fn reset(&mut self) {
        // Keep frozen snapshots; clear non-frozen and runtime flags.
        self.snapshots
            .retain(|s| s.state == ScenarioResultState::Frozen);
        self.current_snapshot = None;
        self.preview_snapshot = None;
        self.compare_snapshot = None;
        self.result_state = ScenarioResultState::Ready;
        self.cancel_requested = false;
        self.pause_requested = false;
        self.last_error = None;
    }

    pub fn rebuild(&mut self) {
        self.result_state = ScenarioResultState::Ready;
        self.cancel_requested = false;
        self.pause_requested = false;
        self.last_error = None;
        // Snapshots retained (non-destructive); next Run creates a new Current.
    }

    /// Apply selected outputs from a snapshot into application settings (selective apply).
    pub fn apply_selected_outputs(&mut self, snapshot_id: Option<Uuid>) -> Vec<FieldId> {
        let id = snapshot_id.or(self.current_snapshot);
        let Some(id) = id else {
            return Vec::new();
        };
        let Some(snap) = self.snapshots.iter().find(|s| s.id == id) else {
            return Vec::new();
        };
        let fields = if !snap.apply_selected.is_empty() {
            snap.apply_selected.clone()
        } else if !self.output_application.selected_fields.is_empty() {
            self.output_application.selected_fields.clone()
        } else {
            snap.retained_fields.clone()
        };
        self.current_snapshot = Some(id);
        if let Some(s) = self.snapshots.iter_mut().find(|s| s.id == id) {
            if s.state != ScenarioResultState::Frozen {
                s.state = ScenarioResultState::Current;
            }
        }
        self.result_state = ScenarioResultState::Current;
        fields
    }

    pub fn add_pass(&mut self, kind: ScenarioPassKind) -> Uuid {
        let pass = ScenarioPass::new(kind);
        let id = pass.id;
        if let Some(last) = self.passes.last() {
            let mut p = pass;
            p.depends_on.push(last.id);
            self.passes.push(p);
        } else {
            self.passes.push(pass);
        }
        id
    }

    pub fn reorder_pass(&mut self, pass_id: Uuid, to_index: usize) -> bool {
        let Some(from) = self.passes.iter().position(|p| p.id == pass_id) else {
            return false;
        };
        let pass = self.passes.remove(from);
        let to = to_index.min(self.passes.len());
        self.passes.insert(to, pass);
        true
    }

    pub fn bind_pass_layer(&mut self, pass_id: Uuid, layer_id: LayerId) -> bool {
        if let Some(p) = self.passes.iter_mut().find(|p| p.id == pass_id) {
            p.layer_id = Some(layer_id);
            true
        } else {
            false
        }
    }
}

/// Project-level scenario library.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SimulationScenarioLibrary {
    pub scenarios: Vec<SimulationScenario>,
    #[serde(default)]
    pub selected: Option<SimulationScenarioId>,
    /// Which scenario is actively driving runs (optional).
    #[serde(default)]
    pub active: Option<SimulationScenarioId>,
}

impl SimulationScenarioLibrary {
    pub fn get(&self, id: SimulationScenarioId) -> Option<&SimulationScenario> {
        self.scenarios.iter().find(|s| s.id == id)
    }

    pub fn get_mut(&mut self, id: SimulationScenarioId) -> Option<&mut SimulationScenario> {
        self.scenarios.iter_mut().find(|s| s.id == id)
    }

    pub fn push(&mut self, scenario: SimulationScenario) -> SimulationScenarioId {
        let id = scenario.id;
        self.scenarios.push(scenario);
        self.selected = Some(id);
        id
    }

    pub fn remove(&mut self, id: SimulationScenarioId) -> Option<SimulationScenario> {
        let idx = self.scenarios.iter().position(|s| s.id == id)?;
        let removed = self.scenarios.remove(idx);
        if self.selected == Some(id) {
            self.selected = self.scenarios.first().map(|s| s.id);
        }
        if self.active == Some(id) {
            self.active = None;
        }
        Some(removed)
    }

    /// Mark all enabled scenarios outdated after upstream Shape / project edits.
    pub fn mark_all_outdated(&mut self) {
        for s in &mut self.scenarios {
            if s.enabled {
                s.mark_outdated();
            }
        }
    }

    pub fn active_mut(&mut self) -> Option<&mut SimulationScenario> {
        let id = self.active.or(self.selected)?;
        self.get_mut(id)
    }
}

/// Undoable scenario commands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SimulationScenarioCommand {
    Add {
        scenario: SimulationScenario,
        index: usize,
    },
    Remove {
        scenario: SimulationScenario,
        index: usize,
    },
    Replace {
        id: SimulationScenarioId,
        before: SimulationScenario,
        after: SimulationScenario,
    },
    SetEnabled {
        id: SimulationScenarioId,
        enabled: bool,
        previous: bool,
    },
    SetActive {
        id: Option<SimulationScenarioId>,
        previous: Option<SimulationScenarioId>,
    },
}

impl SimulationScenarioCommand {
    pub fn apply(&self, lib: &mut SimulationScenarioLibrary) {
        match self {
            Self::Add { scenario, index } => {
                let idx = (*index).min(lib.scenarios.len());
                lib.scenarios.insert(idx, scenario.clone());
                lib.selected = Some(scenario.id);
            }
            Self::Remove { scenario, .. } => {
                let _ = lib.remove(scenario.id);
            }
            Self::Replace { id, after, .. } => {
                if let Some(s) = lib.get_mut(*id) {
                    *s = after.clone();
                }
            }
            Self::SetEnabled { id, enabled, .. } => {
                if let Some(s) = lib.get_mut(*id) {
                    s.enabled = *enabled;
                }
            }
            Self::SetActive { id, .. } => {
                lib.active = *id;
            }
        }
    }

    pub fn invert(&self, lib: &mut SimulationScenarioLibrary) {
        match self {
            Self::Add { scenario, .. } => {
                let _ = lib.remove(scenario.id);
            }
            Self::Remove { scenario, index } => {
                let idx = (*index).min(lib.scenarios.len());
                lib.scenarios.insert(idx, scenario.clone());
                lib.selected = Some(scenario.id);
            }
            Self::Replace { id, before, .. } => {
                if let Some(s) = lib.get_mut(*id) {
                    *s = before.clone();
                }
            }
            Self::SetEnabled { id, previous, .. } => {
                if let Some(s) = lib.get_mut(*id) {
                    s.enabled = *previous;
                }
            }
            Self::SetActive { previous, .. } => {
                lib.active = *previous;
            }
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Self::Add { scenario, .. } => format!("Added Scenario {}", scenario.name),
            Self::Remove { scenario, .. } => format!("Removed Scenario {}", scenario.name),
            Self::Replace { after, .. } => format!("Edited Scenario {}", after.name),
            Self::SetEnabled { enabled, .. } => {
                if *enabled {
                    "Enabled Scenario".into()
                } else {
                    "Disabled Scenario".into()
                }
            }
            Self::SetActive { .. } => "Set Active Scenario".into(),
        }
    }
}

/// Non-blocking diagnostics for pass order / deps / empty setups.
pub fn diagnose_scenario(scenario: &SimulationScenario) -> Vec<SoftDiagnostic> {
    let mut out = Vec::new();
    if scenario.passes.is_empty() {
        out.push(SoftDiagnostic::new(
            "scenario_no_passes",
            format!(
                "‘{}’ has no simulation passes yet — add Flow, Erosion, or Sediment.",
                scenario.name
            ),
        ));
    }
    if scenario.sources.is_empty() {
        out.push(SoftDiagnostic::new(
            "scenario_no_sources",
            format!(
                "‘{}’ has no matter sources — rainfall / springs optional but recommended.",
                scenario.name
            ),
        ));
    }

    let id_set: HashSet<Uuid> = scenario.passes.iter().map(|p| p.id).collect();
    let index_of: HashMap<Uuid, usize> = scenario
        .passes
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id, i))
        .collect();

    for (i, pass) in scenario.passes.iter().enumerate() {
        for dep in &pass.depends_on {
            if !id_set.contains(dep) {
                out.push(SoftDiagnostic::new(
                    "scenario_missing_dependency",
                    format!(
                        "Pass ‘{}’ depends on a missing pass in ‘{}’.",
                        pass.name, scenario.name
                    ),
                ));
            } else if let Some(&dep_i) = index_of.get(dep) {
                if dep_i > i {
                    out.push(SoftDiagnostic::new(
                        "scenario_invalid_order",
                        format!(
                            "Pass ‘{}’ runs before its dependency — reorder passes in ‘{}’.",
                            pass.name, scenario.name
                        ),
                    ));
                }
            }
        }
        // Soft band check vs previous enabled pass.
        if i > 0 {
            let prev = &scenario.passes[i - 1];
            if pass.kind.order_band() < prev.kind.order_band() {
                out.push(SoftDiagnostic::new(
                    "scenario_unusual_order",
                    format!(
                        "‘{}’ ({}) appears after ‘{}’ ({}) — unusual but allowed.",
                        pass.name,
                        pass.kind.label(),
                        prev.name,
                        prev.kind.label()
                    ),
                ));
            }
        }
    }

    // Output influence vs domain — informational when painted influence with world domain.
    if matches!(scenario.output_influence, OutputInfluence::Painted { .. })
        && matches!(
            scenario.domain,
            SimulationDomain::EntireWorld | SimulationDomain::InheritScope
        )
    {
        out.push(SoftDiagnostic::new(
            "scenario_influence_subset",
            format!(
                "‘{}’ output influence is painted — results apply only inside the paint.",
                scenario.name
            ),
        ));
    }

    out
}

/// True when a LayerKind is compatible as a scenario pass (existing Simulation Layers).
pub fn layer_kind_is_scenario_compatible(kind: &LayerKind) -> bool {
    ScenarioPassKind::from_layer_kind(kind).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continental_preset_separates_source_domain_influence() {
        let s = SimulationScenario::continental_hydrology_preset();
        assert_eq!(s.sources.len(), 3);
        assert!(matches!(s.domain, SimulationDomain::NamedRegion { .. }));
        assert!(matches!(s.output_influence, OutputInfluence::EntireDomain));
        assert_eq!(s.passes.len(), 4);
    }

    #[test]
    fn invalid_order_emits_diagnostic() {
        let mut s = SimulationScenario::new("Test");
        let mut late = ScenarioPass::new(ScenarioPassKind::Flow);
        let early = ScenarioPass::new(ScenarioPassKind::Sediment);
        late.depends_on.push(early.id);
        // Put dependent BEFORE dependency → invalid.
        s.passes.push(late);
        s.passes.push(early);
        let diags = diagnose_scenario(&s);
        assert!(diags.iter().any(|d| d.code == "scenario_invalid_order"));
    }
}
