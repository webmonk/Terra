use super::HeightfieldMetrics;
use serde::{Deserialize, Serialize};

/// Tile coordinates in the grid (not sample indices).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TileId {
    pub tx: u32,
    pub tz: u32,
}

/// One height tile with interior samples plus a ghost halo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeightTile {
    pub id: TileId,
    pub interior_width: u32,
    pub interior_height: u32,
    pub halo: u32,
    /// Row-major including halo. Size = stride * stride_z.
    data: Vec<f32>,
}

impl HeightTile {
    pub fn new(id: TileId, metrics: &HeightfieldMetrics) -> Self {
        let origin_x = id.tx * metrics.tile_size;
        let origin_z = id.tz * metrics.tile_size;
        let interior_width = (metrics.width - origin_x).min(metrics.tile_size);
        let interior_height = (metrics.height - origin_z).min(metrics.tile_size);
        let halo = metrics.halo;
        let stride = interior_width + 2 * halo;
        let stride_z = interior_height + 2 * halo;
        Self {
            id,
            interior_width,
            interior_height,
            halo,
            data: vec![0.0; (stride * stride_z) as usize],
        }
    }

    pub fn stride(&self) -> u32 {
        self.interior_width + 2 * self.halo
    }

    pub fn stride_z(&self) -> u32 {
        self.interior_height + 2 * self.halo
    }

    pub fn interior_origin(&self, metrics: &HeightfieldMetrics) -> (u32, u32) {
        (
            self.id.tx * metrics.tile_size,
            self.id.tz * metrics.tile_size,
        )
    }

    #[inline]
    fn idx(&self, gx: i32, gz: i32) -> usize {
        let x = (gx + self.halo as i32) as u32;
        let z = (gz + self.halo as i32) as u32;
        (z * self.stride() + x) as usize
    }

    pub fn get_interior(&self, lx: u32, lz: u32) -> f32 {
        self.data[self.idx(lx as i32, lz as i32)]
    }

    pub fn set_interior(&mut self, lx: u32, lz: u32, value: f32) {
        let i = self.idx(lx as i32, lz as i32);
        self.data[i] = value;
    }

    /// Coordinates relative to interior: (0,0) is first interior sample; negatives are halo.
    pub fn get_with_halo(&self, gx: i32, gz: i32) -> f32 {
        self.data[self.idx(gx, gz)]
    }

    pub fn set_with_halo(&mut self, gx: i32, gz: i32, value: f32) {
        let i = self.idx(gx, gz);
        self.data[i] = value;
    }

    pub fn fill(&mut self, value: f32) {
        self.data.fill(value);
    }

    pub fn interior(&self) -> InteriorIter<'_> {
        InteriorIter {
            tile: self,
            lx: 0,
            lz: 0,
        }
    }

    pub fn map_interior<F: FnMut(f32) -> f32>(&mut self, mut f: F) {
        let halo = self.halo;
        let iw = self.interior_width;
        let ih = self.interior_height;
        let stride = self.stride();
        for lz in 0..ih {
            for lx in 0..iw {
                let idx = ((lz + halo) * stride + (lx + halo)) as usize;
                self.data[idx] = f(self.data[idx]);
            }
        }
    }

    /// Rewrite interior samples with access to local interior coordinates.
    pub fn map_interior_indexed<F: FnMut(u32, u32, f32) -> f32>(&mut self, mut f: F) {
        let halo = self.halo;
        let iw = self.interior_width;
        let ih = self.interior_height;
        let stride = self.stride();
        for lz in 0..ih {
            for lx in 0..iw {
                let idx = ((lz + halo) * stride + (lx + halo)) as usize;
                self.data[idx] = f(lx, lz, self.data[idx]);
            }
        }
    }

    pub fn data(&self) -> &[f32] {
        &self.data
    }
}

pub struct InteriorIter<'a> {
    tile: &'a HeightTile,
    lx: u32,
    lz: u32,
}

impl<'a> Iterator for InteriorIter<'a> {
    type Item = &'a f32;

    fn next(&mut self) -> Option<Self::Item> {
        if self.lz >= self.tile.interior_height {
            return None;
        }
        let v = &self.tile.data[self.tile.idx(self.lx as i32, self.lz as i32)];
        self.lx += 1;
        if self.lx >= self.tile.interior_width {
            self.lx = 0;
            self.lz += 1;
        }
        Some(v)
    }
}
