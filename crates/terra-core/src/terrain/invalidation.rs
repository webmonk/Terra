use crate::fields::FieldId;
use crate::heightfield::{HeightfieldMetrics, TileId};
use crate::layer::{Layer, LayerId, LayerKind, LayerStack};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Normalized, inclusive-exclusive terrain footprint.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct NormalizedRect {
    pub u0: f32,
    pub v0: f32,
    pub u1: f32,
    pub v1: f32,
}

impl NormalizedRect {
    pub const FULL: Self = Self {
        u0: 0.0,
        v0: 0.0,
        u1: 1.0,
        v1: 1.0,
    };

    pub fn new(u0: f32, v0: f32, u1: f32, v1: f32) -> Option<Self> {
        let rect = Self {
            u0: u0.min(u1).clamp(0.0, 1.0),
            v0: v0.min(v1).clamp(0.0, 1.0),
            u1: u0.max(u1).clamp(0.0, 1.0),
            v1: v0.max(v1).clamp(0.0, 1.0),
        };
        (!rect.is_empty()).then_some(rect)
    }

    pub fn is_empty(self) -> bool {
        self.u1 <= self.u0 || self.v1 <= self.v0
    }

    pub fn intersects(self, other: Self) -> bool {
        self.u0 < other.u1 && self.u1 > other.u0 && self.v0 < other.v1 && self.v1 > other.v0
    }

    pub fn touches(self, other: Self) -> bool {
        const EPS: f32 = 1.0e-6;
        self.u0 <= other.u1 + EPS
            && self.u1 + EPS >= other.u0
            && self.v0 <= other.v1 + EPS
            && self.v1 + EPS >= other.v0
    }

    pub fn union(self, other: Self) -> Self {
        Self {
            u0: self.u0.min(other.u0),
            v0: self.v0.min(other.v0),
            u1: self.u1.max(other.u1),
            v1: self.v1.max(other.v1),
        }
    }

    pub fn inflate_world(self, radius_m: f32, world_size: (f32, f32)) -> Self {
        let du = radius_m.max(0.0) / world_size.0.max(1.0e-3);
        let dv = radius_m.max(0.0) / world_size.1.max(1.0e-3);
        Self {
            u0: (self.u0 - du).max(0.0),
            v0: (self.v0 - dv).max(0.0),
            u1: (self.u1 + du).min(1.0),
            v1: (self.v1 + dv).min(1.0),
        }
    }
}

/// Sparse affected regions. Disjoint edits remain disjoint instead of becoming one large box.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RegionSet {
    rects: Vec<NormalizedRect>,
}

impl RegionSet {
    pub fn full() -> Self {
        Self {
            rects: vec![NormalizedRect::FULL],
        }
    }

    pub fn from_rect(rect: NormalizedRect) -> Self {
        Self { rects: vec![rect] }
    }

    pub fn rects(&self) -> &[NormalizedRect] {
        &self.rects
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    pub fn insert(&mut self, mut rect: NormalizedRect) {
        if rect.is_empty() {
            return;
        }
        let mut i = 0;
        while i < self.rects.len() {
            if rect.touches(self.rects[i]) {
                rect = rect.union(self.rects.swap_remove(i));
                i = 0;
            } else {
                i += 1;
            }
        }
        self.rects.push(rect);
    }

    pub fn union_with(&mut self, other: &Self) {
        for rect in &other.rects {
            self.insert(*rect);
        }
    }

    pub fn inflated(&self, radius_m: f32, world_size: (f32, f32)) -> Self {
        let mut out = Self::default();
        for rect in &self.rects {
            out.insert(rect.inflate_world(radius_m, world_size));
        }
        out
    }

    pub fn tiles(&self, metrics: &HeightfieldMetrics) -> Vec<TileId> {
        let mut ids = HashSet::new();
        if metrics.width == 0 || metrics.height == 0 || metrics.tile_size == 0 {
            return Vec::new();
        }
        for rect in &self.rects {
            let x0 = (rect.u0 * metrics.width as f32).floor() as u32;
            let z0 = (rect.v0 * metrics.height as f32).floor() as u32;
            let x1 = ((rect.u1 * metrics.width as f32).ceil() as u32)
                .saturating_sub(1)
                .min(metrics.width - 1);
            let z1 = ((rect.v1 * metrics.height as f32).ceil() as u32)
                .saturating_sub(1)
                .min(metrics.height - 1);
            let tx0 = x0.min(metrics.width - 1) / metrics.tile_size;
            let tz0 = z0.min(metrics.height - 1) / metrics.tile_size;
            let tx1 = x1 / metrics.tile_size;
            let tz1 = z1 / metrics.tile_size;
            for tz in tz0..=tz1 {
                for tx in tx0..=tx1 {
                    ids.insert(TileId { tx, tz });
                }
            }
        }
        let mut ids: Vec<_> = ids.into_iter().collect();
        ids.sort_by_key(|id| (id.tz, id.tx));
        ids
    }
}

/// Physical wavelengths affected by an edit or operation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrequencyBand {
    pub min_wavelength_m: f32,
    pub max_wavelength_m: f32,
}

