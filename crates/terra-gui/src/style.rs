//! Visual design tokens for the Terra editor (dark navy + single blue accent).

use crate::types::Color;

// —— Spacing / sizing ————————————————————————————————————————————————

pub const ROW_H: f32 = 28.0;
pub const LAYER_ROW_H: f32 = 46.0;
pub const PAD: f32 = 10.0;
/// Compact intra-control spacing.
pub const PAD_SM: f32 = 6.0;
/// 1.0 = baked IBM Plex Sans at 14px.
pub const FONT_SCALE: f32 = 1.0;
pub const GAP: f32 = 6.0;
pub const SEPARATOR_H: f32 = 1.0;
pub const HEADER_H: f32 = 36.0;
/// Thin bottom status bar height.
pub const STATUS_STRIP_H: f32 = 36.0;
pub const TOOLBAR_BTN_H: f32 = 30.0;
pub const SCROLLBAR_W: f32 = 8.0;
pub const SCROLLBAR_PAD: f32 = 3.0;
/// Legacy sculpt-only palette height estimate.
pub const TOOL_PALETTE_H: f32 = 140.0;
pub const LAYER_BLEND_H: f32 = 72.0;

/// Far-left workspace mode rail width (logical px).
pub const MODE_RAIL_W: f32 = 80.0;
/// Contextual tool panel width beside the mode rail.
pub const TOOL_PANEL_W: f32 = 190.0;
/// Total left chrome = mode rail + tool panel.
pub const LEFT_CHROME_W: f32 = MODE_RAIL_W + TOOL_PANEL_W;

pub const ICON_SM: f32 = 14.0;
pub const ICON_MD: f32 = 16.0;
pub const ICON_LG: f32 = 20.0;
pub const MODE_ROW_H: f32 = 56.0;
pub const TOOL_ROW_H: f32 = 44.0;
pub const BRUSH_BAR_H: f32 = 48.0;
pub const VIEWPORT_TOOLBAR_H: f32 = 34.0;
/// Top application bar height.
pub const APP_BAR_H: f32 = 56.0;
/// Right rail preferred width.
pub const RIGHT_PANEL_W: f32 = 360.0;
/// Minimap side length.
pub const MINIMAP_SIZE: f32 = 160.0;

/// Corner radius for chips / cards.
pub const RADIUS_SM: f32 = 5.0;
pub const RADIUS_MD: f32 = 8.0;
/// Pill radius (use half the control height for full capsules).
pub const RADIUS_PILL: f32 = 13.0;

// —— Colours ——————————————————————————————————————————————————————————

/// App / panel chrome (~#12151A).
pub const APP_BG: Color = Color::rgb(0.071, 0.082, 0.102);
/// Side panels (~#161A22).
pub const PANEL_BG: Color = Color::rgb(0.086, 0.102, 0.133);
/// Nested surfaces / rows (~#1C222C).
pub const SURFACE: Color = Color::rgb(0.110, 0.133, 0.173);
/// Raised panel / inset control surface.
pub const RAISED_BG: Color = Color::rgb(0.125, 0.150, 0.195);
/// Raised controls.
pub const BUTTON_BG: Color = Color::rgb(0.145, 0.173, 0.220);
pub const BUTTON_HOVER: Color = Color::rgb(0.180, 0.220, 0.280);
pub const BUTTON_ACTIVE: Color = Color::rgb(0.110, 0.145, 0.190);
/// Text / combo input fill.
pub const INPUT_BG: Color = Color::rgb(0.095, 0.112, 0.145);
/// Vibrant accent blue (selection, primary CTAs, slider fill).
pub const ACCENT: Color = Color::rgb(0.220, 0.510, 0.960);
pub const ACCENT_DIM: Color = Color::rgb(0.145, 0.310, 0.580);
pub const ACCENT_SOFT: Color = Color::rgba(0.220, 0.510, 0.960, 0.18);
pub const SELECTED_BG: Color = Color::rgb(0.120, 0.220, 0.400);
pub const HOVER_BG: Color = Color::rgb(0.140, 0.170, 0.220);
pub const TRACK_BG: Color = Color::rgb(0.090, 0.105, 0.130);
pub const THUMB_BG: Color = Color::rgb(0.95, 0.96, 0.98);
pub const THUMB_ACTIVE: Color = Color::rgb(1.0, 1.0, 1.0);
pub const CHECK_BG: Color = Color::rgb(0.120, 0.140, 0.175);
pub const CHECK_ON: Color = Color::rgb(0.220, 0.510, 0.960);

pub const TEXT: Color = Color::rgb(0.92, 0.94, 0.97);
pub const TEXT_DIM: Color = Color::rgb(0.55, 0.60, 0.68);
pub const TEXT_MUTED: Color = Color::rgb(0.40, 0.45, 0.52);
pub const TEXT_DISABLED: Color = Color::rgb(0.28, 0.32, 0.38);

pub const SUCCESS: Color = Color::rgb(0.35, 0.82, 0.55);
pub const WARNING: Color = Color::rgb(0.95, 0.72, 0.28);
pub const ERROR: Color = Color::rgb(0.92, 0.35, 0.38);
pub const DISABLED_BG: Color = Color::rgb(0.10, 0.12, 0.15);
pub const DISABLED_FG: Color = TEXT_DISABLED;

pub const SEPARATOR: Color = Color::rgba(1.0, 1.0, 1.0, 0.06);
pub const BORDER: Color = Color::rgba(1.0, 1.0, 1.0, 0.08);
pub const COMBO_MENU_BG: Color = Color::rgba(0.10, 0.12, 0.16, 0.98);
pub const TOOLBAR_BG: Color = Color::rgb(0.075, 0.088, 0.110);
pub const DOCK_BG: Color = Color::rgb(0.078, 0.090, 0.115);
pub const OVERLAY_BG: Color = Color::rgba(0.07, 0.085, 0.11, 0.94);
pub const VIEWPORT_TOOLBAR_BG: Color = Color::rgba(0.075, 0.090, 0.115, 0.96);
pub const MODE_RAIL_BG: Color = Color::rgb(0.055, 0.065, 0.085);
pub const POPUP_BG: Color = Color::rgba(0.09, 0.11, 0.15, 0.98);
pub const SCROLLBAR_TRACK: Color = Color::rgba(1.0, 1.0, 1.0, 0.04);
pub const SCROLLBAR_THUMB: Color = Color::rgba(1.0, 1.0, 1.0, 0.18);
pub const SCROLLBAR_THUMB_HOVER: Color = Color::rgba(1.0, 1.0, 1.0, 0.32);

/// Layer colour tags (user-assignable).
pub const TAG_RED: Color = Color::rgb(0.85, 0.35, 0.35);
pub const TAG_ORANGE: Color = Color::rgb(0.90, 0.55, 0.25);
pub const TAG_YELLOW: Color = Color::rgb(0.90, 0.80, 0.30);
pub const TAG_GREEN: Color = Color::rgb(0.40, 0.78, 0.45);
pub const TAG_BLUE: Color = Color::rgb(0.35, 0.55, 0.95);
pub const TAG_PURPLE: Color = Color::rgb(0.65, 0.45, 0.90);
pub const TAG_GRAY: Color = Color::rgb(0.50, 0.55, 0.60);
