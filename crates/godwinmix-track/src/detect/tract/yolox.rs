//! What YOLOX wants in and gives out. In: the picture letterboxed into the
//! input size, padded with grey 114, blue green red, 0 to 255, planar. Out:
//! one row per anchor of 85 numbers, box offsets in grid units, then the
//! object score and the 80 class scores, already through a sigmoid.

use crate::detect::Detection;
use crate::frame::FrameRef;
use crate::geom::Rect;

const STRIDES: [usize; 3] = [8, 16, 32];
const ROW: usize = 85;
const PAD: f32 = 114.0;

/// The scale from frame pixels to input pixels.
pub fn letterbox_scale(frame: &FrameRef, iw: usize, ih: usize) -> f32 {
    (iw as f32 / frame.width as f32).min(ih as f32 / frame.height as f32)
}

/// Fills `dst`, 3 by `ih` by `iw`, from `frame`.
pub fn preprocess(frame: &FrameRef, iw: usize, ih: usize, dst: &mut [f32]) {
    let r = letterbox_scale(frame, iw, ih);
    let (rw, rh) = ((frame.width as f32 * r) as usize, (frame.height as f32 * r) as usize);
    let plane = iw * ih;
    dst[..3 * plane].fill(PAD);
    for y in 0..rh.min(ih) {
        let sy = ((y as f32 + 0.5) / r) as u32;
        for x in 0..rw.min(iw) {
            let sx = ((x as f32 + 0.5) / r) as u32;
            let [red, green, blue] = frame.rgb_at(sx, sy);
            let i = y * iw + x;
            dst[i] = blue as f32;
            dst[plane + i] = green as f32;
            dst[2 * plane + i] = red as f32;
        }
    }
}

/// The number of anchor rows the model gives for this input size.
pub fn anchors(iw: usize, ih: usize) -> usize {
    STRIDES.iter().map(|s| (iw / s) * (ih / s)).sum()
}

/// Turns the raw rows into person detections above `threshold`, in the
/// frame's normalised coordinates. Appends to `out`.
pub fn decode(raw: &[f32], frame: &FrameRef, iw: usize, ih: usize, threshold: f32, out: &mut Vec<Detection>) {
    let r = letterbox_scale(frame, iw, ih);
    let (fw, fh) = (frame.width as f32, frame.height as f32);
    let mut k = 0;
    for s in STRIDES {
        let (gw, gh) = (iw / s, ih / s);
        for gy in 0..gh {
            for gx in 0..gw {
                let row = &raw[k * ROW..(k + 1) * ROW];
                k += 1;
                let score = row[4] * row[5];
                if score < threshold {
                    continue;
                }
                let sf = s as f32;
                let cx = (row[0] + gx as f32) * sf / r;
                let cy = (row[1] + gy as f32) * sf / r;
                let (w, h) = (row[2].exp() * sf / r, row[3].exp() * sf / r);
                let rect = Rect::from_center(cx / fw, cy / fh, w / fw, h / fh);
                if rect.is_finite() {
                    out.push(Detection::new(rect.intersection(&Rect::FULL), score));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::PixelFormat;

    #[test]
    fn a_decoded_anchor_lands_where_it_was_put() {
        // A 640 by 360 frame letterboxed into 416 square: scale 0.65.
        let px = vec![0u8; 640 * 360];
        let f = FrameRef::packed(640, 360, PixelFormat::Gray8, &px).unwrap();
        let (iw, ih) = (416, 416);
        let mut raw = vec![0.0f32; anchors(iw, ih) * ROW];
        // Stride 16 starts after the 52 by 52 stride 8 grid. Cell (10, 5).
        let k = 52 * 52 + 5 * 26 + 10;
        let row = &mut raw[k * ROW..(k + 1) * ROW];
        row[..6].copy_from_slice(&[0.5, 0.5, (4.0f32).ln(), (8.0f32).ln(), 0.9, 0.9]);
        let mut out = Vec::new();
        decode(&raw, &f, iw, ih, 0.5, &mut out);
        assert_eq!(out.len(), 1);
        let d = out[0];
        let r = 0.65;
        assert!((d.rect.cx() - 10.5 * 16.0 / r / 640.0).abs() < 1e-4, "{d:?}");
        assert!((d.rect.h - 128.0 / r / 360.0).abs() < 1e-4);
        assert!((d.score - 0.81).abs() < 1e-6);
    }

    #[test]
    fn letterbox_pads_below_a_wide_frame() {
        let px = vec![200u8; 64 * 32 * 3];
        let f = FrameRef::packed(64, 32, PixelFormat::Rgb8, &px).unwrap();
        let mut dst = vec![0.0; 3 * 32 * 32];
        preprocess(&f, 32, 32, &mut dst);
        assert_eq!(dst[0], 200.0);
        assert_eq!(dst[31 * 32], PAD, "the bottom half is padding");
    }
}
