//! The box behind the words, the words on it, and the result in the AYUV the
//! overlay board draws. Plain loops over one small buffer, once per change.

use super::glyphs::Glyphs;
use super::style::{colour, Align, Look, Valign};

/// Straight alpha RGBA, `width * 4` to a row.
pub struct Rgba {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Lay `glyphs` on a `size` box with the look's background, padding and
/// corners, all at `scale`.
pub fn compose(look: &Look, glyphs: Option<&Glyphs>, size: (u32, u32), scale: f64) -> Rgba {
    let (w, h) = (size.0.max(1), size.1.max(1));
    let mut out = Rgba { data: vec![0u8; (w * h * 4) as usize], width: w, height: h };
    if let Ok(bg) = colour(&look.background) {
        rounded_box(&mut out, bg, (look.radius * scale) as f32);
    }
    if let Some(g) = glyphs {
        let pad = (look.padding * scale).round() as i64;
        let x = match look.align {
            Align::Left => pad,
            Align::Center => (w as i64 - g.width as i64) / 2,
            Align::Right => w as i64 - pad - g.width as i64,
        };
        let y = match look.valign {
            Valign::Top => pad,
            Valign::Middle => (h as i64 - g.height as i64) / 2,
            Valign::Bottom => h as i64 - pad - g.height as i64,
        };
        over(&mut out, g, x, y);
    }
    out
}

/// The size a box needs to hold `glyphs` with the look's padding round it.
pub fn fit(look: &Look, glyphs: Option<&Glyphs>, scale: f64) -> (u32, u32) {
    let pad = (look.padding * scale).round() as u32 * 2;
    let line = (look.size * scale * 1.3).round() as u32;
    let (gw, gh) = glyphs.map_or((0, line), |g| (g.width, g.height));
    ((gw + pad).max(2), (gh + pad).max(2))
}

/// Fill the whole picture with `rgba`, cutting the corners round with a one
/// pixel soft edge.
fn rounded_box(out: &mut Rgba, rgba: [u8; 4], radius: f32) {
    let (w, h) = (out.width as f32, out.height as f32);
    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    for y in 0..out.height {
        for x in 0..out.width {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let cx = if px < r { r } else if px > w - r { w - r } else { px };
            let cy = if py < r { r } else if py > h - r { h - r } else { py };
            let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            let cover = (r - d + 0.5).clamp(0.0, 1.0);
            let a = (rgba[3] as f32 * if r > 0.0 { cover } else { 1.0 }).round() as u8;
            let p = ((y * out.width + x) * 4) as usize;
            out.data[p..p + 4].copy_from_slice(&[rgba[0], rgba[1], rgba[2], a]);
        }
    }
}

/// `g` over `out` with its top left at (x, y), straight alpha both sides.
fn over(out: &mut Rgba, g: &Glyphs, x: i64, y: i64) {
    for gy in 0..g.height as i64 {
        let oy = y + gy;
        if oy < 0 || oy >= out.height as i64 {
            continue;
        }
        for gx in 0..g.width as i64 {
            let ox = x + gx;
            if ox < 0 || ox >= out.width as i64 {
                continue;
            }
            let s = ((gy * g.width as i64 + gx) * 4) as usize;
            let sa = g.rgba[s + 3] as u32;
            if sa == 0 {
                continue;
            }
            let d = ((oy * out.width as i64 + ox) * 4) as usize;
            let da = out.data[d + 3] as u32;
            let oa = sa * 255 + da * (255 - sa);
            for c in 0..3 {
                let v = (g.rgba[s + c] as u32 * sa * 255 + out.data[d + c] as u32 * da * (255 - sa)) / oa.max(1);
                out.data[d + c] = v.min(255) as u8;
            }
            out.data[d + 3] = (oa / 255).min(255) as u8;
        }
    }
}

/// RGBA to the AYUV the board draws: BT.709, limited range, like the canvas.
pub fn to_ayuv(rgba: &Rgba) -> Vec<u8> {
    let mut out = vec![0u8; rgba.data.len()];
    for (src, dst) in rgba.data.chunks_exact(4).zip(out.chunks_exact_mut(4)) {
        let (r, g, b) = (src[0] as f32, src[1] as f32, src[2] as f32);
        let y = 16.0 + (0.2126 * r + 0.7152 * g + 0.0722 * b) * 219.0 / 255.0;
        let u = 128.0 + (-0.1146 * r - 0.3854 * g + 0.5 * b) * 224.0 / 255.0;
        let v = 128.0 + (0.5 * r - 0.4542 * g - 0.0458 * b) * 224.0 / 255.0;
        dst.copy_from_slice(&[src[3], y.round() as u8, u.round().clamp(0.0, 255.0) as u8, v.round().clamp(0.0, 255.0) as u8]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_and_black_land_on_the_limited_range_ends() {
        let px = Rgba { data: vec![255, 255, 255, 255, 0, 0, 0, 128], width: 2, height: 1 };
        assert_eq!(to_ayuv(&px), vec![255, 235, 128, 128, 128, 16, 128, 128]);
    }

    #[test]
    fn a_rounded_box_is_clear_in_the_corner_and_solid_in_the_middle() {
        let look = Look { background: "#ff0000".into(), radius: 8.0, ..Look::default() };
        let out = compose(&look, None, (40, 20), 1.0);
        assert_eq!(out.data[3], 0, "the very corner is outside the curve");
        let mid = ((10 * 40 + 20) * 4) as usize;
        assert_eq!(&out.data[mid..mid + 4], &[255, 0, 0, 255]);
    }
}
