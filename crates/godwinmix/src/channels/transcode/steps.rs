//! What one destination's picture goes through on its way out, in the
//! order it happens: the decode, the scale and the encode its plan reads,
//! each with the element the listener builds and whether it runs on the
//! GPU. Read from the spec the listener was handed, so it says what runs.

use godwinmix_protocol::shows::OutputStep;
use serde_json::Value;

use super::Transcode;

const SHOWN: &[&str] = &["decode", "scale", "encode"];

impl Transcode {
    /// Empty for a destination that copies, or that has no plan yet.
    pub fn steps(&self, channel: &str, destination: &str) -> Vec<OutputStep> {
        let state = self.state.lock();
        let Some(plan) = state.outcome(channel, destination).and_then(|o| o.plan()) else { return Vec::new() };
        let Some(ch) = state.channels.get(channel) else { return Vec::new() };
        let built: Vec<&Value> = ch.streams.iter().flat_map(|s| s["nodes"].as_array().into_iter().flatten()).collect();
        plan.nodes
            .iter()
            .filter_map(|id| built.iter().find(|n| n["id"] == id.as_str()))
            .filter(|n| SHOWN.contains(&n["kind"].as_str().unwrap_or_default()) && n["track"] != "audio")
            .map(|n| step(n))
            .collect()
    }
}

fn step(n: &Value) -> OutputStep {
    let accel = n["accel"].as_str().unwrap_or("software");
    OutputStep {
        kind: n["kind"].as_str().unwrap_or_default().to_string(),
        element: n["element"].as_str().map(str::to_string),
        gpu: n["memory"] == "cuda" || (accel != "software" && n["hardware"] != false),
        accel: (accel != "software").then(|| accel.to_string()),
    }
}
