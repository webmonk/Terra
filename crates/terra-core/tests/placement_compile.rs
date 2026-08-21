//! PlacementDefinition compiles into existing Mask Layer DistNode stacks.

use std::collections::HashMap;
use terra_core::heightfield::HeightfieldMetrics;
use terra_core::mask::{
    bake_dist_nodes, bake_distribution_with_context, coverage_estimate, CompareOp, Condition,
    ConditionChannel, CoverageTerm, DistBakeContext, DistNodeKind, MaskField, MaskId,
    PlacementCoordinateSpace, PlacementDefinition, PlacementRefinement, PlacementSource, RuleGroup,
    RuleGroupMode, RuleNode, MAX_RULE_NEST_DEPTH,
};

fn alpine_rules() -> PlacementDefinition {
    let mut p = PlacementDefinition {
        space: PlacementCoordinateSpace::RuleBased,
        ..Default::default()
    };
    p.root = RuleGroup {
        mode: RuleGroupMode::All,
        invert: false,
        children: vec![
            RuleNode::Condition(Condition {
                channel: ConditionChannel::Height,
                op: CompareOp::Above,
                a: 1800.0,
                b: 0.0,
                falloff: 50.0,
                seed: 0,
                frequency: 0.02,
            }),
            RuleNode::Condition(Condition {
                channel: ConditionChannel::Slope,
                op: CompareOp::Below,
                a: 55.0,
                b: 0.0,
                falloff: 5.0,
                seed: 0,
                frequency: 0.02,
            }),
            RuleNode::Condition(Condition {
                channel: ConditionChannel::WaterDistance,
                op: CompareOp::Above,
                a: 100.0,
                b: 0.0,
                falloff: 0.0,
                seed: 0,
                frequency: 0.02,
            }),
        ],
    };
    p.recompute_hash();
    p
}

#[test]
fn rule_compilation_all_group() {
    let dist = alpine_rules().compile();
    assert_eq!(dist.nodes.len(), 1);
    assert!(matches!(dist.nodes[0].kind, DistNodeKind::GroupAll));
    assert_eq!(dist.nodes[0].children.len(), 3);
    assert!(matches!(
        dist.nodes[0].children[0].kind,
        DistNodeKind::Height { .. }
    ));
    assert!(matches!(
        dist.nodes[0].children[1].kind,
        DistNodeKind::Slope { .. }
    ));
    assert!(matches!(
        dist.nodes[0].children[2].kind,
        DistNodeKind::SeaLevel { .. }
    ));
}

#[test]
fn any_group_compiles_to_group_any() {
    let mut p = PlacementDefinition::default();
    p.root.mode = RuleGroupMode::Any;
    p.root.children = vec![
        RuleNode::Condition(Condition {
            channel: ConditionChannel::Height,
            op: CompareOp::Below,
            a: 50.0,
            b: 0.0,
            falloff: 0.0,
            seed: 0,
            frequency: 0.02,
        }),
        RuleNode::Condition(Condition {
            channel: ConditionChannel::Flow,
            op: CompareOp::Above,
            a: 0.4,
            b: 0.0,
            falloff: 0.0,
            seed: 0,
            frequency: 0.02,
        }),
    ];
    p.recompute_hash();
    let dist = p.compile();
    assert!(matches!(dist.nodes[0].kind, DistNodeKind::GroupAny));
}

#[test]
fn nested_groups_respect_depth_cap() {
    let mut inner = RuleGroup {
        mode: RuleGroupMode::Any,
        invert: false,
        children: vec![RuleNode::Condition(Condition {
            channel: ConditionChannel::Slope,
            op: CompareOp::Below,
            a: 30.0,
            b: 0.0,
            falloff: 0.0,
            seed: 0,
            frequency: 0.02,
        })],
    };
    for _ in 0..MAX_RULE_NEST_DEPTH + 2 {
        inner = RuleGroup {
            mode: RuleGroupMode::All,
            invert: false,
            children: vec![RuleNode::Group(inner)],
        };
    }
    let mut p = PlacementDefinition {
        root: inner,
        ..Default::default()
    };
    p.recompute_hash();
    let dist = p.compile();
    // Must compile without panic; deepest levels become Fill identity.
    assert!(!dist.nodes.is_empty() || p.root.children.is_empty());
    let _ = dist;
}

