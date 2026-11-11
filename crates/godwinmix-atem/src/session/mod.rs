//! The session with one switcher, with no socket and no clock of its own.
//!
//! The caller feeds in datagrams and the time, and takes out datagrams to
//! send and the next time it must call back. That keeps every rule of the
//! protocol testable in a plain unit test, including packet loss and a
//! switcher that goes quiet, with no network and no sleeping.

mod inflight;
mod link;
mod receive;
mod timer;
mod timing;

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::command::Command;
use crate::state::State;
use crate::wire::{Flags, Header, MAX_PACKET_ID};

pub use inflight::covered;
pub use link::{reasons, Link, LinkStatus};
pub use timer::hello;
pub use timing::Timing;

/// The largest payload put in one packet; the length field allows 2047.
const MAX_PAYLOAD: usize = 1400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Sending hellos. `next` is when the next one goes, `since` when the
    /// first did, `wait` the gap after the next one.
    Hello { next: Instant, since: Instant, wait: Duration },
    /// The switcher answered; its state is arriving or has arrived.
    Open,
}

/// One session, and the state it has built up.
pub struct Session {
    timing: Timing,
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
        Session::with_timing(hello_session, now, Timing::default())
    }

    pub fn with_timing(hello_session: u16, now: Instant, timing: Timing) -> Session {
        Session {
            timing,
            phase: Phase::Hello { next: now, since: now, wait: timing.hello_every },
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

    pub fn timing(&self) -> Timing {
        self.timing
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
            Phase::Open => self.last_heard + self.timing.silent_after,
        };
        if let Some(resend) = self.inflight.deadline(self.timing.resend_after) {
            at = at.min(resend);
        }
        at
    }

    /// Queue commands. They go in as few packets as fit, in order, laid out
    /// for the protocol version the switcher reported.
    pub fn send(&mut self, commands: &[Command], now: Instant) -> Result<(), NotUp> {
        if !self.link.is_up() {
            return Err(NotUp(self.link.clone()));
        }
        let protocol = self.state.protocol;
        let mut payload = Vec::new();
        for command in commands {
            let block = command.encode_for(protocol);
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
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_timer;
