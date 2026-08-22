//! Preview qualities must actually be cheaper than Full, and Full must stay
//! exactly what the solver produces.
//!
//! Thermal Erosion defaults to the layered (bedrock + debris) solver, which was
//! the one sim that ignored the coarse-to-fine level schedule every other sim
//! honours. It therefore simulated the full grid at every quality, and a Draft
//! brush dab cost the same as a Full one - the progressive Draft -> Medium ->
//! Full ladder did nothing for the layer that dominates sculpt latency.
//!
//! Preview rungs now run the schedule. Full and Export deliberately do not, so
//! exported terrain and the golden stacks are unaffected: a preview is allowed
//! to approximate, an export is not.

use terra_core::eval::{EvalContext, PreviewQuality, StackEvaluator};
use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::layer::{
    BlendMode, FbmParams, FlatParams, Layer, LayerKind, LayerStack, ThermalErosionParams,
};

const RES: u32 = 256;

fn eroded_at(quality: PreviewQuality) -> Heightfield {
    let metrics = HeightfieldMetrics::new(RES, RES, 2048.0, 2048.0);
    let mut stack = LayerStack::new();
    stack.push(Layer::new(
        "Base",
        LayerKind::Flat(FlatParams { height: 40.0 }),
    ));
    let mut fbm = Layer::new("Fbm", LayerKind::Fbm(FbmParams::default()));
    fbm.common.blend = BlendMode::Add;
    stack.push(fbm);
    stack.push(Layer::new(
        "Thermal",
        LayerKind::ThermalErosion(ThermalErosionParams::default()),
    ));

    let mut eval = StackEvaluator::new();
    let mut ctx = EvalContext::new(metrics);
    ctx.quality = quality;
    eval.rebuild_all(&stack, &mut ctx).unwrap()
}

fn stats(a: &Heightfield, b: &Heightfield) -> (f32, f32) {
    let (w, h) = (a.metrics.width, a.metrics.height);
    let mut sum = 0.0f64;
    let mut max = 0.0f32;
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for j in 0..h {
        for i in 0..w {
            let d = (a.get(i, j) - b.get(i, j)).abs();
            sum += d as f64;
            max = max.max(d);
            lo = lo.min(a.get(i, j));
            hi = hi.max(a.get(i, j));
        }
    }
    let relief = (hi - lo).max(1e-3);
    ((sum / (w * h) as f64) as f32 / relief, max / relief)
}

/// Export must be byte-identical to Full: the shortcut is a preview affordance,
/// and an export that silently differed from what the viewport settled on would
/// be worse than a slow one.
#[test]
fn export_quality_matches_full_exactly() {
    let full = eroded_at(PreviewQuality::Full);
    let export = eroded_at(PreviewQuality::Export);
    for j in 0..RES {
        for i in 0..RES {
            assert_eq!(
                full.get(i, j),
                export.get(i, j),
                "Full and Export must run the identical solve at ({i}, {j})"
            );
        }
    }
}

/// Draft must take the schedule - if it produced Full's exact output it would
/// be doing Full's exact work, which is the regression this guards.
#[test]
fn draft_quality_takes_the_level_schedule() {
    let full = eroded_at(PreviewQuality::Full);
    let draft = eroded_at(PreviewQuality::Draft);
    let (mean, _) = stats(&full, &draft);
    assert!(
        mean > 1e-6,
        "Draft reproduced Full exactly, so it ran the full-resolution solve; \
         the layered thermal path is ignoring the level schedule again"
    );
}

/// ...but it must still be a recognisable preview of the same terrain, not a
/// different landscape the refine pass will visibly snap away from.
#[test]
fn draft_stays_a_faithful_preview_of_full() {
    let full = eroded_at(PreviewQuality::Full);
    let draft = eroded_at(PreviewQuality::Draft);
    let (mean, max) = stats(&full, &draft);
    assert!(
        mean < 0.05,
        "Draft deviates from Full by {:.1}% of relief on average; a preview \
         that far off will visibly snap when it refines",
        mean * 100.0
    );
    assert!(
        max < 0.35,
        "worst-case Draft deviation is {:.1}% of relief",
        max * 100.0
    );
}

/// Medium sits between the two - the ladder has to be monotonic or refining
/// through it is wasted work.
#[test]
fn medium_is_closer_to_full_than_draft() {
    let full = eroded_at(PreviewQuality::Full);
    let (draft_mean, _) = stats(&full, &eroded_at(PreviewQuality::Draft));
    let (medium_mean, _) = stats(&full, &eroded_at(PreviewQuality::Medium));
    assert!(
        medium_mean <= draft_mean,
        "Medium ({medium_mean}) must be at least as close to Full as Draft \
         ({draft_mean}); otherwise the refine ladder moves away from the answer"
    );
}
