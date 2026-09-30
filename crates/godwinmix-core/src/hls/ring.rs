//! The segments one rung keeps in memory.
//!
//! A ring of the last few segments, each a list of parts, each part one
//! `Bytes` that every viewer is handed a reference to. Nothing here knows
//! about GStreamer or HTTP, so the whole of it is tested with plain bytes.
//!
//! The first segment is numbered from its start time rather than from zero,
//! so a rung that joins a running ladder gives the same number to the same
//! two seconds as the rungs already there. That is what lets an LL-HLS
//! player switch rungs by asking for the same media sequence number on the
//! other one.

use bytes::Bytes;
use std::collections::VecDeque;

/// One LL-HLS part: a `moof` and its `mdat`, a third of a second or so.
#[derive(Debug, Clone)]
pub struct Part {
    pub bytes: Bytes,
    pub duration_ns: u64,
    /// Starts with a keyframe, so a player may begin here.
    pub independent: bool,
}

#[derive(Debug, Clone)]
pub struct Segment {
    /// Media sequence number.
    pub msn: u64,
    /// Wall clock at the first sample, milliseconds since 1970.
    pub pdt_ms: i64,
    /// Which init segment decodes it. Changes only when the muxer was
    /// rebuilt, and then the playlist says so with a discontinuity.
    pub init: u32,
    pub parts: Vec<Part>,
    pub complete: bool,
}

impl Segment {
    pub fn duration_ns(&self) -> u64 {
        self.parts.iter().map(|p| p.duration_ns).sum()
    }

    pub fn len(&self) -> usize {
        self.parts.iter().map(|p| p.bytes.len()).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

/// How far a rung has got, which is what an LL-HLS blocking reload waits on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Position {
    /// The newest segment that is whole.
    pub complete: Option<u64>,
    /// The segment being written and how many parts it has so far.
    pub open: Option<(u64, u32)>,
}

impl Position {
    /// Whether a playlist now carries segment `msn`, or part `part` of it when
    /// one is named: the condition `_HLS_msn` and `_HLS_part` wait for.
    pub fn reached(&self, msn: u64, part: Option<u32>) -> bool {
        if self.complete.is_some_and(|c| c >= msn) {
            return true;
        }
        match (part, self.open) {
            (Some(p), Some((open, parts))) => open > msn || (open == msn && parts > p),
            _ => false,
        }
    }

    /// The newest media sequence number a playlist lists, whole or not.
    pub fn newest(&self) -> Option<u64> {
        self.open.map(|(m, _)| m).or(self.complete)
    }
}

/// What a playlist is written from: every number, none of the bytes.
#[derive(Debug, Clone, Default)]
pub struct View {
    pub segments: Vec<SegmentView>,
    /// The longest whole segment this ring has ever held, which only grows,
    /// so `EXT-X-TARGETDURATION` never shrinks under a player.
    pub longest_ns: u64,
}

#[derive(Debug, Clone)]
pub struct SegmentView {
    pub msn: u64,
    pub pdt_ms: i64,
    pub init: u32,
    pub complete: bool,
    pub duration_ns: u64,
    pub bytes: usize,
    /// Duration and independence of each part.
    pub parts: Vec<(u64, bool)>,
}

pub struct Ring {
    capacity: usize,
    inits: Vec<(u32, Bytes)>,
    segments: VecDeque<Segment>,
    version: u64,
    longest_ns: u64,
}

impl Ring {
    pub fn new(capacity: usize) -> Ring {
        Ring { capacity: capacity.max(3), inits: Vec::new(), segments: VecDeque::new(), version: 0, longest_ns: 0 }
    }

    /// Bumped by every change, so a rendered playlist can be reused until
    /// something moves.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Take a new init segment. The same bytes again (a muxer that repeats its
    /// header) change nothing.
    pub fn set_init(&mut self, bytes: Bytes) -> u32 {
        if let Some((gen, current)) = self.inits.last() {
            if *current == bytes {
                return *gen;
            }
        }
        let gen = self.inits.last().map(|(g, _)| g + 1).unwrap_or(0);
        self.inits.push((gen, bytes));
        self.version += 1;
        gen
    }

