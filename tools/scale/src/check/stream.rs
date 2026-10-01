//! What one received stream looked like: packets, continuity, PCR, GOPs and
//! silences, counted as datagrams arrive and never waiting on anything.

use crate::ts::{self, pes, psi, PACKET};

#[derive(Default)]
pub struct Stream {
    pub datagrams: u64,
    pub packets: u64,
    pub bytes: u64,
    pub cc_errors: u64,
    pub cc_lost: u64,
    pub pcr_jumps: u64,
    pub pcr_gap_max_ms: f64,
    pub pcr_jitter: (f64, f64),
    pub keyframes: u64,
    pub key_gaps_ms: Vec<f64>,
    pub silence_max_ms: f64,
    pub first_ms: Option<f64>,
    pub last_ms: f64,
    cc: Vec<u8>,
    pmt_pid: Option<u16>,
    pcr_pid: Option<u16>,
    video: Option<(u16, bool)>,
    last_pcr: Option<u64>,
    last_key_pts: Option<u64>,
    pcr_base: Option<(u64, f64)>,
    jitter_done: f64,
}

impl Stream {
    pub fn new() -> Stream {
        Stream { cc: vec![0xFF; 8192], pcr_jitter: (f64::MAX, f64::MIN), ..Stream::default() }
    }

    /// One datagram that arrived `now_ms` after the check started.
    pub fn datagram(&mut self, d: &[u8], now_ms: f64) {
        if self.first_ms.is_some() {
            self.silence_max_ms = self.silence_max_ms.max(now_ms - self.last_ms);
        } else {
            self.first_ms = Some(now_ms);
        }
        self.last_ms = now_ms;
        self.datagrams += 1;
        self.bytes += d.len() as u64;
        let body = if d.first().is_some_and(|b| b & 0xC0 == 0x80) { d.get(12..).unwrap_or(&[]) } else { d };
        for p in body.chunks_exact(PACKET).filter(|p| p[0] == 0x47) {
            self.packet(p, now_ms);
        }
    }

    fn packet(&mut self, p: &[u8], now_ms: f64) {
        self.packets += 1;
        let pid = ts::pid(p);
        if pid == ts::NULL_PID {
            return;
        }
        self.continuity(p, pid);
        self.tables(p, pid);
        if Some(pid) == self.pcr_pid {
            if let Some(v) = ts::pcr(p) {
                self.clock(v, ts::discontinuity(p), now_ms);
            }
        }
        if let Some((vpid, hevc)) = self.video {
            if pid == vpid && pes::keyframe(p, hevc) {
                self.keyframe(pes::pts(p));
            }
        }
    }

    fn continuity(&mut self, p: &[u8], pid: u16) {
        let (cc, last) = (ts::cc(p), self.cc[usize::from(pid)]);
        let payload = ts::has_payload(p);
        if last != 0xFF && !ts::discontinuity(p) {
            let want = if payload { (last + 1) & 0x0F } else { last };
            let duplicate = payload && cc == last;
            if cc != want && !duplicate {
                self.cc_errors += 1;
                self.cc_lost += u64::from(cc.wrapping_sub(want) & 0x0F);
            }
        }
        self.cc[usize::from(pid)] = cc;
    }

    fn tables(&mut self, p: &[u8], pid: u16) {
        if pid == 0 && self.pmt_pid.is_none() {
            self.pmt_pid = psi::pat(p).and_then(|v| v.first().map(|e| e.1));
        } else if Some(pid) == self.pmt_pid && self.pcr_pid.is_none() {
            if let Some(m) = psi::pmt(p) {
                (self.pcr_pid, self.video) = (Some(m.pcr_pid), m.video);
            }
        }
    }

    fn clock(&mut self, pcr: u64, discontinuity: bool, now_ms: f64) {
        let mut restart = discontinuity;
        if let Some(last) = self.last_pcr.filter(|_| !discontinuity) {
            let gap = ts::pcr_delta(last, pcr) as f64 / 27_000.0;
            if gap > 100.0 {
                self.pcr_jumps += 1;
                restart = true;
            } else {
                self.pcr_gap_max_ms = self.pcr_gap_max_ms.max(gap);
            }
        }
        if restart {
            self.jitter_done = self.jitter_ms();
            (self.pcr_jitter, self.pcr_base) = ((f64::MAX, f64::MIN), None);
        }
        self.last_pcr = Some(pcr);
        // Arrival less PCR, unwrapped by the running total of PCR gaps: its
        // spread is how unevenly the stream arrived against its own clock.
        let offset = now_ms - self.pcr_ms(pcr);
        self.pcr_jitter = (self.pcr_jitter.0.min(offset), self.pcr_jitter.1.max(offset));
    }

    fn pcr_ms(&mut self, pcr: u64) -> f64 {
        let (prev, acc) = self.pcr_base.unwrap_or((pcr, 0.0));
        let acc = acc + ts::pcr_delta(prev, pcr) as f64 / 27_000.0;
        self.pcr_base = Some((pcr, acc));
        acc
    }

    fn keyframe(&mut self, pts: Option<u64>) {
        self.keyframes += 1;
        if let (Some(last), Some(now)) = (self.last_key_pts, pts) {
            let gap = ((now + pes::PTS_WRAP - last) % pes::PTS_WRAP) as f64 / 90.0;
            if gap < 60_000.0 {
                self.key_gaps_ms.push(gap);
            }
        }
        self.last_key_pts = pts.or(self.last_key_pts);
    }

    /// The widest spread of arrival against PCR in any stretch between jumps.
    pub fn jitter_ms(&self) -> f64 {
        let now = if self.pcr_jitter.1 < self.pcr_jitter.0 { 0.0 } else { self.pcr_jitter.1 - self.pcr_jitter.0 };
        now.max(self.jitter_done)
    }
}
