//! The `camera/devices` provide: a singleton that answers `discover`.
//!
//! It runs no media and holds no device open. Every `discover` asks the
//! operating system again, because a camera plugged in a second ago is exactly
//! the one the operator is looking for.

use godwinmix_capture_common::devices;
use godwinmix_sdk::prelude::*;
use serde_json::Value;

use crate::tools;

/// The provide id the candidates point at. A candidate's `type` is what
/// `source.add` is called with, so it names the source and not this.
pub const SOURCE_PROVIDE: &str = "camera/source";

pub struct CameraDevices {
    reporter: Option<Reporter>,
}

impl CameraDevices {
    pub fn new() -> CameraDevices {
        CameraDevices { reporter: None }
    }
}

impl Device for CameraDevices {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        reporter.info(format!("camera discovery ready as '{}'", ready.instance));
        self.reporter = Some(reporter);
        Ok(InitializeResult {
            latency_ms: Some(0),
        })
    }

    fn configure(&mut self, _params: Value) -> Result<Configure, RpcError> {
        // There is nothing to keep. The machine is asked at `discover` time.
        Ok(Configure::applied())
    }

    fn discover(&mut self, _timeout_ms: u64) -> Result<Vec<Candidate>, RpcError> {
        // `DeviceMonitor` answers from a registry the platform keeps, so the
        // timeout is never reached and is not slept through. Nothing here
        // waits for a network.
        let candidates = devices::candidates(devices::CAMERA, SOURCE_PROVIDE)
            .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e))?;
        if let Some(r) = &self.reporter {
            r.info(format!("found {} camera(s)", candidates.len()));
        }
        Ok(candidates)
    }

    fn health(&mut self) -> Health {
        Health::ok()
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        tools::dispatch(method, params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_candidate_points_at_the_source_provide_with_ready_params() {
        godwinmix_capture_common::init().unwrap();
        let mut device = CameraDevices::new();
        let found = device.discover(2_000).expect("discovery never fails");
        for candidate in &found {
            assert_eq!(candidate.kind, SOURCE_PROVIDE);
            assert!(!candidate.name.is_empty());
            assert!(
                candidate.params["device"].is_string(),
                "{:?}",
                candidate.params
            );
        }
    }

    #[test]
    fn discovery_holds_nothing_so_configure_always_applies() {
        let mut device = CameraDevices::new();
        assert!(
            device
                .configure(json!({"include_screens": true}))
                .unwrap()
                .applied
        );
        assert_eq!(device.health().state, HealthState::Ok);
    }
}
