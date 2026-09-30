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

fn judge(what: &str, failure: Option<String>, ended: bool, bytes: u64) -> Health {
    if let Some(f) = failure {
        return Health::failing(format!("sending to {what} failed: {f}"));
    }
    if ended {
        return Health::degraded("the core stopped handing over the programme. It starts again when the output is reconnected.");
    }
    if bytes == 0 {
        return Health::degraded(format!(
            "nothing sent to {what} in the last second: the programme has not arrived from the core yet."
        ));
    }
    let mut h = Health::ok();
    h.detail = Some(format!("{:.1} Mbit/s to {what}", bytes as f64 * 8.0 / 1e6));
    h
}

fn muxer(s: &Settings) -> Result<gst::Element, String> {
    let mux = make("mpegtsmux", "mux")?;
    mux.set_property("alignment", s.packets_per_datagram as i32);
    if s.cbr_kbps > 0 {
        mux.set_property("bitrate", u64::from(s.cbr_kbps) * 1000);
    }
    Ok(mux)
}

/// The payloader when the address is rtp://, and the socket.
fn tail(s: &Settings) -> Result<Vec<gst::Element>, String> {
    let e = s.endpoint()?;
    let mut out = Vec::new();
    if s.rtp() {
        let pay = make("rtpmp2tpay", "pay")?;
        pay.set_property("mtu", 12 + 188 * s.packets_per_datagram);
        pay.set_property("pt", 33u32);
        out.push(pay);
    }
    let sink = make("udpsink", "out")?;
    sink.set_property("host", &e.host);
    sink.set_property("port", i32::from(e.port));
    sink.set_property("auto-multicast", false);
    sink.set_property("ttl", s.ttl as i32);
    sink.set_property("ttl-mc", s.ttl as i32);
    sink.set_property("sync", false);
    sink.set_property("async", false);
    if !s.interface.is_empty() {
        sink.set_property("multicast-iface", &s.interface);
    }
    if s.dscp >= 0 {
        sink.set_property("qos-dscp", s.dscp);
    }
    if s.cbr_kbps > 0 {
        // Pace to a hair over the constant rate, so the stream leaves as an
        // even trickle rather than a burst per frame. The margin covers the
        // RTP header and keeps the pacing from ever falling behind.
        sink.set_property("max-bitrate", u64::from(s.cbr_kbps) * 1030);
    }
    out.push(sink);
    Ok(out)
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
