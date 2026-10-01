//! The per packet work: drop stuffing, count what never arrived, keep the
//! chosen program and nothing else.
//!
//! Runs on the streaming thread, once per datagram, so it allocates nothing
//! per media packet, takes no lock and never waits. A table is parsed only when its
//! CRC differs from the last one on that PID, which on a steady feed is never.

use std::collections::{HashMap, HashSet};

use super::plan::{plan, Choice, Plan};
use super::psi::Assembler;
use super::tables::Program;
use super::{header, Header, NULL_PID, PACKET, PAT_PID, SDT_PID};
use crate::counters::Counters;

#[path = "filter_tables.rs"]
mod tables_in;

pub struct Filter {
    choice: Choice,
    /// The last continuity counter per PID, 16 for never seen.
    cc: Vec<u8>,
    assemblers: HashMap<u16, Assembler>,
    /// The CRC of the last section parsed on each PID.
    seen: HashMap<u16, u32>,
    programs: Vec<Program>,
    /// Service names by program number, kept apart because the SDT may
    /// arrive before the PAT has said which programs there are.
    names: HashMap<u16, (String, String)>,
    parsed: HashSet<u16>,
    ts_id: u16,
    plan: Plan,
    version: u8,
    pat_cc: u8,
    pmt_cc: u8,
    /// The tables or the plan moved, and the catalog wants republishing.
    pub changed: bool,
}

impl Filter {
    pub fn new(choice: Choice) -> Filter {
        let assemblers = [(PAT_PID, Assembler::default()), (SDT_PID, Assembler::default())];
        Filter {
            choice,
            cc: vec![16; 8192],
            assemblers: assemblers.into_iter().collect(),
            seen: HashMap::new(),
            programs: Vec::new(),
            names: HashMap::new(),
            parsed: HashSet::new(),
            ts_id: 0,
            plan: Plan::Waiting,
            version: 0,
            pat_cc: 0,
            pmt_cc: 0,
            changed: false,
        }
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    pub fn programs(&self) -> &[Program] {
        &self.programs
    }

    /// The feed was silent long enough that the sender may have restarted.
    /// Forget the counters, so its fresh ones are not counted as loss.
    pub fn resumed(&mut self) {
        self.cc.fill(16);
        self.assemblers.values_mut().for_each(Assembler::reset);
    }

    /// Filter one datagram's worth of packets into `out`. Answers false when
    /// `out` is exactly `packets`, so the caller can pass the original on.
    pub fn feed(&mut self, packets: &[u8], n: &Counters, out: &mut Vec<u8>) -> bool {
        let mut modified = false;
        for p in packets.chunks(PACKET) {
            let Some(h) = header(p) else {
                Counters::add(&n.malformed, 1);
                modified = true;
                continue;
            };
            if h.pid == NULL_PID {
                Counters::add(&n.nulls, 1);
                modified = true;
                continue;
            }
            Counters::add(&n.flagged, u64::from(h.transport_error));
            self.count(&h, n);
            let injected = self.tables(&h, &p[h.payload_at..], out);
            if self.forward(h.pid) {
                out.extend_from_slice(p);
            } else {
                modified = true;
            }
            modified |= injected;
        }
        modified
    }

    fn count(&mut self, h: &Header, n: &Counters) {
        let last = self.cc[h.pid as usize];
        if h.has_payload {
            self.cc[h.pid as usize] = h.cc;
        }
        if last == 16 || h.discontinuity || !h.has_payload {
            return;
        }
        let gap = h.cc.wrapping_sub(last) & 0x0F;
        if gap > 1 {
            Counters::add(&n.ts_lost, u64::from(gap - 1));
            Counters::add(&n.cc_errors, 1);
            if let Some(a) = self.assemblers.get_mut(&h.pid) {
                a.reset();
            }
        }
    }

    fn forward(&self, pid: u16) -> bool {
        match &self.plan {
            Plan::PassAll => true,
            Plan::Waiting | Plan::Missing(_) => false,
            Plan::Only(_) if pid == PAT_PID => false,
            Plan::Only(s) if pid == s.pmt_pid => s.rewrite.is_none(),
            Plan::Only(s) => s.keep.contains(&pid),
        }
    }

    /// Feed a table PID's payload to its assembler, act on each whole section,
    /// and put any rebuilt table into `out`. Answers whether it did.
    fn tables(&mut self, h: &Header, payload: &[u8], out: &mut Vec<u8>) -> bool {
        let Some(a) = self.assemblers.get_mut(&h.pid) else { return false };
        let mut whole = Vec::new();
        a.push(h.start, payload, |s| whole.push(s.to_vec()));
        let mut injected = false;
        for section in whole {
            self.section(h.pid, &section);
            injected |= self.inject(h.pid, &section, out);
        }
        injected
    }

    fn replan(&mut self) {
        let parsed = &self.parsed;
        let next = plan(&self.choice, &self.programs, &|pid| parsed.contains(&pid));
        if next != self.plan {
            self.version = (self.version + 1) & 0x1F;
            self.plan = next;
        }
    }

}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;
