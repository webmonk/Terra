//! Vertical panel layout cursor.

use crate::style::{self, PAD};
use crate::types::Rect;

#[derive(Debug, Clone)]
pub struct Layout {
    pub content: Rect,
    pub cursor_y: f32,
    pub pad: f32,
}

impl Layout {
    pub fn new(panel: Rect) -> Self {
        Self::with_pad(panel, PAD)
    }

    /// Layout with an explicit inset. Use `0.0` for full-bleed rows inside a chrome shell.
    pub fn with_pad(panel: Rect, pad: f32) -> Self {
        Self {
            content: Rect::from_min_max(
                panel.min_x + pad,
                panel.min_y + pad,
                panel.max_x - pad,
                panel.max_y - pad,
            ),
            cursor_y: panel.min_y + pad,
            pad,
        }
    }

    pub fn remaining_height(&self) -> f32 {
        (self.content.max_y - self.cursor_y).max(0.0)
    }

    pub fn allocate(&mut self, height: f32) -> Rect {
        let h = height.max(1.0);
        let rect = Rect::from_pos_size(self.content.min_x, self.cursor_y, self.content.width(), h);
        self.cursor_y += h;
        rect
    }

    pub fn gap(&mut self, h: f32) {
        self.cursor_y += h;
    }

    pub fn separator_rect(&mut self) -> Rect {
        self.gap(style::GAP);
        let r = self.allocate(style::SEPARATOR_H);
        self.gap(style::GAP);
        r
    }
}
