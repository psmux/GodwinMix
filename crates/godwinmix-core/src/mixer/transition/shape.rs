//! One leg of a transition as a function of progress, turned into curves.
//!
//! Every transition after the first four is written the same way: for a given
//! fraction of the window, where is this pad, how opaque is it, and how much
//! of its picture shows. [`leg_curves`] samples that and binds only the
//! properties that move, so a slide drives `xpos` and nothing else and costs
//! what moving a pad costs.
//!
//! "How much shows" is a rectangle on the canvas, and the slot's own
//! `videocrop` is what hides the rest. The crop is upstream of the
//! compositor, so the pad is shrunk and moved by the same amount the picture
//! is trimmed and the two meet exactly. No pass is added: the crop was
//! already in the chain, and a pad that is narrower blends fewer pixels.

use super::{Crossing, Curve, Leg};
use crate::mixer::slots::PadState;
use gstreamer as gst;

/// A rectangle in canvas pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w: w.max(0.0), h: h.max(0.0) }
    }

    pub fn of(s: &PadState) -> Rect {
        Rect::new(s.xpos as f64, s.ypos as f64, s.width as f64, s.height as f64)
    }

    /// The part of this one inside the other, or `None` when nothing is.
    pub fn and(&self, o: &Rect) -> Option<Rect> {
        let x0 = self.x.max(o.x);
        let y0 = self.y.max(o.y);
        let x1 = (self.x + self.w).min(o.x + o.w);
        let y1 = (self.y + self.h).min(o.y + o.h);
        (x1 - x0 >= 2.0 && y1 - y0 >= 2.0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
    }

    fn area(&self) -> f64 {
        self.w * self.h
    }
}

/// What a crop driven transition needs to reach a slot's `videocrop`.
#[derive(Debug, Clone)]
pub struct CropTarget {
    pub element: gst::Element,
    /// The crop the item asks for anyway, in source pixels: left, top, right,
    /// bottom. A transition adds to it and settles back on it.
    pub base: [f64; 4],
    /// Where the picture lands on the canvas once its sizing policy is done.
    pub picture: Rect,
    /// Source pixels per canvas pixel, across and down.
    pub scale: (f64, f64),
    /// The picture's own size after the base crop, in source pixels.
    pub source: (f64, f64),
    /// Added to a running time to get the crop element's stream time, which
    /// is what it syncs its bindings against.
    pub offset_ns: i64,
}

/// One frame of a leg.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shot {
    pub at: Rect,
    pub alpha: f64,
    /// The part of the canvas this leg may show. `None` is all of it.
    pub clip: Option<Rect>,
}

impl Shot {
    pub fn of(s: &PadState) -> Shot {
        Shot { at: Rect::of(s), alpha: s.alpha, clip: None }
    }

    pub fn moved(mut self, dx: f64, dy: f64) -> Shot {
        self.at.x += dx;
        self.at.y += dy;
        self
    }

    /// Scaled about a canvas point. Too small to see is not drawn at all,
    /// rather than drawn as a one pixel dot.
    pub fn scaled(mut self, about: (f64, f64), s: f64) -> Shot {
        let s = s.max(0.0);
        self.at = Rect::new(
            about.0 + (self.at.x - about.0) * s,
            about.1 + (self.at.y - about.1) * s,
            self.at.w * s,
            self.at.h * s,
        );
        if self.at.w < 2.0 || self.at.h < 2.0 {
            self.alpha = 0.0;
        }
        self
    }

    pub fn clipped(mut self, clip: Rect) -> Shot {
        self.clip = Some(clip);
        self
    }
}

/// What one sample writes: the five pad numbers and the four crop numbers.
struct Frame {
    pad: [f64; 5],
    crop: [f64; 4],
}

const PAD: [&str; 5] = ["xpos", "ypos", "width", "height", "alpha"];
const CROP: [&str; 4] = ["left", "top", "right", "bottom"];

