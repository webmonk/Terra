//! Per-frame GUI context (immediate-mode builder).

use crate::draw::{DrawCmd, DrawList};
use crate::font;
use crate::icons::Icon;
use crate::id::Id;
use crate::layout::Layout;
use crate::scroll;
use crate::layout_prefs::{LayoutPrefs, SPLITTER_HIT};
use crate::state::{GuiState, SplitterDrag, SplitterKind, WindowDrag};
use crate::style;
use crate::types::{Align, Color, Rect};
use crate::widgets::{self, WidgetLabState};

/// Fixed editor chrome insets (logical px) — mode rail + tools left, layers+inspector right.
pub const INSET_LEFT: f32 = style::LEFT_CHROME_W;
pub const INSET_RIGHT: f32 = style::RIGHT_PANEL_W;
pub const INSET_TOP: f32 = style::APP_BAR_H;
pub const INSET_BOTTOM: f32 = style::STATUS_STRIP_H;
/// Fraction of the right rail height reserved for the layers stack (rest = inspector).
pub const RIGHT_LAYERS_FRAC: f32 = 0.52;
pub const WINDOW_TITLE_H: f32 = 28.0;

#[derive(Debug, Clone)]
pub struct GuiInput {
    pub pointer: Option<(f32, f32)>,
    pub primary_down: bool,
    /// Set by `GuiContext::begin` from `GuiState::was_primary_down`.
    pub primary_pressed: bool,
    pub primary_released: bool,
    /// Vertical wheel delta (positive = scroll content up / finger up).
    pub scroll_delta: f32,
    /// Text typed since the previous frame, for lightweight editor search fields.
    pub text: String,
    /// Backspace pressed since the previous frame.
    pub backspace_pressed: bool,
    /// Escape pressed since the previous frame.
    pub escape_pressed: bool,
    /// Enter / Return pressed since the previous frame.
    pub enter_pressed: bool,
}

impl Default for GuiInput {
    fn default() -> Self {
        Self {
            pointer: None,
            primary_down: false,
            primary_pressed: false,
            primary_released: false,
            scroll_delta: 0.0,
            text: String::new(),
            backspace_pressed: false,
            escape_pressed: false,
            enter_pressed: false,
        }
    }
}

pub struct GuiContext<'a> {
    pub screen_w: f32,
    pub screen_h: f32,
    pub pixels_per_point: f32,
    pub input: GuiInput,
    pub draw: DrawList,
    /// Popups / menus — always composited after `draw`.
    pub overlay: DrawList,
    pub state: &'a mut GuiState,
    layout: Option<Layout>,
    clip: Option<Rect>,
    drawing_overlay: bool,
    pending_combo_menu: Option<PendingComboMenu>,
    pending_tooltips: Vec<PendingTooltip>,
    active_scroll: Option<ActiveScroll>,
    scroll_consumed: bool,
    overlay_stack: Vec<OverlayFrame>,
    pub image_rgba: Option<(u32, u32, Vec<u8>)>,
}

struct PendingComboMenu {
    id: Id,
    rect: Rect,
    items: Vec<String>,
    selected: usize,
}

struct PendingTooltip {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    title: String,
    body: String,
    shortcut: Option<String>,
}

struct ActiveScroll {
    id: Id,
    viewport: Rect,
    scroll_at_begin: f32,
}

/// Saved panel layout/clip so overlays can nest without destroying `allocate()`.
struct OverlayFrame {
    layout: Option<Layout>,
    clip: Option<Rect>,
    was_drawing_overlay: bool,
}

