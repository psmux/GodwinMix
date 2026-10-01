//! What a small picture says: how much of it is black, and how far it moved
//! from the one before.
//!
//! The decoder hands over a thumbnail sized I420 frame (320 pixels across).
//! Only its luma plane is read, every other pixel of every other row, so the
//! copy kept for the next comparison is about 14 KB at 16:9.

/// One picture, summed up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    /// The share of pixels at or under the black threshold, 0 to 1.
    pub black_ratio: f64,
    /// The mean luma, 0 to 255.
    pub mean: f64,
}

/// A luma plane, sub sampled, kept to compare the next picture against.
#[derive(Debug, Clone, PartialEq)]
pub struct Luma {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

impl Luma {
    /// Every other pixel of every other row of an 8 bit plane with `stride`
    /// bytes per row. `None` when the plane is shorter than it says.
    pub fn sample(plane: &[u8], width: usize, height: usize, stride: usize) -> Option<Luma> {
        if width == 0 || height == 0 || stride < width || plane.len() < stride * (height - 1) + width {
            return None;
        }
        let (w, h) = (width.div_ceil(2), height.div_ceil(2));
        let mut data = Vec::with_capacity(w * h);
        for row in (0..height).step_by(2) {
            let line = &plane[row * stride..row * stride + width];
            data.extend(line.iter().step_by(2));
        }
        Some(Luma { width: w, height: h, data })
    }

    /// How much of it is black, with `black_luma` the highest value that is.
    pub fn look(&self, black_luma: u8) -> Look {
        let n = self.data.len().max(1) as f64;
        let black = self.data.iter().filter(|&&y| y <= black_luma).count() as f64;
        let sum: u64 = self.data.iter().map(|&y| u64::from(y)).sum();
        Look { black_ratio: black / n, mean: sum as f64 / n }
    }

    /// The mean absolute difference from `before`, 0 to 1. `None` when the
    /// two are not the same size, which is a change of input, not a freeze.
    pub fn diff(&self, before: &Luma) -> Option<f64> {
        if self.width != before.width || self.height != before.height || self.data.is_empty() {
            return None;
        }
        let sum: u64 = self.data.iter().zip(&before.data).map(|(a, b)| u64::from(a.abs_diff(*b))).sum();
        Some(sum as f64 / (self.data.len() as f64 * 255.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_black_plane_is_black_and_a_grey_one_is_not() {
        let black = Luma::sample(&[16u8; 320 * 180], 320, 180, 320).unwrap();
        assert_eq!((black.width, black.height), (160, 90));
        assert_eq!(black.look(38).black_ratio, 1.0);
        let grey = Luma::sample(&[128u8; 320 * 180], 320, 180, 320).unwrap();
        assert_eq!(grey.look(38).black_ratio, 0.0);
        assert_eq!(grey.look(38).mean, 128.0);
    }

    #[test]
    fn the_difference_is_zero_for_the_same_picture_and_none_across_sizes() {
        let a = Luma::sample(&[100u8; 64 * 36], 64, 36, 64).unwrap();
        let b = Luma::sample(&[110u8; 64 * 36], 64, 36, 64).unwrap();
        assert_eq!(a.diff(&a), Some(0.0));
        let d = b.diff(&a).unwrap();
        assert!((d - 10.0 / 255.0).abs() < 1e-9, "{d}");
        let small = Luma::sample(&[100u8; 32 * 18], 32, 18, 32).unwrap();
        assert_eq!(small.diff(&a), None);
    }

    #[test]
    fn padding_past_the_width_is_not_read_and_a_short_plane_is_refused() {
        let mut plane = vec![0u8; 8 * 4];
        for row in 0..4 {
            plane[row * 8..row * 8 + 6].fill(200);
        }
        let l = Luma::sample(&plane, 6, 4, 8).unwrap();
        assert!(l.data.iter().all(|&y| y == 200));
        assert!(Luma::sample(&plane[..20], 6, 4, 8).is_none());
    }
}