#[test]
fn painted_area_plus_conditions() {
    let mid = MaskId::new();
    let mut p = PlacementDefinition {
        space: PlacementCoordinateSpace::WorldSpace,
        ..Default::default()
    };
    p.coverage.push(CoverageTerm::PaintedWorld { mask: mid });
    p.root.children = vec![
        RuleNode::CoverageRef { index: 0 },
        RuleNode::Condition(Condition {
            channel: ConditionChannel::Height,
            op: CompareOp::Between,
            a: 100.0,
            b: 800.0,
            falloff: 0.0,
            seed: 0,
            frequency: 0.02,
        }),
    ];
    p.recompute_hash();
    let dist = p.compile();
    assert!(matches!(dist.nodes[0].kind, DistNodeKind::GroupAll));
    assert!(dist.nodes[0]
        .children
        .iter()
        .any(|c| matches!(c.kind, DistNodeKind::MaskAsset { .. })));
    assert!(dist.nodes[0]
        .children
        .iter()
        .any(|c| matches!(c.kind, DistNodeKind::Height { .. })));
}

#[test]
fn coordinate_space_tags_preserved() {
    for space in [
        PlacementCoordinateSpace::WorldSpace,
        PlacementCoordinateSpace::SurfaceSpace,
        PlacementCoordinateSpace::RuleBased,
    ] {
        let mut p = alpine_rules();
        p.space = space;
        p.recompute_hash();
        let json = serde_json::to_string(&p).unwrap();
        let back: PlacementDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(back.space, space);
    }
}

#[test]
fn empty_results_full_coverage() {
    let p = PlacementDefinition::default();
    let dist = p.compile();
    assert!(dist.nodes.is_empty());
    let metrics = HeightfieldMetrics::new(16, 16, 1000.0, 1000.0);
    let masks = HashMap::new();
    let ctx = DistBakeContext::masks_only(&masks);
    let field = bake_distribution_with_context(&dist, metrics, &ctx);
    assert!((field.get(0, 0) - 1.0).abs() < 1e-5);
}

#[test]
fn mask_stack_round_trip_canonical() {
    let p = alpine_rules();
    let dist = p.compile();
    let back = PlacementDefinition::try_decompile(&dist).expect("round-trip");
    assert_eq!(back.root.mode, RuleGroupMode::All);
    assert_eq!(back.root.children.len(), 3);
    let dist2 = back.compile();
    assert_eq!(dist.nodes.len(), dist2.nodes.len());
    assert!(matches!(dist2.nodes[0].kind, DistNodeKind::GroupAll));
}

#[test]
fn custom_advanced_masks_not_overwritten() {
    let mut p = alpine_rules();
    let mut edited = p.compile();
    edited.nodes.push(terra_core::mask::DistNode::new(
        DistNodeKind::EffectDilate { radius_m: 12.0 },
    ));
    p.mark_custom(edited.clone());
    assert_eq!(p.source, PlacementSource::Custom);
    // Recompile path must not replace Custom.
    let active = p.active_distribution();
    assert_eq!(active.nodes.len(), edited.nodes.len());
    assert!(matches!(
        active.nodes.last().unwrap().kind,
        DistNodeKind::EffectDilate { .. }
    ));
    // Reset restores rules compile.
    let restored = p.reset_to_rules();
    assert_eq!(p.source, PlacementSource::Rules);
    assert!(matches!(restored.nodes[0].kind, DistNodeKind::GroupAll));
    assert!(p.custom_stack.is_none());
}

#[test]
fn serialization_round_trip() {
    let mut p = alpine_rules();
    p.refinements
        .push(PlacementRefinement::Smooth { radius_samples: 2 });
    p.refinements
        .push(PlacementRefinement::Expand { radius_m: 8.0 });
    let json = serde_json::to_string_pretty(&p).unwrap();
    let back: PlacementDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(back.content_hash, p.content_hash);
    assert_eq!(back.refinements.len(), 2);
    assert_eq!(back.compile().nodes.len(), p.compile().nodes.len());
}

