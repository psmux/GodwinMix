//! The send pipeline.
//!
//! ```text
//!   FIFO ─pump─► appsrc ─► matroskademux ─┬─► h264parse ─┐
//!                                         └─► aacparse  ─┴─► mpegtsmux ─► [rtpmp2tpay] ─► udpsink
//! ```
//!
//! The core hands an output the programme already encoded, in streamable
//! Matroska on a FIFO. This remuxes it to MPEG-TS and sends it. Nothing is
//! decoded and nothing is encoded: the programme's encode is the only one.
//! `mpegtsmux` does the two things a broadcast receiver cares about: it lines
//! the stream up in groups of seven packets (1316 bytes, one datagram each),
//! and with a bitrate set it pads with null packets to a constant rate.

mod branch;
mod elements;
pub mod settings;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use gmx_netkit::pipe::Pipe;
use godwinmix_capture_common::fifo::{Fifo, Pump};
use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::Health;
use gstreamer as gst;
use gstreamer::prelude::*;

use crate::recv::make;
use elements::{judge, muxer, tail};
use settings::Settings;

pub const NEEDED: &[&str] = &["appsrc", "matroskademux", "mpegtsmux", "udpsink"];
const POLL_MS: u64 = 1000;

pub struct Sender {
    pipe: Pipe,
    pump: Option<Pump>,
    sink: gst::Element,
    last: Arc<Mutex<Health>>,
    stop: Arc<AtomicBool>,
    poller: Option<std::thread::JoinHandle<()>>,
}

impl Sender {
    pub fn start(s: &Settings, fifo: Fifo, reporter: Option<Reporter>) -> Result<Sender, String> {
        gmx_netkit::init()?;
        gmx_netkit::elements::require(NEEDED)?;
        let pipeline = gst::Pipeline::with_name("gmx-udp-output");
        let src = make("appsrc", "in")?;
        src.set_property("caps", gst::Caps::new_empty_simple("video/x-matroska"));
        src.set_property_from_str("format", "bytes");
        // Block the pump when full, so a slow network slows the reading of
        // the FIFO rather than growing memory here.
        src.set_property("block", true);
        src.set_property("max-bytes", 4u64 * 1024 * 1024);
        let demux = make("matroskademux", "demux")?;
        let mux = muxer(s)?;
        let tail = tail(s)?;
        pipeline.add_many([&src, &demux, &mux]).map_err(|e| format!("could not assemble: {e}"))?;
        pipeline.add_many(&tail).map_err(|e| format!("could not assemble: {e}"))?;
        src.link(&demux).map_err(|e| format!("could not link the FIFO to the demuxer: {e}"))?;
        let mut chain = vec![mux.clone()];
        chain.extend(tail.iter().cloned());
        gst::Element::link_many(&chain).map_err(|e| format!("could not link the sender: {e}"))?;
        branch::on_streams(&demux, &pipeline, &mux, reporter.clone());

        let mut pipe = Pipe::wrap(pipeline);
        pipe.play(reporter.clone())?;
        let sink = tail.last().cloned().ok_or("no sink")?;
        // Before the pump starts, so not one packet leaves by the wrong way.
        if !s.interface.is_empty() && s.endpoint()?.multicast() {
            crate::iface::send_multicast_out(&sink, &s.interface)?;
        }
        let mut sender = Sender {
            pipe,
            pump: Some(Pump::start(fifo, src)),
            sink,
            last: Arc::new(Mutex::new(Health::degraded("starting"))),
            stop: Arc::new(AtomicBool::new(false)),
            poller: None,
        };
        sender.poller = sender.spawn_poll(s.describe(), reporter);
        Ok(sender)
    }

    pub fn health(&self) -> Health {
        self.last.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn bytes_sent(&self) -> u64 {
        self.sink.property::<u64>("bytes-served")
    }

    fn spawn_poll(&self, what: String, reporter: Option<Reporter>) -> Option<std::thread::JoinHandle<()>> {
        let (watch, sink, last, stop) = (self.pipe.watch(), self.sink.clone(), self.last.clone(), self.stop.clone());
        std::thread::Builder::new()
            .name("gmx-udp-send-health".into())
            .spawn(move || {
                let (mut before, mut state) = (0u64, String::new());
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
                    let sent = sink.property::<u64>("bytes-served");
                    let health = judge(&what, watch.failure(), watch.ended(), sent - before);
                    before = sent;
                    *last.lock().unwrap_or_else(|e| e.into_inner()) = health.clone();
                    if let Some(r) = &reporter {
                        let now = format!("{:?}", health.state);
                        if now != state {
                            state = now;
                            r.health_changed(health);
                        } else {
                            r.set_health(health);
                        }
                    }
                }
            })
            .ok()
    }
}

impl Drop for Sender {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.poller.take() {
            let _ = t.join();
        }
        if let Some(mut pump) = self.pump.take() {
            pump.stop();
        }
        self.pipe.stop();
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "interleave_tests.rs"]
mod interleave_tests;
