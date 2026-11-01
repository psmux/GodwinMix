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
    let (cw, ch) = (p.width.div_ceil(2), p.height.div_ceil(2));
    for by in 0..ch {
        for bx in 0..cw {
            let (ci, cv) = (by * p.u_stride + bx, by * p.v_stride + bx);
            let (u, v) = (p.u[ci], p.v[cv]);
            let mut sum = 0u32;
            let mut n = 0u32;
            for dy in 0..2 {
                let row = by * 2 + dy;
                if row >= p.height {
                    continue;
                }
                for dx in 0..2 {
                    let col = bx * 2 + dx;
                    if col >= p.width {
                        continue;
                    }
                    let i = row * p.y_stride + col;
                    let y = p.y[i];
                    sum += y as u32;
                    n += 1;
                    p.y[i] = out(lookup(t, y, u, v, 0));
                }
            }
            let mean = (sum / n.max(1)) as u8;
            p.u[ci] = out(lookup(t, mean, u, v, 1));
            p.v[cv] = out(lookup(t, mean, u, v, 2));
        }
    }
}

/// Times 64 and times 8 for the fractions: divide by 512, rounding.
fn out(v: i32) -> u8 {
    ((v + 256) >> 9).clamp(0, 255) as u8
}

/// Channel `c` at `(y, u, v)`, times 512.
#[inline]
pub fn lookup(t: &Table, y: u8, u: u8, v: u8, c: usize) -> i32 {
    let base = (y as usize >> 3) * SY + (u as usize >> 3) * SU + (v as usize >> 3);
    let (fx, fy, fz) = ((y & 7) as i32, (u & 7) as i32, (v & 7) as i32);
    let at = |o: usize| t.values[base + o][c] as i32;
    let c000 = at(0);
    let c111 = at(SY + SU + 1);
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
            frame(&t, Planes { y: &mut y, u: &mut u, v: &mut v, y_stride: w, u_stride: w / 2, v_stride: w / 2, width: w, height: h });
        }
        println!("{:.2} ms a 1080p frame", start.elapsed().as_secs_f64() * 1000.0 / 30.0);
    }
}
