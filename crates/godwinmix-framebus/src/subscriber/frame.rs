//! A frame a reader holds. Its bytes are the owner's shared memory; nothing
//! was copied to hand it over, and dropping it gives the slot back.

use std::os::unix::net::UnixStream;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::{Arc, Condvar, Mutex};

use crate::header::{checksum, NO_PTS};
use crate::ring::Ring;
use crate::Layout;

/// Wakes a reader waiting for one of its own frames to be dropped.
#[derive(Default)]
pub struct Returns {
    lock: Mutex<()>,
    cv: Condvar,
}

impl Returns {
    pub fn notify(&self) {
        let _g = self.lock.lock().unwrap();
        self.cv.notify_all();
    }

    pub fn wait(&self, for_ms: u64) {
        let g = self.lock.lock().unwrap();
        let _ = self.cv.wait_timeout(g, std::time::Duration::from_millis(for_ms));
    }
}

pub struct Frame {
    pub(super) ring: Arc<Ring>,
    /// Keeps the socket open while the frame lives. If it closed, the owner
    /// would give this reader's place, and so its leases, to someone else.
    pub(super) _link: Arc<UnixStream>,
    pub(super) returns: Arc<Returns>,
    pub(super) reader: usize,
    pub(super) slot: usize,
    pub(super) seq: u64,
    pub(super) skipped: u64,
}

impl Frame {
    /// The whole frame, every plane with its padding.
    pub fn data(&self) -> &[u8] {
        self.ring.frame(self.slot)
    }

    pub fn layout(&self) -> Layout {
        self.ring.header().layout().expect("checked when the region was attached")
    }

    /// Plane `i` from its first row to the end of its last.
    pub fn plane(&self, i: usize) -> &[u8] {
        let l = self.layout();
        let start = l.offsets[i] as usize;
        &self.data()[start..start + l.rows(i) as usize * l.strides[i] as usize]
    }

    /// The owner's frame number. It starts at 1 and restarts when the owner
    /// does or when the format changes.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Frames published between the last one this reader took and this one.
    pub fn skipped(&self) -> u64 {
        self.skipped
    }

    pub fn pts(&self) -> Option<u64> {
        Some(self.ring.slot(self.slot).pts.load(Relaxed)).filter(|&p| p != NO_PTS)
    }

    pub fn duration(&self) -> Option<u64> {
        Some(self.ring.slot(self.slot).duration.load(Relaxed)).filter(|&p| p != NO_PTS)
    }

    /// When the owner was handed the frame, on [`crate::monotonic_ns`].
    pub fn captured_ns(&self) -> u64 {
        self.ring.slot(self.slot).captured_ns.load(Relaxed)
    }

    /// When the frame became readable, on [`crate::monotonic_ns`].
    pub fn published_ns(&self) -> u64 {
        self.ring.slot(self.slot).published_ns.load(Relaxed)
    }

    /// `Some(true)` if the bytes match the owner's checksum, `None` if the
    /// owner does not write one.
    pub fn verify(&self) -> Option<bool> {
        let want = self.ring.slot(self.slot).checksum.load(Relaxed);
        (want != 0).then(|| checksum(self.data()) == want)
    }
}

impl AsRef<[u8]> for Frame {
    fn as_ref(&self) -> &[u8] {
        self.data()
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        self.ring.release(self.reader, self.slot);
        self.returns.notify();
    }
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame").field("seq", &self.seq).field("slot", &self.slot).finish()
    }
}
