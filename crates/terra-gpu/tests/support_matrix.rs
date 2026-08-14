use terra_core::layer::{
    BlendMode, EffectFilterKind, EffectFilterParams, FbmParams, FlatParams, FractalNoiseType,
    Layer, LayerKind, LayerStack, LayerTypeRegistry, NoiseParams,
};
use terra_gpu::{compile_gpu_graph, layer_gpu_supported, GpuKernel};

fn graph_for(layer: Layer) -> terra_gpu::GpuComputeGraph {
    let mut stack = LayerStack::new();
    stack.push(layer);
    compile_gpu_graph(&stack, &[])
}

#[test]
fn every_builtin_default_has_consistent_public_support_graph_and_kernel() {
    let registry = LayerTypeRegistry::builtin();
    for meta in registry.all() {
        let layer = registry.create(meta.type_id).expect("registered factory");
        let supported = layer_gpu_supported(&layer, &[]);
        let graph = graph_for(layer.clone());
        assert_eq!(graph.fully_gpu(), supported, "{}", meta.type_id);
        assert_eq!(
            graph.cpu_from,
            (!supported).then_some(0),
            "{}",
            meta.type_id
        );
        if supported {
            let [pass] = graph.passes.as_slice() else {
                panic!("{} must compile to exactly one pass", meta.type_id);
            };
            assert!(
                pass.kernel.matches_layer_kind(&layer.kind),
                "{} selected incompatible {:?}",
                meta.type_id,
                pass.kernel
            );
        }
    }
}

#[test]
fn every_effect_filter_variant_has_an_explicit_executable_plan() {
    for &kind in EffectFilterKind::ALL {
        let layer = Layer::new(
            kind.label(),
            LayerKind::EffectFilter(EffectFilterParams {
                kind,
                ..EffectFilterParams::default()
            }),
        );
        let graph = graph_for(layer);
        let supported = matches!(kind, EffectFilterKind::Smooth | EffectFilterKind::Inflate);
        assert_eq!(graph.fully_gpu(), supported, "{}", kind.label());
        if supported {
            assert_eq!(graph.passes[0].kernel, GpuKernel::EffectFilter);
        } else {
            assert_eq!(graph.cpu_from, Some(0));
        }
    }
}

#[test]
fn fractal_noise_variants_and_blend_modes_are_explicitly_classified() {
    for noise in [
        FractalNoiseType::Value,
        FractalNoiseType::Perlin,
        FractalNoiseType::OpenSimplex,
    ] {
        for make_kind in [LayerKind::Fbm, LayerKind::Ridged] {
            let layer = Layer::new(
                "fractal",
                make_kind(FbmParams {
                    noise,
                    ..FbmParams::default()
                }),
            );
            assert!(!layer_gpu_supported(&layer, &[]), "{noise:?}");
        }
    }

    for (blend, supported) in [
        (BlendMode::Normal, true),
        (BlendMode::Replace, true),
        (BlendMode::Interpolate, true),
        (BlendMode::Add, true),
        (BlendMode::Subtract, true),
        (BlendMode::Multiply, true),
        (BlendMode::Min, true),
        (BlendMode::Max, true),
        (BlendMode::Overlay, true),
        (BlendMode::HeightBlend, false),
        (BlendMode::SmoothMaximum, false),
        (BlendMode::SmoothMinimum, false),
        (BlendMode::SmoothUnion, false),
        (BlendMode::SmoothSubtraction, false),
    ] {
        let mut layer = Layer::new("blend", LayerKind::Flat(FlatParams { height: 2.0 }));
        layer.common.blend = blend;
        assert_eq!(layer_gpu_supported(&layer, &[]), supported, "{blend:?}");
    }
}

#[test]
fn first_unsupported_configuration_owns_cpu_from() {
    let high_seed = NoiseParams {
        seed: u64::from(u32::MAX) + 7,
        ..NoiseParams::default()
    };
    let mut stack = LayerStack::new();
    stack.push(Layer::new(
        "prefix",
        LayerKind::Flat(FlatParams { height: 4.0 }),
    ));
    stack.push(Layer::new(
        "truncated seed",
        LayerKind::NoisePerlin(high_seed),
    ));
    let mut suffix = Layer::new("suffix", LayerKind::Flat(FlatParams { height: 2.0 }));
    suffix.common.blend = BlendMode::Add;
    stack.push(suffix);

    let graph = compile_gpu_graph(&stack, &[]);
    assert_eq!(graph.cpu_from, Some(1));
    assert_eq!(graph.passes.len(), 1);
    assert_eq!(graph.passes[0].flat_index, 0);
}
