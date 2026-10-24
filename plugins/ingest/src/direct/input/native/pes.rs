//! PES packets put back together from the TS packets of one PID, and their
//! headers read (ISO 13818-1, 2.4.3.6).
//!
//! A packet that says its length ends when that much has come, so sound
//! goes on without waiting for the next packet to start. Video usually says
//! 0, unbounded, and ends where the next one starts. A packet that lost a
//! piece on the way is dropped whole.
//!
//! A packet's times are read off its header when it starts (`stream.rs`),
//! not when it ends: an unbounded one ends only when the next one starts,
//! and a sender that restarted may have moved the clock by then.

/// More than this and the packet is junk: a 1080i MPEG-2 I frame at 20
/// Mbit/s is under 1 MB.
const MOST: usize = 4 << 20;

#[derive(Default)]
pub struct Assembly {
    buf: Vec<u8>,
    /// The whole length, when the header said it.
    want: Option<usize>,
    broken: bool,
}

impl Assembly {
    /// Add one TS packet's payload, calling `done` with each whole PES,
    /// and whether it is the one this packet started.
    pub fn push(&mut self, start: bool, payload: &[u8], done: &mut dyn FnMut(&[u8], bool)) {
        if start {
            if !self.broken && self.want.is_none() && !self.buf.is_empty() {
                done(&self.buf, false);
            }
            self.buf.clear();
            self.broken = false;
            self.want = None;
            if payload.len() >= 6 {
                let len = usize::from(u16::from_be_bytes([payload[4], payload[5]]));
                self.want = (len > 0).then_some(len + 6);
            }
        } else if self.buf.is_empty() || self.broken {
            return;
        }
        self.buf.extend_from_slice(payload);
        if self.buf.len() > MOST {
            self.lose();
        }
        if let Some(want) = self.want.filter(|&w| self.buf.len() >= w) {
            if !self.broken {
                done(&self.buf[..want], start);
            }
            self.buf.clear();
            self.want = None;
        }
    }

    /// A packet of this PID went missing: what is half built is no good.
    pub fn lose(&mut self) {
        self.broken = true;
        self.buf.clear();
    }
}

/// A PES header: its times in 90 kHz ticks, and where the data starts.
#[derive(Debug, PartialEq, Eq)]
pub struct Header {
    pub pts: Option<u64>,
    pub dts: Option<u64>,
    pub data_at: usize,
}

pub fn header(p: &[u8]) -> Option<Header> {
    if p.len() < 9 || p[..3] != [0, 0, 1] {
        return None;
    }
    // Streams without the optional header: padding, private 2 and others.
    if matches!(p[3], 0xBC | 0xBE | 0xBF | 0xF0 | 0xF1 | 0xF2 | 0xF8 | 0xFF) {
        return Some(Header { pts: None, dts: None, data_at: 6 });
    }
    let flags = p[7] >> 6;
    let data_at = 9 + usize::from(p[8]);
    if data_at > p.len() {
        return None;
    }
    let pts = (flags & 2 != 0).then(|| time(&p[9..])).flatten();
    let dts = (flags == 3).then(|| time(&p[14..])).flatten();
    Some(Header { pts, dts, data_at })
}

/// A 33 bit time in its five bytes with marker bits.
fn time(b: &[u8]) -> Option<u64> {
    let b = b.get(..5)?;
    Some(
        (u64::from(b[0] >> 1) & 7) << 30
            | u64::from(b[1]) << 22
            | u64::from(b[2] >> 1) << 15
            | u64::from(b[3]) << 7
            | u64::from(b[4] >> 1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_gives_both_times() {
        // PTS 141015 and DTS 129003, as ffmpeg writes them.
        let mut p = vec![0, 0, 1, 0xE0, 0, 0, 0x80, 0xC0, 10];
        p.extend_from_slice(&[0x31, 0x00, 0x09, 0x4D, 0xAF, 0x11, 0x00, 0x07, 0xEF, 0xD7]);
        p.push(0xAA);
        let h = header(&p).unwrap();
        assert_eq!((h.pts, h.dts, h.data_at), (Some(141_015), Some(129_003), 19));
    }

    #[test]
    fn a_sized_packet_ends_at_its_size_and_a_broken_one_is_dropped() {
        let mut got = Vec::new();
        let mut a = Assembly::default();
        let first = [0, 0, 1, 0xC0, 0, 4, 1, 2];
        a.push(true, &first, &mut |p, _| got.push(p.to_vec()));
        a.push(false, &[3, 4, 5], &mut |p, _| got.push(p.to_vec()));
        assert_eq!(got, vec![vec![0, 0, 1, 0xC0, 0, 4, 1, 2, 3, 4]]);
        a.push(true, &[0, 0, 1, 0xE0, 0, 0, 9], &mut |p, _| got.push(p.to_vec()));
        a.lose();
        a.push(true, &[0, 0, 1, 0xE0, 0, 0, 8], &mut |p, _| got.push(p.to_vec()));
        assert_eq!(got.len(), 1, "the broken one never came out");
    }
}
