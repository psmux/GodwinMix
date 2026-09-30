//! One rung (or the audio beside a ladder): its ring, what it carries, and
//! the signal a waiting playlist request listens to.
//!
//! Written by one GStreamer streaming thread, read by any number of HTTP
//! requests. The ring's mutex is held for a push or a clone of some `Bytes`
//! and never across anything that waits, so a slow viewer cannot hold up the
//! packager, and the packager never waits for a viewer. Waiting is done on a
//! `tokio::sync::watch` channel carrying the ring's [`Position`], which a
//! request awaits on the server's runtime rather than on a thread of its own.

use super::playlist;
use super::ring::{Part, Position, Ring, View};
use super::HlsParams;
use bytes::Bytes;
use parking_lot::{Mutex, RwLock};
use tokio::sync::watch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    Video,
    Audio,
}

/// What a playlist says about a rung. Filled in from the caps the muxer
/// was given, so it is empty until the first buffer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackInfo {
    /// RFC 6381, `avc1.64001f`, `hvc1.1.6.L93.B0`, `mp4a.40.2`.
    pub codecs: String,
    pub width: u32,
    pub height: u32,
    /// Frames per second as a fraction.
    pub fps: Option<(i32, i32)>,
    pub channels: u32,
    /// What the rendition asked for, 0 when nobody said.
    pub declared_kbps: u32,
}

pub struct Track {
    pub id: String,
    pub kind: TrackKind,
    params: HlsParams,
    info: RwLock<TrackInfo>,
    ring: Mutex<Ring>,
    pos: watch::Sender<Position>,
    rendered: Mutex<Option<((u64, u64), Bytes)>>,
}

impl Track {
    pub fn new(id: &str, kind: TrackKind, params: HlsParams, declared_kbps: u32) -> Track {
        Track {
            id: id.to_string(),
            kind,
            params,
            info: RwLock::new(TrackInfo { declared_kbps, ..TrackInfo::default() }),
            ring: Mutex::new(Ring::new(params.ring_capacity())),
            pos: watch::Sender::new(Position::default()),
            rendered: Mutex::new(None),
        }
    }

    pub fn info(&self) -> TrackInfo {
        self.info.read().clone()
    }

    pub fn update_info(&self, f: impl FnOnce(&mut TrackInfo)) {
        f(&mut self.info.write());
    }

    // --- The packager's side. Called on a streaming thread. -------------------

    pub fn set_init(&self, bytes: Bytes) {
        self.ring.lock().set_init(bytes);
    }

    pub fn begin(&self, start_ns: u64, pdt_ms: i64) -> Option<u64> {
        self.change(|r| r.begin(start_ns, self.params.segment_ns(), pdt_ms))
    }

    pub fn push_part(&self, part: Part) -> bool {
        self.change(|r| r.push_part(part))
    }

    pub fn close(&self) {
        self.change(Ring::close)
    }

    fn change<T>(&self, f: impl FnOnce(&mut Ring) -> T) -> T {
        let (out, pos) = {
            let mut ring = self.ring.lock();
            let out = f(&mut ring);
            (out, ring.position())
        };
        // Outside the ring's lock: a waiter woken here takes that lock next.
        self.pos.send_if_modified(|p| std::mem::replace(p, pos) != pos);
        out
    }

    // --- The server's side. ---------------------------------------------------

    pub fn position(&self) -> Position {
        *self.pos.borrow()
    }

    /// A receiver to wait on. Awaiting it holds no lock on the ring.
    pub fn watch(&self) -> watch::Receiver<Position> {
        self.pos.subscribe()
    }

    pub fn segment(&self, msn: u64) -> Option<Vec<Bytes>> {
        self.ring.lock().segment(msn)
    }

    pub fn part(&self, msn: u64, index: u32) -> Option<Bytes> {
        self.ring.lock().part(msn, index)
    }

    pub fn init(&self, gen: u32) -> Option<Bytes> {
        self.ring.lock().init(gen)
    }

    pub fn view(&self) -> View {
        self.ring.lock().view()
    }

    pub fn memory(&self) -> usize {
        self.ring.lock().memory()
    }

    /// The media playlist, rendered once per change of the ring (or of the
    /// other rungs' positions) and shared by every request until the next.
    /// `reports` are the other rungs, for `EXT-X-RENDITION-REPORT`.
    pub fn playlist(&self, reports: &[playlist::Report]) -> Bytes {
        let seen = reports.iter().fold(0u64, |h, r| {
            h.wrapping_mul(31).wrapping_add(r.last_msn.wrapping_mul(64) + u64::from(r.last_part.unwrap_or(0)))
        });
        let (key, view) = {
            let ring = self.ring.lock();
            let key = (ring.version(), seen);
            if let Some((k, text)) = self.rendered.lock().as_ref() {
                if *k == key {
                    return text.clone();
                }
            }
            (key, ring.view())
        };
        let text = Bytes::from(playlist::media(&view, &self.params, reports));
        *self.rendered.lock() = Some((key, text.clone()));
        text
    }

    /// The peak and average bitrate of the segments held, in bit/s, for
    /// `BANDWIDTH` and `AVERAGE-BANDWIDTH`.
    pub fn measured_bps(&self) -> Option<(u64, u64)> {
        let view = self.view();
        let whole = view.segments.iter().filter(|s| s.complete && s.duration_ns > 0);
        let (mut peak, mut bytes, mut ns) = (0u64, 0u64, 0u64);
        for s in whole {
            peak = peak.max(s.bytes as u64 * 8_000_000_000 / s.duration_ns);
            bytes += s.bytes as u64;
            ns += s.duration_ns;
        }
        (ns > 0).then(|| (peak, bytes * 8_000_000_000 / ns))
    }
}
