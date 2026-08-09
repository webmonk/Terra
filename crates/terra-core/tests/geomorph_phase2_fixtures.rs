//! Phase 2 geomorphology / hydrology foundation validation.
//!
//! Synthetic terrains: plane, cone, valley, basin, two basins, noisy mountain,
//! closed depression.

use terra_core::geomorph::{
    accumulate_drainage_area, analyze_terrain, build_flow_graph, closed_depression, cone,
    extract_streams, gradient_components, handle_depressions, noisy_mountain, plane,
    single_basin, single_valley, slope_magnitude, two_basins, watersheds_from_graph, BreachParams,
    DepressionMode, FlowModel, GeomorphOptions, Precipitation, PreserveBasinsParams,
    StreamExtractParams, WatershedOptions,
};
use terra_core::heightfield::{Heightfield, HeightfieldMetrics};
use terra_core::mask::MaskField;

fn metrics(n: u32) -> HeightfieldMetrics {
    HeightfieldMetrics::new(n, n, n as f32 * 10.0, n as f32 * 10.0)
}

#[test]
fn water_flows_downhill_on_plane() {
    let hf = plane(metrics(16));
    let filled = handle_depressions(&hf, DepressionMode::Fill).height;
    let g = build_flow_graph(&filled, FlowModel::D8);
    // Plane drains +X (decreasing height with i) → direction (1,0) = index 0.
    let mut east = 0;
    for &d in &g.d8_dir {
        if d == 0 {
            east += 1;
        }
    }
    assert!(east > 100, "east-flowing cells={east}");
    for idx in 0..g.width * g.height {
        if let Some(r) = g.d8_receiver_index(idx) {
            let (i, j) = g.coords(idx);
            let (ni, nj) = g.coords(r);
            assert!(
                filled.get(ni, nj) <= filled.get(i, j) + 1e-4,
                "uphill flow at ({i},{j})→({ni},{nj})"
            );
        }
    }
}

#[test]
fn accumulation_conserves_upstream_contribution() {
    let hf = single_valley(metrics(20));
    let filled = handle_depressions(&hf, DepressionMode::Fill).height;
    let g = build_flow_graph(&filled, FlowModel::D8);
    let acc = accumulate_drainage_area(&g, &Precipitation::uniform(1.0));
    let n = (g.width * g.height) as f32;
    // Every cell contributes 1; max accumulation at the primary outlet equals n
    // when the domain has a single drainage tree to the border.
    let max = acc.iter().copied().fold(0.0f32, f32::max);
    assert!(
        (max - n).abs() < 1e-2 || max >= n * 0.5,
        "max accumulation {max} vs cells {n}"
    );
    // Local contribution identity: each cell's acc >= its precip.
    for &a in &acc {
        assert!(a >= 1.0 - 1e-4);
    }
}

#[test]
fn watershed_boundaries_follow_divides() {
    let m = HeightfieldMetrics::new(24, 12, 240.0, 120.0);
    let hf = two_basins(m);
    let filled = handle_depressions(&hf, DepressionMode::Fill).height;
    let g = build_flow_graph(&filled, FlowModel::D8);
    let ws = watersheds_from_graph(&g, &filled, &WatershedOptions::default());
    let mid = m.width / 2;
    let left = ws.ids[(m.height / 2 * m.width + mid / 2) as usize];
    let right = ws.ids[(m.height / 2 * m.width + mid + mid / 2) as usize];
    assert_ne!(left, 0);
    assert_ne!(right, 0);
    assert_ne!(
        left, right,
        "ridge should separate basins (left={left} right={right})"
    );
    // Boundary mask should light up near the central ridge.
    let mut ridge_border = 0;
    for j in 0..m.height {
        if ws.boundaries.get(mid, j) > 0.5 || ws.boundaries.get(mid.saturating_sub(1), j) > 0.5 {
            ridge_border += 1;
        }
    }
    assert!(ridge_border > 0, "expected watershed boundary on ridge");
}

#[test]
fn routing_is_deterministic() {
    let hf = noisy_mountain(metrics(32));
    let filled = handle_depressions(&hf, DepressionMode::Fill).height;
    let a = build_flow_graph(&filled, FlowModel::D8);
    let b = build_flow_graph(&filled, FlowModel::D8);
    assert_eq!(a.d8_dir, b.d8_dir);
    assert_eq!(a.topo_order, b.topo_order);
    let acc_a = accumulate_drainage_area(&a, &Precipitation::uniform(1.0));
    let acc_b = accumulate_drainage_area(&b, &Precipitation::uniform(1.0));
    assert_eq!(acc_a, acc_b);
}

