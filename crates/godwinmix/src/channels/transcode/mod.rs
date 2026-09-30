//! Channel destinations that ask for a rendition: planned by the planner,
//! admitted by the governor, run by the listener.
//!
//! ```text
//!   records + live streams ──► plan.rs (godwinmix-render, one call per channel)
//!                                  │
//!                                  ▼
//!                              admit.rs (godwinmix-govern, a ticket per node)
//!                                  │
//!            ┌─────────────────────┴───────────────────────┐
//!            ▼                                             ▼
//!   spec.rs: nodes with elements             outcome.rs: each destination's lot
//!   (the channel row's `transcode`)          (its table row, its view)
//! ```
//!
//! Everything here is decided in the core, where the governor, the codec
//! catalogue and the streams' shapes already are. The listener in the ingest
//! plugin is handed what to build and builds it. The reasons are written out
//! in `docs/explanation/channel-transcoding.md`.
//!
//! A destination that asked for nothing never reaches this module's state:
//! its row is the row it always was. One the plan copies gets that same row.

mod admit;
mod governor;
mod machine;
mod model;
mod outcome;
mod plan;
mod shed;
mod source;
mod spec;
mod state;
mod watch;

#[cfg(test)]
mod tests;

use std::sync::OnceLock;

use godwinmix_govern::Governor;
use godwinmix_protocol::destination::{DestinationPlan, DestinationRefusal, StoredDestination};
use parking_lot::Mutex;
use serde_json::{json, Value};

pub use machine::Machine;
use outcome::Outcome;
use state::State;

/// The channels' renditions.
pub struct Transcode {
    governor: governor::Seam,
    machine: OnceLock<Machine>,
    state: Mutex<State>,
}

impl Transcode {
    pub fn new(data_dir: Option<std::path::PathBuf>) -> Transcode {
        Transcode { governor: governor::Seam::new(data_dir), machine: OnceLock::new(), state: Mutex::default() }
    }

    /// For tests: a machine and a governor stated rather than found.
    #[cfg(test)]
    pub fn with(machine: Machine, governor: Governor) -> Transcode {
        let t = Transcode::new(None);
        let _ = t.machine.set(machine);
        t.governor.use_station(governor);
        t
    }

    /// Count against the station's governor. See `governor.rs`.
    pub fn use_governor(&self, governor: Governor) -> bool {
        self.governor.use_station(governor)
    }

    fn machine(&self) -> &Machine {
        self.machine.get_or_init(|| {
            let cat = godwinmix_core::catalogue::global();
            Machine::probe(&cat, &godwinmix_core::catalogue::select::GstRegistry)
        })
    }

    /// The row the listener gets for one destination that is on. `None`
    /// leaves it out: refused or shed, so nothing is sent.
    pub fn row(&self, channel: &str, d: &StoredDestination, mut row: Value) -> Option<Value> {
        if d.rendition.is_none() {
            return Some(row);
        }
        let state = self.state.lock();
        match state.outcome(channel, &d.id) {
            Some(Outcome::Copy(_)) => {}
            Some(Outcome::Refused(_)) => return None,
            Some(Outcome::Transcode { plan, video, audio }) => {
                row["stream"] = json!(plan.stream);
                row["rendition"] = json!(true);
                if let Some(v) = video {
                    row["video"] = json!(v);
                }
                if let Some(a) = audio {
                    row["audio"] = json!(a);
                }
            }
            // Its stream is not live yet: the listener waits rather than
            // send the stream as it is to somewhere that asked for less.
            Some(Outcome::Waiting) | None => row["rendition"] = json!(true),
        }
        Some(row)
    }

    /// The channel row's `transcode`: what to build for each stream.
    pub fn streams(&self, channel: &str) -> Option<Value> {
        let state = self.state.lock();
        let streams = &state.channels.get(channel)?.streams;
        (!streams.is_empty()).then(|| json!(streams))
    }

    /// What the plan gave one destination, or why it gave it nothing.
    pub fn view(&self, channel: &str, destination: &str) -> (Option<DestinationPlan>, Option<DestinationRefusal>) {
        let state = self.state.lock();
        match state.outcome(channel, destination) {
            Some(Outcome::Refused(no)) => (None, Some(no.clone())),
            Some(o) => (o.plan().cloned(), None),
            None => (None, None),
        }
    }

    /// The plan for one channel, for `rendition.plan {scope: "channel:<id>"}`.
    pub fn plan_of(&self, channel: &str) -> Option<godwinmix_render::Plan> {
        self.state.lock().channels.get(channel).map(|c| c.plan.clone())
    }
}