impl<'a> GuiContext<'a> {
    pub fn begin(
        screen_w: f32,
        screen_h: f32,
        pixels_per_point: f32,
        mut input: GuiInput,
        state: &'a mut GuiState,
    ) -> Self {
        input.primary_pressed = input.primary_down && !state.was_primary_down;
        input.primary_released = !input.primary_down && state.was_primary_down;
        // Fresh hover each frame (last-wins as widgets run).
        state.hot = None;
        let pixels_per_point = pixels_per_point.max(0.5);
        // Bake glyphs at the framebuffer pixel size for this DPI.
        font::prepare(pixels_per_point);
        if input.primary_released {
            // Keep open_combo; clear active after widgets process click this frame.
        }
        Self {
            screen_w: screen_w.max(1.0),
            screen_h: screen_h.max(1.0),
            pixels_per_point,
            input,
            draw: DrawList::default(),
            overlay: DrawList::default(),
            state,
            layout: None,
            clip: None,
            drawing_overlay: false,
            pending_combo_menu: None,
            pending_tooltips: Vec::new(),
            active_scroll: None,
            scroll_consumed: false,
            overlay_stack: Vec::new(),
            image_rgba: None,
        }
    }

    /// Route subsequent draw/hit widgets into the overlay layer (no clip).
    /// Preserves the active panel layout so callers can resume `allocate` after `end_overlay`.
    pub fn begin_overlay(&mut self) {
        self.overlay_stack.push(OverlayFrame {
            layout: self.layout.take(),
            clip: self.clip,
            was_drawing_overlay: self.drawing_overlay,
        });
        // Overlay content is unclipped; clear scissor on both lists while drawing it.
        self.draw.set_clip(None);
        self.overlay.set_clip(None);
        self.layout = None;
        self.clip = None;
        self.drawing_overlay = true;
    }

    pub fn end_overlay(&mut self) {
        let Some(frame) = self.overlay_stack.pop() else {
            self.layout = None;
            self.clip = None;
            self.drawing_overlay = false;
            self.draw.set_clip(None);
            self.overlay.set_clip(None);
            return;
        };
        self.layout = frame.layout;
        self.clip = frame.clip;
        self.drawing_overlay = frame.was_drawing_overlay;
        let list = if self.drawing_overlay {
            &mut self.overlay
        } else {
            &mut self.draw
        };
        list.set_clip(self.clip);
    }

    /// Call once after building UI for the frame.
    pub fn end(&mut self) {
        if let Some(menu) = self.pending_combo_menu.take() {
            self.begin_overlay();
            widgets::draw_combo_menu(self, menu.id, menu.rect, &menu.items, menu.selected);
            self.end_overlay();
        }
        let tips = std::mem::take(&mut self.pending_tooltips);
        if !tips.is_empty() {
            self.begin_overlay();
            for tip in tips {
                flush_tooltip(self, tip);
            }
            self.end_overlay();
        }
        if self.input.enter_pressed {
            self.state.text_enter = true;
        }
        if self.input.primary_released {
            self.state.active = None;
            self.state.window_drag = None;
            self.state.scroll_drag = None;
            self.state.splitter_drag = None;
        }
        // Close combo on outside click (no hot on combo/items this frame).
        if self.input.primary_pressed {
            if let Some(open) = self.state.open_combo {
                let still = self.state.hot.map(|h| h == open || is_combo_child(open, h));
                if still != Some(true) {
                    self.state.open_combo = None;
                }
            }
        }
        self.state.was_primary_down = self.input.primary_down;
    }

    /// Queue a tooltip to draw on the overlay after all chrome (safe from scroll clips).
    pub fn queue_tooltip(
        &mut self,
        anchor: Rect,
        title: &str,
        body: &str,
        shortcut: Option<&str>,
    ) {
        let max_w = 240.0;
        let title_w = DrawList::text_width(title, style::FONT_SCALE);
        let body_w = DrawList::text_width(body, style::FONT_SCALE * 0.85).min(max_w - 16.0);
        let sc_w = shortcut
            .map(|s| DrawList::text_width(s, style::FONT_SCALE * 0.8) + 24.0)
            .unwrap_or(0.0);
        let w = (title_w.max(body_w) + sc_w + 20.0).clamp(120.0, max_w);
        let h = if shortcut.is_some() { 54.0 } else { 44.0 };
        let x = (anchor.max_x + 8.0).min(self.screen_w - w - 8.0);
        let y = anchor.min_y.max(style::APP_BAR_H + 4.0);
        self.pending_tooltips.push(PendingTooltip {
            x,
            y,
            w,
            h,
            title: title.to_string(),
            body: body.to_string(),
            shortcut: shortcut.map(|s| s.to_string()),
        });
    }

