//! Compile a layer stack into an ordered GPU compute pass list.
//!
//! Artist UI remains layer-based; this IR is the internal execution plan for
//! `GpuTerrainEngine` (deps, dirty policy, fusion hooks).

use terra_core::layer::{FractalNoiseType, Layer, LayerId, LayerKind, LayerStack};
use terra_core::mask::{MaskAsset, MaskCombine, MaskSource};
use terra_core::tiling::DirtyClass;

/// Kind of GPU work a compiled pass performs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuPassKind {
    Generator,
    Filter,
    Simulation,
    MaskBake,
    Blend,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GpuLayerPlan {
    kind: GpuPassKind,
    dirty_policy: GpuDirtyPolicy,
    halo_texels: u32,
}

fn gpu_mask_supported(layer: &Layer, assets: &[MaskAsset]) -> bool {
    if !layer.common.masks.nodes.is_empty() {
        return false;
    }
    let [entry] = layer.common.masks.entries.as_slice() else {
        return layer.common.masks.entries.is_empty();
    };
    if entry.combine != MaskCombine::Multiply {
        return false;
    }
    assets
        .iter()
        .find(|asset| asset.id == entry.mask.id)
        .is_some_and(|asset| {
            asset.ops.is_empty()
                && matches!(
                    asset.source,
                    MaskSource::Constant(_) | MaskSource::Height { .. } | MaskSource::Slope { .. }
                )
        })
}

fn gpu_pass_for_layer(layer: &Layer, mask_assets: &[MaskAsset]) -> Option<GpuLayerPlan> {
    use LayerKind::*;
    if !gpu_mask_supported(layer, mask_assets) {
        return None;
    }
    let (kind, dirty_policy, halo_texels) = match &layer.kind {
        Flat(_) | Ramp(_) | NoiseValue(_) | NoisePerlin(_) | Mountains(_) | Dunes(_)
        | Canyons(_) | DomainWarp(_) | SculptBase(_) | Mesa(_) | Volcano(_) | Island(_)
        | Plateau(_) | Uplift(_) => (GpuPassKind::Generator, GpuDirtyPolicy::Local, 2),
        Fbm(p) | Ridged(p)
            if matches!(p.noise, FractalNoiseType::Value | FractalNoiseType::Perlin) =>
        {
            (GpuPassKind::Generator, GpuDirtyPolicy::Local, 2)
        }
        Fbm(_) | Ridged(_) => return None,
        Blur(p) => (
            GpuPassKind::Filter,
            GpuDirtyPolicy::Local,
            p.radius.max(1).min(16),
        ),
        EffectFilter(p) => (
            GpuPassKind::Filter,
            GpuDirtyPolicy::Local,
            p.radius.max(1).min(16),
        ),
        Terrace(_) => (GpuPassKind::Filter, GpuDirtyPolicy::Local, 4),
        ThermalErosion(_) | HydraulicErosion(_) | RiverCarve(_) => {
            (GpuPassKind::Simulation, GpuDirtyPolicy::FullField, 0)
        }
        // These CPU operations either modify height without a GPU kernel or publish
        // observable auxiliary fields that the GPU preview cannot currently produce.
        Coastal(_) | Materials(_) | Biomes(_) | Vegetation(_) => return None,
        _ => return None,
    };
    Some(GpuLayerPlan {
        kind,
        dirty_policy,
        halo_texels,
    })
}

/// Whether this complete layer configuration can run on the GPU preview path.
pub fn layer_gpu_supported(layer: &Layer, mask_assets: &[MaskAsset]) -> bool {
    gpu_pass_for_layer(layer, mask_assets).is_some()
}

