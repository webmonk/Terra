//! Matter simulations — Water / Snow / Sand / Debris artist configuration.

use terra_core::biome_definition::BiomeDefinitionId;
use terra_core::document::EditorSession;
use terra_core::fields::FieldId;
use terra_core::matter_sim::{
    diagnose_matter_sim, outputs_for_consumer, sync_scenario_outputs_from_matter,
    MatterArtistSource, MatterOutputConsumer, MatterSimConfig, MatterType,
};
use terra_core::simulation_scenario::{
    OutputInfluence, ScenarioResultState, SimulationDomain, SimulationScenario,
    SimulationScenarioCommand,
};

#[test]
fn source_placement_water_and_sand() {
    let water = MatterSimConfig::water_rivers();
    assert!(water
        .sources
        .iter()
        .any(|s| matches!(s, MatterArtistSource::PaintedSprings { .. })));
    assert!(water
        .sources
        .iter()
        .any(|s| matches!(s, MatterArtistSource::Rainfall { .. })));
    let sand = MatterSimConfig::sand();
    assert!(sand.sources.iter().any(|s| s.is_paintable()));
    assert!(sand
        .sources
        .iter()
        .any(|s| matches!(s, MatterArtistSource::PaintedSand { .. })));
}

#[test]
fn domain_restrictions() {
    let mut debris = MatterSimConfig::debris().with_domain(SimulationDomain::NamedRegion {
        name: "Cliff Face".into(),
    });
    debris.influence = OutputInfluence::Painted { paint_mask: None };
    assert!(matches!(
        debris.domain,
        SimulationDomain::NamedRegion { .. }
    ));
    assert!(matches!(debris.influence, OutputInfluence::Painted { .. }));
    // Domain ≠ influence when painted apply subset.
    assert_ne!(debris.domain.label(), debris.influence.label());
}

#[test]
fn output_maps_declared_per_matter() {
    let water = MatterSimConfig::water_rivers();
    for f in [
        FieldId::FlowDirection,
        FieldId::FlowAccumulation,
        FieldId::RiverChannel,
        FieldId::WaterDepth,
        FieldId::Wetness,
        FieldId::Floodplain,
        FieldId::Erosion,
        FieldId::Deposition,
    ] {
        assert!(water.outputs.contains(&f), "water missing {f:?}");
    }
    let snow = MatterSimConfig::snow();
    assert!(snow.outputs.contains(&FieldId::SnowDepth));
    assert!(snow.outputs.contains(&FieldId::Meltwater));
    assert!(snow.outputs.contains(&FieldId::Drift));
    let sand = MatterSimConfig::sand();
    assert!(sand.outputs.contains(&FieldId::SandDepth));
    assert!(sand.outputs.contains(&FieldId::DuneCrest));
    assert!(sand.outputs.contains(&FieldId::SandMaterialMask));
    let debris = MatterSimConfig::debris();
    assert!(debris.outputs.contains(&FieldId::DebrisDepth));
    assert!(debris.outputs.contains(&FieldId::Instability));
    assert!(debris.outputs.contains(&FieldId::ScatterCandidates));
}

#[test]
fn snapshot_behaviour_non_destructive() {
    let cfg = MatterSimConfig::sand();
    let mut scenario = cfg.build_scenario();
    scenario.matter.push(cfg);
    scenario.begin_run();
    let id = scenario.complete_run(1);
    assert_eq!(scenario.snapshots.len(), 1);
    scenario.mark_outdated();
    assert_eq!(scenario.snapshots.len(), 1);
    assert_eq!(scenario.snapshots[0].id, id);
    // Apply does not bake away the snapshot.
    let fields = scenario.apply_selected_outputs(Some(id));
    assert!(!fields.is_empty());
    assert_eq!(scenario.snapshots.len(), 1);
}

#[test]
fn selective_application() {
    let mut cfg = MatterSimConfig::water_rivers();
    cfg.apply_selected = vec![FieldId::Wetness, FieldId::Floodplain];
    cfg.advanced.allow_height_delta = false;
    let mut scenario = cfg.build_scenario();
    sync_scenario_outputs_from_matter(&mut scenario, &cfg);
    assert_eq!(
        scenario.output_application.selected_fields,
        vec![FieldId::Wetness, FieldId::Floodplain]
    );
    assert!(!scenario.output_application.apply_height);
    scenario.matter.push(cfg);
    scenario.begin_run();
    let sid = scenario.complete_run(2);
    let applied = scenario.apply_selected_outputs(Some(sid));
    assert!(applied.contains(&FieldId::Wetness) || applied.contains(&FieldId::Floodplain));
}

