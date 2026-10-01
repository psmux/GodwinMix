//! One MPEG-TS file held in memory with its send schedule: when each
//! datagram leaves, from the PCR, and how long one pass of the loop lasts.

use crate::ts::{self, psi, PACKET};

/// Seven packets to a datagram, as every IP encoder sends.
pub const PER_DATAGRAM: usize = 7;

pub struct Clip {
    pub name: String,
    pub data: Vec<u8>,
    /// Per packet, its PID's index in `pids`, so a feed keeps its continuity
    /// counters in a small array.
    pub slot: Vec<u8>,
    pub pids: Vec<u16>,
    /// Per datagram, nanoseconds from the start of the loop.
    pub at_ns: Vec<u64>,
    pub loop_ns: u64,
    /// The loop on the PCR clock (27 MHz), a whole number of 90 kHz ticks.
    pub loop_27m: u64,
    pub programs: Vec<u16>,
}

impl Clip {
    pub fn load(path: &str) -> Result<Clip, String> {
        let data = std::fs::read(path).map_err(|e| format!("could not read {path}: {e}"))?;
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        Clip::from_bytes(name, data)
    }

    pub fn from_bytes(name: String, mut data: Vec<u8>) -> Result<Clip, String> {
        data.truncate(data.len() / PACKET * PACKET);
        if data.is_empty() || data.chunks_exact(PACKET).any(|p| p[0] != 0x47) {
            return Err(format!("{name} is not MPEG-TS in 188 byte packets from its first byte"));
        }
        let (slot, pids) = slots(&data)?;
        let (times, loop_27m) = schedule(&name, &data)?;
        let at_ns = times.iter().step_by(PER_DATAGRAM).map(|t| t * 1000 / 27).collect();
        let programs = data.chunks_exact(PACKET).find_map(psi::pat).map(|p| p.into_iter().map(|(n, _)| n).collect()).unwrap_or_default();
        Ok(Clip { name, data, slot, pids, at_ns, loop_ns: loop_27m * 1000 / 27, loop_27m, programs })
    }

    pub fn packets(&self) -> usize {
        self.data.len() / PACKET
    }

    pub fn datagrams(&self) -> usize {
        self.at_ns.len()
    }

    pub fn kbps(&self) -> f64 {
        self.data.len() as f64 * 8.0 / (self.loop_ns as f64 / 1e9) / 1000.0
    }
}

fn slots(data: &[u8]) -> Result<(Vec<u8>, Vec<u16>), String> {
    let mut pids: Vec<u16> = Vec::new();
    let mut slot = Vec::with_capacity(data.len() / PACKET);
    for p in data.chunks_exact(PACKET) {
        let pid = ts::pid(p);
        let i = match pids.iter().position(|&x| x == pid) {
            Some(i) => i,
            None => {
                pids.push(pid);
                pids.len() - 1
            }
        };
        slot.push(u8::try_from(i).map_err(|_| "a clip with more than 255 PIDs is not one this harness sends".to_string())?);
    }
    Ok((slot, pids))
}

/// Each packet's time on the PCR clock from the loop's start, interpolated
/// between the PCRs of the first PID that carries one, and the loop's length.
fn schedule(name: &str, data: &[u8]) -> Result<(Vec<u64>, u64), String> {
    let packets: Vec<&[u8]> = data.chunks_exact(PACKET).collect();
    let pcr_pid = packets.iter().find(|p| ts::pcr(p).is_some()).map(|p| ts::pid(p)).ok_or(format!("{name} carries no PCR"))?;
    let mut marks: Vec<(usize, u64)> = Vec::new();
    for (i, p) in packets.iter().enumerate().filter(|(_, p)| ts::pid(p) == pcr_pid) {
        if let Some(v) = ts::pcr(p) {
            let v = marks.last().map_or(v, |&(_, last)| last + ts::pcr_delta(last % ts::PCR_WRAP, v));
            marks.push((i, v));
        }
    }
    let (&(i0, p0), &(i1, p1)) = (marks.first().unwrap(), marks.last().unwrap());
    if i1 <= i0 || p1 <= p0 {
        return Err(format!("{name} has fewer than two PCRs, so its rate cannot be known"));
    }
    let per_packet = (p1 - p0) as f64 / (i1 - i0) as f64;
    let base = p0 as f64 - i0 as f64 * per_packet;
    let loop_end = p1 as f64 + (packets.len() - i1) as f64 * per_packet - base;
    let loop_27m = ((loop_end / 300.0).round() as u64).max(1) * 300;
    let mut times = Vec::with_capacity(packets.len());
    let mut m = 0;
    for i in 0..packets.len() {
        while m + 1 < marks.len() && marks[m + 1].0 <= i {
            m += 1;
        }
        let t = match marks.get(m + 1) {
            Some(&(j, pj)) if marks[m].0 <= i => {
                let (k, pk) = marks[m];
                pk as f64 + (i - k) as f64 * (pj - pk) as f64 / (j - k) as f64
            }
            _ if i < i0 => base + i as f64 * per_packet,
            _ => p1 as f64 + (i - i1) as f64 * per_packet,
        };
        times.push((t - base).max(0.0) as u64);
    }
    Ok((times, loop_27m))
}

#[cfg(test)]
#[path = "clip_tests.rs"]
pub mod tests;
