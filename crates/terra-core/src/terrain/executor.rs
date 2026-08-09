use crate::fields::FieldId;
use crate::generators::{path_stamp_tile, polygon_height_tile, sculpt_base_tile};
use crate::heightfield::{HeightTile, Heightfield};
use crate::layer::{Layer, LayerKind};
use thiserror::Error;

use super::TerrainTileKey;

#[derive(Debug, Error)]
pub enum TileExecutionError {
    #[error("tile field is not a heightfield")]
    UnsupportedField,
    #[error("layer does not yet have a tile processor")]
    UnsupportedLayer,
    #[error("tile belongs to a different layer")]
    LayerMismatch,
    #[error("tile is outside the requested terrain level")]
    MissingTile,
}

#[derive(Debug, Clone)]
pub struct TileExecutionOutput {
    pub key: TerrainTileKey,
    pub height: HeightTile,
    /// Local processors update interiors only; neighbouring ghost cells are exchanged separately.
    pub needs_halo_exchange: bool,
}

/// First real tiled execution path. Vector authoring tools use this now while other layer kinds
/// continue through the compatible whole-field evaluator until their processors are migrated.
pub fn execute_vector_height_tile(
    input: &Heightfield,
    layer: &Layer,
    key: TerrainTileKey,
) -> Result<TileExecutionOutput, TileExecutionError> {
    if key.layer.is_some_and(|id| id != layer.id()) {
        return Err(TileExecutionError::LayerMismatch);
    }
    if key.field != FieldId::Height {
        return Err(TileExecutionError::UnsupportedField);
    }
    let height = match &layer.kind {
        LayerKind::Path(params) => path_stamp_tile(input, params, key.tile).map(|tile| tile.height),
        LayerKind::SculptBase(params) => Some(sculpt_base_tile(input.metrics, params, key.tile)),
        LayerKind::PolygonHeight(params) => polygon_height_tile(input, params, key.tile),
        _ => return Err(TileExecutionError::UnsupportedLayer),
    }
    .ok_or(TileExecutionError::MissingTile)?;

    Ok(TileExecutionOutput {
        key,
        height,
        needs_halo_exchange: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generators::{polygon_height, sculpt_base};
    use crate::heightfield::{HeightfieldMetrics, TileId};
    use crate::layer::{PolygonHeightMode, PolygonHeightParams, SculptParams};

    #[test]
    fn polygon_tile_matches_legacy_full_field_interior() {
        let metrics = HeightfieldMetrics::new(512, 512, 4096.0, 4096.0);
        let input = Heightfield::filled(metrics, 100.0);
        let params = PolygonHeightParams {
            points: vec![[0.1, 0.1], [0.45, 0.1], [0.45, 0.45], [0.1, 0.45]],
            height: 60.0,
            falloff: 0.02,
            carve: false,
            mode: PolygonHeightMode::RaiseBy,
        };
        let layer = Layer::new("Plateau boundary", LayerKind::PolygonHeight(params.clone()));
        let key = TerrainTileKey {
            layer: Some(layer.id()),
            field: FieldId::Height,
            level: 0,
            tile: TileId { tx: 0, tz: 0 },
        };
        let tile = execute_vector_height_tile(&input, &layer, key)
            .expect("polygon tile execution")
            .height;
        let full = polygon_height(&input, &params);
        let (origin_x, origin_z) = tile.interior_origin(&metrics);
        for lz in 0..tile.interior_height {
            for lx in 0..tile.interior_width {
                assert_eq!(
                    tile.get_interior(lx, lz),
                    full.get(origin_x + lx, origin_z + lz)
                );
            }
        }
    }

    #[test]
    fn sculpt_tile_matches_composed_sculpt_heightfield() {
        let metrics = HeightfieldMetrics::new(512, 512, 4096.0, 4096.0);
        let input = Heightfield::zeros(metrics);
        let params = SculptParams::filled(64, 42.0);
        let layer = Layer::new("Sculpt", LayerKind::SculptBase(params.clone()));
        let id = TileId { tx: 1, tz: 1 };
        let key = TerrainTileKey {
            layer: Some(layer.id()),
            field: FieldId::Height,
            level: 0,
            tile: id,
        };
        let tile = execute_vector_height_tile(&input, &layer, key)
            .expect("sculpt tile execution")
            .height;
        let full = sculpt_base(metrics, &params);
        let (origin_x, origin_z) = tile.interior_origin(&metrics);
        for lz in 0..tile.interior_height {
            for lx in 0..tile.interior_width {
                assert_eq!(
                    tile.get_interior(lx, lz),
                    full.get(origin_x + lx, origin_z + lz)
                );
            }
        }
    }
    #[test]
    fn executor_rejects_a_tile_key_from_another_layer() {
        let metrics = HeightfieldMetrics::new(256, 256, 1024.0, 1024.0);
        let input = Heightfield::zeros(metrics);
        let layer = Layer::new(
            "Sculpt",
            LayerKind::SculptBase(SculptParams::filled(32, 5.0)),
        );
        let key = TerrainTileKey {
            layer: Some(crate::layer::LayerId::new()),
            field: FieldId::Height,
            level: 0,
            tile: TileId { tx: 0, tz: 0 },
        };
        assert!(matches!(
            execute_vector_height_tile(&input, &layer, key),
            Err(TileExecutionError::LayerMismatch)
        ));
    }
}
