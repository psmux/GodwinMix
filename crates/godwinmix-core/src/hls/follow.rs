//! The sound's cutter beside a picture: its segments start where the
//! picture's do, so both playlists have the same numbers for the same
//! moments and their windows cover the same time.
//!
//! The picture is cut at its keyframes, which may come every eight seconds
//! from an encoder left at its defaults; the sound has a sample to cut at
//! every 21 ms. Cut on its own clock, the sound had four segments for every
//! one of the picture's, and after the feed restarted its shorter window
//! had already let go of the old discontinuity while the picture's still
//! listed it. hls.js then loaded nothing for a new viewer.
//!
//! So the sound's muxer writes short fragments, and each is placed in the
//! segment of the picture its middle falls in ([`Ring::place`]): a segment
//! of sound is the run of fragments over one segment of picture, give or
//! take half a fragment. A fragment the picture has not cut far enough to
//! place waits here, for a window at most; nothing on the streaming thread
//! waits for the picture.
//!
//! [`Ring::place`]: super::ring::Ring::place

use super::ring::{Part, Place};
use super::stream::Stream;
use super::track::Track;
use bytes::Bytes;
use std::collections::VecDeque;
use std::sync::{Arc, Weak};

/// Fragments of sound this long, when parts do not set the length: short
/// enough that a segment's edge is within 125 ms of the picture's.
pub const FRAGMENT_NS: u64 = 250_000_000;

/// One fragment of sound and where it is in time.
struct Fragment {
    bytes: Bytes,
    start_ns: u64,
    duration_ns: u64,
    wall_ms: i64,
    decode_time: Option<u64>,
}

pub struct Follow {
    stream: Weak<Stream>,
    track: Arc<Track>,
    /// The shortest segment the picture's muxer cuts.
    min_ns: u64,
    /// How much may wait for the picture before the oldest goes.
    hold_ns: u64,
    /// Where the fragment being written starts: its running time, wall
    /// clock and decode time.
    next: (u64, i64, Option<u64>),
    queue: VecDeque<Fragment>,
}

impl Follow {
    pub fn new(stream: &Arc<Stream>, track: Arc<Track>) -> Follow {
        let p = stream.params;
        let min_ns = p.mux_durations().0;
        let hold_ns = (u64::from(p.window_s) * 1_000_000_000).max(4 * min_ns);
        Follow { stream: Arc::downgrade(stream), track, min_ns, hold_ns, next: (0, 0, None), queue: VecDeque::new() }
    }

    /// A fragment starts: remember where.
    pub fn start(&mut self, start_ns: u64, wall_ms: i64, decode_time: Option<u64>) {
        self.next = (start_ns, wall_ms, decode_time);
    }

    /// The fragment is whole: place it, and whatever waited before it.
    pub fn push(&mut self, bytes: Bytes, duration_ns: u64) {
        let (start_ns, wall_ms, decode_time) = self.next;
        self.queue.push_back(Fragment { bytes, start_ns, duration_ns, wall_ms, decode_time });
        self.drain();
    }

    fn drain(&mut self) {
        let leader = self.stream.upgrade().and_then(|s| s.leader());
        while let Some(f) = self.queue.front() {
            let middle = f.start_ns + f.duration_ns / 2;
            let place = match &leader {
                Some(picture) => picture.place(middle, self.min_ns),
                None => self.own(middle),
            };
            match place {
                Place::Wait if self.held() <= self.hold_ns => break,
                Place::Wait | Place::Drop => {
                    self.queue.pop_front();
                }
                Place::In(msn) => {
                    let Some(f) = self.queue.pop_front() else { break };
                    self.track.begin_at(msn, f.start_ns, f.wall_ms, f.decode_time);
                    self.track.push_part(Part { bytes: f.bytes, duration_ns: f.duration_ns, independent: true });
                }
            }
        }
    }

    /// With no picture in the stream, the sound is cut on its own clock,
    /// every segment's length.
    fn own(&self, middle: u64) -> Place {
        let segment_ns = self.min_ns.max(1);
        match self.track.place(middle, segment_ns) {
            Place::In(msn) => Place::In(msn),
            _ => Place::In(self.track.newest().map_or((middle + segment_ns / 2) / segment_ns, |n| n + 1)),
        }
    }

    fn held(&self) -> u64 {
        self.queue.iter().map(|f| f.duration_ns).sum()
    }

    /// The stream ended: the open segment is whole.
    pub fn eos(&mut self) {
        self.drain();
        self.track.close();
    }
}

#[cfg(test)]
#[path = "follow_tests.rs"]
mod tests;
