//! A camera frame turned into what a model eats, and what it answers turned
//! into a mask.
//!
//! The frame is I420 at canvas size; a model wants a small RGB picture as
//! floats, 1 x 3 x height x width. One pass, nearest pixel, straight from the
//! planes: at 512 x 288 that is about a millisecond, and it runs on the
//! cutout's own thread, never the frame's.

use super::model::Spec;

/// One I420 picture to read from.
pub struct Planes<'a> {
    pub y: &'a [u8],
    pub u: &'a [u8],
    pub v: &'a [u8],
    pub strides: [usize; 3],
    pub width: usize,
    pub height: usize,
}

/// Fill `out` with the model's input for `src`.
pub fn input(src: &Planes<'_>, spec: &Spec, out: &mut Vec<f32>) {
    let (w, h) = (spec.width, spec.height);
    let plane = w * h;
    out.resize(3 * plane, 0.0);
    let scale = 1.0 / (255.0 * spec.std);
    let shift = spec.mean / spec.std;
    // Each output pixel reads the source pixel under its centre.
    let xs: Vec<usize> = (0..w).map(|x| ((2 * x + 1) * src.width / (2 * w)).min(src.width - 1)).collect();
    for oy in 0..h {
        let sy = ((2 * oy + 1) * src.height / (2 * h)).min(src.height - 1);
        let row_y = &src.y[sy * src.strides[0]..];
        let row_u = &src.u[(sy / 2) * src.strides[1]..];
        let row_v = &src.v[(sy / 2) * src.strides[2]..];
        for (ox, &sx) in xs.iter().enumerate() {
            let [r, g, b] = rgb(row_y[sx], row_u[sx / 2], row_v[sx / 2]);
            let i = oy * w + ox;
            out[i] = r * scale - shift;
            out[plane + i] = g * scale - shift;
            out[2 * plane + i] = b * scale - shift;
        }
    }
}

/// BT.709, limited range, which is what the canvas carries.
fn rgb(y: u8, u: u8, v: u8) -> [f32; 3] {
    let c = (y as f32 - 16.0) * 1.164;
    let d = u as f32 - 128.0;
    let e = v as f32 - 128.0;
    [
        (c + 1.793 * e).clamp(0.0, 255.0),
        (c - 0.213 * d - 0.533 * e).clamp(0.0, 255.0),
        (c + 2.112 * d).clamp(0.0, 255.0),
    ]
}

/// A model's answer, 0 to 1 per pixel, as a byte per pixel, carried forward
/// from the last one by `steady` so the edge does not shimmer.
pub fn mask(answer: &[f32], last: Option<&[u8]>, steady: f32, out: &mut Vec<u8>) {
    out.resize(answer.len(), 0);
    let keep = steady.clamp(0.0, 0.95);
    match last.filter(|l| l.len() == answer.len()) {
        Some(last) => {
            for ((o, &a), &l) in out.iter_mut().zip(answer).zip(last) {
                let now = a.clamp(0.0, 1.0) * 255.0;
                *o = (l as f32 * keep + now * (1.0 - keep)).round() as u8;
            }
        }
        None => {
            for (o, &a) in out.iter_mut().zip(answer) {
                *o = (a.clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn spec(w: usize, h: usize, mean: f32, std: f32) -> Spec {
        Spec { name: "t".into(), file: PathBuf::new(), width: w, height: h, mean, std }
    }

    #[test]
    fn white_and_black_land_at_the_ends_of_the_models_range() {
        let (w, h) = (8, 4);
        let y: Vec<u8> = (0..w * h).map(|i| if i % w < w / 2 { 16 } else { 235 }).collect();
        let uv = vec![128u8; (w / 2) * (h / 2)];
        let src = Planes { y: &y, u: &uv, v: &uv, strides: [w, w / 2, w / 2], width: w, height: h };
        let mut out = Vec::new();
        input(&src, &spec(2, 1, 0.5, 0.5), &mut out);
        assert_eq!(out.len(), 6);
        assert!((out[0] + 1.0).abs() < 0.02, "black is -1: {}", out[0]);
        assert!((out[1] - 1.0).abs() < 0.02, "white is 1: {}", out[1]);
        input(&src, &spec(2, 1, 0.0, 1.0), &mut out);
        assert!(out[0].abs() < 0.01 && (out[1] - 1.0).abs() < 0.01);
    }

    #[test]
    fn steady_carries_the_last_mask_forward() {
        let mut out = Vec::new();
        mask(&[1.0, 0.0], None, 0.5, &mut out);
        assert_eq!(out, [255, 0]);
        let last = out.clone();
        mask(&[0.0, 1.0], Some(&last), 0.5, &mut out);
        assert_eq!(out, [128, 128]);
    }
}
