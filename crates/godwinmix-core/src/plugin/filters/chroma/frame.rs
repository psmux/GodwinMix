//! One I420 frame keyed into an AYUV picture, in one pass over the area the
//! matte keeps.
//!
//! The key is decided per 2x2 block, because that is where I420 keeps its
//! colour: one table lookup per block gives its alpha and its despilled U and
//! V. The alpha is then spread back to every pixel with the usual 3:1
//! weights, so an edge is as fine as the luma under it rather than a staircase
//! of blocks. Nothing outside the matte is read or written, and a block that
//! is clear on all sides costs a comparison.

use super::lut::Lut;
use super::params::Matte;

/// One I420 frame to read from.
pub struct I420<'a> {
    pub y: &'a [u8],
    pub u: &'a [u8],
    pub v: &'a [u8],
    pub strides: [usize; 3],
    pub width: usize,
    pub height: usize,
}

/// The part of the frame the matte keeps, in pixels, on even coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: usize,
    pub y: usize,
    pub w: usize,
    pub h: usize,
}

impl Region {
    pub fn of(width: usize, height: usize, m: &Matte) -> Region {
        let even = |n: f32| (n.max(0.0) as usize) & !1;
        let x0 = even(m.left * width as f32);
        let y0 = even(m.top * height as f32);
        let x1 = (even((1.0 - m.right) * width as f32 + 1.0)).min(width & !1).max(x0 + 2);
        let y1 = (even((1.0 - m.bottom) * height as f32 + 1.0)).min(height & !1).max(y0 + 2);
        Region { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }
}

/// Buffers kept between frames, so a frame allocates only its output.
#[derive(Default)]
pub struct Scratch {
    alpha: Vec<u8>,
    chroma: Vec<[u8; 2]>,
    blur: Vec<u16>,
}

/// Key `src` inside `r` into `out`, `r.w * 4` bytes to a row, which must
/// start zeroed: a clear pixel is not written at all.
pub fn key(src: &I420<'_>, r: Region, lut: &Lut, feather: u32, scratch: &mut Scratch, out: &mut [u8]) {
    let (bw, bh) = (r.w / 2, r.h / 2);
    blocks(src, r, lut, scratch);
    if feather > 0 {
        super::feather::soften(&mut scratch.alpha, &mut scratch.blur, bw, bh, feather.div_ceil(2) as usize);
    }
    let alpha = &scratch.alpha;
    let chroma = &scratch.chroma;
    let mut column = vec![0u16; bw];
    for row in 0..r.h {
        let b0 = row / 2;
        let b1 = if row % 2 == 0 { b0.saturating_sub(1) } else { (b0 + 1).min(bh - 1) };
        let (near, far) = (&alpha[b0 * bw..b0 * bw + bw], &alpha[b1 * bw..b1 * bw + bw]);
        let mut any = false;
        for (c, (n, f)) in column.iter_mut().zip(near.iter().zip(far)) {
            *c = 3 * *n as u16 + *f as u16;
            any |= *c != 0;
        }
        if !any {
            continue;
        }
        let luma = &src.y[(r.y + row) * src.strides[0] + r.x..][..r.w];
        let line = &mut out[row * r.w * 4..(row + 1) * r.w * 4];
        let tint = &chroma[b0 * bw..b0 * bw + bw];
        for (x, px) in line.chunks_exact_mut(4).enumerate() {
            let c0 = x / 2;
            let c1 = if x % 2 == 0 { c0.saturating_sub(1) } else { (c0 + 1).min(bw - 1) };
            let a = (3 * column[c0] as u32 + column[c1] as u32 + 8) >> 4;
            if a == 0 {
                continue;
            }
            let [u, v] = tint[c0];
            px.copy_from_slice(&[a.min(255) as u8, luma[x], u, v]);
        }
    }
}

/// The table looked up once per block of the region.
fn blocks(src: &I420<'_>, r: Region, lut: &Lut, scratch: &mut Scratch) {
    let (bw, bh) = (r.w / 2, r.h / 2);
    scratch.alpha.resize(bw * bh, 0);
    scratch.chroma.resize(bw * bh, [128, 128]);
    for by in 0..bh {
        let row_u = &src.u[(r.y / 2 + by) * src.strides[1] + r.x / 2..][..bw];
        let row_v = &src.v[(r.y / 2 + by) * src.strides[2] + r.x / 2..][..bw];
        let alpha = &mut scratch.alpha[by * bw..(by + 1) * bw];
        let chroma = &mut scratch.chroma[by * bw..(by + 1) * bw];
        for i in 0..bw {
            let [a, u, v, _] = lut.get(row_u[i], row_v[i]);
            alpha[i] = a;
            chroma[i] = [u, v];
        }
    }
}

/// The same key written back into the frame itself over black, for a place
/// with nothing to draw it over: a source's input side, or a programme
/// composited on a GPU. Cheaper than `key`, and only as fine as the blocks.
pub fn flatten(y: &mut [u8], u: &mut [u8], v: &mut [u8], strides: [usize; 3], size: (usize, usize), lut: &Lut, m: &Matte) {
    let r = Region::of(size.0, size.1, m);
    for by in 0..size.1 / 2 {
        for bx in 0..size.0 / 2 {
            let (px, py) = (bx * 2, by * 2);
            let inside = px >= r.x && px < r.x + r.w && py >= r.y && py < r.y + r.h;
            let (iu, iv) = (by * strides[1] + bx, by * strides[2] + bx);
            let [a, nu, nv, _] = if inside { lut.get(u[iu], v[iv]) } else { [0, 128, 128, 0] };
            if a == 255 && nu == u[iu] && nv == v[iv] {
                continue;
            }
            let scale = |c: u8, mid: i32| (mid + ((c as i32 - mid) * a as i32 + 127) / 255) as u8;
            u[iu] = scale(nu, 128);
            v[iv] = scale(nv, 128);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let i = (py + dy) * strides[0] + px + dx;
                y[i] = scale(y[i], 16);
            }
        }
    }
}
