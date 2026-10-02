//! The shapes of the new transitions, as numbers on real compositor pads,
//! without a pipeline running. The pixels are checked in `mixer::transition_tests`.

use super::item::{self, Edge, ItemKind, ItemMotion};
use super::shape::{CropTarget, Rect};
use super::tests::{crossing, pads, value_at};
use super::*;
use crate::mixer::slots;

fn linear(mut x: Crossing) -> Crossing {
    x.easing = Easing::Linear;
    x
}

fn curve<'a>(curves: &'a [Curve], on: &gst::Pad, property: &str) -> &'a Curve {
    curves.iter().find(|c| c.is_on(on) && c.property == property).expect("a curve for that property")
}

fn half(x: &Crossing) -> gst::ClockTime {
    x.start + x.duration / 2
}

#[test]
fn a_slide_is_half_a_canvas_away_half_way_and_moves_nothing_else() {
    let (comp, p) = pads(2);
    let x = linear(crossing(vec![p[0].clone()], vec![p[1].clone()]));
    let curves = built_in(&Kind::Slide { direction: Direction::Left }).expect("built in").curves(&x);
    assert_eq!(value_at(curve(&curves, &p[1], "xpos"), half(&x)).round(), 960.0);
    assert!(!curves.iter().any(|c| c.is_on(&p[1]) && c.property == "width"), "a slide never scales");
    let out = curve(&curves, &p[0], "alpha");
    assert_eq!(value_at(out, half(&x)), 1.0, "the old scene stays whole under a slide");
    assert_eq!(out.points.last().expect("points").1, 0.0, "and leaves on the last frame");
    for pad in &p {
        comp.release_request_pad(pad);
    }
}

#[test]
fn a_push_moves_the_old_scene_out_the_far_side() {
    let (comp, p) = pads(2);
    let x = linear(crossing(vec![p[0].clone()], vec![p[1].clone()]));
    let curves = built_in(&Kind::Push { direction: Direction::Up }).expect("built in").curves(&x);
    assert_eq!(value_at(curve(&curves, &p[0], "ypos"), half(&x)).round(), -540.0);
    assert_eq!(value_at(curve(&curves, &p[1], "ypos"), half(&x)).round(), 540.0);
    for pad in &p {
        comp.release_request_pad(pad);
    }
}

#[test]
fn a_wipe_half_way_trims_half_the_picture_and_shrinks_the_pad_to_match() {
    let _ = gst::init();
    let (comp, p) = pads(2);
    let mut x = linear(crossing(vec![p[0].clone()], vec![p[1].clone()]));
    let crop = crate::gstutil::make("videocrop", "tx-shape-crop").expect("videocrop");
    x.incoming[0].crop = Some(CropTarget {
        element: crop.clone(),
        base: [0.0; 4],
        picture: Rect::new(0.0, 0.0, 1920.0, 1080.0),
        scale: (1.0, 1.0),
        source: (1920.0, 1080.0),
        offset_ns: 0,
    });
    let curves = built_in(&Kind::Wipe { direction: Direction::Left }).expect("built in").curves(&x);
    let left = curves.iter().find(|c| c.is_on(&crop) && c.property == "left").expect("the crop moves");
    assert_eq!(value_at(left, half(&x)).round(), 960.0);
    assert_eq!(value_at(curve(&curves, &p[1], "xpos"), half(&x)).round(), 960.0);
    assert_eq!(value_at(curve(&curves, &p[1], "width"), half(&x)).round(), 960.0);
    assert_eq!(left.points.last().expect("points").1, 0.0, "the crop settles back on the item's own");
    for pad in &p {
        comp.release_request_pad(pad);
    }
}

#[test]
fn a_dip_is_at_the_colour_half_way() {
    let (comp, p) = pads(2);
    let x = crossing(vec![p[0].clone()], vec![p[1].clone()]);
    let curves = built_in(&Kind::Dip { colour: 0xffff_ffff }).expect("built in").curves(&x);
    assert_eq!(value_at(curve(&curves, &p[0], "alpha"), half(&x)), 0.0);
    assert_eq!(value_at(curve(&curves, &p[1], "alpha"), half(&x)), 0.0);
    assert_eq!(curve(&curves, &p[1], "alpha").points.last().expect("points").1, 1.0);
    for pad in &p {
        comp.release_request_pad(pad);
    }
}

#[test]
fn a_lower_third_exit_ends_off_its_edge_and_hidden() {
    let (comp, p) = pads(1);
    let mut x = linear(crossing(vec![p[0].clone()], Vec::new()));
    x.out[0].from = slots::PadState { xpos: 40, ypos: 800, width: 640, height: 180, alpha: 1.0 };
    let m = ItemMotion { kind: ItemKind::Slide, duration_ms: 300, edge: Edge::Left, ..Default::default() };
    let curves = item::exit(&x.out[0], &m, &item::window(&x, &m));
    let xpos = curve(&curves, &p[0], "xpos");
    assert_eq!(xpos.points.last().expect("points").1.round(), -640.0, "off the left edge");
    assert_eq!(curve(&curves, &p[0], "alpha").points.last().expect("points").1, 0.0);
    comp.release_request_pad(&p[0]);
}
