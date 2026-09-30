//! Putting table sections back together from the packets that carry them.
//!
//! A PAT is nearly always one packet. A PMT usually is. An SDT for a
//! multiplex with twenty services is not, and a section may also start in the
//! middle of a packet, after the tail of the one before. The pointer field
//! says where. This is the textbook reassembly, with a ceiling so a broken
//! feed cannot make it grow.

/// A section is at most 4096 bytes (a private one), 1024 for the tables here.
const MOST: usize = 4096;

/// One PID's worth of reassembly.
#[derive(Debug, Default)]
pub struct Assembler {
    buf: Vec<u8>,
    /// Waiting for a packet that starts a section.
    lost: bool,
}

impl Assembler {
    /// Feed one packet's payload. Every section it completes is handed to
    /// `done`, CRC checked and whole.
    pub fn push(&mut self, start: bool, payload: &[u8], mut done: impl FnMut(&[u8])) {
        if payload.is_empty() {
            return;
        }
        let mut rest = payload;
        if start {
            let pointer = rest[0] as usize;
            rest = &rest[1..];
            if pointer > rest.len() {
                self.reset();
                return;
            }
            if !self.lost && !self.buf.is_empty() {
                self.buf.extend_from_slice(&rest[..pointer]);
                self.drain(&mut done);
            }
            self.buf.clear();
            self.lost = false;
            rest = &rest[pointer..];
        } else if self.lost || self.buf.is_empty() {
            return;
        }
        self.buf.extend_from_slice(rest);
        if self.buf.len() > MOST {
            self.reset();
            return;
        }
        self.drain(&mut done);
    }

    /// Hand over every whole section at the front of the buffer.
    fn drain(&mut self, done: &mut impl FnMut(&[u8])) {
        loop {
            if self.buf.first().is_none_or(|&t| t == 0xFF) {
                self.buf.clear();
                return;
            }
            if self.buf.len() < 3 {
                return;
            }
            let len = 3 + ((usize::from(self.buf[1] & 0x0F) << 8) | usize::from(self.buf[2]));
            if self.buf.len() < len {
                return;
            }
            let section: Vec<u8> = self.buf.drain(..len).collect();
            if section.len() >= 8 && super::crc32(&section) == 0 {
                done(&section);
            }
        }
    }

    /// A packet went missing in the middle of a section. Wait for the next one
    /// that starts cleanly rather than stitching two halves of different ones.
    pub fn reset(&mut self) {
        self.buf.clear();
        self.lost = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ts::{header, packetize};

    fn sections_of(stream: &[u8]) -> Vec<Vec<u8>> {
        let mut a = Assembler::default();
        let mut got = Vec::new();
        for p in stream.chunks(crate::ts::PACKET) {
            let h = header(p).unwrap();
            a.push(h.start, &p[h.payload_at..], |s| got.push(s.to_vec()));
        }
        got
    }

    fn a_section(len: usize) -> Vec<u8> {
        let body = len - 3;
        let mut s = vec![0x42, 0xF0 | ((body >> 8) as u8), body as u8];
        s.resize(len - 4, 0x5A);
        let crc = crate::ts::crc32(&s);
        s.extend_from_slice(&crc.to_be_bytes());
        s
    }

    #[test]
    fn a_section_over_three_packets_comes_back_whole() {
        let section = a_section(500);
        let mut out = Vec::new();
        packetize(0x11, &section, &mut 0, &mut out);
        assert_eq!(sections_of(&out), vec![section]);
    }

    #[test]
    fn a_missing_middle_packet_loses_that_section_and_not_the_next() {
        let one = a_section(500);
        let two = a_section(120);
        let mut out = Vec::new();
        packetize(0x11, &one, &mut 0, &mut out);
        out.drain(188..376);
        packetize(0x11, &two, &mut 0, &mut out);
        assert_eq!(sections_of(&out), vec![two]);
    }

    #[test]
    fn a_corrupt_section_fails_its_crc_and_is_dropped() {
        let mut section = a_section(40);
        section[10] ^= 1;
        let mut out = Vec::new();
        packetize(0x11, &section, &mut 0, &mut out);
        assert!(sections_of(&out).is_empty());
    }
}
