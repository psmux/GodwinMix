//! The two tables the filter writes: a PAT naming one program, and a PMT
//! naming only the chosen streams. CRC and all.

use super::{Program, Stream};

/// A PAT with one program in it.
pub fn build_pat(ts_id: u16, version: u8, program: u16, pmt_pid: u16) -> Vec<u8> {
    let mut loop_bytes = program.to_be_bytes().to_vec();
    loop_bytes.extend_from_slice(&(0xE000 | pmt_pid).to_be_bytes());
    long_section(0x00, ts_id, version, &loop_bytes)
}

/// A PMT listing only `streams`.
pub fn build_pmt(p: &Program, version: u8, streams: &[&Stream]) -> Vec<u8> {
    let mut b = (0xE000 | p.pcr_pid).to_be_bytes().to_vec();
    b.extend_from_slice(&(0xF000 | p.info.len() as u16).to_be_bytes());
    b.extend_from_slice(&p.info);
    for s in streams {
        b.push(s.stream_type);
        b.extend_from_slice(&(0xE000 | s.pid).to_be_bytes());
        b.extend_from_slice(&(0xF000 | s.info.len() as u16).to_be_bytes());
        b.extend_from_slice(&s.info);
    }
    long_section(0x02, p.number, version, &b)
}

fn long_section(table: u8, id: u16, version: u8, loop_bytes: &[u8]) -> Vec<u8> {
    let len = 5 + loop_bytes.len() + 4;
    let mut s = vec![table, 0xB0 | ((len >> 8) as u8 & 0x0F), len as u8];
    s.extend_from_slice(&id.to_be_bytes());
    s.push(0xC1 | ((version & 0x1F) << 1));
    s.extend_from_slice(&[0, 0]);
    s.extend_from_slice(loop_bytes);
    let crc = crate::ts::crc32(&s);
    s.extend_from_slice(&crc.to_be_bytes());
    s
}