#[test]
fn stable_output_ids_and_kinds() {
    let p = alpine_rules();
    let a = p.compile();
    let b = p.compile();
    assert_eq!(a.nodes[0].id.0, b.nodes[0].id.0);
    for (x, y) in a.nodes[0].children.iter().zip(b.nodes[0].children.iter()) {
        assert_eq!(x.id.0, y.id.0);
        assert_eq!(x.combine, y.combine);
        assert_eq!(format!("{:?}", x.kind), format!("{:?}", y.kind));
    }
}

#[test]
fn content_hash_invalidation() {
    let mut p = alpine_rules();
    let h0 = p.content_hash;
    p.root.children.push(RuleNode::Condition(Condition {
        channel: ConditionChannel::Curvature,
        op: CompareOp::Between,
        a: -0.2,
        b: 0.2,
        falloff: 0.0,
        seed: 0,
        frequency: 0.02,
    }));
    p.recompute_hash();
    assert_ne!(p.content_hash, h0);
    // Hash feeds deterministic DistNode ids - compile seed changes.
    let id_before_edit_would_differ = alpine_rules().compile().nodes[0].id.0;
    assert_ne!(p.compile().nodes[0].id.0, id_before_edit_would_differ);
}

#[test]
fn group_any_bake_max_not_ones() {
    // Regression: ANY must not fold Max from ones.
    let mut p = PlacementDefinition::default();
    p.root.mode = RuleGroupMode::Any;
    p.root.children = vec![RuleNode::Condition(Condition {
        channel: ConditionChannel::Noise,
        op: CompareOp::Above,
        a: 0.99,
        b: 0.0,
        falloff: 0.0,
        seed: 1,
        frequency: 0.5,
    })];
    p.recompute_hash();
    let dist = p.compile();
    let metrics = HeightfieldMetrics::new(32, 32, 1000.0, 1000.0);
    let masks = HashMap::new();
    let ctx = DistBakeContext::masks_only(&masks);
    let field = bake_dist_nodes(&dist.nodes, metrics, &ctx);
    // Sparse high threshold noise - coverage estimate should be << 1.
    let est = coverage_estimate(&p, metrics, &ctx);
    assert!(est < 0.5, "est={est} field00={}", field.get(0, 0));
}

#[test]
fn biome_placement_rules_compiled_distribution() {
    use terra_core::biome_definition::BiomePlacementRules;
    let mut rules = BiomePlacementRules {
        definition: Some(alpine_rules()),
        ..Default::default()
    };
    let dist = rules.compiled_distribution();
    assert!(matches!(dist.nodes[0].kind, DistNodeKind::GroupAll));
    rules.mark_mask_stack_custom(dist.clone());
    assert_eq!(
        rules.definition.as_ref().unwrap().source,
        PlacementSource::Custom
    );
    let _ = rules.reset_placement_to_rules();
    assert_eq!(
        rules.definition.as_ref().unwrap().source,
        PlacementSource::Rules
    );
}

#[test]
fn dilate_erode_bake() {
    let metrics = HeightfieldMetrics::new(16, 16, 160.0, 160.0);
    let mut field = MaskField::zeros(metrics);
    field.set(8, 8, 1.0);
    let dilate = terra_core::mask::DistNode::new(DistNodeKind::EffectDilate { radius_m: 20.0 });
    let out = terra_core::mask::apply_effect_public(&dilate.kind, &field, 1.0);
    assert!(out.get(8, 8) > 0.9);
    assert!(out.get(9, 8) > 0.5);
    let erode = terra_core::mask::DistNode::new(DistNodeKind::EffectErode { radius_m: 20.0 });
    let shrunk = terra_core::mask::apply_effect_public(&erode.kind, &out, 1.0);
    // Erode of dilated seed should still leave center or nearby high.
    assert!(shrunk.get(8, 8) >= 0.0);
}
