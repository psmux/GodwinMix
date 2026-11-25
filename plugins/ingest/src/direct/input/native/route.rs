//! Which show gets which packet: per PID, the subscribers whose program
//! lists it. Built again only when the tables move or a show comes or goes,
//! so routing a packet is one lookup by PID.

use gmx_udp::ts::tables::Program;
use gmx_udp::ts::NULL_PID;

use super::group::{Item, Sub};

pub struct Routes {
    by_pid: Vec<Vec<u16>>,
}

impl Default for Routes {
    fn default() -> Routes {
        Routes { by_pid: vec![Vec::new(); 8192] }
    }
}

impl Routes {
    /// The subscribers that want `pid`, by their place in the list.
    pub fn of(&self, pid: u16) -> &[u16] {
        &self.by_pid[usize::from(pid & 0x1FFF)]
    }

    /// Route each subscriber's program: its PMT, its PCR and every stream
    /// it lists. A subscriber whose program changed is told first. Answers
    /// true while some subscriber could not be told yet (its queue was full).
    pub fn build(&mut self, programs: &[Program], subs: &mut [Sub]) -> bool {
        let mut pending = false;
        self.by_pid.iter_mut().for_each(Vec::clear);
        for (i, s) in subs.iter_mut().enumerate() {
            let chosen = match s.program {
                Some(n) => programs.iter().find(|p| p.number == n),
                None => programs.first(),
            };
            let Some(p) = chosen else { continue };
            // Marked routed only once the show has the message: a show whose
            // queue is full at that moment is told on the next build instead
            // of never, which left it with its packets and no PMT to read them.
            if s.routed != Some((p.number, p.pmt_pid)) && s.tx.try_send(Item::Follow(p.number, p.pmt_pid)).is_ok() {
                s.routed = Some((p.number, p.pmt_pid));
            }
            pending |= s.routed != Some((p.number, p.pmt_pid));
            let mut pids = vec![p.pmt_pid, p.pcr_pid];
            pids.extend(p.streams.iter().map(|e| e.pid));
            pids.sort_unstable();
            pids.dedup();
            for pid in pids.into_iter().filter(|&pid| pid != NULL_PID) {
                self.by_pid[usize::from(pid & 0x1FFF)].push(i as u16);
            }
        }
        pending
    }
}
