//! Core interactive widgets.

use crate::context::GuiContext;
use crate::draw::DrawList;
use crate::icons::{Icon, ICON_PX};
use crate::id::Id;
use crate::style::{self, FONT_SCALE, ROW_H};
use crate::font;
use crate::types::{Color, Rect};

#[derive(Debug)]
pub struct WidgetLabState {
    pub clicks: u32,
    pub checked: bool,
    pub amount: f32,
    pub count: i32,
    pub combo_idx: usize,
    pub selected_row: usize,
}

impl Default for WidgetLabState {
    fn default() -> Self {
        Self {
            clicks: 0,
            checked: true,
            amount: 0.35,
            count: 8,
            combo_idx: 0,
            selected_row: 0,
        }
    }
}

pub fn widget_lab(ui: &mut GuiContext<'_>, lab: &mut WidgetLabState) {
    let vp = ui.viewport_rect();
    let panel_w = 280.0;
    let panel_h = 280.0;
    let rect = Rect::from_pos_size(
        vp.left() + style::PAD,
        (vp.max_y - panel_h - style::PAD).max(vp.top() + 40.0),
        panel_w,
        panel_h,
    );
    ui.begin_panel(rect, style::PANEL_BG);
    label(ui, "Widget Lab");
    ui.separator();
    if button(ui, "Click me") {
        lab.clicks = lab.clicks.saturating_add(1);
    }
    label(ui, &format!("Clicks: {}", lab.clicks));
    checkbox(ui, "Enabled", &mut lab.checked);
    slider_f32(ui, "Amount", &mut lab.amount, 0.0, 1.0);
    slider_i32(ui, "Count", &mut lab.count, 0, 32);
    let items = ["Draft", "Medium", "Full", "Export"];
    combo(ui, "Quality", &mut lab.combo_idx, &items);
    ui.gap(style::GAP);
    label(ui, "Select row:");
    for (i, name) in ["Layer A", "Layer B", "Layer C"].iter().enumerate() {
        if selectable(ui, name, lab.selected_row == i) {
            lab.selected_row = i;
        }
    }
    ui.end_panel();
}

pub fn label(ui: &mut GuiContext<'_>, text: &str) {
    let rect = ui.allocate(ROW_H);
    let y = font::text_top_in_row(rect.min_y, rect.height(), FONT_SCALE);
    ui.label_at(rect.min_x, y, text, style::TEXT, FONT_SCALE);
}

pub fn label_dim(ui: &mut GuiContext<'_>, text: &str) {
    let rect = ui.allocate(ROW_H);
    let y = font::text_top_in_row(rect.min_y, rect.height(), FONT_SCALE);
    ui.label_at(rect.min_x, y, text, style::TEXT_DIM, FONT_SCALE);
}

/// Uppercase-style section header used in inspector / dock.
pub fn section_header(ui: &mut GuiContext<'_>, text: &str) {
    ui.gap(style::GAP);
    let rect = ui.allocate(22.0);
    ui.label_at(
        rect.min_x,
        rect.min_y + 4.0,
        text,
        style::TEXT_MUTED,
        FONT_SCALE * 0.95,
    );
}

/// Collapsible inspector section with Lucide chevron (avoids missing-glyph `?`).
/// Returns `true` when the section body should be drawn.
pub fn collapsible_section(ui: &mut GuiContext<'_>, id: Id, title: &str, expanded: &mut bool) -> bool {
    ui.gap(2.0);
    let rect = ui.allocate(28.0);
    let hovered = ui.pointer_in(rect);
    if hovered {
        ui.state.set_hot(id);
    }
    if hovered && ui.input.primary_pressed {
        ui.state.active = Some(id);
    }
    if ui.input.primary_released && ui.state.is_active(id) && hovered {
        *expanded = !*expanded;
    }
    if hovered {
        ui.panel_rounded(rect, style::HOVER_BG, style::RADIUS_SM);
    }
    let chevron = if *expanded {
        Icon::ChevronDown
    } else {
        Icon::ChevronRight
    };
    ui.icon_centered(
        Rect::from_pos_size(rect.min_x + 2.0, rect.min_y, 20.0, rect.height()),
        chevron,
        style::TEXT_MUTED,
        14.0,
    );
    ui.label_in_rect(
        Rect::from_pos_size(rect.min_x + 24.0, rect.min_y, rect.width() - 28.0, rect.height()),
        title,
        style::TEXT_DIM,
        FONT_SCALE * 0.85,
    );
    ui.panel(
        Rect::from_pos_size(rect.min_x + 4.0, rect.max_y - 1.0, rect.width() - 8.0, 1.0),
        style::SEPARATOR,
    );
    *expanded
}

