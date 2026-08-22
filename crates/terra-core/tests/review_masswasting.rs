//! Adversarial review of commit 8f70081 ("Parallelise layered thermal erosion").
//!
//! The claim under test: the gather rewrite of `thermal_erode_layered` is
//! arithmetically identical to the pre-8f70081 scatter, apart from float
//! summation order. This file reconstructs the ORIGINAL implementation verbatim
//! (from `dbd5edf:crates/terra-core/src/analyze/mass_wasting.rs`) and diffs it
//! against the shipping one on real terrain.

use terra_core::analyze::{thermal_erode_layered, MassWastingState, ThermalResult};
use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::layer::ThermalErosionParams;
use terra_core::mask::MaskField;

// ---------------------------------------------------------------------------
// Verbatim copy of the pre-8f70081 implementation and its private helpers.
// ---------------------------------------------------------------------------

fn neighbors_8(dx: f32, dz: f32) -> [(i32, i32, f32); 8] {
    let diag = dx.hypot(dz);
    [
        (-1, 0, dx),
        (1, 0, dx),
        (0, -1, dz),
        (0, 1, dz),
        (-1, -1, diag),
        (1, -1, diag),
        (-1, 1, diag),
        (1, 1, diag),
    ]
}

fn normalize_mask(data: &[f32], metrics: HeightfieldMetrics) -> MaskField {
    let max_v = data.iter().copied().fold(0.0f32, f32::max);
    if max_v <= 1e-12 {
        return MaskField::zeros(metrics);
    }
    let scaled: Vec<f32> = data.iter().map(|v| (v / max_v).clamp(0.0, 1.0)).collect();
    MaskField::from_raw(metrics, &scaled)
}

fn raw_mask(data: &[f32], metrics: HeightfieldMetrics) -> MaskField {
    MaskField::from_raw(metrics, data)
}

