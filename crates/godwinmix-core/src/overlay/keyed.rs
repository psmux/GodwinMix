//! A keyed camera frame drawn straight onto the programme.
//!
//! A key decides its matte per 2x2 block, where I420 keeps its colour. What
//! it hands the board is that decision and the camera's own frame, untouched:
//! a byte of alpha and a despilled U and V per block. The board reads the
//! luma from the camera frame and the rest from the blocks as it draws, so
//! no picture with alpha is ever written out and read back. That costs about
//! what the compositor pays to draw an opaque pad, which is what the key
//! replaced.
//!
//! The frame is held by reference until the next one replaces it: the layer
//! keeps one and the board may be drawing from another, which is one more
//! than the compositor held of this source before the key.

use super::blend::{Draw, Planes, Rect};
use gstreamer as gst;
use gstreamer_video as gst_video;

/// One keyed frame.
pub struct Keyed {
    pub frame: gst::Buffer,
    pub info: gst_video::VideoInfo,
    /// The part of the frame the matte keeps: x, y, width, height in pixels,
    /// all even. The blocks below cover exactly this.
    pub region: (usize, usize, usize, usize),
    /// Alpha per block, `region.2 / 2` to a row.
    pub alpha: Vec<u8>,
    /// U and V per block, with the spill taken out.
    pub chroma: Vec<[u8; 2]>,
}

/// Draw `k` as the draw says: its `window` is in the region's own pixels.
pub fn draw(dst: &mut Planes<'_>, k: &Keyed, d: &Draw) {
    let frame = Rect::new(0, 0, dst.width, dst.height);
    let Some(area) = d.to.within(&d.clip).and_then(|a| a.within(&frame)) else { return };
    let (rw, rh) = (k.region.2, k.region.3);
    if d.alpha == 0 || rw < 2 || rh < 2 || k.alpha.len() < (rw / 2) * (rh / 2) {
        return;
    }
    let Ok(f) = gst_video::VideoFrameRef::from_buffer_ref_readable(k.frame.as_ref(), &k.info) else { return };
    let Ok(luma_plane) = f.plane_data(0) else { return };
    let span = |at: i32, from: i32, size: i32, to_at: i32, to_size: i32, top: usize| {
        ((from as i64 + (at - to_at) as i64 * size as i64 / to_size.max(1) as i64).max(0) as usize).min(top - 1)
    };
    let cols: Vec<usize> = (area.x..area.right()).map(|x| span(x, d.window.x, d.window.w, d.to.x, d.to.w, rw)).collect();
    let rows: Vec<usize> = (area.y..area.bottom()).map(|y| span(y, d.window.y, d.window.h, d.to.y, d.to.h, rh)).collect();
    let src = Src { luma: luma_plane, stride: k.info.stride()[0] as usize, k, ga: d.alpha as u32 };
    luma(dst, &src, &area, &cols, &rows);
    chroma(dst, &src, &area, &cols, &rows);
}

struct Src<'a> {
    luma: &'a [u8],
    stride: usize,
    k: &'a Keyed,
    ga: u32,
}

fn mix(over: u32, under: u8, a: u32) -> u8 {
    ((over * a + under as u32 * (255 - a) + 127) / 255) as u8
}

fn luma(dst: &mut Planes<'_>, s: &Src<'_>, area: &Rect, cols: &[usize], rows: &[usize]) {
    let (rx, ry, rw, rh) = s.k.region;
    let (bw, bh) = (rw / 2, rh / 2);
    let mut column = vec![0u16; bw];
    let mut alpha = vec![0u8; rw];
    for (j, &py) in rows.iter().enumerate() {
        if !spread(s.k, py, bw, bh, &mut column, &mut alpha) {
            continue;
        }
        let src = &s.luma[(ry + py) * s.stride + rx..][..rw];
        let start = (area.y as usize + j) * dst.strides[0] + area.x as usize;
        let line = &mut dst.y[start..start + area.w as usize];
        for (d, &px) in line.iter_mut().zip(cols) {
            let a = if s.ga == 255 { alpha[px] as u32 } else { alpha[px] as u32 * s.ga / 255 };
            match a {
                0 => {}
                255 => *d = src[px],
                _ => *d = mix(src[px] as u32, *d, a),
            }
        }
    }
}

/// The block alpha spread to every pixel of one row of the region, 3 to 1
/// with the nearest block row and then with the nearest block across, the
/// way a bilinear upscale of the chroma grid would. False for a row that is
/// clear from end to end, which is most of the rows above a presenter.
fn spread(k: &Keyed, py: usize, bw: usize, bh: usize, column: &mut [u16], alpha: &mut [u8]) -> bool {
    let b0 = py / 2;
    let b1 = if py % 2 == 0 { b0.saturating_sub(1) } else { (b0 + 1).min(bh - 1) };
    let (near, far) = (&k.alpha[b0 * bw..][..bw], &k.alpha[b1 * bw..][..bw]);
    let mut any = 0u16;
    for (c, (n, f)) in column.iter_mut().zip(near.iter().zip(far)) {
        *c = 3 * *n as u16 + *f as u16;
        any |= *c;
    }
    if any == 0 {
        return false;
    }
    for bx in 0..bw {
        let (c, l, r) = (column[bx] as u32, column[bx.saturating_sub(1)] as u32, column[(bx + 1).min(bw - 1)] as u32);
        alpha[2 * bx] = ((3 * c + l + 8) >> 4).min(255) as u8;
        alpha[2 * bx + 1] = ((3 * c + r + 8) >> 4).min(255) as u8;
    }
    true
}

fn chroma(dst: &mut Planes<'_>, s: &Src<'_>, area: &Rect, cols: &[usize], rows: &[usize]) {
    let bw = s.k.region.2 / 2;
    for cy in area.y / 2..(area.bottom() + 1) / 2 {
        let ly = (cy * 2).max(area.y);
        let by = rows[(ly - area.y) as usize] / 2;
        let alpha = &s.k.alpha[by * bw..][..bw];
        let tint = &s.k.chroma[by * bw..][..bw];
        for cx in area.x / 2..(area.right() + 1) / 2 {
            let lx = (cx * 2).max(area.x);
            let bx = cols[(lx - area.x) as usize] / 2;
            let mut a = alpha[bx] as u32;
            if s.ga != 255 {
                a = a * s.ga / 255;
            }
            if a == 0 {
                continue;
            }
            let [u, v] = tint[bx];
            let iu = cy as usize * dst.strides[1] + cx as usize;
            let iv = cy as usize * dst.strides[2] + cx as usize;
            dst.u[iu] = if a == 255 { u } else { mix(u as u32, dst.u[iu], a) };
            dst.v[iv] = if a == 255 { v } else { mix(v as u32, dst.v[iv], a) };
        }
    }
}
