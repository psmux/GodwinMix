//! MPEG-TS packet fields: what the sender rewrites and the checker reads.

pub mod pes;
pub mod psi;

pub const PACKET: usize = 188;
pub const NULL_PID: u16 = 0x1FFF;
/// PCR runs on 27 MHz with a 33 bit base, so it wraps at 2^33 * 300.
pub const PCR_WRAP: u64 = (1u64 << 33) * 300;

pub fn pid(p: &[u8]) -> u16 {
    (u16::from(p[1] & 0x1F) << 8) | u16::from(p[2])
}

pub fn unit_start(p: &[u8]) -> bool {
    p[1] & 0x40 != 0
}

pub fn has_payload(p: &[u8]) -> bool {
    p[3] & 0x10 != 0
}

pub fn cc(p: &[u8]) -> u8 {
    p[3] & 0x0F
}

pub fn set_cc(p: &mut [u8], cc: u8) {
    p[3] = (p[3] & 0xF0) | (cc & 0x0F);
}

/// The adaptation field's flags byte, when there is one with any length.
fn flags(p: &[u8]) -> Option<u8> {
    (p[3] & 0x20 != 0 && p[4] > 0).then(|| p[5])
}

pub fn discontinuity(p: &[u8]) -> bool {
    flags(p).is_some_and(|f| f & 0x80 != 0)
}

pub fn pcr(p: &[u8]) -> Option<u64> {
    let f = flags(p)?;
    if f & 0x10 == 0 || p[4] < 7 {
        return None;
    }
    let a = &p[6..12];
    let base = (u64::from(a[0]) << 25) | (u64::from(a[1]) << 17) | (u64::from(a[2]) << 9) | (u64::from(a[3]) << 1) | (u64::from(a[4]) >> 7);
    let ext = (u64::from(a[4] & 1) << 8) | u64::from(a[5]);
    Some(base * 300 + ext)
}

pub fn set_pcr(p: &mut [u8], value: u64) {
    let v = value % PCR_WRAP;
    let (base, ext) = (v / 300, v % 300);
    let a = &mut p[6..12];
    a[0] = (base >> 25) as u8;
    a[1] = (base >> 17) as u8;
    a[2] = (base >> 9) as u8;
    a[3] = (base >> 1) as u8;
    a[4] = (((base & 1) as u8) << 7) | 0x7E | ((ext >> 8) as u8 & 1);
    a[5] = ext as u8;
}

/// Where the payload starts, or None when the packet carries none.
pub fn payload_at(p: &[u8]) -> Option<usize> {
    if !has_payload(p) {
        return None;
    }
    let at = if p[3] & 0x20 != 0 { 5 + usize::from(p[4]) } else { 4 };
    (at < PACKET).then_some(at)
}

/// How far `now` is past `before` on the PCR clock, across a wrap.
pub fn pcr_delta(before: u64, now: u64) -> u64 {
    (now + PCR_WRAP - before) % PCR_WRAP
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn packet_with_pcr(pid: u16, pcr: u64) -> [u8; PACKET] {
        let mut p = [0xFFu8; PACKET];
        p[0] = 0x47;
        p[1] = (pid >> 8) as u8;
        p[2] = pid as u8;
        p[3] = 0x30;
        p[4] = 7;
        p[5] = 0x10;
        set_pcr(&mut p, pcr);
        p
    }

    #[test]
    fn pcr_round_trips_and_wraps() {
        for v in [0, 1, 299, 300, 27_000_000 * 3600 + 17, PCR_WRAP - 1] {
            let p = packet_with_pcr(256, v);
            assert_eq!(pcr(&p), Some(v));
            assert_eq!(pid(&p), 256);
        }
        let p = packet_with_pcr(256, PCR_WRAP + 5);
        assert_eq!(pcr(&p), Some(5));
        assert_eq!(pcr_delta(PCR_WRAP - 10, 20), 30);
    }

    #[test]
    fn continuity_counter_is_the_low_nibble() {
        let mut p = packet_with_pcr(256, 0);
        set_cc(&mut p, 0x1B);
        assert_eq!(cc(&p), 0xB);
        assert_eq!(p[3] & 0xF0, 0x30);
        assert_eq!(payload_at(&p), Some(12));
    }
}
