//! Visual design tokens for the reusable wgpu IMGUI toolkit.
//!
//! Spacing follows an **8px grid**. Prefer `SPACE_*` / `CONTROL_*` / `INSP_*` tokens
//! from shared widgets so panels stay aligned without each call site inventing gaps.
//!
//! Product-specific chrome metrics (mode rail, tool cards, category accents) live in
//! the consuming app — not here.

use crate::types::Color;

// —— 8px spacing scale ————————————————————————————————————————————————

/// 4px — tight intra-row padding.
pub const SPACE_1: f32 = 4.0;
/// 8px — default gap between related controls.
pub const SPACE_2: f32 = 8.0;
/// 12px — section / panel padding.
pub const SPACE_3: f32 = 12.0;
/// 16px — generous block separation (inspector body, selection cards).
pub const SPACE_4: f32 = 16.0;
/// 24px — major section breaks.
pub const SPACE_5: f32 = 24.0;
/// 32px — rare page-level breathing room.
pub const SPACE_6: f32 = 32.0;

// —— Spacing / sizing ————————————————————————————————————————————————

pub const ROW_H: f32 = 28.0;
/// Outer chrome padding (panels, app bar insets).
pub const PAD: f32 = SPACE_3;
pub const PAD_SM: f32 = SPACE_1 + 2.0; // 6 — legacy half-step kept for tool chrome
/// 1.0 = baked IBM Plex Sans at 14px.
pub const FONT_SCALE: f32 = 1.0;
pub const GAP: f32 = SPACE_2;
pub const SEPARATOR_H: f32 = 1.0;
pub const HEADER_H: f32 = 36.0;
/// Thin bottom status bar height.
pub const STATUS_STRIP_H: f32 = 28.0;
pub const TOOLBAR_BTN_H: f32 = 30.0;
pub const SCROLLBAR_W: f32 = 8.0;
pub const SCROLLBAR_PAD: f32 = 3.0;
/// Default title / app bar height for docked shells.
pub const APP_BAR_H: f32 = 52.0;
/// Default right dock width for IDE-style layouts.
pub const RIGHT_PANEL_W: f32 = 340.0;

pub const ICON_SM: f32 = 14.0;
pub const ICON_MD: f32 = 16.0;
pub const ICON_LG: f32 = 20.0;

pub const RADIUS_SM: f32 = 6.0;
pub const RADIUS_MD: f32 = 8.0;
pub const RADIUS_LG: f32 = 10.0;
pub const RADIUS_PILL: f32 = 13.0;
/// Circular slider thumb diameter.
pub const SLIDER_THUMB: f32 = 14.0;

// —— Inspector / form control grid (label | control | value) ——————————

/// Fixed left column for property labels (aligned across all form rows).
pub const CONTROL_LABEL_W: f32 = 96.0;
/// Gutter between label column and control.
pub const CONTROL_GAP: f32 = SPACE_2;
/// Right-side numeric / unit value box width.
pub const CONTROL_VALUE_W: f32 = 56.0;
/// Row height for slider / combo / checkbox form rows.
pub const CONTROL_ROW_H: f32 = 32.0;
/// Inspector scrolled-body padding (breathable, matches reference).
pub const INSP_PAD: f32 = SPACE_4;
/// Selection identity card height (icon + name).
pub const INSP_SELECTION_H: f32 = 44.0;
/// Section header row height (SHAPE / DETAIL / …).
pub const INSP_SECTION_H: f32 = 28.0;

// —— Typography scale (multipliers on FONT_SCALE) ————————————————

/// Section labels / meta (GLOBAL, REGIONS, captions).
pub const TYPE_CAPTION: f32 = 0.72;
/// Secondary labels, opacity, chips.
pub const TYPE_LABEL: f32 = 0.85;
/// Default body / layer names.
pub const TYPE_BODY: f32 = 0.92;
/// Region card titles / panel titles.
pub const TYPE_TITLE: f32 = 1.0;

// —— Colours (charcoal + blue design system) ————————————————————