    pub fn wants_pointer(&self) -> bool {
        self.state.wants_pointer() || self.state.window_drag.is_some()
    }

    pub fn layout(&self) -> &LayoutPrefs {
        &self.state.layout
    }

    pub fn layout_mut(&mut self) -> &mut LayoutPrefs {
        &mut self.state.layout
    }

    /// Central 3D viewport in logical pixels (between side panels / menu).
    pub fn viewport_rect(&self) -> Rect {
        Self::viewport_rect_for(self.screen_w, self.screen_h, &self.state.layout)
    }

    pub fn viewport_rect_for(screen_w: f32, screen_h: f32, layout: &LayoutPrefs) -> Rect {
        let left = layout.left_chrome_w();
        let right = (screen_w - layout.effective_right_w()).max(left + 32.0);
        let top = INSET_TOP;
        let bottom = (screen_h - INSET_BOTTOM).max(top + 32.0);
        Rect::from_min_max(left, top, right, bottom)
    }

    /// Full left chrome (mode rail + contextual tool panel).
    pub fn left_panel_rect(&self) -> Rect {
        Rect::from_min_max(
            0.0,
            INSET_TOP,
            self.state.layout.left_chrome_w(),
            self.screen_h - INSET_BOTTOM,
        )
    }

    /// Far-left workspace mode selector.
    pub fn mode_rail_rect(&self) -> Rect {
        Rect::from_min_max(
            0.0,
            INSET_TOP,
            style::MODE_RAIL_W,
            self.screen_h - INSET_BOTTOM,
        )
    }

    /// Contextual tools for the active workspace mode.
    pub fn tool_panel_rect(&self) -> Rect {
        let w = self.state.layout.effective_tool_panel_w();
        Rect::from_min_max(
            style::MODE_RAIL_W,
            INSET_TOP,
            style::MODE_RAIL_W + w,
            self.screen_h - INSET_BOTTOM,
        )
    }

    pub fn right_panel_rect(&self) -> Rect {
        let rw = self.state.layout.effective_right_w();
        Rect::from_min_max(
            self.screen_w - rw,
            INSET_TOP,
            self.screen_w,
            self.screen_h - INSET_BOTTOM,
        )
    }

    /// Top half of the right rail — Layers list.
    pub fn right_layers_rect(&self) -> Rect {
        let full = self.right_panel_rect();
        if self.state.layout.inspector_collapsed {
            return full;
        }
        let frac = self.state.layout.effective_layers_frac();
        let mid = full.min_y + (full.height() * frac).max(120.0);
        let mid = mid.min(full.max_y - 160.0);
        Rect::from_min_max(full.min_x, full.min_y, full.max_x, mid)
    }

    /// Bottom half of the right rail — Inspector.
    pub fn right_inspector_rect(&self) -> Rect {
        let full = self.right_panel_rect();
        if self.state.layout.inspector_collapsed {
            // Keep a thin strip so the expand control remains reachable.
            return Rect::from_min_max(
                full.min_x,
                full.max_y - style::HEADER_H,
                full.max_x,
                full.max_y,
            );
        }
        let layers = self.right_layers_rect();
        Rect::from_min_max(full.min_x, layers.max_y, full.max_x, full.max_y)
    }

