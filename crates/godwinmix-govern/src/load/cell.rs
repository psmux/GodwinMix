//! The latest load, written by the sampler and read by anyone.
//!
//! The figures are atomics, so a read takes no lock and never waits on the
//! sampler. Device figures are a short list behind a mutex held for one copy.

use parking_lot::Mutex;
use serde::Serialize;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};

/// What the machine is doing, smoothed.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Load {
    /// Every process, moving average, thousandths of a core.
    pub system_millicores: u32,
    /// This process, moving average.
    pub own_millicores: u32,
    /// Everything that is not this process, at its peak over the window.
    pub others_peak_millicores: u32,
    /// How far the machine's load has jumped above its mean in the window.
    pub jitter_millicores: u32,
    pub available_mib: Option<u64>,
    pub devices: Vec<(String, u32)>,
    /// Readings taken so far. Zero means nothing has been measured yet.
    pub samples: u64,
}

impl Load {
    /// How busy a device is, thousandths, when the platform says.
    pub fn device(&self, name: &str) -> Option<u32> {
        self.devices.iter().find(|(n, _)| n == name).map(|(_, v)| *v)
    }
}

const UNKNOWN: u64 = u64::MAX;

#[derive(Debug)]
pub struct LoadCell {
    system: AtomicU32,
    own: AtomicU32,
    others_peak: AtomicU32,
    jitter: AtomicU32,
    available: AtomicU64,
    samples: AtomicU64,
    devices: Mutex<Vec<(String, u32)>>,
}

impl Default for LoadCell {
    fn default() -> Self {
        LoadCell {
            system: AtomicU32::new(0),
            own: AtomicU32::new(0),
            others_peak: AtomicU32::new(0),
            jitter: AtomicU32::new(0),
            available: AtomicU64::new(UNKNOWN),
            samples: AtomicU64::new(0),
            devices: Mutex::new(Vec::new()),
        }
    }
}

impl LoadCell {
    pub fn store(&self, l: &Load) {
        self.system.store(l.system_millicores, Relaxed);
        self.own.store(l.own_millicores, Relaxed);
        self.others_peak.store(l.others_peak_millicores, Relaxed);
        self.jitter.store(l.jitter_millicores, Relaxed);
        self.available.store(l.available_mib.unwrap_or(UNKNOWN), Relaxed);
        if *self.devices.lock() != l.devices {
            *self.devices.lock() = l.devices.clone();
        }
        // Last, so a reader that sees the new count sees the figures with it
        // on any platform where that matters; exactness here is not needed.
        self.samples.store(l.samples, Relaxed);
    }

    pub fn load(&self) -> Load {
        let available = self.available.load(Relaxed);
        Load {
            system_millicores: self.system.load(Relaxed),
            own_millicores: self.own.load(Relaxed),
            others_peak_millicores: self.others_peak.load(Relaxed),
            jitter_millicores: self.jitter.load(Relaxed),
            available_mib: (available != UNKNOWN).then_some(available),
            devices: self.devices.lock().clone(),
            samples: self.samples.load(Relaxed),
        }
    }
}
