//! Time passing: hellos with a growing wait, resends, and giving up on a
//! switcher that has gone quiet.

use std::time::{Duration, Instant};

use super::{reasons, LinkStatus, Phase, Session};

impl Session {
    /// Time has passed: resend, send a hello, or give up on a silent switcher.
    pub fn handle_timeout(&mut self, now: Instant) {
        let t = self.timing;
        match self.phase {
            Phase::Hello { next, since, wait } if now >= next => {
                let silent = now.saturating_duration_since(since) >= t.silent_after;
                // Refused says more than no answer, so it stays.
                let said = self.link.reason.as_deref();
                if silent && said != Some(reasons::NO_ANSWER) && said != Some(reasons::REFUSED) {
                    self.set_reason(Some(reasons::NO_ANSWER));
                }
                self.outbox.push_back(hello(self.hello_session));
                let grown = if silent { (wait * 2).min(t.hello_max) } else { wait };
                self.phase = Phase::Hello { next: now + wait, since, wait: grown };
            }
            Phase::Hello { .. } => {}
            Phase::Open if now >= self.last_heard + t.silent_after => {
                self.restart(now, reasons::SILENT, Duration::ZERO)
            }
            Phase::Open => match self.inflight.due(now, t.resend_after, t.max_resends) {
                Ok(again) => self.outbox.extend(again),
                Err(()) => self.restart(now, reasons::UNACKED, Duration::ZERO),
            },
        }
    }

    /// The network said nothing listens at the address: an ICMP port
    /// unreachable, which a connected socket reports as a refused receive.
    /// Only said while saying hello; an open session waits for its silence
    /// timer, since one stray ICMP is not a lost switcher.
    pub fn handle_unreachable(&mut self) {
        if matches!(self.phase, Phase::Hello { .. }) && self.link.reason.as_deref() != Some(reasons::REFUSED) {
            self.set_reason(Some(reasons::REFUSED));
        }
    }

    /// Drop the session and start again with a hello after `wait`.
    pub(super) fn restart(&mut self, now: Instant, reason: &str, wait: Duration) {
        self.inflight.clear();
        self.outbox.clear();
        let first = self.timing.hello_every;
        self.phase = Phase::Hello { next: now + wait, since: now + wait, wait: first };
        self.session = self.hello_session;
        self.link.status = LinkStatus::Connecting;
        self.set_reason(Some(reason));
    }

    pub(super) fn set_reason(&mut self, reason: Option<&str>) {
        self.link.reason = reason.map(str::to_string);
        self.changed = true;
    }
}

/// The hello that opens a session: the hello flag, length 20, our session
/// id, and the eight byte payload open implementations send. Bytes 8 and 9
/// carry 0x00 0x3a in every known client; what they mean is not known.
pub fn hello(session: u16) -> Vec<u8> {
    let s = session.to_be_bytes();
    vec![0x10, 0x14, s[0], s[1], 0, 0, 0, 0, 0, 0x3a, 0, 0, 0x01, 0, 0, 0, 0, 0, 0, 0]
}
