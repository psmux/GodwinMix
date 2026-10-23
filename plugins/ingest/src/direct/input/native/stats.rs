//! The numbers a native input reports, filled the way the GStreamer path
//! fills them (`runner::watch` and `Loss::fill`): the meter's rates and
//! codecs, then what the transport knows over them.

use std::sync::atomic::Ordering::Relaxed;
use std::time::Instant;

use serde_json::{json, Value};

use gmx_udp::ts::plan::{plan, Choice, Plan};

use super::group::{Group, Subscription};
use super::program::Demux;
use super::tracks;
use crate::direct::input::outlet::Shared;
use crate::direct::input::stats::{InputStats, ProgramInfo, State};

/// A frame within this long, and the input is live, as `runner` has it.
const LIVE_MS: u64 = 3_000;

pub struct Report {
    pub stats: InputStats,
    /// The program asked for, if one was.
    asked: Option<u16>,
    /// The program's byte count and when it was read, for its rate.
    rate: (u64, Instant),
    /// The group's RTP loss when this show joined it.
    rtp_before: u64,
    /// Scrambled packets at the last report.
    scrambled: u64,
    /// The video's tags and fields at the last report, and whether the
    /// last second was coded as fields.
    pictures: (u64, u64),
    fields: bool,
}

impl Report {
    pub fn new(program: Option<u16>, sub: &Subscription) -> Report {
        let stats = InputStats { program, ..InputStats::default() };
        Report { stats, asked: program, rate: (0, Instant::now()), rtp_before: sub.group.counters.rtp_lost.load(Relaxed), scrambled: 0, pictures: (0, 0), fields: false }
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
        let missing = self.asked.and_then(|n| missing(g, n));
        let s = &mut self.stats;
        (s.state, s.error) = match (g.error().or(missing), out.quiet_ms()) {
            (Some(e), None) => (State::Connecting, Some(e)),
            (Some(e), _) => (State::Retrying, Some(e)),
            (None, Some(q)) if q < LIVE_MS => (State::Live, (!said.is_empty()).then(|| format!("left out: {said}"))),
            (None, Some(q)) => (State::Retrying, Some(format!("nothing from {} for {} s", g.address, q / 1000))),
            (None, None) => (State::Connecting, (!said.is_empty()).then(|| format!("{} carries {said}", g.address))),
        };
        let kbps = self.kbps(demux.counts.bytes);
        let rtp = g.counters.rtp_lost.load(Relaxed).saturating_sub(self.rtp_before);
        let lost = if rtp > 0 { rtp } else { demux.counts.ts_lost } + sub.dropped.load(Relaxed);
        let programs = programs(g);
        let frames = self.frames(demux.pictures());
        let fields = self.fields;
        out.publish_with(&mut self.stats, |s| {
            // The meter counts tags, and a field coded stream has a tag per
            // field: the planner keeps the rate it reads, so it must read
            // frames.
            s.fps = (s.fps * frames * 100.0).round() / 100.0;
            if let Some(k) = kbps {
                s.kbps = k;
            }
            (s.cc_errors, s.packets_lost) = (demux.counts.cc_errors, lost);
            s.program = demux.program().or(s.program);
            if let Some(p) = programs.filter(|p| !p.is_empty()) {
                s.programs = p;
            }
            s.tracks = tracks::infos(&demux.picks);
            s.tracks.iter_mut().filter(|t| t.kind == "video").for_each(|t| t.field_coded = fields);
        });
    }

    /// Frames per video tag since the last call: 1 for frames, 0.5 for a
    /// stream of fields, between for one that mixes them.
    fn frames(&mut self, now: (u64, u64)) -> f64 {
        let (tags, fields) = (now.0.saturating_sub(self.pictures.0), now.1.saturating_sub(self.pictures.1));
        self.pictures = now;
        if tags == 0 {
            return if self.fields { 0.5 } else { 1.0 };
        }
        self.fields = fields * 2 > tags;
        1.0 - fields as f64 / tags as f64 / 2.0
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

/// The group's catalog, waited for. Every show on a group reports in the
/// same instant, woken by the same batch, so a `try_lock` here lost to the
/// show beside it every second and a show on a shared feed could go on
/// without its programs list. Nobody holds the lock for more than a copy;
/// the socket thread itself still only tries it.
fn catalog(g: &Group) -> std::sync::MutexGuard<'_, gmx_udp::counters::Catalog> {
    g.catalog.lock().unwrap_or_else(|e| e.into_inner())
}

/// Every program the group's tables list, once they have been read.
fn programs(g: &Group) -> Option<Vec<ProgramInfo>> {
    let c = catalog(g);
    let one = |p: &gmx_udp::ts::tables::Program| ProgramInfo {
        number: p.number,
        name: p.name.clone(),
        provider: p.provider.clone(),
        streams: p.streams.iter().map(|s| s.kind().to_string()).collect(),
    };
    Some(c.programs.iter().map(one).collect())
}

/// Why `program` cannot be taken, when the tables say it is not there:
/// the udp plugin's own words, which list what the feed does carry.
fn missing(g: &Group, program: u16) -> Option<String> {
    let c = catalog(g);
    match plan(&Choice { program, pids: Vec::new() }, &c.programs, &|_| true) {
        Plan::Missing(why) => Some(why),
        _ => None,
    }
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

#[cfg(test)]
mod tests {
    use super::super::group::{Group, Key};
    use super::*;

    #[test]
    fn a_show_reads_the_programs_while_the_show_beside_it_holds_the_catalog() {
        let g = std::sync::Arc::new(Group::new(Key { host: "127.0.0.1".into(), port: 1, source: None, interface: String::new() }, "udp://127.0.0.1:1".into()));
        g.catalog.lock().unwrap().programs = vec![gmx_udp::ts::tables::Program { number: 2, name: "News".into(), ..Default::default() }];
        let (held, go) = std::sync::mpsc::channel();
        let other = g.clone();
        let beside = std::thread::spawn(move || {
            let _c = other.catalog.lock().unwrap();
            held.send(()).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(100));
        });
        go.recv().unwrap();
        let names: Vec<String> = programs(&g).unwrap_or_default().into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["News"]);
        beside.join().unwrap();
    }
}
