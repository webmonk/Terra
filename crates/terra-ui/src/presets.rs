use terra_core::layer::*;

#[derive(Debug, Clone)]
pub struct LayerPreset {
    pub name: String,
    pub description: String,
    pub layers: Vec<(String, LayerKind)>,
}

/// A non-destructive preset for the currently selected layer.
#[derive(Debug, Clone)]
pub struct ContextualPreset {
    pub name: &'static str,
    pub description: &'static str,
    pub kind: LayerKind,
}

/// Returns artist-facing variations for a layer kind.
///
/// Each result starts with `kind` and only adjusts its parameters, so applying
/// it through `PanelAction::SetKind` preserves the layer's common properties.
pub fn contextual_presets(kind: &LayerKind) -> Vec<ContextualPreset> {
    let preset = |name, description, modify: fn(&mut LayerKind)| {
        let mut variant = kind.clone();
        modify(&mut variant);
        ContextualPreset {
            name,
            description,
            kind: variant,
        }
    };

    match kind {
        LayerKind::Mountains(_) => vec![
            preset("Alpine", "Tall, crisp mountain ranges.", |k| {
                if let LayerKind::Mountains(p) = k {
                    p.base.amplitude = 650.0;
                    p.base.frequency = 0.0012;
                    p.base.octaves = 7;
                    p.ridge_sharpness = 2.8;
                    p.range_width = 0.28;
                }
            }),
            preset("Volcanic", "Broad cones with softer ridges.", |k| {
                if let LayerKind::Mountains(p) = k {
                    p.base.amplitude = 420.0;
                    p.base.frequency = 0.0022;
                    p.base.octaves = 4;
                    p.ridge_sharpness = 1.1;
                    p.range_width = 0.5;
                }
            }),
            preset("Rounded", "Weathered, rolling highlands.", |k| {
                if let LayerKind::Mountains(p) = k {
                    p.base.amplitude = 280.0;
                    p.base.frequency = 0.0018;
                    p.base.octaves = 4;
                    p.ridge_sharpness = 0.8;
                    p.range_width = 0.6;
                }
            }),
            preset("Jagged", "Narrow, dramatic peaks.", |k| {
                if let LayerKind::Mountains(p) = k {
                    p.base.amplitude = 800.0;
                    p.base.frequency = 0.0025;
                    p.base.octaves = 9;
                    p.ridge_sharpness = 3.6;
                    p.range_width = 0.18;
                }
            }),
        ],
        LayerKind::HydraulicErosion(_) => vec![
            preset(
                "Light Weathering",
                "A subtle pass over existing terrain.",
                |k| {
                    if let LayerKind::HydraulicErosion(p) = k {
                        p.iterations = 20;
                        p.rainfall = 0.01;
                        p.erosion = 0.12;
                        p.deposition = 0.15;
                    }
                },
            ),
            preset(
                "Young Terrain",
                "Sharp channels and limited sediment.",
                |k| {
                    if let LayerKind::HydraulicErosion(p) = k {
                        p.iterations = 45;
                        p.rainfall = 0.035;
                        p.erosion = 0.45;
                        p.deposition = 0.12;
                    }
                },
            ),
            preset(
                "Mature Valleys",
                "Deep drainage with settled basins.",
                |k| {
                    if let LayerKind::HydraulicErosion(p) = k {
                        p.iterations = 120;
                        p.rainfall = 0.06;
                        p.erosion = 0.5;
                        p.deposition = 0.5;
                        p.capacity = 0.22;
                    }
                },
            ),
        ],
        LayerKind::Dunes(_) => vec![
            preset("Linear", "Long parallel dune ridges.", |k| {
                if let LayerKind::Dunes(p) = k {
                    p.base.frequency = 0.002;
                    p.base.amplitude = 32.0;
                    p.wave_frequency = 0.018;
                    p.asymmetry = 0.55;
                }
            }),
            preset("Crescent", "Compact, wind-shaped barchans.", |k| {
                if let LayerKind::Dunes(p) = k {
                    p.base.frequency = 0.005;
                    p.base.amplitude = 45.0;
                    p.wave_frequency = 0.012;
                    p.asymmetry = 0.78;
                }
            }),
            preset("Star", "High dunes from variable winds.", |k| {
                if let LayerKind::Dunes(p) = k {
                    p.base.frequency = 0.0035;
                    p.base.amplitude = 75.0;
                    p.base.octaves = 5;
                    p.wave_frequency = 0.008;
                    p.asymmetry = 0.45;
                }
            }),
            preset("Coastal", "Low, gentle beach dunes.", |k| {
                if let LayerKind::Dunes(p) = k {
                    p.base.frequency = 0.006;
                    p.base.amplitude = 18.0;
                    p.wave_frequency = 0.02;
                    p.asymmetry = 0.5;
                }
            }),
            preset("Wind Swept", "Sparse ridges under strong wind.", |k| {
                if let LayerKind::Dunes(p) = k {
                    p.base.frequency = 0.0015;
                    p.base.amplitude = 28.0;
                    p.wave_frequency = 0.025;
                    p.asymmetry = 0.9;
                }
            }),
        ],
        LayerKind::Vegetation(_) => vec![
            preset(
                "Sparse Forest",
                "Widely spaced trees on gentle slopes.",
                |k| {
                    if let LayerKind::Vegetation(p) = k {
                        p.density = 0.12;
                        p.min_distance = 8.0;
                        p.max_slope_deg = 25.0;
                    }
                },
            ),
            preset(
                "Dense Forest",
                "Thick coverage across rolling terrain.",
                |k| {
                    if let LayerKind::Vegetation(p) = k {
                        p.density = 0.72;
                        p.min_distance = 2.5;
                        p.max_slope_deg = 35.0;
                    }
                },
            ),
            preset("Alpine Scrub", "Sparse growth above steep terrain.", |k| {
                if let LayerKind::Vegetation(p) = k {
                    p.density = 0.24;
                    p.min_distance = 5.0;
                    p.min_slope_deg = 8.0;
                    p.max_slope_deg = 48.0;
                }
            }),
        ],
        _ => Vec::new(),
    }
}

