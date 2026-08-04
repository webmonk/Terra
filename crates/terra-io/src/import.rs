use crate::IoError;
use terra_core::heightfield::{Heightfield, HeightfieldMetrics};

pub fn import_heightmap_png(
    path: &std::path::Path,
    metrics: HeightfieldMetrics,
    height_scale: f32,
    height_offset: f32,
) -> Result<Heightfield, IoError> {
    let img = image::open(path)?.to_luma16();
    let (iw, ih) = img.dimensions();
    let mut hf = Heightfield::zeros(metrics);
    for j in 0..metrics.height {
        for i in 0..metrics.width {
            let u = i as f32 / metrics.width as f32;
            let v = j as f32 / metrics.height as f32;
            let x = ((u * iw as f32) as u32).min(iw - 1);
            let y = ((v * ih as f32) as u32).min(ih - 1);
            let pix = img.get_pixel(x, y).0[0] as f32 / 65535.0;
            hf.set(i, j, pix * height_scale + height_offset);
        }
    }
    hf.refresh_halos();
    Ok(hf)
}

pub fn import_heightmap_raw(
    path: &std::path::Path,
    metrics: HeightfieldMetrics,
) -> Result<Heightfield, IoError> {
    let bytes = std::fs::read(path)?;
    let expected = (metrics.width * metrics.height) as usize * 4;
    if bytes.len() < expected {
        return Err(IoError::Msg(format!(
            "RAW too small: {} < {}",
            bytes.len(),
            expected
        )));
    }
    let mut data = Vec::with_capacity(expected / 4);
    for chunk in bytes.chunks_exact(4).take(expected / 4) {
        data.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    Ok(Heightfield::from_dense(metrics, &data))
}