/// App / viewport surround (~#12141A).
pub const APP_BG: Color = Color::rgb(0.071, 0.078, 0.102);
/// Side panels (~#16181F).
pub const PANEL_BG: Color = Color::rgb(0.086, 0.094, 0.122);
/// Nested surfaces / rows (~#1C1F28).
pub const SURFACE: Color = Color::rgb(0.110, 0.122, 0.157);
pub const RAISED_BG: Color = Color::rgb(0.125, 0.140, 0.180);
pub const BUTTON_BG: Color = Color::rgb(0.165, 0.180, 0.230);
pub const BUTTON_HOVER: Color = Color::rgb(0.205, 0.225, 0.285);
pub const BUTTON_ACTIVE: Color = Color::rgb(0.125, 0.145, 0.195);
pub const INPUT_BG: Color = Color::rgb(0.095, 0.105, 0.140);

/// Primary action / selection blue (~#3B82F6).
pub const ACCENT: Color = Color::rgb(0.23, 0.51, 0.96);
pub const ACCENT_DIM: Color = Color::rgb(0.14, 0.32, 0.62);
pub const ACCENT_SOFT: Color = Color::rgba(0.23, 0.51, 0.96, 0.18);
/// Brighter accent for chip / primary-button hover.
pub const ACCENT_HOVER: Color = Color::rgb(0.30, 0.58, 1.0);
pub const SELECTED_BG: Color = Color::rgb(0.12, 0.22, 0.40);
pub const HOVER_BG: Color = Color::rgb(0.14, 0.16, 0.22);
/// Unified row hover (layers / lists) — prefer over raw SURFACE.
pub const ROW_HOVER: Color = HOVER_BG;
/// Procedural region card surface (distinct from folder CAT_* fills).
pub const REGION_CARD_BG: Color = Color::rgb(0.100, 0.112, 0.148);
pub const REGION_CARD_BG_HOVER: Color = Color::rgb(0.118, 0.132, 0.175);
pub const REGION_CARD_BORDER: Color = Color::rgba(1.0, 1.0, 1.0, 0.08);
pub const TRACK_BG: Color = Color::rgb(0.08, 0.09, 0.12);
pub const THUMB_BG: Color = Color::rgb(0.96, 0.97, 0.99);
pub const THUMB_ACTIVE: Color = Color::rgb(1.0, 1.0, 1.0);
pub const CHECK_BG: Color = Color::rgb(0.12, 0.14, 0.18);
pub const CHECK_ON: Color = Color::rgb(0.23, 0.51, 0.96);

pub const TEXT: Color = Color::rgb(0.95, 0.96, 0.98);
pub const TEXT_DIM: Color = Color::rgb(0.70, 0.74, 0.82);
pub const TEXT_MUTED: Color = Color::rgb(0.52, 0.56, 0.64);
pub const TEXT_DISABLED: Color = Color::rgb(0.34, 0.37, 0.44);

pub const SUCCESS: Color = Color::rgb(0.35, 0.82, 0.55);
pub const WARNING: Color = Color::rgb(0.95, 0.72, 0.28);
pub const ERROR: Color = Color::rgb(0.92, 0.35, 0.38);
pub const DISABLED_BG: Color = Color::rgb(0.10, 0.11, 0.14);
pub const DISABLED_FG: Color = TEXT_DISABLED;

pub const SEPARATOR: Color = Color::rgba(1.0, 1.0, 1.0, 0.12);
pub const BORDER: Color = Color::rgba(1.0, 1.0, 1.0, 0.14);
pub const COMBO_MENU_BG: Color = Color::rgba(0.10, 0.11, 0.15, 0.98);
pub const TOOLBAR_BG: Color = Color::rgb(0.075, 0.082, 0.105);
pub const DOCK_BG: Color = Color::rgb(0.070, 0.078, 0.100);
pub const OVERLAY_BG: Color = Color::rgba(0.07, 0.08, 0.10, 0.94);
/// Slightly lighter overlay for compact viewport chips / viz panels.
pub const OVERLAY_CHIP_BG: Color = Color::rgba(0.05, 0.055, 0.07, 0.78);
pub const VIEWPORT_TOOLBAR_BG: Color = Color::rgba(0.05, 0.055, 0.07, 0.82);
/// Soft accent text for status chips (progressive RT, etc.).
pub const STATUS_ACCENT: Color = Color::rgb(0.68, 0.82, 1.0);
pub const POPUP_BG: Color = Color::rgba(0.09, 0.10, 0.14, 0.98);
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
