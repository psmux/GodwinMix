//! The audio desk beyond one fader per source: the programme master, buses,
//! cue, a bus on a sound card, a bus as an output's sound.
//!
//! A bus is a mix of its own in the programme pipeline (`bus.rs`), fed from
//! every source through one shared send point per source (`send.rs`), each
//! source at a weight its kind decides (`weight.rs`). Nothing here exists until
//! the first bus does: with no bus the programme pipeline is exactly what it
//! was, and the last bus to go takes the send points with it.
//!
//! Everything in this module is called on the mixer thread. The only work on
//! a streaming thread is the weight probe, which reads three values and writes
//! one, and the only blocking work (finding a sound card) runs on a thread of
//! its own and comes back as a command.

pub mod bus;
mod calls;
pub mod check;
pub mod device;
mod desk;
pub mod encode;
mod outlets;
pub mod send;
mod status;
pub mod weight;

pub use check::Refusal;
pub use calls::Around;
pub use desk::Desk;

use godwinmix_protocol::buses::{BusConfig, BusSetRequest, CueSetRequest, MasterSetRequest, SendSetRequest};

/// One call on the desk, as the mixer queue carries it.
#[derive(Debug)]
pub enum AudioCall {
    Master(MasterSetRequest),
    Add(BusConfig),
    Set(BusSetRequest),
    Remove(String),
    List,
    Send(SendSetRequest),
    Cue(CueSetRequest),
    /// A sound card was looked for off the mixer thread. Never from the API.
    DeviceFound(DeviceFound),
}

/// What looking for a sound card came back with.
pub struct DeviceFound {
    pub bus: String,
    /// Which ask this answers. A bus whose device changed again since, or that
    /// was removed and added, ignores an older answer.
    pub ticket: u64,
    pub sink: Result<gstreamer::Element, String>,
}

impl std::fmt::Debug for DeviceFound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeviceFound").field("bus", &self.bus).field("ticket", &self.ticket).finish()
    }
}

/// How a desk hands a finished device search back to the mixer queue.
pub type Notify = std::sync::Arc<dyn Fn(DeviceFound) + Send + Sync>;

/// The audio monitoring target for a bus: `/pcm/bus:monitor`.
pub fn listen_target(bus: &str) -> String {
    format!("bus:{bus}")
}

/// The bus a monitoring target names, if it names one.
pub fn bus_of_target(target: &str) -> Option<&str> {
    target.strip_prefix("bus:")
}

