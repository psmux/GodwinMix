//! Talkback, and the buses made for one run: a guest's return.
//!
//! A talkback source is the operator's microphone, or any input chosen for
//! it. While it is marked as one it is muted on the programme and stays
//! muted, whatever `source.audio` is asked (the mixer checks
//! [`Desk::is_talker`]), and it reaches a bus only while it is held there.
//! Holding is a weight of 1.0 on that bus (`weight.rs`), so it moves over
//! the same 80 ms as any other change and needs no element of its own.
//!
//! A guest's return bus is added with [`AudioCall::AddGuest`] rather than
//! `Add`: built the same way, but left out of what the runtime store saves,
//! because the guest it is for comes and goes with an invite.

use super::calls::Around;
use super::check::Refusal;
use super::desk::Desk;
use godwinmix_protocol::buses::AudioConfig;

/// Hold `source` on these buses, or let it go.
#[derive(Debug, Clone)]
pub struct TalkCall {
    pub source: String,
    pub buses: Vec<String>,
    pub on: bool,
}

impl Desk {
    pub(super) fn talk(&mut self, call: TalkCall, around: &Around<'_>) -> Result<(), Refusal> {
        if call.on && !around.has_source(&call.source) {
            return Err(around.no_source(&call.source));
        }
        for bus in &call.buses {
            self.live(bus)?;
        }
        for bus in &call.buses {
            let live = self.live(bus)?;
            let mut w = live.shared.lock();
            if call.on {
                w.talk.insert(call.source.clone());
            } else {
                w.talk.remove(&call.source);
            }
        }
        Ok(())
    }

    /// Mark `source` as a talkback source, or unmark it.
    pub(super) fn mark_talker(&mut self, source: &str, on: bool) {
        if on {
            self.talkers.insert(source.to_string());
        } else {
            self.talkers.remove(source);
            for live in &self.live {
                live.shared.lock().talk.remove(source);
            }
        }
    }

    /// Whether `source` is kept off the programme for talkback.
    pub fn is_talker(&self, source: &str) -> bool {
        self.talkers.contains(source)
    }

    /// The buses each source is held on for talkback now.
    pub fn talking(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for live in &self.live {
            for source in &live.shared.lock().talk {
                out.push((live.graph.id.clone(), source.clone()));
            }
        }
        out
    }

    /// What the runtime store keeps: the desk less a guest's return buses.
    pub fn saved(&self) -> AudioConfig {
        let mut config = self.config.clone();
        config.buses.retain(|b| !self.transient.contains(&b.id));
        config
    }
}
