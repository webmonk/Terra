//! Workspace switch isolation: presentation must not mutate project or selection.

use terra_core::document::TerrainDocument;
use terra_core::eval::EvalScheduler;
use terra_app::ui::{AppWorkspace, UiState, WorkspaceMode, WorkspaceState};

fn selection_snapshot(
    doc: &TerrainDocument,
) -> (
    Option<terra_core::layer::LayerId>,
    Option<terra_core::biome_paint::BiomeLayerId>,
    Option<terra_core::biome_definition::BiomeDefinitionId>,
) {
    (
        doc.selected,
        doc.selected_biome_layer,
        doc.biome_library.selected,
    )
}

#[test]
fn switch_workspace_does_not_modify_project_json() {
    let doc = TerrainDocument::new_default();
    let before = doc.to_json().expect("serialize");
    let mut ui = UiState::default();
    let token_scheduler = EvalScheduler::new();
    let token_before = token_scheduler.current_token;

    for mode in WorkspaceMode::ALL {
        ui.switch_workspace_mode(mode);
    }
    ui.switch_app_workspace(AppWorkspace::Hydrology);
    ui.set_biome_color_preview(true);
    ui.set_biome_color_preview(false);

    let after = doc.to_json().expect("serialize after");
    assert_eq!(before, after, "workspace switch must not mutate project");
    assert_eq!(
        token_scheduler.current_token, token_before,
        "workspace switch must not request rebuild"
    );
}

#[test]
fn switch_workspace_does_not_invalidate_scheduler_token() {
    let mut ui = UiState::default();
    let mut scheduler = EvalScheduler::new();
    let token = scheduler.request_rebuild();
    assert_eq!(scheduler.current_token, token);

    ui.switch_workspace_mode(WorkspaceMode::Biomes);
    ui.switch_workspace_mode(WorkspaceMode::Materials);
    ui.switch_workspace_mode(WorkspaceMode::Terrain);

    // Switching UI must not call request_rebuild — token stays put.
    assert_eq!(scheduler.current_token, token);
}

#[test]
fn selection_persists_across_workspace_switches() {
    let mut doc = TerrainDocument::new_default();
    let layer = doc.stack.layer_ids().first().copied();
    doc.selected = layer;
    doc.selected_biome_layer = doc.biome_layers.first().map(|b| b.id);
    let before = selection_snapshot(&doc);

    let mut ui = UiState::default();
    for mode in WorkspaceMode::ALL {
        ui.switch_workspace_mode(mode);
        assert_eq!(
            selection_snapshot(&doc),
            before,
            "selection must survive switch to {:?}",
            mode
        );
    }
}

#[test]
fn biome_preview_toggle_is_workspace_only() {
    let doc = TerrainDocument::new_default();
    let before = doc.to_json().unwrap();
    let mut ui = UiState::default();
    assert!(!ui.biome_color_preview);
    ui.switch_workspace_mode(WorkspaceMode::Biomes);
    assert!(ui.biome_color_preview);
    assert_eq!(doc.to_json().unwrap(), before);
    ui.set_biome_color_preview(false);
    assert!(!ui.biome_color_preview);
    assert_eq!(doc.to_json().unwrap(), before);
}

#[test]
fn workspace_state_round_trip_preserves_presentation() {
    let mut ui = UiState::default();
    ui.switch_workspace_mode(WorkspaceMode::Simulation);
    ui.sculpt_radius = 0.12;
    ui.inspector_advanced = true;
    let snap = ui.workspace_state();
    assert_eq!(snap.mode, WorkspaceMode::Simulation);
    assert_eq!(snap.app_workspace, AppWorkspace::Hydrology);

    let mut ui2 = UiState::default();
    ui2.apply_workspace_state(&snap);
    assert_eq!(ui2.workspace_mode, WorkspaceMode::Simulation);
    assert_eq!(ui2.sculpt_radius, 0.12);
    assert!(ui2.inspector_advanced);
}

#[test]
fn workspace_state_contains_no_project_entities() {
    // Structural: capturing workspace from a rich document still yields presentation-only data.
    let _doc = TerrainDocument::new_default();
    let ui = UiState::default();
    let ws: WorkspaceState = ui.workspace_state();
    assert!(ws.temp_solo.is_empty());
    assert_eq!(ws.mode, WorkspaceMode::Terrain);
}
