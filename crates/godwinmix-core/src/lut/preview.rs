//! Small pictures of a look, for a picker that shows what each one does.
//!
//! A preview runs the same baked table a frame runs through, so what the
//! picker shows is what goes on air. Without a camera to show it on, it uses
//! a test card made here: sky, foliage, six skin tones, every hue and a grey
//! ramp, which is what a grade is judged on.

use super::apply::lookup;
use super::bake::Table;
use crate::plugin::filters::chroma::{rgb_to_yuv, yuv_to_rgb};
use anyhow::{Context, Result};
use image::{imageops, Rgb, RgbImage};

/// `still` at `width`, keeping its shape.
pub fn fit(still: &RgbImage, width: u32) -> RgbImage {
    let w = width.clamp(64, 640);
    let h = ((still.height() as u64 * w as u64) / still.width().max(1) as u64).max(1) as u32;
    imageops::resize(still, w, h, imageops::FilterType::Triangle)
}

/// `still` through `table`, pixel by pixel.
pub fn graded(still: &RgbImage, table: &Table) -> RgbImage {
    let mut out = still.clone();
    for p in out.pixels_mut() {
        let (y, u, v) = rgb_to_yuv(p.0);
        let c = |ch| ((lookup(table, y, u, v, ch) + 256) >> 9).clamp(0, 255) as u8;
        p.0 = yuv_to_rgb(c(0), c(1), c(2));
    }
    out
}

pub fn jpeg(img: &RgbImage) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82).encode_image(img).context("encoding a preview")?;
    Ok(out)
}

const SKIN: [[u8; 3]; 6] = [[255, 224, 196], [234, 192, 160], [212, 160, 120], [176, 120, 84], [130, 84, 56], [86, 54, 36]];

/// The test card, 16 by 9.
pub fn test_card(width: u32) -> RgbImage {
    let (w, h) = (width.clamp(64, 640), width.clamp(64, 640) * 9 / 16);
    RgbImage::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / (w - 1) as f32, y as f32 / (h - 1) as f32);
        Rgb(match fy {
            f if f < 0.38 => mix([40, 90, 170], [250, 190, 140], f / 0.38),
            f if f < 0.52 => mix([30, 70, 30], [120, 170, 70], (fx * 7.0).fract()),
            f if f < 0.74 => SKIN[((fx * 6.0) as usize).min(5)],
            f if f < 0.88 => hue(fx),
            _ => [((fx * 12.0).floor() / 11.0 * 255.0) as u8; 3],
        })
    })
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    std::array::from_fn(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * t.clamp(0.0, 1.0)) as u8)
}

fn hue(t: f32) -> [u8; 3] {
    let h = t * 6.0;
    let f = |n: f32| {
        let k = (n + h) % 6.0;
        (1.0 - (k.min(4.0 - k).clamp(0.0, 1.0))) * 0.8 + 0.1
    };
    [f(5.0), f(3.0), f(1.0)].map(|c| (c * 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::super::bake::{build, Recipe};
    use super::super::looks;
    use super::*;
    use crate::plugin::filters::colour::Settings;

    #[test]
    fn the_card_is_sixteen_by_nine_and_has_colour() {
        let card = test_card(320);
        assert_eq!((card.width(), card.height()), (320, 180));
        let p = card.get_pixel(160, 110).0;
        assert!(p[0] > p[2], "skin is warmer than it is blue: {p:?}");
    }

    #[test]
    fn black_and_white_previews_without_colour() {
        let lut = looks::find("black-and-white").unwrap().lut();
        let t = build(&Recipe { lut: Some(&lut), amount: 1.0, colour: Settings::default() });
        let out = graded(&test_card(160), &t);
        for p in out.pixels().step_by(97) {
            let [r, g, b] = p.0;
            assert!((r as i32 - g as i32).abs() <= 3 && (g as i32 - b as i32).abs() <= 3, "{:?}", p.0);
        }
        assert!(jpeg(&out).unwrap().len() > 500);
    }
}
