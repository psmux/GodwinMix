//! The start of a PES packet: its PTS and DTS, and whether it opens a picture
//! a decoder can start on.

use super::{payload_at, unit_start};

pub const PTS_WRAP: u64 = 1u64 << 33;

/// Where the PTS (and the DTS after it) sit in the packet, when this packet
/// starts a PES with a time stamp.
fn stamps(p: &[u8]) -> Option<(usize, u8)> {
    if !unit_start(p) {
        return None;
    }
    let at = payload_at(p)?;
    let s = &p[at..];
    if s.len() < 19 || s[0..3] != [0, 0, 1] {
        return None;
    }
    // Streams with no optional header: padding, private 2, ECM, EMM, DSM-CC, H.222.1 type E.
    if matches!(s[3], 0xBC | 0xBE | 0xBF | 0xF0 | 0xF1 | 0xF2 | 0xF8 | 0xFF) {
        return None;
    }
    let which = s[7] >> 6;
    (which & 2 != 0).then_some((at + 9, which))
}

fn read(b: &[u8]) -> u64 {
    (u64::from((b[0] >> 1) & 7) << 30) | (u64::from(b[1]) << 22) | (u64::from(b[2] >> 1) << 15) | (u64::from(b[3]) << 7) | u64::from(b[4] >> 1)
}

fn write(b: &mut [u8], v: u64) {
    let v = v % PTS_WRAP;
    b[0] = (b[0] & 0xF1) | (((v >> 29) as u8) & 0x0E);
    b[1] = (v >> 22) as u8;
    b[2] = (((v >> 14) as u8) & 0xFE) | 1;
    b[3] = (v >> 7) as u8;
    b[4] = (((v << 1) as u8) & 0xFE) | 1;
}

pub fn pts(p: &[u8]) -> Option<u64> {
    stamps(p).map(|(at, _)| read(&p[at..at + 5]))
}

/// Moves the PTS and DTS of a PES that starts in this packet by `by` ticks of 90 kHz.
pub fn shift(p: &mut [u8], by: u64) {
    let Some((at, which)) = stamps(p) else { return };
    let v = read(&p[at..at + 5]);
    write(&mut p[at..at + 5], v + by);
    if which == 3 {
        let d = read(&p[at + 5..at + 10]);
        write(&mut p[at + 5..at + 10], d + by);
    }
}

/// True when the video PES starting here opens with an IDR or a parameter
/// set (H.264), or an IRAP or VPS (HEVC). Only the first packet is looked at,
/// which is where an encoder puts them.
pub fn keyframe(p: &[u8], hevc: bool) -> bool {
    let Some((at, _)) = stamps(p) else { return false };
    let header = usize::from(p[at - 1]);
    let es = &p[(at + header).min(p.len())..];
    es.windows(4).any(|w| {
        if w[0..3] != [0, 0, 1] {
            return false;
        }
        if hevc {
            matches!((w[3] >> 1) & 0x3F, 16..=21 | 32)
        } else {
            matches!(w[3] & 0x1F, 5 | 7)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A video PES start with a PTS and a DTS and an H.264 SPS after them.
    fn pes(pts: u64, dts: u64) -> [u8; 188] {
        let mut p = [0xFFu8; 188];
        p[..4].copy_from_slice(&[0x47, 0x41, 0x00, 0x10]);
        let mut h = vec![0, 0, 1, 0xE0, 0, 0, 0x80, 0xC0, 10, 0x31, 0, 1, 0, 1, 0x11, 0, 1, 0, 1];
        h.extend_from_slice(&[0, 0, 0, 1, 0x67, 0x64]);
        p[4..4 + h.len()].copy_from_slice(&h);
        write(&mut p[13..18], pts);
        write(&mut p[18..23], dts);
        p
    }

    #[test]
    fn shifts_pts_and_dts_across_the_wrap() {
        let mut p = pes(1000, 900);
        shift(&mut p, 90_000);
        assert_eq!(pts(&p), Some(91_000));
        assert_eq!(read(&p[18..23]), 90_900);
        let mut q = pes(PTS_WRAP - 10, PTS_WRAP - 20);
        shift(&mut q, 30);
        assert_eq!(pts(&q), Some(20));
        assert_eq!(p[13] & 0xF0, 0x30, "the PTS prefix is kept");
    }

    #[test]
    fn finds_a_parameter_set_as_a_keyframe() {
        assert!(keyframe(&pes(0, 0), false));
        let mut p = pes(0, 0);
        p[27] = 0x41;
        assert!(!keyframe(&p, false));
    }
}
