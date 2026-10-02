//! Putting an AYUV picture with alpha over an I420 frame, in place.
//!
//! Written here rather than taken from `gst_video_overlay_composition_blend`
//! because that one unpacks every destination line to AYUV and packs it back,
//! whatever the picture's alpha is, so a logo that is mostly transparent costs
//! what a full opaque rectangle does. This skips a transparent pixel with one
//! comparison, copies an opaque one, and mixes only the edges. It also crops a
//! window out of the picture with no copy, which is all a ticker is.
//!
//! Chroma is mixed per 2x2 block, weighted by each pixel's alpha, so a thin
//! coloured edge does not bleed the colour of the transparent pixel next to it.

/// A rectangle in pixels. May lie partly outside the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub fn new(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    pub fn right(&self) -> i32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }

    /// The part of this rectangle inside `other`, if there is any.
    pub fn within(&self, other: &Rect) -> Option<Rect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let r = self.right().min(other.right());
        let b = self.bottom().min(other.bottom());
        (r > x && b > y).then(|| Rect::new(x, y, r - x, b - y))
    }
}

/// The three planes of one writable I420 frame.
pub struct Planes<'a> {
    pub y: &'a mut [u8],
    pub u: &'a mut [u8],
    pub v: &'a mut [u8],
    pub strides: [usize; 3],
    pub width: i32,
    pub height: i32,
}

/// One AYUV picture to read from.
pub struct Source<'a> {
    pub data: &'a [u8],
    pub stride: usize,
}

/// Where a picture goes: `window` of the source, stretched onto `to`, and
/// only the part of that inside `clip` drawn, at `alpha` (255 is as is).
#[derive(Debug, Clone, Copy)]
pub struct Draw {
    pub window: Rect,
    pub to: Rect,
    pub clip: Rect,
    pub alpha: u8,
}

/// Draw one picture. A window or a destination with no area draws nothing.
pub fn draw(dst: &mut Planes<'_>, src: &Source<'_>, d: &Draw) {
    let frame = Rect::new(0, 0, dst.width, dst.height);
    let Some(area) = d.to.within(&d.clip).and_then(|a| a.within(&frame)) else { return };
    if d.window.w <= 0 || d.window.h <= 0 || d.alpha == 0 {
        return;
    }
    let cols: Vec<usize> = (area.x..area.right()).map(|x| map(x, &d.to, d.window.x, d.window.w) * 4).collect();
    let rows: Vec<usize> = (area.y..area.bottom()).map(|y| map_y(y, &d.to, d.window.y, d.window.h) * src.stride).collect();
    let ga = d.alpha as u32;
    let alpha_at = |s: usize| if ga == 255 { src.data[s] as u32 } else { src.data[s] as u32 * ga / 255 };
    luma(dst, src, &area, &cols, &rows, &alpha_at);
    chroma(dst, src, &area, &cols, &rows, &alpha_at);
}

fn map(x: i32, to: &Rect, from: i32, span: i32) -> usize {
    (from as i64 + (x - to.x) as i64 * span as i64 / to.w.max(1) as i64).max(0) as usize
}

fn map_y(y: i32, to: &Rect, from: i32, span: i32) -> usize {
    (from as i64 + (y - to.y) as i64 * span as i64 / to.h.max(1) as i64).max(0) as usize
}

fn mix(over: u32, under: u8, a: u32) -> u8 {
    ((over * a + under as u32 * (255 - a) + 127) / 255) as u8
}

fn luma(dst: &mut Planes<'_>, src: &Source<'_>, area: &Rect, cols: &[usize], rows: &[usize], alpha_at: &dyn Fn(usize) -> u32) {
    for (j, srow) in rows.iter().enumerate() {
        let start = (area.y as usize + j) * dst.strides[0] + area.x as usize;
        let line = &mut dst.y[start..start + area.w as usize];
        for (d, col) in line.iter_mut().zip(cols) {
            let s = srow + col;
            let a = alpha_at(s);
            if a == 0 {
                continue;
            }
            let y = src.data[s + 1];
            *d = if a == 255 { y } else { mix(y as u32, *d, a) };
        }
    }
}

fn chroma(dst: &mut Planes<'_>, src: &Source<'_>, area: &Rect, cols: &[usize], rows: &[usize], alpha_at: &dyn Fn(usize) -> u32) {
    for cy in area.y / 2..(area.bottom() + 1) / 2 {
        for cx in area.x / 2..(area.right() + 1) / 2 {
            let (mut sa, mut su, mut sv) = (0u32, 0u32, 0u32);
            for ly in [cy * 2, cy * 2 + 1] {
                if ly < area.y || ly >= area.bottom() {
                    continue;
                }
                for lx in [cx * 2, cx * 2 + 1] {
                    if lx < area.x || lx >= area.right() {
                        continue;
                    }
                    let s = rows[(ly - area.y) as usize] + cols[(lx - area.x) as usize];
                    let a = alpha_at(s);
                    sa += a;
                    su += src.data[s + 2] as u32 * a;
                    sv += src.data[s + 3] as u32 * a;
                }
            }
            if sa == 0 {
                continue;
            }
            let a = (sa / 4).min(255);
            let (u, v) = (su / sa, sv / sa);
            let iu = cy as usize * dst.strides[1] + cx as usize;
            let iv = cy as usize * dst.strides[2] + cx as usize;
            dst.u[iu] = if a == 255 { u as u8 } else { mix(u, dst.u[iu], a) };
            dst.v[iv] = if a == 255 { v as u8 } else { mix(v, dst.v[iv], a) };
        }
    }
}

#[cfg(test)]
#[path = "blend_tests.rs"]
mod tests;