/// Icon-only toggle (Solo, Lock, etc.) with clear on/off colour.
pub fn icon_toggle(
    ui: &mut GuiContext<'_>,
    id: Id,
    icon: Icon,
    rect: Rect,
    active: bool,
) -> bool {
    let hovered = ui.pointer_in(rect);
    if hovered {
        ui.state.set_hot(id);
    }
    if hovered && ui.input.primary_pressed {
        ui.state.active = Some(id);
    }
    let clicked = ui.input.primary_released && ui.state.is_active(id) && hovered;
    let bg = if active {
        style::ACCENT_SOFT
    } else if hovered {
        style::BUTTON_HOVER
    } else {
        style::BUTTON_BG
    };
    ui.panel_rounded(rect, bg, style::RADIUS_SM);
    let size = ICON_PX
        .min(rect.width() - 4.0)
        .min(rect.height() - 4.0)
        .max(10.0);
    ui.icon_at(
        rect.min_x + (rect.width() - size) * 0.5,
        rect.min_y + (rect.height() - size) * 0.5,
        icon,
        if active {
            style::ACCENT
        } else if hovered {
            style::TEXT
        } else {
            style::TEXT_DIM
        },
        size,
    );
    clicked
}

/// Compact status pill with coloured indicator dot + label.
pub fn status_pill(ui: &mut GuiContext<'_>, rect: Rect, label: &str, color: Color) {
    ui.panel_rounded(rect, style::SURFACE, style::RADIUS_PILL);
    let dot = 7.0;
    ui.panel_rounded(
        Rect::from_pos_size(
            rect.min_x + 8.0,
            rect.min_y + (rect.height() - dot) * 0.5,
            dot,
            dot,
        ),
        color,
        dot * 0.5,
    );
    ui.label_at(
        rect.min_x + 20.0,
        font::text_top_in_row(rect.min_y, rect.height(), FONT_SCALE * 0.9),
        label,
        style::TEXT,
        FONT_SCALE * 0.9,
    );
}

/// Segmented control button (viewport Lit/Height/… cluster).
pub fn segmented_button(
    ui: &mut GuiContext<'_>,
    id: Id,
    rect: Rect,
    label: &str,
    active: bool,
) -> bool {
    let hovered = ui.pointer_in(rect);
    if hovered {
        ui.state.set_hot(id);
    }
    if hovered && ui.input.primary_pressed {
        ui.state.active = Some(id);
    }
    let clicked = ui.input.primary_released && ui.state.is_active(id) && hovered;
    ui.panel_rounded(
        rect,
        if active {
            style::ACCENT
        } else if hovered {
            style::BUTTON_HOVER
        } else {
            Color::rgba(0.0, 0.0, 0.0, 0.0)
        },
        style::RADIUS_SM,
    );
    ui.label_centered(
        rect.center_x(),
        font::text_top_in_row(rect.min_y, rect.height(), FONT_SCALE * 0.92),
        label,
        if active {
            style::TEXT
        } else {
            style::TEXT_DIM
        },
        FONT_SCALE * 0.92,
    );
    clicked
}

pub fn accent_button(ui: &mut GuiContext<'_>, text: &str) -> bool {
    accent_button_id(ui, Id::new(text).child("accent_btn"), text)
}

pub fn accent_button_id(ui: &mut GuiContext<'_>, id: Id, text: &str) -> bool {
    let rect = ui.allocate(style::TOOLBAR_BTN_H);
    chip_button(ui, id, text, rect, true)
}

pub fn chip_button(ui: &mut GuiContext<'_>, id: Id, text: &str, rect: Rect, accent: bool) -> bool {
    let hovered = ui.pointer_in(rect);
    if hovered {
        ui.state.set_hot(id);
    }
    if hovered && ui.input.primary_pressed {
        ui.state.active = Some(id);
    }
    let clicked = ui.input.primary_released && ui.state.is_active(id) && hovered;
    let bg = chip_bg(id, ui, hovered, accent);
    let radius = (rect.height() * 0.5).min(style::RADIUS_PILL);
    ui.panel_rounded(rect, bg, radius);
    let tw = DrawList::text_width(text, FONT_SCALE);
    let y = font::text_top_in_row(rect.min_y, rect.height(), FONT_SCALE);
    ui.label_at(
        rect.min_x + (rect.width() - tw) * 0.5,
        y,
        text,
        style::TEXT,
        FONT_SCALE,
    );
    clicked
}

