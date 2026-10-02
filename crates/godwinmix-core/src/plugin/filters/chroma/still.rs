//! The key colour read off a still picture: a source's tile on the mosaic,
//! which is what `source.key_color` has to look at.

use super::colour::rgb_to_yuv;
use super::guess::{self, Guess};
use super::params::Family;
pub use image::RgbImage;

/// The screen in a still: the biggest saturated green or blue area.
pub fn screen(still: &RgbImage, family: Family) -> Option<Guess> {
    guess::dominant(still.pixels().step_by(3).map(|p| rgb_to_yuv(p.0)), family)
}

/// The mean colour of the five by five square around a point, given as 0
/// to 1 across and down.
pub fn at(still: &RgbImage, x: f64, y: f64) -> [u8; 3] {
    let (w, h) = (still.width() as i64, still.height() as i64);
    let (cx, cy) = ((x * (w - 1) as f64) as i64, (y * (h - 1) as f64) as i64);
    let (mut sum, mut n) = ([0u32; 3], 0u32);
    for py in (cy - 2).max(0)..=(cy + 2).min(h - 1) {
        for px in (cx - 2).max(0)..=(cx + 2).min(w - 1) {
            let p = still.get_pixel(px as u32, py as u32).0;
            (0..3).for_each(|i| sum[i] += p[i] as u32);
            n += 1;
        }
    }
    sum.map(|s| (s / n.max(1)) as u8)
}

/// True for a still that is all but black: a tile the mosaic has not drawn
/// yet, or a camera with its cap on.
pub fn is_dark(still: &RgbImage) -> bool {
    let (sum, n) = still.pixels().step_by(7).fold((0u64, 0u64), |(s, n), p| (s + p.0.iter().map(|c| *c as u64).sum::<u64>(), n + 3));
    n == 0 || sum / n < 24
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_and_the_screen_are_read_off_a_still() {
        let mut still = RgbImage::from_pixel(40, 20, image::Rgb([40, 180, 70]));
        for x in 30..40 {
            for y in 0..20 {
                still.put_pixel(x, y, image::Rgb([200, 160, 140]));
            }
        }
        assert_eq!(at(&still, 0.95, 0.5), [200, 160, 140]);
        let g = screen(&still, Family::Any).expect("the screen");
        assert_eq!(g.family, Family::Green);
        assert!((g.share - 0.75).abs() < 0.05, "{}", g.share);
    }
}
