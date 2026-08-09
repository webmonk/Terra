//! Phase 4 HydraulicErosionCore family fixtures.

use terra_core::analyze::{
    apply_transport_model, generate_rainfall_field, HydraulicErosionCore, MassDiagnostics,
    RainMapSource,
};
use terra_core::generators::effect_filter;
use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::layer::{
    EffectFilterKind, EffectFilterParams, HydraulicErosionParams, TransportModel,
};
use terra_core::mask::MaskField;

fn hill(res: u32) -> Heightfield {
    let m = HeightfieldMetrics::new(res, res, res as f32 * 8.0, res as f32 * 8.0);
    let mut hf = Heightfield::zeros(m);
    for j in 0..res {
        for i in 0..res {
            let dx = i as f32 - res as f32 * 0.5;
            let dz = j as f32 - res as f32 * 0.5;
            hf.set(i, j, 120.0 - (dx * dx + dz * dz).sqrt() * 0.55);
        }
    }
    hf
}

#[test]
fn all_transport_models_change_height_and_stay_finite() {
    let hf = hill(32);
    let k = MaskField::filled(hf.metrics, 0.0);
    for model in [
        TransportModel::Hydraulic,
        TransportModel::SoftFlows,
        TransportModel::RidgedFlows,
        TransportModel::ThinFlows,
        TransportModel::WideFlows,
        TransportModel::SedimentFlows,
        TransportModel::HydraulicSediment,
    ] {
        let core = HydraulicErosionCore::with_model(model);
        let (result, outputs, diag, state) = core.simulate(&hf, &k, None, None, None, None);
        assert_eq!(diag.nan_or_inf_cells, 0, "{model:?} produced NaN/Inf");
        assert!(
            result.height.get(16, 16) != hf.get(16, 16)
                || result.erosion_raw.data().iter().any(|v| *v > 0.0),
            "{model:?} should erode or change height"
        );
        assert!(outputs.rainfall.is_some());
        assert!(state.water_depth.iter().all(|v| v.is_finite()));
    }
}

#[test]
fn ridged_protects_divides_without_raising_them() {
    let hf = hill(40);
    let core = HydraulicErosionCore::with_model(TransportModel::RidgedFlows);
    let out = core.apply_height(&hf, None);
    assert!(
        out.get(2, 2) <= hf.get(2, 2) + 0.75,
        "ridged flows may deposit at toes but must not fake-boost divides"
    );
    let max_dh = hf
        .to_dense()
        .iter()
        .zip(out.to_dense().iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(max_dh > 0.1, "ridged flows max_dh={max_dh}");
}

#[test]
fn layered_sediment_softer_than_bedrock() {
    let hf = hill(28);
    let mut p = HydraulicErosionParams::default();
    p.iterations = 24;
    p = apply_transport_model(&p, TransportModel::HydraulicSediment);
    assert!(p.layered_materials);
    assert!(p.sediment_hardness < p.bedrock_hardness);
    let core = HydraulicErosionCore::new(p);
    let k = MaskField::filled(hf.metrics, 0.8);
    let (_, outputs, diag, _) = core.simulate(&hf, &k, None, None, None, None);
    assert_eq!(diag.nan_or_inf_cells, 0);
    assert!(outputs.bedrock.is_some());
    assert!(outputs.loose_sediment.is_some());
}

#[test]
fn rain_map_sources_build_fields() {
    let m = HeightfieldMetrics::new(24, 24, 240.0, 240.0);
    let c = generate_rainfall_field(m, &RainMapSource::Constant(0.5), None, None);
    assert!(c.data().iter().all(|v| (*v - 0.5).abs() < 1e-5));
    let n = generate_rainfall_field(
        m,
        &RainMapSource::Noise {
            seed: 3,
            frequency: 0.03,
            scale: 1.0,
            offset: 0.1,
        },
        None,
        None,
    );
    let min = n.data().iter().cloned().fold(f32::INFINITY, f32::min);
    let max = n.data().iter().cloned().fold(0.0f32, f32::max);
    assert!(max > min);
}

#[test]
fn soft_flows_effect_filter_uses_core() {
    let hf = hill(36);
    let p = EffectFilterParams {
        kind: EffectFilterKind::SoftFlows,
        strength: 1.0,
        amount: 8.0,
        flow_threshold: 0.05,
        ..EffectFilterParams::default()
    };
    let out = effect_filter(&hf, &p);
    let max_dh = hf
        .to_dense()
        .iter()
        .zip(out.to_dense().iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(max_dh > 0.08, "soft flows effect filter max_dh={max_dh}");
    assert!(out.get(2, 2) <= hf.get(2, 2) + 0.05);
}

#[test]
fn mass_diagnostics_on_hydraulic_default() {
    let hf = hill(32);
    let core = HydraulicErosionCore::default();
    let k = MaskField::filled(hf.metrics, 0.0);
    let (result, _, diag, _) = core.simulate(&hf, &k, None, None, None, None);
    let d2 = MassDiagnostics::from_result(&hf, &result);
    assert_eq!(diag.nan_or_inf_cells, d2.nan_or_inf_cells);
}
