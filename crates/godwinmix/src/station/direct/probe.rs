//! Inputs opened only to be looked at: `headend.scan` reads what a feed
//! carries through the same input a show would use, as a row of the direct
//! table with no outputs and nothing watched. A probe is no show: it is in
//! no list, raises no alarm, and goes when the scan is done.

use super::Direct;
use parking_lot::Mutex;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// The ids every probe has, which no show can take: a show's id is a slug
/// and has no dot.
pub const PREFIX: &str = "scan.";

#[derive(Default)]
pub struct Probes {
    rows: Mutex<BTreeMap<String, Probe>>,
}

struct Probe {
    spec: Value,
    /// The host's last `direct.input` for it.
    input: Option<Value>,
}

impl Probes {
    pub fn is_probe(id: &str) -> bool {
        id.starts_with(PREFIX)
    }

    /// The rows the table carries for every probe.
    pub fn rows(&self) -> Vec<Value> {
        let rows = self.rows.lock();
        rows.iter()
            .map(|(id, p)| {
                json!({"id": id, "name": id, "input": p.spec, "outputs": [], "monitor": {"alarms": false, "pictures": false}, "carry": {"mpeg2": true, "tracks": true}})
            })
            .collect()
    }

    /// Keep what the host said of a probe's input. False for an id that is
    /// no probe.
    pub fn take_input(&self, id: &str, v: &Value) -> bool {
        match self.rows.lock().get_mut(id) {
            Some(p) => {
                p.input = Some(v.clone());
                true
            }
            None => Self::is_probe(id),
        }
    }

    pub fn input(&self, id: &str) -> Option<Value> {
        self.rows.lock().get(id).and_then(|p| p.input.clone())
    }
}

impl Direct {
    /// Open `spec` as probe `id` and hand the table over. Answers the ask,
    /// for `wait_handed`.
    pub fn probe(&self, id: &str, spec: Value) -> u64 {
        self.probes.rows.lock().insert(id.to_string(), Probe { spec, input: None });
        self.hand_over()
    }

    /// Close probes and hand the table over.
    pub fn unprobe(&self, ids: &[String]) -> u64 {
        {
            let mut rows = self.probes.rows.lock();
            for id in ids {
                rows.remove(id);
            }
        }
        self.hand_over()
    }
}
