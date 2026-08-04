//! Cheap layer thumbnail slots. GPU-backed thumbnails can replace these later.

use std::collections::HashMap;

use terra_core::layer::LayerId;

/// A small procedural thumbnail retained per layer until real GPU thumbnails are available.
#[derive(Debug, Clone)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug, Default)]
pub struct ThumbnailCache {
    map: HashMap<String, Thumbnail>,
}

impl ThumbnailCache {
    /// Returns a deterministic, non-blocking 32×32 thumbnail placeholder for `layer_id`.
    ///
    /// This is intentionally synchronous and cheap. Async GPU-generated layer previews are
    /// future work; the stored pixels preserve the slot/API shape without stalling the UI.
    pub fn request_or_get(&mut self, layer_id: LayerId) -> &Thumbnail {
        let key = layer_id.0.to_string();
        self.map
            .entry(key.clone())
            .or_insert_with(|| procedural_thumbnail(&key))
    }
}

fn procedural_thumbnail(key: &str) -> Thumbnail {
    let hash = key.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        hash.wrapping_mul(0x0100_0193) ^ u32::from(byte)
    });
    let base = [
        48 + ((hash >> 16) & 0x5f) as u8,
        48 + ((hash >> 8) & 0x5f) as u8,
        48 + (hash & 0x5f) as u8,
    ];
    let (width, height) = (32, 32);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let shade = 0.62 + 0.38 * ((x + y) as f32 / (width + height - 2) as f32);
            rgba.extend(base.map(|channel| (f32::from(channel) * shade) as u8));
            rgba.push(255);
        }
    }
    Thumbnail {
        width,
        height,
        rgba,
    }
}
