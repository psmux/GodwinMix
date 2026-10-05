//! The compositor's part in an fx transition: a cut, at the moment the
//! board's pass needs it.
//!
//! Under a clip the scenes swap on the frame the clip covers most, the way a
//! stinger always has. Under a matte or a shader they swap on the first
//! frame of the window, because from then on the pass draws the old scene
//! back over the new one and only the pass decides what shows.

use super::{step, Crossing, Curve, Layering, Transition};
use crate::fx::Plan;

pub struct FxCut {
    /// Where the swap falls, as a fraction of the window.
    at: f64,
}

impl FxCut {
    pub fn of(plan: &Plan) -> FxCut {
        FxCut { at: plan.cut_at_ms() as f64 / plan.duration_ms.max(1) as f64 }
    }
}

impl Transition for FxCut {
    fn name(&self) -> &str {
        "fx"
    }

    fn curves(&self, x: &Crossing) -> Vec<Curve> {
        let out = x.out.iter().map(|leg| step(&leg.pad, "alpha", x, leg.from.alpha, 0.0, self.at));
        let incoming = x.incoming.iter().map(|leg| step(&leg.pad, "alpha", x, 0.0, leg.to.alpha, self.at));
        out.chain(incoming).collect()
    }

    fn layering(&self) -> Layering {
        Layering::AsIs
    }
}
