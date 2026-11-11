//! The session with one switcher, with no socket and no clock of its own.
//!
//! The caller feeds in datagrams and the time, and takes out datagrams to
//! send and the next time it must call back. That keeps every rule of the
//! protocol testable in a plain unit test, including packet loss and a
//! switcher that goes quiet, with no network and no sleeping.

mod inflight;
mod link;
mod receive;

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::command::Command;
use crate::state::State;
use crate::wire::{Flags, Header, MAX_PACKET_ID};

pub use inflight::{covered, MAX_RESENDS, RESEND_AFTER};
pub use link::{reasons, Link, LinkStatus};

/// How often to send the hello while nothing answers.
pub const HELLO_EVERY: Duration = Duration::from_secs(1);
/// Silence for this long ends a session.
pub const SILENT_AFTER: Duration = Duration::from_secs(5);
/// How long to wait after the switcher said it was full.
pub const FULL_BACKOFF: Duration = Duration::from_secs(5);
/// The largest payload put in one packet; the length field allows 2047.
const MAX_PAYLOAD: usize = 1400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Sending hellos. `next` is when the next one goes, `since` when the
    /// first did.
    Hello { next: Instant, since: Instant },
    /// The switcher answered; its state is arriving or has arrived.
    Open,
}

/// One session, and the state it has built up.
pub struct Session {
    phase: Phase,
    hello_session: u16,
    session: u16,
    next_id: u16,
    last_received: u16,
    last_heard: Instant,
    inflight: inflight::Inflight,
    outbox: VecDeque<Vec<u8>>,
    state: State,
    link: Link,
    changed: bool,
}

/// The command could not be sent, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotUp(pub Link);

impl Session {
    /// A session that starts by sending a hello at `now`. `hello_session` is
    /// the session id the hello carries; the switcher picks the real one.
    pub fn new(hello_session: u16, now: Instant) -> Session {
        Session {
            phase: Phase::Hello { next: now, since: now },
            hello_session: hello_session & 0x7fff,
            session: hello_session & 0x7fff,
            next_id: 1,
            last_received: 0,
            last_heard: now,
            inflight: Default::default(),
            outbox: VecDeque::new(),
            state: State::default(),
            link: Link::connecting(),
            changed: true,
        }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn link(&self) -> &Link {
        &self.link
    }

    /// True once since the last call if the state or the link changed.
    pub fn take_changed(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// The next datagram to send, if there is one.
    pub fn poll_transmit(&mut self) -> Option<Vec<u8>> {
        self.outbox.pop_front()
    }

    /// The latest time the caller should call [`Session::handle_timeout`].
    pub fn poll_timeout(&self) -> Instant {
        let mut at = match self.phase {
            Phase::Hello { next, .. } => next,
            Phase::Open => self.last_heard + SILENT_AFTER,
        };
        if let Some(resend) = self.inflight.deadline() {
            at = at.min(resend);
        }
        at
    }

    /// Queue commands. They go in as few packets as fit, in order.
    pub fn send(&mut self, commands: &[Command], now: Instant) -> Result<(), NotUp> {
        if !self.link.is_up() {
            return Err(NotUp(self.link.clone()));
        }
        let mut payload = Vec::new();
        for command in commands {
            let block = command.encode();
            if !payload.is_empty() && payload.len() + block.len() > MAX_PAYLOAD {
                self.queue_reliable(std::mem::take(&mut payload), now);
            }
            payload.extend(block);
        }
        if !payload.is_empty() {
            self.queue_reliable(payload, now);
        }
        Ok(())
    }

    fn queue_reliable(&mut self, payload: Vec<u8>, now: Instant) {
        let id = self.next_id;
        self.next_id = (self.next_id + 1) % MAX_PACKET_ID;
        let header = Header { flags: Flags::ACK_REQUEST, session: self.session, packet_id: id, ..Header::default() };
        let bytes = header.packet(&payload);
        self.inflight.push(id, bytes.clone(), now);
        self.outbox.push_back(bytes);
    }

    /// Time has passed: resend, send a hello, or give up on a silent switcher.
    pub fn handle_timeout(&mut self, now: Instant) {
        match self.phase {
            Phase::Hello { next, since } if now >= next => {
                let silent = now.saturating_duration_since(since) >= SILENT_AFTER;
                if silent && self.link.reason.as_deref() != Some(reasons::NO_ANSWER) {
                    self.set_reason(Some(reasons::NO_ANSWER));
                }
                self.outbox.push_back(hello(self.hello_session));
                self.phase = Phase::Hello { next: now + HELLO_EVERY, since };
            }
            Phase::Hello { .. } => {}
            Phase::Open if now >= self.last_heard + SILENT_AFTER => {
                self.restart(now, reasons::SILENT, Duration::ZERO)
            }
            Phase::Open => match self.inflight.due(now) {
                Ok(again) => self.outbox.extend(again),
                Err(()) => self.restart(now, reasons::UNACKED, Duration::ZERO),
            },
        }
    }

    /// Drop the session and start again with a hello after `wait`.
    fn restart(&mut self, now: Instant, reason: &str, wait: Duration) {
        self.inflight.clear();
        self.outbox.clear();
        self.phase = Phase::Hello { next: now + wait, since: now + wait };
        self.session = self.hello_session;
        self.link.status = LinkStatus::Connecting;
        self.set_reason(Some(reason));
    }

    fn set_reason(&mut self, reason: Option<&str>) {
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

#[cfg(test)]
mod tests;
