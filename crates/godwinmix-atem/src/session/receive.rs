//! A datagram from the switcher.

use std::time::Instant;

use super::{covered, reasons, LinkStatus, Phase, Session, FULL_BACKOFF};
use crate::state::State;
use crate::wire::{Blocks, Flags, Header, HEADER_LEN, MAX_PACKET_ID};

/// The first payload byte of the switcher's hello: 2 accepts, 3 refuses.
const HELLO_ACCEPTED: u8 = 0x02;
const HELLO_FULL: u8 = 0x03;

impl Session {
    /// Fold in one datagram. Anything that is not a well formed ATEM packet
    /// is ignored.
    pub fn handle_datagram(&mut self, bytes: &[u8], now: Instant) {
        let Some(header) = Header::parse(bytes) else { return };
        let payload = &bytes[HEADER_LEN..];
        if header.flags.has(Flags::HELLO) {
            self.hello_reply(&header, payload, now);
            return;
        }
        if self.phase != Phase::Open {
            return;
        }
        self.last_heard = now;
        self.session = header.session;
        if header.flags.has(Flags::RESEND_REQUEST) {
            let again = self.inflight.from(header.resend_from);
            self.outbox.extend(again);
        }
        if header.flags.has(Flags::ACK_REPLY) {
            self.inflight.ack(header.ack_id);
        }
        if header.flags.has(Flags::ACK_REQUEST) {
            self.reliable(&header, payload);
        }
    }

    fn hello_reply(&mut self, header: &Header, payload: &[u8], now: Instant) {
        self.last_heard = now;
        self.session = header.session;
        match payload.first().copied() {
            Some(HELLO_FULL) => {
                self.restart(now, reasons::FULL, FULL_BACKOFF);
                return;
            }
            Some(HELLO_ACCEPTED) | None => {}
            Some(_) => {}
        }
        if let Phase::Hello { .. } = self.phase {
            self.phase = Phase::Open;
            self.state = State::default();
            self.next_id = 1;
            self.link.status = LinkStatus::Syncing;
            self.link.sessions += 1;
            self.set_reason(None);
        }
        self.last_received = header.packet_id;
        self.ack(header.packet_id);
    }

    fn reliable(&mut self, header: &Header, payload: &[u8]) {
        let expected = (self.last_received + 1) % MAX_PACKET_ID;
        if header.packet_id == expected {
            self.last_received = expected;
            self.ack(expected);
            for block in Blocks::new(payload) {
                if self.state.apply(block) {
                    self.changed = true;
                }
            }
            if self.state.initialised && self.link.status == LinkStatus::Syncing {
                self.link.status = LinkStatus::Up;
                self.changed = true;
            }
        } else if covered(self.last_received, header.packet_id) {
            // A resend of something already taken: our ack was lost.
            self.ack(self.last_received);
        }
        // Anything further ahead waits for the switcher to resend the gap.
    }

    fn ack(&mut self, id: u16) {
        let header = Header { flags: Flags::ACK_REPLY, session: self.session, ack_id: id, ..Header::default() };
        self.outbox.push_back(header.packet(&[]));
    }
}