/// Sample a leg over the window and bind what moves.
pub fn leg_curves(leg: &Leg, x: &Crossing, shot: impl Fn(f64) -> Shot) -> Vec<Curve> {
    let frames: Vec<(gst::ClockTime, Frame)> =
        super::sample(x, |t| t).into_iter().map(|(at, t)| (at, resolve(leg, shot(t)))).collect();
    let now = Rect::of(&leg.from);
    let pad_now = [now.x, now.y, now.w, now.h, leg.from.alpha];
    let mut curves = Vec::new();
    for (i, property) in PAD.iter().enumerate() {
        let points: Vec<_> = frames.iter().map(|(at, f)| (*at, f.pad[i])).collect();
        if moves(&points, pad_now[i]) {
            curves.push(Curve::on(&leg.pad, property, points));
        }
    }
    let Some(crop) = &leg.crop else { return curves };
    for (i, property) in CROP.iter().enumerate() {
        let points: Vec<_> =
            frames.iter().map(|(at, f)| (shift(*at, crop.offset_ns), f.crop[i])).collect();
        if moves(&points, crop.base[i]) {
            curves.push(Curve::on(&crop.element, property, points));
        }
    }
    curves
}

/// True when a property is anywhere other than where it already is.
fn moves(points: &[(gst::ClockTime, f64)], now: f64) -> bool {
    points.iter().any(|(_, v)| (v - now).abs() > 0.5e-3)
}

fn shift(at: gst::ClockTime, by: i64) -> gst::ClockTime {
    gst::ClockTime::from_nseconds((at.nseconds() as i64 + by).max(0) as u64)
}

/// Turn a shot into numbers, trimming the picture to its clip.
fn resolve(leg: &Leg, s: Shot) -> Frame {
    let whole = |s: &Shot, alpha: f64, crop: [f64; 4]| Frame {
        pad: [s.at.x, s.at.y, s.at.w, s.at.h, alpha],
        crop,
    };
    let base = leg.crop.as_ref().map(|c| c.base).unwrap_or_default();
    let Some(clip) = s.clip else { return whole(&s, s.alpha, base) };
    let Some(c) = &leg.crop else {
        // No crop to reach (a turned picture, a group): the leg fades by how
        // much of it the clip would have shown, which is the honest second
        // best and costs nothing.
        let shown = s.at.and(&clip).map(|v| v.area() / s.at.area().max(1.0)).unwrap_or(0.0);
        return whole(&s, s.alpha * shown, base);
    };
    let p = c.picture;
    let Some(v) = p.and(&clip) else { return whole(&s, 0.0, base) };
    if (v.w - p.w).abs() < 0.5 && (v.h - p.h).abs() < 0.5 {
        return whole(&s, s.alpha, base);
    }
    let extra = [
        (v.x - p.x) * c.scale.0,
        (v.y - p.y) * c.scale.1,
        (p.x + p.w - v.x - v.w) * c.scale.0,
        (p.y + p.h - v.y - v.h) * c.scale.1,
    ];
    let mut crop = [0.0; 4];
    for i in 0..4 {
        crop[i] = (base[i] + extra[i].max(0.0)).round();
    }
    // Never trim a picture to nothing: a caps of width 0 is refused by every
    // element downstream. Two pixels is less than the pad is ever shown at.
    keep(&mut crop, 0, 2, c.source.0 + base[0] + base[2]);
    keep(&mut crop, 1, 3, c.source.1 + base[1] + base[3]);
    Frame { pad: [v.x, v.y, v.w, v.h, s.alpha], crop }
}

/// Leave at least two pixels between a pair of opposite crops.
fn keep(crop: &mut [f64; 4], a: usize, b: usize, size: f64) {
    let over = crop[a] + crop[b] - (size - 2.0);
    if over > 0.0 {
        crop[b] = (crop[b] - over).max(0.0);
        crop[a] = crop[a].min(size - 2.0 - crop[b]).max(0.0);
    }
}

/// A leg that stays where it is and leaves on the last frame. What the scene
/// underneath does while another one slides, zooms or wipes over it.
pub fn stay_then_go(leg: &Leg, x: &Crossing) -> Curve {
    super::step(&leg.pad, "alpha", x, leg.from.alpha, 0.0, 1.0)
}

/// A leg that is fully there from the first frame, under one that moves.
pub fn there_at_once(leg: &Leg, x: &Crossing) -> Curve {
    let a = leg.to.alpha;
    Curve::on(&leg.pad, "alpha", super::sample(x, |_| a))
}
