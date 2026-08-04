use crate::IoError;
use std::path::PathBuf;
use terra_core::eval::EvalContext;
use terra_core::heightfield::Heightfield;

pub struct ExportRequest {
    pub out_dir: PathBuf,
    pub include_masks: bool,
}

pub struct ExportResult {
    pub height_path: PathBuf,
    pub height_metadata_path: PathBuf,
    pub raw_path: PathBuf,
    pub raw_metadata_path: PathBuf,
    pub mask_paths: Vec<PathBuf>,
    pub hash: u64,
}

#[derive(serde::Serialize)]
struct HeightMetadata {
    min: f32,
    max: f32,
    width: u32,
    height: u32,
    world_size_x: f32,
    world_size_z: f32,
    dx: f32,
    dz: f32,
}

pub fn export_package(
    hf: &Heightfield,
    ctx: &EvalContext,
    req: &ExportRequest,
) -> Result<ExportResult, IoError> {
    std::fs::create_dir_all(&req.out_dir)?;
    let height_path = req.out_dir.join("height.png");
    write_height_png(hf, &height_path)?;
    let height_metadata_path = req.out_dir.join("height_meta.json");
    write_height_metadata(hf, &height_metadata_path)?;
    let mut mask_paths = Vec::new();
    if req.include_masks {
        for (name, mask) in &ctx.aux {
            let path = req.out_dir.join(format!("mask_{name}.png"));
            write_mask_png(mask, &path)?;
            mask_paths.push(path);
        }
    }
    let hash = hash_heights(hf);
    // Also write RAW f32
    let raw_path = req.out_dir.join("height.r32");
    let dense = hf.to_dense();
    let bytes: Vec<u8> = dense.iter().flat_map(|f| f.to_le_bytes()).collect();
    std::fs::write(&raw_path, bytes)?;
    let raw_metadata_path = req.out_dir.join("height.r32.meta.json");
    write_height_metadata(hf, &raw_metadata_path)?;
    Ok(ExportResult {
        height_path,
        height_metadata_path,
        raw_path,
        raw_metadata_path,
        mask_paths,
        hash,
    })
}

fn write_height_metadata(hf: &Heightfield, path: &std::path::Path) -> Result<(), IoError> {
    let (min, max) = hf.min_max();
    let metrics = hf.metrics;
    let metadata = HeightMetadata {
        min,
        max,
        width: metrics.width,
        height: metrics.height,
        world_size_x: metrics.world_size_x,
        world_size_z: metrics.world_size_z,
        dx: metrics.dx(),
        dz: metrics.dz(),
    };
    std::fs::write(path, serde_json::to_vec_pretty(&metadata)?)?;
    Ok(())
}

pub fn write_height_png(hf: &Heightfield, path: &std::path::Path) -> Result<(), IoError> {
    let (min_h, max_h) = hf.min_max();
    let span = (max_h - min_h).max(1e-6);
    let w = hf.metrics.width;
    let h = hf.metrics.height;
    let mut img = image::GrayImage::new(w, h);
    for j in 0..h {
        for i in 0..w {
            let t = ((hf.get(i, j) - min_h) / span).clamp(0.0, 1.0);
            img.put_pixel(i, j, image::Luma([(t * 255.0) as u8]));
        }
    }
    // Prefer 16-bit
    let mut img16 = image::ImageBuffer::<image::Luma<u16>, Vec<u16>>::new(w, h);
    for j in 0..h {
        for i in 0..w {
            let t = ((hf.get(i, j) - min_h) / span).clamp(0.0, 1.0);
            img16.put_pixel(i, j, image::Luma([(t * 65535.0) as u16]));
        }
    }
    img16.save(path)?;
    let _ = img;
    Ok(())
}

pub fn write_mask_png(
    mask: &terra_core::mask::MaskField,
    path: &std::path::Path,
) -> Result<(), IoError> {
    let w = mask.metrics.width;
    let h = mask.metrics.height;
    let mut img = image::GrayImage::new(w, h);
    for j in 0..h {
        for i in 0..w {
            img.put_pixel(i, j, image::Luma([(mask.get(i, j) * 255.0) as u8]));
        }
    }
    img.save(path)?;
    Ok(())
}

pub fn hash_heights(hf: &Heightfield) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for &v in &hf.to_dense() {
        let bits = v.to_bits() as u64;
        h ^= bits;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use terra_core::heightfield::HeightfieldMetrics;

    #[test]
    fn hash_deterministic() {
        let m = HeightfieldMetrics::new(8, 8, 8.0, 8.0);
        let hf = Heightfield::filled(m, 3.5);
        assert_eq!(hash_heights(&hf), hash_heights(&hf));
    }
}
