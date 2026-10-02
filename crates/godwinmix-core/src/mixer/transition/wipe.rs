//! `wipe` and `box`: the new scene is revealed by an edge or a growing box.
//!
//! Both are a clip, and a clip is the slot's own `videocrop` with the pad
//! shrunk to match. A directional wipe also clips the outgoing scene to the
//! other side of the edge, so a new scene with gaps in it shows the canvas
//! behind rather than the old scene through them. A box has no rectangle for
//! "everything outside it", so the outgoing scene stays whole underneath and
//! leaves on the last frame.
//!
//! An iris (a circle) is not here: a circle cannot be cut by a crop, and doing
//! it needs a mask over the whole canvas every frame, which is the second
//! compositing pass this build does not add on the software path.

use super::params::{Direction, Point};
use super::shape::{leg_curves, stay_then_go, Rect, Shot};
use super::{Crossing, Curve, Layering, Transition};

/// An edge crosses the canvas the way `direction` says, the new scene behind it.
pub struct Wipe {
    pub direction: Direction,
}

impl Wipe {
    /// The part of the canvas the new scene has at progress `p`, and the part
    /// the old one still has.
    pub fn split(&self, canvas: (i32, i32), p: f64) -> (Rect, Rect) {
        let (w, h) = (canvas.0 as f64, canvas.1 as f64);
        match self.direction {
            Direction::Left => (Rect::new(w * (1.0 - p), 0.0, w * p, h), Rect::new(0.0, 0.0, w * (1.0 - p), h)),
            Direction::Right => (Rect::new(0.0, 0.0, w * p, h), Rect::new(w * p, 0.0, w * (1.0 - p), h)),
            Direction::Up => (Rect::new(0.0, h * (1.0 - p), w, h * p), Rect::new(0.0, 0.0, w, h * (1.0 - p))),
            Direction::Down => (Rect::new(0.0, 0.0, w, h * p), Rect::new(0.0, h * p, w, h * (1.0 - p))),
        }
    }
}

impl Transition for Wipe {
    fn name(&self) -> &str {
        "wipe"
    }

    fn layering(&self) -> Layering {
        Layering::IncomingOver
    }

    fn curves(&self, x: &Crossing) -> Vec<Curve> {
        let mut curves = Vec::new();
        for leg in &x.incoming {
            let to = leg.to;
            curves.extend(leg_curves(leg, x, |t| {
                Shot::of(&to).clipped(self.split(x.canvas, x.easing.at(t)).0)
            }));
        }
        for leg in &x.out {
            let from = leg.from;
            curves.extend(leg_curves(leg, x, |t| {
                Shot::of(&from).clipped(self.split(x.canvas, x.easing.at(t)).1)
            }));
        }
        curves
    }
}

/// A box opens out of a point until it is the whole canvas.
pub struct BoxReveal {
    pub point: Point,
}

impl Transition for BoxReveal {
    fn name(&self) -> &str {
        "box"
    }

    fn layering(&self) -> Layering {
        Layering::IncomingOver
    }

    fn curves(&self, x: &Crossing) -> Vec<Curve> {
        let (ox, oy) = self.point.on(x.canvas);
        let (w, h) = (x.canvas.0 as f64, x.canvas.1 as f64);
        let mut curves = Vec::new();
        for leg in &x.incoming {
            let to = leg.to;
            curves.extend(leg_curves(leg, x, |t| {
                let p = x.easing.at(t);
                let clip = Rect::new(ox * (1.0 - p), oy * (1.0 - p), w * p, h * p);
                Shot::of(&to).clipped(clip)
            }));
        }
        curves.extend(x.out.iter().map(|leg| stay_then_go(leg, x)));
        curves
    }
}
