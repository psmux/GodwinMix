//! One program out of a multiplex, demuxed: its PMT followed as it changes,
//! its clock, and each carried stream's PES packets handed to their parsers.
//!
//! The socket thread hands this only the program's own packets (`route`).
//! When the PMT moves (a new version, new PIDs, a stream added or gone), the
//! streams still listed keep their parsers and the new ones get their own,
//! so a sender that restarts with a new layout is followed without a gap.

use gmx_udp::ts::psi::Assembler;
use gmx_udp::ts::tables::parse_pmt;
use gmx_udp::ts::{header, NULL_PID, PACKET};

use super::clock::Clock;
use super::stream::{check, Check, Es, Out};
use super::tracks::{pick, Pick};
use super::Counts;
use crate::direct::input::spec::Carry;

pub struct Demux {
    carry: Carry,
    /// The program number and its PMT PID, once the socket thread says.
    follow: Option<(u16, u16)>,
    pmt: Assembler,
    pmt_cc: u8,
    pmt_crc: Option<u32>,
    pcr_pid: u16,
    pub es: Vec<Es>,
    pub picks: Vec<Pick>,
    clock: Clock,
    pub counts: Counts,
}

impl Demux {
    pub fn new(carry: Carry) -> Demux {
        Demux {
            carry,
            follow: None,
            pmt: Assembler::default(),
            pmt_cc: 16,
            pmt_crc: None,
            pcr_pid: NULL_PID,
            es: Vec::new(),
            picks: Vec::new(),
            clock: Clock::default(),
            counts: Counts::default(),
        }
    }

    pub fn program(&self) -> Option<u16> {
        self.follow.map(|f| f.0)
    }

    /// Follow `number`, whose PMT is on `pmt_pid`. The clock carries on, so
    /// a sender that came back as another program keeps the timeline.
    pub fn follow(&mut self, number: u16, pmt_pid: u16) {
        if self.follow != Some((number, pmt_pid)) {
            self.follow = Some((number, pmt_pid));
            (self.pmt, self.pmt_cc, self.pmt_crc) = (Assembler::default(), 16, None);
            self.es.clear();
            self.picks.clear();
        }
    }

    /// The feed was quiet long enough that its sender may have restarted.
    pub fn resumed(&mut self) {
        self.pmt_cc = 16;
        self.pmt.reset();
        self.es.iter_mut().for_each(Es::resumed);
    }

    /// The program's packets, as the socket thread routed them.
    pub fn feed(&mut self, packets: &[u8], out: &mut Vec<Out>) {
        let Some((_, pmt_pid)) = self.follow else { return };
        for p in packets.chunks_exact(PACKET) {
            let Some(h) = header(p) else { continue };
            self.counts.bytes += PACKET as u64;
            if h.pid == self.pcr_pid {
                self.pcr(p);
            }
            if h.pid == pmt_pid {
                self.pmt_packet(&h, p);
            } else if let Some(es) = self.es.iter_mut().find(|e| e.pid == h.pid) {
                es.packet(&h, p, &mut self.clock, self.pcr_pid == NULL_PID, &mut self.counts, out);
            }
        }
    }

    /// A PCR in the adaptation field (ISO 13818-1, 2.4.3.5).
    fn pcr(&mut self, p: &[u8]) {
        if p[3] & 0x20 == 0 || p[4] < 7 || p[5] & 0x10 == 0 {
            return;
        }
        let b = &p[6..12];
        let base = u64::from(b[0]) << 25 | u64::from(b[1]) << 17 | u64::from(b[2]) << 9 | u64::from(b[3]) << 1 | u64::from(b[4] >> 7);
        let ext = (u16::from(b[4] & 1) << 8) | u16::from(b[5]);
        self.clock.pcr(base, ext, p[5] & 0x80 != 0);
    }

    fn pmt_packet(&mut self, h: &gmx_udp::ts::Header, p: &[u8]) {
        if !h.has_payload {
            return;
        }
        if let Check::Lost(n) = check(&mut self.pmt_cc, h) {
            self.counts.cc_errors += 1;
            self.counts.ts_lost += u64::from(n);
            self.pmt.reset();
        }
        let mut sections = Vec::new();
        self.pmt.push(h.start, &p[h.payload_at..], |s| sections.push(s.to_vec()));
        for s in sections.iter().filter(|s| s[0] == 0x02) {
            self.on_pmt(s);
        }
    }

    fn on_pmt(&mut self, s: &[u8]) {
        let crc = u32::from_be_bytes([s[s.len() - 4], s[s.len() - 3], s[s.len() - 2], s[s.len() - 1]]);
        if self.pmt_crc == Some(crc) || self.follow.is_some_and(|f| u16::from_be_bytes([s[3], s[4]]) != f.0) {
            return;
        }
        let Some((pcr, _, streams)) = parse_pmt(s) else { return };
        self.pmt_crc = Some(crc);
        self.pcr_pid = pcr;
        self.picks = pick(&streams, self.carry);
        let mut old = std::mem::take(&mut self.es);
        for p in &self.picks {
            let Some((codec, track)) = p.carried else { continue };
            let (pid, kind) = (p.stream.pid, p.stream.stream_type);
            let mut es = match old.iter().position(|e| e.pid == pid && e.stream_type == kind && e.codec == codec) {
                Some(at) => old.swap_remove(at),
                None => Es::new(pid, kind, codec, track),
            };
            es.track = track;
            self.es.push(es);
        }
    }

    /// Everything left out right now, joined for a message.
    pub fn notes(&self) -> String {
        self.picks.iter().filter_map(|p| p.note.clone()).collect::<Vec<_>>().join("; ")
    }
}
