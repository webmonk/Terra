//! Layer thumbnail cache: real shaded-relief previews rendered from the
//! evaluator's per-layer cached heights, with a deterministic procedural
//! placeholder until a layer's first evaluation lands.

use std::collections::HashMap;

use terra_core::layer::LayerId;
use terra_core::Heightfield;

pub const LAYER_THUMB_RES: u32 = 48;

#[derive(Debug, Clone)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

#[derive(Debug)]
struct Entry {
    thumb: Thumbnail,
    /// Evaluator cache generation the thumb was rendered from; `None` for
    /// procedural placeholders (always eligible for replacement).
    generation: Option<u64>,
}

#[derive(Debug, Default)]
pub struct ThumbnailCache {
    map: HashMap<String, Entry>,
}

impl ThumbnailCache {
    /// Returns the layer's thumbnail, falling back to a deterministic
    /// placeholder until a real preview is inserted.
    pub fn request_or_get(&mut self, layer_id: LayerId) -> &Thumbnail {
        let key = layer_key(layer_id);
        &self
            .map
            .entry(key.clone())
            .or_insert_with(|| Entry {
                thumb: procedural_thumbnail(&key, LAYER_THUMB_RES),
                generation: None,
            })
            .thumb
    }

    /// True when the cached thumb is a placeholder or from an older eval
    /// generation than `generation`.
    pub fn needs_refresh(&self, layer_id: LayerId, generation: u64) -> bool {
        match self.map.get(&layer_key(layer_id)) {
            Some(entry) => entry.generation != Some(generation),
            None => true,
        }
    }

    /// Install a real preview rendered from evaluated height data.
    pub fn insert_real(&mut self, layer_id: LayerId, generation: u64, thumb: Thumbnail) {
        self.map.insert(
            layer_key(layer_id),
            Entry {
                thumb,
                generation: Some(generation),
            },
        );
    }

    /// Drop thumbs for layers that no longer exist (project switch / delete).
    /// Mask chips are content-fingerprinted and cheap; they persist here.
    pub fn retain_layers(&mut self, live: &[LayerId]) {
        let keys: std::collections::HashSet<String> =
            live.iter().map(|id| layer_key(*id)).collect();
        self.map
            .retain(|k, _| !k.starts_with("layer:") || keys.contains(k));
    }

    /// Grayscale chip for a painted mask, re-rendered whenever the buffer
    /// reports a mutation.
    ///
    /// This keys on the buffer's revision counter rather than a sampled
    /// content hash: any affordable stride can land on a multiple of the
    /// mask width and degenerate to reading a single column, so a whole
    /// brush stroke could leave the hash unchanged and the chip stale.
    pub fn mask_chip(
        &mut self,
        mask_id: terra_core::mask::MaskId,
        paint: &terra_core::mask::PaintBuffer,
    ) -> &Thumbnail {
        let key = format!("mask:{}", mask_id.0);
        let fp = paint_fingerprint(paint);
        let entry = self.map.entry(key).or_insert_with(|| Entry {
            thumb: render_mask_chip(paint, MASK_CHIP_RES),
            generation: Some(fp),
        });
        if entry.generation != Some(fp) {
            entry.thumb = render_mask_chip(paint, MASK_CHIP_RES);
            entry.generation = Some(fp);
        }
        &entry.thumb
    }
}

pub const MASK_CHIP_RES: u32 = 32;

