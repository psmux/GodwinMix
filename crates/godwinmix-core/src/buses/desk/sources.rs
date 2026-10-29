//! Sources joining and leaving the buses, and a bus taken down.

use super::Desk;
use crate::buses::send::SendPoint;
use crate::buses::weight::Shared;
use crate::plugin::branch::ProgrammeBranch;
use gstreamer as gst;

impl Desk {
    /// The send point for a branch, put in if it is not there yet.
    pub(super) fn point_for(&mut self, branch: &ProgrammeBranch) -> Option<&mut SendPoint> {
        if !self.points.iter().any(|p| p.meter == branch.meter) {
            match SendPoint::insert(&self.pipeline, branch) {
                Ok(p) => self.points.push(p),
                Err(e) => {
                    tracing::error!(source = %branch.id, ?e, "could not put a source on the buses");
                    return None;
                }
            }
        }
        self.points.iter_mut().find(|p| p.meter == branch.meter)
    }

    /// A source was added: put it on every bus, if there are any.
    pub fn join_source(&mut self, branch: &ProgrammeBranch) {
        if self.live.is_empty() {
            return;
        }
        let buses: Vec<(String, gst::Element, Shared)> =
            self.live.iter().map(|l| (l.graph.id.clone(), l.graph.mix.clone(), l.shared.clone())).collect();
        let cue = self.cue.clone();
        let Some(point) = self.point_for(branch) else { return };
        for (id, mix, shared) in buses {
            if let Err(e) = point.join(&id, &mix, shared, cue.clone()) {
                tracing::warn!(bus = %id, source = %branch.id, ?e, "a source could not join a bus");
            }
        }
    }

    /// A source is going, or its branch is being held: take it off the buses
    /// and its send point out, before the branch itself comes down.
    pub fn leave_source(&mut self, id: &str) {
        while let Some(pos) = self.points.iter().position(|p| p.source == id) {
            self.points.remove(pos).remove();
        }
    }

    /// Take one built bus down: its consumers first, then every source's link
    /// to it, then its own elements. The send points go with the last bus.
    pub(in crate::buses) fn tear_down(&mut self, id: &str) {
        let Some(pos) = self.live.iter().position(|l| l.graph.id == id) else { return };
        let mut live = self.live.remove(pos);
        if let Some(d) = live.device.take() {
            d.stop();
        }
        if let Some(e) = live.encoder.take() {
            e.stop();
        }
        for p in &mut self.points {
            p.leave(id);
        }
        live.graph.remove();
        if self.live.is_empty() {
            for p in self.points.drain(..) {
                p.remove();
            }
        }
    }
}