impl FrequencyBand {
    pub const ALL: Self = Self {
        min_wavelength_m: 0.0,
        max_wavelength_m: f32::INFINITY,
    };

    pub fn new(min_wavelength_m: f32, max_wavelength_m: f32) -> Self {
        Self {
            min_wavelength_m: min_wavelength_m.max(0.0).min(max_wavelength_m.max(0.0)),
            max_wavelength_m: max_wavelength_m.max(min_wavelength_m).max(0.0),
        }
    }

    /// A sampled level contributes when it can represent at least part of this band.
    pub fn contributes_at(self, sample_spacing_m: f32) -> bool {
        self.max_wavelength_m >= sample_spacing_m.max(0.0) * 2.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum OperationLocality {
    Local {
        radius_m: f32,
    },
    Expanding {
        radius_m_per_iteration: f32,
        iterations: u32,
    },
    Watershed {
        boundary_pad_m: f32,
    },
    Global,
}

impl OperationLocality {
    fn expand(self, regions: &RegionSet, world_size: (f32, f32)) -> RegionSet {
        match self {
            Self::Local { radius_m } => regions.inflated(radius_m, world_size),
            Self::Expanding {
                radius_m_per_iteration,
                iterations,
            } => regions.inflated(radius_m_per_iteration * iterations as f32, world_size),
            Self::Watershed { boundary_pad_m } => regions.inflated(boundary_pad_m, world_size),
            Self::Global => RegionSet::full(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct OperationDescriptor {
    pub layer: LayerId,
    pub bounds: RegionSet,
    pub frequency: FrequencyBand,
    pub min_level: u8,
    pub max_level: u8,
    pub locality: OperationLocality,
    pub required: Vec<FieldId>,
    pub produced: Vec<FieldId>,
}

impl OperationDescriptor {
    pub fn from_layer(layer: &Layer, base: HeightfieldMetrics, max_level: u8) -> Self {
        Self {
            layer: layer.id(),
            bounds: layer_bounds(layer, base),
            frequency: frequency_for(&layer.kind, base),
            min_level: 0,
            max_level,
            locality: locality_for(&layer.kind, base),
            required: layer.kind.required_fields(),
            produced: layer.kind.produced_fields(),
        }
    }
}

fn layer_bounds(layer: &Layer, base: HeightfieldMetrics) -> RegionSet {
    match &layer.kind {
        LayerKind::Path(p) if !p.nodes.is_empty() => {
            let (mut u0, mut v0, mut u1, mut v1) = (1.0f32, 1.0f32, 0.0f32, 0.0f32);
            for node in &p.nodes {
                u0 = u0.min(node.u);
                v0 = v0.min(node.v);
                u1 = u1.max(node.u);
                v1 = v1.max(node.v);
            }
            let pad = 0.0;
            NormalizedRect::new(u0, v0, u1, v1)
                .map(|rect| {
                    RegionSet::from_rect(
                        rect.inflate_world(pad, (base.world_size_x, base.world_size_z)),
                    )
                })
                .unwrap_or_default()
        }
        LayerKind::PolygonHeight(p) if p.points.len() >= 3 => {
            let (mut u0, mut v0, mut u1, mut v1) = (1.0f32, 1.0f32, 0.0f32, 0.0f32);
            for point in &p.points {
                u0 = u0.min(point[0]);
                v0 = v0.min(point[1]);
                u1 = u1.max(point[0]);
                v1 = v1.max(point[1]);
            }
            let pad_m = 0.0;
            NormalizedRect::new(u0, v0, u1, v1)
                .map(|rect| {
                    RegionSet::from_rect(
                        rect.inflate_world(pad_m, (base.world_size_x, base.world_size_z)),
                    )
                })
                .unwrap_or_default()
        }
        _ => RegionSet::full(),
    }
}

fn locality_for(kind: &LayerKind, base: HeightfieldMetrics) -> OperationLocality {
    match kind {
        LayerKind::Path(p) => {
            let max_width_scale = p
                .nodes
                .iter()
                .map(|node| node.width.max(0.0))
                .fold(1.0f32, f32::max);
            OperationLocality::Local {
                radius_m: p.width.max(0.0) * max_width_scale
                    + p.falloff.max(0.0)
                    + p.noise_strength.abs(),
            }
        }
        LayerKind::PolygonHeight(p) => OperationLocality::Local {
            radius_m: p.falloff.max(0.0) * base.world_size_x.min(base.world_size_z),
        },
        LayerKind::Blur(p) => OperationLocality::Local {
            radius_m: p.radius as f32 * base.dx().max(base.dz()),
        },
        LayerKind::ThermalErosion(p) => OperationLocality::Expanding {
            radius_m_per_iteration: base.dx().max(base.dz()),
            iterations: p.iterations,
        },
        LayerKind::HydraulicErosion(p) => OperationLocality::Expanding {
            radius_m_per_iteration: base.dx().max(base.dz()),
            iterations: p.iterations,
        },
        LayerKind::StreamPowerErosion(_)
        | LayerKind::RiverCarve(_)
        | LayerKind::RiverNetwork(_)
        | LayerKind::MultiScaleAmplify(_) => OperationLocality::Watershed {
            boundary_pad_m: base.tile_size as f32 * base.dx().max(base.dz()),
        },
        // Most generators and modifiers are pointwise even when their authored bounds are global.
        // They propagate an upstream local change without expanding it.
        _ => OperationLocality::Local { radius_m: 0.0 },
    }
}

fn frequency_for(kind: &LayerKind, base: HeightfieldMetrics) -> FrequencyBand {
    let cell = base.dx().max(base.dz());
    match kind {
        LayerKind::Path(p) => FrequencyBand::new(cell, (p.width + p.falloff * 2.0).max(cell)),
        LayerKind::PolygonHeight(p) => FrequencyBand::new(
            cell,
            (p.falloff * base.world_size_x.min(base.world_size_z)).max(cell),
        ),
        LayerKind::Blur(p) => FrequencyBand::new(cell, cell * p.radius.max(1) as f32 * 2.0),
        _ => FrequencyBand::ALL,
    }
}

#[derive(Debug, Clone)]
pub struct DirtyEvent {
    pub source: LayerId,
    pub fields: Vec<FieldId>,
    pub regions: RegionSet,
    pub frequency: FrequencyBand,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InvalidationKey {
    pub layer: LayerId,
    pub field: FieldId,
    pub level: u8,
    pub tile: TileId,
}

#[derive(Debug, Clone, Default)]
pub struct InvalidationSet {
    keys: HashSet<InvalidationKey>,
}

impl InvalidationSet {
    pub fn keys(&self) -> impl Iterator<Item = &InvalidationKey> {
        self.keys.iter()
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub fn insert(&mut self, key: InvalidationKey) {
        self.keys.insert(key);
    }

    pub fn propagate(
        event: &DirtyEvent,
        stack: &LayerStack,
        level_metrics: &[HeightfieldMetrics],
    ) -> Self {
        if level_metrics.is_empty() {
            return Self::default();
        }
        let layers = stack.flatten_layers();
        let Some(start) = layers.iter().position(|layer| layer.id() == event.source) else {
            return Self::default();
        };
        let mut out = Self::default();
        let mut regions = event.regions.clone();
        let mut fields = event.fields.clone();
        let max_level = level_metrics.len().saturating_sub(1) as u8;
        for layer in layers.into_iter().skip(start) {
            let descriptor = OperationDescriptor::from_layer(
                layer,
                level_metrics[max_level as usize],
                max_level,
            );
            regions = descriptor.locality.expand(
                &regions,
                (
                    level_metrics[max_level as usize].world_size_x,
                    level_metrics[max_level as usize].world_size_z,
                ),
            );
            if !descriptor.produced.is_empty() {
                fields = descriptor.produced.clone();
            }
            for (level, metrics) in level_metrics.iter().enumerate() {
                let level = level as u8;
                if level < descriptor.min_level || level > descriptor.max_level {
                    continue;
                }
                if !event
                    .frequency
                    .contributes_at(metrics.dx().max(metrics.dz()))
                    || !descriptor
                        .frequency
                        .contributes_at(metrics.dx().max(metrics.dz()))
                {
                    continue;
                }
                for tile in regions.tiles(metrics) {
                    for field in &fields {
                        out.insert(InvalidationKey {
                            layer: layer.id(),
                            field: field.clone(),
                            level,
                            tile,
                        });
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layer::{BlendMode, FlatParams, Layer, PathNode, PathParams};

    #[test]
    fn region_set_preserves_disjoint_rectangles() {
        let mut regions = RegionSet::default();
        regions.insert(NormalizedRect::new(0.0, 0.0, 0.1, 0.1).unwrap());
        regions.insert(NormalizedRect::new(0.9, 0.9, 1.0, 1.0).unwrap());
        assert_eq!(regions.rects().len(), 2);
    }

    #[test]
    fn path_bounds_touch_only_local_tiles() {
        let metrics = HeightfieldMetrics::new(1024, 1024, 4096.0, 4096.0);
        let layer = Layer::new(
            "Path",
            LayerKind::Path(PathParams {
                nodes: vec![
                    PathNode {
                        u: 0.1,
                        v: 0.1,
                        height: 0.0,
                        width: 1.0,
                    },
                    PathNode {
                        u: 0.2,
                        v: 0.2,
                        height: 10.0,
                        width: 1.0,
                    },
                ],
                width: 20.0,
                falloff: 10.0,
                ..Default::default()
            }),
        );
        let descriptor = OperationDescriptor::from_layer(&layer, metrics, 0);
        let tiles = descriptor.bounds.tiles(&metrics);
        assert!(!tiles.is_empty());
        assert!(tiles.len() < metrics.tile_count() as usize);
    }

    #[test]
    fn propagation_keeps_unrelated_world_tiles_clean() {
        let metrics = HeightfieldMetrics::new(1024, 1024, 4096.0, 4096.0);
        let mut stack = LayerStack::new();
        let path = Layer::new("Path", LayerKind::Path(PathParams::default()));
        let source = path.id();
        stack.push(path);
        let mut upper = Layer::new("Upper", LayerKind::Flat(FlatParams { height: 1.0 }));
        upper.common.blend = BlendMode::Add;
        stack.push(upper);
        let event = DirtyEvent {
            source,
            fields: vec![FieldId::Height],
            regions: RegionSet::from_rect(NormalizedRect::new(0.1, 0.1, 0.2, 0.2).unwrap()),
            frequency: FrequencyBand::ALL,
            revision: 1,
        };
        let invalid = InvalidationSet::propagate(&event, &stack, &[metrics]);
        assert!(!invalid.is_empty());
        assert!(invalid
            .keys()
            .all(|key| key.tile.tx <= 1 && key.tile.tz <= 1));
    }
}
