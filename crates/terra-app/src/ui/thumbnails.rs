//! Cheap layer / region thumbnail slots. GPU-backed previews can replace these later.

use std::collections::HashMap;

use terra_core::layer::LayerId;

/// A small procedural thumbnail retained until real GPU thumbnails are available.
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
    /// Returns a deterministic, non-blocking thumbnail for `layer_id` (48×48).
    pub fn request_or_get(&mut self, layer_id: LayerId) -> &Thumbnail {
        let key = format!("layer:{}", layer_id.0);
        self.map
            .entry(key.clone())
            .or_insert_with(|| procedural_thumbnail(&key, 48))
    }
}

fn procedural_thumbnail(key: &str, size: u32) -> Thumbnail {
    let hash = key.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        hash.wrapping_mul(0x0100_0193) ^ u32::from(byte)
    });
    let base = [
        40 + ((hash >> 16) & 0x6f) as u8,
        48 + ((hash >> 8) & 0x5f) as u8,
        56 + (hash & 0x4f) as u8,
    ];
    let (width, height) = (size, size);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            // Soft vignette + diagonal ridge for a terrain-like placeholder.
            let nx = x as f32 / (width - 1) as f32;
            let ny = y as f32 / (height - 1) as f32;
            let ridge = (1.0 - (nx - ny).abs() * 1.4).clamp(0.25, 1.0);
            let vignette = 1.0 - ((nx - 0.5).powi(2) + (ny - 0.5).powi(2)) * 1.6;
            let shade = (0.45 + 0.55 * ridge * vignette.clamp(0.35, 1.0)).clamp(0.2, 1.0);
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
