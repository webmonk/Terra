//! Deterministic seeding independent of dispatch order.

use crate::heightfield::TileId;
use crate::layer::LayerId;
use uuid::Uuid;

/// Mix world + layer seeds into a stable 64-bit layer seed.
pub fn layer_seed(world_seed: u64, layer_id: LayerId) -> u64 {
    mix64(world_seed ^ uuid_mix(layer_id.0))
}

/// Per-tile seed (does not depend on evaluation order).
pub fn tile_seed(world_seed: u64, layer_id: LayerId, tile: TileId) -> u64 {
    let base = layer_seed(world_seed, layer_id);
    mix64(base ^ ((tile.tx as u64) << 32) ^ (tile.tz as u64))
}

/// Per-sample seed for stochastic kernels.
pub fn sample_seed(world_seed: u64, layer_id: LayerId, tile: TileId, i: u32, j: u32) -> u64 {
    let base = tile_seed(world_seed, layer_id, tile);
    mix64(base ^ ((i as u64) << 32) ^ (j as u64))
}

fn uuid_mix(id: Uuid) -> u64 {
    let b = id.as_bytes();
    let mut x = 0u64;
    for (k, byte) in b.iter().enumerate() {
        x ^= (*byte as u64).wrapping_shl(((k % 8) * 8) as u32);
    }
    mix64(x)
}

/// SplitMix64 finalizer — cheap, deterministic avalanche.
#[inline]
fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn sample_seeds_are_order_independent() {
        let layer = LayerId(Uuid::nil());
        let tile = TileId { tx: 3, tz: 7 };
        let a = sample_seed(42, layer, tile, 10, 20);
        let b = sample_seed(42, layer, tile, 10, 20);
        assert_eq!(a, b);
        assert_ne!(sample_seed(42, layer, tile, 11, 20), a);
        assert_ne!(tile_seed(42, layer, TileId { tx: 4, tz: 7 }), tile_seed(42, layer, tile));
    }
}
