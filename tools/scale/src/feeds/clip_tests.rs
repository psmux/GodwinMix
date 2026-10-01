//! A synthetic clip, and the pacing worked out from it.

use super::*;
use crate::ts::psi::tests::{pat_packet, pmt_packet};

/// A constant rate clip: a PAT, a PMT, then `n` packets on PID 256, a PCR on
/// every tenth, one packet every `step` ticks of 27 MHz.
pub fn synthetic(n: usize, step: u64) -> Vec<u8> {
    let mut out = pat_packet(&[(1, 4096)]).to_vec();
    out.extend_from_slice(&pmt_packet(4096, 256, &[(0x1B, 256)]));
    for i in 2..n {
        let mut p = [0u8; PACKET];
        p[..4].copy_from_slice(&[0x47, 0x01, 0x00, 0x10]);
        if i % 10 == 0 {
            p[3] = 0x30;
            p[4] = 7;
            p[5] = 0x10;
            ts::set_pcr(&mut p, 27_000_000 + i as u64 * step);
        }
        out.extend_from_slice(&p);
    }
    out
}

#[test]
fn a_constant_rate_clip_is_paced_evenly() {
    let c = Clip::from_bytes("t.ts".into(), synthetic(700, 2700)).unwrap();
    assert_eq!(c.packets(), 700);
    assert_eq!(c.datagrams(), 100);
    assert_eq!(c.loop_27m, 700 * 2700);
    assert_eq!(c.at_ns[1], 7 * 2700 * 1000 / 27);
    assert_eq!(c.programs, vec![1]);
    assert_eq!(c.pids, vec![0, 4096, 256]);
    let kbps = 700.0 * 188.0 * 8.0 / 0.07 / 1000.0;
    assert!((c.kbps() - kbps).abs() < 1.0, "{} against {kbps}", c.kbps());
}

#[test]
fn refuses_what_it_cannot_pace() {
    assert!(Clip::from_bytes("x".into(), vec![0u8; 188]).is_err());
    assert!(Clip::from_bytes("x".into(), pat_packet(&[(1, 4096)]).to_vec()).is_err());
}