/// Icon-only chip (toolbar tools, layer controls).
pub fn icon_button(ui: &mut GuiContext<'_>, id: Id, icon: Icon, rect: Rect) -> bool {
    let hovered = ui.pointer_in(rect);
    if hovered {
        ui.state.set_hot(id);
    }
    if hovered && ui.input.primary_pressed {
        ui.state.active = Some(id);
    }
    let clicked = ui.input.primary_released && ui.state.is_active(id) && hovered;
    let bg = if ui.state.is_active(id) && hovered {
        style::BUTTON_ACTIVE
    } else if hovered {
        style::BUTTON_HOVER
    } else {
        style::BUTTON_BG
    };
    ui.panel_rounded(rect, bg, style::RADIUS_SM);
    let size = ICON_PX
        .min(rect.width() - 4.0)
        .min(rect.height() - 4.0)
        .max(10.0);
    ui.icon_at(
        rect.min_x + (rect.width() - size) * 0.5,
        rect.min_y + (rect.height() - size) * 0.5,
        icon,
        style::TEXT,
        size,
    );
    clicked
}

/// Chip with a leading Lucide icon and label.
pub fn chip_icon_button(
    ui: &mut GuiContext<'_>,
    id: Id,
    icon: Icon,
    text: &str,
    rect: Rect,
    accent: bool,
) -> bool {
    let hovered = ui.pointer_in(rect);
    if hovered {
        ui.state.set_hot(id);
    }
    if hovered && ui.input.primary_pressed {
        ui.state.active = Some(id);
    }
    let clicked = ui.input.primary_released && ui.state.is_active(id) && hovered;
    let radius = (rect.height() * 0.5).min(style::RADIUS_PILL);
    ui.panel_rounded(rect, chip_bg(id, ui, hovered, accent), radius);
    let size = style::ICON_MD;
    let tw = DrawList::text_width(text, FONT_SCALE);
    let gap = 6.0;
    let content_w = size + gap + tw;
    let start = rect.min_x + (rect.width() - content_w) * 0.5;
    ui.icon_at(
        start,
        rect.min_y + (rect.height() - size) * 0.5,
        icon,
        style::TEXT,
        size,
    );
    let y = font::text_top_in_row(rect.min_y, rect.height(), FONT_SCALE);
    ui.label_at(start + size + gap, y, text, style::TEXT, FONT_SCALE);
    clicked
}

fn chip_bg(id: Id, ui: &GuiContext<'_>, hovered: bool, accent: bool) -> Color {
    if accent {
        if ui.state.is_active(id) && hovered {
            style::ACCENT_DIM
        } else if hovered {
            Color::rgb(0.30, 0.58, 1.0)
        } else {
            style::ACCENT
        }
    } else if ui.state.is_active(id) && hovered {
        style::BUTTON_ACTIVE
    } else if hovered {
        style::BUTTON_HOVER
    } else {
        style::BUTTON_BG
    }
}

pub fn button(ui: &mut GuiContext<'_>, text: &str) -> bool {
    button_id(ui, Id::new(text), text)
}

pub fn button_id(ui: &mut GuiContext<'_>, id: Id, text: &str) -> bool {
    button_id_align(ui, id, text, true)
}

/// Menu / dropdown row — left-aligned label.
pub fn menu_button(ui: &mut GuiContext<'_>, id: Id, text: &str) -> bool {
    button_id_align(ui, id, text, false)
}

