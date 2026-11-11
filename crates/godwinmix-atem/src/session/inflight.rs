//! Packets this client sent that the switcher has not acknowledged yet.

use std::time::{Duration, Instant};

use crate::wire::{Flags, MAX_PACKET_ID};

struct Pending {
    id: u16,
    bytes: Vec<u8>,
    sent_at: Instant,
    resends: u32,
}

#[derive(Default)]
pub struct Inflight {
    pending: Vec<Pending>,
}

/// True when `id` is at or before `newest`, counting the wrap at 0x8000:
/// anything up to half the id space behind counts as before.
pub fn covered(newest: u16, id: u16) -> bool {
    let behind = (newest.wrapping_sub(id)) % MAX_PACKET_ID;
    behind < MAX_PACKET_ID / 2
}

impl Inflight {
    pub fn push(&mut self, id: u16, bytes: Vec<u8>, now: Instant) {
        self.pending.push(Pending { id, bytes, sent_at: now, resends: 0 });
    }

    /// The switcher acknowledged `id`, and with it everything before.
    pub fn ack(&mut self, id: u16) {
        self.pending.retain(|p| !covered(id, p.id));
    }

    pub fn clear(&mut self) {
        self.pending.clear();
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
mod tests {
    use super::*;

    const RESEND_AFTER: Duration = Duration::from_millis(200);
    const MAX_RESENDS: u32 = 10;

    #[test]
    fn covered_counts_across_the_wrap() {
        assert!(covered(5, 5));
        assert!(covered(5, 1));
        assert!(!covered(5, 6));
        assert!(covered(2, 0x7ffe));
        assert!(!covered(0x7ffe, 2));
    }

    #[test]
    fn an_ack_clears_everything_up_to_it() {
        let now = Instant::now();
        let mut inflight = Inflight::default();
        for id in [0x7ffe, 0x7fff, 0, 1] {
            inflight.push(id, vec![0x08, 12], now);
        }
        inflight.ack(0);
        assert_eq!(inflight.len(), 1);
    }

    #[test]
    fn a_packet_is_resent_as_a_retransmit_and_then_given_up_on() {
        let start = Instant::now();
        let mut inflight = Inflight::default();
        inflight.push(1, vec![0x08, 12], start);
        let mut now = start;
        for _ in 0..MAX_RESENDS {
            now += RESEND_AFTER;
            let again = inflight.due(now, RESEND_AFTER, MAX_RESENDS).unwrap();
            assert_eq!(again[0][0], 0x28);
        }
        assert!(inflight.due(now + RESEND_AFTER, RESEND_AFTER, MAX_RESENDS).is_err());
    }
}
