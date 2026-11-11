//! A small colour histogram per track, so a person who walks behind the
//! pulpit and comes out again gets their id back.

use crate::frame::FrameRef;
use crate::geom::Rect;

const BINS: usize = 4;
const LEN: usize = BINS * BINS * BINS;
const GRID_X: u32 = 12;
const GRID_Y: u32 = 16;

/// Red, green and blue, four levels each, normalised to sum to one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Histogram([f32; LEN]);

impl Default for Histogram {
    fn default() -> Self {
        Self([0.0; LEN])
    }
}

impl Histogram {
    /// Samples the torso of `rect`: the middle half across and from 15% to
    /// 70% down, which is clothing far more often than background or skin.
    /// A fixed grid of points, so the cost does not grow with the box.
    pub fn sample(frame: &FrameRef, rect: &Rect) -> Self {
        let mut h = Self::default();
        let (fw, fh) = (frame.width as f32, frame.height as f32);
        let (x0, x1) = (rect.x + rect.w * 0.25, rect.x + rect.w * 0.75);
        let (y0, y1) = (rect.y + rect.h * 0.15, rect.y + rect.h * 0.70);
        let mut n = 0.0;
        for gy in 0..GRID_Y {
            let y = y0 + (y1 - y0) * (gy as f32 + 0.5) / GRID_Y as f32;
            if !(0.0..1.0).contains(&y) {
                continue;
            }
            for gx in 0..GRID_X {
                let x = x0 + (x1 - x0) * (gx as f32 + 0.5) / GRID_X as f32;
                if !(0.0..1.0).contains(&x) {
                    continue;
                }
                let [r, g, b] = frame.rgb_at((x * fw) as u32, (y * fh) as u32);
                let bin = (r as usize >> 6) * BINS * BINS + (g as usize >> 6) * BINS + (b as usize >> 6);
                h.0[bin] += 1.0;
                n += 1.0;
            }
        }
        if n > 0.0 {
            h.0.iter_mut().for_each(|v| *v /= n);
        }
        h
    }

    /// True when nothing was sampled.
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|v| *v == 0.0)
    }

    /// The Bhattacharyya coefficient: 1 for the same colours, 0 for none in
    /// common.
    pub fn similarity(&self, other: &Histogram) -> f32 {
        self.0.iter().zip(other.0.iter()).map(|(a, b)| (a * b).sqrt()).sum()
    }

    /// Moves this histogram towards `other` by `rate`, 0 to 1.
    pub fn blend(&mut self, other: &Histogram, rate: f32) {
        if self.is_empty() {
            *self = *other;
            return;
        }
        for (a, b) in self.0.iter_mut().zip(other.0.iter()) {
            *a += (b - *a) * rate;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::PixelFormat;

    #[test]
    fn same_shirt_matches_and_a_different_one_does_not() {
        // Left half red, right half blue.
        let (w, h) = (40u32, 20u32);
        let mut px = vec![0u8; (w * h * 3) as usize];
        for (i, p) in px.chunks_mut(3).enumerate() {
            let x = i as u32 % w;
            p.copy_from_slice(if x < w / 2 { &[220, 30, 30] } else { &[30, 30, 220] });
        }
        let f = FrameRef::packed(w, h, PixelFormat::Rgb8, &px).unwrap();
        let red = Histogram::sample(&f, &Rect::new(0.0, 0.0, 0.5, 1.0));
        let red2 = Histogram::sample(&f, &Rect::new(0.05, 0.0, 0.4, 1.0));
        let blue = Histogram::sample(&f, &Rect::new(0.5, 0.0, 0.5, 1.0));
        assert!(red.similarity(&red2) > 0.99);
        assert!(red.similarity(&blue) < 0.01);
    }
}