    /// Draw and interact with dock splitters. Call once per frame after panels.
    /// Returns `true` when layout changed this frame.
    pub fn draw_layout_splitters(&mut self) -> bool {
        let mut changed = false;
        let screen_h = self.screen_h;
        let screen_w = self.screen_w;
        let top = INSET_TOP;
        let bottom = screen_h - INSET_BOTTOM;

        // Continue active drag.
        if let Some(drag) = self.state.splitter_drag {
            if let Some((px, py)) = self.input.pointer {
                match drag.kind {
                    SplitterKind::ToolPanel => {
                        let delta = px - drag.start_pointer;
                        let w = (drag.start_value + delta)
                            .clamp(LayoutPrefs::TOOL_PANEL_MIN, LayoutPrefs::TOOL_PANEL_MAX);
                        if self.state.layout.tool_panel_collapsed {
                            self.state.layout.tool_panel_collapsed = false;
                        }
                        if (w - self.state.layout.tool_panel_w).abs() > 0.5 {
                            self.state.layout.tool_panel_w = w;
                            changed = true;
                        }
                    }
                    SplitterKind::RightPanel => {
                        let delta = drag.start_pointer - px;
                        let w = (drag.start_value + delta)
                            .clamp(LayoutPrefs::RIGHT_PANEL_MIN, LayoutPrefs::RIGHT_PANEL_MAX);
                        if (w - self.state.layout.right_panel_w).abs() > 0.5 {
                            self.state.layout.right_panel_w = w;
                            changed = true;
                        }
                    }
                    SplitterKind::LayersInspector => {
                        let full = self.right_panel_rect();
                        let local = (py - full.min_y) / full.height().max(1.0);
                        let frac = local.clamp(
                            LayoutPrefs::LAYERS_FRAC_MIN,
                            LayoutPrefs::LAYERS_FRAC_MAX,
                        );
                        if self.state.layout.inspector_collapsed {
                            self.state.layout.inspector_collapsed = false;
                        }
                        if (frac - self.state.layout.layers_frac).abs() > 0.002 {
                            self.state.layout.layers_frac = frac;
                            changed = true;
                        }
                    }
                }
            }
            self.state.set_hot(Id::new("__splitter_drag"));
            return changed;
        }

        // Tool panel | viewport splitter
        if !self.state.layout.tool_panel_collapsed {
            let x = self.state.layout.left_chrome_w();
            let hit = Rect::from_pos_size(x - SPLITTER_HIT * 0.5, top, SPLITTER_HIT, bottom - top);
            let id = Id::new("split_tools");
            if self.pointer_in(hit) {
                self.state.set_hot(id);
                self.panel(hit, style::ACCENT_SOFT);
                if self.input.primary_pressed {
                    self.state.splitter_drag = Some(SplitterDrag {
                        kind: SplitterKind::ToolPanel,
                        start_pointer: self.input.pointer.map(|(x, _)| x).unwrap_or(x),
                        start_value: self.state.layout.tool_panel_w,
                    });
                }
            }
        }

        // Viewport | right panel splitter
        {
            let x = screen_w - self.state.layout.effective_right_w();
            let hit = Rect::from_pos_size(x - SPLITTER_HIT * 0.5, top, SPLITTER_HIT, bottom - top);
            let id = Id::new("split_right");
            if self.pointer_in(hit) {
                self.state.set_hot(id);
                self.panel(hit, style::ACCENT_SOFT);
                if self.input.primary_pressed {
                    self.state.splitter_drag = Some(SplitterDrag {
                        kind: SplitterKind::RightPanel,
                        start_pointer: self.input.pointer.map(|(x, _)| x).unwrap_or(x),
                        start_value: self.state.layout.right_panel_w,
                    });
                }
            }
        }

        // Layers | inspector splitter
        if !self.state.layout.inspector_collapsed {
            let layers = self.right_layers_rect();
            let y = layers.max_y;
            let hit = Rect::from_pos_size(
                layers.min_x,
                y - SPLITTER_HIT * 0.5,
                layers.width(),
                SPLITTER_HIT,
            );
            let id = Id::new("split_layers");
            if self.pointer_in(hit) {
                self.state.set_hot(id);
                self.panel(hit, style::ACCENT_SOFT);
                if self.input.primary_pressed {
                    self.state.splitter_drag = Some(SplitterDrag {
                        kind: SplitterKind::LayersInspector,
                        start_pointer: self.input.pointer.map(|(_, y)| y).unwrap_or(y),
                        start_value: self.state.layout.layers_frac,
                    });
                }
            }
        }

        changed
    }

    pub fn bottom_dock_rect(&self) -> Rect {
        Rect::from_min_max(
            0.0,
            self.screen_h - INSET_BOTTOM,
            self.screen_w,
            self.screen_h,
        )
    }

    pub fn pointer_in(&self, rect: Rect) -> bool {
        let Some((x, y)) = self.input.pointer else {
            return false;
        };
        if !self.drawing_overlay {
            if let Some(clip) = self.clip {
                if !clip.contains(x, y) {
                    return false;
                }
            }
        }
        rect.contains(x, y)
    }

