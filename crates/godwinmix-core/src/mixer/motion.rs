//! The mixer's half of a transition: what needs a pipeline rather than a
//! curve.
//!
//! Which scene is drawn on top, the slate's colour for a dip, and an item that
//! is shown or hidden on air playing its own way on or off. The curves
//! themselves are `transition`'s; this only decides which ones and binds them.

use super::slots::Placement;
use super::transition::{self, item, params, Crossing, Curve, Kind, TransitionSpec};
use super::{Command, Mixer, RunningTransition};
use crate::scene::id::Id;
use gstreamer as gst;
use gstreamer::prelude::*;
use tracing::debug;

impl Mixer {
    /// The curves for a crossing: the transition's own, with any item that
    /// plays its own motion on a take swapped in, and the audio. Answers how
    /// long the longest of them runs.
    pub(super) fn crossing_curves(
        &mut self,
        spec: &TransitionSpec,
        x: &Crossing,
    ) -> anyhow::Result<(Vec<Curve>, gst::ClockTime)> {
        let built: Option<Box<dyn transition::Transition>> = match spec.kind {
            // A cut for the scene, with items that move on their own.
            Kind::Cut => Some(Box::new(item::Snap)),
            _ => transition::built_in(&spec.kind),
        };
        let mut curves = match built {
            Some(t) => {
                self.pool.layer(&x.out, t.layering());
                t.curves(x)
            }
            None => self.plugin_curves(spec, x)?,
        };
        let longest = item::on_take(x, &mut curves);
        curves.extend(transition::audio_curves(x));
        Ok((curves, longest))
    }

    /// A cut that still has something to play: a take whose items enter or
    /// leave with motions of their own on a take. `None` for every other cut.
    pub(super) fn motion_take(&self, leaving: &[Placement], arriving: &[Placement]) -> Option<TransitionSpec> {
        let ms = item::any_on_take(arriving.iter().map(|p| &p.motion), leaving.iter().map(|p| &p.motion))?;
        Some(TransitionSpec::new(Kind::Cut, ms))
    }

    /// Give the slate the colour a dip goes through. True when it was changed
    /// and has to be put back. Black is what the slate already draws.
    pub(super) fn tint_slate(&self, kind: &Kind) -> bool {
        let Kind::Dip { colour } = kind else { return false };
        if *colour == params::BLACK {
            return false;
        }
        let Some(slate) = self.program.by_name("slate") else { return false };
        slate.set_property_from_str("pattern", "solid-color");
        slate.set_property("foreground-color", *colour);
        true
    }

    pub(super) fn untint_slate(&self) {
        if let Some(slate) = self.program.by_name("slate") {
            slate.set_property_from_str("pattern", "black");
        }
    }

    /// Placements with every item that is still entering held at nothing, so
    /// the apply that binds its slot never draws it in its place first.
    pub(super) fn hold_entering(&self, mut placements: Vec<Placement>) -> Vec<Placement> {
        if self.entering.is_empty() {
            return placements;
        }
        for p in &mut placements {
            if p.item.is_some_and(|id| self.entering.contains(&id)) {
                p.alpha = 0.0;
            }
        }
        placements
    }

    /// The scene on air was applied again with items shown or hidden: play
    /// each one's `enter` or `exit`. False when no item has one to play, and
    /// the caller applies the scene as the cut it has always been.
    pub(super) fn play_items(&mut self, id: u64, leaving: &[Placement]) -> bool {
        let arriving = self.current_placements();
        let had = |p: &Placement, list: &[Placement]| p.item.is_some() && list.iter().any(|q| q.item == p.item);
        let plays = |m: Option<item::ItemMotion>| m.is_some_and(|m| m.plays());
        let entering: Vec<Placement> =
            arriving.iter().filter(|p| plays(p.motion.enter) && !had(p, leaving)).cloned().collect();
        let exiting: Vec<Id> = leaving
            .iter()
            .filter(|p| plays(p.motion.exit) && !had(p, &arriving))
            .filter_map(|p| p.item)
            .collect();
        if entering.is_empty() && exiting.is_empty() {
            return false;
        }
        let out = self.pool.hold_items(&exiting);
        self.entering = entering.iter().filter_map(|p| p.item).collect();
        self.apply_visibility(true);
        let incoming = self.pool.legs_of(&entering.iter().collect::<Vec<_>>());
        let start = self.compositor_now();
        let x = Crossing {
            out: Vec::new(),
            incoming: Vec::new(),
            audio: Vec::new(),
            cover: None,
            start,
            duration: gst::ClockTime::ZERO,
            easing: Default::default(),
            canvas: (self.canvas.width, self.canvas.height),
        };
        let mut curves = Vec::new();
        let mut longest = gst::ClockTime::ZERO;
        let legs = incoming.iter().map(|l| (l, l.motion.enter, true));
        for (leg, motion, arriving) in legs.chain(out.iter().map(|l| (l, l.motion.exit, false))) {
            let Some(m) = motion.filter(|m| m.plays()) else { continue };
            let w = item::window(&x, &m);
            longest = longest.max(w.duration);
            curves.extend(if arriving { item::enter(leg, &m, &w) } else { item::exit(leg, &m, &w) });
        }
        debug!(entering = incoming.len(), leaving = out.len(), "items playing their own way on and off");
        self.bind_window(id, "item", curves, start, longest, false);
        true
    }

    /// Bind a set of curves, arm the end of their window and remember them as
    /// the transition on the canvas.
    pub(super) fn bind_window(
        &mut self,
        id: u64,
        kind: &str,
        curves: Vec<Curve>,
        start: gst::ClockTime,
        duration: gst::ClockTime,
        slate: bool,
    ) {
        let bound = self.controllers.bind(curves);
        if !bound.unbound.is_empty() {
            let over = std::time::Duration::from_nanos(duration.nseconds());
            super::ramp_curves(bound.unbound.clone(), over, self.take_generation.clone());
        }
        // The end is armed on the clock, which is a latency ahead of the
        // picture: the last frame of the window is composed at `start +
        // duration` on the compositor's timeline and pushed one latency
        // later, which is `duration` from now.
        let now = self.running_time().unwrap_or(gst::ClockTime::ZERO);
        let frame = gst::ClockTime::from_mseconds(1000 / self.canvas.fps.numer().max(1) as u64);
        let end = self
            .schedule_command(Command::TransitionEnd { transition: id }, (now + duration + frame).mseconds())
            .ok();
        self.running_transition = Some(RunningTransition {
            id,
            kind: kind.to_string(),
            bound,
            window: (start, start + duration),
            end,
            clip: None,
            slate,
        });
    }
}
