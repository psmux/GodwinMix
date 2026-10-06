//! The arithmetic of each mode, on limited range luma and centred chroma.

use super::Blend;

/// 0 to 219 luma as 0 to 256, without a division.
#[inline(always)]
fn unit(v: i32) -> i32 {
    (v * 299 + 255) >> 8
}

pub struct Normal;
pub struct Screen;
pub struct Add;
pub struct Luma;

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
