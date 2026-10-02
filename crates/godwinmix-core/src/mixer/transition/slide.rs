//! `slide` and `push`: the new scene travels in from an edge.
//!
//! Position only. A pad that moves is the cheapest thing a compositor does:
//! no scaling, no conversion, no crop, and the part off the canvas is not
//! blended at all. The cost of a slide is the cost of drawing both scenes,
//! which is the cost of a fade.

use super::params::Direction;
use super::shape::{leg_curves, stay_then_go, Shot};
use super::{Crossing, Curve, Layering, Transition};

/// The incoming scene slides in over the outgoing one, or pushes it out.
pub struct Slide {
    pub direction: Direction,
    /// Push the outgoing scene off the far edge ahead of the incoming one.
    pub push: bool,
}

impl Transition for Slide {
    fn name(&self) -> &str {
        if self.push {
            "push"
        } else {
            "slide"
        }
    }

    fn layering(&self) -> Layering {
        Layering::IncomingOver
    }

    fn curves(&self, x: &Crossing) -> Vec<Curve> {
        let (vx, vy) = self.direction.vector();
        // One canvas along the way it travels.
        let (dx, dy) = (vx * x.canvas.0 as f64, vy * x.canvas.1 as f64);
        let mut curves = Vec::new();
        for leg in &x.incoming {
            let to = leg.to;
            curves.extend(leg_curves(leg, x, |t| {
                let left = 1.0 - x.easing.at(t);
                Shot::of(&to).moved(-dx * left, -dy * left)
            }));
        }
        for leg in &x.out {
            if self.push {
                let from = leg.from;
                curves.extend(leg_curves(leg, x, |t| {
                    let gone = x.easing.at(t);
                    let shot = Shot::of(&from).moved(dx * gone, dy * gone);
                    // Off the canvas by the end, and hidden there so the next
                    // take does not find it drawn off screen.
                    Shot { alpha: if t >= 1.0 { 0.0 } else { from.alpha }, ..shot }
                }));
            } else {
                curves.push(stay_then_go(leg, x));
            }
        }
        curves
    }
}