/// Compile the preview stack into a GPU pass list.
///
/// Stops recording passes at the first unsupported layer (sets `cpu_from`).
pub fn compile_gpu_graph(stack: &LayerStack, mask_assets: &[MaskAsset]) -> GpuComputeGraph {
    let layers: Vec<&Layer> = stack.flatten_layers();
    let mut graph = GpuComputeGraph::default();
    for (flat_index, layer) in layers.iter().enumerate() {
        if !layer.common.enabled {
            continue;
        }
        let Some(plan) = gpu_pass_for_layer(layer, mask_assets) else {
            graph.cpu_from = Some(flat_index);
            break;
        };
        graph.passes.push(GpuPass {
            layer_id: layer.id(),
            kind: plan.kind,
            dirty_policy: plan.dirty_policy,
            flat_index,
            halo_texels: plan.halo_texels,
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
    use std::collections::HashMap;
    use terra_core::layer::{
        BiomesParams, CoastalParams, EffectFilterKind, EffectFilterParams, FbmParams, FlatParams,
        Layer, LayerKind, LayerStack, LayerTypeRegistry, MaterialsParams, NoiseParams,
        VegetationParams,
    };
    use terra_core::mask::{
        bake_distribution, bake_mask_assets, DistributionEntry, MaskId, MaskOp, MaskRef,
    };

    fn single_layer_graph(layer: Layer) -> GpuComputeGraph {
        let mut stack = LayerStack::new();
        stack.push(layer);
        compile_gpu_graph(&stack, &[])
    }

    fn masked_flat(asset: &MaskAsset) -> Layer {
        let mut layer = Layer::new("masked", LayerKind::Flat(FlatParams::default()));
        layer.common.masks.push(MaskRef::new(asset.id));
        layer
    }

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
        let g = compile_gpu_graph(&stack, &[]);
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

    /// Revert check for #48: support depends on the authored fractal noise family.
    #[test]
    fn fractal_noise_support_rejects_open_simplex_without_substitution() {
        for make_kind in [
            |params| LayerKind::Fbm(params),
            |params| LayerKind::Ridged(params),
        ] {
            for (noise, supported) in [
                (FractalNoiseType::Value, true),
                (FractalNoiseType::Perlin, true),
                (FractalNoiseType::OpenSimplex, false),
            ] {
                let layer = Layer::new(
                    "fractal",
                    make_kind(FbmParams {
                        noise,
                        ..FbmParams::default()
                    }),
                );
                assert_eq!(layer_gpu_supported(&layer, &[]), supported);
                let graph = single_layer_graph(layer);
                assert_eq!(graph.fully_gpu(), supported);
                assert_eq!(graph.cpu_from, (!supported).then_some(0));
            }
        }
    }

    /// Revert check for #48: semantic CPU operations cannot compile as GPU no-ops.
    #[test]
    fn semantic_noops_and_coastal_are_cpu_boundaries() {
        let layers = [
            Layer::new("coastal", LayerKind::Coastal(CoastalParams::default())),
            Layer::new(
                "materials",
                LayerKind::Materials(MaterialsParams::default()),
            ),
            Layer::new("biomes", LayerKind::Biomes(BiomesParams::default())),
            Layer::new(
                "vegetation",
                LayerKind::Vegetation(VegetationParams::default()),
            ),
        ];
        for layer in layers {
            assert!(!layer_gpu_supported(&layer, &[]));
            let graph = single_layer_graph(layer);
            assert_eq!(graph.cpu_from, Some(0));
            assert!(graph.passes.is_empty());
        }
    }

    #[test]
    fn public_support_query_and_graph_agree_for_every_builtin_default() {
        let registry = LayerTypeRegistry::builtin();
        for meta in registry.all() {
            let layer = registry.create(meta.type_id).expect("registered factory");
            let supported = layer_gpu_supported(&layer, &[]);
            let graph = single_layer_graph(layer);
            assert_eq!(
                graph.fully_gpu(),
                supported,
                "support disagreement for {}",
                meta.type_id
            );
            assert_eq!(graph.cpu_from, (!supported).then_some(0));
        }
    }

    #[test]
    fn graph_reports_first_unsupported_flattened_index() {
        let mut stack = LayerStack::new();
        stack.push(Layer::new(
            "noise",
            LayerKind::NoisePerlin(NoiseParams::default()),
        ));
        stack.push(Layer::new(
            "materials",
            LayerKind::Materials(MaterialsParams::default()),
        ));
        stack.push(Layer::new("flat", LayerKind::Flat(FlatParams::default())));

        let graph = compile_gpu_graph(&stack, &[]);
        assert_eq!(graph.cpu_from, Some(1));
        assert_eq!(graph.passes.len(), 1);
        assert_eq!(graph.passes[0].flat_index, 0);
    }

    #[test]
    fn simple_single_entry_masks_are_the_only_gpu_supported_contract() {
        for source in [
            MaskSource::Constant(0.5),
            MaskSource::Height {
                min: 10.0,
                max: 20.0,
            },
            MaskSource::Slope {
                min_deg: 5.0,
                max_deg: 35.0,
            },
        ] {
            let asset = MaskAsset::new(MaskId::new(), "supported", source);
            let layer = masked_flat(&asset);
            assert!(layer_gpu_supported(&layer, std::slice::from_ref(&asset)));
        }

        let empty = Layer::new("empty", LayerKind::Flat(FlatParams::default()));
        assert!(layer_gpu_supported(&empty, &[]));
    }

    #[test]
    fn complex_or_unproven_masks_start_cpu_fallback_at_the_owner() {
        let mut cases = Vec::new();

        let missing = MaskAsset::new(MaskId::new(), "missing", MaskSource::Constant(0.5));
        cases.push((masked_flat(&missing), Vec::new(), "missing asset"));

        for source in [
            MaskSource::Curvature {
                min: -1.0,
                max: 1.0,
            },
            MaskSource::Noise {
                seed: 0x1_0000_0001,
                frequency: 0.05,
            },
        ] {
            let asset = MaskAsset::new(MaskId::new(), "unproven", source);
            cases.push((masked_flat(&asset), vec![asset], "unproven source"));
        }

        let mut operated = MaskAsset::new(MaskId::new(), "operated", MaskSource::Constant(0.2));
        operated.ops.push(MaskOp::Invert);
        cases.push((masked_flat(&operated), vec![operated], "asset operation"));

        let combined = MaskAsset::new(MaskId::new(), "combined", MaskSource::Constant(0.5));
        let mut non_multiply = masked_flat(&combined);
        non_multiply.common.masks.entries[0].combine = MaskCombine::Subtract;
        cases.push((non_multiply, vec![combined], "non-Multiply combine"));

        for (layer, assets, reason) in cases {
            assert!(!layer_gpu_supported(&layer, &assets), "{reason}");
            let mut stack = LayerStack::new();
            stack.push(layer);
            let graph = compile_gpu_graph(&stack, &assets);
            assert_eq!(graph.cpu_from, Some(0), "{reason}");
            assert!(graph.passes.is_empty(), "{reason}");
        }
    }

    #[test]
    fn ordered_distribution_fixture_is_non_commutative_and_cpu_bound() {
        let metrics = terra_core::heightfield::HeightfieldMetrics::new(2, 2, 2.0, 2.0);
        let first = MaskAsset::new(MaskId::new(), "first", MaskSource::Constant(0.8));
        let second = MaskAsset::new(MaskId::new(), "second", MaskSource::Constant(0.25));
        let assets = vec![first.clone(), second.clone()];
        let baked = bake_mask_assets(
            &assets,
            &terra_core::heightfield::Heightfield::zeros(metrics),
            metrics,
            &HashMap::new(),
        );

        let mut layer = masked_flat(&first);
        layer.common.masks.entries.push(DistributionEntry {
            mask: MaskRef::new(second.id),
            combine: MaskCombine::Subtract,
        });
        let oracle = bake_distribution(&layer.common.masks, &baked, metrics);
        assert!((oracle.get(0, 0) - 0.55).abs() < 1.0e-6);
        assert!(!layer_gpu_supported(&layer, &assets));

        let mut stack = LayerStack::new();
        stack.push(layer);
        assert_eq!(compile_gpu_graph(&stack, &assets).cpu_from, Some(0));
    }

    #[test]
    fn asset_operation_fixture_changes_the_cpu_mask_and_requires_fallback() {
        let metrics = terra_core::heightfield::HeightfieldMetrics::new(2, 2, 2.0, 2.0);
        let mut asset = MaskAsset::new(MaskId::new(), "invert", MaskSource::Constant(0.2));
        asset.ops.push(MaskOp::Invert);
        let baked = bake_mask_assets(
            std::slice::from_ref(&asset),
            &terra_core::heightfield::Heightfield::zeros(metrics),
            metrics,
            &HashMap::new(),
        );
        let layer = masked_flat(&asset);
        let oracle = bake_distribution(&layer.common.masks, &baked, metrics);
        assert!((oracle.get(0, 0) - 0.8).abs() < 1.0e-6);
        assert!(!layer_gpu_supported(&layer, std::slice::from_ref(&asset)));
    }
}