fn thermal_erode_layered_original(
    input: &Heightfield,
    p: &ThermalErosionParams,
    hardness: &MaskField,
    initial: Option<&MassWastingState>,
) -> ThermalResult {
    let metrics = input.metrics;
    let w = metrics.width as usize;
    let hh = metrics.height as usize;
    let n = w * hh;
    let dx = metrics.dx();
    let dz = metrics.dz();
    let neighbors = neighbors_8(dx, dz);
    let talus_slope = p.talus_angle_deg.to_radians().tan();
    let strength = p.strength.clamp(0.0, 1.0);
    let weathering = p.weathering_rate.max(0.0);
    let material_cap = p.material_amount.max(0.0);
    let transport_hops = p.transport_distance.max(1.0).round() as u32;

    let mut state = if let Some(s) = initial {
        s.clone()
    } else {
        MassWastingState::from_height(input, 0.0, 0.0)
    };

    let mut erosion = vec![0.0f32; n];
    let mut deposit = vec![0.0f32; n];
    let mut instability = vec![0.0f32; n];

    for _ in 0..p.iterations.max(1) {
        // --- Weathering: bedrock -> debris (Yang K_th excess slope) ---
        let surface = state.sync_surface();
        for j in 0..hh as i32 {
            for i in 0..w as i32 {
                let idx = j as usize * w + i as usize;
                let k = hardness.get(i as u32, j as u32).clamp(0.0, 1.0);
                let soft = 1.0 - k;
                if soft <= 1e-6 || weathering <= 1e-8 {
                    continue;
                }
                let h0 = surface[idx];
                let mut max_excess = 0.0f32;
                for &(di, dj, dist) in &neighbors {
                    let ni = i + di;
                    let nj = j + dj;
                    if ni < 0 || nj < 0 || ni >= w as i32 || nj >= hh as i32 {
                        continue;
                    }
                    let nidx = nj as usize * w + ni as usize;
                    let excess = h0 - surface[nidx] - talus_slope * dist;
                    if excess > max_excess {
                        max_excess = excess;
                    }
                }
                if max_excess <= 1e-6 {
                    continue;
                }
                instability[idx] = instability[idx].max(max_excess);
                let detach = (weathering * soft * max_excess * strength * 0.25)
                    .min(material_cap)
                    .min(state.bedrock[idx].max(0.0));
                if detach <= 1e-8 {
                    continue;
                }
                state.bedrock[idx] -= detach;
                state.debris[idx] += detach;
                erosion[idx] += detach;
            }
        }

        // --- Transport: move loose debris downhill ---
        for _hop in 0..transport_hops.max(1) {
            let surface = state.sync_surface();
            let debris_src = state.debris.clone();
            let mut delta_debris = vec![0.0f32; n];
            for j in 0..hh as i32 {
                for i in 0..w as i32 {
                    let idx = j as usize * w + i as usize;
                    let available = debris_src[idx];
                    if available <= 1e-8 {
                        continue;
                    }
                    let h0 = surface[idx];
                    let mut deltas = Vec::with_capacity(8);
                    let mut sum = 0.0f32;
                    for &(di, dj, dist) in &neighbors {
                        let ni = i + di;
                        let nj = j + dj;
                        if ni < 0 || nj < 0 || ni >= w as i32 || nj >= hh as i32 {
                            continue;
                        }
                        let nidx = nj as usize * w + ni as usize;
                        let diff = h0 - surface[nidx] - talus_slope * dist;
                        if diff > 0.0 {
                            deltas.push((nidx, diff));
                            sum += diff;
                        }
                    }
                    if sum <= 0.0 {
                        continue;
                    }
                    let move_amt = (sum * strength * 0.125)
                        .min(available)
                        .min(material_cap.max(available));
                    if move_amt <= 1e-8 {
                        continue;
                    }
                    delta_debris[idx] -= move_amt;
                    erosion[idx] += move_amt * 0.15;
                    for (nidx, diff) in deltas {
                        let share = move_amt * (diff / sum);
                        delta_debris[nidx] += share;
                        deposit[nidx] += share;
                    }
                }
            }
            for i in 0..n {
                state.debris[i] = (debris_src[i] + delta_debris[i]).max(0.0);
            }
        }

        // --- Classical fallback: peel thin bedrock ---
        {
            let surface = state.sync_surface();
            let mut delta_b = vec![0.0f32; n];
            let mut delta_d = vec![0.0f32; n];
            for j in 0..hh as i32 {
                for i in 0..w as i32 {
                    let idx = j as usize * w + i as usize;
                    if state.debris[idx] > 1e-5 {
                        continue;
                    }
                    let k = hardness.get(i as u32, j as u32).clamp(0.0, 1.0);
                    let soft = 1.0 - k;
                    if soft <= 1e-6 {
                        continue;
                    }
                    let h0 = surface[idx];
                    let mut deltas = Vec::with_capacity(8);
                    let mut sum = 0.0f32;
                    for &(di, dj, dist) in &neighbors {
                        let ni = i + di;
                        let nj = j + dj;
                        if ni < 0 || nj < 0 || ni >= w as i32 || nj >= hh as i32 {
                            continue;
                        }
                        let nidx = nj as usize * w + ni as usize;
                        let diff = h0 - surface[nidx] - talus_slope * dist;
                        if diff > 0.0 {
                            deltas.push((nidx, diff));
                            sum += diff;
                        }
                    }
                    if sum <= 0.0 {
                        continue;
                    }
                    let move_amt = (sum * strength * 0.125 * soft)
                        .min(state.bedrock[idx])
                        .min(material_cap.max(sum));
                    if move_amt <= 1e-8 {
                        continue;
                    }
                    delta_b[idx] -= move_amt;
                    erosion[idx] += move_amt;
                    for (nidx, diff) in deltas {
                        let share = move_amt * (diff / sum);
                        delta_d[nidx] += share;
                        deposit[nidx] += share;
                    }
                }
            }
            for i in 0..n {
                state.bedrock[i] = (state.bedrock[i] + delta_b[i]).max(0.0);
                state.debris[i] = (state.debris[i] + delta_d[i]).max(0.0);
            }
        }
    }

    let surface = state.sync_surface();
    let mut stability = vec![1.0f32; n];
    for j in 0..hh as i32 {
        for i in 0..w as i32 {
            let idx = j as usize * w + i as usize;
            let h0 = surface[idx];
            let mut max_excess = 0.0f32;
            for &(di, dj, dist) in &neighbors {
                let ni = i + di;
                let nj = j + dj;
                if ni < 0 || nj < 0 || ni >= w as i32 || nj >= hh as i32 {
                    continue;
                }
                let nidx = nj as usize * w + ni as usize;
                let excess = h0 - surface[nidx] - talus_slope * dist;
                if excess > max_excess {
                    max_excess = excess;
                }
            }
            stability[idx] =
                (1.0 - (max_excess / (dx.max(dz) * 4.0)).clamp(0.0, 1.0)).clamp(0.0, 1.0);
            if max_excess > instability[idx] {
                instability[idx] = max_excess;
            }
        }
    }

    let (e_mask, d_mask) = (
        normalize_mask(&erosion, metrics),
        normalize_mask(&deposit, metrics),
    );
    ThermalResult {
        height: Heightfield::from_dense(metrics, &surface),
        bedrock: raw_mask(&state.bedrock, metrics),
        loose_debris: raw_mask(&state.debris, metrics),
        sediment: raw_mask(&state.sediment, metrics),
        erosion: e_mask,
        deposition: d_mask,
        talus_stability: raw_mask(&stability, metrics),
        instability: normalize_mask(&instability, metrics),
        erosion_raw: raw_mask(&erosion, metrics),
        deposition_raw: raw_mask(&deposit, metrics),
    }
}

