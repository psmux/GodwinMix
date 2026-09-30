//! MPEG-TS inside RTP (RFC 2250, SMPTE 2022-2): strip the header, count the
//! sequence numbers that never arrived.
//!
//! Done here rather than with `rtpmp2tdepay` because the same socket may carry
//! either, and a datagram says which by its first byte: 0x47 is a TS packet,
//! and an RTP version 2 header starts 0x80 to 0xBF. So the operator never has
//! to say which one a feed is, and a feed that changes its mind still works.

/// What one datagram turned out to be.
#[derive(Debug, PartialEq, Eq)]
pub enum Datagram<'a> {
    /// Bare TS packets.
    Ts(&'a [u8]),
    /// RTP around TS packets, with its sequence number.
    Rtp { seq: u16, payload: &'a [u8] },
    /// Neither. Counted and dropped.
    Junk,
}

/// Work out what `d` is and where its TS packets are.
pub fn classify(d: &[u8]) -> Datagram<'_> {
    match d.first() {
        Some(&super::SYNC) => Datagram::Ts(d),
        Some(&b) if b >> 6 == 2 => strip(d).unwrap_or(Datagram::Junk),
        _ => Datagram::Junk,
    }
}

fn strip(d: &[u8]) -> Option<Datagram<'_>> {
    let csrc = usize::from(d[0] & 0x0F);
    let mut at = 12 + 4 * csrc;
    if d.len() < at {
        return None;
    }
    if d[0] & 0x10 != 0 {
        let words = usize::from(u16::from_be_bytes([*d.get(at + 2)?, *d.get(at + 3)?]));
        at += 4 + 4 * words;
    }
    let mut end = d.len();
    if d[0] & 0x20 != 0 {
        end = end.checked_sub(usize::from(*d.last()?))?;
    }
    let payload = d.get(at..end)?;
    if payload.first() != Some(&super::SYNC) {
        return None;
    }
    Some(Datagram::Rtp { seq: u16::from_be_bytes([d[2], d[3]]), payload })
}

/// Sequence numbers seen so far, and how many were skipped.
#[derive(Debug, Default)]
pub struct Sequence {
    last: Option<u16>,
}

/// A jump larger than this is a sender that restarted, not loss.
const RESTART: u16 = 3000;

impl Sequence {
    /// How many datagrams went missing before `seq`. A late or repeated one
    /// counts nothing, and a sender that restarted counts nothing either.
    pub fn lost_before(&mut self, seq: u16) -> u64 {
        let Some(last) = self.last else {
            self.last = Some(seq);
            return 0;
        };
        let gap = seq.wrapping_sub(last);
        if gap == 0 || gap > u16::MAX / 2 {
            return 0;
        }
        self.last = Some(seq);
        if gap > RESTART {
            return 0;
        }
        u64::from(gap - 1)
    }

    /// Forget, after a silence long enough that the sender may have restarted.
    pub fn reset(&mut self) {
        self.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rtp(seq: u16, extra: &[u8]) -> Vec<u8> {
        let mut d = vec![0x80, 33];
        d.extend_from_slice(&seq.to_be_bytes());
        d.extend_from_slice(&[0; 8]);
        d.extend_from_slice(extra);
        d.push(0x47);
        d.extend_from_slice(&[0; 187]);
        d
    }

    #[test]
    fn a_datagram_says_by_its_first_byte_what_it_is() {
        let ts = [0x47u8; 188];
        assert_eq!(classify(&ts), Datagram::Ts(&ts));
        let d = rtp(9, &[]);
        assert_eq!(classify(&d), Datagram::Rtp { seq: 9, payload: &d[12..] });
        assert_eq!(classify(&[0x12, 0x34]), Datagram::Junk);
    }

    #[test]
    fn a_header_extension_is_skipped_over() {
        let mut d = rtp(1, &[0xBE, 0xDE, 0, 1, 1, 2, 3, 4]);
        d[0] |= 0x10;
        let Datagram::Rtp { payload, .. } = classify(&d) else { panic!("not rtp") };
        assert_eq!(payload.len(), 188);
    }

    #[test]
    fn gaps_count_and_late_or_repeated_packets_do_not() {
        let mut s = Sequence::default();
        assert_eq!(s.lost_before(65534), 0);
        assert_eq!(s.lost_before(1), 2, "65535 and 0 went missing across the wrap");
        assert_eq!(s.lost_before(0), 0, "a late one");
        assert_eq!(s.lost_before(1), 0, "a repeat");
        assert_eq!(s.lost_before(20_000), 0, "a restart");
    }
}
