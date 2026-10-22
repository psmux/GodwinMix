//! The MPEG-TS segments one rendition keeps: each one `Bytes`, numbered and
//! timed as the CMAF picture's segment of the same moment, so the two
//! playlists describe the same segments.

use super::listing::Entry;
use bytes::Bytes;
use crate::hls::track::TrackInfo;
use parking_lot::{Mutex, RwLock};
use std::collections::VecDeque;

struct Segment {
    msn: u64,
    disc: u32,
    pdt_ms: i64,
    start_ns: u64,
    duration_ns: u64,
    bytes: Bytes,
}

#[derive(Default)]
struct Ring {
    capacity: usize,
    segments: VecDeque<Segment>,
    longest_ns: u64,
    version: u64,
}

/// One TS rendition: `main` (the picture and the first sound) or a further
/// sound alone, `audio1`, `audio2`.
pub struct TsTrack {
    pub id: String,
    /// The sound track it carries alone, or None for `main`.
    pub sound: Option<u8>,
    /// What its sound is: the first track's for `main`. Set by the packager
    /// from the caps, since a TS only output packages no CMAF sound.
    pub sound_info: RwLock<TrackInfo>,
    ring: Mutex<Ring>,
    /// The media playlist's listing, kept until the ring moves.
    listed: Mutex<Option<(u64, Vec<Entry>, u64)>>,
}

impl TsTrack {
    pub fn new(id: &str, sound: Option<u8>, capacity: usize) -> TsTrack {
        let ring = Ring { capacity: capacity.max(3), ..Ring::default() };
        TsTrack { id: id.to_string(), sound, sound_info: RwLock::default(), ring: Mutex::new(ring), listed: Mutex::new(None) }
    }

    /// A whole segment. One whose number is at or past this one's goes
    /// first: the picture's ring numbered again after a break.
    pub fn push(&self, msn: u64, disc: u32, pdt_ms: i64, start_ns: u64, duration_ns: u64, bytes: Bytes) {
        let mut r = self.ring.lock();
        while r.segments.back().is_some_and(|s| s.msn >= msn) {
            r.segments.pop_back();
        }
        r.segments.push_back(Segment { msn, disc, pdt_ms, start_ns, duration_ns, bytes });
        while r.segments.len() > r.capacity {
            r.segments.pop_front();
        }
        r.longest_ns = r.longest_ns.max(duration_ns);
        r.version += 1;
    }

    /// The segments held, oldest first, and the longest there has been.
    pub fn listing(&self) -> (Vec<Entry>, u64) {
        let r = self.ring.lock();
        let mut kept = self.listed.lock();
        if let Some((_, entries, longest)) = kept.as_ref().filter(|k| k.0 == r.version) {
            return (entries.clone(), *longest);
        }
        let entries: Vec<Entry> = r
            .segments
            .iter()
            .map(|s| Entry {
                msn: s.msn,
                disc: s.disc,
                start_ms: s.pdt_ms,
                duration_ms: (s.duration_ns / 1_000_000) as u32,
                bytes: s.bytes.len(),
            })
            .collect();
        *kept = Some((r.version, entries.clone(), r.longest_ns));
        (entries, r.longest_ns)
    }

    /// A segment by its number.
    pub fn segment(&self, msn: u64) -> Option<Bytes> {
        self.ring.lock().segments.iter().find(|s| s.msn == msn).map(|s| s.bytes.clone())
    }

    /// A segment by the wall clock its name was made from: its start in
    /// ms, or its start's second and its length in ms.
    pub fn segment_at(&self, at: super::At) -> Option<Bytes> {
        let r = self.ring.lock();
        let found = r.segments.iter().find(|s| match at {
            super::At::Msn(m) => s.msn == m,
            super::At::Start(ms) => s.pdt_ms == ms,
            super::At::Clock { unix_s, duration_ms } => s.pdt_ms.div_euclid(1000) == unix_s && (s.duration_ns / 1_000_000) as u32 == duration_ms,
        });
        found.map(|s| s.bytes.clone())
    }

    /// The peak and average bit/s of the segments held.
    pub fn measured_bps(&self) -> Option<(u64, u64)> {
        let r = self.ring.lock();
        let (mut peak, mut bytes, mut ns) = (0u64, 0u64, 0u64);
        for s in r.segments.iter().filter(|s| s.duration_ns > 0) {
            peak = peak.max(s.bytes.len() as u64 * 8_000_000_000 / s.duration_ns);
            bytes += s.bytes.len() as u64;
            ns += s.duration_ns;
        }
        (ns > 0).then(|| (peak, bytes * 8_000_000_000 / ns))
    }

    /// Where the newest segment starts in running time, for a test.
    pub fn newest_start_ns(&self) -> Option<u64> {
        self.ring.lock().segments.back().map(|s| s.start_ns)
    }

    pub fn memory(&self) -> usize {
        self.ring.lock().segments.iter().map(|s| s.bytes.len()).sum()
    }
}
