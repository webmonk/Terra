//! Archetype relief must stay proportionate to world size.
//!
//! The macro parameters (mountain amplitude, mesa height, noise frequency) were
//! authored as absolute metres against a roughly 10 km world. Nothing scaled
//! them, so relief came out near-constant regardless of world extent, and it
//! broke at both ends: an Alpine Range built at 2 km produced about 3030 m of
//! relief, a mountain half again taller than the world is wide, which renders
//! as a needle; the same archetype at 40 km came out effectively flat.
//!
//! `world_scale` in `world_archetype` now scales the macro parameters against a
//! 10 km reference: vertical terms multiply by the ratio, spatial frequencies
//! divide by it, so an archetype is the same landscape at any extent rather
//! than a fixed-height feature in a variable-size box.
//!
//! That fixes the large-world end outright. It does not fully fix the small-
//! world end, because below about 4 km the dominant relief no longer comes from
//! the macro layers at all - it comes from the shared process chain (landscape
//! evolution, stream power, debris, thermal), whose parameters are relative
//! rather than absolute metres and which therefore needs its own scaling rule.
//! See `relief_at_small_worlds_is_still_out_of_proportion`.

use std::collections::HashMap;
use terra_core::eval::{EvalContext, PreviewQuality, StackEvaluator};
use terra_core::mask::bake_mask_assets;
use terra_core::world_archetype::WorldTemplate;

/// Relief as a fraction of world extent, at Draft.
fn relief_ratio(template: WorldTemplate, world_m: f32) -> f32 {
    let doc = template.build(world_m, 192);
    let metrics = doc.metrics;
    let mut ctx = EvalContext::new(metrics);
    ctx.quality = PreviewQuality::Draft;
    ctx.mask_assets = doc.masks.clone();
    let seed = terra_core::Heightfield::zeros(metrics);
    ctx.masks = bake_mask_assets(&doc.masks, &seed, metrics, &HashMap::new());
    let mut eval = StackEvaluator::new();
    let h = eval
        .rebuild_all(&doc.stack, &mut ctx)
        .expect("archetype Draft eval");
    let (min_h, max_h) = h.min_max();
    // `build` clamps the requested extent, so measure against what was built.
    (max_h - min_h) / doc.metrics.world_size_x
}

fn shaping_templates() -> impl Iterator<Item = WorldTemplate> {
    // Blank has no baked evolution, so its relief is legitimately ~0.
    WorldTemplate::all()
        .iter()
        .copied()
        .filter(|t| !matches!(t, WorldTemplate::Blank))
}

/// At the extents the archetypes are actually usable on, relief must be a sane
/// fraction of world size. Before the scaling rule the large-world end was
/// degenerate: Badlands, Young/Old Mountains, Dune Field and Coastal all came
/// out at 0.00-0.01x at 40 km, which is a flat plane with a texture on it.
#[test]
fn relief_is_proportionate_at_large_worlds() {
    const LO: f32 = 0.015;
    const HI: f32 = 0.5;
    let mut failures = Vec::new();
    for template in shaping_templates() {
        // A dune field is legitimately near-flat: 30 m dunes over kilometres of
        // sand is the point of it, so only the upper bound is meaningful there.
        let lo = if matches!(template, WorldTemplate::DuneField) {
            0.0
        } else {
            LO
        };
        for world_m in [12_000.0f32, 40_000.0] {
            let r = relief_ratio(template, world_m);
            if !(lo..=HI).contains(&r) {
                failures.push(format!("{template:?} at {world_m:.0} m: relief/world = {r:.3}x"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "archetype relief is out of proportion to world size:\n  {}",
        failures.join("\n  ")
    );
}

/// Scale invariance across the large-world range: the same archetype at 12 km
/// and 40 km should be the same landscape at a different size, so the relief
/// ratio should barely move. This is what makes the fix a scaling rule rather
/// than a per-size fudge.
#[test]
fn relief_ratio_is_stable_across_large_worlds() {
    for template in shaping_templates() {
        let ratios: Vec<f32> = [12_000.0f32, 40_000.0]
            .iter()
            .map(|w| relief_ratio(template, *w))
            .collect();
        let lo = ratios.iter().cloned().fold(f32::INFINITY, f32::min);
        let hi = ratios.iter().cloned().fold(0.0f32, f32::max);
        assert!(
            hi <= lo * 3.0,
            "{template:?} relief ratio swings {lo:.3}x..{hi:.3}x between 12 km \
             and 40 km; the archetype should scale with extent, not stay fixed \
             in absolute metres"
        );
    }
}

/// Known-open: small worlds still produce disproportionate relief.
///
/// Scaling the macro parameters took Desert at 2 km from 1.27x to 0.66x and
/// Alpine from 1.44x to 1.18x, but did not get either into band. Below roughly
/// 4 km the macro layers are no longer what dominates: the shared process chain
/// contributes most of the relief, and its parameters are relative (`uplift`,
/// `erosion` in `[0, 1+]`) rather than absolute metres, so the same trick does
/// not apply to them. Fixing this means giving the evolution/erosion chain its
/// own extent-aware scaling, which changes generated terrain for every project
/// and wants a maintainer decision rather than a quiet change.
///
/// Ignored rather than deleted so the invariant stays written down and the
/// measurement is reproducible: `cargo test -p terra-core --test
/// archetype_world_scaling -- --ignored`.
#[test]
#[ignore = "known open: process-chain relief does not scale with world extent"]
fn relief_at_small_worlds_is_still_out_of_proportion() {
    const HI: f32 = 0.5;
    let mut failures = Vec::new();
    for template in shaping_templates() {
        for world_m in [2_048.0f32, 4_000.0] {
            let r = relief_ratio(template, world_m);
            if r > HI {
                failures.push(format!("{template:?} at {world_m:.0} m: relief/world = {r:.3}x"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "small worlds still get disproportionate relief:\n  {}",
        failures.join("\n  ")
    );
}
