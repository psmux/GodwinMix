//! What the filter does with a whole PAT, PMT or SDT: keep the catalog of
//! programs up to date, and replan when it moves.

use std::collections::HashSet;

use super::super::tables::{self, Program};
use super::super::plan::Plan;
use super::super::{packetize, PAT_PID, SDT_PID};
use super::Filter;

impl Filter {
    pub(super) fn section(&mut self, pid: u16, s: &[u8]) {
        let crc = u32::from_be_bytes([s[s.len() - 4], s[s.len() - 3], s[s.len() - 2], s[s.len() - 1]]);
        if self.seen.insert(pid, crc) == Some(crc) {
            return;
        }
        match (pid, s[0]) {
            (PAT_PID, 0x00) => self.on_pat(s),
            (SDT_PID, 0x42) => self.on_sdt(s),
            (_, 0x02) => self.on_pmt(pid, s),
            _ => return,
        }
        self.changed = true;
        self.replan();
    }

    fn on_pat(&mut self, s: &[u8]) {
        self.ts_id = u16::from_be_bytes([s[3], s[4]]);
        let old = std::mem::take(&mut self.programs);
        for (number, pmt_pid) in tables::parse_pat(s) {
            let kept = old.iter().find(|p| p.number == number && p.pmt_pid == pmt_pid);
            self.programs.push(kept.cloned().unwrap_or(Program { number, pmt_pid, ..Default::default() }));
            self.assemblers.entry(pmt_pid).or_default();
        }
        self.name_programs();
        let live: HashSet<u16> = self.programs.iter().map(|p| p.pmt_pid).collect();
        self.parsed.retain(|pid| live.contains(pid));
    }

    fn on_pmt(&mut self, pid: u16, s: &[u8]) {
        let number = u16::from_be_bytes([s[3], s[4]]);
        let Some(p) = self.programs.iter_mut().find(|p| p.number == number && p.pmt_pid == pid) else {
            return;
        };
        if let Some((pcr, info, streams)) = tables::parse_pmt(s) {
            (p.pcr_pid, p.info, p.streams) = (pcr, info, streams);
            self.parsed.insert(pid);
        }
    }

    fn on_sdt(&mut self, s: &[u8]) {
        for (id, provider, name) in tables::parse_sdt(s) {
            self.names.insert(id, (provider, name));
        }
        self.name_programs();
    }

    fn name_programs(&mut self) {
        for p in &mut self.programs {
            if let Some((provider, name)) = self.names.get(&p.number) {
                (p.provider, p.name) = (provider.clone(), name.clone());
            }
        }
    }

    /// The tables the core sees when one program is chosen out of several: a
    /// PAT naming only it, and a PMT naming only the chosen streams.
    pub(super) fn inject(&mut self, pid: u16, section: &[u8], out: &mut Vec<u8>) -> bool {
        let Plan::Only(sel) = &self.plan else { return false };
        if pid == PAT_PID && section[0] == 0x00 {
            let pat = tables::build_pat(self.ts_id, self.version, sel.program, sel.pmt_pid);
            packetize(PAT_PID, &pat, &mut self.pat_cc, out);
            return true;
        }
        let (Some(keep), true) = (&sel.rewrite, pid == sel.pmt_pid && section[0] == 0x02) else {
            return false;
        };
        let Some(p) = self.programs.iter().find(|p| p.number == sel.program) else { return false };
        let streams: Vec<_> = p.streams.iter().filter(|s| keep.contains(&s.pid)).collect();
        let pmt = tables::build_pmt(p, self.version, &streams);
        packetize(pid, &pmt, &mut self.pmt_cc, out);
        true
    }
}