fn button_id_align(ui: &mut GuiContext<'_>, id: Id, text: &str, center: bool) -> bool {
    let rect = ui.allocate(ROW_H);
    let hovered = ui.pointer_in(rect);
    if hovered {
        ui.state.set_hot(id);
    }
    let active = ui.state.is_active(id);
    if hovered && ui.input.primary_pressed {
        ui.state.active = Some(id);
    }
    let clicked = ui.input.primary_released && active && hovered;

    let bg = if active && hovered {
        style::BUTTON_ACTIVE
    } else if hovered {
        style::BUTTON_HOVER
    } else if center {
        style::BUTTON_BG
    } else {
        Color::rgba(0.0, 0.0, 0.0, 0.0)
    };
    if center || bg.a > 0.0 {
        ui.panel(rect, bg);
    }
    if center {
        let tw = DrawList::text_width(text, FONT_SCALE);
        ui.label_at(
            rect.min_x + (rect.width() - tw) * 0.5,
            rect.min_y + 4.0,
            text,
            style::TEXT,
            FONT_SCALE,
        );
    } else {
        ui.label_at(
            rect.min_x + 10.0,
            rect.min_y + 4.0,
            text,
            style::TEXT,
            FONT_SCALE,
        );
    }
    clicked
}

pub fn checkbox(ui: &mut GuiContext<'_>, text: &str, value: &mut bool) -> bool {
    checkbox_id(ui, Id::new(text).child("check"), text, value)
}

pub fn checkbox_id(ui: &mut GuiContext<'_>, id: Id, text: &str, value: &mut bool) -> bool {
    let rect = ui.allocate(ROW_H);
    let box_size = 16.0;
    let box_rect = Rect::from_pos_size(
        rect.min_x,
        rect.min_y + (ROW_H - box_size) * 0.5,
        box_size,
        box_size,
    );
    let hovered = ui.pointer_in(rect);
    if hovered {
        ui.state.set_hot(id);
    }
    if hovered && ui.input.primary_pressed {
        ui.state.active = Some(id);
    }
    let clicked = ui.input.primary_released && ui.state.is_active(id) && hovered;
    if clicked {
        *value = !*value;
    }

    ui.panel(
        box_rect,
        if *value {
            style::CHECK_ON
        } else if hovered {
            style::BUTTON_HOVER
        } else {
            style::CHECK_BG
        },
    );
    if *value {
        ui.icon_at(
            box_rect.min_x,
            box_rect.min_y,
            Icon::Check,
            style::TEXT,
            box_size,
        );
    }
    ui.label_at(
        box_rect.max_x + 8.0,
        rect.min_y + 4.0,
        text,
        style::TEXT,
        FONT_SCALE,
    );
    clicked
}

pub fn slider_f32(
    ui: &mut GuiContext<'_>,
    text: &str,
    value: &mut f32,
    min: f32,
    max: f32,
) -> bool {
    slider_f32_id(ui, Id::new(text).child("slider_f"), text, value, min, max)
}

pub fn slider_f32_id(
    ui: &mut GuiContext<'_>,
    id: Id,
    text: &str,
    value: &mut f32,
    min: f32,
    max: f32,
) -> bool {
    slider_inner(ui, id, text, value, min, max, false)
}

pub fn slider_i32(
    ui: &mut GuiContext<'_>,
    text: &str,
    value: &mut i32,
    min: i32,
    max: i32,
) -> bool {
    slider_i32_id(ui, Id::new(text).child("slider_i"), text, value, min, max)
}

pub fn slider_i32_id(
    ui: &mut GuiContext<'_>,
    id: Id,
    text: &str,
    value: &mut i32,
    min: i32,
    max: i32,
) -> bool {
    let mut f = *value as f32;
    let changed = slider_inner(ui, id, text, &mut f, min as f32, max as f32, true);
    if changed {
        *value = f.round() as i32;
    }
    changed
}