// ---------------------------------------------------------------------------
// Terrain fixtures
// ---------------------------------------------------------------------------

fn hash2(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B9)
        ^ (y as u32).wrapping_mul(0x85EB_CA6B)
        ^ seed.wrapping_mul(0xC2B2_AE35);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545_F491);
    h ^= h >> 13;
    (h & 0xFFFF) as f32 / 65535.0
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let fx = x - xi as f32;
    let fy = y - yi as f32;
    let sx = fx * fx * (3.0 - 2.0 * fx);
    let sy = fy * fy * (3.0 - 2.0 * fy);
    let a = hash2(xi, yi, seed);
    let b = hash2(xi + 1, yi, seed);
    let c = hash2(xi, yi + 1, seed);
    let d = hash2(xi + 1, yi + 1, seed);
    let ab = a + (b - a) * sx;
    let cd = c + (d - c) * sx;
    ab + (cd - ab) * sy
}

fn fbm(x: f32, y: f32, seed: u32) -> f32 {
    let mut amp = 1.0;
    let mut freq = 1.0;
    let mut sum = 0.0;
    let mut norm = 0.0;
    for o in 0..5 {
        sum += amp * value_noise(x * freq, y * freq, seed.wrapping_add(o * 977));
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

/// Steep fbm relief with a hard cliff band, on a non-square grid so any
/// row/column asymmetry in the gather shows up.
fn fbm_terrain(w: u32, h: u32) -> Heightfield {
    let metrics = HeightfieldMetrics::new(w, h, 1000.0, 1200.0);
    let mut data = vec![0.0f32; (w * h) as usize];
    for j in 0..h {
        for i in 0..w {
            let x = i as f32 / w as f32 * 6.0;
            let y = j as f32 / h as f32 * 6.0;
            let mut v = fbm(x, y, 7) * 900.0 - 120.0;
            // Sharp cliff so the talus threshold is comfortably exceeded.
            if i > w / 3 && i < 2 * w / 3 {
                v += 400.0;
            }
            data[(j * w + i) as usize] = v;
        }
    }
    Heightfield::from_dense(metrics, &data)
}

fn varied_hardness(metrics: HeightfieldMetrics) -> MaskField {
    let w = metrics.width;
    let h = metrics.height;
    let mut data = vec![0.0f32; (w * h) as usize];
    for j in 0..h {
        for i in 0..w {
            let x = i as f32 / w as f32 * 4.0;
            let y = j as f32 / h as f32 * 4.0;
            let mut k = fbm(x, y, 31);
            // Include fully-hard (soft <= 1e-6) and fully-soft cells so both
            // early-out branches of every pass are exercised.
            if (i + j) % 17 == 0 {
                k = 1.0;
            }
            if (i * 3 + j) % 23 == 0 {
                k = 0.0;
            }
            data[(j * w + i) as usize] = k;
        }
    }
    MaskField::from_raw(metrics, &data)
}

struct Div {
    max_abs: f32,
    /// `max_abs` scaled by the field's own magnitude - the only relative
    /// measure that is meaningful when individual cells sit near zero.
    max_scaled: f32,
    scale: f32,
    at: usize,
    n_diff: usize,
}

fn compare(name: &str, a: &[f32], b: &[f32]) -> Div {
    assert_eq!(a.len(), b.len(), "{name}: length mismatch");
    let scale = a
        .iter()
        .chain(b.iter())
        .fold(0.0f32, |m, v| m.max(v.abs()))
        .max(1e-6);
    let mut d = Div {
        max_abs: 0.0,
        max_scaled: 0.0,
        scale,
        at: 0,
        n_diff: 0,
    };
    for i in 0..a.len() {
        let abs = (a[i] - b[i]).abs();
        if abs > 0.0 {
            d.n_diff += 1;
        }
        if abs > d.max_abs {
            d.max_abs = abs;
            d.at = i;
        }
    }
    d.max_scaled = d.max_abs / scale;
    d
}

// ---------------------------------------------------------------------------
// Differential test
// ---------------------------------------------------------------------------

fn run_case(
    label: &str,
    input: &Heightfield,
    p: &ThermalErosionParams,
    hardness: &MaskField,
    initial: Option<&MassWastingState>,
) {
    let old = thermal_erode_layered_original(input, p, hardness, initial);
    let new = thermal_erode_layered(input, p, hardness, initial);

    let w = input.metrics.width as usize;

    let fields: [(&str, Vec<f32>, Vec<f32>); 8] = [
        ("height", old.height.to_dense(), new.height.to_dense()),
        (
            "bedrock",
            old.bedrock.data().to_vec(),
            new.bedrock.data().to_vec(),
        ),
        (
            "loose_debris",
            old.loose_debris.data().to_vec(),
            new.loose_debris.data().to_vec(),
        ),
        (
            "erosion_raw",
            old.erosion_raw.data().to_vec(),
            new.erosion_raw.data().to_vec(),
        ),
        (
            "deposition_raw",
            old.deposition_raw.data().to_vec(),
            new.deposition_raw.data().to_vec(),
        ),
        (
            "talus_stability",
            old.talus_stability.data().to_vec(),
            new.talus_stability.data().to_vec(),
        ),
        (
            "instability",
            old.instability.data().to_vec(),
            new.instability.data().to_vec(),
        ),
        (
            "sediment",
            old.sediment.data().to_vec(),
            new.sediment.data().to_vec(),
        ),
    ];

    for (fname, a, b) in fields.iter() {
        let d = compare(fname, a, b);
        // Reassociating up-to-8-term f32 sums over a few iterations should stay
        // within a few ULP of the field's own magnitude. Anything larger is a
        // structural change, not reassociation.
        eprintln!(
            "[{label}] {fname}: max_abs={:.3e} (scale {:.4e}, {:.2e} of scale) \
             worst ({}, {}) diff_cells={}/{}",
            d.max_abs,
            d.scale,
            d.max_scaled,
            d.at % w,
            d.at / w,
            d.n_diff,
            a.len()
        );
        assert!(
            d.max_scaled < 1e-5,
            "[{label}] {fname} diverged: max_abs={:.6e} = {:.3e} of field scale {:.4e}; \
             worst cell ({}, {}) old={:.9} new={:.9}, {} of {} cells differ",
            d.max_abs,
            d.max_scaled,
            d.scale,
            d.at % w,
            d.at / w,
            a[d.at],
            b[d.at],
            d.n_diff,
            a.len()
        );
    }
}

#[test]
fn gather_rewrite_matches_original_scatter() {
    let input = fbm_terrain(96, 80);
    let hardness = varied_hardness(input.metrics);

    // Multi-iteration, multi-hop: every pass and several hops execute.
    let p = ThermalErosionParams {
        talus_angle_deg: 32.0,
        iterations: 4,
        strength: 0.7,
        material_amount: 6.0,
        weathering_rate: 0.6,
        transport_distance: 3.0,
        layered_materials: true,
        ..ThermalErosionParams::default()
    };

    run_case("fresh-state", &input, &p, &hardness, None);

    // A pre-loaded debris inventory makes the transport pass dominant and the
    // bedrock-peel pass mostly-skipped, and vice versa on bare cells.
    let seeded = MassWastingState::from_height(&input, 0.4, 0.2);
    run_case("seeded-debris", &input, &p, &hardness, Some(&seeded));

    // Zero weathering: transport + peel only.
    let p_no_weather = ThermalErosionParams {
        weathering_rate: 0.0,
        ..p.clone()
    };
    run_case(
        "no-weathering",
        &input,
        &p_no_weather,
        &hardness,
        Some(&seeded),
    );

    // Tiny material cap: `move_amt` is cap-limited nearly everywhere.
    let p_capped = ThermalErosionParams {
        material_amount: 0.01,
        ..p.clone()
    };
    run_case("tiny-cap", &input, &p_capped, &hardness, Some(&seeded));
}

/// Boundary-only probe: a 1-cell-wide ridge against each edge. If the gather
/// pass had an asymmetric in-bounds test, edge cells would diverge while the
/// interior matched.
#[test]
fn gather_rewrite_matches_original_at_boundaries() {
    let metrics = HeightfieldMetrics::new(24, 18, 240.0, 180.0);
    let w = metrics.width as usize;
    let h = metrics.height as usize;
    let mut data = vec![0.0f32; w * h];
    for j in 0..h {
        for i in 0..w {
            // Spikes hugging every edge and corner.
            let edge = i == 0 || j == 0 || i == w - 1 || j == h - 1;
            data[j * w + i] = if edge { 300.0 } else { 0.0 };
        }
    }
    let input = Heightfield::from_dense(metrics, &data);
    let hardness = MaskField::from_raw(metrics, &vec![0.1f32; w * h]);
    let p = ThermalErosionParams {
        talus_angle_deg: 20.0,
        iterations: 3,
        strength: 1.0,
        material_amount: 20.0,
        weathering_rate: 0.8,
        transport_distance: 4.0,
        layered_materials: true,
        ..ThermalErosionParams::default()
    };
    let seeded = MassWastingState::from_height(&input, 0.5, 0.0);
    run_case("edge-ridge", &input, &p, &hardness, Some(&seeded));
}

/// A cell can have `sum > 0` (it is over-steep) and still shed nothing, because
/// `move_for` vetoed it (no debris available / rock too hard). `sums[nidx]` then
/// stays 0 while `surface[nidx] - h_me - talus*dist` is still positive for its
/// downhill neighbours. If pass 2 of `redistribute_gather` (mass_wasting.rs:333)
/// did not gate on `outgoing[nidx] <= 0.0` first, that would be a 0/0 NaN which
/// would then poison the whole field via `sync_surface`.
#[test]
fn non_shedding_uphill_neighbours_do_not_divide_by_zero() {
    let metrics = HeightfieldMetrics::new(33, 33, 330.0, 330.0);
    let w = metrics.width as usize;
    let h = metrics.height as usize;
    let mut data = vec![0.0f32; w * h];
    // A cone: every interior cell is over-steep toward at least one neighbour,
    // and the apex is over-steep toward all eight.
    for j in 0..h {
        for i in 0..w {
            let r = (((i as f32 - 16.0).powi(2)) + ((j as f32 - 16.0).powi(2))).sqrt();
            data[j * w + i] = (16.0 - r).max(0.0) * 60.0;
        }
    }
    let input = Heightfield::from_dense(metrics, &data);

    // Hardness 1.0 on a checkerboard: those cells have `soft <= 1e-6`, so the
    // bedrock-peel `move_for` returns 0 for them even though their `sum` is
    // large - exactly the "sum > 0 but sums[] stays 0" case - and every soft
    // cell has at least one such uphill neighbour.
    let mut k = vec![0.0f32; w * h];
    for j in 0..h {
        for i in 0..w {
            k[j * w + i] = if (i + j) % 2 == 0 { 1.0 } else { 0.0 };
        }
    }
    let hardness = MaskField::from_raw(metrics, &k);

    let p = ThermalErosionParams {
        talus_angle_deg: 15.0,
        iterations: 5,
        strength: 1.0,
        material_amount: 50.0,
        // No weathering, no starting debris: the transport pass also sees
        // `available == 0` everywhere on the first hop, so *every* cell has
        // `sum > 0` with `sums[] == 0`.
        weathering_rate: 0.0,
        transport_distance: 3.0,
        layered_materials: true,
        ..ThermalErosionParams::default()
    };
    let result = thermal_erode_layered(&input, &p, &hardness, None);

    for (name, field) in [
        ("bedrock", result.bedrock.data()),
        ("loose_debris", result.loose_debris.data()),
        ("erosion_raw", result.erosion_raw.data()),
        ("deposition_raw", result.deposition_raw.data()),
        ("talus_stability", result.talus_stability.data()),
        ("instability", result.instability.data()),
    ] {
        assert!(
            field.iter().all(|v| v.is_finite()),
            "{name} contains a non-finite value"
        );
    }
    assert!(
        result.height.to_dense().iter().all(|v| v.is_finite()),
        "height contains a non-finite value"
    );

    // And it is the same answer the scatter gave.
    run_case("cone-hard-checkerboard", &input, &p, &hardness, None);
}

// ---------------------------------------------------------------------------
// erosion.rs: Vec<(usize, f32)> -> [(usize, f32); 8] + n_deltas
// ---------------------------------------------------------------------------

/// Pre-8f70081 height-only pass, using the heap `Vec` for the downhill list.
fn thermal_erode_height_only_original(
    input: &Heightfield,
    p: &ThermalErosionParams,
    hardness: &MaskField,
) -> Vec<f32> {
    let mut h = input.to_dense();
    let w = input.metrics.width as usize;
    let hh = input.metrics.height as usize;
    let dx = input.metrics.dx();
    let dz = input.metrics.dz();
    let talus_slope = p.talus_angle_deg.to_radians().tan();
    let strength = p.strength.clamp(0.0, 1.0);
    let neighbors = neighbors_8(dx, dz);

    for _ in 0..p.iterations {
        let src = h.clone();
        for j in 0..hh as i32 {
            for i in 0..w as i32 {
                let idx = j as usize * w + i as usize;
                let h0 = src[idx];
                let k = hardness.get(i as u32, j as u32).clamp(0.0, 1.0);
                let soft = 1.0 - k;
                if soft <= 1e-6 {
                    continue;
                }
                let mut deltas = Vec::with_capacity(8);
                let mut sum = 0.0f32;
                for &(di, dj, distance) in &neighbors {
                    let ni = i + di;
                    let nj = j + dj;
                    if ni < 0 || nj < 0 || ni >= w as i32 || nj >= hh as i32 {
                        continue;
                    }
                    let nidx = nj as usize * w + ni as usize;
                    let diff = h0 - src[nidx] - talus_slope * distance;
                    if diff > 0.0 {
                        deltas.push((nidx, diff));
                        sum += diff;
                    }
                }
                if sum <= 0.0 {
                    continue;
                }
                let move_amt = sum * strength * 0.125 * soft;
                h[idx] -= move_amt;
                for (nidx, diff) in deltas {
                    let share = move_amt * (diff / sum);
                    h[nidx] += share;
                }
            }
        }
    }
    h
}

/// The fixed-size `deltas` array in erosion.rs can only be written once per
/// entry of the 8-element `neighbors` array, so `n_deltas <= 8`. An isolated
/// interior spike drives it to exactly 8 on every iteration - the boundary
/// case - and the result must match the `Vec` version bit for bit.
#[test]
fn height_only_delta_array_holds_all_eight_downhill_neighbours() {
    let metrics = HeightfieldMetrics::new(21, 21, 210.0, 210.0);
    let w = metrics.width as usize;
    let hh = metrics.height as usize;
    let mut data = vec![0.0f32; w * hh];
    // Isolated interior spikes: all 8 neighbours strictly downhill.
    for j in (2..hh - 2).step_by(3) {
        for i in (2..w - 2).step_by(3) {
            data[j * w + i] = 500.0;
        }
    }
    let input = Heightfield::from_dense(metrics, &data);
    let hardness = MaskField::from_raw(metrics, &vec![0.0f32; w * hh]);
    let p = ThermalErosionParams {
        talus_angle_deg: 5.0,
        iterations: 6,
        strength: 1.0,
        layered_materials: false,
        ..ThermalErosionParams::default()
    };

    let expected = thermal_erode_height_only_original(&input, &p, &hardness);
    let (height, _, _) = terra_core::analyze::thermal_erode_with_hardness(&input, &p, &hardness);
    let got = height.to_dense();
    for idx in 0..expected.len() {
        assert_eq!(
            expected[idx].to_bits(),
            got[idx].to_bits(),
            "cell ({}, {}) differs: Vec form {} vs array form {}",
            idx % w,
            idx / w,
            expected[idx],
            got[idx]
        );
    }
}
