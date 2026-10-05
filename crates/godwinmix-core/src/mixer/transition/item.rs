//! An item's own way on and off: its `enter` and `exit`.
//!
//! The same shots the scene transitions use, aimed at one pad. A lower third
//! that slides in from the left is a `slide` whose distance is the way to the
//! left edge rather than a canvas, and it lands where the scene put it
//! because the last frame of an entrance is the item's own placement.

use super::easing::Easing;
use super::shape::{leg_curves, Rect, Shot};
use super::{Crossing, Curve, Leg, Transition};
use crate::mixer::slots::PadState;

/// How an item comes on or goes off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ItemKind {
    #[default]
    Cut,
    Fade,
    Slide,
    Zoom,
    Wipe,
    /// Drawn as it is until the end, then gone: the item stays while a
    /// graphic plays its own way out.
    Hold,
}

impl ItemKind {
    pub fn parse(name: &str) -> ItemKind {
        match name {
            "fade" => ItemKind::Fade,
            "slide" => ItemKind::Slide,
            "zoom" => ItemKind::Zoom,
            "wipe" => ItemKind::Wipe,
            "hold" => ItemKind::Hold,
            _ => ItemKind::Cut,
        }
    }
}

/// The canvas edge an item comes in from and goes out to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Edge {
    #[default]
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    pub fn parse(name: Option<&str>) -> Edge {
        match name {
            Some("right") => Edge::Right,
            Some("top") => Edge::Top,
            Some("bottom") => Edge::Bottom,
            _ => Edge::Left,
        }
    }
}

/// One entrance or exit, as a scene item stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemMotion {
    pub kind: ItemKind,
    pub duration_ms: u64,
    pub easing: Easing,
    pub edge: Edge,
    /// Also played when a scene holding the item is taken, in place of the
    /// scene's transition for this item.
    pub on_take: bool,
}

impl ItemMotion {
    /// True when this motion has frames to play.
    pub fn plays(&self) -> bool {
        self.kind != ItemKind::Cut && self.duration_ms > 0
    }
}

/// Both of an item's motions. Empty for every item that has none, which is
/// every item made before item transitions existed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Motion {
    pub enter: Option<ItemMotion>,
    pub exit: Option<ItemMotion>,
}

/// An item arriving: from nothing to where the scene put it.
pub fn enter(leg: &Leg, m: &ItemMotion, x: &Crossing) -> Vec<Curve> {
    let (to, canvas) = (leg.to, x.canvas);
    let picture = leg.crop.as_ref().map(|c| c.picture).unwrap_or(Rect::of(&to));
    leg_curves(leg, x, |t| shot(&to, picture, m, canvas, x.easing.at(t)))
}

/// An item leaving: from where it is to gone, and hidden on the last frame.
pub fn exit(leg: &Leg, m: &ItemMotion, x: &Crossing) -> Vec<Curve> {
    let (from, canvas) = (leg.from, x.canvas);
    let picture = leg.crop.as_ref().map(|c| c.picture).unwrap_or(Rect::of(&from));
    leg_curves(leg, x, |t| {
        let s = shot(&from, picture, m, canvas, 1.0 - x.easing.at(t));
        Shot { alpha: if t >= 1.0 { 0.0 } else { s.alpha }, ..s }
    })
}

/// The item `present` of the way in: 0 is gone, 1 is in its place.
fn shot(s: &PadState, picture: Rect, m: &ItemMotion, canvas: (i32, i32), present: f64) -> Shot {
    let here = Shot::of(s);
    let away = 1.0 - present;
    let (w, h) = (canvas.0 as f64, canvas.1 as f64);
    let r = Rect::of(s);
    match m.kind {
        ItemKind::Cut | ItemKind::Hold => Shot { alpha: if present > 0.0 { s.alpha } else { 0.0 }, ..here },
        ItemKind::Fade => Shot { alpha: s.alpha * present, ..here },
        ItemKind::Slide => match m.edge {
            Edge::Left => here.moved(-(r.x + r.w) * away, 0.0),
            Edge::Right => here.moved((w - r.x) * away, 0.0),
            Edge::Top => here.moved(0.0, -(r.y + r.h) * away),
            Edge::Bottom => here.moved(0.0, (h - r.y) * away),
        },
        ItemKind::Zoom => here.scaled((r.x + r.w / 2.0, r.y + r.h / 2.0), present),
        ItemKind::Wipe => {
            let p = picture;
            here.clipped(match m.edge {
                Edge::Left => Rect::new(p.x, p.y, p.w * present, p.h),
                Edge::Right => Rect::new(p.x + p.w * away, p.y, p.w * present, p.h),
                Edge::Top => Rect::new(p.x, p.y, p.w, p.h * present),
                Edge::Bottom => Rect::new(p.x, p.y + p.h * away, p.w, p.h * present),
            })
        }
    }
}

/// A crossing for one item's motion: the same start, its own length and
/// easing.
pub fn window(x: &Crossing, m: &ItemMotion) -> Crossing {
    Crossing {
        out: Vec::new(),
        incoming: Vec::new(),
        audio: Vec::new(),
        cover: None,
        start: x.start,
        duration: gstreamer::ClockTime::from_mseconds(m.duration_ms.min(super::MAX_DURATION_MS)),
        easing: m.easing,
        canvas: x.canvas,
    }
}

/// The scene's curves with each item that has its own motion on a take
/// playing that instead. Answers how long the longest of them runs.
pub fn on_take(x: &Crossing, curves: &mut Vec<Curve>) -> gstreamer::ClockTime {
    let mut longest = x.duration;
    let mut swap = |leg: &Leg, m: Option<ItemMotion>, arriving: bool| {
        let Some(m) = m.filter(|m| m.on_take && m.plays()) else { return };
        curves.retain(|c| !c.is_on(&leg.pad) && !leg.crop.as_ref().is_some_and(|k| c.is_on(&k.element)));
        let w = window(x, &m);
        longest = longest.max(w.duration);
        curves.extend(if arriving { enter(leg, &m, &w) } else { exit(leg, &m, &w) });
    };
    for leg in &x.incoming {
        swap(leg, leg.motion.enter, true);
    }
    for leg in &x.out {
        swap(leg, leg.motion.exit, false);
    }
    longest
}

/// True when a take between these two sets of legs has an item with a motion
/// of its own to play.
pub fn any_on_take<'a>(arriving: impl Iterator<Item = &'a Motion>, leaving: impl Iterator<Item = &'a Motion>) -> Option<u64> {
    let enter = arriving.filter_map(|m| m.enter).filter(|m| m.on_take && m.plays());
    let exit = leaving.filter_map(|m| m.exit).filter(|m| m.on_take && m.plays());
    enter.chain(exit).map(|m| m.duration_ms).max()
}

/// A cut, written as curves, for a take that is a cut for the scene but has
/// items with motions of their own: everything else changes on the first
/// frame of the window.
pub struct Snap;

impl Transition for Snap {
    fn name(&self) -> &str {
        "cut"
    }

    fn curves(&self, x: &Crossing) -> Vec<Curve> {
        let at = |leg: &Leg, v: f64| Curve::on(&leg.pad, "alpha", super::sample(x, |_| v));
        let mut curves: Vec<Curve> = x.out.iter().map(|leg| at(leg, 0.0)).collect();
        curves.extend(x.incoming.iter().map(|leg| at(leg, leg.to.alpha)));
        curves
    }
}