fn slider_inner(
    ui: &mut GuiContext<'_>,
    id: Id,
    text: &str,
    value: &mut f32,
    min: f32,
    max: f32,
    integer: bool,
) -> bool {
    let rect = ui.allocate(ROW_H + 6.0);
    let label_w = (rect.width() * 0.34).clamp(48.0, 110.0);
    let value_w = 52.0;
    ui.label_at(
        rect.min_x,
        rect.min_y + 6.0,
        text,
        style::TEXT_DIM,
        FONT_SCALE,
    );

    let track = Rect::from_min_max(
        rect.min_x + label_w,
        rect.min_y + 12.0,
        rect.max_x - value_w - 6.0,
        rect.min_y + 16.0,
    );
    let value_box = Rect::from_pos_size(
        rect.max_x - value_w,
        rect.min_y + 4.0,
        value_w,
        ROW_H - 2.0,
    );
    let edit_id = id.child("edit");
    let editing = ui.state.text_focus == Some(edit_id);

    // Value box interaction — click to edit.
    let value_hovered = ui.pointer_in(value_box);
    if value_hovered {
        ui.state.set_hot(edit_id);
    }
    if value_hovered && ui.input.primary_pressed {
        ui.state.text_focus = Some(edit_id);
        ui.state.text_buffer = if integer {
            format!("{:.0}", *value)
        } else {
            format!("{:.2}", *value)
        };
        ui.state.text_enter = false;
        ui.state.active = Some(edit_id);
    }

    let mut changed = false;
    let span = (max - min).max(1e-6);

    if editing {
        // Type into buffer.
        if !ui.input.text.is_empty() {
            for ch in ui.input.text.chars() {
                if ch.is_ascii_digit() || ch == '.' || ch == '-' {
                    ui.state.text_buffer.push(ch);
                }
            }
        }
        if ui.input.backspace_pressed {
            ui.state.text_buffer.pop();
        }
        let commit = ui.state.text_enter || ui.input.enter_pressed;
        let cancel = ui.input.escape_pressed;
        let clicked_away = ui.input.primary_pressed && !value_hovered;
        if commit || clicked_away {
            if let Ok(parsed) = ui.state.text_buffer.parse::<f32>() {
                let mut v = parsed.clamp(min, max);
                if integer {
                    v = v.round();
                }
                if (v - *value).abs() > 1e-6 {
                    *value = v;
                    changed = true;
                }
            }
            ui.state.clear_text_focus();
        } else if cancel {
            ui.state.clear_text_focus();
        }
    } else {
        // Slider drag on track area (not the value box).
        let track_hit = Rect::from_min_max(
            track.min_x - 4.0,
            rect.min_y,
            track.max_x + 4.0,
            rect.max_y,
        );
        let hovered = ui.pointer_in(track_hit);
        if hovered {
            ui.state.set_hot(id);
        }
        if hovered && ui.input.primary_pressed {
            ui.state.active = Some(id);
        }
        if ui.state.is_active(id) {
            if let Some((px, _)) = ui.input.pointer {
                let t = ((px - track.min_x) / track.width()).clamp(0.0, 1.0);
                let mut v = min + t * span;
                if integer {
                    v = v.round();
                }
                if (v - *value).abs() > 1e-6 {
                    *value = v.clamp(min, max);
                    changed = true;
                }
            }
        }
    }

    let t = ((*value - min) / span).clamp(0.0, 1.0);
    ui.panel(track, style::TRACK_BG);
    let fill_w = track.width() * t;
    if fill_w > 0.5 {
        ui.panel(
            Rect::from_pos_size(track.min_x, track.min_y, fill_w, track.height()),
            style::ACCENT,
        );
    }
    let thumb_s = 12.0;
    let thumb_x = track.min_x + fill_w - thumb_s * 0.5;
    let thumb = Rect::from_pos_size(
        thumb_x.clamp(track.min_x - 2.0, track.max_x - thumb_s + 2.0),
        track.center_y() - thumb_s * 0.5,
        thumb_s,
        thumb_s,
    );
    ui.panel(
        thumb,
        if ui.state.is_active(id) {
            style::THUMB_ACTIVE
        } else {
            style::THUMB_BG
        },
    );

    // Value box (editable).
    ui.panel_rounded(
        value_box,
        if editing {
            style::INPUT_BG
        } else if value_hovered {
            style::BUTTON_HOVER
        } else {
            style::SURFACE
        },
        style::RADIUS_SM,
    );
    let display = if editing {
        ui.state.text_buffer.clone()
    } else if integer {
        format!("{:.0}", *value)
    } else {
        format!("{:.2}", *value)
    };
    let tw = DrawList::text_width(&display, FONT_SCALE * 0.9);
    ui.label_at(
        value_box.min_x + (value_box.width() - tw).max(4.0) * 0.5,
        value_box.min_y + 5.0,
        &display,
        style::TEXT,
        FONT_SCALE * 0.9,
    );
    changed
}

pub fn selectable(ui: &mut GuiContext<'_>, text: &str, selected: bool) -> bool {
    let id = Id::new(text).child("sel");
    let rect = ui.allocate(ROW_H);
    let hovered = ui.pointer_in(rect);
    if hovered {
        ui.state.set_hot(id);
    }
    if hovered && ui.input.primary_pressed {
        ui.state.active = Some(id);
    }
    let clicked = ui.input.primary_released && ui.state.is_active(id) && hovered;

    let bg = if selected {
        style::SELECTED_BG
    } else if hovered {
        style::BUTTON_HOVER
    } else {
        Color::rgba(0.0, 0.0, 0.0, 0.0)
    };
    if bg.a > 0.0 {
        ui.panel(rect, bg);
    }
    ui.label_at(
        rect.min_x + 6.0,
        rect.min_y + 4.0,
        text,
        style::TEXT,
        FONT_SCALE,
    );
    clicked
}

