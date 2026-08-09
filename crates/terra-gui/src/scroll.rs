//! Scrollbar drawing + drag interaction for overflow panels.

use crate::context::GuiContext;
use crate::id::Id;
use crate::state::ScrollDrag;
use crate::style::{self, SCROLLBAR_PAD, SCROLLBAR_W};
use crate::types::Rect;

/// Vertical scrollbar on the right edge of `viewport`.
///
/// `content_h` is the full content height in logical px (including padding),
/// measured from `viewport.min_y`.
pub fn scrollbar_y(
    ui: &mut GuiContext<'_>,
    id: Id,
    viewport: Rect,
    content_h: f32,
    scroll_y: &mut f32,
) {
    let view_h = viewport.height().max(1.0);
    let content_h = content_h.max(view_h);
    let max_scroll = (content_h - view_h).max(0.0);
    *scroll_y = scroll_y.clamp(0.0, max_scroll);
    if max_scroll < 1.0 {
        return;
    }

    let track = Rect::from_pos_size(
        viewport.max_x - SCROLLBAR_W - SCROLLBAR_PAD,
        viewport.min_y + SCROLLBAR_PAD,
        SCROLLBAR_W,
        (view_h - SCROLLBAR_PAD * 2.0).max(8.0),
    );
    // Never clamp with min > max — short panels (e.g. menus) can have track < 22px.
    let min_thumb = 22.0_f32.min(track.height());
    let thumb_h = (track.height() * (view_h / content_h)).clamp(min_thumb, track.height());
    let travel = (track.height() - thumb_h).max(0.0);
    let t = if max_scroll > 0.0 {
        *scroll_y / max_scroll
    } else {
        0.0
    };
    let mut thumb = Rect::from_pos_size(
        track.min_x,
        track.min_y + travel * t,
        track.width(),
        thumb_h,
    );

    let thumb_id = id.child("thumb");
    let track_id = id.child("track");

    // Continue thumb drag only while the primary button stays down.
    if let Some(drag) = ui.state.scroll_drag {
        if drag.id == id && drag.vertical {
            if !ui.input.primary_down {
                ui.state.scroll_drag = None;
                if ui.state.is_active(thumb_id) {
                    ui.state.active = None;
                }
            } else if let Some((_, py)) = ui.input.pointer {
                let delta = py - drag.start_pointer;
                *scroll_y =
                    (drag.start_scroll + delta * drag.scroll_per_pixel).clamp(0.0, max_scroll);
                let t = *scroll_y / max_scroll;
                thumb = Rect::from_pos_size(
                    track.min_x,
                    track.min_y + travel * t,
                    track.width(),
                    thumb_h,
                );
                ui.state.set_hot(thumb_id);
                ui.state.active = Some(thumb_id);
            }
        }
    }

    let pointer_in_track = ui
        .input
        .pointer
        .map(|(x, y)| track.contains(x, y))
        .unwrap_or(false);
    let pointer_in_thumb = ui
        .input
        .pointer
        .map(|(x, y)| thumb.contains(x, y))
        .unwrap_or(false);

    if pointer_in_thumb {
        ui.state.set_hot(thumb_id);
    } else if pointer_in_track {
        ui.state.set_hot(track_id);
    }

    if pointer_in_thumb && ui.input.primary_pressed {
        ui.state.active = Some(thumb_id);
        if let Some((_, py)) = ui.input.pointer {
            ui.state.scroll_drag = Some(ScrollDrag {
                id,
                vertical: true,
                start_scroll: *scroll_y,
                start_pointer: py,
                scroll_per_pixel: if travel > 0.5 {
                    max_scroll / travel
                } else {
                    0.0
                },
            });
        }
    } else if pointer_in_track && ui.input.primary_pressed && !pointer_in_thumb {
        // Jump so the thumb centers on the click.
        if let Some((_, py)) = ui.input.pointer {
            let center = py - thumb_h * 0.5;
            let t = ((center - track.min_y) / travel.max(1.0)).clamp(0.0, 1.0);
            *scroll_y = t * max_scroll;
            ui.state.active = Some(thumb_id);
            ui.state.scroll_drag = Some(ScrollDrag {
                id,
                vertical: true,
                start_scroll: *scroll_y,
                start_pointer: py,
                scroll_per_pixel: if travel > 0.5 {
                    max_scroll / travel
                } else {
                    0.0
                },
            });
        }
    }

    let thumb_hot = ui.state.is_hot(thumb_id) || ui.state.is_active(thumb_id);
    ui.panel(track, style::SCROLLBAR_TRACK);
    ui.panel(
        thumb,
        if thumb_hot {
            style::SCROLLBAR_THUMB_HOVER
        } else {
            style::SCROLLBAR_THUMB
        },
    );
}

