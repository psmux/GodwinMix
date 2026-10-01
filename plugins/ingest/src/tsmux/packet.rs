//! Cutting a table or a PES packet into 188 byte transport packets.

pub const PACKET: usize = 188;

/// What the first packet of a unit carries besides its payload.
#[derive(Debug, Clone, Copy, Default)]
pub struct First {
    /// The 27 MHz clock, as a 90 kHz base (the extension is always zero).
    pub pcr: Option<u64>,
    /// A decoder can start here.
    pub random_access: bool,
}

/// One PID's continuity counter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Counter(u8);

impl Counter {
    fn next(&mut self) -> u8 {
        let now = self.0;
        self.0 = (self.0 + 1) & 0x0f;
        now
    }
}

/// Append the packets for one unit of `payload` on `pid` to `out`. Tables
/// go with a pointer field in front (`table`), PES packets without.
pub fn write(out: &mut Vec<u8>, pid: u16, cc: &mut Counter, first: First, table: bool, payload: &[u8]) {
    let mut rest = payload;
    let mut start = true;
    loop {
        let mut field = Vec::new();
        if start && (first.pcr.is_some() || first.random_access) {
            field.push((if first.random_access { 0x40 } else { 0 }) | (if first.pcr.is_some() { 0x10 } else { 0 }));
            if let Some(base) = first.pcr {
                field.extend_from_slice(&pcr_bytes(base));
            }
        }
        let pointer = usize::from(start && table);
        let room = PACKET - 4 - pointer - if field.is_empty() { 0 } else { 1 + field.len() };
        let take = rest.len().min(room);
        // Stuff a short last packet out to 188 with the adaptation field.
        let short = room - take;
        let has_field = !field.is_empty() || short > 0;
        if short > 0 {
            if field.is_empty() {
                // One byte of room is the length byte alone; more needs flags.
                if short > 1 {
                    field.push(0);
                }
                field.resize(short.saturating_sub(1), 0xff);
                if short > 1 {
                    field[0] = 0;
                }
            } else {
                field.resize(field.len() + short, 0xff);
            }
        }
        let control = if has_field { 0x30 } else { 0x10 };
        out.push(0x47);
        out.push((if start { 0x40 } else { 0 }) | ((pid >> 8) as u8 & 0x1f));
        out.push(pid as u8);
        out.push(control | cc.next());
        if has_field {
            out.push(field.len() as u8);
            out.extend_from_slice(&field);
        }
        if pointer == 1 {
            out.push(0);
        }
        out.extend_from_slice(&rest[..take]);
        rest = &rest[take..];
        start = false;
        if rest.is_empty() {
            return;
        }
    }
}

fn pcr_bytes(base: u64) -> [u8; 6] {
    let b = base & ((1 << 33) - 1);
    [(b >> 25) as u8, (b >> 17) as u8, (b >> 9) as u8, (b >> 1) as u8, ((b & 1) << 7) as u8 | 0x7e, 0]
}

/// A PES header and its payload, ready to cut. `pts` and `dts` are 90 kHz.
pub fn pes(stream_id: u8, pts: u64, dts: Option<u64>, data_len: usize) -> Vec<u8> {
    let header_len = if dts.is_some() { 10 } else { 5 };
    let total = 3 + header_len + data_len;
    // Video may be longer than the field can say; zero means "unbounded".
    let length = if stream_id >= 0xe0 || total > 0xffff { 0 } else { total as u16 };
    let mut h = vec![0, 0, 1, stream_id];
    h.extend_from_slice(&length.to_be_bytes());
    h.push(0x84); // marker bits and data alignment
    h.push(if dts.is_some() { 0xc0 } else { 0x80 });
    h.push(header_len as u8);
    h.extend_from_slice(&stamp(if dts.is_some() { 0x3 } else { 0x2 }, pts));
    if let Some(dts) = dts {
        h.extend_from_slice(&stamp(0x1, dts));
    }
    h
}

fn stamp(prefix: u8, ts: u64) -> [u8; 5] {
    let t = ts & ((1 << 33) - 1);
    [
        (prefix << 4) | (((t >> 30) as u8 & 0x07) << 1) | 1,
        (t >> 22) as u8,
        (((t >> 15) as u8) << 1) | 1,
        (t >> 7) as u8,
        ((t as u8) << 1) | 1,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packets(payload_len: usize, first: First) -> Vec<u8> {
        let mut out = Vec::new();
        write(&mut out, 0x100, &mut Counter::default(), first, false, &vec![0xaa; payload_len]);
        out
    }

    #[test]
    fn every_length_cuts_into_whole_packets_with_the_payload_intact() {
        for len in [1, 2, 3, 182, 183, 184, 185, 400, 10_000] {
            for first in [First::default(), First { pcr: Some(123_456), random_access: true }] {
                let out = packets(len, first);
                assert_eq!(out.len() % PACKET, 0, "len {len}");
                let mut got = 0;
                for (i, p) in out.chunks(PACKET).enumerate() {
                    assert_eq!(p[0], 0x47);
                    assert_eq!(p[3] & 0x0f, i as u8 & 0x0f, "continuity");
                    let at = if p[3] & 0x20 != 0 { 5 + p[4] as usize } else { 4 };
                    got += p[at..].iter().filter(|b| **b == 0xaa).count();
                }
                assert_eq!(got, len, "len {len}");
            }
        }
    }

    #[test]
    fn a_pts_reads_back_as_written() {
        let s = stamp(0x2, 0x1_2345_6789);
        let back = (u64::from(s[0] >> 1 & 7) << 30) | (u64::from(s[1]) << 22) | (u64::from(s[2] >> 1) << 15) | (u64::from(s[3]) << 7) | u64::from(s[4] >> 1);
        assert_eq!(back, 0x1_2345_6789);
    }
}
