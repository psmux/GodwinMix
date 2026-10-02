//! `dip`: out to a colour, then in from it.
//!
//! The colour is the slate. It is already at the bottom of the compositor,
//! full canvas and opaque, drawing black under every scene, so dipping to
//! black is two alpha curves and nothing else. Another colour is the slate's
//! `videotestsrc` told to draw that colour for the length of the dip, which is
//! a property write on an element that is producing a frame anyway; the mixer
//! does it, because a transition never touches an element.

use super::{Crossing, Curve, Transition};

pub struct Dip;

impl Transition for Dip {
    fn name(&self) -> &str {
        "dip"
    }

    fn curves(&self, x: &Crossing) -> Vec<Curve> {
        let e = x.easing;
        let mut curves = Vec::new();
        for leg in &x.out {
            let a = leg.from.alpha;
            let points = super::sample(x, |t| a * (1.0 - e.at((t * 2.0).min(1.0))));
            curves.push(Curve::on(&leg.pad, "alpha", points));
        }
        for leg in &x.incoming {
            let a = leg.to.alpha;
            let points = super::sample(x, |t| a * e.at((t * 2.0 - 1.0).max(0.0)));
            curves.push(Curve::on(&leg.pad, "alpha", points));
        }
        curves
    }
}
