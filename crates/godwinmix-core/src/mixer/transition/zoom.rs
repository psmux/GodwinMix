//! `zoom` and `zoom-out`: one scene grows out of a point, or shrinks into one.
//!
//! The pad's own box is scaled, so the compositor's scaler does the work it
//! already does for any item smaller than the canvas. It rebuilds that scaler
//! when the size changes, once a frame for the length of the zoom and never
//! otherwise; `move` has paid the same since it was written.

use super::params::Point;
use super::shape::{leg_curves, stay_then_go, there_at_once, Shot};
use super::{Crossing, Curve, Layering, Transition};

pub struct Zoom {
    pub point: Point,
    /// The outgoing scene shrinks away over the incoming one, rather than the
    /// incoming one growing over it.
    pub out: bool,
}

impl Transition for Zoom {
    fn name(&self) -> &str {
        if self.out {
            "zoom-out"
        } else {
            "zoom"
        }
    }

    fn layering(&self) -> Layering {
        if self.out {
            Layering::OutgoingOver
        } else {
            Layering::IncomingOver
        }
    }

    fn curves(&self, x: &Crossing) -> Vec<Curve> {
        let about = self.point.on(x.canvas);
        let mut curves = Vec::new();
        if self.out {
            for leg in &x.out {
                let from = leg.from;
                curves.extend(leg_curves(leg, x, |t| {
                    let shot = Shot::of(&from).scaled(about, 1.0 - x.easing.at(t));
                    Shot { alpha: if t >= 1.0 { 0.0 } else { shot.alpha }, ..shot }
                }));
            }
            curves.extend(x.incoming.iter().map(|leg| there_at_once(leg, x)));
        } else {
            for leg in &x.incoming {
                let to = leg.to;
                curves.extend(leg_curves(leg, x, |t| Shot::of(&to).scaled(about, x.easing.at(t))));
            }
            curves.extend(x.out.iter().map(|leg| stay_then_go(leg, x)));
        }
        curves
    }
}