    fn push_panel(&mut self, rect: Rect, color: Color) {
        if self.drawing_overlay {
            self.overlay.panel(rect, color);
        } else {
            self.draw.panel(rect, color);
        }
    }

    pub fn panel(&mut self, rect: Rect, color: Color) {
        if !self.drawing_overlay {
            if let Some(clip) = self.clip {
                if !rect.intersects(clip) {
                    return;
                }
            }
        }
        self.push_panel(rect, color);
    }

    pub fn panel_rounded(&mut self, rect: Rect, color: Color, radius: f32) {
        if !self.drawing_overlay {
            if let Some(clip) = self.clip {
                if !rect.intersects(clip) {
                    return;
                }
            }
        }
        if self.drawing_overlay {
            self.overlay.panel_rounded(rect, color, radius);
        } else {
            self.draw.panel_rounded(rect, color, radius);
        }
    }

    pub fn label_at(&mut self, x: f32, y: f32, text: &str, color: Color, scale: f32) {
        if !self.drawing_overlay {
            if let Some(clip) = self.clip {
                let h = font::line_height(scale);
                let w = DrawList::text_width(text, scale);
                // Cheap CPU cull; GPU scissor catches partial overflow.
                if y >= clip.max_y || y + h <= clip.min_y || x >= clip.max_x || x + w <= clip.min_x
                {
                    return;
                }
            }
        }
        if self.drawing_overlay {
            self.overlay.label(x, y, text, color, scale);
        } else {
            self.draw.label(x, y, text, color, scale);
        }
    }

    /// Draw left-aligned text vertically centred in `rect` (uses font line metrics).
    pub fn label_in_rect(&mut self, rect: Rect, text: &str, color: Color, scale: f32) {
        let y = font::text_top_in_row(rect.min_y, rect.height(), scale);
        self.label_at(rect.min_x, y, text, color, scale);
    }

    /// Draw centred text vertically centred in `rect`.
    pub fn label_centered_in_rect(&mut self, rect: Rect, text: &str, color: Color, scale: f32) {
        let y = font::text_top_in_row(rect.min_y, rect.height(), scale);
        self.label_centered(rect.center_x(), y, text, color, scale);
    }

    pub fn label_centered(&mut self, x: f32, y: f32, text: &str, color: Color, scale: f32) {
        if self.drawing_overlay {
            self.overlay
                .label_aligned(x, y, text, color, scale, Align::Center);
        } else {
            self.draw
                .label_aligned(x, y, text, color, scale, Align::Center);
        }
    }

    /// Centre an icon of `size` inside `rect`.
    pub fn icon_centered(&mut self, rect: Rect, icon: Icon, color: Color, size: f32) {
        let size = size.max(8.0);
        let x = rect.min_x + (rect.width() - size) * 0.5;
        let y = rect.min_y + (rect.height() - size) * 0.5;
        self.icon_at(x, y, icon, color, size);
    }

    /// Draw a Lucide icon at `(x, y)` with logical square size `size`.
    pub fn icon_at(&mut self, x: f32, y: f32, icon: Icon, color: Color, size: f32) {
        if !self.drawing_overlay {
            if let Some(clip) = self.clip {
                if y >= clip.max_y
                    || y + size <= clip.min_y
                    || x >= clip.max_x
                    || x + size <= clip.min_x
                {
                    return;
                }
            }
        }
        if self.drawing_overlay {
            self.overlay.icon(x, y, icon, color, size);
        } else {
            self.draw.icon(x, y, icon, color, size);
        }
    }

    pub fn begin_panel(&mut self, rect: Rect, color: Color) {
        self.open_panel(rect, color, 0.0, None);
    }

