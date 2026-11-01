//! A LUT, an amount and a colour correction, baked into one table that maps
//! the canvas's Y'CbCr straight to Y'CbCr.
//!
//! The table has 33 points a side, one every 8 code values from 0 to 256, so
//! the point below a byte is `byte >> 3` and how far past it is `byte & 7`.
//! At every point the colour correction runs first, the result is turned to
//! RGB (BT.709, limited range, what the canvas carries), the LUT runs on
//! that, the amount mixes it with what went in, and it is turned back. A
//! frame then never converts a colour or reads the LUT: it interpolates in
//! this table (`apply`).
//!
//! Values are kept as code value times 64, which is what lets `apply` do
//! the interpolation in whole numbers without losing a step.

use super::Lut;
use crate::plugin::filters::colour::tables::{chroma_value, luma_value};
use crate::plugin::filters::colour::Settings;

pub const POINTS: usize = 33;
pub const ONE: i32 = 64;

/// One table per channel, each `[(y * 33 + u) * 33 + v]` times [`ONE`]. Kept
/// apart so a luma lookup reads only the luma table.
pub struct Table {
    pub channels: [Vec<u16>; 3],
}

/// Everything that decides the table.
pub struct Recipe<'a> {
    pub lut: Option<&'a Lut>,
    /// 0 leaves the LUT out, 1 is all of it.
    pub amount: f32,
    pub colour: Settings,
}

impl Recipe<'_> {
    /// True when the table would change nothing, so none is made.
    pub fn is_identity(&self) -> bool {
        (self.lut.is_none() || self.amount <= 0.0) && self.colour.is_identity()
    }
}

pub fn build(r: &Recipe) -> Table {
    let n = POINTS * POINTS * POINTS;
    let mut channels = [Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n)];
    for yi in 0..POINTS {
        for ui in 0..POINTS {
            for vi in 0..POINTS {
                let p = point(r, (yi * 8) as f64, (ui * 8) as f64, (vi * 8) as f64);
                (0..3).for_each(|c| channels[c].push(p[c]));
            }
        }
    }
    Table { channels }
}

fn point(r: &Recipe, y: f64, u: f64, v: f64) -> [u16; 3] {
    let (y, (u, v)) = if r.colour.is_identity() { (y, (u, v)) } else { (luma_value(&r.colour, y), chroma_value(&r.colour, u, v)) };
    let (y, u, v) = match r.lut.filter(|_| r.amount > 0.0) {
        Some(lut) => {
            // Luma past black or white goes round the LUT and is added back
            // after, so a table point at 240 does not pull 235 down. Colour
            // outside RGB is held at the cube's edge, as a grading tool does.
            let legal = y.clamp(16.0, 235.0);
            let rgb = to_rgb(legal, u, v).map(|c| c.clamp(0.0, 1.0));
            let graded = lut.sample(rgb);
            let k = r.amount.min(1.0);
            let (gy, gu, gv) = to_yuv(std::array::from_fn(|i| rgb[i] + (graded[i] - rgb[i]) * k));
            (gy + (y - legal), gu, gv)
        }
        None => (y, u, v),
    };
    [y, u, v].map(|c| (c.clamp(0.0, 255.0) * ONE as f64).round() as u16)
}

/// Limited range BT.709 to RGB, 0 to 1 for a legal colour.
fn to_rgb(y: f64, u: f64, v: f64) -> [f32; 3] {
    let y = (y - 16.0) / 219.0;
    let (u, v) = ((u - 128.0) / 224.0, (v - 128.0) / 224.0);
    let r = y + 1.5748 * v;
    let b = y + 1.8556 * u;
    let g = (y - 0.2126 * r - 0.0722 * b) / 0.7152;
    [r as f32, g as f32, b as f32]
}

fn to_yuv(rgb: [f32; 3]) -> (f64, f64, f64) {
    let [r, g, b] = rgb.map(|c| c as f64);
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    (16.0 + 219.0 * y, 128.0 + 224.0 * (b - y) / 1.8556, 128.0 + 224.0 * (r - y) / 1.5748)
}
