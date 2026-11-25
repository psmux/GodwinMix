//! Packets this client sent that the switcher has not acknowledged yet.
//!
//! At most [`WINDOW`] are out at once and the rest wait their turn. A
//! switcher takes packets in order only, so a burst of forty with one lost
//! near the front throws away the other thirty nine, and every resend round
//! would do the same.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::wire::{Flags, MAX_PACKET_ID};

struct Pending {
    id: u16,
    bytes: Vec<u8>,
    sent_at: Instant,
    resends: u32,
}

/// How many packets may wait for an acknowledgement at once.
pub const WINDOW: usize = 8;

#[derive(Default)]
pub struct Inflight {
    pending: Vec<Pending>,
    waiting: VecDeque<(u16, Vec<u8>)>,
}

/// True when `id` is at or before `newest`, counting the wrap at 0x8000:
/// anything up to half the id space behind counts as before.
pub fn covered(newest: u16, id: u16) -> bool {
    let behind = (newest.wrapping_sub(id)) % MAX_PACKET_ID;
    behind < MAX_PACKET_ID / 2
}

impl Inflight {
    /// Take a packet; answers it when it may go now, or keeps it until the
    /// switcher has caught up.
    pub fn push(&mut self, id: u16, bytes: Vec<u8>, now: Instant) -> Option<Vec<u8>> {
        if self.pending.len() >= WINDOW || !self.waiting.is_empty() {
            self.waiting.push_back((id, bytes));
            return None;
        }
        self.pending.push(Pending { id, bytes: bytes.clone(), sent_at: now, resends: 0 });
        Some(bytes)
    }

    /// The switcher acknowledged `id`, and with it everything before.
    /// Answers the waiting packets that may go now. A switcher that is still
    /// acknowledging is alive, so the packets left start counting again.
    pub fn ack(&mut self, id: u16, now: Instant) -> Vec<Vec<u8>> {
        let before = self.pending.len();
        self.pending.retain(|p| !covered(id, p.id));
        if self.pending.len() == before {
            return Vec::new();
        }
        self.pending.iter_mut().for_each(|p| p.resends = 0);
        let mut out = Vec::new();
        while self.pending.len() < WINDOW {
            let Some((id, bytes)) = self.waiting.pop_front() else { break };
            self.pending.push(Pending { id, bytes: bytes.clone(), sent_at: now, resends: 0 });
            out.push(bytes);
        }
        out
    }

    pub fn clear(&mut self) {
        self.pending.clear();
        self.waiting.clear();
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// When the next resend falls due.
    pub fn deadline(&self, after: Duration) -> Option<Instant> {
        self.pending.iter().map(|p| p.sent_at + after).min()
    }

    /// Packets due again, marked as retransmits. `Err` when one has been sent
    /// too often, which ends the session.
    pub fn due(&mut self, now: Instant, after: Duration, most: u32) -> Result<Vec<Vec<u8>>, ()> {
        let mut out = Vec::new();
        for p in &mut self.pending {
            if now < p.sent_at + after {
                continue;
            }
            if p.resends >= most {
                return Err(());
            }
            p.bytes[0] |= Flags::RETRANSMIT.0 << 3;
            p.sent_at = now;
            p.resends += 1;
            out.push(p.bytes.clone());
        }
        Ok(out)
    }

    /// Everything from `from` on, for a resend request.
    pub fn from(&self, from: u16) -> Vec<Vec<u8>> {
        self.pending
            .iter()
            .filter(|p| covered(p.id, from))
            .map(|p| p.bytes.clone())
            .collect()
    }
}

#[cfg(test)]
#[path = "tests_inflight.rs"]
mod tests;
