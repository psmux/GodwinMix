//! A binding reused by the next transition, synced the way a loaded
//! compositor syncs it: late, and never on the window's first frame.
//!
//! `gst_object_sync_values` is called here by hand with the running time an
//! aggregator would pass, so the order of events is fixed and the test does
//! not need a pipeline, a clock or a quiet machine.

use super::shape::there_at_once;
use super::tests::{crossing, pads};
use super::*;

/// A transition that brought the incoming pad in at alpha 1, synced on every
/// frame of its window as the compositor does, then settled. Five seconds
/// before `x`, as the take before always is.
fn before(controllers: &mut Controllers, x: &Crossing) {
    let mut x = x.clone();
    x.start -= gst::ClockTime::from_seconds(5);
    let bound = controllers.bind(vec![there_at_once(&x.incoming[0], &x)]);
    let mut at = x.start;
    while at <= x.end() {
        let _ = x.incoming[0].pad.sync_values(at);
        at += gst::ClockTime::from_mseconds(33);
    }
    bound.settle();
}

/// The scene coming in under a slide is at alpha 1 for the whole window. The
/// same pad came in at alpha 1 in the transition before, so its binding last
/// wrote 1, and an apply has hidden the pad by hand since. A compositor that
/// is behind first syncs the pad a frame or two into the window, past the one
/// point that differs from 1. The pad must still be drawn.
#[test]
fn a_late_first_sync_still_draws_a_pad_that_holds_the_value_it_ended_on_last_time() {
    let (comp, p) = pads(1);
    let mut controllers = Controllers::default();
    let x = crossing(Vec::new(), vec![p[0].clone()]);
    let frame = gst::ClockTime::from_mseconds(33);

    // The transition before: in at 1, drawn through to its end, settled at 1.
    before(&mut controllers, &x);
    assert_eq!(p[0].property::<f64>("alpha"), 1.0);
    // Cut away: the apply hides the pad by hand.
    p[0].set_property("alpha", 0.0f64);

    // The same take again, first synced two frames late.
    let bound = controllers.bind(vec![there_at_once(&x.incoming[0], &x)]);
    let _ = p[0].sync_values(x.start + frame * 2);
    let drawn = p[0].property::<f64>("alpha");
    assert!(drawn > 0.99, "the incoming pad was left at {drawn} two frames into the window");
    bound.settle();
    comp.release_request_pad(&p[0]);
}

/// The same with the first sync past the end of the window, which is what a
/// compositor seconds behind does: the value is the curve's last, and it is
/// written.
#[test]
fn a_first_sync_after_the_window_writes_where_the_curve_ended() {
    let (comp, p) = pads(1);
    let mut controllers = Controllers::default();
    let x = crossing(Vec::new(), vec![p[0].clone()]);
    before(&mut controllers, &x);
    p[0].set_property("alpha", 0.0f64);
    let bound = controllers.bind(vec![there_at_once(&x.incoming[0], &x)]);
    let _ = p[0].sync_values(x.end() + gst::ClockTime::from_seconds(2));
    assert_eq!(p[0].property::<f64>("alpha"), 1.0);
    bound.settle();
    comp.release_request_pad(&p[0]);
}

/// A sync before the window leaves the property alone: the control source
/// has nothing to say before its first point, and the pad keeps what the
/// apply wrote.
#[test]
fn a_sync_before_the_window_leaves_the_pad_as_the_apply_left_it() {
    let (comp, p) = pads(1);
    let mut controllers = Controllers::default();
    let x = crossing(Vec::new(), vec![p[0].clone()]);
    p[0].set_property("alpha", 0.0f64);
    let bound = controllers.bind(vec![there_at_once(&x.incoming[0], &x)]);
    let _ = p[0].sync_values(x.start - gst::ClockTime::from_mseconds(33));
    assert_eq!(p[0].property::<f64>("alpha"), 0.0);
    let _ = p[0].sync_values(x.start);
    assert_eq!(p[0].property::<f64>("alpha"), 1.0, "and the window's first frame is drawn whole");
    bound.settle();
    comp.release_request_pad(&p[0]);
}
