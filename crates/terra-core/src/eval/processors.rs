use super::{EvalContext, EvalError};
use crate::analyze;
use crate::generators;
use crate::heightfield::Heightfield;
use crate::hydro;
use crate::layer::*;
use crate::surface;

pub struct ProcessorRegistry {
    // Kind handled via match on LayerKind for serde-friendly built-ins.
}

impl ProcessorRegistry {
    pub fn builtin() -> Self {
        Self {}
    }

    pub fn evaluate(
        &self,
        ctx: &mut EvalContext,
        input: &Heightfield,
        layer: &Layer,
    ) -> Result<Heightfield, EvalError> {
        match &layer.kind {
            LayerKind::SculptBase(p) => Ok(generators::sculpt_base(ctx.metrics, p)),
            LayerKind::Flat(p) => Ok(generators::flat(ctx.metrics, p.height)),
            LayerKind::Ramp(p) => Ok(generators::ramp(ctx.metrics, p)),
            LayerKind::NoiseValue(p) => Ok(generators::noise_field(
                ctx.metrics,
                p,
                FractalNoiseType::Value,
            )),
            LayerKind::NoisePerlin(p) => Ok(generators::noise_field(
                ctx.metrics,
                p,
                FractalNoiseType::Perlin,
            )),
            LayerKind::NoiseOpenSimplex(p) => Ok(generators::noise_field(
                ctx.metrics,
                p,
                FractalNoiseType::OpenSimplex,
            )),
            LayerKind::NoiseWorley(p) => Ok(generators::worley_field(ctx.metrics, p)),
            LayerKind::Fbm(p) => Ok(generators::fbm_field(ctx.metrics, p)),
            LayerKind::Ridged(p) => Ok(generators::ridged_field(ctx.metrics, p)),
            LayerKind::DomainWarp(p) => Ok(generators::domain_warp_field(ctx.metrics, p)),
            LayerKind::Terrace(p) => Ok(generators::terrace(input, p)),
            LayerKind::Plateau(p) => Ok(generators::plateau(input, p)),
            LayerKind::Mountains(p) => Ok(generators::mountains(ctx.metrics, p)),
            LayerKind::Dunes(p) => Ok(generators::dunes(ctx.metrics, p)),
            LayerKind::Canyons(p) => Ok(generators::canyons(ctx.metrics, p)),
            LayerKind::VoronoiRegions(p) => Ok(generators::voronoi_regions(ctx.metrics, p)),
            LayerKind::ImportHeightmap(p) => generators::import_heightmap(ctx.metrics, p),
            LayerKind::ThermalErosion(p) => {
                let levels = match ctx.quality {
                    crate::eval::PreviewQuality::Draft => {
                        analyze::draft_sim_levels(ctx.metrics.width)
                    }
                    _ => analyze::default_sim_levels(ctx.metrics.width),
                };
                let (hf, erosion, deposit) = analyze::thermal_erode_leveled(input, p, &levels);
                ctx.aux.insert("erosion".into(), erosion);
                ctx.aux.insert("deposition".into(), deposit);
                Ok(hf)
            }
            LayerKind::HydraulicErosion(p) => {
                let levels = match ctx.quality {
                    crate::eval::PreviewQuality::Draft => {
                        analyze::draft_sim_levels(ctx.metrics.width)
                    }
                    _ => analyze::default_sim_levels(ctx.metrics.width),
                };
                let result = analyze::hydraulic_erode_leveled(input, p, &levels);
                ctx.aux.insert("wetness".into(), result.wetness);
                ctx.aux.insert("sediment".into(), result.sediment);
                ctx.aux.insert("erosion".into(), result.erosion);
                ctx.aux.insert("deposition".into(), result.deposition);
                Ok(result.height)
            }
            LayerKind::RiverCarve(p) => {
                let (hf, flow, acc) = hydro::carve_rivers(input, p);
                ctx.aux.insert("flow_direction".into(), flow);
                ctx.aux.insert("flow_accumulation".into(), acc);
                Ok(hf)
            }
            LayerKind::Blur(p) => Ok(generators::blur(input, p)),
            LayerKind::Coastal(p) => Ok(generators::coastal(input, p)),
            LayerKind::Materials(p) => {
                let weights = surface::material_weights(input, p, ctx.aux.get("wetness"));
                ctx.aux.insert("materials".into(), weights);
                Ok(input.clone())
            }
            LayerKind::Biomes(p) => {
                let biomes = surface::biome_mask(input, p, ctx.aux.get("wetness"));
                ctx.aux.insert("biomes".into(), biomes);
                Ok(input.clone())
            }
            LayerKind::Vegetation(p) => {
                let density = surface::vegetation_density(input, p, ctx.aux.get("biomes"));
                ctx.aux.insert("vegetation".into(), density);
                Ok(input.clone())
            }
        }
    }
}
