//! GeoTIFF / DEM import bridge.
//!
//! Full GDAL bindings are feature-gated for native builds. Without GDAL, we accept
//! GeoTIFF files that `image` can decode as grayscale (common DEM exports) and
//! document CRS as unknown.

use crate::IoError;
use terra_core::heightfield::{Heightfield, HeightfieldMetrics};

#[derive(Debug, Clone)]
pub struct GeoTiffInfo {
    pub width: u32,
    pub height: u32,
    pub has_crs: bool,
    pub note: String,
}

pub fn read_geotiff_heights(
    path: &std::path::Path,
    world_size_x: f32,
    world_size_z: f32,
    height_scale: f32,
) -> Result<(Heightfield, GeoTiffInfo), IoError> {
    let img = image::open(path).map_err(|e| {
        IoError::Msg(format!(
            "GeoTIFF open failed ({e}). For full CRS/GDAL support, convert with gdal_translate or enable a native GDAL feature."
        ))
    })?;
    let g = img.to_luma16();
    let (w, h) = g.dimensions();
    let metrics = HeightfieldMetrics::new(w, h, world_size_x, world_size_z);
    let mut hf = Heightfield::zeros(metrics);
    for j in 0..h {
        for i in 0..w {
            let pix = g.get_pixel(i, j).0[0] as f32 / 65535.0;
            hf.set(i, j, pix * height_scale);
        }
    }
    let info = GeoTiffInfo {
        width: w,
        height: h,
        has_crs: false,
        note: "Decoded via image crate; CRS not parsed. Use GDAL for georeferencing.".into(),
    };
    Ok((hf, info))
}