    /// Scrollable panel. Call [`end_panel_scrolled`] when finished so a scrollbar is drawn.
    pub fn begin_panel_scrolled(&mut self, id: Id, rect: Rect, color: Color, scroll_y: &mut f32) {
        // Apply in-progress thumb drag before laying out content (same-frame feedback).
        if let Some(drag) = self.state.scroll_drag {
            if drag.id == id && drag.vertical {
                if let Some((_, py)) = self.input.pointer {
                    let max = self
                        .state
                        .scroll_max
                        .get(&id.0)
                        .copied()
                        .unwrap_or(f32::MAX);
                    *scroll_y = (drag.start_scroll
                        + (py - drag.start_pointer) * drag.scroll_per_pixel)
                        .clamp(0.0, max);
                }
            }
        }
        self.apply_wheel_scroll_y(id, rect, scroll_y);
        self.open_panel(rect, color, *scroll_y, Some(id));
    }

    /// Apply mouse-wheel scrolling only when the region overflowed last frame.
    fn apply_wheel_scroll_y(&mut self, id: Id, viewport: Rect, scroll_y: &mut f32) {
        if self.scroll_consumed {
            return;
        }
        let hovering = self
            .input
            .pointer
            .map(|(x, y)| viewport.contains(x, y))
            .unwrap_or(false);
        if !hovering {
            return;
        }
        if self.input.scroll_delta.abs() < 1e-4 {
            return;
        }

        let max = self.state.scroll_max.get(&id.0).copied().unwrap_or(0.0);
        self.scroll_consumed = true;
        if max < 1.0 {
            // Content fits — keep offset at zero so layout doesn't jitter.
            *scroll_y = 0.0;
            return;
        }
        *scroll_y = (*scroll_y - self.input.scroll_delta * 24.0).clamp(0.0, max);
    }

    fn open_panel(&mut self, rect: Rect, color: Color, scroll_y: f32, scroll_id: Option<Id>) {
        // Background ignores content clip; content is GPU-scissored to `rect`.
        let list = if self.drawing_overlay {
            &mut self.overlay
        } else {
            &mut self.draw
        };
        list.panel(rect, color);
        list.set_clip(Some(rect));
        self.clip = Some(rect);
        let mut layout = Layout::new(rect);
        if scroll_id.is_some() {
            // Keep labels clear of the scrollbar gutter.
            layout.content.max_x =
                (layout.content.max_x - style::SCROLLBAR_W - style::SCROLLBAR_PAD)
                    .max(layout.content.min_x + 32.0);
        }
        layout.cursor_y -= scroll_y.max(0.0);
        self.layout = Some(layout);
        self.active_scroll = scroll_id.map(|id| ActiveScroll {
            id,
            viewport: rect,
            scroll_at_begin: scroll_y.max(0.0),
        });
    }

    pub fn end_panel(&mut self) {
        let list = if self.drawing_overlay {
            &mut self.overlay
        } else {
            &mut self.draw
        };
        list.set_clip(None);
        self.clip = None;
        self.layout = None;
        self.active_scroll = None;
    }

    /// Finish a scrolled panel: clamp scroll and draw a vertical scrollbar when needed.
    pub fn end_panel_scrolled(&mut self, scroll_y: &mut f32) {
        let region = self.active_scroll.take();
        let cursor_y = self.layout_cursor_y();
        // Clear clip so the scrollbar is not scissored away at the edge.
        let list = if self.drawing_overlay {
            &mut self.overlay
        } else {
            &mut self.draw
        };
        list.set_clip(None);
        self.clip = None;
        self.layout = None;

        let Some(region) = region else {
            return;
        };
        let content_bottom = cursor_y.unwrap_or(region.viewport.max_y) + region.scroll_at_begin;
        let content_h =
            (content_bottom + style::PAD - region.viewport.min_y).max(region.viewport.height());
        let max_scroll = (content_h - region.viewport.height()).max(0.0);
        self.state.scroll_max.insert(region.id.0, max_scroll);
        if max_scroll < 1.0 {
            *scroll_y = 0.0;
        }
        scroll::scrollbar_y(self, region.id, region.viewport, content_h, scroll_y);
    }

    pub fn separator(&mut self) {
        let rect = self.allocate(style::SEPARATOR_H + style::GAP);
        let line = Rect::from_pos_size(
            rect.min_x,
            rect.min_y + style::GAP * 0.5,
            rect.width(),
            style::SEPARATOR_H,
        );
        self.panel(line, style::SEPARATOR);
    }

