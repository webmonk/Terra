//! Simulation Scenarios — authoring containers over Simulation Layers.

use terra_core::document::EditorSession;
use terra_core::fields::FieldId;
use terra_core::layer::{
    HydraulicErosionParams, Layer, LayerKind, LayerStack, StackNode, ThermalErosionParams,
};
use terra_core::simulation_scenario::{
    diagnose_scenario, layer_kind_is_scenario_compatible, MatterSourceKind, OutputInfluence,
    ScenarioPass, ScenarioPassKind, ScenarioResultState, ScenarioScope, SimulationDomain,
    SimulationScenario, SimulationScenarioCommand, SimulationScenarioLibrary,
};

#[test]
fn scenario_lifecycle_run_outdated_freeze_reset() {
    let mut s = SimulationScenario::continental_hydrology_preset();
    assert_eq!(s.result_state, ScenarioResultState::Ready);

    s.begin_run();
    assert_eq!(s.result_state, ScenarioResultState::Running);
    let snap = s.complete_run(1);
    assert_eq!(s.result_state, ScenarioResultState::Current);
    assert_eq!(s.current_snapshot, Some(snap));
    assert_eq!(s.snapshots.len(), 1);

    s.mark_outdated();
    assert_eq!(s.result_state, ScenarioResultState::Outdated);
    assert_eq!(s.snapshots[0].state, ScenarioResultState::Outdated);

    // Second run — previous becomes outdated, new is current.
    s.begin_run();
    let snap2 = s.complete_run(2);
    assert_eq!(s.snapshots.len(), 2);
    assert_eq!(s.current_snapshot, Some(snap2));
    assert_eq!(s.snapshots[0].state, ScenarioResultState::Outdated);

    assert!(s.freeze_result(Some(snap2)));
    assert_eq!(s.result_state, ScenarioResultState::Frozen);
    assert_eq!(s.snapshots[1].state, ScenarioResultState::Frozen);

    // Upstream change must not unfreeze.
    s.mark_outdated();
    assert_eq!(s.result_state, ScenarioResultState::Frozen);

    s.reset();
    assert!(s
        .snapshots
        .iter()
        .all(|x| x.state == ScenarioResultState::Frozen));
    assert_eq!(s.result_state, ScenarioResultState::Ready);
}

#[test]
fn source_domain_output_influence_are_distinct() {
    let s = SimulationScenario::continental_hydrology_preset();
    assert_eq!(s.sources.len(), 3);
    assert!(s
        .sources
        .iter()
        .any(|x| matches!(x.kind, MatterSourceKind::Rainfall)));
    assert!(s
        .sources
        .iter()
        .any(|x| matches!(x.kind, MatterSourceKind::PaintedSprings)));
    assert!(s
        .sources
        .iter()
        .any(|x| matches!(x.kind, MatterSourceKind::Snowmelt)));
    assert!(matches!(s.domain, SimulationDomain::NamedRegion { .. }));
    assert!(matches!(s.output_influence, OutputInfluence::EntireDomain));
    // Domain label ≠ influence label for painted influence cases.
    let mut painted = s.clone();
    painted.output_influence = OutputInfluence::Painted { paint_mask: None };
    assert_ne!(painted.domain.label(), painted.output_influence.label());
}

#[test]
fn pass_ordering_diagnostics() {
    let mut s = SimulationScenario::new("Order Test");
    let mut dependent = ScenarioPass::new(ScenarioPassKind::Sediment);
    let producer = ScenarioPass::new(ScenarioPassKind::Flow);
    dependent.depends_on.push(producer.id);
    // Invalid: dependent before producer.
    s.passes.push(dependent);
    s.passes.push(producer);
    let diags = diagnose_scenario(&s);
    assert!(diags.iter().any(|d| d.code == "scenario_invalid_order"));

    // Valid continental order has no invalid_order.
    let ok = SimulationScenario::continental_hydrology_preset();
    let diags = diagnose_scenario(&ok);
    assert!(!diags.iter().any(|d| d.code == "scenario_invalid_order"));
}

#[test]
fn artists_can_add_and_reorder_passes() {
    let mut s = SimulationScenario::new("Custom");
    let a = s.add_pass(ScenarioPassKind::Flow);
    let b = s.add_pass(ScenarioPassKind::HydraulicErosion);
    assert_eq!(s.passes.len(), 2);
    assert!(s.reorder_pass(b, 0));
    assert_eq!(s.passes[0].id, b);
    assert_eq!(s.passes[1].id, a);
}

#[test]
fn outdated_results_keep_snapshots() {
    let mut s = SimulationScenario::continental_hydrology_preset();
    s.begin_run();
    s.complete_run(1);
    let n = s.snapshots.len();
    s.mark_outdated();
    assert_eq!(s.snapshots.len(), n);
    assert!(!s.snapshots.is_empty());
}

#[test]
fn frozen_results_survive_reset_and_outdated() {
    let mut s = SimulationScenario::new("Freeze");
    s.begin_run();
    let id = s.complete_run(1);
    s.freeze_result(Some(id));
    s.mark_outdated();
    assert_eq!(s.snapshots[0].state, ScenarioResultState::Frozen);
    s.reset();
    assert_eq!(s.snapshots.len(), 1);
    assert_eq!(s.snapshots[0].state, ScenarioResultState::Frozen);
}

