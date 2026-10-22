//! The numbers a native input reports, filled the way the GStreamer path
//! fills them (`runner::watch` and `Loss::fill`): the meter's rates and
//! codecs, then what the transport knows over them.

use std::sync::atomic::Ordering::Relaxed;
use std::time::Instant;

use serde_json::{json, Value};

use super::group::{Group, Subscription};
use super::program::Demux;
use super::tracks;
use crate::direct::input::outlet::Shared;
use crate::direct::input::stats::{InputStats, ProgramInfo, State};

/// A frame within this long, and the input is live, as `runner` has it.
const LIVE_MS: u64 = 3_000;

pub struct Report {
    pub stats: InputStats,
    /// The program's byte count and when it was read, for its rate.
    rate: (u64, Instant),
    /// The group's RTP loss when this show joined it.
    rtp_before: u64,
    /// Scrambled packets at the last report.
    scrambled: u64,
}

impl Report {
    pub fn new(program: Option<u16>, sub: &Subscription) -> Report {
        let stats = InputStats { program, ..InputStats::default() };
        Report { stats, rate: (0, Instant::now()), rtp_before: sub.group.counters.rtp_lost.load(Relaxed), scrambled: 0 }
    }

    /// Fill and hand on this second's numbers.
    pub fn publish(&mut self, out: &Shared, demux: &Demux, sub: &Subscription) {
        let g = &sub.group;
        let notes = demux.notes();
        let scrambled = demux.counts.scrambled.saturating_sub(std::mem::replace(&mut self.scrambled, demux.counts.scrambled));
        let mut said: Vec<String> = (!notes.is_empty()).then_some(notes).into_iter().collect();
        if scrambled > 0 {
            said.push(format!("{scrambled} scrambled packets in the last second, which cannot be read without the key"));
        }
        let said = said.join("; ");
        let s = &mut self.stats;
        (s.state, s.error) = match (g.error(), out.quiet_ms()) {
            (Some(e), _) => (State::Retrying, Some(e)),
            (None, Some(q)) if q < LIVE_MS => (State::Live, (!said.is_empty()).then(|| format!("left out: {said}"))),
            (None, Some(q)) => (State::Retrying, Some(format!("nothing from {} for {} s", g.address, q / 1000))),
            (None, None) => (State::Connecting, (!said.is_empty()).then(|| format!("{} carries {said}", g.address))),
        };
        let kbps = self.kbps(demux.counts.bytes);
        let rtp = g.counters.rtp_lost.load(Relaxed).saturating_sub(self.rtp_before);
        let lost = if rtp > 0 { rtp } else { demux.counts.ts_lost } + sub.dropped.load(Relaxed);
        let programs = programs(g);
        out.publish_with(&mut self.stats, |s| {
            if let Some(k) = kbps {
                s.kbps = k;
            }
            (s.cc_errors, s.packets_lost) = (demux.counts.cc_errors, lost);
            s.program = demux.program().or(s.program);
            if let Some(p) = programs.filter(|p| !p.is_empty()) {
                s.programs = p;
            }
            s.tracks = tracks::infos(&demux.picks);
        });
    }

    /// The program's rate since the last call, stuffing left out.
    fn kbps(&mut self, bytes: u64) -> Option<u32> {
        let secs = self.rate.1.elapsed().as_secs_f64();
        if secs < 0.5 {
            return None;
        }
        let kbps = (bytes.saturating_sub(self.rate.0) as f64 * 8.0 / 1000.0 / secs).round() as u32;
        self.rate = (bytes, Instant::now());
        Some(kbps)
    }
}

/// Every program the group's tables list, when the lock is free.
fn programs(g: &Group) -> Option<Vec<ProgramInfo>> {
    let c = g.catalog.try_lock().ok()?;
    let one = |p: &gmx_udp::ts::tables::Program| ProgramInfo {
        number: p.number,
        name: p.name.clone(),
        provider: p.provider.clone(),
        streams: p.streams.iter().map(|s| s.kind().to_string()).collect(),
    };
    Some(c.programs.iter().map(one).collect())
}

/// One group's row for the wall: a dead group is one fault, not one per
/// program on it.
pub fn group_json(g: &Group) -> Value {
    let n = &g.counters;
    json!({
        "address": g.address,
        "interface": g.key.interface,
        "shows": g.subs().list.len(),
        "datagrams": n.datagrams.load(Relaxed),
        "bytes_in": n.bytes_in.load(Relaxed),
        "cc_errors": n.cc_errors.load(Relaxed),
        "ts_packets_lost": n.ts_lost.load(Relaxed),
        "rtp_packets_lost": n.rtp_lost.load(Relaxed),
        "null_packets": n.nulls.load(Relaxed),
        "silent_ms": n.silent_ms(),
        "receive_buffer": g.buffer.load(Relaxed),
        "error": g.error(),
    })
}