    pub fn gap(&mut self, h: f32) {
        let _ = self.allocate(h);
    }

    pub fn allocate(&mut self, height: f32) -> Rect {
        self.layout
            .as_mut()
            .expect("allocate without begin_panel")
            .allocate(height)
    }

    pub fn layout_cursor_y(&self) -> Option<f32> {
        self.layout.as_ref().map(|l| l.cursor_y)
    }

    pub fn widget_lab(&mut self, lab: &mut WidgetLabState) {
        widgets::widget_lab(self, lab);
    }

    /// Queue a full RGBA image for the next GPU upload (one image slot per frame).
    pub fn image(&mut self, rect: Rect, width: u32, height: u32, rgba: &[u8]) {
        if let Some(clip) = self.clip {
            if !rect.intersects(clip) {
                return;
            }
        }
        self.image_rgba = Some((width, height, rgba.to_vec()));
        self.draw.cmds.push(DrawCmd::Image { rect });
    }

    /// Floating titled window that can be dragged by its title bar.
    /// `default_rect` is used the first time the window is opened; position is then persisted.
    pub fn begin_window(
        &mut self,
        id: crate::id::Id,
        title: &str,
        default_rect: Rect,
        open: &mut bool,
        scroll_y: &mut f32,
    ) -> bool {
        if !*open {
            return false;
        }

        let (mut x, mut y) = *self
            .state
            .window_pos
            .entry(id.0)
            .or_insert((default_rect.min_x, default_rect.min_y));
        let w = default_rect.width().max(160.0);
        let h = default_rect.height().max(120.0);

        // Title-bar drag (continues while primary is held, even off the bar).
        let drag_id = id.child("drag");
        if let Some(drag) = self.state.window_drag {
            if drag.id == id {
                if let Some((px, py)) = self.input.pointer {
                    x = px - drag.grab_x;
                    y = py - drag.grab_y;
                }
            }
        }

        // Keep the title bar reachable inside the app window.
        let max_x = (self.screen_w - w).max(0.0);
        let max_y = (self.screen_h - WINDOW_TITLE_H).max(0.0);
        x = x.clamp(0.0, max_x);
        y = y.clamp(0.0, max_y);
        self.state.window_pos.insert(id.0, (x, y));

        let rect = Rect::from_pos_size(x, y, w, h);
        let title_bar = Rect::from_pos_size(rect.min_x, rect.min_y, rect.width(), WINDOW_TITLE_H);
        let close = Rect::from_pos_size(rect.max_x - 26.0, rect.min_y + 4.0, 20.0, 20.0);

        if self
            .input
            .pointer
            .map(|(px, py)| rect.contains(px, py))
            .unwrap_or(false)
        {
            self.state.set_hot(id.child("bg"));
        }

        // Drag handle = title bar minus close button.
        let drag_zone = Rect::from_min_max(
            title_bar.min_x,
            title_bar.min_y,
            close.min_x,
            title_bar.max_y,
        );
        let drag_hov = self
            .input
            .pointer
            .map(|(px, py)| drag_zone.contains(px, py))
            .unwrap_or(false);
        if drag_hov {
            self.state.set_hot(drag_id);
        }
        if drag_hov && self.input.primary_pressed {
            self.state.active = Some(drag_id);
            if let Some((px, py)) = self.input.pointer {
                self.state.window_drag = Some(WindowDrag {
                    id,
                    grab_x: px - x,
                    grab_y: py - y,
                });
            }
        }

        self.push_panel(rect, style::PANEL_BG);
        self.push_panel(title_bar, Color::rgba(0.10, 0.12, 0.15, 0.98));
        // Accent underline under title.
        self.push_panel(
            Rect::from_pos_size(rect.min_x, title_bar.max_y - 1.0, rect.width(), 1.0),
            style::SEPARATOR,
        );
        self.label_at(
            title_bar.min_x + style::PAD,
            title_bar.min_y + 6.0,
            title,
            style::TEXT,
            style::FONT_SCALE,
        );

        let close_id = id.child("close");
        let close_hov = self
            .input
            .pointer
            .map(|(px, py)| close.contains(px, py))
            .unwrap_or(false);
        if close_hov {
            self.state.set_hot(close_id);
        }
        if close_hov && self.input.primary_pressed {
            self.state.active = Some(close_id);
        }
        if self.input.primary_released && self.state.is_active(close_id) && close_hov {
            *open = false;
            return false;
        }
        self.push_panel(
            close,
            if close_hov {
                style::BUTTON_HOVER
            } else {
                style::BUTTON_BG
            },
        );
        self.icon_at(
            close.min_x + 2.0,
            close.min_y + 2.0,
            Icon::X,
            style::TEXT,
            16.0,
        );

        let content = Rect::from_min_max(
            rect.min_x,
            rect.min_y + WINDOW_TITLE_H,
            rect.max_x,
            rect.max_y,
        );
        let scroll_id = id.child("scroll");
        if let Some(drag) = self.state.scroll_drag {
            if drag.id == scroll_id && drag.vertical {
                if let Some((_, py)) = self.input.pointer {
                    let max = self
                        .state
                        .scroll_max
                        .get(&scroll_id.0)
                        .copied()
                        .unwrap_or(f32::MAX);
                    *scroll_y = (drag.start_scroll
                        + (py - drag.start_pointer) * drag.scroll_per_pixel)
                        .clamp(0.0, max);
                }
            }
        }
        self.apply_wheel_scroll_y(scroll_id, content, scroll_y);

        let list = if self.drawing_overlay {
            &mut self.overlay
        } else {
            &mut self.draw
        };
        list.set_clip(Some(content));
        self.clip = Some(content);
        let mut layout = Layout::new(content);
        layout.content.max_x = (layout.content.max_x - style::SCROLLBAR_W - style::SCROLLBAR_PAD)
            .max(layout.content.min_x + 32.0);
        layout.cursor_y -= scroll_y.max(0.0);
        self.layout = Some(layout);
        self.active_scroll = Some(ActiveScroll {
            id: scroll_id,
            viewport: content,
            scroll_at_begin: scroll_y.max(0.0),
        });
        true
    }

