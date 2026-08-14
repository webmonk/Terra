//! Persisted document-version compatibility contract.

use terra_core::document::{TerrainDocument, DOCUMENT_VERSION};
use terra_core::layer::{LayerId, LayerKind};
use terra_core::mask::{MaskCombine, MaskId, MaskSource};
use uuid::Uuid;

// Emitted by TerrainDocument::to_json at audited commit 7b336c4.
const V1_FIXTURE: &str = include_str!("fixtures/document_v1_7b336c4.json");
// Emitted by TerrainDocument::to_json at historical commit b450e72.
const V2_FIXTURE: &str = include_str!("fixtures/document_v2_b450e72.json");

fn layer_id(value: &str) -> LayerId {
    LayerId(Uuid::parse_str(value).expect("valid fixture layer id"))
}

fn mask_id(value: &str) -> MaskId {
    MaskId(Uuid::parse_str(value).expect("valid fixture mask id"))
}

fn assert_fixture_semantics(doc: &TerrainDocument, expected_name: &str) {
    let base_id = layer_id("11111111-1111-4111-8111-111111111111");
    let hills_id = layer_id("22222222-2222-4222-8222-222222222222");
    let height_mask_id = mask_id("33333333-3333-4333-8333-333333333333");

    assert_eq!(doc.version, DOCUMENT_VERSION);
    assert_eq!(doc.name, expected_name);
    assert_eq!(doc.selected, Some(hills_id));
    assert_eq!(doc.presets_used, ["Fixture Preset"]);

    let base = doc.stack.find(base_id).expect("legacy base survives");
    let LayerKind::SculptBase(params) = &base.kind else {
        panic!("legacy base kind changed");
    };
    assert_eq!(params.resolution, 2);
    assert_eq!(params.samples, [12.5; 4]);
    assert_eq!(params.fill_height, 12.5);

    let hills = doc.stack.find(hills_id).expect("legacy hills survive");
    assert_eq!(hills.common.opacity, 0.625);
    let LayerKind::NoiseValue(params) = &hills.kind else {
        panic!("legacy hills kind changed");
    };
    assert_eq!(params.seed, 4242);
    assert_eq!(params.frequency, 0.0075);
    assert_eq!(params.amplitude, 137.25);
    assert_eq!(params.octaves, 3);
    assert_eq!(params.lacunarity, 2.25);
    assert_eq!(params.persistence, 0.45);
    assert_eq!(params.offset_x, 11.0);
    assert_eq!(params.offset_z, -7.5);
    assert_eq!(params.remap_min, -0.75);
    assert_eq!(params.remap_max, 0.9);

    let entry = hills
        .common
        .masks
        .entries
        .first()
        .expect("legacy mask reference survives");
    assert_eq!(hills.common.masks.entries.len(), 1);
    assert_eq!(entry.mask.id, height_mask_id);
    assert_eq!(entry.mask.strength, 0.7);
    assert!(entry.mask.invert);
    assert_eq!(entry.combine, MaskCombine::Multiply);

    assert_eq!(doc.masks.len(), 1);
    let mask = doc
        .masks
        .iter()
        .find(|mask| mask.id == height_mask_id)
        .expect("legacy mask asset survives");
    assert_eq!(mask.name, "Legacy Height Mask");
    assert!(matches!(
        mask.source,
        MaskSource::Height {
            min: 25.0,
            max: 275.0
        }
    ));
}

fn assert_fixture_round_trip(raw: &str, source_version: u32, expected_name: &str) {
    let raw_value: serde_json::Value = serde_json::from_str(raw).expect("valid fixture JSON");
    assert_eq!(raw_value["version"], source_version);

    let loaded = TerrainDocument::from_json(raw).expect("fixture must load");
    assert_fixture_semantics(&loaded, expected_name);
    let normalized_layer_ids = loaded.stack.layer_ids();

    let saved = loaded.to_json().expect("normalized fixture must save");
    let saved_value: serde_json::Value = serde_json::from_str(&saved).expect("valid saved JSON");
    assert_eq!(saved_value["version"], DOCUMENT_VERSION);
    assert!(DOCUMENT_VERSION >= 2);

    let reloaded = TerrainDocument::from_json(&saved).expect("saved fixture must reload");
    assert_fixture_semantics(&reloaded, expected_name);
    assert_eq!(reloaded.stack.layer_ids(), normalized_layer_ids);
}

#[test]
fn original_version_2_writer_fixture_loads_and_round_trips() {
    assert_fixture_round_trip(V2_FIXTURE, 2, "Version 2 compatibility fixture");
}

#[test]
fn regressed_version_1_writer_fixture_loads_and_round_trips() {
    assert_fixture_round_trip(V1_FIXTURE, 1, "Version 1 compatibility fixture");
}

#[test]
fn writer_never_emits_a_version_lower_than_2() {
    let mut doc = TerrainDocument::from_json(V1_FIXTURE).expect("version 1 fixture must load");
    doc.version = 1;
    let saved = doc.to_json().expect("document must save");
    let value: serde_json::Value = serde_json::from_str(&saved).expect("valid saved JSON");
    assert_eq!(value["version"], DOCUMENT_VERSION);
    assert!(DOCUMENT_VERSION >= 2);
}

#[test]
fn genuinely_future_version_is_rejected_clearly() {
    let mut value: serde_json::Value = serde_json::from_str(V2_FIXTURE).expect("valid fixture");
    value["version"] = serde_json::json!(DOCUMENT_VERSION + 1);
    let error = TerrainDocument::from_json(&serde_json::to_string(&value).unwrap())
        .expect_err("future version must be rejected");
    let message = error.to_string();
    assert!(message.contains(&format!(
        "unsupported document version {}",
        DOCUMENT_VERSION + 1
    )));
    assert!(message.contains(&format!("latest supported {DOCUMENT_VERSION}")));
}
