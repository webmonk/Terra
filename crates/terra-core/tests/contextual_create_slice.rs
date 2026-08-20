//! Contextual create - owner inference, override, workspaces, undo (WC single-stack).

use terra_core::contextual_create::{
    available_kinds, execute_create, override_owner, propose_owner, CreateContext, CreateError,
    CreateKind, CreateOwner, CreateWorkspace, CreatedEntity,
};
use terra_core::document::{EditorSession, TerrainDocument};

fn ctx_for(doc: &TerrainDocument, workspace: CreateWorkspace) -> CreateContext {
    CreateContext::from_document(doc, workspace, false)
}

#[test]
fn sculpt_owner_is_terrain_stack() {
    let session = EditorSession::new();
    let ctx = ctx_for(&session.document, CreateWorkspace::Sculpt);
    let p = propose_owner(&session.document, CreateKind::SculptLayer, &ctx);
    assert_eq!(p.owner, CreateOwner::World);
    assert_eq!(p.label, "Terrain");
    assert!(!p.ambiguous);
}

#[test]
fn biome_host_is_terrain_stack() {
    let session = EditorSession::new();
    let ctx = ctx_for(&session.document, CreateWorkspace::Biomes);
    let p = propose_owner(&session.document, CreateKind::Biome, &ctx);
    assert_eq!(p.owner, CreateOwner::World);
    assert_eq!(p.label, "Terrain");
}

#[test]
fn region_kind_removed_from_catalog() {
    let session = EditorSession::new();
    let ctx = ctx_for(&session.document, CreateWorkspace::World);
    let kinds = available_kinds(&ctx);
    assert!(kinds.contains(&CreateKind::WorldRule));
    assert!(kinds.contains(&CreateKind::Biome));
    assert!(!kinds.iter().any(|k| k.label() == "Region"));
}

#[test]
fn creation_from_each_workspace() {
    let mut session = EditorSession::new();
    let biome_id = session.document.active_biome.expect("default biome");

    let cases = [
        (CreateWorkspace::Sculpt, CreateKind::SculptLayer),
        (CreateWorkspace::Biomes, CreateKind::Biome),
        (CreateWorkspace::Develop, CreateKind::TerrainEffect),
        (CreateWorkspace::Rules, CreateKind::WorldRule),
        (CreateWorkspace::Simulation, CreateKind::RiverSource),
    ];
    for (ws, kind) in cases {
        let ctx = ctx_for(&session.document, ws);
        let out = execute_create(&mut session, kind, &ctx, None, None).unwrap();
        assert_eq!(out.kind, kind);
        let _ = biome_id;
        let _ = out;
    }
}

#[test]
fn selection_after_creation() {
    let mut session = EditorSession::new();
    let ctx = ctx_for(&session.document, CreateWorkspace::Sculpt);
    let out = execute_create(&mut session, CreateKind::SculptLayer, &ctx, None, None).unwrap();
    match out.entity {
        CreatedEntity::Layer(id) => {
            assert_eq!(session.document.selected, Some(id));
            assert!(session.document.stack.find(id).is_some());
        }
        other => panic!("expected layer, got {:?}", other),
    }
}

#[test]
fn undo_and_redo_world_rule() {
    let mut session = EditorSession::new();
    let ctx = ctx_for(&session.document, CreateWorkspace::Rules);
    let before_rules = session.document.world_rules.rules.len();
    execute_create(&mut session, CreateKind::WorldRule, &ctx, None, None).unwrap();
    assert_eq!(session.document.world_rules.rules.len(), before_rules + 1);
    assert!(session.undo_world_rule());
    assert_eq!(session.document.world_rules.rules.len(), before_rules);
    assert!(session.redo_world_rule());
    assert_eq!(session.document.world_rules.rules.len(), before_rules + 1);
}

#[test]
fn undo_and_redo_matter_scenario() {
    let mut session = EditorSession::new();
    let ctx = ctx_for(&session.document, CreateWorkspace::Simulation);
    let before = session.document.simulation_scenarios.scenarios.len();
    execute_create(&mut session, CreateKind::RiverSource, &ctx, None, None).unwrap();
    assert_eq!(
        session.document.simulation_scenarios.scenarios.len(),
        before + 1
    );
    assert!(session.undo_scenario());
    assert_eq!(
        session.document.simulation_scenarios.scenarios.len(),
        before
    );
    assert!(session.redo_scenario());
    assert_eq!(
        session.document.simulation_scenarios.scenarios.len(),
        before + 1
    );
}

#[test]
fn invalid_context_terrain_effect_without_biome() {
    let mut session = EditorSession::new();
    session.document.active_biome = None;
    if let Some(surface) = session
        .document
        .stack
        .find_category_mut(terra_core::layer::StackCategory::Surface)
    {
        surface.children.clear();
    }
    let ctx = ctx_for(&session.document, CreateWorkspace::Develop);
    let err = execute_create(&mut session, CreateKind::TerrainEffect, &ctx, None, None)
        .expect_err("should require biome");
    assert!(matches!(err, CreateError::OwnerRequired { .. }));
}

#[test]
fn terrain_owner_override_accepted() {
    let session = EditorSession::new();
    let proposal = override_owner(
        &session.document,
        CreateKind::SculptLayer,
        CreateOwner::World,
    )
    .expect("terrain owner");
    assert_eq!(proposal.owner, CreateOwner::World);
    assert_eq!(proposal.label, "Terrain");
}

#[test]
fn empty_project_behaviour() {
    let mut doc = TerrainDocument::new_default();
    doc.stack.nodes.clear();
    doc.active_biome = None;
    doc.selected = None;
    let ctx = CreateContext {
        empty_project: true,
        has_terrain: false,
        active_biome: None,
        ..ctx_for(&doc, CreateWorkspace::AllTools)
    };
    let kinds = available_kinds(&ctx);
    assert!(kinds.contains(&CreateKind::WorldRule));
    assert!(kinds.contains(&CreateKind::RiverSource));
    assert!(!kinds.contains(&CreateKind::TerrainEffect));

    let mut session = EditorSession::new();
    session.document = doc;
    let out = execute_create(&mut session, CreateKind::WorldRule, &ctx, None, None).unwrap();
    assert!(matches!(out.entity, CreatedEntity::WorldRule(_)));
}

#[test]
fn auto_switch_workspace_flag() {
    let mut session = EditorSession::new();
    let mut ctx = ctx_for(&session.document, CreateWorkspace::Sculpt);
    ctx.auto_switch_workspace = false;
    let out = execute_create(&mut session, CreateKind::WorldRule, &ctx, None, None).unwrap();
    assert!(!out.should_switch_workspace);

    ctx.auto_switch_workspace = true;
    let out = execute_create(&mut session, CreateKind::WorldRule, &ctx, None, None).unwrap();
    assert!(out.should_switch_workspace);
}

#[test]
fn owner_label_shown_before_create() {
    let session = EditorSession::new();
    let ctx = ctx_for(&session.document, CreateWorkspace::Sculpt);
    let p = propose_owner(&session.document, CreateKind::SculptLayer, &ctx);
    let line = p.summary_line(CreateKind::SculptLayer);
    assert!(line.contains("Terrain"));
    assert!(line.contains("Noise Layer") || line.contains("Create"));
}
