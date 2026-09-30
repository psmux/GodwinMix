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
mod options;
mod service;
pub(crate) mod stats;

pub use options::PublisherOptions;
use service::Shared;
pub use stats::{PublisherStats, ReaderStats};

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
        conn::claim_path(name, &path)?;
        let listener = UnixListener::bind(&path)
            .map_err(|e| Error::Os(format!("binding {}: {e}", path.display())))?;
        listener.set_nonblocking(true)?;
        let ring = Arc::new(Ring::create(
            &layout,
            opts.max_readers,
            opts.leases_per_reader,
        )?);
        let shared = Arc::new(Shared::new(ring.clone())?);
        let thread = shared.clone();
        let service = std::thread::Builder::new()
            .name(format!("framebus {name}"))
            .spawn(move || thread.run(listener))?;
        Ok(Publisher {
            name: name.clone(),
            path,
            opts,
            ring,
            seq: 0,
            shared,
            service: Some(service),
        })
    }

    pub fn name(&self) -> &BusName {
        &self.name
    }

    pub fn layout(&self) -> Layout {
        self.ring
            .header()
            .layout()
            .expect("our own header is valid")
    }

    /// Change the frame format. Readers are handed a new region; frames they
    /// hold from the old one stay valid until they drop them.
    pub fn set_layout(&mut self, layout: Layout) -> Result<(), Error> {
        if layout == self.layout() {
            return Ok(());
        }
        let ring = Arc::new(Ring::create(
            &layout,
            self.opts.max_readers,
            self.opts.leases_per_reader,
        )?);
        self.ring.header().retired.store(1, Relaxed);
        self.ring = ring.clone();
        self.seq = 0;
        self.shared.replace(ring);
        Ok(())
    }

    /// Publish one frame, written by `fill` straight into shared memory.
    /// `false` when every slot was leased and the frame was dropped.
    pub fn write(
        &mut self,
        pts: Option<u64>,
        duration: Option<u64>,
        fill: impl FnOnce(&mut [u8]),
    ) -> bool {
        self.write_len(pts, duration, 0, fill)
    }

    /// The same, filling only the first `len` bytes of the slot (0 for all of
    /// it). A chunk of sound is shorter than the slot it is in.
    pub fn write_len(
        &mut self,
        pts: Option<u64>,
        duration: Option<u64>,
        len: u64,
        fill: impl FnOnce(&mut [u8]),
    ) -> bool {
        let captured_ns = monotonic_ns();
        let Some(slot) = self.ring.claim() else {
            return false;
        };
        let size = self.ring.header().frame_size;
        let len = if len == 0 { size } else { len.min(size) };
        // SAFETY: the slot is claimed: no reader leases it and none can until
        // it is published, so this is the only reference to its bytes.
        let bytes =
            unsafe { std::slice::from_raw_parts_mut(self.ring.data(slot), len as usize) };
        fill(bytes);
        let sum = if self.opts.checksum {
            checksum(bytes)
        } else {
            0
        };
        self.seq += 1;
        let meta = Meta {
            pts: pts.unwrap_or(NO_PTS),
            duration: duration.unwrap_or(NO_PTS),
            captured_ns,
            checksum: sum,
            len,
        };
        self.ring.publish(slot, self.seq, meta, monotonic_ns());
        self.shared.nudge();
        true
    }

    pub fn stats(&self) -> PublisherStats {
        PublisherStats::read(&self.ring)
    }
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