#[test]
fn cancellation_sets_cancelled_state() {
    let mut s = SimulationScenario::new("Cancel");
    s.begin_run();
    s.request_cancel();
    assert!(s.cancel_requested);
    s.mark_cancelled();
    assert_eq!(s.result_state, ScenarioResultState::Cancelled);
    assert!(!s.cancel_requested);
}

#[test]
fn snapshot_persistence_serialization() {
    let mut s = SimulationScenario::continental_hydrology_preset();
    s.begin_run();
    s.complete_run(7);
    s.output_application.selected_fields = vec![FieldId::Wetness, FieldId::Sediment];
    let json = serde_json::to_string(&s).expect("ser");
    let back: SimulationScenario = serde_json::from_str(&json).expect("de");
    assert_eq!(back.snapshots.len(), 1);
    assert_eq!(back.snapshots[0].generation, 7);
    assert_eq!(back.name, "Continental Hydrology");
    assert_eq!(back.passes.len(), 4);
}

#[test]
fn selective_output_application() {
    let mut s = SimulationScenario::continental_hydrology_preset();
    s.output_application.selected_fields = vec![FieldId::Wetness];
    s.begin_run();
    let id = s.complete_run(1);
    if let Some(snap) = s.snapshots.iter_mut().find(|x| x.id == id) {
        snap.apply_selected = vec![FieldId::Sediment];
    }
    let applied = s.apply_selected_outputs(Some(id));
    assert_eq!(applied, vec![FieldId::Sediment]);
    assert_eq!(s.result_state, ScenarioResultState::Current);
}

#[test]
fn document_library_serialization() {
    let mut session = EditorSession::new();
    session
        .document
        .simulation_scenarios
        .push(SimulationScenario::continental_hydrology_preset());
    let json = session.document.to_json().expect("doc");
    assert!(json.contains("simulation_scenarios") || json.contains("Continental"));
    let back = terra_core::document::TerrainDocument::from_json(&json).expect("load");
    assert_eq!(back.simulation_scenarios.scenarios.len(), 1);
}

#[test]
fn existing_simulation_layer_compatibility() {
    let mut stack = LayerStack::new();
    let hyd = Layer::new(
        "Hydraulic",
        LayerKind::HydraulicErosion(HydraulicErosionParams::default()),
    );
    let therm = Layer::new(
        "Thermal",
        LayerKind::ThermalErosion(ThermalErosionParams::default()),
    );
    assert!(layer_kind_is_scenario_compatible(&hyd.kind));
    assert!(layer_kind_is_scenario_compatible(&therm.kind));

    let hid = hyd.id();
    let pass = ScenarioPass::from_layer(hid, &hyd.kind, hyd.common.name.clone()).unwrap();
    assert_eq!(pass.kind, ScenarioPassKind::HydraulicErosion);
    assert_eq!(pass.layer_id, Some(hid));
    stack.nodes.push(StackNode::Layer(hyd));
    stack.nodes.push(StackNode::Layer(therm));

    let mut scenario = SimulationScenario::new("From Stack");
    scenario.passes.push(pass);
    assert_eq!(scenario.bound_layer_ids(), vec![hid]);
}

#[test]
fn undo_redo_scenario_commands() {
    let mut session = EditorSession::new();
    let scenario = SimulationScenario::continental_hydrology_preset();
    let id = scenario.id;
    session.push_scenario_command(SimulationScenarioCommand::Add { scenario, index: 0 });
    assert_eq!(session.document.simulation_scenarios.scenarios.len(), 1);
    assert!(session.undo_scenario());
    assert!(session.document.simulation_scenarios.scenarios.is_empty());
    assert!(session.redo_scenario());
    assert_eq!(session.document.simulation_scenarios.scenarios[0].id, id);
}

#[test]
fn scopes_and_rebuild() {
    let mut s = SimulationScenario::new("Scoped");
    s.scope = ScenarioScope::World;
    s.begin_run();
    s.complete_run(1);
    s.mark_outdated();
    s.rebuild();
    assert_eq!(s.result_state, ScenarioResultState::Ready);
    // Snapshots retained after rebuild.
    assert!(!s.snapshots.is_empty());
}

#[test]
fn preview_and_compare() {
    let mut s = SimulationScenario::new("Compare");
    s.begin_run();
    let a = s.complete_run(1);
    s.begin_run();
    let b = s.complete_run(2);
    assert!(s.preview_old_result(a));
    assert_eq!(s.preview_snapshot, Some(a));
    s.set_compare(Some(b));
    assert_eq!(s.compare_snapshot, Some(b));
}

#[test]
fn library_mark_all_outdated() {
    let mut lib = SimulationScenarioLibrary::default();
    let mut a = SimulationScenario::new("A");
    a.begin_run();
    a.complete_run(1);
    lib.push(a);
    lib.mark_all_outdated();
    assert_eq!(lib.scenarios[0].result_state, ScenarioResultState::Outdated);
}
