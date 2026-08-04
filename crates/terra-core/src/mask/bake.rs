//! Bake project mask assets into `MaskField`s for evaluation.

use super::{apply_mask_ops, MaskAsset, MaskField, MaskId, MaskSource};
use crate::analyze;
use crate::heightfield::{Heightfield, HeightfieldMetrics};
use std::collections::HashMap;

/// Bake all mask assets against a reference heightfield.
///
/// If `reference` metrics differ from `target`, heights are sampled by normalized UV.
pub fn bake_mask_assets(
    assets: &[MaskAsset],
    reference: &Heightfield,
    target: HeightfieldMetrics,
    aux: &HashMap<String, MaskField>,
) -> HashMap<MaskId, MaskField> {
    let hf = if reference.metrics.width == target.width && reference.metrics.height == target.height
    {
        reference.clone()
    } else {
        resample_height(reference, target)
    };

    let mut out = HashMap::new();
    for asset in assets {
        let mut field = bake_source(&asset.source, assets, &hf, aux);
        apply_mask_ops(&mut field, &asset.ops);
        out.insert(asset.id, field);
    }
    out
}

fn resample_height(src: &Heightfield, target: HeightfieldMetrics) -> Heightfield {
    let mut hf = Heightfield::zeros(target);
    for j in 0..target.height {
        for i in 0..target.width {
            let u = (i as f32 + 0.5) / target.width as f32;
            let v = (j as f32 + 0.5) / target.height as f32;
            let si = ((u * src.metrics.width as f32) as u32).min(src.metrics.width - 1);
            let sj = ((v * src.metrics.height as f32) as u32).min(src.metrics.height - 1);
            hf.set(i, j, src.get(si, sj));
        }
    }
    hf
}

fn bake_source(
    source: &MaskSource,
    assets: &[MaskAsset],
    hf: &Heightfield,
    aux: &HashMap<String, MaskField>,
) -> MaskField {
    match source {
        MaskSource::None | MaskSource::Named(_) => MaskField::ones(hf.metrics),
        MaskSource::Constant(v) => MaskField::filled(hf.metrics, *v),
        MaskSource::Height { min, max } => MaskField::from_height_range(hf, *min, *max),
        MaskSource::Slope { min_deg, max_deg } => {
            let s = analyze::slope_degrees(hf);
            let mut m = MaskField::zeros(hf.metrics);
            for j in 0..hf.metrics.height {
                for i in 0..hf.metrics.width {
                    let deg = s.get(i, j) * 90.0;
                    let t = ((deg - *min_deg) / (*max_deg - *min_deg).max(1e-3)).clamp(0.0, 1.0);
                    m.set(i, j, t);
                }
            }
            m
        }
        MaskSource::Aspect {
            center_deg,
            width_deg,
        } => analyze::aspect_mask(hf, *center_deg, *width_deg),
        MaskSource::Curvature { min, max } => {
            let c = analyze::curvature(hf);
            let mut m = c;
            for v in m.data_mut() {
                *v = ((*v - *min) / (*max - *min).max(1e-3)).clamp(0.0, 1.0);
            }
            m
        }
        MaskSource::Convexity => analyze::convexity(hf),
        MaskSource::Concavity => analyze::concavity(hf),
        MaskSource::AmbientOcclusion { radius, strength } => {
            analyze::ambient_occlusion(hf, *radius, *strength)
        }
        MaskSource::DistanceField { threshold } => {
            let mut seeds = MaskField::zeros(hf.metrics);
            for j in 0..hf.metrics.height {
                for i in 0..hf.metrics.width {
                    if hf.get(i, j) > *threshold {
                        seeds.set(i, j, 1.0);
                    }
                }
            }
            analyze::jump_flood_distance(&seeds)
        }
        MaskSource::Noise { seed, frequency } => {
            let mut m = MaskField::zeros(hf.metrics);
            for j in 0..hf.metrics.height {
                for i in 0..hf.metrics.width {
                    let x = hf.metrics.world_x(i) * *frequency;
                    let z = hf.metrics.world_z(j) * *frequency;
                    let n = crate::noise::value_noise2(x, z, *seed) * 0.5 + 0.5;
                    m.set(i, j, n.clamp(0.0, 1.0));
                }
            }
            m
        }
        MaskSource::Painted { mask_id } => {
            let Some(paint) = assets
                .iter()
                .find(|asset| asset.id == *mask_id)
                .and_then(|asset| asset.paint.as_ref())
            else {
                return MaskField::zeros(hf.metrics);
            };
            let mut field = MaskField::zeros(hf.metrics);
            for j in 0..hf.metrics.height {
                for i in 0..hf.metrics.width {
                    let u = (i as f32 + 0.5) / hf.metrics.width as f32;
                    let v = (j as f32 + 0.5) / hf.metrics.height as f32;
                    field.set(i, j, paint.sample_uv(u, v));
                }
            }
            field
        }
        MaskSource::FlowDirection => aux
            .get("flow_direction")
            .cloned()
            .unwrap_or_else(|| MaskField::ones(hf.metrics)),
        MaskSource::FlowAccumulation { min, max } => {
            let Some(acc) = aux.get("flow_accumulation") else {
                return MaskField::ones(hf.metrics);
            };
            let mut m = MaskField::zeros(hf.metrics);
            for j in 0..hf.metrics.height {
                for i in 0..hf.metrics.width {
                    let v = acc.get(i, j);
                    let t = ((v - *min) / (*max - *min).max(1e-6)).clamp(0.0, 1.0);
                    m.set(i, j, t);
                }
            }
            m
        }
        MaskSource::Wetness => aux
            .get("wetness")
            .cloned()
            .unwrap_or_else(|| MaskField::ones(hf.metrics)),
        MaskSource::Sediment => aux
            .get("sediment")
            .cloned()
            .unwrap_or_else(|| MaskField::ones(hf.metrics)),
        MaskSource::Erosion => aux
            .get("erosion")
            .cloned()
            .unwrap_or_else(|| MaskField::ones(hf.metrics)),
        MaskSource::Deposition => aux
            .get("deposition")
            .cloned()
            .unwrap_or_else(|| MaskField::ones(hf.metrics)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mask::{MaskAsset, MaskId, MaskSource, PaintBuffer};

    #[test]
    fn painted_mask_bakes_painted_values() {
        let metrics = HeightfieldMetrics::new(8, 8, 8.0, 8.0);
        let id = MaskId::new();
        let mut paint = PaintBuffer::new(8, 8);
        paint.stamp_circle(0.5, 0.5, 0.25, 1.0, false);
        let assets = vec![MaskAsset {
            id,
            name: "Paint".into(),
            source: MaskSource::Painted { mask_id: id },
            ops: Vec::new(),
            paint: Some(paint),
        }];
        let baked = bake_mask_assets(
            &assets,
            &Heightfield::zeros(metrics),
            metrics,
            &HashMap::new(),
        );
        let field = &baked[&id];
        assert!(field.data().iter().any(|&v| v > 0.0));
        assert!(field.data().iter().any(|&v| v < 1.0));
    }
}