pub fn builtin_presets() -> Vec<LayerPreset> {
    vec![
        LayerPreset {
            name: "Rolling Hills".into(),
            description: "Soft fBm hills over a flat base".into(),
            layers: vec![
                (
                    "Base".into(),
                    LayerKind::SculptBase(SculptParams::filled(512, 30.0)),
                ),
                (
                    "Hills".into(),
                    LayerKind::Fbm(FbmParams {
                        base: NoiseParams {
                            seed: 11,
                            frequency: 0.0012,
                            amplitude: 90.0,
                            octaves: 5,
                            ..NoiseParams::default()
                        },
                        noise: FractalNoiseType::Perlin,
                    }),
                ),
            ],
        },
        LayerPreset {
            name: "Alpine Range".into(),
            description: "Ridged mountains + thermal + rivers".into(),
            layers: vec![
                (
                    "Base".into(),
                    LayerKind::SculptBase(SculptParams::filled(512, 40.0)),
                ),
                (
                    "Mountains".into(),
                    LayerKind::Mountains(MountainParams::default()),
                ),
                (
                    "Thermal".into(),
                    LayerKind::ThermalErosion(ThermalErosionParams {
                        iterations: 25,
                        ..ThermalErosionParams::default()
                    }),
                ),
                (
                    "Rivers".into(),
                    LayerKind::RiverCarve(RiverCarveParams::default()),
                ),
            ],
        },
        LayerPreset {
            name: "Desert Dunes".into(),
            description: "Warped dunes with coastal cutoff".into(),
            layers: vec![
                (
                    "Base".into(),
                    LayerKind::SculptBase(SculptParams::filled(512, 10.0)),
                ),
                ("Dunes".into(), LayerKind::Dunes(DuneParams::default())),
                (
                    "Warp".into(),
                    LayerKind::DomainWarp(DomainWarpParams::default()),
                ),
                ("Coast".into(), LayerKind::Coastal(CoastalParams::default())),
            ],
        },
        LayerPreset {
            name: "Eroded Highlands".into(),
            description: "Layered fBm highlands weathered by thermal and hydraulic erosion".into(),
            layers: vec![
                (
                    "Base".into(),
                    LayerKind::SculptBase(SculptParams::filled(512, 120.0)),
                ),
                (
                    "Highlands".into(),
                    LayerKind::Fbm(FbmParams {
                        base: NoiseParams {
                            seed: 73,
                            frequency: 0.0018,
                            amplitude: 280.0,
                            octaves: 7,
                            ..NoiseParams::default()
                        },
                        noise: FractalNoiseType::Perlin,
                    }),
                ),
                (
                    "Thermal Weathering".into(),
                    LayerKind::ThermalErosion(ThermalErosionParams {
                        iterations: 45,
                        strength: 0.65,
                        ..ThermalErosionParams::default()
                    }),
                ),
                (
                    "Hydraulic Erosion".into(),
                    LayerKind::HydraulicErosion(HydraulicErosionParams {
                        iterations: 80,
                        ..HydraulicErosionParams::default()
                    }),
                ),
                (
                    "Materials".into(),
                    LayerKind::Materials(MaterialsParams::default()),
                ),
            ],
        },
        LayerPreset {
            name: "River Valley".into(),
            description: "Mountain-fed valley with river carving, coast, and material rules".into(),
            layers: vec![
                (
                    "Base".into(),
                    LayerKind::SculptBase(SculptParams::filled(512, 45.0)),
                ),
                (
                    "Valley Mountains".into(),
                    LayerKind::Mountains(MountainParams {
                        range_width: 0.42,
                        ..MountainParams::default()
                    }),
                ),
                (
                    "Valley Detail".into(),
                    LayerKind::Fbm(FbmParams {
                        base: NoiseParams {
                            seed: 119,
                            frequency: 0.003,
                            amplitude: 45.0,
                            octaves: 4,
                            ..NoiseParams::default()
                        },
                        noise: FractalNoiseType::Perlin,
                    }),
                ),
                (
                    "River Carve".into(),
                    LayerKind::RiverCarve(RiverCarveParams {
                        accumulation_threshold: 35.0,
                        depth: 32.0,
                        width: 6.0,
                        ..RiverCarveParams::default()
                    }),
                ),
                (
                    "Coastal Edge".into(),
                    LayerKind::Coastal(CoastalParams {
                        sea_level: 30.0,
                        ..CoastalParams::default()
                    }),
                ),
                (
                    "Materials".into(),
                    LayerKind::Materials(MaterialsParams::default()),
                ),
            ],
        },
    ]
}

pub fn layers_from_preset(name: &str) -> Option<Vec<Layer>> {
    builtin_presets()
        .into_iter()
        .find(|p| p.name == name)
        .map(|p| {
            // Layer::new applies World Creator defaults (generators → Add).
            p.layers
                .into_iter()
                .map(|(n, k)| Layer::new(n, k))
                .collect()
        })
}
