//! The desk: the master, the buses that are built, and the send points that
//! feed them.

use super::bus::BusGraph;
use super::check::{self, Refusal};
use super::device::DevicePlayer;
use super::encode::{BusEncoder, EncodeWith};
use super::send::SendPoint;
use super::weight::{CueMarks, Mode, Shared, Weights};
use super::Notify;
use crate::caps::CanvasCaps;
use crate::plugin::branch::ProgrammeBranch;
use godwinmix_protocol::buses::{AudioConfig, BusConfig};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;

mod sources;

/// One bus that is built.
pub(super) struct Live {
    pub graph: BusGraph,
    pub shared: Shared,
    pub device: Option<DevicePlayer>,
    /// Why the device is not playing, or that it is being opened.
    pub device_state: Option<String>,
    pub ticket: u64,
    pub encoder: Option<BusEncoder>,
}

pub struct Desk {
    pub(super) pipeline: gst::Pipeline,
    pub(super) canvas: CanvasCaps,
    pub(super) latency_ns: i64,
    pub(super) encode: EncodeWith,
    /// The programme's master, `pgm-master`.
    pub(super) master: gst::Element,
    /// What is configured, and what the runtime store saves.
    pub config: AudioConfig,
    pub(super) live: Vec<Live>,
    pub(super) points: Vec<SendPoint>,
    pub(super) cue: CueMarks,
    pub(super) notify: Option<Notify>,
    pub(super) tickets: u64,
}

impl Desk {
    pub fn new(pipeline: &gst::Pipeline, canvas: &CanvasCaps, latency_ns: i64, encode: EncodeWith, master: gst::Element, config: AudioConfig) -> Self {
        let desk = Self {
            pipeline: pipeline.clone(),
            canvas: canvas.clone(),
            latency_ns,
            encode,
            master,
            config,
            live: Vec::new(),
            points: Vec::new(),
            cue: Arc::new(Mutex::new(Vec::new())),
            notify: None,
            tickets: 0,
        };
        desk.apply_master();
        desk
    }

    /// How a device search gets back onto the mixer queue.
    pub fn set_notify(&mut self, notify: Notify) {
        self.notify = Some(notify);
    }

    pub(super) fn apply_master(&self) {
        let m = self.config.master;
        self.master.set_property("volume", m.gain);
        self.master.set_property("mute", m.muted);
    }

    /// Build every configured bus. At start, before any source: each joins as
    /// it is added.
    pub fn start(&mut self) {
        for cfg in self.config.buses.clone() {
            if let Err(e) = self.build(&cfg, &[]) {
                tracing::error!(bus = %cfg.id, %e, "could not build a configured bus");
            }
        }
    }

    /// Build one bus and put every source on it.
    pub(super) fn build(&mut self, cfg: &BusConfig, branches: &[&ProgrammeBranch]) -> Result<(), Refusal> {
        let graph = BusGraph::build(&self.pipeline, &self.canvas, &cfg.id, self.latency_ns)
            .map_err(|e| Refusal::Failed(format!("bus {} could not be built: {e:#}. Nothing was changed.", cfg.id)))?;
        graph.set_master(cfg.gain, cfg.muted);
        let shared = Arc::new(Mutex::new(Weights { mode: mode_of(cfg), sends: cfg.sends.clone() }));
        if self.live.is_empty() {
            for b in branches {
                self.point_for(b);
            }
        }
        for p in &mut self.points {
            if let Err(e) = p.join(&cfg.id, &graph.mix, shared.clone(), self.cue.clone()) {
                tracing::warn!(bus = %cfg.id, source = %p.source, ?e, "a source could not join a bus");
            }
        }
        self.live.push(Live { graph, shared, device: None, device_state: None, ticket: 0, encoder: None });
        if let Some(dev) = cfg.device.clone().filter(|d| !d.is_empty()) {
            self.open_device(&cfg.id, &dev);
        }
        tracing::info!(bus = %cfg.id, kind = cfg.kind(), "bus built");
        Ok(())
    }

    pub(super) fn live(&self, id: &str) -> Result<&Live, Refusal> {
        self.live.iter().find(|l| l.graph.id == id).ok_or_else(|| self.no_bus(id))
    }

    pub(super) fn no_bus(&self, id: &str) -> Refusal {
        Refusal::NotFound { kind: "bus", id: id.to_string(), known: self.config.buses.iter().map(|b| b.id.clone()).collect() }
    }

    /// Stop everything, for a mixer shutting down.
    pub fn shutdown(&mut self) {
        let ids: Vec<String> = self.live.iter().map(|l| l.graph.id.clone()).collect();
        for id in ids {
            self.tear_down(&id);
        }
    }
}

pub(super) fn mode_of(cfg: &BusConfig) -> Mode {
    match (&cfg.minus, cfg.cue) {
        (_, true) => Mode::Cue,
        (Some(m), _) => Mode::Minus(m.clone()),
        _ => Mode::Sends,
    }
}

pub(super) fn checked(cfg: &BusConfig) -> Result<(), Refusal> {
    check::bus(cfg)
}
