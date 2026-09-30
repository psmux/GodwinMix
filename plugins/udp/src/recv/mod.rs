//! The receive pipeline, and the thread that says how it is doing.
//!
//! ```text
//!   udpsrc ──(probe: TS or RTP, count loss, keep one program)──► queue ──► fdsink fd=1
//! ```
//!
//! What leaves on stdout is MPEG-TS, and the core demuxes and decodes it once
//! on its own hardware aware path, the same as `srt/source`. The probe is the
//! only thing this plugin adds, and it touches four header bytes per packet.
//! The socket is opened when the pipeline starts and closed when it stops, so
//! a source that is not running holds no port.

pub mod health;
pub mod probe;
pub mod settings;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use gmx_netkit::pipe::Pipe;
use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::Health;
use gstreamer as gst;
use gstreamer::prelude::*;

use crate::counters::{Counters, SharedCatalog};
use crate::ts::filter::Filter;
use health::{assess, Inputs, Reading};
use probe::{Probe, Verdict};
use settings::Settings;

const POLL_MS: u64 = 1000;

/// Where the TS goes: stdout in a running plugin, a file in the tests.
#[derive(Debug, Clone)]
pub enum Sink {
    Stdout,
    #[allow(dead_code)]
    File(std::path::PathBuf),
}

pub struct Receiver {
    pipe: Pipe,
    address: String,
    counters: Arc<Counters>,
    catalog: SharedCatalog,
    last: Arc<Mutex<Health>>,
    stop: Arc<AtomicBool>,
    poller: Option<std::thread::JoinHandle<()>>,
}

impl Receiver {
    pub fn start(settings: &Settings, reporter: Option<Reporter>, sink: Sink) -> Result<Receiver, String> {
        gmx_netkit::init()?;
        gmx_netkit::elements::require(&["udpsrc", "queue", "fdsink"])?;
        let endpoint = settings.endpoint()?;
        let address = with_interface(&endpoint.display(settings.scheme()), &settings.interface);
        let pipeline = gst::Pipeline::with_name("gmx-udp-source");
        let src = udpsrc(settings)?;
        let queue = make("queue", "q")?;
        // Leaky: a core that is busy for a moment loses the oldest datagrams
        // here rather than making the socket back up. One second is plenty.
        queue.set_property("max-size-time", 1_000_000_000u64);
        queue.set_property("max-size-bytes", 0u32);
        queue.set_property("max-size-buffers", 0u32);
        queue.set_property_from_str("leaky", "downstream");
        let out = sink_for(&sink)?;
        pipeline.add_many([&src, &queue, &out]).map_err(|e| format!("could not assemble: {e}"))?;
        gst::Element::link_many([&src, &queue, &out]).map_err(|e| format!("could not link: {e}"))?;

        let counters = Arc::new(Counters::default());
        let catalog = SharedCatalog::default();
        attach_probe(&src, Probe::new(Filter::new(settings.choice()), counters.clone(), catalog.clone()))?;

        let mut pipe = Pipe::wrap(pipeline);
        if let Err(e) = pipe.play(reporter.clone()) {
            // The bus has the reason; the state change only says it failed.
            std::thread::sleep(std::time::Duration::from_millis(200));
            return Err(health::explain(&pipe.failure().unwrap_or(e), &address));
        }
        let mut r = Receiver {
            pipe,
            address,
            counters,
            catalog,
            last: Arc::new(Mutex::new(Health::degraded("starting"))),
            stop: Arc::new(AtomicBool::new(false)),
            poller: None,
        };
        r.poller = r.spawn_poll(reporter);
        Ok(r)
    }

