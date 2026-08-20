//! EffectFilter -> GPU preview mode mapping (CPU remains export oracle).

use terra_core::layer::EffectFilterKind;

/// Map artist filter kinds onto `effect_filter.wgsl` mode indices.
pub fn effect_filter_mode(kind: EffectFilterKind) -> u32 {
    use EffectFilterKind::*;
    match kind {
        Smooth => 0,
        Distortion | Swirl => 1,
        SpikeRemoval => 2,
        Shore => 3,
        Denoise | Kuwahara => 4,
        Inflate => 5,
        Deflate => 6,
        Balloon => 7,
        TerraceSimple | TerraceIrregular | TerraceSteep => 8,
        Curve => 9,
        Cutoff | AddSet => 10,
        RockySharp | RockyWide | RockyLayers | CliffReinforce | RockyPlateaus | RockyCliffs
        | RockyHard | Rocky | Cliffs | Chipped | Rugged | Ridged | SmoothRidges | Canyon
        | AngleBreak | WindCarve | Squeeze => 11,
        NoiseBillow | NoiseGabor | NoisePerlin | NoiseValue | NoiseSimplex | NoiseWhite
        | NoisePhasor | NoiseVoronoi | DesignVoronoi | Hexagons | ScatterDetail | NoiseRidged
        | NoiseWave => 12,
        AngleBlur | DirectionalBlur => 13,
        ZeroEdge | BorderBlend | WashedOff => 14,
        FlattenFilter => 15,
        Strata => 16,
        Crater => 17,
        TalusFill | SedimentFillSoft | MudSettle | HydraulicSediment | SoftFlows | ThinFlows
        | RidgedFlows | WideFlows | SedimentFlows => 18,
        // Sharpen-style leftover / blocks
        Blocks => 23,
    }
}

/// Some kinds share the "flows" downhill smear kernel.
pub fn effect_filter_mode_override(kind: EffectFilterKind) -> Option<u32> {
    use EffectFilterKind::*;
    match kind {
        SoftFlows | ThinFlows | RidgedFlows | WideFlows | SedimentFlows | HydraulicSediment => {
            Some(21)
        }
        Swirl => Some(22),
        Chipped | RockySharp => Some(19),
        Kuwahara => Some(20),
        _ => None,
    }
}

pub fn resolve_effect_mode(kind: EffectFilterKind) -> u32 {
    effect_filter_mode_override(kind).unwrap_or_else(|| effect_filter_mode(kind))
}
