//! Screen, Add and luma key: an AYUV picture over an I420 frame by a blend
//! mode other than plain alpha.
//!
//! The canvas is I420 and stays I420, so the modes are worked in Y'CbCr
//! rather than RGB. Add is exact there, short of where it clips, because
//! the conversion is linear: the clip's light is added to each component.
//! Screen is `a + b - ab`, which is not linear, so its luma is exact and its
//! chroma is the first order term of the same product, `Ud(1 - Ys) + Us(1 -
//! Yd)`. On the light leaks, flames and bokeh these modes are for, which are
//! warm light on black, the two are hard to tell apart side by side.
//!
//! Every mode skips a source pixel that adds nothing (black for Screen and
//! Add, clear for luma), so a clip that is mostly black costs mostly a
//! comparison per pixel. Chroma is done before luma because it reads the
//! frame's own luma, which the luma pass then changes.

use super::blend::{self, Draw, Planes, Rect, Source};

/// How a picture goes over the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Screen,
    Add,
    Luma,
}

/// Draw `src` over `dst` by `mode`. `Normal` is the board's own blend.
pub fn draw(dst: &mut Planes<'_>, src: &Source<'_>, d: &Draw, mode: Mode) {
    if mode == Mode::Normal {
        return blend::draw(dst, src, d);
    }
    let frame = Rect::new(0, 0, dst.width, dst.height);
    let Some(area) = d.to.within(&d.clip).and_then(|a| a.within(&frame)) else { return };
    if d.window.w <= 0 || d.window.h <= 0 || d.alpha == 0 {
        return;
    }
    let map = |at: i32, to: i32, span: i32, from: i32, size: i32| (from as i64 + (at - to) as i64 * span as i64 / size.max(1) as i64).max(0) as usize;
    let cols: Vec<usize> = (area.x..area.right()).map(|x| map(x, d.to.x, d.window.w, d.window.x, d.to.w) * 4).collect();
    let rows: Vec<usize> = (area.y..area.bottom()).map(|y| map(y, d.to.y, d.window.h, d.window.y, d.to.h) * src.stride).collect();
    let g = d.alpha as i32;
    chroma(dst, src, &area, &cols, &rows, mode, g);
    luma(dst, src, &area, &cols, &rows, mode, g);
}

/// Limited range luma as 0 to 255.
fn level(y: u8) -> i32 {
    ((y as i32 - 16) * 255 / 219).clamp(0, 255)
}

fn unlevel(v: i32) -> u8 {
    (16 + v.clamp(0, 255) * 219 / 255) as u8
}

/// `under` moved towards `over` by `g` of 255.
fn toward(under: i32, over: i32, g: i32) -> i32 {
    under + (over - under) * g / 255
}

fn luma(dst: &mut Planes<'_>, src: &Source<'_>, area: &Rect, cols: &[usize], rows: &[usize], mode: Mode, g: i32) {
    for (j, srow) in rows.iter().enumerate() {
        let start = (area.y as usize + j) * dst.strides[0] + area.x as usize;
        let line = &mut dst.y[start..start + area.w as usize];
        for (d, col) in line.iter_mut().zip(cols) {
            let s = srow + col;
            let ys = level(src.data[s + 1]);
            let a = src.data[s] as i32;
            if ys == 0 || a == 0 {
                continue;
            }
            let yd = level(*d);
            let out = match mode {
                Mode::Add => yd + ys * a / 255,
                Mode::Screen => yd + (ys * a / 255) * (255 - yd) / 255,
                _ => yd + (ys - yd) * (ys * a / 255) / 255,
            };
            *d = unlevel(toward(yd, out, g));
        }
    }
}

fn chroma(dst: &mut Planes<'_>, src: &Source<'_>, area: &Rect, cols: &[usize], rows: &[usize], mode: Mode, g: i32) {
    for cy in area.y / 2..(area.bottom() + 1) / 2 {
        let ly = ((cy * 2).max(area.y) - area.y) as usize;
        let srow = rows[ly];
        let yrow = (cy as usize * 2) * dst.strides[0];
        for cx in area.x / 2..(area.right() + 1) / 2 {
            let lx = ((cx * 2).max(area.x) - area.x) as usize;
            let s = srow + cols[lx];
            let (a, ys) = (src.data[s] as i32, level(src.data[s + 1]));
            let (us, vs) = (src.data[s + 2] as i32 - 128, src.data[s + 3] as i32 - 128);
            if a == 0 || (ys == 0 && us.abs() < 2 && vs.abs() < 2) {
                continue;
            }
            let yd = level(dst.y[yrow + cx as usize * 2]);
            let iu = cy as usize * dst.strides[1] + cx as usize;
            let iv = cy as usize * dst.strides[2] + cx as usize;
            for (plane, i, c) in [(0, iu, us), (1, iv, vs)] {
                let p = if plane == 0 { &mut dst.u[i] } else { &mut dst.v[i] };
                let cd = *p as i32 - 128;
                let c = if mode == Mode::Luma { c } else { c * a / 255 };
                let out = match mode {
                    Mode::Add => cd + c,
                    Mode::Screen => cd * (255 - ys * a / 255) / 255 + c * (255 - yd) / 255,
                    _ => cd + (c - cd) * (ys * a / 255) / 255,
                };
                *p = (128 + toward(cd, out, g)).clamp(16, 240) as u8;
            }
        }
    }
}

#[cfg(test)]
#[path = "modes_tests.rs"]
mod tests;
