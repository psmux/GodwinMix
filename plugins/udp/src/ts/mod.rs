//! Just enough MPEG-TS to choose a program and count what went missing.
//!
//! No demuxer, no parser of elementary streams. A 188 byte packet has a four
//! byte header, and the tables that say which PIDs belong to which program
//! (PAT, PMT, SDT) are small and repeat several times a second. Reading those
//! and dropping the packets nobody asked for is what a hardware IRD does to a
//! multiplex, and it costs a few comparisons per packet.

pub mod filter;
pub mod plan;
pub mod psi;
pub mod rtp;
pub mod tables;

/// Every TS packet is this long. M2TS (192) is a Blu-ray thing and does not
/// travel over UDP.
pub const PACKET: usize = 188;
/// The first byte of every packet.
pub const SYNC: u8 = 0x47;
/// Stuffing. A CBR multiplex is full of these and nothing downstream wants them.
pub const NULL_PID: u16 = 0x1FFF;
pub const PAT_PID: u16 = 0x0000;
pub const SDT_PID: u16 = 0x0011;

/// The header fields of one packet, read without copying it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub pid: u16,
    pub start: bool,
    pub transport_error: bool,
    pub has_payload: bool,
    pub cc: u8,
    /// The encoder said the counter jumps here on purpose.
    pub discontinuity: bool,
    /// Where the payload starts in the packet, or `PACKET` when there is none.
    pub payload_at: usize,
}

/// Read a packet header. `None` when the sync byte is wrong or the adaptation
/// field claims more bytes than the packet has.
pub fn header(p: &[u8]) -> Option<Header> {
    if p.len() < PACKET || p[0] != SYNC {
        return None;
    }
    let afc = (p[3] >> 4) & 0x3;
    let mut payload_at = 4;
    let mut discontinuity = false;
    if afc & 0x2 != 0 {
        let len = p[4] as usize;
        if len > 0 {
            discontinuity = p[5] & 0x80 != 0;
        }
        payload_at = 5 + len;
        if payload_at > PACKET {
            return None;
        }
    }
    let has_payload = afc & 0x1 != 0 && payload_at < PACKET;
    Some(Header {
        pid: (u16::from(p[1] & 0x1F) << 8) | u16::from(p[2]),
        start: p[1] & 0x40 != 0,
        transport_error: p[1] & 0x80 != 0,
        has_payload,
        cc: p[3] & 0x0F,
        discontinuity,
        payload_at: if has_payload { payload_at } else { PACKET },
    })
}

/// CRC-32/MPEG-2: polynomial 0x04C11DB7, no reflection, no final xor.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 { (crc << 1) ^ 0x04C1_1DB7 } else { crc << 1 };
        }
    }
    crc
}

/// Cut a whole section (CRC included) into packets on `pid`, stuffed with
/// 0xFF, with the continuity counter carried in `cc`.
pub fn packetize(pid: u16, section: &[u8], cc: &mut u8, out: &mut Vec<u8>) {
    let mut rest = section;
    let mut first = true;
    loop {
        let mut p = [0xFFu8; PACKET];
        p[0] = SYNC;
        p[1] = ((pid >> 8) as u8 & 0x1F) | if first { 0x40 } else { 0 };
        p[2] = pid as u8;
        p[3] = 0x10 | (*cc & 0x0F);
        *cc = (*cc + 1) & 0x0F;
        let mut at = 4;
        if first {
            p[4] = 0; // pointer_field: the section starts right here
            at = 5;
        }
        let take = rest.len().min(PACKET - at);
        p[at..at + take].copy_from_slice(&rest[..take]);
        out.extend_from_slice(&p);
        rest = &rest[take..];
        first = false;
        if rest.is_empty() {
            return;
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// One packet with a payload and nothing else, for the other tests.
    pub fn packet(pid: u16, cc: u8, start: bool, payload: &[u8]) -> Vec<u8> {
        let mut p = vec![0xFFu8; PACKET];
        p[0] = SYNC;
        p[1] = ((pid >> 8) as u8 & 0x1F) | if start { 0x40 } else { 0 };
        p[2] = pid as u8;
        p[3] = 0x10 | (cc & 0x0F);
        let n = payload.len().min(PACKET - 4);
        p[4..4 + n].copy_from_slice(&payload[..n]);
        p
    }

    #[test]
    fn the_crc_matches_the_value_every_pat_on_air_carries() {
        // The PAT ffmpeg writes for one program on PMT PID 0x1000, which
        // ends in the CRC 2A B1 04 B2.
        let pat = [0x00, 0xB0, 0x0D, 0x00, 0x01, 0xC1, 0x00, 0x00, 0x00, 0x01, 0xF0, 0x00];
        assert_eq!(crc32(&pat), 0x2AB1_04B2);
    }

    #[test]
    fn a_header_reads_pid_counter_and_the_payload_offset() {
        let p = packet(0x100, 7, true, &[1, 2, 3]);
        let h = header(&p).expect("a well formed packet");
        assert_eq!((h.pid, h.cc, h.start, h.payload_at), (0x100, 7, true, 4));
        let mut bad = p.clone();
        bad[0] = 0;
        assert!(header(&bad).is_none());
    }

    #[test]
    fn a_long_section_is_cut_into_counted_packets() {
        let section = vec![0xABu8; 400];
        let mut cc = 14;
        let mut out = Vec::new();
        packetize(0x20, &section, &mut cc, &mut out);
        assert_eq!(out.len(), 3 * PACKET);
        assert_eq!(cc, 1);
        assert_eq!(header(&out[PACKET..]).unwrap().cc, 15);
    }
}
