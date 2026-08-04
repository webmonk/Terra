use super::MaskField;
use serde::{Deserialize, Serialize};

/// Dense UV-space paint values stored with a mask asset.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PaintBuffer {
    pub width: u32,
    pub height: u32,
    pub samples: Vec<f32>,
}

impl PaintBuffer {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            samples: vec![0.0; (width * height) as usize],
        }
    }

    /// Stamps a soft circle centered at normalized terrain UV coordinates.
    pub fn stamp_circle(&mut self, u: f32, v: f32, radius_uv: f32, strength: f32, erase: bool) {
        if self.width == 0
            || self.height == 0
            || self.samples.len() != (self.width * self.height) as usize
        {
            return;
        }
        let radius = radius_uv.max(1e-6);
        let min_i = ((u - radius) * self.width as f32).floor().max(0.0) as u32;
        let max_i = ((u + radius) * self.width as f32)
            .ceil()
            .min(self.width as f32 - 1.0) as u32;
        let min_j = ((v - radius) * self.height as f32).floor().max(0.0) as u32;
        let max_j = ((v + radius) * self.height as f32)
            .ceil()
            .min(self.height as f32 - 1.0) as u32;
        for j in min_j..=max_j {
            for i in min_i..=max_i {
                let x = (i as f32 + 0.5) / self.width as f32;
                let y = (j as f32 + 0.5) / self.height as f32;
                let d = ((x - u).powi(2) + (y - v).powi(2)).sqrt() / radius;
                if d <= 1.0 {
                    let amount = (1.0 - d * d) * strength;
                    let sample = &mut self.samples[(j * self.width + i) as usize];
                    *sample = if erase {
                        *sample - amount
                    } else {
                        *sample + amount
                    }
                    .clamp(0.0, 1.0);
                }
            }
        }
    }

    pub fn sample_uv(&self, u: f32, v: f32) -> f32 {
        if self.width == 0
            || self.height == 0
            || self.samples.len() != (self.width * self.height) as usize
        {
            return 0.0;
        }
        let i = (u.clamp(0.0, 1.0) * self.width as f32) as u32;
        let j = (v.clamp(0.0, 1.0) * self.height as f32) as u32;
        self.samples[(j.min(self.height - 1) * self.width + i.min(self.width - 1)) as usize]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaintStroke {
    pub points: Vec<(f32, f32)>,
    pub radius: f32,
    pub strength: f32,
    pub erase: bool,
}

impl PaintStroke {
    pub fn apply(&self, field: &mut MaskField) {
        let dx = field.metrics.dx();
        let dz = field.metrics.dz();
        let r_samples = (self.radius / dx.min(dz)).max(1.0);
        for &(wx, wz) in &self.points {
            let (ci, cj) = field.metrics.sample_index(wx, wz);
            let r = r_samples as i32;
            for dj in -r..=r {
                for di in -r..=r {
                    let ii = ci as i32 + di;
                    let jj = cj as i32 + dj;
                    if ii < 0
                        || jj < 0
                        || ii >= field.metrics.width as i32
                        || jj >= field.metrics.height as i32
                    {
                        continue;
                    }
                    let dist = ((di * di + dj * dj) as f32).sqrt() / r_samples;
                    if dist > 1.0 {
                        continue;
                    }
                    let falloff = (1.0 - dist * dist) * self.strength;
                    let cur = field.get(ii as u32, jj as u32);
                    let next = if self.erase {
                        cur - falloff
                    } else {
                        cur + falloff
                    };
                    field.set(ii as u32, jj as u32, next);
                }
            }
        }
    }
}
