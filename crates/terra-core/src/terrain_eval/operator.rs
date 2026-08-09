//! Terrain operator descriptors and layer adapters.

use crate::fields::FieldId;
use crate::layer::{FieldContract, Layer, LayerId, LayerKind};
use crate::tiling::DirtyClass;
use serde::{Deserialize, Serialize};

use super::mode::EvalMode;
use super::scale::ScaleBand;
use super::tiling::TileEvalSpec;

/// Stable operator identity (layer-backed or synthetic derived ops).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OperatorId {
    Layer(LayerId),
    Derived(FieldId),
    Synthetic(String),
}

/// Optional GPU kernel tag (terra-gpu binds implementations).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GpuKernelId(pub String);

impl GpuKernelId {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

/// Full declaration for a terrain operation.
///
/// Used to build execution dependencies automatically from the artist layer stack.
#[derive(Debug, Clone)]
pub struct OperatorDescriptor {
    pub id: OperatorId,
    pub name: String,
    pub reads: Vec<FieldId>,
    pub read_writes: Vec<FieldId>,
    pub writes: Vec<FieldId>,
    pub modifies_height: bool,
    pub modifies_geology: bool,
    pub iterative: bool,
    pub supports_incremental: bool,
    pub preferred_scale: ScaleBand,
    /// Preferred physical sample spacing (metres); `None` = inherit eval mode.
    pub preferred_sample_spacing_m: Option<f32>,
    pub tile: TileEvalSpec,
    pub dirty_class: DirtyClass,
    pub deterministic_seed: u64,
    pub gpu_kernels: Vec<GpuKernelId>,
    pub has_cpu_reference: bool,
}

impl OperatorDescriptor {
    pub fn from_layer(layer: &Layer, layer_seed: u64) -> Self {
        let contract = FieldContract::from_kind(&layer.kind);
        let reads = {
            let mut r = contract.required.clone();
            for o in &contract.optional {
                if !r.contains(o) {
                    r.push(o.clone());
                }
            }
            // Modifiers always conceptually read height even if listed elsewhere.
            if !r.contains(&FieldId::Height)
                && matches!(
                    layer.kind.category(),
                    crate::layer::OperationCategory::Modifier
                        | crate::layer::OperationCategory::Simulation
                        | crate::layer::OperationCategory::Surface
                )
            {
                r.insert(0, FieldId::Height);
            }
            r
        };
        let writes = contract.produced.clone();
        let read_writes: Vec<FieldId> = writes
            .iter()
            .filter(|f| reads.contains(f))
            .cloned()
            .collect();
        let writes_only: Vec<FieldId> = writes
            .iter()
            .filter(|f| !read_writes.contains(f))
            .cloned()
            .collect();

        let modifies_height = writes.contains(&FieldId::Height);
        let modifies_geology = writes.iter().any(|f| {
            matches!(
                f,
                FieldId::Hardness
                    | FieldId::Erodibility
                    | FieldId::BedrockHeight
                    | FieldId::SoilDepth
                    | FieldId::SoilThickness
                    | FieldId::SedimentThickness
                    | FieldId::StrataMaterialId
                    | FieldId::Lithology
                    | FieldId::StrataReference
                    | FieldId::Materials
            )
        });
        let iterative = matches!(
            layer.kind,
            LayerKind::ThermalErosion(_)
                | LayerKind::HydraulicErosion(_)
                | LayerKind::StreamPowerErosion(_)
                | LayerKind::MultiScaleAmplify(_)
                | LayerKind::LandscapeEvolution(_)
                | LayerKind::HydrologyRepair(_)
                | LayerKind::SandSimulation(_)
                | LayerKind::FluidSimulation(_)
        );
        let dirty = contract.spatial;
        let requires_global = matches!(dirty, DirtyClass::BasinDependent);
        let halo = match dirty {
            DirtyClass::Local => 2,
            DirtyClass::Expanding => 8,
            DirtyClass::BasinDependent => 16,
        };
        let preferred_scale = layer.kind.scale_band();
        let gpu_kernels = gpu_kernels_for_kind(&layer.kind);

        OperatorDescriptor {
            id: OperatorId::Layer(layer.id()),
            name: layer.common.name.clone(),
            reads,
            read_writes,
            writes: writes_only,
            modifies_height,
            modifies_geology,
            iterative,
            supports_incremental: matches!(dirty, DirtyClass::Local),
            preferred_scale,
            preferred_sample_spacing_m: Some(preferred_scale.preferred_sample_spacing_m()),
            tile: if requires_global {
                TileEvalSpec::basin_global(halo)
            } else {
                TileEvalSpec::local(halo)
            },
            dirty_class: dirty,
            deterministic_seed: layer_seed,
            gpu_kernels,
            has_cpu_reference: true,
        }
    }

