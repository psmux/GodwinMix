//! Running a baked table over an I420 frame, in place.
//!
//! Each luma sample is looked up with its own Y and its block's U and V, and
//! only the Y of the answer is kept. Each 2x2 block's chroma is looked up
//! once, at the block's mean Y, and only U and V are kept. That is seven
//! channel lookups a block where a full conversion would need twelve, and
//! the chroma is what an RGB pass followed by 4:2:0 would have averaged it
//! to anyway.
//!
//! The lookup is tetrahedral: four of the eight corners around a point,
//! picked by which of its three fractions is largest, all in whole numbers.

use super::bake::{Table, POINTS};

const SY: usize = POINTS * POINTS;
const SU: usize = POINTS;

/// One I420 frame's planes and their geometry.
pub struct Planes<'a> {
    pub y: &'a mut [u8],
    pub u: &'a mut [u8],
    pub v: &'a mut [u8],
    pub y_stride: usize,
    pub u_stride: usize,
    pub v_stride: usize,
    pub width: usize,
    pub height: usize,
}

pub fn frame(t: &Table, p: Planes) {
    let [ty, tu, tv] = &t.channels;
    let (cw, ch) = (p.width.div_ceil(2), p.height.div_ceil(2));
    for by in 0..ch {
        let top = by * 2 * p.y_stride;
        // The second row of the block, or the first again on an odd height.
        let second = if by * 2 + 1 < p.height { top + p.y_stride } else { top };
        let (urow, vrow) = (by * p.u_stride, by * p.v_stride);
        for bx in 0..cw {
            let (u, v) = (p.u[urow + bx], p.v[vrow + bx]);
            let x0 = bx * 2;
            let x1 = if x0 + 1 < p.width { x0 + 1 } else { x0 };
            let (a, b, c, d) = (p.y[top + x0], p.y[top + x1], p.y[second + x0], p.y[second + x1]);
            let mean = ((a as u32 + b as u32 + c as u32 + d as u32 + 2) >> 2) as u8;
            let k = Corner::of(u, v);
            p.y[top + x0] = out(k.lookup(ty, a));
            p.y[top + x1] = out(k.lookup(ty, b));
            p.y[second + x0] = out(k.lookup(ty, c));
            p.y[second + x1] = out(k.lookup(ty, d));
            p.u[urow + bx] = out(k.lookup(tu, mean));
            p.v[vrow + bx] = out(k.lookup(tv, mean));
        }
    }
}

/// `frame`, with a large picture cut into bands of block rows, one a core up
/// to four, so a 1080p look costs the frame's thread a quarter of the time.
/// A small picture is not worth the threads and runs on this one.
pub fn frame_parallel(t: &Table, p: Planes) {
    let parts = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(4);
    if parts < 2 || p.width * p.height < 1280 * 720 {
        return frame(t, p);
    }
    let rows = p.height.div_ceil(2);
    let per = rows.div_ceil(parts);
    let (mut y, mut u, mut v) = (p.y, p.u, p.v);
    let mut bands = Vec::with_capacity(parts);
    let mut row = 0;
    while row < rows {
        let take = per.min(rows - row);
        let last = row + take == rows;
        let height = if last { p.height - row * 2 } else { take * 2 };
        let cut = |len: usize, stride: usize, n: usize| if last { len } else { (stride * n).min(len) };
        let (ny, nu, nv) = (cut(y.len(), p.y_stride, take * 2), cut(u.len(), p.u_stride, take), cut(v.len(), p.v_stride, take));
        let (by, ry) = std::mem::take(&mut y).split_at_mut(ny);
        let (bu, ru) = std::mem::take(&mut u).split_at_mut(nu);
        let (bv, rv) = std::mem::take(&mut v).split_at_mut(nv);
        (y, u, v) = (ry, ru, rv);
        bands.push(Planes { y: by, u: bu, v: bv, y_stride: p.y_stride, u_stride: p.u_stride, v_stride: p.v_stride, width: p.width, height });
        row += take;
    }
    std::thread::scope(|s| {
        let mut bands = bands.into_iter();
        let mine = bands.next();
        for band in bands {
            s.spawn(move || frame(t, band));
        }
        if let Some(band) = mine {
            frame(t, band);
        }
    });
}

