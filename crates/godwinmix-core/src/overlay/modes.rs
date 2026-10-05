//! Screen, Add and luma key: an AYUV picture over an I420 frame by a blend
//! mode other than plain alpha, and plain alpha again for a whole frame.
//!
//! The canvas is I420 and stays I420, so the modes are worked in Y'CbCr
//! rather than RGB, on limited range luma (16 to 235) as it is. Add is exact
//! there, short of where it clips, because the conversion is linear: the
//! clip's light is added to each component. Screen is `a + b - ab`, which is
//! not linear, so its luma is exact and its chroma is the first order term
//! of the same product, `Ud(1 - Ys) + Us(1 - Yd)`. On the light leaks,
//! flames and bokeh these modes are for, warm light on black, the two are
//! hard to tell apart side by side.
//!
//! Each mode is its own loop with no branch per pixel and no division
//! (`/ 219` is a multiply by 299 and a shift), so the compiler can do many
//! pixels at once. Chroma is done before luma because it reads the frame's
//! own luma, which the luma pass then changes.

use super::blend::{Draw, Planes, Rect, Source};

/// How a picture goes over the frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Screen,
    Add,
    Luma,
}

/// One mode's arithmetic. `y`, `d` are limited range luma; `a` is the
/// pixel's weight 0 to 256; chroma is centred on 0.
trait Blend {
    fn luma(d: i32, y: i32, a: i32) -> i32;
    fn chroma(cd: i32, c: i32, a: i32, ys: i32, yd: i32) -> i32;
}

/// 0 to 219 luma as 0 to 256, without a division.
#[inline(always)]
fn unit(v: i32) -> i32 {
    (v * 299 + 255) >> 8
}

struct Normal;
struct Screen;
struct Add;
struct Luma;

impl Blend for Normal {
    fn luma(d: i32, y: i32, a: i32) -> i32 {
        d + (((y - d) * a) >> 8)
    }
    fn chroma(cd: i32, c: i32, a: i32, _: i32, _: i32) -> i32 {
        cd + (((c - cd) * a) >> 8)
    }
}

impl Blend for Add {
    fn luma(d: i32, y: i32, a: i32) -> i32 {
        d + (((y - 16).max(0) * a) >> 8)
    }
    fn chroma(cd: i32, c: i32, a: i32, _: i32, _: i32) -> i32 {
        cd + ((c * a) >> 8)
    }
}

impl Blend for Screen {
    fn luma(d: i32, y: i32, a: i32) -> i32 {
        let ys = ((y - 16).max(0) * a) >> 8;
        let yd = (d - 16).max(0);
        d + ((ys * (219 - yd).max(0) * 299) >> 16)
    }
    fn chroma(cd: i32, c: i32, a: i32, ys: i32, yd: i32) -> i32 {
        let s = unit((ys * a) >> 8);
        ((cd * (256 - s)) >> 8) + ((((c * a) >> 8) * (256 - unit(yd))) >> 8)
    }
}

impl Blend for Luma {
    fn luma(d: i32, y: i32, a: i32) -> i32 {
        let k = (unit((y - 16).max(0)) * a) >> 8;
        d + (((y - d) * k) >> 8)
    }
    fn chroma(cd: i32, c: i32, a: i32, ys: i32, _: i32) -> i32 {
        let k = (unit(ys) * a) >> 8;
        cd + (((c - cd) * k) >> 8)
    }
}

/// Draw `src` over `dst` by `mode`.
pub fn draw(dst: &mut Planes<'_>, src: &Source<'_>, d: &Draw, mode: Mode) {
    match mode {
        Mode::Normal => run::<Normal>(dst, src, d),
        Mode::Screen => run::<Screen>(dst, src, d),
        Mode::Add => run::<Add>(dst, src, d),
        Mode::Luma => run::<Luma>(dst, src, d),
    }
}

