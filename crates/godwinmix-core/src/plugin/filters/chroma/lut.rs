//! The key as a table: for every chroma pair a frame can hold, how solid the
//! pixel is and what its colour becomes once the spill is taken out.
//!
//! Everything the key decides depends on U and V alone, so it is worked out
//! once per settings change for all 65,536 pairs and each frame does one
//! lookup per 2x2 block. Building it takes under a millisecond; a frame
//! never does any of the arithmetic below.
//!
//! The measure is how much a colour leans towards the key colour, in the key
//! colour's own terms. With `k` the key's chroma and `c` the pixel's, `p` is
//! how far `c` reaches along `k`, `q` how far it strays sideways, and
//!
//! ```text
//!   m = (p - q) / |k|
//! ```
//!
//! is 1 at the key colour, 0 for any grey however bright or dark, and below
//! 0 for colours on the far side, skin among them. A shadow on the screen has
//! the key's hue at less saturation and lands between, which is what
//! `similarity` decides about: `m` above `1 - similarity` is clear, and the
//! band `smoothness` wide under that is the soft edge.
//!
//! Spill is the part of `p` that is more than `q`: the green a pixel carries
//! beyond what its own hue explains. `spill` takes that much of it out, along
//! the key's direction, and leaves brightness alone.

use super::params::Settings;

/// Below this the key colour is too close to grey to tell from anything.
pub const MIN_KEY_CHROMA: f32 = 12.0;

pub struct Lut {
    /// `[alpha, u, v, 0]` for each `u << 8 | v`.
    entries: Vec<[u8; 4]>,
}

impl Lut {
    /// The table for this key colour, in U and V, and these settings.
    pub fn build(key: (u8, u8), s: &Settings) -> Lut {
        let (kx, ky) = (key.0 as f32 - 128.0, key.1 as f32 - 128.0);
        let kn = (kx * kx + ky * ky).sqrt();
        if kn < MIN_KEY_CHROMA {
            return Lut::opaque();
        }
        let (hx, hy) = (kx / kn, ky / kn);
        let clear_from = 1.0 - s.similarity;
        let band = s.smoothness.max(0.005);
        let mut entries = Vec::with_capacity(65536);
        for u in 0..256u32 {
            for v in 0..256u32 {
                let (cx, cy) = (u as f32 - 128.0, v as f32 - 128.0);
                let p = cx * hx + cy * hy;
                let q = (cx * hy - cy * hx).abs();
                let m = (p - q) / kn;
                let solid = ((clear_from - m) / band).clamp(0.0, 1.0);
                let take = s.spill * (p - q).max(0.0);
                let (nu, nv) = (cx - take * hx, cy - take * hy);
                entries.push([
                    (solid * 255.0).round() as u8,
                    (nu + 128.0).round().clamp(0.0, 255.0) as u8,
                    (nv + 128.0).round().clamp(0.0, 255.0) as u8,
                    0,
                ]);
            }
        }
        Lut { entries }
    }

    /// No key at all: every pixel solid and unchanged. What a filter draws
    /// while it is still looking for the colour to key.
    pub fn opaque() -> Lut {
        let mut entries = Vec::with_capacity(65536);
        for u in 0..256u32 {
            for v in 0..256u32 {
                entries.push([255, u as u8, v as u8, 0]);
            }
        }
        Lut { entries }
    }

    /// Alpha, U and V for one chroma pair.
    #[inline(always)]
    pub fn get(&self, u: u8, v: u8) -> [u8; 4] {
        self.entries[(u as usize) << 8 | v as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::super::colour::rgb_to_yuv;
    use super::*;

    fn key_of(rgb: [u8; 3]) -> (u8, u8) {
        let (_, u, v) = rgb_to_yuv(rgb);
        (u, v)
    }

    fn at(lut: &Lut, rgb: [u8; 3]) -> [u8; 4] {
        let (_, u, v) = rgb_to_yuv(rgb);
        lut.get(u, v)
    }

    #[test]
    fn the_key_colour_is_clear_and_skin_and_grey_are_solid() {
        let lut = Lut::build(key_of([40, 180, 70]), &Settings::default());
        assert_eq!(at(&lut, [40, 180, 70])[0], 0, "the screen itself");
        assert_eq!(at(&lut, [20, 110, 35])[0], 0, "the screen in shadow");
        for skin in [[224, 172, 140], [141, 85, 36], [255, 219, 172]] {
            assert_eq!(at(&lut, skin)[0], 255, "skin {skin:?}");
        }
        for grey in [[0, 0, 0], [128, 128, 128], [250, 250, 250]] {
            assert_eq!(at(&lut, grey)[0], 255, "grey {grey:?}");
        }
    }

    #[test]
    fn spill_takes_the_green_out_of_a_greenish_edge_and_leaves_skin_alone() {
        let s = Settings { spill: 1.0, ..Settings::default() };
        let lut = Lut::build(key_of([40, 180, 70]), &s);
        // A grey edge pixel with some of the screen in it.
        let (_, u, v) = rgb_to_yuv([120, 150, 120]);
        let out = lut.get(u, v);
        assert!(out[0] > 0, "the edge is still partly there");
        let moved = |a: u8, b: u8| (a as i32 - 128).abs() < (b as i32 - 128).abs();
        assert!(moved(out[1], u) && moved(out[2], v), "chroma pulled towards grey: {u},{v} -> {},{}", out[1], out[2]);
        let (_, su, sv) = rgb_to_yuv([224, 172, 140]);
        assert_eq!(&lut.get(su, sv)[1..3], &[su, sv], "skin keeps its colour");
    }

    #[test]
    fn a_grey_key_keys_nothing() {
        let lut = Lut::build((128, 130), &Settings::default());
        assert_eq!(lut.get(42, 26)[0], 255);
    }
}
