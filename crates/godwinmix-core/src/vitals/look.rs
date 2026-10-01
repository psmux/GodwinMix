//! The programme as the mosaic shows it: its cell's share of black pixels,
//! and the motion the snapshot tracker already measured for that cell.
//!
//! The mosaic is already composited and encoded for whoever is looking, and
//! the tracker already decodes it to luma for motion, so this costs one
//! more JPEG decode a second and nothing in the programme pipeline.

use crate::snapshot::{self, Latest, Pick};

/// Black share and motion of the programme cell, or `None` when the mosaic
/// has no programme return.
pub fn programme(latest: &Latest, black_luma: u8) -> Option<(f64, Option<f64>)> {
    let cell = snapshot::find_cell(&latest.cells, &Pick::Program)?;
    let index = latest.cells.iter().position(|c| c.index == cell.index)?;
    let motion = latest.motion.as_ref().and_then(|m| m.get(index).copied());
    let mosaic = snapshot::decode_jpeg(&latest.jpeg).ok()?;
    let crop = snapshot::crop_cell(&mosaic, cell);
    Some((black_ratio(&crop, black_luma), motion))
}

/// The share of pixels whose luma is at or under `black_luma`.
pub fn black_ratio(img: &image::RgbImage, black_luma: u8) -> f64 {
    let n = (img.width() * img.height()).max(1) as f64;
    let black = img
        .pixels()
        .filter(|p| {
            // BT.601 luma from full range RGB, which is what a JPEG decodes to.
            let [r, g, b] = p.0.map(u32::from);
            (299 * r + 587 * g + 114 * b) / 1000 <= u32::from(black_luma)
        })
        .count();
    black as f64 / n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_is_counted_by_luma() {
        let black = image::RgbImage::from_pixel(8, 8, image::Rgb([16, 16, 16]));
        assert_eq!(black_ratio(&black, 38), 1.0);
        let mut half = image::RgbImage::from_pixel(8, 8, image::Rgb([200, 180, 40]));
        for x in 0..4 {
            for y in 0..8 {
                half.put_pixel(x, y, image::Rgb([0, 0, 0]));
            }
        }
        assert_eq!(black_ratio(&half, 38), 0.5);
    }
}
