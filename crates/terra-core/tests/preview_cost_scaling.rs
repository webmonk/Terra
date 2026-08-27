//! How CPU stack eval cost scales with preview resolution.
//!
//! Context: the interactive quality ladder is Draft 512, Medium 1024, Full
//! `preview_resolution`, where `preview_resolution` is roughly world metres
//! capped at `INTERACTIVE_PREVIEW_CAP` (8192). Any world wider than about 2 km
//! therefore asks Full to evaluate the whole stack at 8192 squared, which is
//! 67 million samples.
//!
//! Measured on a release build, Alpine at a 12.6 km world:
//!
//! ```text
//! res=  256  samples=     65536  Full eval =    2383 ms
//! res=  512  samples=    262144  Full eval =   10179 ms
//! res= 1024  samples=   1048576  Full eval =   45819 ms
//! res= 2048  samples=   4194304  Full eval =  200916 ms
//! ```
//!
//! Cost rises about 4.4x per 4x samples, so it is near-linear in sample count.
//! Extrapolating the same curve to 8192 squared puts one Full preview at
//! roughly an hour - and that is the release figure; the editor runs the dev
//! profile, which is slower again.
//!
//! That is why progressive refinement never appears to converge on a large
//! world: Full is not slow, it is unreachable, so the preview sits on the rung
//! below it indefinitely. Capping the interactive Full rung (and leaving 8192
//! to Export, which is a deliberate one-off the user waits for) is the obvious
//! lever, but it changes what Full means for every project, so it wants a
//! maintainer decision rather than a quiet change.
//!
//! Ignored because it takes several minutes. Run it with:
//! `cargo test -p terra-core --test preview_cost_scaling --release -- --ignored --nocapture`

use std::collections::HashMap;
use std::time::Instant;
use terra_core::eval::{EvalContext, PreviewQuality, StackEvaluator};
use terra_core::mask::bake_mask_assets;
use terra_core::world_archetype::WorldTemplate;

#[test]
#[ignore = "several minutes; a measurement, not a gate"]
fn preview_cost_scaling() {
    for res in [256u32, 512, 1024, 2048] {
        let doc = WorldTemplate::Alpine.build(12_600.0, res);
        let metrics = doc.metrics;
        let mut ctx = EvalContext::new(metrics);
        ctx.quality = PreviewQuality::Full;
        ctx.mask_assets = doc.masks.clone();
        let seed = terra_core::Heightfield::zeros(metrics);
        ctx.masks = bake_mask_assets(&doc.masks, &seed, metrics, &HashMap::new());
        let mut eval = StackEvaluator::new();
        let t0 = Instant::now();
        let _ = eval.rebuild_all(&doc.stack, &mut ctx).expect("eval");
        let ms = t0.elapsed().as_millis();
        println!(
            "res={res:>5}  samples={:>10}  Full eval = {ms:>7} ms",
            res as u64 * res as u64
        );
    }
}
