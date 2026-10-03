//! `icecast/output`: the `Output` methods, and the errors both halves share.

use godwinmix_capture_common::fifo::{open_read, Fifo};
use godwinmix_sdk::prelude::*;
use serde_json::{json, Value};

use crate::send::Sender;
use crate::settings::Settings;

pub fn internal(e: impl std::fmt::Display) -> RpcError {
    RpcError::new(codes::INTERNAL_ERROR, e.to_string())
}

pub fn invalid(e: impl std::fmt::Display) -> RpcError {
    RpcError::new(codes::INVALID_PARAMS, e.to_string())
}

pub fn no_method(kind: &str, other: &str) -> RpcError {
    RpcError::new(codes::METHOD_NOT_FOUND, format!("{kind} has no method '{other}'. It answers 'stats', and the standard methods in docs/reference/plugin-protocol.md."))
}

#[derive(Default)]
pub struct IcecastOutput {
    settings: Settings,
    reporter: Option<Reporter>,
    fifo: Option<Fifo>,
    address: String,
    sender: Option<Sender>,
}

impl IcecastOutput {
    fn open_fifo(&mut self, address: &str) -> Result<(), RpcError> {
        if !address.is_empty() {
            self.address = address.to_string();
        }
        if self.fifo.is_none() && !self.address.is_empty() {
            self.fifo = Some(open_read(std::path::Path::new(&self.address)).map_err(internal)?);
        }
        Ok(())
    }
}

impl Output for IcecastOutput {
    fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError> {
        self.settings = Settings::from_params(&ready.params).map_err(invalid)?;
        gmx_netkit::init().map_err(internal)?;
        gmx_netkit::elements::require(crate::send::NEEDED).map_err(internal)?;
        let media = if ready.media.is_empty() { PluginEnv::from_env().media } else { ready.media.clone() };
        self.open_fifo(&media)?;
        reporter.info(format!("icecast/output '{}' will send to {}", ready.instance, self.settings.describe()));
        self.reporter = Some(reporter);
        Ok(InitializeResult::default())
    }

    fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError> {
        if let Some(p) = self.settings.problem() {
            return Err(invalid(p));
        }
        self.sender = None;
        self.open_fifo(&params.media)?;
        let fifo = self.fifo.take().ok_or_else(|| invalid("the core sent no media address, so there is no programme to send"))?;
        self.sender = Some(Sender::start(&self.settings, fifo, self.reporter.clone()).map_err(internal)?);
        Ok(StartResult::default())
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        self.sender = None;
        Ok(())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let wanted = Settings::from_params(&params).map_err(invalid)?;
        let (running, changed) = (self.sender.is_some(), wanted != self.settings);
        self.settings = wanted;
        if !running || !changed {
            return Ok(Configure::applied());
        }
        Ok(Configure::restart_required("the mount and the encoder are set up once. Reconnect the output and it sends with the new settings."))
    }

    fn health(&mut self) -> Health {
        let Some(s) = &self.sender else { return Health::degraded("not started yet") };
        if let Some(f) = s.pipe.failure() {
            return Health::failing(format!("sending to {} failed: {f}", self.settings.describe()));
        }
        let sent = s.sent();
        let refused = s.state.last_error.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let (Some(why), false) = (refused, s.state.connected.load(std::sync::atomic::Ordering::Relaxed)) {
            return Health::degraded(format!("{why} Trying again."));
        }
        if sent == 0 {
            return Health::degraded(format!("connected to {}, waiting for the programme's sound", self.settings.describe()));
        }
        let mut h = Health::ok();
        h.detail = Some(format!("{} kB sent to {}", sent / 1000, self.settings.describe()));
        h
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        match method {
            "stats" => Ok(json!({"address": self.settings.describe(), "bytes_sent": self.sender.as_ref().map(Sender::sent)})),
            other => Err(no_method("icecast/output", other)),
        }
    }
}
