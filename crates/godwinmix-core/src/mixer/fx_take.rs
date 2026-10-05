//! The mixer's half of an fx transition and of an effect: put the board's
//! pass on, and hand it the old scene's pad when that can stay live.
//!
//! The curves are `transition::fx_cut`'s, a cut at the right moment. What
//! the picture does around the cut is the pass's, drawn after the compositor
//! by the overlay board, so nothing here changes how a scene is composited.
//!
//! A clip's cut has to fall under the clip, so a clip transition does not
//! start until its clip has a first frame. The take is answered at once and
//! the scene on air stays on air, live, while the clip opens (a few frames
//! on a warm machine), then the window starts on the next frame. A clip that
//! has not opened in `READY_WAIT` starts anyway, so a broken file costs a
//! late cut and never a stuck take.

use super::transition::{Crossing, Kind, TransitionSpec};
use super::{Command, Mixer, RunningTransition};
use crate::fx::frame::Outgoing;
use crate::fx::player::Player;
use crate::fx::{Look, Plan};
use gstreamer as gst;
use tracing::{debug, warn};

/// Frames a matte or a shader waits after the take before it starts, so the
/// board has kept a whole frame of the old scene by then.
const LEAD_FRAMES: u64 = 2;

/// The longest a clip transition waits for its clip.
const READY_WAIT_MS: u64 = 800;

/// A clip transition waiting for its first frame.
pub struct PendingFx {
    id: u64,
    spec: TransitionSpec,
    x: Crossing,
    player: Option<Player>,
    asked: std::time::Instant,
}

impl Drop for PendingFx {
    fn drop(&mut self) {
        if let Some(p) = &self.player {
            p.stop();
        }
    }
}

impl Mixer {
    /// Start a clip transition's clip and hold the window until it has a
    /// frame. False for anything else, which goes ahead at once.
    pub(super) fn defer_fx(&mut self, id: u64, spec: &TransitionSpec, x: &Crossing) -> bool {
        let Kind::Fx(plan) = &spec.kind else { return false };
        let Look::Clip { path, mode, .. } = &plan.look else { return false };
        let player = match Player::start(path, crate::fx::decode_size(*mode, (self.canvas.width, self.canvas.height))) {
            Ok(p) => p,
            Err(e) => {
                warn!(fx = %plan.name, error = %format!("{e:#}"), "the clip would not open; the take is a cut");
                return false;
            }
        };
        let tx = self.handle.clone();
        player.on_first_frame(move || {
            let _ = tx.send(Command::FxReady { transition: id });
        });
        let now = self.running_time().unwrap_or(gst::ClockTime::ZERO);
        let fallback = self.schedule_command(Command::FxReady { transition: id }, now.mseconds() + READY_WAIT_MS).ok();
        self.running_transition = Some(RunningTransition {
            id,
            kind: plan.name.clone(),
            bound: self.controllers.bind(hold(x)),
            window: (x.start, x.start),
            end: fallback,
            clip: None,
            slate: false,
            fx: None,
        });
        self.pending_fx = Some(PendingFx { id, spec: spec.clone(), x: x.clone(), player: Some(player), asked: std::time::Instant::now() });
        true
    }

    /// The clip is ready, or has had long enough: cut under it from now.
    pub(super) fn fx_ready(&mut self, id: u64) {
        let Some(mut pending) = self.pending_fx.take_if(|p| p.id == id) else { return };
        if let Some(r) = self.running_transition.take() {
            if let Some(end) = r.end {
                end.unschedule();
            }
            r.bound.settle();
        }
        let mut x = pending.x.clone();
        x.start = self.compositor_now();
        let Kind::Fx(plan) = &pending.spec.kind else { return };
        let curves = match self.crossing_curves(&pending.spec, &x) {
            Ok(c) => c,
            Err(e) => return warn!(error = %format!("{e:#}"), "the fx transition had no curves"),
        };
        self.bind_window(id, &plan.name, curves.0, x.start, curves.1, false);
        let canvas = (self.canvas.width, self.canvas.height);
        let window = (x.start.nseconds(), x.end().nseconds());
        let started = crate::fx::transition(plan, &self.overlay, canvas, window, x.easing, Outgoing::Held, pending.player.take());
        debug!(fx = %plan.name, waited_ms = pending.asked.elapsed().as_millis() as u64, "a clip transition's window started");
        if let (Some(running), Ok(fx)) = (self.running_transition.as_mut(), started) {
            running.fx = Some(fx);
        }
    }

    /// Move the window of a matte or a shader on by `LEAD_FRAMES`, and say by
    /// how much. Nothing for anything else.
    pub(super) fn fx_lead(&self, spec: &TransitionSpec, x: &mut Crossing) -> gst::ClockTime {
        let Kind::Fx(plan) = &spec.kind else { return gst::ClockTime::ZERO };
        if matches!(plan.look, Look::Clip { .. }) {
            return gst::ClockTime::ZERO;
        }
        let lead = self.canvas.frame_duration() * LEAD_FRAMES;
        x.start += lead;
        lead
    }

    /// Put the pass for a matte or a shader on the board. A pass that will
    /// not start leaves the take the cut its curves already make.
    pub(super) fn start_fx(&mut self, spec: &TransitionSpec, x: &Crossing) -> Option<crate::fx::Running> {
        let Kind::Fx(plan) = &spec.kind else { return None };
        let canvas = (self.canvas.width, self.canvas.height);
        let window = (x.start.nseconds(), x.end().nseconds());
        let outgoing = self.outgoing_of(x, canvas);
        match crate::fx::transition(plan, &self.overlay, canvas, window, x.easing, outgoing, None) {
            Ok(running) => Some(running),
            Err(e) => {
                warn!(fx = %plan.name, error = %format!("{e:#}"), "the transition's look would not start; the take is a cut");
                None
            }
        }
    }

    /// The old scene's one pad, when the old scene was one picture filling
    /// the canvas, so the pass can keep it moving. Otherwise the pass holds
    /// the last frame before the window.
    fn outgoing_of(&self, x: &Crossing, canvas: (i32, i32)) -> Outgoing {
        if let [leg] = x.out.as_slice() {
            let f = &leg.from;
            let whole = f.xpos == 0 && f.ypos == 0 && (f.width, f.height) == canvas && f.alpha >= 0.99;
            if let Some(live) = whole.then(|| Outgoing::live(&leg.pad, canvas)).flatten() {
                return live;
            }
        }
        Outgoing::Held
    }

    /// `fx.fire`: an effect over the programme until its clip ends.
    pub(super) fn fire_fx(&mut self, plan: &Plan, opacity: f64) -> anyhow::Result<()> {
        crate::fx::fire(plan, &self.overlay, (self.canvas.width, self.canvas.height), opacity)
    }
}

/// Every pad held where it is while the clip opens: the old scene up, the
/// new one at nothing. Bound as curves, so the supervisor's visibility tick,
/// which leaves a driven pad alone, does not show the new scene early.
fn hold(x: &Crossing) -> Vec<super::transition::Curve> {
    let until = x.start + gst::ClockTime::from_seconds(60);
    let at = |pad: &gst::Pad, a: f64| super::transition::Curve::on(pad, "alpha", vec![(x.start, a), (until, a)]);
    x.out.iter().map(|l| at(&l.pad, l.from.alpha)).chain(x.incoming.iter().map(|l| at(&l.pad, 0.0))).collect()
}
