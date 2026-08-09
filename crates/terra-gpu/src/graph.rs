//! Compile a layer stack into an ordered GPU compute pass list.
//!
//! Artist UI remains layer-based; this IR is the internal execution plan for
//! `GpuTerrainEngine` (deps, dirty policy, fusion hooks).

use terra_core::layer::{Layer, LayerId, LayerKind, LayerStack};
use terra_core::tiling::DirtyClass;

/// Kind of GPU work a compiled pass performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuPassKind {
    Generator,
    Filter,
    Simulation,
    MaskBake,
    Blend,
    /// Materials / biomes / vegetation — height no-op; aux later.
    AuxNoop,
}

/// Dirty policy for a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuDirtyPolicy {
    /// Local ops — honor SampleRect + halo.
    Local,
    /// Basin / global coupling — full field (or level-step coarse).
    FullField,
}

/// One executable GPU pass corresponding to a flattened layer (or mask prep).
#[derive(Debug, Clone)]
pub struct GpuPass {
    pub layer_id: LayerId,
    pub kind: GpuPassKind,
    pub dirty_policy: GpuDirtyPolicy,
    /// Flattened index into `stack.flatten_layers()`.
    pub flat_index: usize,
    /// Approximate kernel halo in texels for dirty expansion.
    pub halo_texels: u32,
}

/// Compiled interactive GPU graph.
#[derive(Debug, Clone, Default)]
pub struct GpuComputeGraph {
    pub passes: Vec<GpuPass>,
    /// First flat index that is not GPU-supported (None = fully GPU).
    pub cpu_from: Option<usize>,
}

impl GpuComputeGraph {
    pub fn fully_gpu(&self) -> bool {
        self.cpu_from.is_none()
    }
}

fn pass_kind(kind: &LayerKind) -> Option<(GpuPassKind, GpuDirtyPolicy, u32)> {
    use LayerKind::*;
    match kind {
        Flat(_) | Ramp(_) | NoiseValue(_) | NoisePerlin(_) | Fbm(_) | Ridged(_) | Mountains(_)
        | Dunes(_) | Canyons(_) | DomainWarp(_) | SculptBase(_) | Mesa(_) | Volcano(_)
        | Island(_) | Plateau(_) | Uplift(_) => {
            Some((GpuPassKind::Generator, GpuDirtyPolicy::Local, 2))
        }
        Blur(p) => Some((
            GpuPassKind::Filter,
            GpuDirtyPolicy::Local,
            p.radius.max(1).min(16),
        )),
        EffectFilter(p) => Some((
            GpuPassKind::Filter,
            GpuDirtyPolicy::Local,
            p.radius.max(1).min(16),
        )),
        Terrace(_) | Coastal(_) => Some((GpuPassKind::Filter, GpuDirtyPolicy::Local, 4)),
        ThermalErosion(_) | HydraulicErosion(_) | RiverCarve(_) => {
            Some((GpuPassKind::Simulation, GpuDirtyPolicy::FullField, 0))
        }
        Materials(_) | Biomes(_) | Vegetation(_) => {
            Some((GpuPassKind::AuxNoop, GpuDirtyPolicy::Local, 0))
        }
        _ => None,
    }
}

/// Compile the preview stack into a GPU pass list.
///
/// Stops recording passes at the first unsupported layer (sets `cpu_from`).
pub fn compile_gpu_graph(stack: &LayerStack) -> GpuComputeGraph {
    let layers: Vec<&Layer> = stack.flatten_layers();
    let mut graph = GpuComputeGraph::default();
    for (flat_index, layer) in layers.iter().enumerate() {
        if !layer.common.enabled {
            continue;
        }
        let Some((kind, dirty_policy, halo)) = pass_kind(&layer.kind) else {
            graph.cpu_from = Some(flat_index);
            break;
        };
        // Painted / graph masks still force CPU until GPU mask graphs land.
        if !layer.common.masks.nodes.is_empty() {
            graph.cpu_from = Some(flat_index);
            break;
        }
        graph.passes.push(GpuPass {
            layer_id: layer.id(),
            kind,
            dirty_policy,
            flat_index,
            halo_texels: halo,
        });
    }
    graph
}

/// Expand a dirty rect by halo, clamped to field bounds.
pub fn expand_dirty_rect(
    rect: (u32, u32, u32, u32),
    halo: u32,
    width: u32,
    height: u32,
) -> (u32, u32, u32, u32) {
    let (x, y, w, h) = rect;
    let x0 = x.saturating_sub(halo);
    let y0 = y.saturating_sub(halo);
    let x1 = (x + w).saturating_add(halo).min(width);
    let y1 = (y + h).saturating_add(halo).min(height);
    (x0, y0, x1.saturating_sub(x0), y1.saturating_sub(y0))
}

/// Map pass dirty policy to core `DirtyClass`.
pub fn dirty_class_for(policy: GpuDirtyPolicy) -> DirtyClass {
    match policy {
        GpuDirtyPolicy::Local => DirtyClass::Local,
        GpuDirtyPolicy::FullField => DirtyClass::BasinDependent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use terra_core::layer::{
        EffectFilterKind, EffectFilterParams, FlatParams, Layer, LayerKind, LayerStack, NoiseParams,
    };

    #[test]
    fn compiles_noise_filter_stack_as_fully_gpu() {
        let mut stack = LayerStack::new();
        stack.push(Layer::new(
            "noise",
            LayerKind::NoisePerlin(NoiseParams::default()),
        ));
        stack.push(Layer::new(
            "smooth",
            LayerKind::EffectFilter(EffectFilterParams {
                kind: EffectFilterKind::Smooth,
                ..EffectFilterParams::default()
            }),
        ));
        stack.push(Layer::new("flat", LayerKind::Flat(FlatParams::default())));
        let g = compile_gpu_graph(&stack);
        assert!(
            g.fully_gpu(),
            "expected fully GPU graph, cpu_from={:?}",
            g.cpu_from
        );
        assert!(g.passes.len() >= 2);
    }

    #[test]
    fn expand_dirty_respects_bounds() {
        let r = expand_dirty_rect((10, 10, 20, 20), 8, 100, 100);
        assert_eq!(r, (2, 2, 36, 36));
        let edge = expand_dirty_rect((0, 0, 5, 5), 8, 100, 100);
        assert_eq!(edge.0, 0);
        assert_eq!(edge.1, 0);
    }
}
