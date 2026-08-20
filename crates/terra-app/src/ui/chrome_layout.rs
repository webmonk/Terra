//! Terra editor chrome metrics and mode accents (product UI, not toolkit).

use terra_gui::Color;

pub const MODE_ROW_H: f32 = 52.0;
/// Thumbnail inside a tool card (logical px).
pub const TOOL_THUMB_SIZE: f32 = 64.0;
/// Full tool card height (thumb + label).
pub const TOOL_CARD_H: f32 = 96.0;
pub const TOOL_CARD_GAP: f32 = 6.0;
/// Min content width before falling back to a single card column.
pub const TOOL_CARD_2COL_MIN_W: f32 = 150.0;
pub const BRUSH_BAR_H: f32 = 48.0;
pub const VIEWPORT_TOOLBAR_H: f32 = 36.0;
/// Bottom viewport tool-mode bar (Move / Sculpt / Mask / ...).
pub const VIEWPORT_TOOL_MODE_BAR_H: f32 = 56.0;

pub const LAYER_ROW_H: f32 = 46.0;
/// Compact layer row when the right rail is narrow.
pub const LAYER_ROW_H_COMPACT: f32 = 36.0;
/// Thumbnail size inside a layer row.
pub const LAYER_THUMB_SZ: f32 = 28.0;

/// Bottom-left scale bar length (logical px).
pub const SCALE_BAR_W: f32 = 48.0;
pub const SCALE_BAR_H: f32 = 18.0;
/// Top-right view gizmo size.
pub const VIEW_GIZMO_SIZE: f32 = 72.0;
pub const MODE_RAIL_BG: Color = Color::rgb(0.050, 0.055, 0.075);

/// Mode-rail icon accents (unselected).
pub const MODE_TERRAIN: Color = Color::rgb(0.55, 0.62, 0.78);
pub const MODE_BIOMES: Color = Color::rgb(0.42, 0.82, 0.48);
pub const MODE_SIMULATION: Color = Color::rgb(0.35, 0.78, 0.82);
pub const MODE_MATERIALS: Color = Color::rgb(0.95, 0.62, 0.32);
pub const MODE_OBJECTS: Color = Color::rgb(0.70, 0.48, 0.90);
pub const MODE_MASKS: Color = Color::rgb(0.55, 0.68, 0.98);
pub const MODE_UTILITIES: Color = Color::rgb(0.62, 0.66, 0.74);