/// Cache key for a painted mask: its dimensions plus the buffer's own
/// mutation counter, which every `PaintBuffer` mutator bumps.
fn paint_fingerprint(paint: &terra_core::mask::PaintBuffer) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for v in [
        paint.width as u64,
        paint.height as u64,
        paint.revision(),
    ] {
        hash ^= v;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn render_mask_chip(paint: &terra_core::mask::PaintBuffer, res: u32) -> Thumbnail {
    let (w, h) = (paint.width.max(1), paint.height.max(1));
    let mut rgba = Vec::with_capacity((res * res * 4) as usize);
    for ty in 0..res {
        for tx in 0..res {
            let i = (tx as u64 * (w as u64 - 1) / (res as u64 - 1).max(1)) as u32;
            let j = (ty as u64 * (h as u64 - 1) / (res as u64 - 1).max(1)) as u32;
            let v = paint.samples()[(j.min(h - 1) * w + i.min(w - 1)) as usize];
            let g = (v.clamp(0.0, 1.0) * 255.0) as u8;
            rgba.extend_from_slice(&[g, g, g, 255]);
        }
    }
    Thumbnail {
        width: res,
        height: res,
        rgba,
    }
}

fn layer_key(layer_id: LayerId) -> String {
    format!("layer:{}", layer_id.0)
}

/// Shaded-relief preview of a heightfield: hypsometric ramp modulated by a
/// simple NW-lit lambert term. Reads `res × res` samples at stride.
pub fn render_height_thumbnail(hf: &Heightfield, res: u32) -> Thumbnail {
    let (w, h) = (hf.metrics.width, hf.metrics.height);
    let res = res.max(2);
    let sample = |tx: u32, ty: u32| -> f32 {
        let i = (tx as u64 * (w.max(1) as u64 - 1) / (res as u64 - 1)) as u32;
        let j = (ty as u64 * (h.max(1) as u64 - 1) / (res as u64 - 1)) as u32;
        hf.get(i.min(w - 1), j.min(h - 1))
    };

    let mut heights = vec![0.0f32; (res * res) as usize];
    for ty in 0..res {
        for tx in 0..res {
            heights[(ty * res + tx) as usize] = sample(tx, ty);
        }
    }
    shade_height_samples(res, &heights, hf.metrics.world_size_x)
}

/// Shade an already-downsampled `res × res` row-major height grid (the form
/// the eval worker ships as [`terra_core::eval::LayerPreview`]).
pub fn shade_height_samples(res: u32, heights: &[f32], world_size_x: f32) -> Thumbnail {
    let res = res.max(2);
    debug_assert_eq!(heights.len(), (res * res) as usize);
    let (mut min, mut max) = (f32::INFINITY, f32::NEG_INFINITY);
    for &v in heights {
        min = min.min(v);
        max = max.max(v);
    }
    if !min.is_finite() {
        min = 0.0;
        max = 1.0;
    }
    let span = (max - min).max(1e-3);

    // Low → high: deep slate, moss, tan, grey rock, white.
    const RAMP: [[f32; 3]; 5] = [
        [0.16, 0.22, 0.28],
        [0.30, 0.42, 0.30],
        [0.58, 0.52, 0.38],
        [0.55, 0.55, 0.58],
        [0.92, 0.93, 0.95],
    ];
    let ramp = |t: f32| -> [f32; 3] {
        let t = t.clamp(0.0, 1.0) * (RAMP.len() - 1) as f32;
        let i = (t as usize).min(RAMP.len() - 2);
        let f = t - i as f32;
        [
            RAMP[i][0] + (RAMP[i + 1][0] - RAMP[i][0]) * f,
            RAMP[i][1] + (RAMP[i + 1][1] - RAMP[i][1]) * f,
            RAMP[i][2] + (RAMP[i + 1][2] - RAMP[i][2]) * f,
        ]
    };

    // World metres per thumb texel, for slope-consistent shading.
    let texel_m = (world_size_x / res as f32).max(1e-3);
    let at = |tx: i64, ty: i64| -> f32 {
        let tx = tx.clamp(0, res as i64 - 1) as u32;
        let ty = ty.clamp(0, res as i64 - 1) as u32;
        heights[(ty * res + tx) as usize]
    };

    let mut rgba = Vec::with_capacity((res * res * 4) as usize);
    for ty in 0..res as i64 {
        for tx in 0..res as i64 {
            let hc = at(tx, ty);
            let dx = (at(tx + 1, ty) - at(tx - 1, ty)) / (2.0 * texel_m);
            let dy = (at(tx, ty + 1) - at(tx, ty - 1)) / (2.0 * texel_m);
            // Lambert with NW light at 45 deg elevation.
            let len = (dx * dx + dy * dy + 1.0).sqrt();
            let light = ((-dx) * -0.5 + (-dy) * -0.5 + 0.707) / len;
            let shade = (0.35 + 0.75 * light.clamp(0.0, 1.4)).clamp(0.25, 1.15);
            let base = ramp((hc - min) / span);
            for c in base {
                rgba.push(((c * shade).clamp(0.0, 1.0) * 255.0) as u8);
            }
            rgba.push(255);
        }
    }
    Thumbnail {
        width: res,
        height: res,
        rgba,
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

#[cfg(test)]
mod tests {
    use super::*;
    use terra_core::HeightfieldMetrics;

    #[test]
    fn real_thumbnail_replaces_placeholder_and_tracks_generation() {
        let mut cache = ThumbnailCache::default();
        let id = LayerId::new();
        let placeholder = cache.request_or_get(id).rgba.clone();
        assert!(cache.needs_refresh(id, 1));

        let hf = Heightfield::filled(HeightfieldMetrics::new(16, 16, 64.0, 64.0), 10.0);
        cache.insert_real(id, 1, render_height_thumbnail(&hf, LAYER_THUMB_RES));
        assert!(!cache.needs_refresh(id, 1));
        assert!(cache.needs_refresh(id, 2));
        assert_ne!(cache.request_or_get(id).rgba, placeholder);
    }

    /// A stroke wholly between sampled columns must still refresh the chip.
    /// The previous strided content hash could read a single column (any
    /// stride that is a multiple of the mask width), so a full stroke left
    /// the chip stale; the buffer revision cannot miss.
    #[test]
    fn mask_chip_refreshes_for_a_stroke_between_sampled_columns() {
        use terra_core::mask::{MaskId, MaskPaintTool, PaintBuffer};
        // 512 wide: the old stride (len / 512) was exactly the width.
        for res in [256u32, 512, 1024] {
            let mut cache = ThumbnailCache::default();
            let id = MaskId::new();
            let mut paint = PaintBuffer::new(res, res);
            paint.fill();
            let before = cache.mask_chip(id, &paint).rgba.clone();
            // Erase a disc away from column 0 and the strided columns.
            paint.edit_circle(0.37, 0.37, 0.08, 1.0, 1.0, MaskPaintTool::Erase);
            assert_ne!(
                cache.mask_chip(id, &paint).rgba,
                before,
                "chip must refresh after a stroke at {res}x{res}"
            );
        }
    }

    #[test]
    fn mask_chip_refreshes_on_paint_change() {
        use terra_core::mask::{MaskId, MaskPaintTool, PaintBuffer};
        let mut cache = ThumbnailCache::default();
        let id = MaskId::new();
        let mut paint = PaintBuffer::new(64, 64);
        paint.fill();
        let before = cache.mask_chip(id, &paint).rgba.clone();
        assert!(before.iter().step_by(4).all(|&g| g == 255));

        paint.edit_circle(0.5, 0.5, 0.3, 1.0, 0.5, MaskPaintTool::Erase);
        let after = cache.mask_chip(id, &paint).rgba.clone();
        assert_ne!(before, after, "chip must re-render when paint changes");

        // Unchanged paint reuses the cached render.
        let again = cache.mask_chip(id, &paint).rgba.clone();
        assert_eq!(after, again);

        // Mask chips survive layer retention sweeps.
        cache.retain_layers(&[]);
        assert_eq!(cache.mask_chip(id, &paint).rgba, again);
    }

    #[test]
    fn shaded_relief_brightens_with_height() {
        let m = HeightfieldMetrics::new(32, 32, 64.0, 64.0);
        let mut hf = Heightfield::zeros(m);
        // Ramp from 0 (left) to 100 (right).
        for j in 0..32 {
            for i in 0..32 {
                hf.set(i, j, i as f32 * 100.0 / 31.0);
            }
        }
        hf.refresh_halos();
        let t = render_height_thumbnail(&hf, 16);
        let px = |x: u32, y: u32| t.rgba[((y * 16 + x) * 4) as usize] as i32;
        // High side of the ramp should read brighter than the low side.
        assert!(px(14, 8) > px(1, 8));
    }
}
