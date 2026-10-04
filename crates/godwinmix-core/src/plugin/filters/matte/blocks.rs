//! The newest mask laid over a frame, as the board draws a key: an alpha and
//! a U and V per 2x2 block of the area the matte keeps.
//!
//! The same shape the chroma key hands over (`overlay::keyed::Keyed`), so the
//! board draws a cutout exactly as it draws a key, in stacking order, with a
//! desk or a lower third in front of it. The mask is a model's size, a few
//! hundred pixels across; each block reads it bilinearly, in fixed point, from
//! row and column tables built once a frame.

use crate::plugin::filters::chroma::frame::Region;

/// Where in the mask each block of one axis reads: the two neighbours and how
/// far between them, out of 256.
fn axis(blocks: usize, offset: usize, frame: usize, mask: usize) -> Vec<(usize, usize, u32)> {
    (0..blocks)
        .map(|b| {
            // The block's centre, in mask pixels, less half a pixel.
            let at = ((offset + 2 * b + 1) as f32 * mask as f32 / frame as f32 - 0.5).max(0.0);
            let lo = (at as usize).min(mask - 1);
            let hi = (lo + 1).min(mask - 1);
            (lo, hi, ((at - lo as f32) * 256.0) as u32)
        })
        .collect()
}

/// Alpha per block of `r`, from `mask` (its size `mw` x `mh`) through `curve`.
pub fn alpha(mask: &[u8], (mw, mh): (usize, usize), frame: (usize, usize), r: Region, curve: &[u8; 256], out: &mut Vec<u8>) {
    let (bw, bh) = (r.w / 2, r.h / 2);
    out.resize(bw * bh, 255);
    if mw == 0 || mh == 0 || mask.len() < mw * mh {
        out.fill(255);
        return;
    }
    let cols = axis(bw, r.x, frame.0, mw);
    let rows = axis(bh, r.y, frame.1, mh);
    for (by, &(y0, y1, fy)) in rows.iter().enumerate() {
        let (top, bottom) = (&mask[y0 * mw..][..mw], &mask[y1 * mw..][..mw]);
        let row = &mut out[by * bw..][..bw];
        for (o, &(x0, x1, fx)) in row.iter_mut().zip(&cols) {
            let t = top[x0] as u32 * (256 - fx) + top[x1] as u32 * fx;
            let b = bottom[x0] as u32 * (256 - fx) + bottom[x1] as u32 * fx;
            let v = (t * (256 - fy) + b * fy + (1 << 15)) >> 16;
            *o = curve[v.min(255) as usize];
        }
    }
}

/// The frame's own U and V per block of `r`: a cutout has no spill to take
/// out, unlike a key.
pub fn chroma(u: &[u8], v: &[u8], strides: [usize; 3], r: Region, out: &mut Vec<[u8; 2]>) {
    let (bw, bh) = (r.w / 2, r.h / 2);
    out.resize(bw * bh, [128, 128]);
    for by in 0..bh {
        let ru = &u[(r.y / 2 + by) * strides[1] + r.x / 2..][..bw];
        let rv = &v[(r.y / 2 + by) * strides[2] + r.x / 2..][..bw];
        for (o, (&cu, &cv)) in out[by * bw..][..bw].iter_mut().zip(ru.iter().zip(rv)) {
            *o = [cu, cv];
        }
    }
}

/// The cutout written into the frame itself over black, for a place with no
/// board to draw it on: a source's input side, or a programme composited on
/// a GPU. As fine as the blocks.
pub fn flatten(y: &mut [u8], u: &mut [u8], v: &mut [u8], strides: [usize; 3], size: (usize, usize), alpha: &[u8], r: Region) {
    let bw = r.w / 2;
    for by in 0..size.1 / 2 {
        for bx in 0..size.0 / 2 {
            let (px, py) = (bx * 2, by * 2);
            let inside = px >= r.x && px < r.x + r.w && py >= r.y && py < r.y + r.h;
            let a = if inside { alpha[((py - r.y) / 2) * bw + (px - r.x) / 2] as i32 } else { 0 };
            if a == 255 {
                continue;
            }
            let scale = |c: u8, mid: i32| (mid + ((c as i32 - mid) * a + 127) / 255) as u8;
            let (iu, iv) = (by * strides[1] + bx, by * strides[2] + bx);
            u[iu] = scale(u[iu], 128);
            v[iv] = scale(v[iv], 128);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let i = (py + dy) * strides[0] + px + dx;
                y[i] = scale(y[i], 16);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> [u8; 256] {
        std::array::from_fn(|i| i as u8)
    }

    #[test]
    fn a_mask_half_on_lands_on_the_matching_half_of_the_frame() {
        // A 4 x 2 mask, left half 0, right half 255, over an 8 x 4 frame.
        let mask = [0, 0, 255, 255, 0, 0, 255, 255];
        let r = Region { x: 0, y: 0, w: 8, h: 4 };
        let mut out = Vec::new();
        alpha(&mask, (4, 2), (8, 4), r, &identity(), &mut out);
        assert_eq!(out.len(), 8);
        assert!(out[0] < 10 && out[3] > 245, "{out:?}");
        assert_eq!(out[0..4], out[4..8], "both rows the same");
    }

    /// What a 1080p frame costs on its own thread: the mask laid over it and
    /// the frame's chroma copied. `cargo test --release -- --ignored cost`.
    #[test]
    #[ignore]
    fn cost_per_1080p_frame() {
        let (w, h) = (1920usize, 1080usize);
        let mask: Vec<u8> = (0..512 * 288).map(|i| (i % 256) as u8).collect();
        let (u, v) = (vec![100u8; (w / 2) * (h / 2)], vec![150u8; (w / 2) * (h / 2)]);
        let r = Region { x: 0, y: 0, w, h };
        let (mut a, mut c) = (Vec::new(), Vec::new());
        let curve = identity();
        let n = 200;
        let t = std::time::Instant::now();
        for _ in 0..n {
            alpha(&mask, (512, 288), (w, h), r, &curve, &mut a);
            chroma(&u, &v, [w, w / 2, w / 2], r, &mut c);
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
        eprintln!("cutout blocks at 1080p: {ms:.2} ms a frame");
        assert!(ms < 8.0, "{ms} ms");
    }

    #[test]
    fn no_mask_yet_shows_the_whole_picture() {
        let r = Region { x: 0, y: 0, w: 4, h: 4 };
        let mut out = Vec::new();
        alpha(&[], (0, 0), (4, 4), r, &identity(), &mut out);
        assert_eq!(out, [255; 4]);
    }

    #[test]
    fn flattening_takes_what_is_cut_out_to_black() {
        let (mut y, mut u, mut v) = (vec![200u8; 16], vec![60u8; 4], vec![200u8; 4]);
        let r = Region { x: 0, y: 0, w: 4, h: 4 };
        flatten(&mut y, &mut u, &mut v, [4, 2, 2], (4, 4), &[0, 255, 255, 255], r);
        assert_eq!((y[0], u[0], v[0]), (16, 128, 128));
        assert_eq!((y[2], u[1]), (200, 60));
    }
}
