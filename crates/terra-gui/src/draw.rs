//! Immediate-mode draw list built each frame.

use crate::font;
use crate::icons::{Icon, ICON_PX};
use crate::types::{Align, Color, Rect};

#[derive(Debug, Clone)]
pub enum DrawCmd {
    /// Change GPU scissor in logical pixels (`None` = full surface).
    SetClip(Option<Rect>),
    /// Solid axis-aligned quad in logical pixels.
    Rect { rect: Rect, color: Color },
    /// Solid rounded rectangle (SDF) in logical pixels.
    RoundedRect {
        rect: Rect,
        color: Color,
        radius: f32,
    },
    /// Textured glyph quad in *physical framebuffer pixels* (1:1 with atlas texels).
    /// `uv` is atlas [u0,v0,u1,v1].
    Glyph {
        x_px: f32,
        y_px: f32,
        w_px: f32,
        h_px: f32,
        color: Color,
        uv: [f32; 4],
    },
    /// RGBA image region from the per-frame atlas (`image_id` indexes GuiContext::images).
    Image { rect: Rect, image_id: u32 },
}

#[derive(Debug, Default)]
pub struct DrawList {
    pub cmds: Vec<DrawCmd>,
}

impl DrawList {
    pub fn clear(&mut self) {
        self.cmds.clear();
    }

    pub fn set_clip(&mut self, clip: Option<Rect>) {
        self.cmds.push(DrawCmd::SetClip(clip));
    }

    pub fn panel(&mut self, rect: Rect, color: Color) {
        self.cmds.push(DrawCmd::Rect { rect, color });
    }

    pub fn panel_rounded(&mut self, rect: Rect, color: Color, radius: f32) {
        self.cmds.push(DrawCmd::RoundedRect {
            rect,
            color,
            radius: radius.max(0.0),
        });
    }

    pub fn label(&mut self, x: f32, y: f32, text: &str, color: Color, scale: f32) {
        self.label_aligned(x, y, text, color, scale, Align::Left);
    }

    pub fn label_aligned(
        &mut self,
        x: f32,
        y: f32,
        text: &str,
        color: Color,
        scale: f32,
        align: Align,
    ) {
        let scale = font::snap_text_scale(scale);
        let ppp = font::active_ppp().max(0.5);
        let width = Self::text_width(text, scale);
        let native = (scale - 1.0).abs() < 0.001;

        let mut pen_x = match align {
            Align::Left => (x * ppp).round(),
            Align::Center => ((x - width * 0.5) * ppp).round(),
        };
        let pen_y = (y * ppp).round();

        for ch in text.chars() {
            let byte = font::printable_ascii(ch);
            let g = font::glyph(byte);
            let adv_px = (g.advance_px * scale).round().max(0.0);

            if g.px_w > 0 && g.px_h > 0 {
                let (gw, gh, gx, gy) = if native {
                    (
                        g.px_w as f32,
                        g.px_h as f32,
                        pen_x + g.x0_px.round(),
                        pen_y + g.y0_px.round(),
                    )
                } else {
                    (
                        (g.px_w as f32 * scale).round().max(1.0),
                        (g.px_h as f32 * scale).round().max(1.0),
                        pen_x + (g.x0_px * scale).round(),
                        pen_y + (g.y0_px * scale).round(),
                    )
                };
                self.cmds.push(DrawCmd::Glyph {
                    x_px: gx,
                    y_px: gy,
                    w_px: gw,
                    h_px: gh,
                    color,
                    uv: g.uv,
                });
            }
            pen_x += adv_px;
        }
    }

    pub fn text_width(text: &str, scale: f32) -> f32 {
        font::text_width(text, scale)
    }

    pub fn truncate_to_width(text: &str, scale: f32, max_w: f32) -> String {
        font::truncate_to_width(text, scale, max_w)
    }

    /// Draw a Lucide icon with top-left at `(x, y)` and logical square size `size`.
    /// Prefer `ICON_PX` (16) for crisp 1:1 atlas sampling.
    pub fn icon(&mut self, x: f32, y: f32, icon: Icon, color: Color, size: f32) {
        let size = size.max(8.0);
        // Snap non-native sizes to the nearest whole logical pixel to reduce blur.
        let size = if (size - ICON_PX).abs() < 0.5 {
            ICON_PX
        } else {
            size.round().max(8.0)
        };
        let scale = size / ICON_PX;
        let ppp = font::active_ppp().max(0.5);
        let g = font::icon_glyph(icon);
        if g.px_w == 0 || g.px_h == 0 {
            return;
        }

        let origin_x = (x * ppp).round();
        let origin_y = (y * ppp).round();
        let native = (scale - 1.0).abs() < 0.001;
        let (gw, gh, gx, gy) = if native {
            (
                g.px_w as f32,
                g.px_h as f32,
                origin_x + g.x0_px.round(),
                origin_y + g.y0_px.round(),
            )
        } else {
            (
                (g.px_w as f32 * scale).round().max(1.0),
                (g.px_h as f32 * scale).round().max(1.0),
                origin_x + (g.x0_px * scale).round(),
                origin_y + (g.y0_px * scale).round(),
            )
        };
        self.cmds.push(DrawCmd::Glyph {
            x_px: gx,
            y_px: gy,
            w_px: gw,
            h_px: gh,
            color,
            uv: g.uv,
        });
    }
}
