//! The motion backend: no model at all. The frame is scaled down to grey,
//! compared with a slowly learned background, and the changed pixels are
//! gathered into blocks and joined into boxes. For a Raspberry Pi, for a
//! fixed wide camera, and as the fallback while a model loads. It sees
//! anything that moves, so its detections are `Class::Motion`.

mod blobs;

use blobs::Blobs;

use super::{Class, Detection, Detector};
use crate::frame::FrameRef;
use crate::geom::Rect;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MotionConfig {
    /// The width the frame is scaled to before anything else, in pixels.
    pub width: usize,
    /// How far, 0 to 255, a pixel's brightness must move from the
    /// background to count as changed.
    pub threshold: u8,
    /// The background learns 1 / 2^`learn_shift` of each new frame.
    pub learn_shift: u8,
    /// Changed pixels are counted in square blocks of this many pixels.
    pub block: usize,
    /// The fraction of a block that must change for it to count.
    pub block_fill: f32,
    /// Boxes smaller than this, as a fraction of the frame, are dropped.
    pub min_area: f32,
    /// When more than this fraction of the frame changes at once, the light
    /// changed or the camera moved: the background starts again.
    pub max_coverage: f32,
}

impl Default for MotionConfig {
    fn default() -> Self {
        Self {
            width: 160,
            threshold: 24,
            learn_shift: 5,
            block: 4,
            block_fill: 0.25,
            min_area: 0.003,
            max_coverage: 0.5,
        }
    }
}

/// A detector that finds what moves against a learned background.
#[derive(Debug, Default)]
pub struct MotionDetector {
    pub config: MotionConfig,
    w: usize,
    h: usize,
    cur: Vec<u8>,
    bg: Vec<u16>,
    blobs: Blobs,
    warm: bool,
}

impl MotionDetector {
    pub fn new(config: MotionConfig) -> Self {
        Self { config, ..Default::default() }
    }

    fn resize(&mut self, frame: &FrameRef) {
        let w = self.config.width.clamp(8, frame.width as usize);
        let h = ((w as f32 / frame.aspect()).round() as usize).max(8);
        if (w, h) != (self.w, self.h) {
            (self.w, self.h) = (w, h);
            self.cur.resize(w * h, 0);
            self.bg.resize(w * h, 0);
            self.warm = false;
        }
    }

    /// Compares with the background, learns, and counts changed pixels per
    /// block. Returns the fraction of the frame that changed.
    fn difference(&mut self) -> f32 {
        let (thr, shift, b) = (self.config.threshold as i32, self.config.learn_shift, self.config.block);
        self.blobs.start(self.w / b, self.h / b);
        let mut changed = 0usize;
        for y in 0..self.h {
            for x in 0..self.w {
                let i = y * self.w + x;
                let (c, bg) = (self.cur[i] as i32, self.bg[i] as i32);
                if (c - (bg >> 8)).abs() > thr {
                    changed += 1;
                    self.blobs.count(x / b, y / b);
                }
                self.bg[i] = (bg + (((c << 8) - bg) >> shift)) as u16;
            }
        }
        changed as f32 / (self.w * self.h) as f32
    }

    fn relearn(&mut self) {
        for (bg, c) in self.bg.iter_mut().zip(&self.cur) {
            *bg = (*c as u16) << 8;
        }
        self.warm = true;
    }
}

impl Detector for MotionDetector {
    fn detect(&mut self, frame: &FrameRef) -> Vec<Detection> {
        let mut out = Vec::new();
        self.detect_into(frame, &mut out);
        out
    }

    fn detect_into(&mut self, frame: &FrameRef, out: &mut Vec<Detection>) {
        out.clear();
        if frame.validate().is_err() {
            return;
        }
        self.resize(frame);
        frame.downscale_luma(&mut self.cur, self.w, self.h);
        if !self.warm {
            return self.relearn();
        }
        if self.difference() > self.config.max_coverage {
            return self.relearn();
        }
        let b = self.config.block;
        let min_count = ((b * b) as f32 * self.config.block_fill).ceil() as u16;
        self.blobs.label(min_count);
        let (bw, bh) = ((self.w / b) as f32, (self.h / b) as f32);
        // The blocks cover whole multiples of `b`; scale to their extent.
        let (sx, sy) = ((self.w / b * b) as f32 / self.w as f32, (self.h / b * b) as f32 / self.h as f32);
        for blob in self.blobs.boxes() {
            let r = Rect::new(
                blob.x0 as f32 / bw * sx,
                blob.y0 as f32 / bh * sy,
                (blob.x1 + 1 - blob.x0) as f32 / bw * sx,
                (blob.y1 + 1 - blob.y0) as f32 / bh * sy,
            );
            if r.area() >= self.config.min_area {
                let fill = blob.blocks as f32 / ((blob.x1 + 1 - blob.x0) * (blob.y1 + 1 - blob.y0)) as f32;
                let det = Detection { rect: r, score: 0.55 + 0.4 * fill.min(1.0), class: Class::Motion, keypoints: None };
                out.push(det);
            }
        }
    }

    fn reset(&mut self) {
        self.warm = false;
    }
}
