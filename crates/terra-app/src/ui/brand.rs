//! Shared brand assets (toolbar + project home).

use std::sync::OnceLock;

/// Load `assets/logo.png`, trim empty margins, and punch near-black to alpha
/// so the wordmark composites cleanly on dark chrome (not a black box).
pub fn brand_logo() -> &'static (u32, u32, Vec<u8>) {
    static LOGO: OnceLock<(u32, u32, Vec<u8>)> = OnceLock::new();
    LOGO.get_or_init(|| {
        let bytes = include_bytes!("../../../../assets/logo.png");
        let img = image::load_from_memory(bytes)
            .expect("assets/logo.png")
            .to_rgba8();
        trim_logo(img)
    })
}

fn trim_logo(img: image::RgbaImage) -> (u32, u32, Vec<u8>) {
    let w = img.width() as usize;
    let h = img.height() as usize;
    let raw = img.as_raw();

    let mut min_x = w;
    let mut min_y = h;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    let mut found = false;

    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let r = raw[i];
            let g = raw[i + 1];
            let b = raw[i + 2];
            let a = raw[i + 3];
            // Content: visible, non-near-black pixels (white mark on dark plate).
            if a > 12 && (r > 28 || g > 28 || b > 28) {
                found = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    if !found {
        return (img.width(), img.height(), img.into_raw());
    }

    // Small pad so edges aren't clipped after scale.
    let pad = 4usize;
    let x0 = min_x.saturating_sub(pad);
    let y0 = min_y.saturating_sub(pad);
    let x1 = (max_x + pad).min(w - 1);
    let y1 = (max_y + pad).min(h - 1);
    let cw = (x1 - x0 + 1) as u32;
    let ch = (y1 - y0 + 1) as u32;

    let mut out = Vec::with_capacity((cw * ch * 4) as usize);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let i = (y * w + x) * 4;
            let r = raw[i];
            let g = raw[i + 1];
            let b = raw[i + 2];
            let a = raw[i + 3];
            if a > 0 && r < 18 && g < 18 && b < 18 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                out.extend_from_slice(&[r, g, b, a]);
            }
        }
    }
    (cw, ch, out)
}
