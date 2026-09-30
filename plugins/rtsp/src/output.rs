//! `rtsp/output`: the `Output` methods.
//!
//! The FIFO is opened at `initialize`, as every output plugin does: the core
//! opens its end before it calls `start`, and each open waits for the other.
//! The port is opened at `start` and closed at `stop`.

use std::sync::Arc;

use godwinmix_capture_common::fifo::{open_read, Fifo};
use godwinmix_sdk::prelude::*;
use serde_json::{json, Value};

use crate::feed::Feed;
use crate::ingest::Ingest;
use crate::server::Server;
use crate::settings::Settings;

pub struct RtspOutput {
    settings: Settings,
    reporter: Option<Reporter>,
    fifo: Option<Fifo>,
    address: String,
    running: Option<(Arc<Server>, Ingest, Arc<Feed>)>,
}

pub fn internal(e: impl std::fmt::Display) -> RpcError {
    RpcError::new(codes::INTERNAL_ERROR, e.to_string())
}

impl RtspOutput {
    pub fn new() -> RtspOutput {
        RtspOutput { settings: Settings::default(), reporter: None, fifo: None, address: String::new(), running: None }
    }

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

/// Serve `fifo` at the settings' address.
pub fn serve(s: &Settings, fifo: Fifo, reporter: Option<Reporter>) -> Result<(Arc<Server>, Ingest, Arc<Feed>), String> {
    gmx_netkit::init()?;
    let server = Arc::new(Server::start(s)?);
    let feed = Arc::new(Feed::default());
    let (weak, f, r) = (Arc::downgrade(&server), feed.clone(), reporter.clone());
    let ready = Box::new(move || {
        if let Some(server) = weak.upgrade() {
            if let Err(e) = server.mount(&f) {
                if let Some(r) = &r {
                    r.error(e);
                }
            }
        }
    });
    let ingest = Ingest::start(fifo, feed.clone(), ready, reporter)?;
    Ok((server, ingest, feed))
}

impl Output for RtspOutput {
    fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError> {
        self.settings = Settings::from_params(&ready.params).map_err(|e| RpcError::new(codes::INVALID_PARAMS, e))?;
        gmx_netkit::init().map_err(internal)?;
        gmx_netkit::elements::require(crate::ingest::NEEDED).map_err(internal)?;
        let media = if ready.media.is_empty() { PluginEnv::from_env().media } else { ready.media.clone() };
        self.open_fifo(&media)?;
        reporter.info(format!("rtsp/output '{}' will serve {}", ready.instance, self.settings.describe()));
        self.reporter = Some(reporter);
        Ok(InitializeResult::default())
    }

    fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError> {
        self.running = None;
        self.open_fifo(&params.media)?;
        let fifo = self.fifo.take().ok_or_else(|| {
            RpcError::new(codes::INVALID_PARAMS, "the core sent no media address, so there is no programme to serve. See docs/reference/plugin-lifecycle.md.")
        })?;
        self.running = Some(serve(&self.settings, fifo, self.reporter.clone()).map_err(internal)?);
        Ok(StartResult::default())
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        self.running = None;
        Ok(())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let wanted = Settings::from_params(&params).map_err(|e| RpcError::new(codes::INVALID_PARAMS, e))?;
        if wanted == self.settings || self.running.is_none() {
            self.settings = wanted;
            return Ok(Configure::applied());
        }
        self.settings = wanted;
        Ok(Configure::restart_required("the port and path are opened once. Reconnect the output and it serves at the new address."))
    }

    fn health(&mut self) -> Health {
        let Some((server, ingest, feed)) = &self.running else { return Health::degraded("not started yet") };
        if let Some(f) = ingest.pipe.failure() {
            return Health::failing(format!("reading the programme failed: {f}"));
        }
        if !server.mounted() {
            return Health::degraded("waiting for the programme from the core before the stream can be served");
        }
        let mut h = Health::ok();
        h.detail = Some(format!("{} at {}, {} frames served", plural(server.clients()), self.settings.describe(), feed.frames()));
        h
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        match method {
            "stats" => Ok(json!({
                "url": self.settings.describe(),
                "clients": self.running.as_ref().map(|(s, _, _)| s.clients()),
                "bytes_read": self.running.as_ref().map(|(_, i, _)| i.bytes()),
            })),
            other => Err(RpcError::new(
                codes::METHOD_NOT_FOUND,
                format!("rtsp/output has no method '{other}'. It answers 'stats', and the standard output methods in docs/reference/plugin-protocol.md."),
            )),
        }
    }
}

fn plural(n: u64) -> String {
    if n == 1 { "1 client".into() } else { format!("{n} clients") }
}
