//! Software versions of the shaders that ship with the mixer, for a machine
//! with no GStreamer GL: a Raspberry Pi with no GL in its runtime, a server
//! with no display, a laptop whose driver will not give GStreamer a context.
//!
//! Each is written to the same idea as its GLSL rather than to the same
//! arithmetic, and in integer rows and lookups where the GLSL has floats,
//! so it runs at 1080p30 on one core. The two look alike side by side; they
//! are not the same to the pixel and nothing relies on that.

use super::super::frame::Pic;
use crate::overlay::blend::Planes;

/// One frame of a transition: the old picture, the frame holding the new
/// one (written in place), and the progress.
pub type Shader = fn(&Pic<'_>, &mut Planes<'_>, f64);

/// The software version of a shipped shader, by the name it ships under.
pub fn find(name: &str) -> Option<Shader> {
    match name {
        "glitch-slice" => Some(glitch_slice),
        "ripple" => Some(ripple),
        _ => None,
    }
}

fn hash(n: f32) -> f32 {
    ((n * 12.9898).sin() * 43758.547).rem_euclid(1.0)
}

/// Bands of the picture jump sideways, and each band changes to the new
/// scene at its own moment. `glitch-slice.glsl`.
fn glitch_slice(old: &Pic<'_>, f: &mut Planes<'_>, t: f64) {
    const BANDS: f32 = 24.0;
    let (w, h) = (f.width as usize, f.height as usize);
    let p = t as f32;
    let step = (p * 12.0).floor();
    let mut line = vec![0u8; w];
    let band = |y: usize, rows: usize| ((y as f32 / rows as f32) * BANDS).floor();
    let shift = |row: f32| ((hash(row + step * 7.0) - 0.5) * 2.0 * 0.12 * (p * std::f32::consts::PI).sin()) as f64;
    let new_now = |row: f32| p >= hash(row * 3.1) * 0.8 + 0.1;
    for (plane, rows, cols) in [(0usize, h, w), (1, h / 2, w / 2), (2, h / 2, w / 2)] {
        for y in 0..rows {
            let row = band(y, rows);
            let by = ((shift(row) * cols as f64).round() as i64).rem_euclid(cols as i64) as usize;
            let (dst, ds) = match plane {
                0 => (&mut *f.y, f.strides[0]),
                1 => (&mut *f.u, f.strides[1]),
                _ => (&mut *f.v, f.strides[2]),
            };
            let src: &[u8] = if new_now(row) {
                line[..cols].copy_from_slice(&dst[y * ds..y * ds + cols]);
                &line[..cols]
            } else {
                let o = [old.y, old.u, old.v][plane];
                &o[y * old.strides[plane]..y * old.strides[plane] + cols]
            };
            let out = &mut dst[y * ds..y * ds + cols];
            out[..cols - by].copy_from_slice(&src[by..]);
            out[cols - by..].copy_from_slice(&src[..by]);
        }
    }
}

/// Rings run out from the centre and the new scene comes up through them.
/// `ripple.glsl`.
fn ripple(old: &Pic<'_>, f: &mut Planes<'_>, t: f64) {
    let (w, h) = (f.width as usize, f.height as usize);
    let p = t as f32;
    let amp = 0.04 * (p * std::f32::consts::PI).sin();
    let up = {
        let x = ((p - 0.25) / 0.5).clamp(0.0, 1.0);
        (x * x * (3.0 - 2.0 * x) * 255.0) as u32
    };
    // The wave by distance from the centre, in a table, so a pixel is a
    // square root and a lookup.
    let table: Vec<f32> = (0..=1024).map(|i| (i as f32 / 1024.0 * 28.0 - p * 40.0).sin() * amp).collect();
    let new: [Vec<u8>; 3] = [f.y.to_vec(), f.u.to_vec(), f.v.to_vec()];
    for (plane, rows, cols) in [(0usize, h, w), (1, h / 2, w / 2), (2, h / 2, w / 2)] {
        let (dst, ds) = match plane {
            0 => (&mut *f.y, f.strides[0]),
            1 => (&mut *f.u, f.strides[1]),
            _ => (&mut *f.v, f.strides[2]),
        };
        let (o, os) = ([old.y, old.u, old.v][plane], old.strides[plane]);
        for y in 0..rows {
            let dy = (y as f32 + 0.5) / rows as f32 - 0.5;
            for x in 0..cols {
                let dx = (x as f32 + 0.5) / cols as f32 - 0.5;
                let dist = (dx * dx + dy * dy).sqrt().max(1e-4);
                let wave = table[((dist * 1024.0) as usize).min(1024)];
                let sx = ((x as f32 + dx / dist * wave * cols as f32) as isize).clamp(0, cols as isize - 1) as usize;
                let sy = ((y as f32 + dy / dist * wave * rows as f32) as isize).clamp(0, rows as isize - 1) as usize;
                let (a, b) = (o[sy * os + sx] as u32, new[plane][sy * ds + sx] as u32);
                dst[y * ds + x] = ((a * (255 - up) + b * up + 127) / 255) as u8;
            }
        }
    }
}