pub fn combo(ui: &mut GuiContext<'_>, text: &str, selected: &mut usize, items: &[&str]) -> bool {
    let id = Id::new(text).child("combo");
    let mut changed = false;

    if let Some((pick_id, idx)) = ui.state.combo_pick.take() {
        if pick_id == id {
            if idx < items.len() && *selected != idx {
                *selected = idx;
                changed = true;
            }
        } else {
            // Not ours — put back for another combo this frame (unlikely).
            ui.state.combo_pick = Some((pick_id, idx));
        }
    }

    let rect = ui.allocate(ROW_H);
    let label_w = (rect.width() * 0.38).clamp(48.0, 120.0);
    ui.label_at(
        rect.min_x,
        rect.min_y + 4.0,
        text,
        style::TEXT_DIM,
        FONT_SCALE,
    );

    let field = Rect::from_min_max(rect.min_x + label_w, rect.min_y, rect.max_x, rect.max_y);
    let hovered = ui.pointer_in(field);
    if hovered {
        ui.state.set_hot(id);
    }
    if hovered && ui.input.primary_pressed {
        ui.state.active = Some(id);
        if ui.state.open_combo == Some(id) {
            ui.state.open_combo = None;
        } else {
            ui.state.open_combo = Some(id);
        }
    }

    let current = items.get(*selected).copied().unwrap_or("?");
    let bg = if hovered || ui.state.open_combo == Some(id) {
        style::BUTTON_HOVER
    } else {
        style::BUTTON_BG
    };
    ui.panel_rounded(field, bg, style::RADIUS_SM);
    ui.label_in_rect(
        Rect::from_pos_size(field.min_x + 8.0, field.min_y, field.width() - 28.0, field.height()),
        current,
        style::TEXT,
        FONT_SCALE,
    );
    ui.icon_centered(
        Rect::from_pos_size(field.max_x - 22.0, field.min_y, 18.0, field.height()),
        Icon::ChevronDown,
        style::TEXT_DIM,
        14.0,
    );

    if ui.state.open_combo == Some(id) {
        let below = field.max_y + 2.0;
        let menu_h = ROW_H * items.len() as f32;
        let open_down = below + menu_h <= ui.screen_h - 4.0;
        let menu = if open_down {
            Rect::from_pos_size(field.min_x, below, field.width(), menu_h)
        } else {
            Rect::from_pos_size(
                field.min_x,
                (field.min_y - menu_h - 2.0).max(0.0),
                field.width(),
                menu_h,
            )
        };
        ui.queue_combo_menu(id, menu, items, *selected);
    }

    changed
}

pub(crate) fn draw_combo_menu(
    ui: &mut GuiContext<'_>,
    combo_id: Id,
    menu: Rect,
    items: &[String],
    selected: usize,
) {
    ui.panel(menu, style::COMBO_MENU_BG);
    let popup_id = combo_id.child("popup");
    if ui.pointer_in(menu) {
        ui.state.set_hot(popup_id);
    }

    for (i, item) in items.iter().enumerate() {
        let item_id = combo_id.child("item").with(i as u64);
        let row = Rect::from_pos_size(
            menu.min_x,
            menu.min_y + ROW_H * i as f32,
            menu.width(),
            ROW_H,
        );
        let hovered = ui.pointer_in(row);
        if hovered {
            ui.state.set_hot(item_id);
        }
        if hovered && ui.input.primary_pressed {
            ui.state.active = Some(item_id);
            ui.state.combo_pick = Some((combo_id, i));
            ui.state.open_combo = None;
        }
        let bg = if i == selected {
            style::SELECTED_BG
        } else if hovered {
            style::BUTTON_HOVER
        } else {
            Color::rgba(0.0, 0.0, 0.0, 0.0)
        };
        if bg.a > 0.0 {
            ui.panel(row, bg);
        }
        ui.label_at(
            row.min_x + 10.0,
            row.min_y + 4.0,
            item,
            style::TEXT,
            FONT_SCALE,
        );
    }
}
