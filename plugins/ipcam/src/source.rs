//! `ipcam/source`: the `Source` methods.

use godwinmix_sdk::prelude::*;
use serde_json::{json, Value};

use crate::camera::{Camera, Sink};
use crate::settings::{Mode, Settings};

#[derive(Default)]
pub struct IpCam {
    settings: Settings,
    reporter: Option<Reporter>,
    camera: Option<Camera>,
}

fn invalid(e: String) -> RpcError {
    RpcError::new(codes::INVALID_PARAMS, e)
}

pub const NEEDED: &[&str] = &["souphttpsrc", "multipartdemux", "jpegparse", "matroskamux", "fdsink"];

impl Source for IpCam {
    fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError> {
        self.settings = Settings::from_params(&ready.params).map_err(invalid)?;
        gmx_netkit::init().map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e))?;
        gmx_netkit::elements::require(NEEDED).map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e))?;
        reporter.info(format!("ipcam/source '{}' will read {}", ready.instance, self.settings.redacted()));
        self.reporter = Some(reporter);
        Ok(InitializeResult { latency_ms: Some(100) })
    }

    fn start(&mut self, _params: &StartParams) -> Result<StartResult, RpcError> {
        if let Some(p) = self.settings.problem() {
            return Err(invalid(p));
        }
        self.camera = None;
        let camera = Camera::start(&self.settings, Sink::Stdout, self.reporter.clone()).map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e))?;
        self.camera = Some(camera);
        Ok(StartResult { latency_ms: Some(100) })
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        self.camera = None;
        Ok(())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let wanted = Settings::from_params(&params).map_err(invalid)?;
        let running = self.camera.is_some();
        let changed = wanted != self.settings;
        self.settings = wanted;
        if !running || !changed {
            return Ok(Configure::applied());
        }
        Ok(Configure::restart_required("a camera takes a new address, login or rate by being opened again"))
    }

    fn health(&mut self) -> Health {
        let Some(c) = &self.camera else { return Health::degraded("not started yet") };
        if let Some(f) = c.pipe.failure() {
            return Health::failing(format!("reading {} failed: {f}", self.settings.redacted()));
        }
        let error = c.last_error.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match (c.frames.load(std::sync::atomic::Ordering::Relaxed), error) {
            (_, Some(e)) if self.settings.mode == Mode::Snapshot => Health::degraded(format!("the last snapshot failed: {e}")),
            (0, _) => Health::degraded(format!("waiting for the first picture from {}", self.settings.redacted())),
            (n, _) => {
                let mut h = Health::ok();
                h.detail = Some(format!("{n} pictures from {}", self.settings.redacted()));
                h
            }
        }
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        match method {
            "stats" => Ok(json!({
                "address": self.settings.redacted(),
                "mode": if self.settings.mode == Mode::Snapshot { "snapshot" } else { "mjpeg" },
                "pictures": self.camera.as_ref().map(|c| c.frames.load(std::sync::atomic::Ordering::Relaxed)),
            })),
            other => Err(RpcError::new(
                codes::METHOD_NOT_FOUND,
                format!("ipcam/source has no method '{other}'. It answers 'stats', and the standard source methods in docs/reference/plugin-protocol.md."),
            )),
        }
    }
}