    /// The health from the last reading, at most a second old.
    pub fn health(&self) -> Health {
        self.last.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn stats(&self) -> serde_json::Value {
        self.counters.json()
    }

    pub fn programs(&self) -> serde_json::Value {
        self.catalog.lock().unwrap_or_else(|e| e.into_inner()).json()
    }

    pub fn address(&self) -> &str {
        &self.address
    }

    fn spawn_poll(&self, reporter: Option<Reporter>) -> Option<std::thread::JoinHandle<()>> {
        let (watch, counters, catalog) = (self.pipe.watch(), self.counters.clone(), self.catalog.clone());
        let (last, stop, address) = (self.last.clone(), self.stop.clone(), self.address.clone());
        std::thread::Builder::new()
            .name("gmx-udp-health".into())
            .spawn(move || {
                let mut reading = Reading::default();
                let mut state = String::new();
                while !stop.load(Ordering::Relaxed) {
                    let cat = catalog.lock().unwrap_or_else(|e| e.into_inner()).clone();
                    let inputs = Inputs { address: &address, failure: watch.failure(), counters: &counters, catalog: &cat };
                    let (health, next) = assess(&inputs, reading);
                    reading = next;
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
                    std::thread::sleep(std::time::Duration::from_millis(POLL_MS));
                }
            })
            .ok()
    }
}

impl Drop for Receiver {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.poller.take() {
            let _ = t.join();
        }
        self.pipe.stop();
    }
}

fn udpsrc(s: &Settings) -> Result<gst::Element, String> {
    let e = s.endpoint()?;
    let src = make("udpsrc", "src")?;
    src.set_property("address", &e.host);
    src.set_property("port", i32::from(e.port));
    // Two receivers of one group on one machine is normal; two sources fighting
    // over one unicast port is a mistake, and refusing the second says so.
    src.set_property("reuse", e.multicast());
    src.set_property("auto-multicast", true);
    src.set_property("buffer-size", (s.receive_buffer_kb * 1024) as i32);
    src.set_property("retrieve-sender-address", false);
    if !s.interface.is_empty() {
        src.set_property("multicast-iface", &s.interface);
    }
    if let Some(source) = &e.source {
        src.set_property("multicast-source", format!("+{source}"));
    }
    let caps = gst::Caps::builder("video/mpegts").field("systemstream", true).field("packetsize", 188).build();
    src.set_property("caps", &caps);
    Ok(src)
}

fn attach_probe(src: &gst::Element, probe: Probe) -> Result<(), String> {
    let pad = src.static_pad("src").ok_or("udpsrc has no src pad")?;
    let probe = Mutex::new(probe);
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, info| {
        let Some(gst::PadProbeData::Buffer(buffer)) = info.data.as_mut() else {
            return gst::PadProbeReturn::Ok;
        };
        let verdict = match buffer.map_readable() {
            Ok(map) => probe.lock().unwrap_or_else(|e| e.into_inner()).datagram(map.as_slice()),
            Err(_) => return gst::PadProbeReturn::Ok,
        };
        match verdict {
            Verdict::Keep => gst::PadProbeReturn::Ok,
            Verdict::Drop => gst::PadProbeReturn::Drop,
            Verdict::Replace(bytes) => {
                let mut fresh = gst::Buffer::from_mut_slice(bytes);
                if let Some(b) = fresh.get_mut() {
                    b.set_pts(buffer.pts());
                    b.set_dts(buffer.dts());
                }
                *buffer = fresh;
                gst::PadProbeReturn::Ok
            }
        }
    });
    Ok(())
}

fn sink_for(sink: &Sink) -> Result<gst::Element, String> {
    let out = match sink {
        Sink::Stdout => {
            let out = make("fdsink", "out")?;
            out.set_property("fd", 1i32);
            out
        }
        Sink::File(path) => {
            let out = make("filesink", "out")?;
            out.set_property("location", path.to_string_lossy().to_string());
            // Unbuffered, so a test reading the file sees each datagram as it lands.
            out.set_property_from_str("buffer-mode", "unbuffered");
            out
        }
    };
    out.set_property("sync", false);
    out.set_property("async", false);
    Ok(out)
}

fn with_interface(address: &str, iface: &str) -> String {
    if iface.is_empty() { address.to_string() } else { format!("{address} on {iface}") }
}

pub fn make(factory: &str, name: &str) -> Result<gst::Element, String> {
    let b = gst::ElementFactory::make(factory);
    let b = if name.is_empty() { b } else { b.name(name) };
    b.build().map_err(|e| format!("could not make '{factory}': {e}"))
}

#[cfg(test)]
#[path = "tests.rs"]
pub mod tests;
