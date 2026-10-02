//! One I420 frame keyed: a decision per block of the area the matte keeps.
//!
//! The key is decided per 2x2 block, because that is where I420 keeps its
//! colour: one table lookup per block gives its alpha and its despilled U and
//! V. The board spreads the alpha back to every pixel as it draws (see
//! `overlay::keyed`), so an edge is as fine as the luma under it rather than
//! a staircase of blocks. Nothing outside the matte is read.

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

/// The key's decision for every 2x2 block of the region: alpha, and U and V
/// with the spill taken out. One table lookup a block, then the feather.
pub fn blocks(src: &I420<'_>, r: Region, lut: &Lut, feather: u32, alpha: &mut Vec<u8>, chroma: &mut Vec<[u8; 2]>) {
    let (bw, bh) = (r.w / 2, r.h / 2);
    alpha.resize(bw * bh, 0);
    chroma.resize(bw * bh, [128, 128]);
    for by in 0..bh {
        let row_u = &src.u[(r.y / 2 + by) * src.strides[1] + r.x / 2..][..bw];
        let row_v = &src.v[(r.y / 2 + by) * src.strides[2] + r.x / 2..][..bw];
        let a_row = &mut alpha[by * bw..(by + 1) * bw];
        let c_row = &mut chroma[by * bw..(by + 1) * bw];
        for i in 0..bw {
            let [a, u, v, _] = lut.get(row_u[i], row_v[i]);
            a_row[i] = a;
            c_row[i] = [u, v];
        }
    }
    if feather > 0 {
        super::feather::soften(alpha, bw, bh, feather.div_ceil(2) as usize);
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
