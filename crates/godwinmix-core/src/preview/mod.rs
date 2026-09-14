//! Preview and monitoring streams: MJPEG pictures, PCM and Opus sound, WebRTC
//! through WHEP, and raw frames over a local socket.
//!
//! # Nothing runs unless asked
//!
//! Every stream in here is built when a client opens it and taken down when the
//! last one closes. An idle core has no JPEG cutter, no audio branch, no WebRTC
//! session and no local socket. `gmx_stream_clients{kind}` is zero for every
//! kind, and `/metrics` says so rather than leaving a dashboard to infer it.
//!
//! [`StreamClients`] is how that is counted. A stream takes a [`ClientGuard`]
//! for its life; the guard is what the gauge reads and what the teardown hangs
//! off. There is no way to open one of these streams without being counted,
//! which is the same rule `multiview.rs` applies to the mosaic.
//!
//! # What each stream costs
//!
//! | Stream | Cost while open |
//! |---|---|
//! | `/mjpeg/sheet` | nothing beyond the mosaic it holds up: the frames are the mosaic's own JPEGs |
//! | `/mjpeg/{source}`, `/mjpeg/program` | one decode, one crop and one encode per frame, on a blocking thread |
//! | `/pcm/*` | one leaky queue, a convert and a resample on the raw audio tee |
//! | `/opus/*` | the same, plus an Opus encoder at 64 kbit/s |
//! | `/whep/*` | one video encoder and one audio encoder per session, and a programme encoder lease |
//! | `preview.open` | a `unixfdsink`, which copies no pixels at all |
//!
//! The picture streams are fed from the mosaic pipeline, never from the
//! programme one, so a preview client that misbehaves cannot reach air. That
//! is the same isolation boundary the mosaic has always had.

pub mod audio;
pub mod hub;
pub mod local;
pub mod mjpeg;
pub mod whep;

pub use hub::{AudioStream, LocalStream, PreviewDemand, PreviewHandle};

use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// How many clients each kind of stream has, for `gmx_stream_clients{kind}`.
///
/// Kinds are the words a person would use: `mjpeg`, `pcm`, `opus`, `whep`,
/// `unixfd`, `preview`.
#[derive(Default)]
pub struct StreamClients {
    counts: Mutex<BTreeMap<String, u64>>,
    opened: AtomicU64,
}

impl StreamClients {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Count one client of `kind` for as long as the guard lives.
    pub fn open(self: &Arc<Self>, kind: &str) -> ClientGuard {
        *self.counts.lock().entry(kind.to_string()).or_insert(0) += 1;
        self.opened.fetch_add(1, Ordering::Relaxed);
        ClientGuard { clients: self.clone(), kind: kind.to_string() }
    }

    /// Every kind with at least one client. A kind with none is not listed,
    /// which is why `/metrics` declares the known kinds itself and fills in
    /// zero for the rest.
    pub fn counts(&self) -> BTreeMap<String, u64> {
        self.counts.lock().clone()
    }

    pub fn count(&self, kind: &str) -> u64 {
        self.counts.lock().get(kind).copied().unwrap_or(0)
    }

    pub fn total(&self) -> u64 {
        self.counts.lock().values().sum()
    }

    /// Streams opened since boot, across every kind.
    pub fn opened(&self) -> u64 {
        self.opened.load(Ordering::Relaxed)
    }
}

/// Proof that one client is on a stream. Dropping it takes the count down and,
/// for the streams that own a branch, is what eventually removes it.
pub struct ClientGuard {
    clients: Arc<StreamClients>,
    kind: String,
}

impl ClientGuard {
    pub fn kind(&self) -> &str {
        &self.kind
    }
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        let mut counts = self.clients.counts.lock();
        if let Some(n) = counts.get_mut(&self.kind) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                counts.remove(&self.kind);
            }
        }
    }
}

/// The kinds `/metrics` declares up front, so a scrape of an idle core lists
/// them all at zero and a dashboard does not break when the first client
/// arrives.
pub const STREAM_KINDS: &[&str] = &["mjpeg", "pcm", "opus", "whep", "unixfd", "preview"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kind_with_no_clients_is_not_listed_and_counts_zero() {
        let clients = StreamClients::new();
        assert_eq!(clients.total(), 0);
        assert!(clients.counts().is_empty());
        assert_eq!(clients.count("mjpeg"), 0);

        let a = clients.open("mjpeg");
        let b = clients.open("mjpeg");
        let c = clients.open("pcm");
        assert_eq!(clients.count("mjpeg"), 2);
        assert_eq!(clients.total(), 3);
        assert_eq!(clients.opened(), 3);

        drop(a);
        assert_eq!(clients.count("mjpeg"), 1);
        drop(b);
        drop(c);
        assert_eq!(clients.total(), 0);
        assert!(clients.counts().is_empty(), "a kind with no clients must not linger");
        // Opened is a counter, not a gauge: it never goes down.
        assert_eq!(clients.opened(), 3);
        assert_eq!(a_guard_kind(&clients), "mjpeg");
    }

    fn a_guard_kind(clients: &Arc<StreamClients>) -> String {
        clients.open("mjpeg").kind().to_string()
    }
}
