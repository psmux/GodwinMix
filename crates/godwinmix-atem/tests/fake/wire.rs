//! The fake's side of the session: loss, acks, its own resends, and what
//! it answers to each datagram.

use std::sync::Mutex;
use std::time::Instant;

use godwinmix_atem::session::covered;
use godwinmix_atem::wire::{Blocks, Flags, Header, MAX_PACKET_ID};
use tokio::net::UdpSocket;

use super::{dump, sim, Seen, Wire};

impl Wire {
    /// One in `lose_every` on average, picked by a fixed seed xorshift: a
    /// fixed stride would fall in step with the resend timer and lose the
    /// same packet every time, which no network does.
    pub(super) fn lose(&mut self) -> bool {
        self.count ^= self.count << 13;
        self.count ^= self.count >> 17;
        self.count ^= self.count << 5;
        self.lose_every > 0 && self.count % self.lose_every == 0
    }

    pub(super) async fn send(&mut self, socket: &UdpSocket, bytes: Vec<u8>) {
        if let (Some(peer), false) = (self.peer, self.lose()) {
            let _ = socket.send_to(&bytes, peer).await;
        }
    }

    pub(super) fn reliable(&mut self, payload: Vec<u8>) -> Vec<u8> {
        let id = self.next_id;
        self.next_id = (self.next_id + 1) % MAX_PACKET_ID;
        let bytes = Header { flags: Flags::ACK_REQUEST, session: self.session, packet_id: id, ..Header::default() }.packet(&payload);
        self.inflight.push((id, bytes.clone(), Instant::now()));
        bytes
    }

    fn ack(&self, id: u16) -> Vec<u8> {
        Header { flags: Flags::ACK_REPLY, session: self.session, ack_id: id, ..Header::default() }.packet(&[])
    }

    /// One datagram in; the datagrams to send back.
    pub(super) fn take(&mut self, h: &Header, payload: &[u8], seen: &Mutex<Seen>) -> Vec<Vec<u8>> {
        let mut seen = seen.lock().unwrap();
        if h.flags.has(Flags::HELLO) {
            seen.hellos += 1;
            seen.sim = sim::Sim::default();
            (self.next_id, self.expect, self.session) = (1, 1, self.session.wrapping_add(1) | 0x8000);
            self.inflight.clear();
            let s = h.session.to_be_bytes();
            let mut out = vec![vec![0x10, 0x14, s[0], s[1], 0, 0, 0, 0, 0, 0, 0, 0, 0x02, 0, 0, 0, 0, 0, 0, 0]];
            out.extend(dump::payloads(1300).into_iter().map(|p| self.reliable(p)));
            return out;
        }
        if h.flags.has(Flags::ACK_REPLY) {
            self.inflight.retain(|p| !covered(h.ack_id, p.0));
        }
        if !h.flags.has(Flags::ACK_REQUEST) {
            return Vec::new();
        }
        if h.packet_id != self.expect {
            // A repeat is acked again; a packet after a gap waits.
            return if covered(self.expect.wrapping_sub(1) % MAX_PACKET_ID, h.packet_id) { vec![self.ack(h.packet_id)] } else { Vec::new() };
        }
        self.expect = (self.expect + 1) % MAX_PACKET_ID;
        seen.headers.push(*h);
        let mut answer = Vec::new();
        for b in Blocks::new(payload) {
            seen.blocks.push(godwinmix_atem::wire::block(&b.name, b.body));
            answer.extend(seen.sim.apply(&b.name, b.body).concat());
        }
        let mut out = vec![self.ack(h.packet_id)];
        if !answer.is_empty() {
            out.push(self.reliable(answer));
        }
        out
    }
}