    pub fn end_window(&mut self, scroll_y: &mut f32) {
        self.end_panel_scrolled(scroll_y);
    }

    /// Draw a horizontal scrollbar for a free-form scrollable region (e.g. dock presets).
    pub fn scrollbar_x(&mut self, id: Id, viewport: Rect, content_w: f32, scroll_x: &mut f32) {
        scroll::scrollbar_x(self, id, viewport, content_w, scroll_x);
    }

    pub(crate) fn queue_combo_menu(&mut self, id: Id, rect: Rect, items: &[&str], selected: usize) {
        self.pending_combo_menu = Some(PendingComboMenu {
            id,
            rect,
            items: items.iter().map(|s| (*s).to_string()).collect(),
            selected,
        });
    }
}

fn flush_tooltip(ui: &mut GuiContext<'_>, tip: PendingTooltip) {
    let tip_rect = Rect::from_pos_size(tip.x, tip.y, tip.w, tip.h);
    ui.panel_rounded(tip_rect, style::POPUP_BG, style::RADIUS_SM);
    ui.label_at(
        tip_rect.min_x + 8.0,
        tip_rect.min_y + 6.0,
        &tip.title,
        style::TEXT,
        style::FONT_SCALE,
    );
    if let Some(sc) = &tip.shortcut {
        let sw = DrawList::text_width(sc, style::FONT_SCALE * 0.8);
        ui.label_at(
            tip_rect.max_x - sw - 10.0,
            tip_rect.min_y + 6.0,
            sc,
            style::ACCENT,
            style::FONT_SCALE * 0.8,
        );
    }
    ui.label_at(
        tip_rect.min_x + 8.0,
        tip_rect.min_y + 24.0,
        &tip.body,
        style::TEXT_DIM,
        style::FONT_SCALE * 0.8,
    );
}

fn is_combo_child(combo: crate::id::Id, hot: crate::id::Id) -> bool {
    // Item ids are combo.child("item").with(i)
    for i in 0..64u64 {
        if combo.child("item").with(i) == hot {
            return true;
        }
    }
    hot == combo.child("popup")
}