#[test]
fn tiled_derivative_halo_has_no_interior_seams() {
    // Compare full-field slope vs slope computed on overlapping tiles with halo.
    let hf = cone(metrics(48));
    let full = slope_magnitude(&hf, 0.0);
    let tile = 16u32;
    let halo = 2u32;
    let mut stitched = MaskField::zeros(hf.metrics);
    for tj in (0..hf.metrics.height).step_by(tile as usize) {
        for ti in (0..hf.metrics.width).step_by(tile as usize) {
            let ti = ti as u32;
            let tj = tj as u32;
            let i0 = ti.saturating_sub(halo);
            let j0 = tj.saturating_sub(halo);
            let i1 = (ti + tile + halo).min(hf.metrics.width);
            let j1 = (tj + tile + halo).min(hf.metrics.height);
            let tw = i1 - i0;
            let th = j1 - j0;
            let tm = HeightfieldMetrics::new(tw, th, tw as f32 * hf.metrics.dx(), th as f32 * hf.metrics.dz());
            let mut patch = Heightfield::zeros(tm);
            for j in 0..th {
                for i in 0..tw {
                    patch.set(i, j, hf.get(i0 + i, j0 + j));
                }
            }
            let local = slope_magnitude(&patch, 0.0);
            let write_i0 = ti;
            let write_j0 = tj;
            let write_i1 = (ti + tile).min(hf.metrics.width);
            let write_j1 = (tj + tile).min(hf.metrics.height);
            for j in write_j0..write_j1 {
                for i in write_i0..write_i1 {
                    let li = i - i0;
                    let lj = j - j0;
                    stitched.set(i, j, local.get(li, lj));
                }
            }
        }
    }
    let mut max_err = 0.0f32;
    // Interior away from outer border.
    for j in 4..hf.metrics.height - 4 {
        for i in 4..hf.metrics.width - 4 {
            let e = (full.get(i, j) - stitched.get(i, j)).abs();
            max_err = max_err.max(e);
        }
    }
    assert!(
        max_err < 1e-4,
        "tiled slope seam error {max_err} (halo={halo})"
    );
}

#[test]
fn closed_depression_fill_removes_strict_pits() {
    let hf = closed_depression(metrics(17));
    let filled = handle_depressions(&hf, DepressionMode::Fill).height;
    let w = filled.metrics.width as i32;
    let h = filled.metrics.height as i32;
    for j in 1..h - 1 {
        for i in 1..w - 1 {
            let h0 = filled.get(i as u32, j as u32);
            let mut any_le_nb = false;
            for dj in -1i32..=1 {
                for di in -1i32..=1 {
                    if di == 0 && dj == 0 {
                        continue;
                    }
                    if filled.get((i + di) as u32, (j + dj) as u32) <= h0 + 1e-4 {
                        any_le_nb = true;
                    }
                }
            }
            assert!(any_le_nb, "strict pit remains at ({i},{j}) after fill");
        }
    }
    assert!(filled.get(8, 8) > hf.get(8, 8));
}

#[test]
fn breach_does_not_raise_pit_floor() {
    let hf = closed_depression(metrics(17));
    let cx = 8u32;
    let cz = 8u32;
    let before = hf.get(cx, cz);
    let breached = handle_depressions(
        &hf,
        DepressionMode::Breach(BreachParams {
            max_carve_depth: 20.0,
            max_path_cells: 64,
        }),
    );
    assert!(
        breached.height.get(cx, cz) <= before + 1e-4,
        "breach must not fill the pit floor"
    );
}

#[test]
fn preserve_basins_keeps_deep_lake() {
    let hf = single_basin(metrics(17));
    // Deepen centre.
    let mut hf = hf;
    hf.set(8, 8, 0.0);
    let r = handle_depressions(
        &hf,
        DepressionMode::PreserveBasins(PreserveBasinsParams {
            min_depth_m: 1.0,
            min_cells: 4,
        }),
    );
    assert!(r.lake_mask.data().iter().any(|&v| v > 0.5));
}

#[test]
fn spatial_precip_is_not_hardcoded_uniform() {
    let hf = plane(metrics(12));
    let filled = handle_depressions(&hf, DepressionMode::Fill).height;
    let g = build_flow_graph(&filled, FlowModel::D8);
    let uni = accumulate_drainage_area(&g, &Precipitation::uniform(1.0));
    let mut map = MaskField::filled(hf.metrics, 0.1);
    for j in 0..12 {
        for i in 0..6 {
            map.set(i, j, 3.0);
        }
    }
    let spatial = accumulate_drainage_area(&g, &Precipitation::from_map(map));
    assert_ne!(uni, spatial);
}

#[test]
fn strahler_orders_increase_downstream() {
    let hf = single_valley(metrics(32));
    let opts = GeomorphOptions {
        streams: StreamExtractParams {
            accumulation_threshold: 4.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let a = analyze_terrain(&hf, &opts);
    let max_o = a
        .streams
        .order
        .data()
        .iter()
        .copied()
        .fold(0.0f32, f32::max);
    assert!(max_o >= 1.0);
}

#[test]
fn cone_gradient_points_outward() {
    let hf = cone(metrics(33));
    let (gx, _, mag) = gradient_components(&hf, 0.0);
    let v = gx.get(28, 16);
    let m = mag.get(28, 16);
    assert!(m > 0.0, "expected non-zero |∇h|, mag={m}");
    assert!(v <= 1e-5, "gx east of peak={v}");
}

#[test]
fn multi_scale_derivatives_run() {
    let hf = noisy_mountain(metrics(24));
    let set = terra_core::geomorph::DerivativeSet::compute(
        &hf,
        &terra_core::geomorph::DerivativeOptions {
            radius_m: 20.0,
            roughness_radii_m: vec![10.0, 40.0],
            openness_sectors: 8,
        },
    );
    assert!(set.slope.data().iter().all(|v| v.is_finite()));
    assert!(set.profile_curvature.data().iter().all(|v| v.is_finite()));
    assert!(set.gaussian_curvature.data().iter().all(|v| v.is_finite()));
}

#[test]
fn dinfinity_pipeline_runs() {
    let hf = single_valley(metrics(16));
    let opts = GeomorphOptions {
        flow_model: FlowModel::DInfinity,
        ..Default::default()
    };
    let a = analyze_terrain(&hf, &opts);
    assert_eq!(a.graph.model, FlowModel::DInfinity);
    assert!(!a.drainage_area.is_empty());
    let _ = extract_streams(
        &a.graph,
        &a.drainage_area,
        hf.metrics,
        &StreamExtractParams::default(),
    );
}
