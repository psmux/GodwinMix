//! The `audio-device/devices` provide: a singleton that answers `discover`.

use godwinmix_capture_common::devices;
use godwinmix_sdk::prelude::*;
use serde_json::Value;

use crate::tools;

pub const SOURCE_PROVIDE: &str = "audio-device/source";

pub struct AudioDevices {
    reporter: Option<Reporter>,
}

impl AudioDevices {
    pub fn new() -> AudioDevices {
        AudioDevices { reporter: None }
    }
}

impl Device for AudioDevices {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        reporter.info(format!(
            "audio input discovery ready as '{}'",
            ready.instance
        ));
        self.reporter = Some(reporter);
        Ok(InitializeResult {
            latency_ms: Some(0),
        })
    }

    fn configure(&mut self, _params: Value) -> Result<Configure, RpcError> {
        Ok(Configure::applied())
    }

    fn discover(&mut self, _timeout_ms: u64) -> Result<Vec<Candidate>, RpcError> {
        let candidates = devices::candidates(devices::MICROPHONE, SOURCE_PROVIDE)
            .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e))?;
        if let Some(r) = &self.reporter {
            r.info(format!("found {} sound input(s)", candidates.len()));
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
    fn every_candidate_points_at_the_source_provide_with_ready_params() {
        godwinmix_capture_common::init().unwrap();
        let mut device = AudioDevices::new();
        for candidate in device.discover(2_000).expect("discovery never fails") {
            assert_eq!(candidate.kind, SOURCE_PROVIDE);
            assert!(candidate.params["device"].is_string());
        }
    }
}