/// Horizontal scrollbar along the bottom of `viewport`.
pub fn scrollbar_x(
    ui: &mut GuiContext<'_>,
    id: Id,
    viewport: Rect,
    content_w: f32,
    scroll_x: &mut f32,
) {
    let view_w = viewport.width().max(1.0);
    let content_w = content_w.max(view_w);
    let max_scroll = (content_w - view_w).max(0.0);
    *scroll_x = scroll_x.clamp(0.0, max_scroll);
    if max_scroll < 1.0 {
        return;
    }

    let track = Rect::from_pos_size(
        viewport.min_x + SCROLLBAR_PAD,
        viewport.max_y - SCROLLBAR_W - SCROLLBAR_PAD,
        (view_w - SCROLLBAR_PAD * 2.0).max(8.0),
        SCROLLBAR_W,
    );
    let thumb_w = {
        let min_thumb = 28.0_f32.min(track.width());
        (track.width() * (view_w / content_w)).clamp(min_thumb, track.width())
    };
    let travel = (track.width() - thumb_w).max(0.0);
    let t = *scroll_x / max_scroll;
    let mut thumb = Rect::from_pos_size(
        track.min_x + travel * t,
        track.min_y,
        thumb_w,
        track.height(),
    );

    let thumb_id = id.child("thumb");
    let track_id = id.child("track");

    if let Some(drag) = ui.state.scroll_drag {
        if drag.id == id && !drag.vertical {
            if !ui.input.primary_down {
                ui.state.scroll_drag = None;
                if ui.state.is_active(thumb_id) {
                    ui.state.active = None;
                }
            } else if let Some((px, _)) = ui.input.pointer {
                let delta = px - drag.start_pointer;
                *scroll_x =
                    (drag.start_scroll + delta * drag.scroll_per_pixel).clamp(0.0, max_scroll);
                let t = *scroll_x / max_scroll;
                thumb = Rect::from_pos_size(
                    track.min_x + travel * t,
                    track.min_y,
                    thumb_w,
                    track.height(),
                );
                ui.state.set_hot(thumb_id);
                ui.state.active = Some(thumb_id);
            }
        }
    }

    let pointer_in_track = ui
        .input
        .pointer
        .map(|(x, y)| track.contains(x, y))
        .unwrap_or(false);
    let pointer_in_thumb = ui
        .input
        .pointer
        .map(|(x, y)| thumb.contains(x, y))
        .unwrap_or(false);

    if pointer_in_thumb {
        ui.state.set_hot(thumb_id);
    } else if pointer_in_track {
        ui.state.set_hot(track_id);
    }

    if pointer_in_thumb && ui.input.primary_pressed {
        ui.state.active = Some(thumb_id);
        if let Some((px, _)) = ui.input.pointer {
            ui.state.scroll_drag = Some(ScrollDrag {
                id,
                vertical: false,
                start_scroll: *scroll_x,
                start_pointer: px,
                scroll_per_pixel: if travel > 0.5 {
                    max_scroll / travel
                } else {
                    0.0
                },
            });
        }
    } else if pointer_in_track && ui.input.primary_pressed && !pointer_in_thumb {
        if let Some((px, _)) = ui.input.pointer {
            let center = px - thumb_w * 0.5;
            let t = ((center - track.min_x) / travel.max(1.0)).clamp(0.0, 1.0);
            *scroll_x = t * max_scroll;
            ui.state.active = Some(thumb_id);
            ui.state.scroll_drag = Some(ScrollDrag {
                id,
                vertical: false,
                start_scroll: *scroll_x,
                start_pointer: px,
                scroll_per_pixel: if travel > 0.5 {
                    max_scroll / travel
                } else {
                    0.0
                },
            });
        }
    }

    let thumb_hot = ui.state.is_hot(thumb_id) || ui.state.is_active(thumb_id);
    ui.panel(track, style::SCROLLBAR_TRACK);
    ui.panel(
        thumb,
        if thumb_hot {
            style::SCROLLBAR_THUMB_HOVER
        } else {
            style::SCROLLBAR_THUMB
        },
    );
}
