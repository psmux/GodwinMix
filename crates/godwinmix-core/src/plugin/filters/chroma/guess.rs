//! A first guess at the key colour: the screen is the biggest saturated
//! area of one hue in the picture, so find that hue and average it.
//!
//! The same guess serves the filter, which looks at its own first frames
//! when no colour was given, and `source.key_colour`, which looks at a still
//! of the source. Both hand it Y, U and V samples; neither needs every pixel,
//! and a few thousand are plenty.

use super::colour::yuv_to_rgb;
use super::lut::MIN_KEY_CHROMA;
use super::params::Family;

/// What the guess found.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Guess {
    pub yuv: (u8, u8, u8),
    pub rgb: [u8; 3],
    /// The share of the samples that were the screen, 0 to 1.
    pub share: f32,
    pub family: Family,
}

/// The least share of the picture the screen must cover to be believed.
pub const MIN_SHARE: f32 = 0.08;
const BINS: usize = 72;

/// The dominant screen colour among `samples`, if there is one.
pub fn dominant(samples: impl Iterator<Item = (u8, u8, u8)>, family: Family) -> Option<Guess> {
    let mut bins = [(0u32, 0u64, 0u64, 0u64); BINS];
    let mut total = 0u32;
    for (y, u, v) in samples {
        total += 1;
        let (cx, cy) = (u as f32 - 128.0, v as f32 - 128.0);
        if (cx * cx + cy * cy).sqrt() < MIN_KEY_CHROMA * 1.5 || !fits(family, cx, cy) {
            continue;
        }
        let bin = hue_bin(cx, cy);
        let b = &mut bins[bin];
        *b = (b.0 + 1, b.1 + y as u64, b.2 + u as u64, b.3 + v as u64);
    }
    // The peak, with a bin either side, so a screen whose hue sits on a
    // boundary is not split in two.
    let around = |i: usize| [(i + BINS - 1) % BINS, i, (i + 1) % BINS];
    let score = |i: usize| around(i).iter().map(|j| bins[*j].0).sum::<u32>();
    let peak = (0..BINS).max_by_key(|i| score(*i))?;
    let (n, sy, su, sv) = around(peak).iter().fold((0u32, 0u64, 0u64, 0u64), |a, j| {
        let b = bins[*j];
        (a.0 + b.0, a.1 + b.1, a.2 + b.2, a.3 + b.3)
    });
    let share = n as f32 / total.max(1) as f32;
    if n == 0 || share < MIN_SHARE {
        return None;
    }
    let mean = |s: u64| (s / n as u64) as u8;
    let yuv = (mean(sy), mean(su), mean(sv));
    let found = if (yuv.1 as i32) < 128 && (yuv.2 as i32) < 128 { Family::Green } else { Family::Blue };
    let family = if family == Family::Any { found } else { family };
    Some(Guess { yuv, rgb: yuv_to_rgb(yuv.0, yuv.1, yuv.2), share, family })
}

/// The colour at one place, as the mean of the samples given for it.
pub fn mean(samples: impl Iterator<Item = (u8, u8, u8)>) -> Option<(u8, u8, u8)> {
    let (mut n, mut sy, mut su, mut sv) = (0u32, 0u32, 0u32, 0u32);
    for (y, u, v) in samples {
        (n, sy, su, sv) = (n + 1, sy + y as u32, su + u as u32, sv + v as u32);
    }
    (n > 0).then(|| ((sy / n) as u8, (su / n) as u8, (sv / n) as u8))
}

/// Whether a chroma direction belongs to the family looked for. Green has
/// both U and V under the middle; blue has U well over it.
fn fits(family: Family, cx: f32, cy: f32) -> bool {
    match family {
        Family::Green => cx < -6.0 && cy < -6.0,
        Family::Blue => cx > 12.0 && cx > cy.abs() * 0.6,
        Family::Any => true,
    }
}

fn hue_bin(cx: f32, cy: f32) -> usize {
    let turn = (cy.atan2(cx) / std::f32::consts::TAU).rem_euclid(1.0);
    ((turn * BINS as f32) as usize).min(BINS - 1)
}

#[cfg(test)]
mod tests {
    use super::super::colour::rgb_to_yuv;
    use super::*;

    fn picture(screen: [u8; 3], share: f32) -> Vec<(u8, u8, u8)> {
        let n = 1000;
        let lit = (n as f32 * share) as usize;
        let mut px: Vec<_> = (0..lit).map(|i| rgb_to_yuv([screen[0], screen[1].saturating_sub((i % 9) as u8), screen[2]])).collect();
        px.extend((lit..n).map(|i| rgb_to_yuv([200 - (i % 50) as u8, 150, 120])));
        px
    }

    #[test]
    fn a_green_screen_is_found_and_its_colour_is_close() {
        let g = dominant(picture([50, 180, 80], 0.6).into_iter(), Family::Green).expect("a screen");
        assert_eq!(g.family, Family::Green);
        assert!((g.rgb[1] as i32 - 176).abs() < 10, "{:?}", g.rgb);
        assert!(g.share > 0.5);
    }

    #[test]
    fn a_blue_screen_is_found_when_anything_is_asked_for() {
        let g = dominant(picture([20, 70, 200], 0.5).into_iter(), Family::Any).expect("a screen");
        assert_eq!(g.family, Family::Blue);
    }

    #[test]
    fn a_picture_with_no_screen_gives_no_guess() {
        assert!(dominant(picture([50, 180, 80], 0.02).into_iter(), Family::Green).is_none());
    }
}
