//! The governor itself: the profile, the live load, and the book of tickets.
//!
//! Reads of the load are lock free. Admission and release take one mutex,
//! for the arithmetic only: nothing under it reads a file, sleeps or calls
//! out, so it is safe from any thread, a GStreamer one included.

mod admit;
mod room;

pub use admit::{Admit, Claim};

use crate::config::GovernorConfig;
use crate::fingerprint::{cores, Machine};
use crate::headroom;
use crate::load::{Load, LoadCell, Sampler};
use crate::profile::Profile;
use crate::shed::Held;
use godwinmix_protocol::rendition::Cost;
use parking_lot::{Mutex, RwLock};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Default)]
pub(crate) struct Book {
    next: u64,
    pub(crate) held: BTreeMap<u64, Held>,
}

impl Book {
    fn total(&self) -> Cost {
        self.held.values().fold(Cost::default(), |a, h| a.plus(h.cost))
    }

    fn on_device(&self, device: &str) -> (u32, u32) {
        self.held
            .values()
            .filter(|h| h.device.as_deref() == Some(device))
            .fold((0, 0), |(m, s), h| (m + h.cost.device_millis, s + h.cost.device_sessions))
    }
}

pub(crate) struct Inner {
    cores: u32,
    memory_total_mib: u64,
    config: GovernorConfig,
    profile: RwLock<Arc<Profile>>,
    load: Arc<LoadCell>,
    pub(crate) book: Mutex<Book>,
    sampler: Mutex<Option<Sampler>>,
}

/// One per machine. Cheap to clone; every clone is the same governor.
#[derive(Clone)]
pub struct Governor {
    pub(crate) inner: Arc<Inner>,
}

impl Governor {
    /// A governor for this machine. It admits against `profile` and the book
    /// alone until [`Governor::start_sampling`] is called.
    pub fn new(config: GovernorConfig, profile: Profile) -> Governor {
        let m = Machine::current();
        Governor::with_machine(config, profile, m.cores.max(cores()), m.memory_mib)
    }

    /// The same with the machine stated, for tests and for sizing a machine
    /// that is not this one.
    pub fn with_machine(config: GovernorConfig, profile: Profile, cores: u32, memory_total_mib: u64) -> Governor {
        Governor {
            inner: Arc::new(Inner {
                cores: cores.max(1),
                memory_total_mib,
                config,
                profile: RwLock::new(Arc::new(profile)),
                load: Arc::new(LoadCell::default()),
                book: Mutex::new(Book::default()),
                sampler: Mutex::new(None),
            }),
        }
    }

    /// Sample the machine once a second from now on. Calling it again does
    /// nothing. The sampler stops when the last clone is dropped.
    pub fn start_sampling(&self) -> std::io::Result<()> {
        let mut s = self.inner.sampler.lock();
        if s.is_none() {
            *s = Some(Sampler::start(self.inner.load.clone(), Duration::from_secs(1))?);
        }
        Ok(())
    }

    /// Replace the profile, after a calibration.
    pub fn set_profile(&self, profile: Profile) {
        *self.inner.profile.write() = Arc::new(profile);
    }

    pub fn profile(&self) -> Arc<Profile> {
        self.inner.profile.read().clone()
    }

    /// The latest load. Lock free.
    pub fn load(&self) -> Load {
        self.inner.load.load()
    }

    /// Where the sampler writes. A test or another sampler may write here too.
    pub fn load_cell(&self) -> &Arc<LoadCell> {
        &self.inner.load
    }

    /// The CPU kept free, thousandths of a core, as things stand.
    pub fn reserve(&self) -> u32 {
        let l = self.inner.load.load();
        headroom::reserve(self.inner.cores, self.inner.config.desktop, l.jitter_millicores, self.inner.config.reserve_override())
    }

    /// What could be admitted now on the CPU alone, or on `device`.
    pub fn headroom(&self, device: Option<&str>) -> Cost {
        let book = self.inner.book.lock();
        self.have(&book, &self.inner.load.load(), device)
    }

    /// Every ticket held, for a status page.
    pub fn held(&self) -> Vec<Held> {
        self.inner.book.lock().held.values().cloned().collect()
    }
}

impl Inner {
    pub(crate) fn release(&self, id: u64) {
        self.book.lock().held.remove(&id);
    }
}

#[cfg(test)]
mod tests;