    pub fn derived(field: FieldId, reads: Vec<FieldId>) -> Self {
        let name = field.display_name();
        OperatorDescriptor {
            id: OperatorId::Derived(field.clone()),
            name,
            reads,
            read_writes: Vec::new(),
            writes: vec![field],
            modifies_height: false,
            modifies_geology: false,
            iterative: false,
            supports_incremental: true,
            preferred_scale: ScaleBand::Meso,
            preferred_sample_spacing_m: None,
            tile: TileEvalSpec::local(1),
            dirty_class: DirtyClass::Local,
            deterministic_seed: 0,
            gpu_kernels: Vec::new(),
            has_cpu_reference: true,
        }
    }

    pub fn all_outputs(&self) -> Vec<FieldId> {
        let mut out = self.read_writes.clone();
        for w in &self.writes {
            if !out.contains(w) {
                out.push(w.clone());
            }
        }
        out
    }

    pub fn all_inputs(&self) -> Vec<FieldId> {
        let mut inp = self.reads.clone();
        for rw in &self.read_writes {
            if !inp.contains(rw) {
                inp.push(rw.clone());
            }
        }
        inp
    }
}

fn gpu_kernels_for_kind(kind: &LayerKind) -> Vec<GpuKernelId> {
    match kind {
        LayerKind::ThermalErosion(_) => vec![GpuKernelId::new("thermal_erosion")],
        LayerKind::HydraulicErosion(_) => vec![GpuKernelId::new("hydraulic_erosion")],
        LayerKind::StreamPowerErosion(_) => vec![GpuKernelId::new("stream_power")],
        LayerKind::LandscapeEvolution(_) => vec![
            GpuKernelId::new("stream_power"),
            GpuKernelId::new("landscape_evolution"),
        ],
        LayerKind::RiverCarve(_) => vec![GpuKernelId::new("river_carve")],
        LayerKind::NoisePerlin(_) | LayerKind::Fbm(_) | LayerKind::Ridged(_) => {
            vec![GpuKernelId::new("noise")]
        }
        LayerKind::Blur(_) => vec![GpuKernelId::new("blur")],
        _ => Vec::new(),
    }
}

/// Trait for multi-field terrain operations.
///
/// Phase 1: descriptors drive the graph; execution still often delegates to
/// `ProcessorRegistry`. Implementors may provide CPU and/or GPU paths later.
pub trait TerrainOperator: Send + Sync {
    fn descriptor(&self) -> &OperatorDescriptor;

    /// Optional CPU reference evaluation against a field set.
    /// Default: unsupported (caller uses legacy processor path).
    fn evaluate_cpu(
        &self,
        _state: &mut super::TerrainFieldSet,
        _mode: EvalMode,
    ) -> Result<(), String> {
        Err("CPU operator path not implemented; use StackEvaluator".into())
    }

    fn supports_gpu(&self) -> bool {
        !self.descriptor().gpu_kernels.is_empty()
    }
}

/// Adapter that exposes an existing layer as a [`TerrainOperator`].
#[derive(Debug, Clone)]
pub struct LayerOperatorAdapter {
    pub layer_id: LayerId,
    pub descriptor: OperatorDescriptor,
}

impl LayerOperatorAdapter {
    pub fn new(layer: &Layer, world_seed: u64) -> Self {
        let seed = super::determinism::layer_seed(world_seed, layer.id());
        Self {
            layer_id: layer.id(),
            descriptor: OperatorDescriptor::from_layer(layer, seed),
        }
    }
}

impl TerrainOperator for LayerOperatorAdapter {
    fn descriptor(&self) -> &OperatorDescriptor {
        &self.descriptor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::HydraulicErosionParams;

    #[test]
    fn hydraulic_declares_multi_field_contract() {
        let layer = Layer::new(
            "Hydro",
            LayerKind::HydraulicErosion(HydraulicErosionParams::default()),
        );
        let d = OperatorDescriptor::from_layer(&layer, 1);
        assert!(d.modifies_height);
        assert!(d.iterative);
        assert!(d.tile.requires_global_pass || d.dirty_class == DirtyClass::Expanding);
        assert!(d.reads.contains(&FieldId::Height));
        assert!(d.all_outputs().contains(&FieldId::Wetness));
    }
}
