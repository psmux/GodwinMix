//! Synthetic scenes shared by the integration tests: people as boxes that
//! move on known paths, and pictures of them for the pixel backends.
#![allow(dead_code)]

use std::time::{Duration, Instant};

use godwinmix_track::{Detection, Rect};

/// One person on a straight path, visible between two times.
#[derive(Clone, Copy, Debug)]
pub struct Walker {
    pub start: Rect,
    /// Normalised units per second.
    pub vx: f32,
    pub vy: f32,
    /// Hidden for frames in this range, end exclusive.
    pub hidden: (usize, usize),
    pub colour: [u8; 3],
}

impl Walker {
    pub fn new(start: Rect, vx: f32, colour: [u8; 3]) -> Self {
        Self { start, vx, vy: 0.0, hidden: (0, 0), colour }
    }

    pub fn hide(mut self, from: usize, to: usize) -> Self {
        self.hidden = (from, to);
        self
    }

    pub fn at(&self, frame: usize, fps: f32) -> Rect {
        let s = frame as f32 / fps;
        Rect::new(self.start.x + self.vx * s, self.start.y + self.vy * s, self.start.w, self.start.h)
    }

    pub fn visible(&self, frame: usize) -> bool {
        !(self.hidden.0..self.hidden.1).contains(&frame)
    }
}

/// The time of frame `n` at `fps`.
pub fn time(t0: Instant, n: usize, fps: f32) -> Instant {
    t0 + Duration::from_secs_f32(n as f32 / fps)
}

/// The detections a perfect detector gives for frame `n`, with a little
/// deterministic jitter so the filter has noise to smooth.
pub fn detections(walkers: &[Walker], n: usize, fps: f32, out: &mut Vec<Detection>) {
    out.clear();
    for (i, w) in walkers.iter().enumerate() {
        if w.visible(n) {
            let j = jitter(n * 7 + i) * 0.004;
            let r = w.at(n, fps);
            out.push(Detection::new(Rect::new(r.x + j, r.y - j, r.w, r.h), 0.9));
        }
    }
}

/// A repeatable value in -1..1.
pub fn jitter(seed: usize) -> f32 {
    let x = (seed as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15).rotate_left(17);
    (x % 2001) as f32 / 1000.0 - 1.0
}

/// Paints the walkers as solid boxes over a grey, slightly textured
/// background, packed RGB.
pub fn paint(walkers: &[Walker], n: usize, fps: f32, w: u32, h: u32, out: &mut Vec<u8>) {
    out.clear();
    for y in 0..h {
        for x in 0..w {
            let v = 90 + ((x * 7 + y * 13) % 11) as u8;
            out.extend_from_slice(&[v, v, v]);
        }
    }
    for wk in walkers.iter().filter(|wk| wk.visible(n)) {
        let r = wk.at(n, fps);
        let (x0, y0) = ((r.x * w as f32).max(0.0) as u32, (r.y * h as f32).max(0.0) as u32);
        let (x1, y1) = ((r.right() * w as f32).min(w as f32) as u32, (r.bottom() * h as f32).min(h as f32) as u32);
        // Clothes with a pattern in them, as real ones have.
        let dark = wk.colour.map(|c| c / 2);
        let left = r.x * w as f32;
        for y in y0..y1 {
            for x in x0..x1 {
                let i = ((y * w + x) * 3) as usize;
                let stripe = ((x as f32 - left) / 3.0) as i32 % 2 == 0;
                out[i..i + 3].copy_from_slice(if stripe { &wk.colour } else { &dark });
            }
        }
    }
}