/// Times 64 and times 8 for the fractions: divide by 512, rounding.
#[inline]
fn out(v: i32) -> u8 {
    ((v + 256) >> 9).clamp(0, 255) as u8
}

/// The U and V half of a lookup, the same for all four pixels of a block.
struct Corner {
    base: usize,
    fy: i32,
    fz: i32,
}

impl Corner {
    #[inline]
    fn of(u: u8, v: u8) -> Corner {
        Corner { base: (u as usize >> 3) * SU + (v as usize >> 3), fy: (u & 7) as i32, fz: (v & 7) as i32 }
    }

    /// One channel's table at `y` and this block's U and V, times 512.
    #[inline]
    fn lookup(&self, t: &[u16], y: u8) -> i32 {
        let base = (y as usize >> 3) * SY + self.base;
        let (fx, fy, fz) = ((y & 7) as i32, self.fy, self.fz);
        // Every corner is read through one slice, so the compiler checks
        // its bounds once.
        let cube = &t[base..base + SY + SU + 2];
        let at = |o: usize| cube[o] as i32;
        let (c000, c111) = (at(0), at(SY + SU + 1));
        if fx >= fy {
            if fy >= fz {
                (8 - fx) * c000 + (fx - fy) * at(SY) + (fy - fz) * at(SY + SU) + fz * c111
            } else if fx >= fz {
                (8 - fx) * c000 + (fx - fz) * at(SY) + (fz - fy) * at(SY + 1) + fy * c111
            } else {
                (8 - fz) * c000 + (fz - fx) * at(1) + (fx - fy) * at(SY + 1) + fy * c111
            }
        } else if fz >= fy {
            (8 - fz) * c000 + (fz - fy) * at(1) + (fy - fx) * at(SU + 1) + fx * c111
        } else if fz >= fx {
            (8 - fy) * c000 + (fy - fz) * at(SU) + (fz - fx) * at(SU + 1) + fx * c111
        } else {
            (8 - fy) * c000 + (fy - fx) * at(SU) + (fx - fz) * at(SY + SU) + fz * c111
        }
    }
}

/// Channel `c` at `(y, u, v)`, times 512.
pub fn lookup(t: &Table, y: u8, u: u8, v: u8, c: usize) -> i32 {
    Corner::of(u, v).lookup(&t.channels[c], y)
}

#[cfg(test)]
mod tests {
    use super::super::bake::{build, Recipe};
    use super::super::{looks, Lut};
    use super::*;
    use crate::plugin::filters::colour::Settings;

    fn table(lut: &Lut, amount: f32) -> Table {
        build(&Recipe { lut: Some(lut), amount, colour: Settings::default() })
    }

