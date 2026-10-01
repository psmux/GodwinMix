//! The PAT and PMT, read from one packet each, which is where every encoder
//! this harness meets puts them. Enough to know the programs, the PCR PID and
//! the video PID.

use super::{payload_at, pid, unit_start};

/// The section in this packet, from its table id to its end before the CRC.
fn section(p: &[u8], table: u8) -> Option<&[u8]> {
    if !unit_start(p) {
        return None;
    }
    let at = payload_at(p)?;
    let start = at + 1 + usize::from(p[at]);
    let s = p.get(start..)?;
    if s.len() < 12 || s[0] != table {
        return None;
    }
    let len = (usize::from(s[1] & 0x0F) << 8) | usize::from(s[2]);
    s.get(..(3 + len).checked_sub(4)?)
}

/// `(program number, PMT PID)` for each program, the network entry left out.
pub fn pat(p: &[u8]) -> Option<Vec<(u16, u16)>> {
    if pid(p) != 0 {
        return None;
    }
    let s = section(p, 0x00)?;
    let entries = s.get(8..)?;
    let programs = entries
        .chunks_exact(4)
        .map(|e| (u16::from_be_bytes([e[0], e[1]]), u16::from_be_bytes([e[2], e[3]]) & 0x1FFF))
        .filter(|(n, _)| *n != 0)
        .collect();
    Some(programs)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pmt {
    pub pcr_pid: u16,
    pub video: Option<(u16, bool)>,
    pub streams: Vec<(u8, u16)>,
}

/// A PMT: its PCR PID, its streams, and the first video stream (true when HEVC).
pub fn pmt(p: &[u8]) -> Option<Pmt> {
    let s = section(p, 0x02)?;
    let pcr_pid = u16::from_be_bytes([s[8], s[9]]) & 0x1FFF;
    let info = (usize::from(s[10] & 0x0F) << 8) | usize::from(s[11]);
    let mut at = 12 + info;
    let mut out = Pmt { pcr_pid, ..Pmt::default() };
    while at + 5 <= s.len() {
        let kind = s[at];
        let es = u16::from_be_bytes([s[at + 1], s[at + 2]]) & 0x1FFF;
        out.streams.push((kind, es));
        if out.video.is_none() && matches!(kind, 0x01 | 0x02 | 0x10 | 0x1B | 0x24) {
            out.video = Some((es, kind == 0x24));
        }
        at += 5 + ((usize::from(s[at + 3] & 0x0F) << 8) | usize::from(s[at + 4]));
    }
    Some(out)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    /// A packet holding one section, CRC left as zeros (nothing here checks it).
    pub fn packet(pid: u16, body: &[u8]) -> [u8; 188] {
        let mut p = [0xFFu8; 188];
        p[..5].copy_from_slice(&[0x47, 0x40 | (pid >> 8) as u8, pid as u8, 0x10, 0]);
        let len = body.len() + 4 - 3;
        let mut s = body.to_vec();
        s[1] = 0xB0 | (len >> 8) as u8;
        s[2] = len as u8;
        s.extend_from_slice(&[0, 0, 0, 0]);
        p[5..5 + s.len()].copy_from_slice(&s);
        p
    }

    pub fn pat_packet(programs: &[(u16, u16)]) -> [u8; 188] {
        let mut b = vec![0x00, 0, 0, 0, 1, 0xC1, 0, 0];
        for (n, pmt) in programs {
            b.extend_from_slice(&n.to_be_bytes());
            b.extend_from_slice(&(0xE000 | pmt).to_be_bytes());
        }
        packet(0, &b)
    }

    pub fn pmt_packet(pid: u16, pcr: u16, streams: &[(u8, u16)]) -> [u8; 188] {
        let mut b = vec![0x02, 0, 0, 0, 1, 0xC1, 0, 0];
        b.extend_from_slice(&(0xE000 | pcr).to_be_bytes());
        b.extend_from_slice(&[0xF0, 0]);
        for (kind, es) in streams {
            b.push(*kind);
            b.extend_from_slice(&(0xE000 | es).to_be_bytes());
            b.extend_from_slice(&[0xF0, 0]);
        }
        packet(pid, &b)
    }

    #[test]
    fn reads_two_programs_and_their_streams() {
        let p = pat_packet(&[(1, 4096), (2, 4097)]);
        assert_eq!(pat(&p), Some(vec![(1, 4096), (2, 4097)]));
        let m = pmt(&pmt_packet(4097, 258, &[(0x04, 259), (0x24, 258)])).unwrap();
        assert_eq!(m.pcr_pid, 258);
        assert_eq!(m.video, Some((258, true)));
        assert_eq!(m.streams.len(), 2);
    }
}
