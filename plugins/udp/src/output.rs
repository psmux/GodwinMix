//! `udp/output`: the `Output` methods.
//!
//! The FIFO is opened at `initialize`, as `file-record` does and for the same
//! reason: the core opens its end before it calls `start`, and each open waits
//! for the other end, so waiting for `start` would deadlock.

use godwinmix_capture_common::fifo::{open_read, Fifo};
use godwinmix_sdk::prelude::*;
use serde_json::{json, Value};

use crate::send::settings::Settings;
use crate::send::Sender;
use crate::source::internal;

pub struct UdpOutput {
    settings: Settings,
    reporter: Option<Reporter>,
    fifo: Option<Fifo>,
    address: String,
    sender: Option<Sender>,
}

impl UdpOutput {
    pub fn new() -> UdpOutput {
        UdpOutput { settings: Settings::default(), reporter: None, fifo: None, address: String::new(), sender: None }
    }

    fn open_fifo(&mut self, address: &str) -> Result<(), RpcError> {
        if !address.is_empty() {
            self.address = address.to_string();
        }
        if self.fifo.is_some() || self.address.is_empty() {
            return Ok(());
        }
        self.fifo = Some(open_read(std::path::Path::new(&self.address)).map_err(internal)?);
        Ok(())
    }
}

impl Output for UdpOutput {
    fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError> {
        self.settings = Settings::from_params(&ready.params).map_err(|e| RpcError::new(codes::INVALID_PARAMS, e))?;
        gmx_netkit::init().map_err(internal)?;
        gmx_netkit::elements::require(crate::send::NEEDED).map_err(internal)?;
        let media = if ready.media.is_empty() { PluginEnv::from_env().media } else { ready.media.clone() };
        self.open_fifo(&media)?;
        reporter.info(format!("udp/output '{}' will send to {}", ready.instance, self.settings.describe()));
        self.reporter = Some(reporter);
        Ok(InitializeResult::default())
    }

    fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError> {
        self.sender = None;
        self.open_fifo(&params.media)?;
        let fifo = self.fifo.take().ok_or_else(|| {
            RpcError::new(
                codes::INVALID_PARAMS,
                "the core sent no media address, so there is no programme to send. An output \
                 reads it from the FIFO named in start.params.media; see \
                 docs/reference/plugin-lifecycle.md.",
            )
        })?;
        let sender = Sender::start(&self.settings, fifo, self.reporter.clone()).map_err(internal)?;
        self.sender = Some(sender);
        Ok(StartResult::default())
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        self.sender = None;
        Ok(())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let wanted = Settings::from_params(&params).map_err(|e| RpcError::new(codes::INVALID_PARAMS, e))?;
        if wanted == self.settings {
            return Ok(Configure::applied());
        }
        self.settings = wanted;
        if self.sender.is_none() {
            return Ok(Configure::applied());
        }
        Ok(Configure::restart_required(
            "the socket and the multiplexer are set up once. Reconnect the output and it \
             sends with the new address, TTL or rate.",
        ))
    }

    fn health(&mut self) -> Health {
        match &self.sender {
            Some(s) => s.health(),
            None => Health::degraded("not started yet"),
        }
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        match method {
            "stats" => Ok(json!({
                "address": self.settings.describe(),
                "bytes_sent": self.sender.as_ref().map(Sender::bytes_sent),
            })),
            other => Err(RpcError::new(
                codes::METHOD_NOT_FOUND,
                format!(
                    "udp/output has no method '{other}'. It answers 'stats', and the standard \
                     output methods in docs/reference/plugin-protocol.md."
                ),
            )),
        }
    }
}
