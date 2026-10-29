//! Events: the in and out points an operator marks, kept in order.
//!
//! An event is a stretch of the clock, not of any one ring, so the same
//! event plays on every angle. Ids are slugs counted up from `replay-1` and
//! never reused while the mixer runs, so a Stream Deck button bound to
//! `replay-3` never plays a different moment after `replay-2` is deleted.

use super::Refusal;
use godwinmix_protocol::replay::{MarkPoint, MarkRequest, ReplayEvent};

#[derive(Default)]
pub struct Marks {
    pub events: Vec<ReplayEvent>,
    next: u32,
}

const MAX_BACK_SECS: f64 = godwinmix_protocol::replay::MAX_SECONDS as f64;

impl Marks {
    /// Apply one `replay.mark` at `now_ms` and answer with the event it
    /// opened, closed or made.
    pub fn mark(&mut self, req: &MarkRequest, now_ms: u64) -> Result<ReplayEvent, Refusal> {
        let back = req.seconds.unwrap_or(0.0);
        if !back.is_finite() || !(0.0..=MAX_BACK_SECS).contains(&back) {
            return Err(Refusal::invalid("seconds", format!("seconds must be 0 to {MAX_BACK_SECS}, not {back}")));
        }
        let back_ms = (back * 1000.0) as u64;
        match req.point {
            Some(MarkPoint::In) => {
                // A second in point before an out starts again from here.
                self.events.retain(|e| e.out_ms.is_some());
                Ok(self.open(now_ms.saturating_sub(back_ms), None, req))
            }
            Some(MarkPoint::Out) => self.close(now_ms.saturating_sub(back_ms), req),
            None if back_ms == 0 => Err(Refusal::invalid(
                "point",
                "say which point to mark: point \"in\" or \"out\", or give seconds for an event of the last that many seconds".into(),
            )),
            None => Ok(self.open(now_ms.saturating_sub(back_ms), Some(now_ms), req)),
        }
    }

    fn open(&mut self, in_ms: u64, out_ms: Option<u64>, req: &MarkRequest) -> ReplayEvent {
        self.next += 1;
        let id = format!("replay-{}", self.next);
        let name = req.name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| id.clone());
        let event = ReplayEvent { id, name, in_ms, out_ms, angle: req.angle.clone() };
        self.events.push(event.clone());
        event
    }

    fn close(&mut self, out_ms: u64, req: &MarkRequest) -> Result<ReplayEvent, Refusal> {
        let Some(open) = self.events.iter_mut().rev().find(|e| e.out_ms.is_none()) else {
            return Err(Refusal::NotInState(
                "there is no open event to close. Mark an in point first (point \"in\"), or give seconds \
                 alone for an event of the last that many seconds"
                    .into(),
            ));
        };
        if out_ms <= open.in_ms {
            return Err(Refusal::invalid("seconds", format!("the out point would be at or before the in point of {}", open.id)));
        }
        open.out_ms = Some(out_ms);
        if let Some(name) = req.name.clone().filter(|n| !n.trim().is_empty()) {
            open.name = name;
        }
        if req.angle.is_some() {
            open.angle = req.angle.clone();
        }
        Ok(open.clone())
    }

    pub fn get(&self, id: &str) -> Result<&ReplayEvent, Refusal> {
        self.events.iter().find(|e| e.id == id).ok_or_else(|| self.missing(id))
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<ReplayEvent, Refusal> {
        if name.trim().is_empty() {
            return Err(Refusal::invalid("name", "a name cannot be empty".into()));
        }
        let missing = self.missing(id);
        let e = self.events.iter_mut().find(|e| e.id == id).ok_or(missing)?;
        e.name = name.trim().to_string();
        Ok(e.clone())
    }

    pub fn delete(&mut self, id: &str) -> Result<(), Refusal> {
        let before = self.events.len();
        self.events.retain(|e| e.id != id);
        if self.events.len() == before {
            return Err(self.missing(id));
        }
        Ok(())
    }

    fn missing(&self, id: &str) -> Refusal {
        Refusal::NotFound { kind: "event", id: id.to_string(), known: self.events.iter().map(|e| e.id.clone()).collect() }
    }
}