#[test]
fn cross_biome_movement_domain() {
    let biome_a = BiomeDefinitionId::new();
    let biome_b = BiomeDefinitionId::new();
    // Matter may move across biomes when domain is world / multi-biome.
    let mut sand = MatterSimConfig::sand().with_domain(SimulationDomain::EntireWorld);
    let scenario = sand.build_scenario();
    assert!(matches!(
        scenario.domain,
        SimulationDomain::EntireWorld | SimulationDomain::InheritScope
    ));
    sand.domain = SimulationDomain::NamedRegion {
        name: "Cross-biome".into(),
    };
    // Selected biomes scope on the scenario for ownership, domain stays movable.
    let mut s = sand.build_scenario();
    s.scope =
        terra_core::simulation_scenario::ScenarioScope::SelectedBiomes(vec![biome_a, biome_b]);
    assert!(matches!(
        s.scope,
        terra_core::simulation_scenario::ScenarioScope::SelectedBiomes(ref v) if v.len() == 2
    ));
}

#[test]
fn frozen_outdated_results() {
    let cfg = MatterSimConfig::debris();
    let mut scenario = cfg.build_scenario();
    scenario.matter.push(cfg);
    scenario.begin_run();
    let id = scenario.complete_run(1);
    assert!(scenario.freeze_result(Some(id)));
    scenario.mark_outdated();
    assert_eq!(scenario.result_state, ScenarioResultState::Frozen);
    assert_eq!(scenario.snapshots[0].state, ScenarioResultState::Frozen);
}

#[test]
fn downstream_output_consumption() {
    let water = MatterSimConfig::water_rivers();
    let mats = outputs_for_consumer(&water, MatterOutputConsumer::Materials);
    assert!(mats.iter().any(|f| matches!(
        f,
        FieldId::Wetness | FieldId::Floodplain | FieldId::WaterDepth
    )));
    let rules = outputs_for_consumer(&water, MatterOutputConsumer::WorldRules);
    assert!(!rules.is_empty());
    let scatter = outputs_for_consumer(&MatterSimConfig::debris(), MatterOutputConsumer::Scatter);
    assert!(scatter.iter().any(|f| matches!(
        f,
        FieldId::ScatterCandidates | FieldId::Instability | FieldId::DebrisDepth
    )));
    let biome = outputs_for_consumer(
        &MatterSimConfig::snow(),
        MatterOutputConsumer::BiomePlacement,
    );
    assert!(biome
        .iter()
        .any(|f| matches!(f, FieldId::Snow | FieldId::Wetness)));
}

#[test]
fn diagnostics_excessive_settings() {
    let mut cfg = MatterSimConfig::sand();
    cfg.advanced.iterations = 999;
    cfg.artist.repose_angle_deg = 2.0;
    cfg.artist.strength = 3.0;
    let diags = diagnose_matter_sim(&cfg);
    assert!(diags
        .iter()
        .any(|d| d.code == "matter_excessive_iterations"));
    assert!(diags.iter().any(|d| d.code == "matter_unstable_angle"));
    assert!(diags.iter().any(|d| d.code == "matter_excessive_strength"));
}

#[test]
fn before_after_compare() {
    let mut cfg = MatterSimConfig::snow();
    let mut scenario = cfg.build_scenario();
    scenario.begin_run();
    let before = scenario.complete_run(1);
    scenario.begin_run();
    let after = scenario.complete_run(2);
    cfg.set_compare(Some(before), Some(after));
    assert_eq!(cfg.compare_before, Some(before));
    assert_eq!(cfg.compare_after, Some(after));
    scenario.set_compare(Some(before));
    scenario.preview_old_result(after);
    assert_eq!(scenario.compare_snapshot, Some(before));
    assert_eq!(scenario.preview_snapshot, Some(after));
}

#[test]
fn serialization_matter_on_scenario() {
    let cfg = MatterSimConfig::water_rivers();
    let mut scenario = cfg.build_scenario();
    scenario.matter.push(cfg);
    let json = serde_json::to_string(&scenario).expect("ser");
    let back: SimulationScenario = serde_json::from_str(&json).expect("de");
    assert_eq!(back.matter.len(), 1);
    assert_eq!(back.matter[0].matter, MatterType::WaterRivers);
    assert!(!back.matter[0].outputs.is_empty());
}

#[test]
fn document_add_matter_scenario_undo() {
    let mut session = EditorSession::new();
    let cfg = MatterSimConfig::sand();
    let mut scenario = cfg.build_scenario();
    scenario.matter.push(cfg);
    let id = scenario.id;
    session.push_scenario_command(SimulationScenarioCommand::Add { scenario, index: 0 });
    assert_eq!(session.document.simulation_scenarios.scenarios.len(), 1);
    assert_eq!(
        session.document.simulation_scenarios.scenarios[0]
            .matter
            .len(),
        1
    );
    assert!(session.undo_scenario());
    assert!(session.document.simulation_scenarios.scenarios.is_empty());
    assert!(session.redo_scenario());
    assert_eq!(session.document.simulation_scenarios.scenarios[0].id, id);
}

#[test]
fn artist_controls_before_advanced() {
    let cfg = MatterSimConfig::snow();
    // Artist surface present with defaults; Advanced is separate struct.
    assert!(cfg.artist.strength > 0.0);
    assert!(cfg.advanced.iterations >= 1);
    assert!(!cfg.advanced.allow_height_delta); // non-destructive by default
}