fn run<B: Blend>(dst: &mut Planes<'_>, src: &Source<'_>, d: &Draw) {
    let frame = Rect::new(0, 0, dst.width, dst.height);
    let Some(area) = d.to.within(&d.clip).and_then(|a| a.within(&frame)) else { return };
    if d.window.w <= 0 || d.window.h <= 0 || d.alpha == 0 {
        return;
    }
    let map = |at: i32, to: i32, span: i32, from: i32, size: i32| (from as i64 + (at - to) as i64 * span as i64 / size.max(1) as i64).max(0) as usize;
    let cols: Vec<usize> = (area.x..area.right()).map(|x| map(x, d.to.x, d.window.w, d.window.x, d.to.w) * 4).collect();
    let rows: Vec<usize> = (area.y..area.bottom()).map(|y| map(y, d.to.y, d.window.h, d.window.y, d.to.h) * src.stride).collect();
    let g = d.alpha as i32 + (d.alpha as i32 >> 7);
    // A picture at its own size, or at exactly half of it (light is decoded
    // at half, see `fx::decode_size`), is read straight along its row: no
    // column table, so the compiler can take many pixels at a time.
    let step = match (d.window.w * 2 == d.to.w, d.window.w == d.to.w) {
        (_, true) => 1,
        (true, _) if area.x == d.to.x && area.w % 2 == 0 => 2,
        _ => 0,
    };
    chroma::<B>(dst, src, &area, &cols, &rows, g, step);
    for (j, srow) in rows.iter().enumerate() {
        let start = (area.y as usize + j) * dst.strides[0] + area.x as usize;
        let line = &mut dst.y[start..start + area.w as usize];
        let one = |d: &mut u8, p: &[u8]| {
            let a = (p[0] as i32 * g) >> 8;
            *d = B::luma(*d as i32, p[1] as i32, a + (a >> 7)).clamp(16, 235) as u8;
        };
        if step > 0 {
            let (from, n) = (srow + cols[0], line.len() / step);
            let pixels = src.data[from..from + n * 4].chunks_exact(4);
            line.chunks_exact_mut(step).zip(pixels).for_each(|(ds, p)| ds.iter_mut().for_each(|d| one(d, p)));
        } else {
            line.iter_mut().zip(&cols).for_each(|(d, &c)| one(d, &src.data[srow + c..srow + c + 4]));
        }
    }
}

fn chroma<B: Blend>(dst: &mut Planes<'_>, src: &Source<'_>, area: &Rect, cols: &[usize], rows: &[usize], g: i32, step: usize) {
    let even = area.x % 2 == 0 && area.w % 2 == 0;
    let weight = |p: &[u8]| {
        let a = (p[0] as i32 * g) >> 8;
        a + (a >> 7)
    };
    let one = |u: &mut u8, v: &mut u8, p: &[u8], yd: u8| {
        let (a, ys, yd) = (weight(p), (p[1] as i32 - 16).max(0), (yd as i32 - 16).max(0));
        *u = (128 + B::chroma(*u as i32 - 128, p[2] as i32 - 128, a, ys, yd)).clamp(16, 240) as u8;
        *v = (128 + B::chroma(*v as i32 - 128, p[3] as i32 - 128, a, ys, yd)).clamp(16, 240) as u8;
    };
    for cy in area.y / 2..(area.bottom() + 1) / 2 {
        let ly = ((cy * 2).max(area.y) - area.y) as usize;
        let srow = rows[ly];
        let (c0, c1) = (area.x as usize / 2, (area.right() as usize).div_ceil(2));
        let y0 = cy as usize * 2 * dst.strides[0];
        let ys = &dst.y[y0 + c0 * 2..y0 + c1 * 2];
        let us = &mut dst.u[cy as usize * dst.strides[1] + c0..cy as usize * dst.strides[1] + c1];
        let vs = &mut dst.v[cy as usize * dst.strides[2] + c0..cy as usize * dst.strides[2] + c1];
        let lumas = ys.chunks_exact(2).map(|l| l[0]);
        if even && step > 0 {
            // One source pixel per chroma sample: every other one at the
            // picture's own size, every one at half of it.
            let stride = 8 / step;
            let from = srow + cols[0];
            let pixels = src.data[from..from + us.len() * stride].chunks_exact(stride);
            us.iter_mut().zip(vs.iter_mut()).zip(pixels.zip(lumas)).for_each(|((u, v), (p, yd))| one(u, v, p, yd));
        } else {
            for (i, ((u, v), yd)) in us.iter_mut().zip(vs.iter_mut()).zip(lumas).enumerate() {
                let lx = ((c0 + i) * 2).max(area.x as usize) - area.x as usize;
                one(u, v, &src.data[srow + cols[lx.min(cols.len() - 1)]..][..4], yd);
            }
        }
    }
}
#[cfg(test)]
#[path = "modes_tests.rs"]
mod tests;
