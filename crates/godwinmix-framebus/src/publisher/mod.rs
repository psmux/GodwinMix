//! The owner side: one per device or stream.
//!
//! `write` runs on whatever thread has the frame (a GStreamer streaming
//! thread, usually). It claims a slot, fills it, publishes it and nudges the
//! readers, and none of that can wait on a reader: the claim skips leased
//! slots, and each nudge is one non blocking byte. Accepting readers, handing
//! them the region and noticing them die happens on the bus's own thread.

use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::Arc;
use std::thread::JoinHandle;

use crate::header::{checksum, NO_PTS};
use crate::ring::{Meta, Ring};
use crate::{monotonic_ns, BusName, Error, Layout, Registry};

mod conn;
mod service;
mod stats;

use service::Shared;
pub(crate) use stats::copy_planes;
pub use stats::{PublisherStats, ReaderStats};

#[derive(Clone, Debug)]
pub struct PublisherOptions {
    /// Readers that can be attached at once. At most 32.
    pub max_readers: usize,
    /// Frames one reader may hold at once. A reader that holds this many
    /// waits for one to be dropped; the owner never does.
    pub leases_per_reader: usize,
    /// Write a checksum of every frame into its slot, for tests and the
    /// benchmark's check pass. Costs one pass over the frame.
    pub checksum: bool,
}

impl Default for PublisherOptions {
    fn default() -> Self {
        PublisherOptions { max_readers: 8, leases_per_reader: 3, checksum: false }
    }
}

pub struct Publisher {
    name: BusName,
    path: PathBuf,
    opts: PublisherOptions,
    ring: Arc<Ring>,
    seq: u64,
    shared: Arc<Shared>,
    service: Option<JoinHandle<()>>,
}

impl Publisher {
    /// Publish `name` in `registry`, for frames of `layout`.
    pub fn create(
        registry: &Registry,
        name: &BusName,
        layout: Layout,
        opts: PublisherOptions,
    ) -> Result<Publisher, Error> {
        let path = registry.path(name)?;
        claim_path(name, &path)?;
        let listener = UnixListener::bind(&path)
            .map_err(|e| Error::Os(format!("binding {}: {e}", path.display())))?;
        listener.set_nonblocking(true)?;
        let ring = Arc::new(Ring::create(&layout, opts.max_readers, opts.leases_per_reader)?);
        let shared = Arc::new(Shared::new(ring.clone())?);
        let thread = shared.clone();
        let service = std::thread::Builder::new()
            .name(format!("framebus {name}"))
            .spawn(move || thread.run(listener))?;
        Ok(Publisher { name: name.clone(), path, opts, ring, seq: 0, shared, service: Some(service) })
    }

    pub fn name(&self) -> &BusName {
        &self.name
    }

    pub fn layout(&self) -> Layout {
        self.ring.header().layout().expect("our own header is valid")
    }

    /// Change the frame format. Readers are handed a new region; frames they
    /// hold from the old one stay valid until they drop them.
    pub fn set_layout(&mut self, layout: Layout) -> Result<(), Error> {
        if layout == self.layout() {
            return Ok(());
        }
        let ring = Arc::new(Ring::create(&layout, self.opts.max_readers, self.opts.leases_per_reader)?);
        self.ring.header().retired.store(1, Relaxed);
        self.ring = ring.clone();
        self.seq = 0;
        self.shared.replace(ring);
        Ok(())
    }

    /// Publish one frame, written by `fill` straight into shared memory.
    /// `false` when every slot was leased and the frame was dropped.
    pub fn write(&mut self, pts: Option<u64>, duration: Option<u64>, fill: impl FnOnce(&mut [u8])) -> bool {
        let captured_ns = monotonic_ns();
        let Some(slot) = self.ring.claim() else { return false };
        let len = self.ring.header().frame_size as usize;
        // SAFETY: the slot is claimed: no reader leases it and none can until
        // it is published, so this is the only reference to its bytes.
        let bytes = unsafe { std::slice::from_raw_parts_mut(self.ring.data(slot), len) };
        fill(bytes);
        let sum = if self.opts.checksum { checksum(bytes) } else { 0 };
        self.seq += 1;
        let meta = Meta {
            pts: pts.unwrap_or(NO_PTS),
            duration: duration.unwrap_or(NO_PTS),
            captured_ns,
            checksum: sum,
        };
        self.ring.publish(slot, self.seq, meta, monotonic_ns());
        self.shared.nudge();
        true
    }

    /// Publish one frame given as planes with their own strides, copying each
    /// row into the slot's layout.
    pub fn write_planes(&mut self, pts: Option<u64>, planes: &[(&[u8], usize)]) -> bool {
        let layout = self.layout();
        self.write(pts, None, |dst| copy_planes(&layout, planes, dst))
    }

    pub fn stats(&self) -> PublisherStats {
        PublisherStats::read(&self.ring)
    }
}

/// Take `path` for this owner: refused if a live owner answers on it,
/// cleared if the socket was left by one that died.
fn claim_path(name: &BusName, path: &std::path::Path) -> Result<(), Error> {
    if !path.exists() {
        return Ok(());
    }
    if std::os::unix::net::UnixStream::connect(path).is_ok() {
        return Err(Error::NameTaken { name: name.to_string(), pid: 0 });
    }
    std::fs::remove_file(path)
        .map_err(|e| Error::Os(format!("removing the stale socket {}: {e}", path.display())))
}

impl Drop for Publisher {
    fn drop(&mut self) {
        self.ring.header().retired.store(1, Relaxed);
        self.shared.stop();
        if let Some(t) = self.service.take() {
            let _ = t.join();
        }
        let _ = std::fs::remove_file(&self.path);
    }
}
