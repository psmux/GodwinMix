//! A green screen shot made up in code, with the answer known.
//!
//! A presenter (shirt, face, a head of hair drawn as thin strands at partial
//! cover) in front of an unevenly lit screen, with a motion blurred arm and
//! green spill on every edge. Because it is generated, the true alpha and the
//! true colour of every pixel are known, and a key can be scored by how far
//! its composite over another background lands from the true one.

use super::colour::rgb_to_yuv;

pub const SCREEN: [u8; 3] = [45, 175, 75];
const SHIRT: [u8; 3] = [60, 64, 92];
const SKIN: [u8; 3] = [224, 172, 140];
const HAIR: [u8; 3] = [70, 46, 32];

pub struct Shot {
    pub width: usize,
    pub height: usize,
    /// What the camera sees, as packed I420.
    pub i420: Vec<u8>,
    /// The true cover of the presenter, 0 to 1.
    pub alpha: Vec<f32>,
    /// The presenter's true colour, before any spill.
    pub fg: Vec<[u8; 3]>,
}

/// Cover and colour of the presenter at one point, in units of the frame.
fn presenter(x: f32, y: f32) -> (f32, [u8; 3]) {
    let head = ((x - 0.5) / 0.07).powi(2) + ((y - 0.33) / 0.11).powi(2);
    let body = ((x - 0.5) / 0.2).powi(2) + ((y - 0.95) / 0.42).powi(2);
    // Hair: a cap over the head, broken into strands that thin out upwards.
    let cap = ((x - 0.5) / 0.085).powi(2) + ((y - 0.27) / 0.09).powi(2);
    if cap < 1.0 && y < 0.31 {
        let strand = ((x * 900.0).sin() * 0.5 + 0.5) * (1.0 - cap).sqrt();
        return (strand.clamp(0.0, 1.0), HAIR);
    }
    if head < 1.0 {
        return (1.0, SKIN);
    }
    // The arm, moving: a band whose left edge is smeared over 3 percent.
    if (0.62..0.70).contains(&y) && (0.18..0.33).contains(&x) {
        return (((x - 0.18) / 0.03).clamp(0.0, 1.0), SHIRT);
    }
    let soft = (1.0 - body).clamp(0.0, 0.004) / 0.004;
    (soft, SHIRT)
}

/// The shot at this size.
pub fn studio(width: usize, height: usize) -> Shot {
    let mut rgb = vec![[0u8; 3]; width * height];
    let mut alpha = vec![0f32; width * height];
    let mut fg = vec![[0u8; 3]; width * height];
    for j in 0..height {
        for i in 0..width {
            let (x, y) = (i as f32 / width as f32, j as f32 / height as f32);
            // Brighter in the middle, a fifth darker at the edges and corners.
            let light = 1.05 - 0.25 * ((x - 0.5).powi(2) + (y - 0.45).powi(2)).sqrt() * 1.6;
            let grain = ((i * 31 + j * 17) % 7) as f32 - 3.0;
            let screen = SCREEN.map(|c| (c as f32 * light + grain).clamp(0.0, 255.0));
            let (a, colour) = presenter(x, y);
            // Spill: the screen's light on whatever is near an edge of it.
            let spill = if a > 0.0 && a < 1.0 { 0.25 } else { 0.06 };
            let lit = [0, 1, 2].map(|k| colour[k] as f32 * (1.0 - spill) + screen[k] * spill);
            let seen = [0, 1, 2].map(|k| (a * lit[k] + (1.0 - a) * screen[k]).round() as u8);
            let n = j * width + i;
            (rgb[n], alpha[n], fg[n]) = (seen, a, colour);
        }
    }
    Shot { width, height, i420: to_i420(&rgb, width, height), alpha, fg }
}

/// Packed I420 from RGB, chroma averaged over each 2x2 block.
pub fn to_i420(rgb: &[[u8; 3]], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 3 / 2];
    let (ys, rest) = out.split_at_mut(w * h);
    let (us, vs) = rest.split_at_mut(w * h / 4);
    for j in 0..h {
        for i in 0..w {
            ys[j * w + i] = rgb_to_yuv(rgb[j * w + i]).0;
        }
    }
    for by in 0..h / 2 {
        for bx in 0..w / 2 {
            let (mut su, mut sv) = (0u32, 0u32);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (_, u, v) = rgb_to_yuv(rgb[(by * 2 + dy) * w + bx * 2 + dx]);
                (su, sv) = (su + u as u32, sv + v as u32);
            }
            us[by * w / 2 + bx] = (su / 4) as u8;
            vs[by * w / 2 + bx] = (sv / 4) as u8;
        }
    }
    out
}

/// How far a composite of the shot over `under` lands from the true one: the
/// mean error in luma, and the mean green left in the chroma of edge pixels
/// (a fringe), both in 8 bit steps. `canvas` is packed I420.
pub fn score(shot: &Shot, canvas: &[u8], under: [u8; 3]) -> (f32, f32) {
    let (w, h) = (shot.width, shot.height);
    let (by, bu, bv) = rgb_to_yuv(under);
    let (mut err, mut fringe, mut edges) = (0f64, 0f64, 0u32);
    for j in 0..h {
        for i in 0..w {
            let n = j * w + i;
            let a = shot.alpha[n];
            let (fy, fu, fv) = rgb_to_yuv(shot.fg[n]);
            let truth = a * fy as f32 + (1.0 - a) * by as f32;
            err += (canvas[n] as f32 - truth).abs() as f64;
            if a > 0.05 && a < 0.95 {
                // Green is low U and low V: how far below the true mix both sit.
                let c = (j / 2) * (w / 2) + i / 2;
                let (gu, gv) = (canvas[w * h + c] as f32, canvas[w * h + w * h / 4 + c] as f32);
                let tu = a * fu as f32 + (1.0 - a) * bu as f32;
                let tv = a * fv as f32 + (1.0 - a) * bv as f32;
                fringe += ((tu - gu).max(0.0) + (tv - gv).max(0.0)) as f64;
                edges += 1;
            }
        }
    }
    ((err / (w * h) as f64) as f32, (fringe / edges.max(1) as f64) as f32)
}