    fn run(t: &Table, w: usize, h: usize, yuv: (u8, u8, u8)) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let (mut y, mut u, mut v) = (vec![yuv.0; w * h], vec![yuv.1; w.div_ceil(2) * h.div_ceil(2)], vec![yuv.2; w.div_ceil(2) * h.div_ceil(2)]);
        let (cw, _) = (w.div_ceil(2), 0);
        frame(t, Planes { y: &mut y, u: &mut u, v: &mut v, y_stride: w, u_stride: cw, v_stride: cw, width: w, height: h });
        (y, u, v)
    }

    /// Colours a camera sees, inside RGB: black, white, grey, skin, grass, sky.
    fn real_colours() -> Vec<(u8, u8, u8)> {
        [[0, 0, 0], [255, 255, 255], [128, 128, 128], [230, 180, 150], [48, 176, 80], [20, 60, 200], [200, 40, 30]]
            .into_iter()
            .map(crate::plugin::filters::chroma::rgb_to_yuv)
            .collect()
    }

    #[test]
    fn an_identity_lut_leaves_legal_colours_within_a_step() {
        let t = table(&Lut::from_fn("id", 17, |c| c), 1.0);
        for yuv in real_colours() {
            let (y, u, v) = run(&t, 4, 4, yuv);
            for (got, want) in [(y[0], yuv.0), (u[0], yuv.1), (v[0], yuv.2)] {
                assert!((got as i32 - want as i32).abs() <= 1, "{yuv:?} came back {:?}", (y[0], u[0], v[0]));
            }
        }
    }

    #[test]
    fn black_and_white_takes_the_colour_out() {
        let t = table(&looks::find("black-and-white").unwrap().lut(), 1.0);
        let (_, u, v) = run(&t, 2, 2, real_colours()[6]);
        assert!((u[0] as i32 - 128).abs() <= 1 && (v[0] as i32 - 128).abs() <= 1, "{} {}", u[0], v[0]);
    }

    #[test]
    fn half_the_amount_lands_half_way() {
        let lut = looks::find("black-and-white").unwrap().lut();
        let skin = real_colours()[3];
        let (_, u, _) = run(&table(&lut, 0.5), 2, 2, skin);
        let want = (skin.1 as i32 + 128) / 2;
        assert!((u[0] as i32 - want).abs() <= 1, "{} wanted {want}", u[0]);
    }

    #[test]
    fn bands_on_several_cores_match_one_core() {
        let t = table(&looks::find("teal-orange").unwrap().lut(), 1.0);
        let (w, h): (usize, usize) = (1281, 723);
        let cw = w.div_ceil(2);
        let ch = h.div_ceil(2);
        let make = || -> (Vec<u8>, Vec<u8>, Vec<u8>) {
            ((0..w * h).map(|i| (i * 7 % 220 + 16) as u8).collect(), (0..cw * ch).map(|i| (i * 13 % 224 + 16) as u8).collect(), (0..cw * ch).map(|i| (i * 5 % 224 + 16) as u8).collect())
        };
        let (mut y1, mut u1, mut v1) = make();
        let (mut y2, mut u2, mut v2) = make();
        frame(&t, Planes { y: &mut y1, u: &mut u1, v: &mut v1, y_stride: w, u_stride: cw, v_stride: cw, width: w, height: h });
        frame_parallel(&t, Planes { y: &mut y2, u: &mut u2, v: &mut v2, y_stride: w, u_stride: cw, v_stride: cw, width: w, height: h });
        assert!(y1 == y2 && u1 == u2 && v1 == v2);
    }

    #[test]
    fn an_odd_sized_frame_reaches_its_last_column_and_row() {
        let t = table(&looks::find("black-and-white").unwrap().lut(), 1.0);
        let (y, u, _) = run(&t, 5, 3, real_colours()[5]);
        assert_eq!(y.len(), 15);
        assert!(u.iter().all(|&c| (c as i32 - 128).abs() <= 1), "{u:?}");
    }

    /// `cargo test -p godwinmix-core --release lut::apply -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn cost_of_a_1080p_frame() {
        let t = table(&looks::find("teal-orange").unwrap().lut(), 1.0);
        let (w, h) = (1920, 1080);
        let mut y: Vec<u8> = (0..w * h).map(|i| (i * 7 % 220 + 16) as u8).collect();
        let mut u: Vec<u8> = (0..w * h / 4).map(|i| (i * 13 % 224 + 16) as u8).collect();
        let mut v: Vec<u8> = (0..w * h / 4).map(|i| (i * 5 % 224 + 16) as u8).collect();
        let start = std::time::Instant::now();
        for _ in 0..30 {
            frame_parallel(&t, Planes { y: &mut y, u: &mut u, v: &mut v, y_stride: w, u_stride: w / 2, v_stride: w / 2, width: w, height: h });
        }
        println!("{:.2} ms a 1080p frame", start.elapsed().as_secs_f64() * 1000.0 / 30.0);
    }
}
