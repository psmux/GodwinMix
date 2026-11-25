//! The desk as it is reported: to `audio.bus.list`, to the status snapshot,
//! and to the meters.

use super::bus;
use super::desk::Desk;
use godwinmix_protocol::buses::{AudioStatus, BusStatus};

impl Desk {
    pub fn status(&self, outputs_on: &dyn Fn(&str) -> Vec<String>) -> AudioStatus {
        let buses = self
            .config
            .buses
            .iter()
            .map(|cfg| {
                let live = self.live.iter().find(|l| l.graph.id == cfg.id);
                let fault = live.and_then(|l| l.device.as_ref()).and_then(|d| d.fault.lock().clone());
                let state = live.and_then(|l| l.device_state.clone()).or(fault);
                let state = match (live, state) {
                    (None, _) => Some("not built: see the log for why".to_string()),
                    (_, s) => s,
                };
                BusStatus {
                    config: cfg.clone(),
                    kind: cfg.kind().to_string(),
                    outputs: outputs_on(&cfg.id),
                    device_state: state,
                    listen: format!("/pcm/{}", super::listen_target(&cfg.id)),
                }
            })
            .collect();
        AudioStatus { master: self.config.master, buses, cue: self.cue.lock().clone() }
    }

    /// The bus a level message came from, when it came from one that is built.
    pub fn meter_owner(&self, element: &str) -> Option<String> {
        let id = bus::bus_of_meter(element)?;
        self.live.iter().any(|l| l.graph.id == id).then(|| id.to_string())
    }

    /// True while any bus is built.
    pub fn has_buses(&self) -> bool {
        !self.live.is_empty()
    }
}
