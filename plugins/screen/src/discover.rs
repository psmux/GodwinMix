//! The `screen/devices` provide: a singleton that answers `discover`.
//!
//! What it can find depends on the platform, and it says so rather than
//! pretending. Windows ships a GStreamer device provider for monitors; macOS
//! and X11 do not, and there the answer is one candidate standing for the
//! whole screen.

use godwinmix_sdk::prelude::*;
use serde_json::Value;

use crate::tools;

pub const SOURCE_PROVIDE: &str = "screen/source";

pub struct ScreenDevices {
    reporter: Option<Reporter>,
}

impl ScreenDevices {
    pub fn new() -> ScreenDevices {
        ScreenDevices { reporter: None }
    }
}

impl Device for ScreenDevices {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        reporter.info(format!("screen discovery ready as '{}'", ready.instance));
        self.reporter = Some(reporter);
        Ok(InitializeResult {
            latency_ms: Some(0),
        })
    }

    fn configure(&mut self, _params: Value) -> Result<Configure, RpcError> {
        Ok(Configure::applied())
    }

    fn discover(&mut self, _timeout_ms: u64) -> Result<Vec<Candidate>, RpcError> {
        let candidates: Vec<Candidate> = tools::screens()
            .into_iter()
            .map(|screen| {
                let index = screen["index"].as_u64().unwrap_or(0);
                Candidate {
                    kind: SOURCE_PROVIDE.to_string(),
                    name: screen["name"]
                        .as_str()
                        .unwrap_or("The whole screen")
                        .to_string(),
                    params: serde_json::json!({"monitor": index}),
                    // Lower than a camera's certainty on purpose. Most
                    // platforms will not enumerate their monitors, so the
                    // entry standing for the whole screen is an offer rather
                    // than a reading.
                    confidence: 0.7,
                }
            })
            .collect();
        if let Some(r) = &self.reporter {
            r.info(format!("offering {} screen(s)", candidates.len()));
        }
        Ok(candidates)
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        tools::dispatch(method, params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_always_something_to_offer_and_it_is_ready_for_source_add() {
        godwinmix_capture_common::init().unwrap();
        let mut device = ScreenDevices::new();
        let found = device.discover(2_000).expect("discovery never fails");
        assert!(!found.is_empty());
        for candidate in &found {
            assert_eq!(candidate.kind, SOURCE_PROVIDE);
            assert!(candidate.params["monitor"].is_number());
        }
    }
}