    pub fn init(&self, gen: u32) -> Option<Bytes> {
        self.inits.iter().find(|(g, _)| *g == gen).map(|(_, b)| b.clone())
    }

    pub fn current_init(&self) -> Option<u32> {
        self.inits.last().map(|(g, _)| *g)
    }

    /// Start a segment whose first sample is at `start_ns` of running time,
    /// closing the one before it. Returns its media sequence number.
    pub fn begin(&mut self, start_ns: u64, segment_ns: u64, pdt_ms: i64) -> Option<u64> {
        let init = self.current_init()?;
        self.close();
        // A playlist numbers its segments one apart, so only the first is
        // placed by time; after that a late keyframe makes a long segment,
        // never a gap in the numbers.
        let msn = match self.segments.back() {
            Some(last) => last.msn + 1,
            None => (start_ns + segment_ns / 2) / segment_ns.max(1),
        };
        self.segments.push_back(Segment { msn, pdt_ms, init, parts: Vec::new(), complete: false });
        while self.segments.len() > self.capacity {
            self.segments.pop_front();
        }
        self.forget_unused_inits();
        self.version += 1;
        Some(msn)
    }

    /// Add a part to the open segment. False when no segment is open, which
    /// is a muxer that sent a continuation before its first fragment.
    pub fn push_part(&mut self, part: Part) -> bool {
        match self.segments.back_mut() {
            Some(open) if !open.complete => {
                open.parts.push(part);
                self.version += 1;
                true
            }
            _ => false,
        }
    }

    /// The open segment is whole.
    pub fn close(&mut self) {
        if let Some(open) = self.segments.back_mut().filter(|s| !s.complete) {
            open.complete = true;
            self.longest_ns = self.longest_ns.max(open.duration_ns());
            self.version += 1;
        }
    }

    pub fn position(&self) -> Position {
        let mut pos = Position::default();
        for s in self.segments.iter().rev() {
            if s.complete {
                pos.complete = Some(s.msn);
                break;
            }
            pos.open.get_or_insert((s.msn, s.parts.len() as u32));
        }
        pos
    }

    fn find(&self, msn: u64) -> Option<&Segment> {
        self.segments.iter().find(|s| s.msn == msn)
    }

    /// A whole segment, as the parts it is made of.
    pub fn segment(&self, msn: u64) -> Option<Vec<Bytes>> {
        let s = self.find(msn).filter(|s| s.complete)?;
        Some(s.parts.iter().map(|p| p.bytes.clone()).collect())
    }

    pub fn part(&self, msn: u64, index: u32) -> Option<Bytes> {
        self.find(msn)?.parts.get(index as usize).map(|p| p.bytes.clone())
    }

    pub fn view(&self) -> View {
        let segments = self
            .segments
            .iter()
            .map(|s| SegmentView {
                msn: s.msn,
                pdt_ms: s.pdt_ms,
                init: s.init,
                complete: s.complete,
                duration_ns: s.duration_ns(),
                bytes: s.len(),
                parts: s.parts.iter().map(|p| (p.duration_ns, p.independent)).collect(),
            })
            .collect();
        View { segments, longest_ns: self.longest_ns }
    }

    /// Bytes held: every segment and every init still referenced.
    pub fn memory(&self) -> usize {
        self.segments.iter().map(Segment::len).sum::<usize>()
            + self.inits.iter().map(|(_, b)| b.len()).sum::<usize>()
    }

    fn forget_unused_inits(&mut self) {
        let oldest = self.segments.front().map(|s| s.init);
        let newest = self.current_init();
        if let (Some(oldest), Some(newest)) = (oldest, newest) {
            self.inits.retain(|(g, _)| *g >= oldest || *g == newest);
        }
    }
}

#[cfg(test)]
#[path = "ring_tests.rs"]
mod tests;
