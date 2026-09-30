//! What is free, and what to give up when there is too little.

use super::{Book, Governor};
use crate::headroom::{self, DeviceUse, Inputs};
use crate::load::Load;
use crate::shed::{self, Held, ShedStep};
use godwinmix_protocol::rendition::Cost;

impl Governor {
    /// What to drop, in order, when the machine is over its line; empty
    /// when it is not. The governor decides the order and does nothing
    /// itself: the caller stops or changes each thing, which drops or
    /// lowers its ticket, and raises the step's `why` as an alert.
    ///
    /// The line is capacity less the reserve. Shedding starts once live load
    /// has eaten half the reserve, and frees enough to be back under the line.
    pub fn shed(&self) -> Vec<ShedStep> {
        // A show's book is the station's, and so is the load: the station
        // decides and says which of this show's tickets to give up.
        if let Some(remote) = self.remote() {
            return remote.shed();
        }
        let load = self.load();
        if load.samples == 0 {
            return Vec::new();
        }
        let cap = self.inner.cores * 1000;
        let reserve = self.reserve();
        let line = cap.saturating_sub(reserve);
        if load.system_millicores <= cap.saturating_sub(reserve / 2) {
            return Vec::new();
        }
        let excess = load.system_millicores - line;
        let held: Vec<Held> = self.inner.book.lock().held.values().cloned().collect();
        shed::plan(&held, excess, &self.profile())
    }

    pub(crate) fn have(&self, book: &Book, load: &Load, device: Option<&str>) -> Cost {
        let profile = self.inner.profile.read().clone();
        let device = device.map(|d| {
            let (committed_millis, committed_sessions) = book.on_device(d);
            DeviceUse { committed_millis, committed_sessions, session_limit: profile.session_limit(d), measured_millis: load.device(d) }
        });
        headroom::have(&Inputs {
            cores: self.inner.cores,
            memory_total_mib: self.inner.memory_total_mib,
            load,
            committed: book.total(),
            reserve_millicores: headroom::reserve(self.inner.cores, self.inner.config.desktop, load.jitter_millicores, self.inner.config.reserve_override()),
            device,
            uplink_kbps: self.inner.config.uplink_kbps,
        })
    }
}
